use std::collections::HashSet;
use std::os::unix::fs::FileTypeExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use evdev::{Device, EventSummary, KeyCode};

use super::Hotkey;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyAction {
    Toggle,
    PushToTalkStart,
    PushToTalkStop,
    Cancel,
}

pub struct EvdevListener {
    stop: Arc<AtomicBool>,
}

impl EvdevListener {
    pub fn spawn(
        toggle: Hotkey,
        ptt: Hotkey,
        cancel: Hotkey,
        tx: flume::Sender<HotkeyAction>,
    ) -> anyhow::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = stop.clone();
        std::thread::Builder::new()
            .name("scribe-evdev".into())
            .spawn(move || {
                if let Err(err) = run(toggle, ptt, cancel, tx, stop_thread) {
                    tracing::warn!("evdev hotkeys unavailable: {err:#}");
                }
            })?;
        Ok(Self { stop })
    }
}

impl Drop for EvdevListener {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn run(
    toggle: Hotkey,
    ptt: Hotkey,
    cancel: Hotkey,
    tx: flume::Sender<HotkeyAction>,
    stop: Arc<AtomicBool>,
) -> anyhow::Result<()> {
    let toggle = toggle.chord();
    let ptt = ptt.chord();
    let cancel = cancel.chord();

    let mut devices = open_keyboards()?;
    if devices.is_empty() {
        anyhow::bail!("no readable keyboard devices under /dev/input");
    }
    tracing::info!("evdev watching {} keyboard device(s)", devices.len());

    let mut down: HashSet<KeyCode> = HashSet::new();
    let mut ptt_held = false;
    let mut toggle_latched = false;
    let mut cancel_latched = false;

    while !stop.load(Ordering::Relaxed) {
        let mut got_event = false;
        for device in &mut devices {
            match device.fetch_events() {
                Ok(events) => {
                    for event in events {
                        let EventSummary::Key(_, code, value) = event.destructure() else {
                            continue;
                        };
                        got_event = true;
                        match value {
                            1 => {
                                down.insert(code);
                            }
                            0 => {
                                down.remove(&code);
                            }
                            _ => continue, // ignore repeats
                        }

                        let toggle_now = toggle.is_held(&down);
                        if toggle_now && !toggle_latched {
                            let _ = tx.send(HotkeyAction::Toggle);
                        }
                        toggle_latched = toggle_now;

                        let cancel_now = cancel.is_held(&down);
                        if cancel_now && !cancel_latched {
                            let _ = tx.send(HotkeyAction::Cancel);
                        }
                        cancel_latched = cancel_now;

                        let ptt_now = ptt.is_held(&down);
                        if ptt_now && !ptt_held {
                            let _ = tx.send(HotkeyAction::PushToTalkStart);
                        } else if !ptt_now && ptt_held {
                            let _ = tx.send(HotkeyAction::PushToTalkStop);
                        }
                        ptt_held = ptt_now;
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(err) => tracing::debug!("evdev read: {err}"),
            }
        }
        if !got_event {
            std::thread::sleep(Duration::from_millis(8));
        }
    }
    Ok(())
}

fn open_keyboards() -> anyhow::Result<Vec<Device>> {
    let mut devices = Vec::new();
    let entries = std::fs::read_dir("/dev/input").context("read /dev/input")?;
    for entry in entries.flatten() {
        let path: PathBuf = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.file_type().is_char_device() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("event") {
            continue;
        }
        match Device::open(&path) {
            Ok(device) => {
                let has_keys = device.supported_keys().is_some_and(|keys| {
                    keys.contains(KeyCode::KEY_SPACE) || keys.contains(KeyCode::KEY_A)
                });
                if has_keys {
                    devices.push(device);
                }
            }
            Err(err) => tracing::debug!("skip {}: {err}", path.display()),
        }
    }
    Ok(devices)
}

pub fn permission_hint() -> String {
    "Scribe reads `/dev/input` for global shortcuts so it works on X11, Wayland, and every compositor without extra bindings. Add your user to the `input` group (see scripts/install-linux-input.sh) and log in again.".into()
}

pub fn can_read_input() -> bool {
    open_keyboards().map(|d| !d.is_empty()).unwrap_or(false)
}
