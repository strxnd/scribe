use std::f32::consts::PI;
use std::time::{Duration, Instant};

use gpui::{
    black, canvas, div, point, px, rgb, size, transparent_black, white, Animation, AnimationExt,
    App, AppContext, BorderStyle, Bounds, BoxShadow, Context, Corners, Edges, Entity, Hsla,
    InteractiveElement, IntoElement, ParentElement, Pixels, Point, Render, Styled, Window,
    WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions, quad,
};

use crate::app::{AppModel, Phase};

const ENTER_MS: f32 = 280.0;
const EXIT_MS: f32 = 220.0;

const PILL_W: f32 = 188.0;
const PILL_H: f32 = 48.0;
const PAD: f32 = 10.0;
const BOTTOM_MARGIN: f32 = 48.0;

const BAR_COUNT: usize = 23;
const BAR_W: f32 = 2.5;
const BAR_GAP: f32 = 3.2;
const MIN_H: f32 = 4.0;
const MAX_H: f32 = 22.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Visual {
    Bars,
    Wave,
    Exit,
}

pub struct HudView {
    pub model: Entity<AppModel>,
    appeared: Instant,
    visual: Visual,
    visual_since: Instant,
    smooth_level: f32,
    bar_h: [f32; BAR_COUNT],
    closed: bool,
}

impl HudView {
    pub fn new(model: Entity<AppModel>, phase: Phase, cx: &mut Context<Self>) -> Self {
        cx.observe(&model, |_, _, cx| cx.notify()).detach();
        crate::ui::pin::pin_overlay_async(
            (PILL_W + PAD * 2.0) as i32,
            (PILL_H + PAD * 2.0) as i32,
            BOTTOM_MARGIN as i32,
        );
        let visual = visual_for(phase);
        Self {
            model,
            appeared: Instant::now(),
            visual,
            visual_since: Instant::now(),
            smooth_level: 0.0,
            bar_h: [MIN_H; BAR_COUNT],
            closed: false,
        }
    }
}

impl Render for HudView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.set_background_appearance(WindowBackgroundAppearance::Transparent);

        let model = self.model.read(cx);
        let next = visual_for(model.phase);
        if next != self.visual {
            self.visual = next;
            self.visual_since = Instant::now();
            if next == Visual::Bars {
                self.appeared = Instant::now();
            }
        }
        let target = (model.level * 7.0).clamp(0.0, 1.0);
        let follow = if target > self.smooth_level { 0.5 } else { 0.14 };
        self.smooth_level += (target - self.smooth_level) * follow;
        let energy = self.smooth_level;
        let t = self.appeared.elapsed().as_secs_f32();

        for i in 0..BAR_COUNT {
            let want = match self.visual {
                Visual::Bars => listen_height(i, t, energy),
                Visual::Wave => wave_height(i, t),
                Visual::Exit => self.bar_h[i],
            };
            let k = match self.visual {
                Visual::Bars if want > self.bar_h[i] => 0.46,
                Visual::Bars => 0.18,
                Visual::Wave => 0.28,
                Visual::Exit => 0.10,
            };
            self.bar_h[i] += (want - self.bar_h[i]) * k;
        }

        let visual = self.visual;
        let enter = ease_out((t * 1000.0 / ENTER_MS).clamp(0.0, 1.0));
        let exit = if visual == Visual::Exit {
            ease_out(
                (self.visual_since.elapsed().as_secs_f32() * 1000.0 / EXIT_MS).clamp(0.0, 1.0),
            )
        } else {
            0.0
        };
        if visual == Visual::Exit && exit >= 1.0 && !self.closed {
            self.closed = true;
            window.remove_window();
        }
        let appear = if visual == Visual::Exit {
            (1.0 - exit).clamp(0.0, 1.0)
        } else {
            enter
        };
        let bars = self.bar_h;

        div()
            .id("indicator")
            .size_full()
            .bg(transparent_black())
            .with_animation(
                "indicator-tick",
                Animation::new(Duration::from_millis(16)).repeat(),
                move |el, _| {
                    el.child(
                        canvas(
                            |_, _, _| {},
                            move |bounds, _, window, _| {
                                paint_indicator(window, bounds, bars, appear);
                            },
                        )
                        .size_full(),
                    )
                },
            )
    }
}

fn visual_for(phase: Phase) -> Visual {
    match phase {
        Phase::Listening => Visual::Bars,
        Phase::Working => Visual::Wave,
        Phase::Idle => Visual::Exit,
    }
}

fn ease_out(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(4)
}

fn mix(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn envelope(u: f32) -> f32 {
    (PI * u.clamp(0.0, 1.0)).sin().max(0.0).powf(0.72)
}

fn bar_seed(i: usize) -> f32 {
    let x = (i as u32)
        .wrapping_mul(1_103_515_245)
        .wrapping_add(12_345);
    (x % 10_000) as f32 / 10_000.0
}

fn listen_height(i: usize, t: f32, energy: f32) -> f32 {
    let n = (BAR_COUNT - 1).max(1) as f32;
    let u = i as f32 / n;
    let env = envelope(u);
    let seed = bar_seed(i);
    let idle_ripple = ((u * PI * 2.0) - t * 2.15).sin() * 0.5 + 0.5;
    let idle = 0.18 + 0.16 * idle_ripple;
    let a = (t * 8.6 + seed * 11.0 + u * 2.8).sin() * 0.5 + 0.5;
    let b = (t * 14.2 + seed * 6.4).sin() * 0.5 + 0.5;
    let live = 0.32 + 0.68 * (a * 0.6 + b * 0.4);
    let unit = mix(idle, live, energy) * mix(0.22, 1.0, env);
    mix(MIN_H, MAX_H, unit.clamp(0.0, 1.0))
}

/// A single bright pulse sweeping through the bars — readable as "working",
/// not a scribble and not another equalizer jumble.
fn wave_height(i: usize, t: f32) -> f32 {
    let n = (BAR_COUNT - 1).max(1) as f32;
    let u = i as f32 / n;
    let env = envelope(u).max(0.4);
    let pos = (t * 0.9).fract();
    let mut d = (u - pos).abs();
    d = d.min(1.0 - d);
    let sigma = 0.08;
    let bump = (-(d * d) / (2.0 * sigma * sigma)).exp();
    let unit = 0.08 + 0.92 * bump;
    mix(MIN_H, MAX_H, (unit * env).clamp(0.0, 1.0))
}

fn paint_indicator(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    bars: [f32; BAR_COUNT],
    appear: f32,
) {
    if appear < 0.01 {
        return;
    }
    let pill = Bounds {
        origin: point(bounds.origin.x + px(PAD), bounds.origin.y + px(PAD)),
        size: size(px(PILL_W * appear.clamp(0.35, 1.0)), px(PILL_H)),
    };
    let radius = px(PILL_H * 0.5);
    let fill: Hsla = rgb(0x111111).into();
    window.paint_shadows(
        pill,
        Corners::all(radius),
        &[BoxShadow {
            color: black().opacity(0.45 * appear),
            offset: point(px(0.), px(2.)),
            blur_radius: px(14.),
            spread_radius: px(0.),
        }],
    );
    window.paint_quad(quad(
        pill,
        Corners::all(radius),
        fill.opacity(appear),
        Edges::default(),
        transparent_black(),
        BorderStyle::default(),
    ));

    let n = (BAR_COUNT - 1).max(1) as f32;
    let span = BAR_COUNT as f32 * BAR_W + (BAR_COUNT - 1) as f32 * BAR_GAP;
    let width = span * mix(0.72, 1.0, appear);
    let center = Point {
        x: pill.origin.x + pill.size.width / 2.,
        y: pill.origin.y + pill.size.height / 2.,
    };
    let start = center.x - px(width * 0.5);
    let ink = white().opacity((0.94 * appear).clamp(0.0, 1.0));

    for i in 0..BAR_COUNT {
        let u = i as f32 / n;
        let delay = (u - 0.5).abs() * 0.42;
        let local = ease_out(((appear - delay) / (1.0 - delay).max(0.08)).clamp(0.0, 1.0));
        let x = start + px(u * width);
        let h = (bars[i] * local).max(BAR_W);
        if !h.is_finite() {
            continue;
        }
        paint_bar(window, x, center.y, BAR_W, h, ink);
    }
}

fn paint_bar(
    window: &mut Window,
    x: Pixels,
    y: Pixels,
    width: f32,
    height: f32,
    color: Hsla,
) {
    let width = width.clamp(0.5, 48.0);
    let height = height.clamp(width, 80.0);
    let hw = px(width * 0.5);
    let hh = px(height * 0.5);
    window.paint_quad(quad(
        Bounds {
            origin: point(x - hw, y - hh),
            size: size(px(width), px(height)),
        },
        Corners::all(px(width * 0.5)),
        color,
        Edges::default(),
        transparent_black(),
        BorderStyle::default(),
    ));
}

fn hud_bounds(cx: &App) -> (Bounds<Pixels>, Option<gpui::DisplayId>) {
    let hud_size = size(px(PILL_W + PAD * 2.0), px(PILL_H + PAD * 2.0));
    let display = cx.displays().into_iter().next();
    let display_id = display.as_ref().map(|d| d.id());
    let bounds = if let Some(display) = display {
        let screen = display.bounds();
        Bounds {
            origin: point(
                screen.origin.x + (screen.size.width - hud_size.width) / 2.,
                screen.origin.y + screen.size.height - hud_size.height - px(BOTTOM_MARGIN),
            ),
            size: hud_size,
        }
    } else {
        Bounds::centered(None, hud_size, cx)
    };
    (bounds, display_id)
}

pub fn open(cx: &mut App, model: Entity<AppModel>, phase: Phase) -> anyhow::Result<WindowHandle<HudView>> {
    let (bounds, display_id) = hud_bounds(cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            display_id,
            titlebar: None,
            focus: false,
            show: true,
            kind: WindowKind::Normal,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            window_background: WindowBackgroundAppearance::Transparent,
            window_decorations: Some(gpui::WindowDecorations::Client),
            window_min_size: Some(bounds.size),
            app_id: Some("dev.scribe.Scribe".into()),
            ..Default::default()
        },
        |window, cx| {
            window.set_background_appearance(WindowBackgroundAppearance::Transparent);
            cx.new(|cx| HudView::new(model, phase, cx))
        },
    )
}
