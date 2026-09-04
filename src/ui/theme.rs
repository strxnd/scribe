use gpui::{rgb, Hsla, Rgba};

pub fn bg() -> Rgba {
    rgb(0x111111)
}
pub fn surface() -> Rgba {
    rgb(0x1a1a1a)
}
pub fn surface_2() -> Rgba {
    rgb(0x242424)
}
pub fn border() -> Rgba {
    rgb(0x333333)
}
pub fn text() -> Rgba {
    rgb(0xf2f2f2)
}
pub fn muted() -> Rgba {
    rgb(0x8a8a8a)
}
pub fn ink() -> Rgba {
    rgb(0xf5f5f5)
}

/// Light fill used for primary actions. Kept as `coral` so existing call sites stay put.
pub fn coral() -> Rgba {
    rgb(0xe8e8e8)
}
pub fn amber() -> Rgba {
    rgb(0xd4d4d4)
}
pub fn green() -> Rgba {
    rgb(0xf5f5f5)
}
pub fn blue() -> Rgba {
    rgb(0xc4c4c4)
}

pub fn with_alpha(color: Rgba, alpha: f32) -> Hsla {
    let mut c: Hsla = color.into();
    c.a = alpha;
    c
}
