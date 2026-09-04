use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{
    px, size, App, AppContext, Bounds, Context, Entity, WindowBounds, WindowHandle, WindowKind,
    WindowOptions,
};
use parking_lot::Mutex;

use crate::audio::CaptureHandle;
use crate::hotkey::HotkeyAction;
use crate::inject::InjectOutcome;
use crate::pipeline::Session;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Listening,
    Working,
}

pub struct AppModel {
    pub session: Arc<Mutex<Session>>,
    pub phase: Phase,
    pub ptt: bool,
    pub level: f32,
    pub status: String,
    pub last_transcript: String,
    pub error: Option<String>,
    pub download: Option<String>,
    capture: Option<CaptureHandle>,
    listening_since: Option<Instant>,
    _evdev: Option<crate::hotkey::EvdevListener>,
    hud: Option<WindowHandle<crate::ui::hud::HudView>>,
}

impl AppModel {
    pub fn new(session: Session, evdev: Option<crate::hotkey::EvdevListener>) -> Self {
        Self {
            session: Arc::new(Mutex::new(session)),
            phase: Phase::Idle,
            ptt: false,
            level: 0.0,
            status: "Idle — hold Right Ctrl to speak".into(),
            last_transcript: String::new(),
            error: None,
            download: None,
            capture: None,
            listening_since: None,
            _evdev: evdev,
            hud: None,
        }
    }

    pub fn handle_hotkey(&mut self, action: HotkeyAction, cx: &mut Context<Self>) {
        match action {
            HotkeyAction::Toggle => {
                if self.phase == Phase::Listening {
                    self.stop_listening(cx);
                } else if self.phase == Phase::Idle {
                    self.start_listening(false, cx);
                }
            }
            HotkeyAction::PushToTalkStart => {
                if self.phase == Phase::Idle {
                    self.start_listening(true, cx);
                }
            }
            HotkeyAction::PushToTalkStop => {
                if self.phase == Phase::Listening && self.ptt {
                    self.stop_listening(cx);
                }
            }
            HotkeyAction::Cancel => {
                if self.phase == Phase::Listening {
                    self.cancel_listening(cx);
                }
            }
        }
    }

    pub fn start_listening(&mut self, ptt: bool, cx: &mut Context<Self>) {
        if self.phase != Phase::Idle {
            return;
        }
        let device = self.session.lock().config.audio.input_device.clone();
        match crate::audio::start(device.as_deref()) {
            Ok(capture) => {
                self.capture = Some(capture);
                self.phase = Phase::Listening;
                self.ptt = ptt;
                self.error = None;
                self.listening_since = Some(Instant::now());
                self.status = if ptt {
                    "Listening — release to transcribe".into()
                } else {
                    "Listening — click Stop or press the toggle".into()
                };
                self.show_indicator(cx);
                cx.notify();
            }
            Err(err) => {
                self.error = Some(format!("Microphone: {err:#}"));
                self.status = "Microphone unavailable".into();
                cx.notify();
            }
        }
    }

    pub fn tick_level(&mut self, cx: &mut Context<Self>) {
        if let Some(capture) = &self.capture {
            self.level = capture.level();
            cx.notify();
        }
    }

    fn cancel_listening(&mut self, cx: &mut Context<Self>) {
        if let Some(capture) = self.capture.take() {
            let _ = capture.stop();
        }
        self.phase = Phase::Idle;
        self.ptt = false;
        self.level = 0.0;
        self.status = "Cancelled".into();
        self.hide_indicator_soon(cx);
        cx.notify();
    }

    pub fn stop_listening(&mut self, cx: &mut Context<Self>) {
        let Some(capture) = self.capture.take() else {
            return;
        };
        let recorded = capture.stop();
        self.level = 0.0;
        self.ptt = false;
        if recorded.is_too_short() {
            self.phase = Phase::Idle;
            self.status = "Too short — hold the key while you speak".into();
            self.hide_indicator_soon(cx);
            cx.notify();
            return;
        }
        self.phase = Phase::Working;
        self.status = "Transcribing…".into();
        self.show_indicator(cx);
        cx.notify();

        let samples = recorded.samples;
        let session = self.session.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let mut session = session.lock();
                    session.transcribe_and_inject(&samples)
                })
                .await;
            this.update(cx, |this, cx| {
                this.phase = Phase::Idle;
                match result {
                    Ok(done) => {
                        this.last_transcript = done.final_text.clone();
                        this.status = match done.inject {
                            InjectOutcome::Pasted => {
                                format!("Typed into the focused app ({})", done.engine.label())
                            }
                            InjectOutcome::ClipboardOnly => {
                                "Copied — press Ctrl+V in the focused app".into()
                            }
                            InjectOutcome::Empty => "Nothing to insert".into(),
                        };
                        this.error = None;
                    }
                    Err(err) => {
                        this.error = Some(format!("{err:#}"));
                        this.status = "Transcription failed".into();
                    }
                }
                this.hide_indicator_soon(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn show_indicator(&mut self, cx: &mut Context<Self>) {
        if self.hud.is_some() {
            return;
        }
        let model = cx.entity();
        let phase = self.phase;
        cx.defer(move |cx| {
            let (active, already) = {
                let this = model.read(cx);
                (this.phase != Phase::Idle, this.hud.is_some())
            };
            if !active || already {
                return;
            }
            match crate::ui::hud::open(cx, model.clone(), phase) {
                Ok(handle) => {
                    let _ = model.update(cx, |this, _| this.hud = Some(handle));
                }
                Err(err) => tracing::warn!("indicator: {err:#}"),
            }
        });
    }

    fn hide_indicator_soon(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(260))
                .await;
            this.update(cx, |this, cx| {
                if this.phase != Phase::Idle {
                    return;
                }
                if let Some(handle) = this.hud.take() {
                    let _ = handle.update(cx, |_, window, _| window.remove_window());
                }
            })
            .ok();
        })
        .detach();
    }

    pub fn save_config(&mut self, cx: &mut Context<Self>) {
        let session = self.session.lock();
        if let Err(err) = session.config.save(&session.paths.config_file()) {
            self.error = Some(format!("Save config: {err:#}"));
        }
        cx.notify();
    }

    pub fn download_model(&mut self, id: String, cx: &mut Context<Self>) {
        self.download = Some(format!("Downloading {id}…"));
        self.status = format!("Downloading {id}…");
        cx.notify();
        let session = self.session.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn({
                    let session = session.clone();
                    async move {
                        let paths = session.lock().paths.clone();
                        crate::stt::download_model(&paths, &id, |_| {})
                    }
                })
                .await;
            this.update(cx, |this, cx| {
                this.download = None;
                match result {
                    Ok(_) => {
                        this.session.lock().drop_engine();
                        this.status = "Model ready".into();
                        this.error = None;
                    }
                    Err(err) => {
                        this.error = Some(format!("Download: {err:#}"));
                        this.status = "Download failed".into();
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

pub fn open_settings(cx: &mut App, model: Entity<AppModel>) {
    let bounds = Bounds::centered(None, size(px(920.), px(640.)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(gpui::TitlebarOptions {
                title: Some("Scribe".into()),
                ..Default::default()
            }),
            focus: true,
            show: true,
            kind: WindowKind::Normal,
            window_min_size: Some(size(px(640.), px(480.))),
            app_id: Some("dev.scribe.Scribe".into()),
            ..Default::default()
        },
        |_, cx| cx.new(|_| crate::ui::settings::SettingsView { model }),
    )
    .ok();
}

pub fn wire_hotkeys(model: Entity<AppModel>, rx: flume::Receiver<HotkeyAction>, cx: &mut App) {
    let model_hotkeys = model.clone();
    cx.spawn(async move |cx| {
        while let Ok(action) = rx.recv_async().await {
            model_hotkeys
                .update(cx, |model, cx| model.handle_hotkey(action, cx))
                .ok();
        }
    })
    .detach();

    cx.spawn(async move |cx| {
        loop {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(50))
                .await;
            model
                .update(cx, |model, cx| {
                    if model.phase == Phase::Listening {
                        model.tick_level(cx);
                    }
                })
                .ok();
        }
    })
    .detach();
}
