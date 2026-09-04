use gpui::prelude::*;
use gpui::{
    div, px, ClickEvent, Context, ElementId, Entity, FontWeight, InteractiveElement, IntoElement,
    ParentElement, Render, SharedString, StatefulInteractiveElement, Styled, Window,
};

use crate::app::{AppModel, Phase};
use crate::config::InjectMethod;
use crate::hotkey;
use crate::history;
use crate::stt::{self, SpeechEngineKind};
use crate::ui::theme;

pub struct SettingsView {
    pub model: Entity<AppModel>,
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let width = window.viewport_size().width;
        let narrow = width < px(760.);
        let body = if narrow {
            div()
                .flex()
                .flex_col()
                .gap_4()
                .child(self.sidebar(cx))
                .child(self.main_column(cx))
        } else {
            div()
                .flex()
                .flex_row()
                .gap_5()
                .child(self.sidebar(cx).w(px(280.)).flex_shrink_0())
                .child(self.main_column(cx).flex_1().min_w_0())
        };

        div()
            .size_full()
            .bg(theme::bg())
            .text_color(theme::text())
            .font_family("Inter")
            .child(
                div()
                    .id("settings-scroll")
                    .size_full()
                    .px_6()
                    .py_5()
                    .overflow_y_scroll()
                    .child(body),
            )
    }
}

impl SettingsView {
    fn sidebar(&self, cx: &mut Context<Self>) -> gpui::Div {
        let (status, error, pi_line, input_ok, uinput_ok, model_ok, phase) = {
            let model = self.model.read(cx);
            let session = model.session.lock();
            let pi = session.pi.selected(&session.config.llm.model);
            let pi_line = match pi {
                Some(m) => format!("Pi · {}", m.slug()),
                None => "Pi · no model detected".into(),
            };
            let input_ok = hotkey::can_read_input();
            let uinput_ok = session.uinput_ready();
            let model_ok = session.model_ready();
            (
                model.status.clone(),
                model.error.clone(),
                pi_line,
                input_ok,
                uinput_ok,
                model_ok,
                model.phase,
            )
        };

        let mut col = div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme::amber())
                            .font_weight(FontWeight::MEDIUM)
                            .child("LINUX DICTATION"),
                    )
                    .child(
                        div()
                            .text_xl()
                            .font_weight(FontWeight::BOLD)
                            .child("Scribe"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme::muted())
                            .child("Speak. It types into the focused app."),
                    ),
            )
            .child(card("Status", status))
            .child(card("Language model", pi_line))
            .child(kv_card(&[
                ("Speech model", if model_ok { "ready" } else { "download required" }),
                ("Global shortcuts", if input_ok { "evdev connected" } else { "grant /dev/input" }),
                ("Typing", if uinput_ok { "uinput" } else { "clipboard fallback" }),
            ]))
            .child(self.hotkey_card(cx))
            .child(self.action_row(cx, model_ok, phase));
        if let Some(err) = error {
            col = col.child(card("Needs attention", err));
        }
        col
    }

    fn hotkey_card(&self, cx: &mut Context<Self>) -> gpui::Div {
        let session = self.model.read(cx).session.lock();
        let toggle = session.config.hotkeys.toggle.to_string();
        let ptt = session.config.hotkeys.push_to_talk.to_string();
        let cancel = session.config.hotkeys.cancel.to_string();
        drop(session);
        kv_card(&[
            ("Toggle", &toggle),
            ("Push to talk", &ptt),
            ("Cancel", &cancel),
        ])
        .child(
            div()
                .mt_2()
                .text_xs()
                .text_color(theme::muted())
                .whitespace_normal()
                .child("Registered by Scribe. No compositor bind file required."),
        )
    }

    fn action_row(&self, cx: &mut Context<Self>, model_ok: bool, phase: Phase) -> gpui::Div {
        let listen = self.model.clone();
        let listen_btn = match phase {
            Phase::Listening => {
                let listen = listen.clone();
                pill_button(
                    "Stop listening",
                    theme::coral(),
                    cx.listener(move |_, _, _, cx| {
                        listen.update(cx, |m, cx| m.stop_listening(cx));
                    }),
                )
            }
            Phase::Working => pill_button("Working…", theme::amber(), |_, _, _| {}),
            Phase::Idle => pill_button(
                "Start listening",
                theme::coral(),
                cx.listener(move |_, _, _, cx| {
                    listen.update(cx, |m, cx| m.start_listening(false, cx));
                }),
            ),
        };
        div()
            .flex()
            .flex_row()
            .gap_2()
            .child(listen_btn)
            .when(!model_ok, |el| {
                el.child(pill_button("Download model", theme::amber(), cx.listener(move |this, _, _, cx| {
                    let id = {
                        let session = this.model.read(cx).session.lock();
                        match session.config.speech.engine {
                            SpeechEngineKind::Parakeet => session.config.speech.parakeet_model.clone(),
                            SpeechEngineKind::Whisper => session.config.speech.whisper_model.clone(),
                        }
                    };
                    this.model.update(cx, |m, cx| m.download_model(id, cx));
                })))
            })
    }

    fn main_column(&self, cx: &mut Context<Self>) -> gpui::Div {
        div()
            .flex()
            .flex_col()
            .gap_4()
            .min_w_0()
            .child(self.speech_card(cx))
            .child(self.pi_card(cx))
            .child(self.inject_card(cx))
            .child(self.history_card(cx))
            .when(!hotkey::can_read_input(), |el| {
                el.child(card(
                    "Input permission",
                    hotkey::permission_hint(),
                ))
            })
    }

    fn speech_card(&self, cx: &mut Context<Self>) -> gpui::Div {
        let engine = self.model.read(cx).session.lock().config.speech.engine;
        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .rounded_xl()
            .bg(theme::surface())
            .border_1()
            .border_color(theme::border())
            .child(section_title("Speech"))
            .child(muted_copy(
                "Fully local. NVIDIA Parakeet for speed, OpenAI Whisper via whisper.cpp for familiarity.",
            ))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .child(self.engine_chip(SpeechEngineKind::Parakeet, engine, cx))
                    .child(self.engine_chip(SpeechEngineKind::Whisper, engine, cx)),
            )
            .child(self.model_picker(engine, cx))
    }

    fn engine_chip(
        &self,
        kind: SpeechEngineKind,
        current: SpeechEngineKind,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let selected = kind == current;
        div()
            .id(ElementId::from(kind.label()))
            .px_3()
            .py_2()
            .rounded_lg()
            .cursor_pointer()
            .bg(if selected { theme::surface_2() } else { theme::bg() })
            .border_1()
            .border_color(if selected { theme::amber() } else { theme::border() })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.model.update(cx, |m, cx| {
                    m.session.lock().config.speech.engine = kind;
                    m.session.lock().drop_engine();
                    m.save_config(cx);
                });
            }))
            .child(kind.label())
    }

    fn model_picker(&self, engine: SpeechEngineKind, cx: &mut Context<Self>) -> gpui::Div {
        let session = self.model.read(cx).session.lock();
        let current = match engine {
            SpeechEngineKind::Parakeet => session.config.speech.parakeet_model.clone(),
            SpeechEngineKind::Whisper => session.config.speech.whisper_model.clone(),
        };
        let paths = session.paths.clone();
        drop(session);
        let entries: Vec<_> = stt::CATALOG
            .iter()
            .filter(|e| e.kind == engine)
            .cloned()
            .collect();
        div()
            .flex()
            .flex_col()
            .gap_2()
            .children(entries.into_iter().map(|entry| {
                let installed = stt::is_installed(&paths, entry.kind, entry.id);
                let selected = current == entry.id;
                let id = entry.id.to_string();
                let id_click = id.clone();
                div()
                    .id(entry.id)
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .py_2()
                    .rounded_lg()
                    .cursor_pointer()
                    .bg(if selected { theme::surface_2() } else { theme::bg() })
                    .border_1()
                    .border_color(if selected { theme::amber() } else { theme::border() })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.model.update(cx, |m, cx| {
                            match engine {
                                SpeechEngineKind::Parakeet => {
                                    m.session.lock().config.speech.parakeet_model = id_click.clone();
                                }
                                SpeechEngineKind::Whisper => {
                                    m.session.lock().config.speech.whisper_model = id_click.clone();
                                }
                            }
                            m.session.lock().drop_engine();
                            m.save_config(cx);
                        });
                    }))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(div().text_sm().child(entry.label))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme::muted())
                                    .child(format!("{} · {}", entry.size_hint, if installed { "installed" } else { "not downloaded" })),
                            ),
                    )
                    .when(!installed, |el| {
                        let id = id.clone();
                        el.child(pill_button("Download", theme::blue(), cx.listener(move |this, _, _, cx| {
                            this.model.update(cx, |m, cx| m.download_model(id.clone(), cx));
                        })))
                    })
            }))
    }

    fn pi_card(&self, cx: &mut Context<Self>) -> gpui::Div {
        let session = self.model.read(cx).session.lock();
        let polish = session.config.llm.polish;
        let requested = session.config.llm.model.clone();
        let cli = session.pi.cli_present;
        let notes = session.pi.notes.clone();
        let models: Vec<_> = session.pi.models.iter().map(|m| m.slug()).collect();
        drop(session);

        let mut body = div()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .rounded_xl()
            .bg(theme::surface())
            .border_1()
            .border_color(theme::border())
            .child(section_title("Pi"))
            .child(muted_copy(
                "The only LLM provider in v1. Scribe auto-detects models from the pi CLI, ~/.pi/agent, and provider environment variables. Speech never leaves this machine.",
            ))
            .child(muted_copy_xs(if cli {
                "pi CLI found — using Pi's own model list when possible"
            } else {
                "pi CLI not found — reading Pi config and environment"
            }))
            .child(toggle_row("Polish transcript after speech-to-text", polish, cx.listener(|this, _, _, cx| {
                this.model.update(cx, |m, cx| {
                    let next = !m.session.lock().config.llm.polish;
                    m.session.lock().config.llm.polish = next;
                    m.save_config(cx);
                });
            })))
            .child(self.model_slug_row("auto", requested == "auto", cx));

        for slug in models {
            let selected = requested == slug;
            body = body.child(self.model_slug_row(&slug, selected, cx));
        }
        for note in notes {
            body = body.child(muted_copy_xs(note));
        }
        body
    }

    fn model_slug_row(&self, slug: &str, selected: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let slug_owned = slug.to_string();
        let label = if slug == "auto" {
            "Auto-detect".to_string()
        } else {
            slug.to_string()
        };
        let id = SharedString::from(format!("pi-{slug}"));
        div()
            .id(id)
            .px_3()
            .py_2()
            .rounded_lg()
            .cursor_pointer()
            .bg(if selected { theme::surface_2() } else { theme::bg() })
            .border_1()
            .border_color(if selected { theme::amber() } else { theme::border() })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.model.update(cx, |m, cx| {
                    m.session.lock().config.llm.model = slug_owned.clone();
                    m.save_config(cx);
                });
            }))
            .child(label)
    }

    fn inject_card(&self, cx: &mut Context<Self>) -> gpui::Div {
        let method = self.model.read(cx).session.lock().config.inject.method;
        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .rounded_xl()
            .bg(theme::surface())
            .border_1()
            .border_color(theme::border())
            .child(section_title("Insert text"))
            .child(muted_copy(
                "Works across X11 and Wayland. Auto copies to the clipboard and pastes with a virtual keyboard when /dev/uinput is available.",
            ))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .child(self.inject_chip(InjectMethod::Auto, method, cx))
                    .child(self.inject_chip(InjectMethod::Uinput, method, cx))
                    .child(self.inject_chip(InjectMethod::Clipboard, method, cx)),
            )
    }

    fn inject_chip(
        &self,
        method: InjectMethod,
        current: InjectMethod,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let label = match method {
            InjectMethod::Auto => "Auto",
            InjectMethod::Uinput => "Virtual keyboard",
            InjectMethod::Clipboard => "Clipboard only",
        };
        let selected = method == current;
        div()
            .id(gpui::ElementId::from(label))
            .px_3()
            .py_2()
            .rounded_lg()
            .cursor_pointer()
            .bg(if selected { theme::surface_2() } else { theme::bg() })
            .border_1()
            .border_color(if selected { theme::amber() } else { theme::border() })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.model.update(cx, |m, cx| {
                    m.session.lock().config.inject.method = method;
                    m.save_config(cx);
                });
            }))
            .child(label)
    }

    fn history_card(&self, cx: &mut Context<Self>) -> gpui::Div {
        let path = self.model.read(cx).session.lock().paths.history_file();
        let items = history::load_recent(&path, 8);
        let mut col = div()
            .flex()
            .flex_col()
            .gap_2()
            .p_4()
            .rounded_xl()
            .bg(theme::surface())
            .border_1()
            .border_color(theme::border())
            .child(section_title("Recent"))
            .child(muted_copy(
                self.model
                    .read(cx)
                    .last_transcript
                    .clone()
                    .if_empty("Nothing dictated yet."),
            ));
        if items.is_empty() {
            col = col.child(muted_copy(
                "Transcripts stay on this machine in history.jsonl.",
            ));
        }
        for item in items {
            col = col.child(
                div()
                    .p_3()
                    .rounded_lg()
                    .bg(theme::bg())
                    .child(
                        div()
                            .w_full()
                            .text_sm()
                            .whitespace_normal()
                            .child(item.final_text),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme::muted())
                            .child(format!(
                                "{} · {}",
                                item.at.format("%H:%M"),
                                item.engine
                            )),
                    ),
            );
        }
        col
    }
}

fn section_title(title: &str) -> gpui::Div {
    div()
        .text_sm()
        .font_weight(FontWeight::SEMIBOLD)
        .child(title.to_string())
}

fn muted_copy(text: impl Into<String>) -> gpui::Div {
    div()
        .w_full()
        .text_sm()
        .text_color(theme::muted())
        .whitespace_normal()
        .child(text.into())
}

fn muted_copy_xs(text: impl Into<String>) -> gpui::Div {
    div()
        .w_full()
        .text_xs()
        .text_color(theme::muted())
        .whitespace_normal()
        .child(text.into())
}

fn card(title: impl Into<String>, body: impl Into<String>) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .p_4()
        .rounded_xl()
        .bg(theme::surface())
        .border_1()
        .border_color(theme::border())
        .child(div().text_xs().text_color(theme::muted()).child(title.into()))
        .child(
            div()
                .w_full()
                .text_sm()
                .whitespace_normal()
                .child(body.into()),
        )
}

fn kv_card(rows: &[(&str, &str)]) -> gpui::Div {
    let mut col = div()
        .flex()
        .flex_col()
        .gap_2()
        .p_4()
        .rounded_xl()
        .bg(theme::surface())
        .border_1()
        .border_color(theme::border());
    for (k, v) in rows {
        col = col.child(
            div()
                .flex()
                .flex_row()
                .justify_between()
                .child(div().text_xs().text_color(theme::muted()).child((*k).to_string()))
                .child(div().text_xs().child((*v).to_string())),
        );
    }
    col
}

fn pill_button(
    label: &'static str,
    color: gpui::Rgba,
    listener: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(gpui::ElementId::from(label))
        .px_3()
        .py_2()
        .rounded_lg()
        .bg(color)
        .text_color(theme::bg())
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .cursor_pointer()
        .on_click(listener)
        .child(label)
}

fn toggle_row(
    label: &'static str,
    on: bool,
    listener: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id("polish-toggle")
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .cursor_pointer()
        .on_click(listener)
        .child(div().text_sm().child(label))
        .child(
            div()
                .w(px(36.))
                .h(px(20.))
                .rounded_full()
                .bg(if on { theme::green() } else { theme::border() })
                .p(px(2.))
                .child(
                    div()
                        .size(px(16.))
                        .rounded_full()
                        .bg(theme::text())
                        .when(on, |el| el.ml(px(16.))),
                ),
        )
}

trait IfEmpty {
    fn if_empty(self, fallback: &str) -> String;
}

impl IfEmpty for String {
    fn if_empty(self, fallback: &str) -> String {
        if self.trim().is_empty() {
            fallback.to_string()
        } else {
            self
        }
    }
}
