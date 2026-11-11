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
    fn test_embedded_panel_texture_valid_jpeg() {
        assert!(PANEL_TEXTURE_BYTES.len() > 1000);
        let img = image::load_from_memory(PANEL_TEXTURE_BYTES).expect("Valid JPEG");
        assert!(img.width() >= 1000);
        assert!(img.height() >= 100);
    }
}
