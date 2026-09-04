mod evdev_backend;
mod portal;
mod x11;

pub use evdev_backend::{can_read_input, permission_hint, EvdevListener};
pub use evdev_backend::HotkeyAction;

use anyhow::Context;
use serde::{Deserialize, Serialize};

/// A layout-independent combo described with Linux input key names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Hotkey {
    pub spec: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chord {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub super_key: bool,
    pub key: KeyName,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyName {
    Space,
    Escape,
    Enter,
    Tab,
    Backspace,
    RightCtrl,
    LeftCtrl,
    RightAlt,
    LeftAlt,
    RightShift,
    LeftShift,
    RightSuper,
    LeftSuper,
    Char(char),
    F(u8),
}

impl Hotkey {
    pub fn parse(spec: &str) -> anyhow::Result<Self> {
        Chord::parse(spec)?;
        Ok(Self {
            spec: normalize(spec),
        })
    }

    pub fn chord(&self) -> Chord {
        Chord::parse(&self.spec).expect("hotkey was validated on parse")
    }
}

impl std::fmt::Display for Hotkey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.spec)
    }
}

impl Chord {
    pub fn parse(spec: &str) -> anyhow::Result<Self> {
        let mut ctrl = false;
        let mut alt = false;
        let mut shift = false;
        let mut super_key = false;
        let mut key = None;

        for raw in spec.split('+') {
            let part = raw.trim();
            if part.is_empty() {
                continue;
            }
            match &*part.to_ascii_lowercase().replace([' ', '_', '-'], "") {
                "ctrl" | "control" | "leftctrl" => ctrl = true,
                "rightctrl" | "rctrl" => {
                    key = Some(KeyName::RightCtrl);
                }
                "alt" | "leftalt" | "opt" | "option" => alt = true,
                "rightalt" | "ralt" | "altgr" => key = Some(KeyName::RightAlt),
                "shift" | "leftshift" => shift = true,
                "rightshift" | "rshift" => key = Some(KeyName::RightShift),
                "super" | "meta" | "win" | "cmd" | "leftsuper" => super_key = true,
                "rightsuper" | "rsuper" | "rightmeta" => key = Some(KeyName::RightSuper),
                "space" => key = Some(KeyName::Space),
                "esc" | "escape" => key = Some(KeyName::Escape),
                "enter" | "return" => key = Some(KeyName::Enter),
                "tab" => key = Some(KeyName::Tab),
                "backspace" => key = Some(KeyName::Backspace),
                other if other.starts_with('f') && other[1..].parse::<u8>().is_ok() => {
                    let n = other[1..].parse::<u8>().unwrap();
                    anyhow::ensure!((1..=24).contains(&n), "invalid function key {other}");
                    key = Some(KeyName::F(n));
                }
                other if other.len() == 1 => {
                    let c = other.chars().next().unwrap();
                    anyhow::ensure!(c.is_ascii_alphanumeric(), "unsupported key {other}");
                    key = Some(KeyName::Char(c));
                }
                other => anyhow::bail!("unknown key `{other}`"),
            }
        }

        let key = key.context("hotkey needs a non-modifier key")?;
        Ok(Self {
            ctrl,
            alt,
            shift,
            super_key,
            key,
        })
    }

    pub fn evdev_codes(&self) -> Vec<evdev::KeyCode> {
        let mut codes = Vec::new();
        if self.ctrl {
            codes.push(evdev::KeyCode::KEY_LEFTCTRL);
        }
        if self.alt {
            codes.push(evdev::KeyCode::KEY_LEFTALT);
        }
        if self.shift {
            codes.push(evdev::KeyCode::KEY_LEFTSHIFT);
        }
        if self.super_key {
            codes.push(evdev::KeyCode::KEY_LEFTMETA);
        }
        codes.push(self.key.evdev());
        codes
    }

    /// True when every key in the chord is currently held.
    pub fn is_held(&self, down: &std::collections::HashSet<evdev::KeyCode>) -> bool {
        let ctrl = !self.ctrl
            || down.contains(&evdev::KeyCode::KEY_LEFTCTRL)
            || down.contains(&evdev::KeyCode::KEY_RIGHTCTRL);
        let alt = !self.alt
            || down.contains(&evdev::KeyCode::KEY_LEFTALT)
            || down.contains(&evdev::KeyCode::KEY_RIGHTALT);
        let shift = !self.shift
            || down.contains(&evdev::KeyCode::KEY_LEFTSHIFT)
            || down.contains(&evdev::KeyCode::KEY_RIGHTSHIFT);
        let super_key = !self.super_key
            || down.contains(&evdev::KeyCode::KEY_LEFTMETA)
            || down.contains(&evdev::KeyCode::KEY_RIGHTMETA);
        ctrl && alt && shift && super_key && down.contains(&self.key.evdev())
    }

    pub fn display(&self) -> String {
        let mut parts = Vec::new();
        if self.super_key {
            parts.push("Super".into());
        }
        if self.ctrl {
            parts.push("Ctrl".into());
        }
        if self.alt {
            parts.push("Alt".into());
        }
        if self.shift {
            parts.push("Shift".into());
        }
        parts.push(self.key.label());
        parts.join("+")
    }
}

impl KeyName {
    pub fn evdev(self) -> evdev::KeyCode {
        match self {
            Self::Space => evdev::KeyCode::KEY_SPACE,
            Self::Escape => evdev::KeyCode::KEY_ESC,
            Self::Enter => evdev::KeyCode::KEY_ENTER,
            Self::Tab => evdev::KeyCode::KEY_TAB,
            Self::Backspace => evdev::KeyCode::KEY_BACKSPACE,
            Self::RightCtrl => evdev::KeyCode::KEY_RIGHTCTRL,
            Self::LeftCtrl => evdev::KeyCode::KEY_LEFTCTRL,
            Self::RightAlt => evdev::KeyCode::KEY_RIGHTALT,
            Self::LeftAlt => evdev::KeyCode::KEY_LEFTALT,
            Self::RightShift => evdev::KeyCode::KEY_RIGHTSHIFT,
            Self::LeftShift => evdev::KeyCode::KEY_LEFTSHIFT,
            Self::RightSuper => evdev::KeyCode::KEY_RIGHTMETA,
            Self::LeftSuper => evdev::KeyCode::KEY_LEFTMETA,
            Self::F(n) => match n {
                1 => evdev::KeyCode::KEY_F1,
                2 => evdev::KeyCode::KEY_F2,
                3 => evdev::KeyCode::KEY_F3,
                4 => evdev::KeyCode::KEY_F4,
                5 => evdev::KeyCode::KEY_F5,
                6 => evdev::KeyCode::KEY_F6,
                7 => evdev::KeyCode::KEY_F7,
                8 => evdev::KeyCode::KEY_F8,
                9 => evdev::KeyCode::KEY_F9,
                10 => evdev::KeyCode::KEY_F10,
                11 => evdev::KeyCode::KEY_F11,
                12 => evdev::KeyCode::KEY_F12,
                _ => evdev::KeyCode::KEY_F1,
            },
            Self::Char(c) => match c.to_ascii_lowercase() {
                'a' => evdev::KeyCode::KEY_A,
                'b' => evdev::KeyCode::KEY_B,
                'c' => evdev::KeyCode::KEY_C,
                'd' => evdev::KeyCode::KEY_D,
                'e' => evdev::KeyCode::KEY_E,
                'f' => evdev::KeyCode::KEY_F,
                'g' => evdev::KeyCode::KEY_G,
                'h' => evdev::KeyCode::KEY_H,
                'i' => evdev::KeyCode::KEY_I,
                'j' => evdev::KeyCode::KEY_J,
                'k' => evdev::KeyCode::KEY_K,
                'l' => evdev::KeyCode::KEY_L,
                'm' => evdev::KeyCode::KEY_M,
                'n' => evdev::KeyCode::KEY_N,
                'o' => evdev::KeyCode::KEY_O,
                'p' => evdev::KeyCode::KEY_P,
                'q' => evdev::KeyCode::KEY_Q,
                'r' => evdev::KeyCode::KEY_R,
                's' => evdev::KeyCode::KEY_S,
                't' => evdev::KeyCode::KEY_T,
                'u' => evdev::KeyCode::KEY_U,
                'v' => evdev::KeyCode::KEY_V,
                'w' => evdev::KeyCode::KEY_W,
                'x' => evdev::KeyCode::KEY_X,
                'y' => evdev::KeyCode::KEY_Y,
                'z' => evdev::KeyCode::KEY_Z,
                '0' => evdev::KeyCode::KEY_0,
                '1' => evdev::KeyCode::KEY_1,
                '2' => evdev::KeyCode::KEY_2,
                '3' => evdev::KeyCode::KEY_3,
                '4' => evdev::KeyCode::KEY_4,
                '5' => evdev::KeyCode::KEY_5,
                '6' => evdev::KeyCode::KEY_6,
                '7' => evdev::KeyCode::KEY_7,
                '8' => evdev::KeyCode::KEY_8,
                '9' => evdev::KeyCode::KEY_9,
                _ => evdev::KeyCode::KEY_SPACE,
            },
        }
    }

    pub fn x11_keysym(self) -> u32 {
        match self {
            Self::Space => 0x0020,
            Self::Escape => 0xff1b,
            Self::Enter => 0xff0d,
            Self::Tab => 0xff09,
            Self::Backspace => 0xff08,
            Self::RightCtrl => 0xffe4,
            Self::LeftCtrl => 0xffe3,
            Self::RightAlt => 0xffea,
            Self::LeftAlt => 0xffe9,
            Self::RightShift => 0xffe2,
            Self::LeftShift => 0xffe1,
            Self::RightSuper => 0xffec,
            Self::LeftSuper => 0xffeb,
            Self::F(n) => 0xffbe + (n.saturating_sub(1) as u32),
            Self::Char(c) => c.to_ascii_lowercase() as u32,
        }
    }

    pub fn label(self) -> String {
        match self {
            Self::Space => "Space".into(),
            Self::Escape => "Escape".into(),
            Self::Enter => "Enter".into(),
            Self::Tab => "Tab".into(),
            Self::Backspace => "Backspace".into(),
            Self::RightCtrl => "RightCtrl".into(),
            Self::LeftCtrl => "LeftCtrl".into(),
            Self::RightAlt => "RightAlt".into(),
            Self::LeftAlt => "LeftAlt".into(),
            Self::RightShift => "RightShift".into(),
            Self::LeftShift => "LeftShift".into(),
            Self::RightSuper => "RightSuper".into(),
            Self::LeftSuper => "LeftSuper".into(),
            Self::F(n) => format!("F{n}"),
            Self::Char(c) => c.to_ascii_uppercase().to_string(),
        }
    }
}

fn normalize(spec: &str) -> String {
    Chord::parse(spec).map(|c| c.display()).unwrap_or_else(|_| spec.to_string())
}

/// Start every backend that can run on this session. Evdev covers all
/// display servers; X11 grab and the GlobalShortcuts portal swallow keys
/// when the compositor supports them. No window-manager bind files required.
pub fn start_listeners(
    toggle: Hotkey,
    ptt: Hotkey,
    cancel: Hotkey,
) -> anyhow::Result<(flume::Receiver<HotkeyAction>, Option<EvdevListener>)> {
    let (tx, rx) = flume::unbounded();
    let evdev = match EvdevListener::spawn(toggle.clone(), ptt.clone(), cancel.clone(), tx.clone()) {
        Ok(listener) => Some(listener),
        Err(err) => {
            tracing::warn!("evdev listener: {err:#}");
            None
        }
    };
    if let Err(err) = x11::spawn(toggle.clone(), ptt.clone(), cancel.clone(), tx.clone()) {
        tracing::debug!("x11 listener: {err:#}");
    }
    if let Err(err) = portal::spawn(toggle, ptt, cancel, tx) {
        tracing::debug!("portal listener: {err:#}");
    }
    Ok((rx, evdev))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default_chords() {
        let toggle = Chord::parse("Super+Shift+Space").unwrap();
        assert!(toggle.super_key && toggle.shift);
        assert_eq!(toggle.key, KeyName::Space);
        assert_eq!(toggle.display(), "Super+Shift+Space");

        let ptt = Chord::parse("RightCtrl").unwrap();
        assert_eq!(ptt.key, KeyName::RightCtrl);
        assert!(!ptt.ctrl);
    }

    #[test]
    fn rejects_modifiers_only() {
        assert!(Chord::parse("Super+Shift").is_err());
    }

    #[test]
    fn held_matches_either_side_modifiers() {
        let chord = Chord::parse("Ctrl+Alt+D").unwrap();
        let mut down = std::collections::HashSet::new();
        down.insert(evdev::KeyCode::KEY_RIGHTCTRL);
        down.insert(evdev::KeyCode::KEY_LEFTALT);
        down.insert(evdev::KeyCode::KEY_D);
        assert!(chord.is_held(&down));
        down.remove(&evdev::KeyCode::KEY_D);
        assert!(!chord.is_held(&down));
    }
}
