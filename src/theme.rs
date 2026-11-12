//! Design tokens. Everything visual reads from here (see PLAN.md §7).

use iced::font::{Family, Weight};
use iced::{Color, Font};

pub const NIGHT: Color = rgb(0x16142A);
pub const DUSK: Color = rgb(0x262245);
pub const HAZE: Color = rgb(0x9A94BF);
pub const PAPER: Color = rgb(0xF1EDF8);
pub const MARQUEE: Color = rgb(0xFFB547);

/// Layout is designed at 1080p and scaled to the window height.
pub const DESIGN_HEIGHT: f32 = 1080.0;
/// TV overscan-safe margin; everything aligns to it.
pub const MARGIN: f32 = 96.0;

/// Type scale (px at 1080p).
#[allow(dead_code)]
pub mod size {
    pub const XS: f32 = 18.0;
    pub const S: f32 = 24.0;
    pub const M: f32 = 32.0;
    pub const L: f32 = 44.0;
    pub const XL: f32 = 64.0;
    pub const XXL: f32 = 88.0;
}

pub mod cover {
    pub const W: f32 = 200.0;
    pub const H: f32 = 300.0;
    pub const GAP: f32 = 28.0;
    /// Extra scale when focused.
    pub const FOCUS_GROW: f32 = 0.15;
    pub const RADIUS: f32 = 10.0;
}

const fn rgb(hex: u32) -> Color {
    Color::from_rgb8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

pub const fn display(weight: Weight) -> Font {
    Font { family: Family::Name("Bricolage Grotesque"), weight, ..Font::DEFAULT }
}

pub const fn body(weight: Weight) -> Font {
    Font { family: Family::Name("Atkinson Hyperlegible Next"), weight, ..Font::DEFAULT }
}

pub const FONT_FILES: &[&[u8]] = &[
    include_bytes!("../assets/fonts/Bricolage-400.ttf"),
    include_bytes!("../assets/fonts/Bricolage-600.ttf"),
    include_bytes!("../assets/fonts/Bricolage-800.ttf"),
    include_bytes!("../assets/fonts/Atkinson-400.ttf"),
    include_bytes!("../assets/fonts/Atkinson-700.ttf"),
];

pub fn lerp(a: Color, b: Color, t: f32) -> Color {
    Color {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: a.a + (b.a - a.a) * t,
    }
}

pub fn alpha(c: Color, a: f32) -> Color {
    Color { a, ..c }
}
