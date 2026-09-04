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
    file.write_all(b"\n")?;
    Ok(())
}

pub fn load_recent(path: &Path, limit: usize) -> Vec<HistoryItem> {
    let Ok(file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let reader = std::io::BufReader::new(file);
    let mut items = Vec::new();
    for line in reader.lines().map_while(Result::ok) {
        if let Ok(item) = serde_json::from_str::<HistoryItem>(&line) {
            items.push(item);
        }
    }
    if items.len() > limit {
        items.drain(0..items.len() - limit);
    }
    items.reverse();
    items
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
