use ashpd::desktop::global_shortcuts::{GlobalShortcuts, NewShortcut};
use futures_lite::StreamExt;

use super::{Hotkey, HotkeyAction};

/// XDG Desktop Portal Global Shortcuts. Works on compositors that implement
/// the portal (GNOME 48+, KDE, some others) without writing WM bind configs.
pub fn spawn(
    toggle: Hotkey,
    ptt: Hotkey,
    cancel: Hotkey,
    tx: flume::Sender<HotkeyAction>,
) -> anyhow::Result<()> {
    std::thread::Builder::new()
        .name("scribe-portal-hotkeys".into())
        .spawn(move || {
            if let Err(err) = smol::block_on(run(toggle, ptt, cancel, tx)) {
                tracing::warn!("portal global shortcuts unavailable: {err:#}");
            }
        })?;
    Ok(())
}

async fn run(
    toggle: Hotkey,
    ptt: Hotkey,
    cancel: Hotkey,
    tx: flume::Sender<HotkeyAction>,
) -> anyhow::Result<()> {
    let proxy = GlobalShortcuts::new().await?;
    let session = proxy.create_session().await?;
    let toggle_bind = portal_binding(&toggle);
    let ptt_bind = portal_binding(&ptt);
    let cancel_bind = portal_binding(&cancel);
    let shortcuts = [
        NewShortcut::new("toggle", "Toggle dictation").preferred_trigger(toggle_bind.as_str()),
        NewShortcut::new("ptt", "Push to talk").preferred_trigger(ptt_bind.as_str()),
        NewShortcut::new("cancel", "Cancel dictation").preferred_trigger(cancel_bind.as_str()),
    ];
    proxy
        .bind_shortcuts(&session, &shortcuts, None)
        .await?
        .response()?;

    let mut activated = proxy.receive_activated().await?;
    let mut deactivated = proxy.receive_deactivated().await?;
    tracing::info!("portal global shortcuts bound");

    loop {
        futures_lite::future::or(
            async {
                if let Some(event) = activated.next().await {
                    match event.shortcut_id() {
                        "toggle" => {
                            let _ = tx.send(HotkeyAction::Toggle);
                        }
                        "ptt" => {
                            let _ = tx.send(HotkeyAction::PushToTalkStart);
                        }
                        "cancel" => {
                            let _ = tx.send(HotkeyAction::Cancel);
                        }
                        _ => {}
                    }
                }
            },
            async {
                if let Some(event) = deactivated.next().await {
                    if event.shortcut_id() == "ptt" {
                        let _ = tx.send(HotkeyAction::PushToTalkStop);
                    }
                }
            },
        )
        .await;
    }
}

fn portal_binding(hotkey: &Hotkey) -> String {
    let chord = hotkey.chord();
    let mut parts: Vec<String> = Vec::new();
    if chord.ctrl {
        parts.push("CTRL".into());
    }
    if chord.alt {
        parts.push("ALT".into());
    }
    if chord.shift {
        parts.push("SHIFT".into());
    }
    if chord.super_key {
        parts.push("SUPER".into());
    }
    let last = match chord.key {
        super::KeyName::Space => "SPACE".to_string(),
        super::KeyName::Escape => "ESCAPE".to_string(),
        super::KeyName::Enter => "RETURN".to_string(),
        super::KeyName::Tab => "TAB".to_string(),
        super::KeyName::Backspace => "BACKSPACE".to_string(),
        super::KeyName::RightCtrl => "Control_R".to_string(),
        super::KeyName::LeftCtrl => "Control_L".to_string(),
        super::KeyName::RightAlt => "Alt_R".to_string(),
        super::KeyName::LeftAlt => "Alt_L".to_string(),
        super::KeyName::RightShift => "Shift_R".to_string(),
        super::KeyName::LeftShift => "Shift_L".to_string(),
        super::KeyName::RightSuper => "Super_R".to_string(),
        super::KeyName::LeftSuper => "Super_L".to_string(),
        super::KeyName::F(n) => format!("F{n}"),
        super::KeyName::Char(c) => c.to_ascii_uppercase().to_string(),
    };
    parts.push(last);
    parts.join("+")
}
