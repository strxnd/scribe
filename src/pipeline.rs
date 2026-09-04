use crate::config::Config;
use crate::history::{self, HistoryItem};
use crate::inject::{InjectOutcome, Injector};
use crate::llm::{self, PiStatus};
use crate::paths::AppPaths;
use crate::stt::{self, LoadedEngine, SpeechEngineKind};

pub struct Session {
    pub paths: AppPaths,
    pub config: Config,
    pub pi: PiStatus,
    engine: Option<LoadedEngine>,
    injector: Injector,
}

#[derive(Debug, Clone)]
pub struct PipelineResult {
    pub raw: String,
    pub final_text: String,
    pub polished: bool,
    pub inject: InjectOutcome,
    pub engine: SpeechEngineKind,
}

impl Session {
    pub fn new(paths: AppPaths, config: Config) -> Self {
        let pi = llm::detect();
        Self {
            paths,
            config,
            pi,
            engine: None,
            injector: Injector::new(),
        }
    }

    pub fn refresh_pi(&mut self) {
        self.pi = llm::detect();
    }

    pub fn uinput_ready(&self) -> bool {
        self.injector.uinput_ready()
    }

    pub fn model_ready(&self) -> bool {
        stt::is_installed(
            &self.paths,
            self.config.speech.engine,
            model_name(&self.config),
        )
    }

    pub fn transcribe_and_inject(&mut self, samples: &[f32]) -> anyhow::Result<PipelineResult> {
        let engine_kind = self.config.speech.engine;
        let name = model_name(&self.config).to_string();
        if self.engine.is_none() {
            self.engine = Some(stt::load(&self.paths, engine_kind, &name)?);
        }
        let raw = self
            .engine
            .as_mut()
            .unwrap()
            .transcribe(samples, self.config.speech.language.as_deref())?;
        if raw.is_empty() {
            anyhow::bail!("nothing was said");
        }

        let mut polished = false;
        let final_text = if self.config.llm.polish {
            match llm::polish(&self.pi, &self.config.llm.model, &raw) {
                Ok(text) if !text.is_empty() => {
                    polished = true;
                    text
                }
                Ok(_) => raw.clone(),
                Err(err) => {
                    tracing::warn!("Pi polish skipped: {err:#}");
                    raw.clone()
                }
            }
        } else {
            raw.clone()
        };

        let inject = self.injector.inject(&final_text, &self.config.inject)?;
        let item = HistoryItem {
            at: chrono::Utc::now(),
            engine: engine_kind.label().into(),
            raw: raw.clone(),
            final_text: final_text.clone(),
            polished,
        };
        if let Err(err) = history::append(&self.paths.history_file(), &item) {
            tracing::warn!("history: {err:#}");
        }

        Ok(PipelineResult {
            raw,
            final_text,
            polished,
            inject,
            engine: engine_kind,
        })
    }

    pub fn drop_engine(&mut self) {
        self.engine = None;
    }
}

fn model_name(config: &Config) -> &str {
    match config.speech.engine {
        SpeechEngineKind::Whisper => &config.speech.whisper_model,
        SpeechEngineKind::Parakeet => &config.speech.parakeet_model,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_name_tracks_engine() {
        let mut cfg = Config::default();
        cfg.speech.engine = SpeechEngineKind::Whisper;
        cfg.speech.whisper_model = "ggml-tiny.en".into();
        assert_eq!(model_name(&cfg), "ggml-tiny.en");
        cfg.speech.engine = SpeechEngineKind::Parakeet;
        assert_eq!(model_name(&cfg), cfg.speech.parakeet_model);
    }
}
