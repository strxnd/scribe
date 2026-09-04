use std::time::Duration;

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ChangeWindowAttributesAux, ClientMessageEvent, ConfigureWindowAux, ConnectionExt,
    EventMask, StackMode, Window,
};

const CLASS_MARK: &str = "dev.scribe.Scribe";

/// Keep the HUD overlay at the bottom-center of the screen.
///
/// GNOME/Mutter ignores the initial `ConfigureWindow` for app windows, so we
/// steal the HUD with `override_redirect` and place it ourselves.
pub fn pin_overlay_async(width: i32, height: i32, bottom_margin: i32) {
    if std::env::var_os("DISPLAY").is_none() {
        return;
    }
    std::thread::Builder::new()
        .name("scribe-hud-pin".into())
        .spawn(move || {
            for i in 0..16 {
                std::thread::sleep(Duration::from_millis(30 + i * 25));
                match pin_once(width, height, bottom_margin) {
                    Ok(true) => tracing::debug!("hud pin ok"),
                    Ok(false) => {}
                    Err(err) => tracing::debug!("hud pin: {err:#}"),
                }
            }
        })
        .ok();
}

fn pin_once(width: i32, height: i32, bottom_margin: i32) -> anyhow::Result<bool> {
    let (conn, screen_num) = x11rb::connect(None)?;
    let screen = &conn.setup().roots[screen_num];
    let root = screen.root;
    let sw = screen.width_in_pixels as i32;
    let sh = screen.height_in_pixels as i32;
    let Some(win) = find_hud(&conn, root, width, height)? else {
        return Ok(false);
    };

    let x = ((sw - width) / 2).max(0);
    let y = (sh - height - bottom_margin).max(0);
    let pos = conn.translate_coordinates(win, root, 0, 0)?.reply()?;
    if (pos.dst_x as i32 - x).abs() <= 8 && (pos.dst_y as i32 - y).abs() <= 8 {
        conn.configure_window(win, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE))?;
        conn.flush()?;
        return Ok(true);
    }

    conn.unmap_window(win)?;
    conn.change_window_attributes(
        win,
        &ChangeWindowAttributesAux::new().override_redirect(Some(1u32)),
    )?;
    conn.reparent_window(win, root, x as i16, y as i16)?;
    conn.configure_window(
        win,
        &ConfigureWindowAux::new()
            .x(x)
            .y(y)
            .width(width as u32)
            .height(height as u32)
            .stack_mode(StackMode::ABOVE),
    )?;
    conn.map_window(win)?;
    conn.configure_window(win, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE))?;

    let net_move = intern(&conn, b"_NET_MOVERESIZE_WINDOW")?;
    let msg = ClientMessageEvent::new(
        32,
        win,
        net_move,
        [1u32 | (2 << 8), x as u32, y as u32, width as u32, height as u32],
    );
    conn.send_event(
        false,
        root,
        EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
        msg,
    )?;
    conn.flush()?;
    Ok(true)
}

fn intern(conn: &impl Connection, name: &[u8]) -> anyhow::Result<u32> {
    Ok(conn.intern_atom(false, name)?.reply()?.atom)
}

fn find_hud(conn: &impl Connection, root: Window, width: i32, height: i32) -> anyhow::Result<Option<Window>> {
    let mut candidates = Vec::new();
    collect(conn, root, 0, &mut candidates)?;
    let mut best = None;
    for win in candidates {
        if !class_matches(conn, win)? {
            continue;
        }
        let geom = conn.get_geometry(win)?.reply()?;
        let dw = (geom.width as i32 - width).abs();
        let dh = (geom.height as i32 - height).abs();
        if dw <= 48 && dh <= 32 {
            best = Some(win);
            break;
        }
    }
    Ok(best)
}

fn collect(conn: &impl Connection, win: Window, depth: u8, out: &mut Vec<Window>) -> anyhow::Result<()> {
    if depth > 6 {
        return Ok(());
    }
    out.push(win);
    let tree = conn.query_tree(win)?.reply()?;
    for child in tree.children {
        collect(conn, child, depth + 1, out)?;
    }
    Ok(())
}

fn class_matches(conn: &impl Connection, win: Window) -> anyhow::Result<bool> {
    let reply = conn
        .get_property(false, win, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 256)?
        .reply()?;
    if reply.value.is_empty() {
        return Ok(false);
    }
    let text = String::from_utf8_lossy(&reply.value);
    Ok(text.contains(CLASS_MARK) || text.to_ascii_lowercase().contains("scribe"))
}
