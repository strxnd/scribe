use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Context;
use serde::Deserialize;
use serde_json::json;

/// Pi is the only LLM provider in v1. Models are auto-detected from the `pi`
/// CLI when installed, otherwise from `~/.pi/agent/auth.json`, `models.json`,
/// and the same environment variables Pi itself reads.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedModel {
    pub provider: String,
    pub id: String,
    pub api: ApiKind,
    pub base_url: String,
    pub source: ModelSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiKind {
    OpenAiCompletions,
    AnthropicMessages,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelSource {
    PiCli,
    AuthFile,
    Env,
    ModelsJson,
}

impl DetectedModel {
    pub fn slug(&self) -> String {
        format!("{}/{}", self.provider, self.id)
    }
}

#[derive(Debug, Clone)]
pub struct PiStatus {
    pub cli_present: bool,
    pub models: Vec<DetectedModel>,
    pub notes: Vec<String>,
}

impl PiStatus {
    pub fn selected<'a>(&'a self, requested: &str) -> Option<&'a DetectedModel> {
        if self.models.is_empty() {
            return None;
        }
        if requested.is_empty() || requested == "auto" {
            return self.models.first();
        }
        self.models.iter().find(|m| m.slug() == requested || m.id == requested)
    }
}

pub fn detect() -> PiStatus {
    detect_from(PiPaths::default(), which::which("pi").ok())
}

struct PiPaths {
    auth: PathBuf,
    models: PathBuf,
}

impl Default for PiPaths {
    fn default() -> Self {
        let home = dirs_home();
        Self {
            auth: home.join(".pi/agent/auth.json"),
            models: home.join(".pi/agent/models.json"),
        }
    }
}

fn dirs_home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

fn detect_from(paths: PiPaths, pi_bin: Option<PathBuf>) -> PiStatus {
    let mut notes = Vec::new();
    let mut models = Vec::new();
    let cli_present = pi_bin.is_some();

    if let Some(bin) = &pi_bin {
        match list_models_cli(bin) {
            Ok(found) if !found.is_empty() => {
                models.extend(found);
            }
            Ok(_) => notes.push("pi --list-models returned no models".into()),
            Err(err) => notes.push(format!("pi --list-models failed: {err}")),
        }
    } else {
        notes.push("pi CLI not on PATH; detecting models from Pi config and environment".into());
    }

    if models.is_empty() {
        models.extend(models_from_json(&paths.models));
        models.extend(models_from_auth_and_env(&paths.auth));
    }

    models.sort_by(|a, b| a.slug().cmp(&b.slug()));
    models.dedup_by(|a, b| a.slug() == b.slug());
    rank_for_dictation(&mut models);

    if models.is_empty() {
        notes.push("No Pi models available. Run `pi /login` or set a provider API key.".into());
    }

    PiStatus {
        cli_present,
        models,
        notes,
    }
}

fn list_models_cli(bin: &Path) -> anyhow::Result<Vec<DetectedModel>> {
    let output = Command::new(bin)
        .args(["--list-models"])
        .output()
        .context("spawn pi")?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut models = Vec::new();
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('-') || line.to_ascii_lowercase().contains("provider") {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }
        let (provider, id) = if parts[0].contains('/') {
            let (p, m) = parts[0].split_once('/').unwrap();
            (p.to_string(), m.to_string())
        } else {
            (parts[0].to_string(), parts[1].to_string())
        };
        if let Some(meta) = builtin(&provider) {
            models.push(DetectedModel {
                provider,
                id,
                api: meta.api,
                base_url: meta.base_url.to_string(),
                source: ModelSource::PiCli,
            });
        } else {
            models.push(DetectedModel {
                provider: provider.clone(),
                id,
                api: ApiKind::OpenAiCompletions,
                base_url: String::new(),
                source: ModelSource::PiCli,
            });
        }
    }
    Ok(models)
}

#[derive(Clone, Copy)]
struct Builtin {
    id: &'static str,
    env: &'static str,
    base_url: &'static str,
    api: ApiKind,
    default_model: &'static str,
}

fn builtins() -> &'static [Builtin] {
    &[
        Builtin {
            id: "openai",
            env: "OPENAI_API_KEY",
            base_url: "https://api.openai.com/v1",
            api: ApiKind::OpenAiCompletions,
            default_model: "gpt-4o-mini",
        },
        Builtin {
            id: "anthropic",
            env: "ANTHROPIC_API_KEY",
            base_url: "https://api.anthropic.com",
            api: ApiKind::AnthropicMessages,
            default_model: "claude-3-5-haiku-latest",
        },
        Builtin {
            id: "google",
            env: "GEMINI_API_KEY",
            base_url: "https://generativelanguage.googleapis.com/v1beta/openai",
            api: ApiKind::OpenAiCompletions,
            default_model: "gemini-2.0-flash",
        },
        Builtin {
            id: "groq",
            env: "GROQ_API_KEY",
            base_url: "https://api.groq.com/openai/v1",
            api: ApiKind::OpenAiCompletions,
            default_model: "llama-3.1-8b-instant",
        },
        Builtin {
            id: "openrouter",
            env: "OPENROUTER_API_KEY",
            base_url: "https://openrouter.ai/api/v1",
            api: ApiKind::OpenAiCompletions,
            default_model: "openai/gpt-4o-mini",
        },
        Builtin {
            id: "deepseek",
            env: "DEEPSEEK_API_KEY",
            base_url: "https://api.deepseek.com",
            api: ApiKind::OpenAiCompletions,
            default_model: "deepseek-chat",
        },
        Builtin {
            id: "mistral",
            env: "MISTRAL_API_KEY",
            base_url: "https://api.mistral.ai/v1",
            api: ApiKind::OpenAiCompletions,
            default_model: "mistral-small-latest",
        },
        Builtin {
            id: "cerebras",
            env: "CEREBRAS_API_KEY",
            base_url: "https://api.cerebras.ai/v1",
            api: ApiKind::OpenAiCompletions,
            default_model: "llama3.1-8b",
        },
        Builtin {
            id: "xai",
            env: "XAI_API_KEY",
            base_url: "https://api.x.ai/v1",
            api: ApiKind::OpenAiCompletions,
            default_model: "grok-2-1212",
        },
        Builtin {
            id: "together",
            env: "TOGETHER_API_KEY",
            base_url: "https://api.together.xyz/v1",
            api: ApiKind::OpenAiCompletions,
            default_model: "meta-llama/Llama-3.2-3B-Instruct-Turbo",
        },
        Builtin {
            id: "fireworks",
            env: "FIREWORKS_API_KEY",
            base_url: "https://api.fireworks.ai/inference/v1",
            api: ApiKind::OpenAiCompletions,
            default_model: "accounts/fireworks/models/llama-v3p1-8b-instruct",
        },
        Builtin {
            id: "nvidia",
            env: "NVIDIA_API_KEY",
            base_url: "https://integrate.api.nvidia.com/v1",
            api: ApiKind::OpenAiCompletions,
            default_model: "meta/llama-3.1-8b-instruct",
        },
    ]
}

fn builtin(id: &str) -> Option<&'static Builtin> {
    builtins().iter().find(|b| b.id == id)
}

fn models_from_auth_and_env(auth_path: &Path) -> Vec<DetectedModel> {
    let mut models = Vec::new();
    let auth = load_auth(auth_path).unwrap_or_default();
    for provider in builtins() {
        let has_auth = auth.has_api_key(provider.id) || std::env::var(provider.env).ok().filter(|v| !v.is_empty()).is_some();
        if has_auth {
            models.push(DetectedModel {
                provider: provider.id.into(),
                id: provider.default_model.into(),
                api: provider.api,
                base_url: provider.base_url.into(),
                source: if auth.has_api_key(provider.id) {
                    ModelSource::AuthFile
                } else {
                    ModelSource::Env
                },
            });
        }
    }
    models
}

fn models_from_json(path: &Path) -> Vec<DetectedModel> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(file) = serde_json::from_str::<ModelsFile>(&raw) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (provider_id, provider) in file.providers.unwrap_or_default() {
        let api = match provider.get("api").and_then(|v| v.as_str()) {
            Some("anthropic-messages" | "anthropic") => ApiKind::AnthropicMessages,
            _ => ApiKind::OpenAiCompletions,
        };
        let base = provider
            .get("baseUrl")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let models = provider
            .get("models")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        for model in models {
            if let Some(id) = model.get("id").and_then(|v| v.as_str()) {
                out.push(DetectedModel {
                    provider: provider_id.clone(),
                    id: id.to_string(),
                    api,
                    base_url: base.clone(),
                    source: ModelSource::ModelsJson,
                });
            }
        }
    }
    out
}

#[derive(Default, Deserialize)]
struct ModelsFile {
    providers: Option<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Default)]
struct AuthFile {
    providers: serde_json::Map<String, serde_json::Value>,
}

impl AuthFile {
    fn has_api_key(&self, provider: &str) -> bool {
        self.providers.get(provider).is_some_and(|v| {
            v.get("type").and_then(|t| t.as_str()) == Some("api_key")
                && v.get("key").and_then(|k| k.as_str()).is_some_and(|k| !k.is_empty())
        })
    }

    fn api_key(&self, provider: &str) -> Option<String> {
        let entry = self.providers.get(provider)?;
        let key = entry.get("key")?.as_str()?;
        resolve_key(key, entry.get("env"))
    }
}

fn load_auth(path: &Path) -> anyhow::Result<AuthFile> {
    let raw = std::fs::read_to_string(path)?;
    let providers: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&raw)?;
    Ok(AuthFile { providers })
}

fn resolve_key(key: &str, env_obj: Option<&serde_json::Value>) -> Option<String> {
    if let Some(name) = key.strip_prefix('$') {
        let name = name.trim_matches(|c| c == '{' || c == '}');
        if let Some(map) = env_obj.and_then(|v| v.as_object()) {
            if let Some(val) = map.get(name).and_then(|v| v.as_str()) {
                return Some(val.to_string());
            }
        }
        std::env::var(name).ok().filter(|v| !v.is_empty())
    } else if key.starts_with('!') {
        None
    } else {
        Some(key.to_string())
    }
}

fn rank_for_dictation(models: &mut [DetectedModel]) {
    models.sort_by_key(|m| {
        let local = m.source == ModelSource::ModelsJson;
        let cheap = m.id.contains("mini")
            || m.id.contains("haiku")
            || m.id.contains("8b")
            || m.id.contains("flash")
            || m.id.contains("small");
        (!local, !cheap, m.slug())
    });
}

const POLISH_SYSTEM: &str = "You are a dictation editor for a desktop voice-typing app. \
Rewrite the speech-to-text transcript into clean prose. \
Fix punctuation, capitalization, and obvious homophones. \
Honor spoken commands such as 'new line', 'new paragraph', 'period', and 'comma'. \
Do not add ideas the speaker did not say. Return only the cleaned transcript.";

pub fn polish(status: &PiStatus, requested: &str, transcript: &str) -> anyhow::Result<String> {
    let text = transcript.trim();
    anyhow::ensure!(!text.is_empty(), "empty transcript");
    let model = status
        .selected(requested)
        .cloned()
        .context("no Pi model available")?;

    if status.cli_present && model.source == ModelSource::PiCli {
        if let Ok(out) = polish_via_cli(&model, text) {
            return Ok(out);
        }
    }
    polish_via_http(&model, text)
}

fn polish_via_cli(model: &DetectedModel, transcript: &str) -> anyhow::Result<String> {
    let prompt = format!("{POLISH_SYSTEM}\n\nTranscript:\n{transcript}");
    let output = Command::new("pi")
        .args([
            "--print",
            "--no-session",
            "--provider",
            &model.provider,
            "--model",
            &model.id,
        ])
        .arg(&prompt)
        .output()
        .context("spawn pi --print")?;
    if !output.status.success() {
        anyhow::bail!(
            "pi --print failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    anyhow::ensure!(!stdout.is_empty(), "pi returned empty output");
    Ok(stdout)
}

fn polish_via_http(model: &DetectedModel, transcript: &str) -> anyhow::Result<String> {
    let key = api_key_for(&model.provider).context("missing API key for Pi provider")?;
    let user = format!("Transcript:\n{transcript}");
    match model.api {
        ApiKind::AnthropicMessages => anthropic_complete(&model.base_url, &key, &model.id, &user),
        ApiKind::OpenAiCompletions => openai_complete(&model.base_url, &key, &model.id, &user),
    }
}

fn api_key_for(provider: &str) -> Option<String> {
    let paths = PiPaths::default();
    if let Ok(auth) = load_auth(&paths.auth) {
        if let Some(key) = auth.api_key(provider) {
            return Some(key);
        }
    }
    let env = builtins().iter().find(|b| b.id == provider)?.env;
    std::env::var(env).ok().filter(|v| !v.is_empty())
}

fn openai_complete(base: &str, key: &str, model: &str, user: &str) -> anyhow::Result<String> {
    let url = format!("{}/chat/completions", base.trim_end_matches('/'));
    let body = json!({
        "model": model,
        "temperature": 0.2,
        "messages": [
            {"role": "system", "content": POLISH_SYSTEM},
            {"role": "user", "content": user}
        ]
    });
    let mut response = ureq::post(&url)
        .header("Authorization", &format!("Bearer {key}"))
        .header("Content-Type", "application/json")
        .send_json(&body)
        .context("OpenAI-compatible complete")?;
    let value: serde_json::Value = response.body_mut().read_json()?;
    value
        .pointer("/choices/0/message/content")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .context("unexpected completions response")
}

fn anthropic_complete(base: &str, key: &str, model: &str, user: &str) -> anyhow::Result<String> {
    let url = format!("{}/v1/messages", base.trim_end_matches('/'));
    let body = json!({
        "model": model,
        "max_tokens": 1024,
        "system": POLISH_SYSTEM,
        "messages": [{"role": "user", "content": user}]
    });
    let mut response = ureq::post(&url)
        .header("x-api-key", key)
        .header("anthropic-version", "2023-06-01")
        .header("Content-Type", "application/json")
        .send_json(&body)
        .context("Anthropic complete")?;
    let value: serde_json::Value = response.body_mut().read_json()?;
    value
        .pointer("/content/0/text")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .context("unexpected Anthropic response")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn detects_env_openai() {
        let dir = tempdir().unwrap();
        let paths = PiPaths {
            auth: dir.path().join("missing-auth.json"),
            models: dir.path().join("missing-models.json"),
        };
        temp_env("OPENAI_API_KEY", "sk-test", || {
            let status = detect_from(paths, None);
            assert!(status.models.iter().any(|m| m.provider == "openai"));
            assert_eq!(status.selected("auto").unwrap().id, "gpt-4o-mini");
        });
    }

    #[test]
    fn prefers_models_json_local() {
        let dir = tempdir().unwrap();
        let models = dir.path().join("models.json");
        std::fs::write(
            &models,
            r#"{
              "providers": {
                "ollama": {
                  "baseUrl": "http://127.0.0.1:11434/v1",
                  "api": "openai-completions",
                  "apiKey": "ollama",
                  "models": [{"id": "llama3.2"}]
                }
              }
            }"#,
        )
        .unwrap();
        let paths = PiPaths {
            auth: dir.path().join("auth.json"),
            models,
        };
        let status = detect_from(paths, None);
        let first = status.selected("auto").unwrap();
        assert_eq!(first.provider, "ollama");
        assert_eq!(first.id, "llama3.2");
    }

    #[test]
    fn auth_json_api_key() {
        let dir = tempdir().unwrap();
        let auth = dir.path().join("auth.json");
        std::fs::write(
            &auth,
            r#"{"anthropic":{"type":"api_key","key":"sk-ant-test"}}"#,
        )
        .unwrap();
        let paths = PiPaths {
            auth,
            models: dir.path().join("models.json"),
        };
        let status = detect_from(paths, None);
        assert!(status.models.iter().any(|m| m.provider == "anthropic"));
    }

    fn temp_env(key: &str, value: &str, f: impl FnOnce()) {
        let prev = std::env::var(key).ok();
        std::env::set_var(key, value);
        f();
        match prev {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }
}
