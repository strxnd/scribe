use std::process::{Command, Stdio};

use anyhow::Context;
use arboard::Clipboard;
use evdev::{uinput::VirtualDevice, AttributeSet, InputEvent, KeyCode};

use crate::config::{InjectConfig, InjectMethod, PasteCombo};

pub struct Injector {
    device: Option<VirtualDevice>,
}

impl Injector {
    pub fn new() -> Self {
        Self {
            device: open_uinput().ok(),
        }
    }

    pub fn uinput_ready(&self) -> bool {
        self.device.is_some()
    }

    pub fn inject(&mut self, text: &str, cfg: &InjectConfig) -> anyhow::Result<InjectOutcome> {
        if text.is_empty() {
            return Ok(InjectOutcome::Empty);
        }
        copy_clipboard(text)?;
        match cfg.method {
            InjectMethod::Clipboard => Ok(InjectOutcome::ClipboardOnly),
            InjectMethod::Uinput => {
                self.paste(cfg.paste_combo)?;
                Ok(InjectOutcome::Pasted)
            }
            InjectMethod::Auto => {
                if self.device.is_some() {
                    self.paste(cfg.paste_combo)?;
                    Ok(InjectOutcome::Pasted)
                } else if paste_with_tools(cfg.paste_combo).is_ok() {
                    Ok(InjectOutcome::Pasted)
                } else {
                    Ok(InjectOutcome::ClipboardOnly)
                }
            }
        }
    }

    fn paste(&mut self, combo: PasteCombo) -> anyhow::Result<()> {
        let device = self
            .device
            .as_mut()
            .context("uinput device is not available")?;
        let keys = match combo {
            PasteCombo::CtrlShiftV => vec![KeyCode::KEY_LEFTCTRL, KeyCode::KEY_LEFTSHIFT, KeyCode::KEY_V],
            PasteCombo::ShiftInsert => vec![KeyCode::KEY_LEFTSHIFT, KeyCode::KEY_INSERT],
            PasteCombo::CtrlV | PasteCombo::Auto => {
                if looks_like_wayland() {
                    // Many terminals want Ctrl+Shift+V; regular apps want Ctrl+V.
                    // Auto prefers Ctrl+V because it matches editors, browsers, and GTK.
                    vec![KeyCode::KEY_LEFTCTRL, KeyCode::KEY_V]
                } else {
                    vec![KeyCode::KEY_LEFTCTRL, KeyCode::KEY_V]
                }
            }
        };
        emit_combo(device, &keys)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InjectOutcome {
    Pasted,
    ClipboardOnly,
    Empty,
}

fn copy_clipboard(text: &str) -> anyhow::Result<()> {
    match Clipboard::new().and_then(|mut cb| cb.set_text(text.to_string())) {
        Ok(()) => return Ok(()),
        Err(err) => tracing::debug!("arboard clipboard: {err}"),
    }
    if which::which("wl-copy").is_ok() {
        let mut child = Command::new("wl-copy")
            .stdin(Stdio::piped())
            .spawn()
            .context("wl-copy")?;
        if let Some(stdin) = child.stdin.as_mut() {
            use std::io::Write;
            stdin.write_all(text.as_bytes())?;
        }
        let _ = child.wait();
        return Ok(());
    }
    if which::which("xclip").is_ok() {
        let mut child = Command::new("xclip")
            .args(["-selection", "clipboard"])
            .stdin(Stdio::piped())
            .spawn()
            .context("xclip")?;
        if let Some(stdin) = child.stdin.as_mut() {
            use std::io::Write;
            stdin.write_all(text.as_bytes())?;
        }
        let _ = child.wait();
        return Ok(());
    }
    anyhow::bail!("could not write clipboard (install wl-clipboard or xclip)")
}

fn paste_with_tools(combo: PasteCombo) -> anyhow::Result<()> {
    let args = match combo {
        PasteCombo::CtrlShiftV => vec!["ctrl+shift+v"],
        PasteCombo::ShiftInsert => vec!["shift+Insert"],
        _ => vec!["ctrl+v"],
    };
    if which::which("xdotool").is_ok() && std::env::var_os("DISPLAY").is_some() {
        Command::new("xdotool").arg("key").args(&args).status()?;
        return Ok(());
    }
    if which::which("wtype").is_ok() {
        // wtype types characters; for paste we still rely on clipboard + key.
        Command::new("wtype").args(["-M", "ctrl", "-k", "v"]).status()?;
        return Ok(());
    }
    anyhow::bail!("no paste helper")
}

fn looks_like_wayland() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
}

fn open_uinput() -> anyhow::Result<VirtualDevice> {
    let mut keys = AttributeSet::<KeyCode>::new();
    for key in [
        KeyCode::KEY_LEFTCTRL,
        KeyCode::KEY_LEFTSHIFT,
        KeyCode::KEY_LEFTALT,
        KeyCode::KEY_V,
        KeyCode::KEY_INSERT,
    ] {
        keys.insert(key);
    }
    let device = VirtualDevice::builder()?
        .name("Scribe dictation")
        .with_keys(&keys)?
        .build()?;
    // Give udev / libinput a moment to pick up the virtual keyboard.
    std::thread::sleep(std::time::Duration::from_millis(80));
    Ok(device)
}

fn emit_combo(device: &mut VirtualDevice, keys: &[KeyCode]) -> anyhow::Result<()> {
    for key in keys {
        device.emit(&[InputEvent::new(1, key.code(), 1)])?;
    }
    device.emit(&[InputEvent::new(0, 0, 0)])?;
    std::thread::sleep(std::time::Duration::from_millis(12));
    for key in keys.iter().rev() {
        device.emit(&[InputEvent::new(1, key.code(), 0)])?;
    }
    device.emit(&[InputEvent::new(0, 0, 0)])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_is_noop() {
        let mut injector = Injector { device: None };
        let outcome = injector
            .inject("", &InjectConfig::default())
            .unwrap();
        assert_eq!(outcome, InjectOutcome::Empty);
    }
}
