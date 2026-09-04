use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::Context;

use crate::paths::AppPaths;
use super::{SpeechEngineKind, is_installed};

#[derive(Debug, Clone)]
pub struct CatalogEntry {
    pub kind: SpeechEngineKind,
    pub id: &'static str,
    pub label: &'static str,
    pub size_hint: &'static str,
    pub files: &'static [DownloadFile],
}

#[derive(Debug, Clone)]
pub struct DownloadFile {
    pub url: &'static str,
    pub relpath: &'static str,
}

#[derive(Debug, Clone)]
pub struct Progress {
    pub file: String,
    pub downloaded: u64,
    pub total: Option<u64>,
}

pub const CATALOG: &[CatalogEntry] = &[
    CatalogEntry {
        kind: SpeechEngineKind::Parakeet,
        id: "parakeet-tdt-0.6b-v3-int8",
        label: "Parakeet TDT 0.6B v3 (int8, multilingual)",
        size_hint: "~640 MB",
        files: &[
            DownloadFile {
                url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/main/encoder-model.int8.onnx?download=true",
                relpath: "parakeet-tdt-0.6b-v3-int8/encoder-model.int8.onnx",
            },
            DownloadFile {
                url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/main/decoder_joint-model.int8.onnx?download=true",
                relpath: "parakeet-tdt-0.6b-v3-int8/decoder_joint-model.int8.onnx",
            },
            DownloadFile {
                url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/main/nemo128.onnx?download=true",
                relpath: "parakeet-tdt-0.6b-v3-int8/nemo128.onnx",
            },
            DownloadFile {
                url: "https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx/resolve/main/vocab.txt?download=true",
                relpath: "parakeet-tdt-0.6b-v3-int8/vocab.txt",
            },
        ],
    },
    CatalogEntry {
        kind: SpeechEngineKind::Whisper,
        id: "ggml-tiny.en",
        label: "Whisper Tiny English",
        size_hint: "~75 MB",
        files: &[DownloadFile {
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin?download=true",
            relpath: "ggml-tiny.en.bin",
        }],
    },
    CatalogEntry {
        kind: SpeechEngineKind::Whisper,
        id: "ggml-base.en",
        label: "Whisper Base English",
        size_hint: "~142 MB",
        files: &[DownloadFile {
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin?download=true",
            relpath: "ggml-base.en.bin",
        }],
    },
    CatalogEntry {
        kind: SpeechEngineKind::Whisper,
        id: "ggml-small.en",
        label: "Whisper Small English",
        size_hint: "~466 MB",
        files: &[DownloadFile {
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en.bin?download=true",
            relpath: "ggml-small.en.bin",
        }],
    },
    CatalogEntry {
        kind: SpeechEngineKind::Whisper,
        id: "ggml-small",
        label: "Whisper Small multilingual",
        size_hint: "~466 MB",
        files: &[DownloadFile {
            url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin?download=true",
            relpath: "ggml-small.bin",
        }],
    },
];

pub fn download_model(
    paths: &AppPaths,
    id: &str,
    mut on_progress: impl FnMut(Progress),
) -> anyhow::Result<PathBuf> {
    let entry = CATALOG
        .iter()
        .find(|e| e.id == id)
        .with_context(|| format!("unknown model `{id}`"))?;
    paths.ensure()?;
    for file in entry.files {
        let dest = paths.models_dir.join(file.relpath);
        if dest.is_file() {
            continue;
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        download_file(file.url, &dest, |downloaded, total| {
            on_progress(Progress {
                file: file.relpath.to_string(),
                downloaded,
                total,
            });
        })?;
    }
    anyhow::ensure!(
        is_installed(paths, entry.kind, entry.id),
        "download finished but model files are incomplete"
    );
    Ok(paths.models_dir.join(entry.files[0].relpath).parent().unwrap_or(&paths.models_dir).to_path_buf())
}

fn download_file(
    url: &str,
    dest: &Path,
    mut on_progress: impl FnMut(u64, Option<u64>),
) -> anyhow::Result<()> {
    let tmp = dest.with_extension("partial");
    let mut response = ureq::get(url).call().with_context(|| format!("GET {url}"))?;
    let total = response
        .headers()
        .get("Content-Length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());
    let mut body = response.body_mut().as_reader();
    let mut file = std::fs::File::create(&tmp)?;
    let mut buf = [0u8; 64 * 1024];
    let mut downloaded = 0u64;
    loop {
        let n = body.read(&mut buf)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        downloaded += n as u64;
        on_progress(downloaded, total);
    }
    file.flush()?;
    std::fs::rename(tmp, dest)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_ids_are_unique() {
        let mut ids: Vec<_> = CATALOG.iter().map(|e| e.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), CATALOG.len());
    }
}
