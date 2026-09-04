//! Scribe — system-wide AI dictation for Linux.
//!
//! Speech stays on-device (NVIDIA Parakeet and OpenAI Whisper). Language-model
//! cleanup goes through Pi, which auto-detects whichever models you already
//! configured.

pub mod app;
pub mod audio;
pub mod config;
pub mod history;
pub mod hotkey;
pub mod inject;
pub mod llm;
pub mod paths;
pub mod pipeline;
pub mod stt;
pub mod ui;

pub use config::Config;
pub use paths::AppPaths;
