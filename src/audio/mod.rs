use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use parking_lot::Mutex;

pub const SAMPLE_RATE: u32 = 16_000;

pub struct CaptureHandle {
    stop: Arc<AtomicBool>,
    samples: Arc<Mutex<Vec<f32>>>,
    level: Arc<Mutex<f32>>,
    started: Instant,
}

impl CaptureHandle {
    pub fn stop(self) -> RecordedAudio {
        self.stop.store(true, Ordering::Relaxed);
        // Give the audio callback a beat to notice.
        std::thread::sleep(std::time::Duration::from_millis(30));
        let samples = std::mem::take(&mut *self.samples.lock());
        RecordedAudio {
            samples,
            duration: self.started.elapsed(),
        }
    }

    pub fn level(&self) -> f32 {
        *self.level.lock()
    }
}

#[derive(Debug, Clone)]
pub struct RecordedAudio {
    pub samples: Vec<f32>,
    pub duration: std::time::Duration,
}

impl RecordedAudio {
    pub fn is_too_short(&self) -> bool {
        self.duration.as_millis() < 250 || self.samples.len() < SAMPLE_RATE as usize / 8
    }
}

pub fn start(device_name: Option<&str>) -> anyhow::Result<CaptureHandle> {
    if let Ok(path) = std::env::var("UTTER_DEMO_WAV") {
        tracing::info!("demo capture from {path}");
        return start_from_wav(path);
    }
    let host = cpal::default_host();
    let device = if let Some(name) = device_name {
        host.input_devices()?
            .find(|d| d.name().ok().as_deref() == Some(name))
            .ok_or_else(|| anyhow::anyhow!("input device `{name}` not found"))?
    } else {
        host.default_input_device()
            .ok_or_else(|| anyhow::anyhow!("no default microphone"))?
    };
    let config = device.default_input_config()?;
    tracing::info!(
        "mic `{}` {:?} {}Hz {}ch",
        device.name().unwrap_or_else(|_| "unknown".into()),
        config.sample_format(),
        config.sample_rate().0,
        config.channels()
    );

    let stop = Arc::new(AtomicBool::new(false));
    let samples = Arc::new(Mutex::new(Vec::new()));
    let level = Arc::new(Mutex::new(0.0f32));
    let handle = CaptureHandle {
        stop: stop.clone(),
        samples: samples.clone(),
        level: level.clone(),
        started: Instant::now(),
    };

    let channels = config.channels() as usize;
    let in_rate = config.sample_rate().0;
    let format = config.sample_format();
    let stream_config: cpal::StreamConfig = config.into();

    std::thread::Builder::new()
        .name("scribe-audio".into())
        .spawn(move || {
            let err_fn = |err| tracing::error!("audio stream: {err}");
            let samples_f = samples.clone();
            let level_f = level.clone();
            let stop_f = stop.clone();
            let stream = match format {
                cpal::SampleFormat::F32 => device.build_input_stream(
                    &stream_config,
                    {
                        let samples = samples_f.clone();
                        let level = level_f.clone();
                        let stop = stop_f.clone();
                        move |data: &[f32], _| {
                            ingest(data, channels, in_rate, &samples, &level, &stop)
                        }
                    },
                    err_fn,
                    None,
                ),
                cpal::SampleFormat::I16 => device.build_input_stream(
                    &stream_config,
                    {
                        let samples = samples_f.clone();
                        let level = level_f.clone();
                        let stop = stop_f.clone();
                        move |data: &[i16], _| {
                            let converted: Vec<f32> =
                                data.iter().map(|s| *s as f32 / i16::MAX as f32).collect();
                            ingest(&converted, channels, in_rate, &samples, &level, &stop)
                        }
                    },
                    err_fn,
                    None,
                ),
                cpal::SampleFormat::U16 => device.build_input_stream(
                    &stream_config,
                    {
                        let samples = samples_f.clone();
                        let level = level_f.clone();
                        let stop = stop_f.clone();
                        move |data: &[u16], _| {
                            let converted: Vec<f32> = data
                                .iter()
                                .map(|s| (*s as f32 / u16::MAX as f32) * 2.0 - 1.0)
                                .collect();
                            ingest(&converted, channels, in_rate, &samples, &level, &stop)
                        }
                    },
                    err_fn,
                    None,
                ),
                other => {
                    tracing::error!("unsupported sample format {other:?}");
                    return;
                }
            };
            let Ok(stream) = stream else {
                tracing::error!("failed to open microphone stream");
                return;
            };
            if let Err(err) = stream.play() {
                tracing::error!("play microphone: {err}");
                return;
            }
            while !stop.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            drop(stream);
        })?;

    Ok(handle)
}

fn start_from_wav(path: impl AsRef<std::path::Path>) -> anyhow::Result<CaptureHandle> {
    let all = load_wav_mono_16k(path.as_ref())?;
    anyhow::ensure!(!all.is_empty(), "demo wav is empty");

    let stop = Arc::new(AtomicBool::new(false));
    let samples = Arc::new(Mutex::new(Vec::new()));
    let level = Arc::new(Mutex::new(0.0f32));
    let handle = CaptureHandle {
        stop: stop.clone(),
        samples: samples.clone(),
        level: level.clone(),
        started: Instant::now(),
    };

    std::thread::Builder::new()
        .name("scribe-demo-wav".into())
        .spawn(move || {
            let chunk = SAMPLE_RATE as usize / 20;
            let mut i = 0;
            while !stop.load(Ordering::Relaxed) && i < all.len() {
                let end = (i + chunk).min(all.len());
                let slice = &all[i..end];
                if let Some(peak) = rms(slice) {
                    *level.lock() = peak;
                }
                samples.lock().extend_from_slice(slice);
                i = end;
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            *level.lock() = 0.0;
            while !stop.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        })?;
    Ok(handle)
}

pub fn load_wav_mono_16k(path: &std::path::Path) -> anyhow::Result<Vec<f32>> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;
    let rate = spec.sample_rate;
    let pcm: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| anyhow::anyhow!(err))?,
        hound::SampleFormat::Int => {
            match spec.bits_per_sample {
                16 => reader
                    .samples::<i16>()
                    .map(|s| s.map(|v| v as f32 / 32768.0))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|err| anyhow::anyhow!(err))?,
                32 => reader
                    .samples::<i32>()
                    .map(|s| s.map(|v| v as f32 / 2_147_483_648.0))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|err| anyhow::anyhow!(err))?,
                bits => anyhow::bail!("unsupported PCM bit depth {bits}"),
            }
        }
    };
    let mono = downmix_mono(&pcm, channels);
    Ok(resample_linear(&mono, rate, SAMPLE_RATE))
}

fn ingest(
    data: &[f32],
    channels: usize,
    in_rate: u32,
    samples: &Arc<Mutex<Vec<f32>>>,
    level: &Arc<Mutex<f32>>,
    stop: &Arc<AtomicBool>,
) {
    if stop.load(Ordering::Relaxed) {
        return;
    }
    let mono = downmix_mono(data, channels);
    let resampled = resample_linear(&mono, in_rate, SAMPLE_RATE);
    if let Some(peak) = rms(&resampled) {
        *level.lock() = peak;
    }
    samples.lock().extend_from_slice(&resampled);
}

pub fn downmix_mono(input: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return input.to_vec();
    }
    input
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

pub fn resample_linear(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    let ratio = to as f64 / from as f64;
    let out_len = ((input.len() as f64) * ratio).round().max(1.0) as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src = i as f64 / ratio;
        let idx = src.floor() as usize;
        let frac = (src - idx as f64) as f32;
        let a = input[idx.min(input.len() - 1)];
        let b = input[(idx + 1).min(input.len() - 1)];
        out.push(a + (b - a) * frac);
    }
    out
}

fn rms(samples: &[f32]) -> Option<f32> {
    if samples.is_empty() {
        return None;
    }
    let sum: f32 = samples.iter().map(|s| s * s).sum();
    Some((sum / samples.len() as f32).sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downmix_averages_channels() {
        let stereo = [1.0, 3.0, 2.0, 4.0];
        let mono = downmix_mono(&stereo, 2);
        assert_eq!(mono, vec![2.0, 3.0]);
    }

    #[test]
    fn resample_doubles_length() {
        let input = vec![0.0, 1.0, 0.0];
        let out = resample_linear(&input, 8_000, 16_000);
        assert_eq!(out.len(), 6);
        assert!((out[0] - 0.0).abs() < 1e-6);
    }

    #[test]
    fn identity_resample() {
        let input = vec![0.1, 0.2, 0.3];
        assert_eq!(resample_linear(&input, 16_000, 16_000), input);
    }

    #[test]
    fn loads_pcm16_wav() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tone.wav");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&path, spec).unwrap();
        for i in 0..1600 {
            let s = ((i as f32 / 16.0).sin() * 16_000.0) as i16;
            writer.write_sample(s).unwrap();
        }
        writer.finalize().unwrap();
        let samples = load_wav_mono_16k(&path).unwrap();
        assert_eq!(samples.len(), 1600);
        assert!(samples.iter().any(|s| s.abs() > 0.1));
    }
}
