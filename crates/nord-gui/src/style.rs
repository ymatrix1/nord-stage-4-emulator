//! Hardware Surface Colors & Authentic Texture Tokens.
//!
//! Spec: `docs/specs/visual-style-and-panel.md`

use egui::Color32;

/// Signature Nord Crimson Red (`#8E1B1F`).
pub const CHASSIS_RED: Color32 = Color32::from_rgb(0x8E, 0x1B, 0x1F);

/// Dark recessed chassis chamfer shadow (`#641014`).
pub const CHASSIS_DARK: Color32 = Color32::from_rgb(0x64, 0x10, 0x14);

/// Bright red highlight for recessed STORE / ARP RUN buttons (`#B8252A`).
pub const BUTTON_STORE_RED: Color32 = Color32::from_rgb(0xB8, 0x25, 0x2A);

/// Anodized Slate Blue-Grey Module Background (`#323D47`).
pub const MODULE_SLATE: Color32 = Color32::from_rgb(0x32, 0x3D, 0x47);

/// Slightly darker slate for recessed frames (`#273038`).
pub const MODULE_SLATE_DARK: Color32 = Color32::from_rgb(0x27, 0x30, 0x38);

/// Brushed Silver-Blue Module Header Strip (`#A3AFB9`).
pub const MODULE_HEADER_BAR: Color32 = Color32::from_rgb(0xA3, 0xAF, 0xB9);

/// Silkscreen group outline border (`#9CA8B3`).
pub const GROUP_BORDER: Color32 = Color32::from_rgb(0x9C, 0xA8, 0xB3);

/// Matte Grey rectangular layer button (`#8C9196`).
pub const BUTTON_GREY: Color32 = Color32::from_rgb(0x8C, 0x91, 0x96);

/// Black selector pill button (`#1C1F22`).
pub const BUTTON_BLACK: Color32 = Color32::from_rgb(0x1C, 0x1F, 0x22);

/// Recessed button bezel dark border (`#0D0F11`).
pub const BUTTON_BEZEL: Color32 = Color32::from_rgb(0x0D, 0x0F, 0x11);

/// Active state indicator LED red (`#FF2A2A`).
pub const LED_RED: Color32 = Color32::from_rgb(0xFF, 0x2A, 0x2A);

/// Dim unlit state for red LEDs (`#4A0A0A`).
pub const LED_RED_DIM: Color32 = Color32::from_rgb(0x4A, 0x0A, 0x0A);

/// 11-segment level ladder & split active indicator LED green (`#5CE62E`).
pub const LED_GREEN: Color32 = Color32::from_rgb(0x5C, 0xE6, 0x2E);

/// Dim unlit state for green LEDs (`#0F380A`).
pub const LED_GREEN_DIM: Color32 = Color32::from_rgb(0x0F, 0x38, 0x0A);

/// Amber Morph assignment LED (`#FFB020`).
pub const LED_AMBER: Color32 = Color32::from_rgb(0xFF, 0xB0, 0x20);

/// Dim unlit state for amber LEDs (`#45300A`).
pub const LED_AMBER_DIM: Color32 = Color32::from_rgb(0x45, 0x30, 0x0A);

/// High-contrast monochrome OLED display background (`#080A0C`).
pub const OLED_BG: Color32 = Color32::from_rgb(0x08, 0x0A, 0x0C);

/// OLED ice-white pixel graphic & text font color (`#F0F6FC`).
pub const OLED_FG: Color32 = Color32::from_rgb(0xF0, 0xF6, 0xFC);

/// White harmonic drawbar cap (`#ECEBE6`).
pub const DRAWBAR_WHITE: Color32 = Color32::from_rgb(0xEC, 0xEB, 0xE6);

/// Black sub/mutation drawbar cap (`#1A1C1E`).
pub const DRAWBAR_BLACK: Color32 = Color32::from_rgb(0x1A, 0x1C, 0x1E);

/// Raw JPEG bytes of the authentic Nord Stage 4 high-fidelity front panel texture.
pub const PANEL_TEXTURE_BYTES: &[u8] = include_bytes!("../assets/panel_texture.jpg");

/// Decodes and loads the embedded panel texture into an `egui::TextureHandle`.
pub fn load_panel_texture(ctx: &egui::Context) -> egui::TextureHandle {
    let img = image::load_from_memory(PANEL_TEXTURE_BYTES)
        .expect("Embedded panel_texture.jpg must be valid JPEG");
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width() as usize, rgba.height() as usize);
    let color_image = egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba.into_raw());
    ctx.load_texture(
        "stage4_panel_texture",
        color_image,
        egui::TextureOptions::LINEAR,
    )
}

/// Red of the Program / Master blocks behind legends (`bay_program`, `panel::frames`).
pub const PROGRAM_RED: Color32 = Color32::from_rgb(115, 18, 22);

/// Smallest contrast ratio (WCAG 2, 4.5:1 for small text) a printed legend keeps against the panel,
/// dimmed or not.
pub const LEGEND_MIN_CONTRAST: f32 = 4.5;

fn linear(c: u8) -> f32 {
    let v = f32::from(c) / 255.0;
    if v <= 0.039_28 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

fn luminance(c: Color32) -> f32 {
    0.2126 * linear(c.r()) + 0.7152 * linear(c.g()) + 0.0722 * linear(c.b())
}

/// WCAG 2 contrast ratio between two opaque colours (1 to 21).
pub fn contrast_ratio(a: Color32, b: Color32) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// Colour of a dimmed (not supported yet, or secondary) legend: `ink` faded toward the panel, but
/// never below [`LEGEND_MIN_CONTRAST`] against the slate and the red of the panel, so a dimmed
/// legend stays readable. The fade is strongest at a blend of 0.45.
pub fn dim_legend(ink: Color32) -> Color32 {
    let lerp = |bg: Color32, t: f32| {
        let m = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8;
        Color32::from_rgb(m(bg.r(), ink.r()), m(bg.g(), ink.g()), m(bg.b(), ink.b()))
    };
    let mut t = 0.45_f32;
    while t < 1.0 {
        let c = lerp(MODULE_SLATE, t);
        if contrast_ratio(c, MODULE_SLATE) >= LEGEND_MIN_CONTRAST
            && contrast_ratio(lerp(PROGRAM_RED, t), PROGRAM_RED) >= LEGEND_MIN_CONTRAST
        {
            return c;
        }
        t += 0.01;
    }
    ink
}

/// Configure egui visual style to match authentic Nord chassis aesthetics.
pub fn setup_nord_visuals(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = CHASSIS_RED;
    visuals.window_fill = MODULE_SLATE;
    visuals.extreme_bg_color = OLED_BG;
    ctx.set_visuals(visuals);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_tokens() {
        assert_eq!(CHASSIS_RED.r(), 0x8E);
        assert_eq!(MODULE_SLATE.g(), 0x3D);
        assert_eq!(LED_GREEN.b(), 0x2E);
        assert_eq!(BUTTON_STORE_RED.r(), 0xB8);
    }

    #[test]
    fn contrast_ratio_matches_wcag_reference_values() {
        assert!((contrast_ratio(Color32::WHITE, Color32::BLACK) - 21.0).abs() < 0.01);
        assert!((contrast_ratio(Color32::WHITE, Color32::WHITE) - 1.0).abs() < 1e-3);
    }

    #[test]
    fn dimmed_legends_keep_the_minimum_contrast_on_slate_and_red() {
        for ink in [
            Color32::WHITE,
            Color32::from_gray(225),
            Color32::from_rgb(235, 190, 190),
        ] {
            let c = dim_legend(ink);
            for bg in [MODULE_SLATE, PROGRAM_RED] {
                assert!(
                    contrast_ratio(c, bg) >= LEGEND_MIN_CONTRAST,
                    "{c:?} on {bg:?}"
                );
            }
            // Still dimmer than the full ink.
            assert!(luminance(c) < luminance(ink));
        }
        // The legacy greys (150 and 105) were below the floor.
        assert!(contrast_ratio(Color32::from_gray(105), MODULE_SLATE) < LEGEND_MIN_CONTRAST);
    }

    #[test]
    fn test_embedded_panel_texture_valid_jpeg() {
        assert!(PANEL_TEXTURE_BYTES.len() > 1000);
        let img = image::load_from_memory(PANEL_TEXTURE_BYTES).expect("Valid JPEG");
        assert!(img.width() >= 1000);
        assert!(img.height() >= 100);
    }
}
