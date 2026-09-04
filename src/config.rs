use std::path::Path;

use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::hotkey::Hotkey;
use crate::stt::SpeechEngineKind;

/// User preferences stored at `~/.config/scribe/config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub speech: SpeechConfig,
    pub llm: LlmConfig,
    pub hotkeys: HotkeyConfig,
    pub inject: InjectConfig,
    pub audio: AudioConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SpeechConfig {
    /// Whisper or Parakeet. Default prefers Parakeet when the files exist.
    pub engine: SpeechEngineKind,
    /// Whisper ggml file stem, e.g. `ggml-base.en`.
    pub whisper_model: String,
    /// Parakeet model directory name under the models folder.
    pub parakeet_model: String,
    pub language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LlmConfig {
    /// Run the transcript through Pi after speech-to-text.
    pub polish: bool,
    /// `auto` lets Pi pick the first available model. Otherwise `provider/id`.
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HotkeyConfig {
    pub toggle: Hotkey,
    pub push_to_talk: Hotkey,
    pub cancel: Hotkey,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct InjectConfig {
    pub method: InjectMethod,
    pub paste_combo: PasteCombo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InjectMethod {
    Auto,
    Uinput,
    Clipboard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PasteCombo {
    Auto,
    CtrlV,
    CtrlShiftV,
    ShiftInsert,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioConfig {
    pub input_device: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            speech: SpeechConfig::default(),
            llm: LlmConfig::default(),
            hotkeys: HotkeyConfig::default(),
            inject: InjectConfig::default(),
            audio: AudioConfig::default(),
        }
    }
}

impl Default for SpeechConfig {
    fn default() -> Self {
        Self {
            engine: SpeechEngineKind::Parakeet,
            whisper_model: "ggml-base.en".into(),
            parakeet_model: "parakeet-tdt-0.6b-v3-int8".into(),
            language: None,
        }
    }
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            polish: true,
            model: "auto".into(),
        }
    }
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            toggle: Hotkey::parse("Super+Shift+Space").unwrap(),
            push_to_talk: Hotkey::parse("RightCtrl").unwrap(),
            cancel: Hotkey::parse("Super+Shift+Escape").unwrap(),
        }
    }
}

impl Default for InjectConfig {
    fn default() -> Self {
        Self {
            method: InjectMethod::Auto,
            paste_combo: PasteCombo::Auto,
        }
    }
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self { input_device: None }
    }
}

impl Config {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("read {}", path.display()))?;
        let cfg: Self = toml::from_str(&raw).context("parse config.toml")?;
        Ok(cfg)
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let raw = toml::to_string_pretty(self)?;
        std::fs::write(path, raw)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn roundtrip_toml() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut cfg = Config::default();
        cfg.speech.engine = SpeechEngineKind::Whisper;
        cfg.llm.model = "openai/gpt-4o-mini".into();
        cfg.save(&path).unwrap();
        let loaded = Config::load(&path).unwrap();
        assert_eq!(loaded.speech.engine, SpeechEngineKind::Whisper);
        assert_eq!(loaded.llm.model, "openai/gpt-4o-mini");
        assert_eq!(loaded.hotkeys.push_to_talk.to_string(), "RightCtrl");
    }

    #[test]
    fn missing_file_is_default() {
        let cfg = Config::load(Path::new("/no/such/scribe.toml")).unwrap();
        assert!(cfg.llm.polish);
        assert_eq!(cfg.llm.model, "auto");
    }
}
