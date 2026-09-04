use std::path::PathBuf;

use anyhow::Context;
use directories::ProjectDirs;

/// Filesystem locations for config, models, and history.
#[derive(Debug, Clone)]
pub struct AppPaths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub models_dir: PathBuf,
}

impl AppPaths {
    pub fn new() -> anyhow::Result<Self> {
        let dirs = ProjectDirs::from("dev", "scribe", "scribe")
            .context("could not resolve XDG directories")?;
        let config_dir = dirs.config_dir().to_path_buf();
        let data_dir = dirs.data_dir().to_path_buf();
        Ok(Self {
            models_dir: data_dir.join("models"),
            config_dir,
            data_dir,
        })
    }

    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }

    pub fn history_file(&self) -> PathBuf {
        self.data_dir.join("history.jsonl")
    }

    pub fn ensure(&self) -> anyhow::Result<()> {
        std::fs::create_dir_all(&self.config_dir)?;
        std::fs::create_dir_all(&self.data_dir)?;
        std::fs::create_dir_all(&self.models_dir)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_under_xdg() {
        let paths = AppPaths::new().unwrap();
        assert!(paths.config_file().ends_with("scribe/config.toml"));
        assert!(paths.models_dir.ends_with("scribe/models"));
    }
}
