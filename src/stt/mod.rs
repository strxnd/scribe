use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use transcribe_rs::SpeechModel;
use transcribe_rs::TranscribeOptions;

use crate::AppPaths;

mod download;

pub use download::{download_model, CatalogEntry, Progress, CATALOG};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SpeechEngineKind {
    Parakeet,
    Whisper,
}

impl SpeechEngineKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Parakeet => "NVIDIA Parakeet",
            Self::Whisper => "OpenAI Whisper",
        }
    }
}

pub enum LoadedEngine {
    Parakeet(transcribe_rs::onnx::parakeet::ParakeetModel),
    Whisper(transcribe_rs::whisper_cpp::WhisperEngine),
}

impl LoadedEngine {
    pub fn transcribe(&mut self, samples: &[f32], language: Option<&str>) -> anyhow::Result<String> {
        let options = TranscribeOptions {
            language: language.map(|s| s.to_string()),
            translate: false,
            leading_silence_ms: None,
            trailing_silence_ms: None,
        };
        let result = match self {
            Self::Parakeet(model) => model.transcribe(samples, &options),
            Self::Whisper(model) => model.transcribe(samples, &options),
        }
        .map_err(|err| anyhow::anyhow!(err))?;
        Ok(cleanup_transcript(&result.text))
    }
}

pub fn engine_path(paths: &AppPaths, kind: SpeechEngineKind, name: &str) -> PathBuf {
    match kind {
        SpeechEngineKind::Whisper => {
            let with_bin = paths.models_dir.join(format!("{name}.bin"));
            if with_bin.is_file() {
                with_bin
            } else {
                paths.models_dir.join(name)
            }
        }
        SpeechEngineKind::Parakeet => paths.models_dir.join(name),
    }
}

pub fn is_installed(paths: &AppPaths, kind: SpeechEngineKind, name: &str) -> bool {
    let path = engine_path(paths, kind, name);
    match kind {
        SpeechEngineKind::Whisper => path.is_file(),
        SpeechEngineKind::Parakeet => {
            path.join("encoder-model.int8.onnx").is_file()
                && path.join("decoder_joint-model.int8.onnx").is_file()
                && path.join("nemo128.onnx").is_file()
                && path.join("vocab.txt").is_file()
        }
    }
}

pub fn load(paths: &AppPaths, kind: SpeechEngineKind, name: &str) -> anyhow::Result<LoadedEngine> {
    let path = engine_path(paths, kind, name);
    anyhow::ensure!(
        is_installed(paths, kind, name),
        "{} model `{name}` is not downloaded yet",
        kind.label()
    );
    match kind {
        SpeechEngineKind::Parakeet => {
            let model = transcribe_rs::onnx::parakeet::ParakeetModel::load(
                &path,
                &transcribe_rs::onnx::Quantization::Int8,
            )
            .map_err(|err| anyhow::anyhow!(err))?;
            Ok(LoadedEngine::Parakeet(model))
        }
        SpeechEngineKind::Whisper => {
            let model = transcribe_rs::whisper_cpp::WhisperEngine::load(&path)
                .map_err(|err| anyhow::anyhow!(err))?;
            Ok(LoadedEngine::Whisper(model))
        }
    }
}

pub fn cleanup_transcript(text: &str) -> String {
    let mut out = text.trim().to_string();
    for artifact in ["[BLANK_AUDIO]", "[Silence]", "(silence)", "[MUSIC]"] {
        out = out.replace(artifact, "");
    }
    let collapsed = out.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.trim().to_string()
}

pub fn missing_files(paths: &AppPaths, kind: SpeechEngineKind, name: &str) -> Vec<String> {
    let path = engine_path(paths, kind, name);
    match kind {
        SpeechEngineKind::Whisper => {
            if path.is_file() {
                Vec::new()
            } else {
                vec![path.display().to_string()]
            }
        }
        SpeechEngineKind::Parakeet => ["encoder-model.int8.onnx", "decoder_joint-model.int8.onnx", "nemo128.onnx", "vocab.txt"]
            .into_iter()
            .filter(|file| !path.join(file).is_file())
            .map(|file| path.join(file).display().to_string())
            .collect(),
    }
}

pub fn whisper_candidates() -> &'static [&'static str] {
    &["ggml-tiny.en", "ggml-base.en", "ggml-small.en", "ggml-tiny", "ggml-base", "ggml-small"]
}

pub fn resolve_existing_whisper(dir: &Path, name: &str) -> Option<PathBuf> {
    let direct = dir.join(name);
    if direct.is_file() {
        return Some(direct);
    }
    let bin = dir.join(format!("{name}.bin"));
    bin.is_file().then_some(bin)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_whisper_artifacts() {
        assert_eq!(cleanup_transcript("  hello   [BLANK_AUDIO] world  "), "hello world");
        assert_eq!(cleanup_transcript("[Silence]"), "");
    }
}
