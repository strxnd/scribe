use anyhow::Context;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt, EventMask, GrabMode, ModMask};
use x11rb::protocol::Event;

use super::{Hotkey, HotkeyAction};

/// X11 `XGrabKey` fallback. Swallows the combo on X11 (and XWayland).
pub fn spawn(
    toggle: Hotkey,
    ptt: Hotkey,
    cancel: Hotkey,
    tx: flume::Sender<HotkeyAction>,
) -> anyhow::Result<()> {
    if std::env::var_os("DISPLAY").is_none() {
        anyhow::bail!("DISPLAY is not set");
    }
    std::thread::Builder::new()
        .name("scribe-x11-hotkeys".into())
        .spawn(move || {
            if let Err(err) = run(toggle, ptt, cancel, tx) {
                tracing::warn!("X11 hotkeys unavailable: {err:#}");
            }
        })?;
    Ok(())
}

fn run(
    toggle: Hotkey,
    ptt: Hotkey,
    cancel: Hotkey,
    tx: flume::Sender<HotkeyAction>,
) -> anyhow::Result<()> {
    let (conn, screen_num) = x11rb::connect(None).context("connect to X11")?;
    let screen = &conn.setup().roots[screen_num];
    let root = screen.root;
    let min_kc = conn.setup().min_keycode;
    let max_kc = conn.setup().max_keycode;

    let toggle_kc = keycode_for(&conn, min_kc, max_kc, toggle.chord().key.x11_keysym())?;
    let ptt_kc = keycode_for(&conn, min_kc, max_kc, ptt.chord().key.x11_keysym())?;
    let cancel_kc = keycode_for(&conn, min_kc, max_kc, cancel.chord().key.x11_keysym())?;

    grab_all(&conn, root, toggle_kc, x_mods(&toggle.chord()))?;
    grab_all(&conn, root, ptt_kc, x_mods(&ptt.chord()))?;
    grab_all(&conn, root, cancel_kc, x_mods(&cancel.chord()))?;
    conn.flush()?;
    tracing::info!("X11 global grabs installed");

    let mut ptt_held = false;
    loop {
        let event = conn.wait_for_event()?;
        match event {
            Event::KeyPress(ev) => {
                let detail = ev.detail;
                if detail == toggle_kc {
                    let _ = tx.send(HotkeyAction::Toggle);
                } else if detail == cancel_kc {
                    let _ = tx.send(HotkeyAction::Cancel);
                } else if detail == ptt_kc && !ptt_held {
                    ptt_held = true;
                    let _ = tx.send(HotkeyAction::PushToTalkStart);
                }
            }
            Event::KeyRelease(ev) => {
                if ev.detail == ptt_kc && ptt_held {
                    ptt_held = false;
                    let _ = tx.send(HotkeyAction::PushToTalkStop);
                }
            }
            _ => {}
        }
    }
}

fn x_mods(chord: &super::Chord) -> ModMask {
    let mut mods = ModMask::default();
    if chord.ctrl {
        mods |= ModMask::CONTROL;
    }
    if chord.shift {
        mods |= ModMask::SHIFT;
    }
    if chord.alt {
        mods |= ModMask::M1;
    }
    if chord.super_key {
        mods |= ModMask::M4;
    }
    mods
}

fn grab_all(
    conn: &impl Connection,
    root: u32,
    keycode: u8,
    mods: ModMask,
) -> anyhow::Result<()> {
    let extras = [
        ModMask::default(),
        ModMask::LOCK,
        ModMask::M2,
        ModMask::LOCK | ModMask::M2,
    ];
    for extra in extras {
        conn.grab_key(
            true,
            root,
            mods | extra,
            keycode,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
        )?
        .check()?;
    }
    conn.change_window_attributes(
        root,
        &x11rb::protocol::xproto::ChangeWindowAttributesAux::new()
            .event_mask(EventMask::KEY_PRESS | EventMask::KEY_RELEASE),
    )?;
    Ok(())
}

fn keycode_for(
    conn: &impl Connection,
    min_kc: u8,
    max_kc: u8,
    keysym: u32,
) -> anyhow::Result<u8> {
    let count = max_kc.saturating_sub(min_kc).saturating_add(1);
    let mapping = conn.get_keyboard_mapping(min_kc, count)?.reply()?;
    let per = mapping.keysyms_per_keycode as usize;
    for (i, chunk) in mapping.keysyms.chunks(per).enumerate() {
        if chunk.contains(&keysym) {
            return Ok(min_kc + i as u8);
        }
    }
    anyhow::bail!("no keycode for keysym {keysym:#x}")
}
