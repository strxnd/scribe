use std::io::{BufRead, Write};
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryItem {
    pub at: DateTime<Utc>,
    pub engine: String,
    pub raw: String,
    pub final_text: String,
    pub polished: bool,
}

pub fn append(path: &Path, item: &HistoryItem) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    serde_json::to_writer(&mut file, item)?;
    file.write_all(b"\\n")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn append_and_read() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("history.jsonl");
        append(
            &path,
            &HistoryItem {
                at: Utc::now(),
                engine: "whisper".into(),
                raw: "hello".into(),
                final_text: "Hello.".into(),
                polished: true,
            },
        )
        .unwrap();
        let items = load_recent(&path, 10);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].final_text, "Hello.");
    }
}
