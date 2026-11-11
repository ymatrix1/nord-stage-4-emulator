//! 3D Skeuomorphic Hardware Widgets: Knobs, Drawbars, Faders, Buttons, LEDs, OLEDs, Keybed.
//!
//! Crafted to match 1:1 commercial hardware synthesizers (Nord Stage 4, Arturia, Softube)
//! with realistic physical depth: ambient occlusion drop shadows, metallic bevels,
//! concave grips, domed LED optics with diffuse light bloom, and glossy OLED glass.
//!
//! Spec: `docs/specs/visual-style-and-panel.md`

use crate::style::*;
use egui::{
    pos2, vec2, Align2, Color32, FontId, Painter, Pos2, Rect, Sense, Stroke, StrokeKind, Ui,
};
use std::f32::consts::{PI, TAU};

// -----------------------------------------------------------------------------
// Core Typography & Silkscreen
// -----------------------------------------------------------------------------

/// Crisp hardware silkscreen label with subtle drop shadow for maximum legibility.
pub fn label_3d(p: &Painter, pos: Pos2, text: &str, size: f32, col: Color32, align: Align2) {
    // 1. Subtle dark drop shadow
    p.text(
        pos + vec2(0.5, 0.8),
        align,
        text,
        FontId::proportional(size),
        Color32::from_black_alpha(160),
    );
    // 2. Crisp foreground silkscreen
    p.text(pos, align, text, FontId::proportional(size), col);
}

/// Compatibility alias for `label_3d`.
pub fn label(p: &Painter, pos: Pos2, s: &str, size: f32, col: Color32, al: Align2) {
    label_3d(p, pos, s, size, col, al);
}

// -----------------------------------------------------------------------------
// Module Chassis Cutouts & Group Boxes
// -----------------------------------------------------------------------------

/// Section Frame: Stamped powder-coated chassis cutout with recessed depth and brushed silver header.
pub fn section_frame_3d(p: &Painter, rect: Rect, title: &str) {
    // 1. Recessed outer drop shadow into red chassis
    p.rect_filled(
        rect.expand(2.5),
        6.0,
        Color32::from_rgb(0x38, 0x07, 0x0A), // Deep chassis recess
    );

    // 2. Anodized Slate Blue-Grey Module Background
    p.rect_filled(rect, 5.0, MODULE_SLATE);

    // 3. Brushed Silver-Blue Header Bar
    let hdr_rect = Rect::from_min_size(rect.min, vec2(rect.width(), 24.0));
    p.rect_filled(hdr_rect, 5.0, MODULE_HEADER_BAR);
    // Square off bottom corners of header
    p.rect_filled(
        Rect::from_min_size(
            pos2(hdr_rect.left(), hdr_rect.bottom() - 6.0),
            vec2(hdr_rect.width(), 6.0),
        ),
        0.0,
        MODULE_HEADER_BAR,
    );

    // Header top highlight line
    p.line_segment(
        [
            pos2(hdr_rect.left() + 4.0, hdr_rect.top() + 1.0),
            pos2(hdr_rect.right() - 4.0, hdr_rect.top() + 1.0),
        ],
        Stroke::new(1.0_f32, Color32::from_white_alpha(110)),
    );

    // Header bottom shadow line
    p.line_segment(
        [
            pos2(hdr_rect.left(), hdr_rect.bottom()),
            pos2(hdr_rect.right(), hdr_rect.bottom()),
        ],
        Stroke::new(1.2_f32, Color32::from_rgb(0x18, 0x1E, 0x24)),
    );

    // Engraved header title text (subtle 3D embossing)
    p.text(
        pos2(rect.left() + 10.0, hdr_rect.center().y + 0.5),
        Align2::LEFT_CENTER,
        title,
        FontId::proportional(12.5),
        Color32::from_white_alpha(100),
    );
    p.text(
        pos2(rect.left() + 10.0, hdr_rect.center().y),
        Align2::LEFT_CENTER,
        title,
        FontId::proportional(12.5),
        Color32::from_rgb(0x16, 0x1A, 0x20),
    );

    // Module boundary stroke
    p.rect_stroke(
        rect,
        5.0,
        Stroke::new(1.2_f32, Color32::from_rgb(0x1E, 0x25, 0x2C)),
        StrokeKind::Inside,
    );
}

/// Compatibility alias for `section_frame_3d`.
pub fn section_frame(p: &Painter, rect: Rect, title: &str) {
    section_frame_3d(p, rect, title);
}

/// Silkscreen Rounded Group Box with centered title cutout.
pub fn group_box_3d(p: &Painter, rect: Rect, title: &str) {
    // Border line
    p.rect_stroke(
        rect,
        6.0,
        Stroke::new(1.2_f32, GROUP_BORDER),
        StrokeKind::Inside,
    );

    // Header title cutout pill
    if !title.is_empty() {
        let text_w = title.len() as f32 * 6.5 + 12.0;
        let pill_rect =
            Rect::from_center_size(pos2(rect.center().x, rect.top()), vec2(text_w, 14.0));
        p.rect_filled(pill_rect, 0.0, MODULE_SLATE);

        label_3d(
            p,
            pill_rect.center(),
            title,
            9.0,
            Color32::from_gray(240),
            Align2::CENTER_CENTER,
        );
    }
}

/// Compatibility alias for `group_box_3d`.
pub fn group_frame(p: &Painter, rect: Rect, title: &str) {
    group_box_3d(p, rect, title);
}

// -----------------------------------------------------------------------------
// 3D Domed LED with Chrome Bezel & Diffuse Light Bloom
// -----------------------------------------------------------------------------

/// Renders an authentic domed LED indicator with metal bezel ring and luminous light bloom.
pub fn led_3d(p: &Painter, center: Pos2, is_on: bool, col_on: Color32, col_off: Color32) {
    // 1. Chrome / Dark Nickel Bezel Ring
    p.circle_filled(center, 5.5, Color32::from_rgb(0x12, 0x15, 0x18)); // Shadow socket
    p.circle_stroke(
        center,
        5.2,
        Stroke::new(1.2_f32, Color32::from_rgb(0x55, 0x5D, 0x66)), // Outer chrome highlight
    );

    // 2. Light Bloom / Glow onto panel when active
    if is_on {
        p.circle_filled(center, 10.0, col_on.gamma_multiply(0.18));
        p.circle_filled(center, 7.0, col_on.gamma_multiply(0.35));
    }

    // 3. LED Lens Core
    let core_col = if is_on { col_on } else { col_off };
    p.circle_filled(center, 3.8, core_col);

    // 4. Hot specular center glint
    if is_on {
        p.circle_filled(center, 1.8, Color32::WHITE.gamma_multiply(0.75));
        p.circle_filled(
            center + vec2(-1.0, -1.0),
            1.0,
            Color32::from_white_alpha(220),
        );
    } else {
        // Unlit glass reflection glint
        p.circle_filled(
            center + vec2(-0.8, -0.8),
            1.0,
            Color32::from_white_alpha(40),
        );
    }
}

/// Compatibility alias for `led_3d`.
pub fn led(p: &Painter, c: Pos2, on: bool, col_on: Color32, col_off: Color32) {
    led_3d(p, c, on, col_on, col_off);
}

// -----------------------------------------------------------------------------
// 3D Tactile Button (Convex Stamped Cap with Depression Physics)
// -----------------------------------------------------------------------------

/// Authentic 3D tactile button with recessed metal socket, bevel relief, and click depression.
pub fn button_3d(
    ui: &mut Ui,
    rect: Rect,
    id: &str,
    label: &str,
    active: bool,
    is_grey: bool,
) -> bool {
    let resp = ui.interact(rect, ui.id().with(id), Sense::click());
    let is_down = resp.is_pointer_button_down_on();
    let p = ui.painter();

    // 1. Recessed dark bezel cutout in the panel
    p.rect_filled(rect.expand(2.0), 4.0, Color32::from_rgb(0x0E, 0x10, 0x13));

    // 2. Button Body Position (depresses by 1.5px when clicked)
    let body_offset = if is_down {
        vec2(0.5, 1.5)
    } else {
        vec2(0.0, 0.0)
    };
    let body_rect = rect.translate(body_offset);

    // Base button color
    let base_color = if is_grey {
        if active {
            BUTTON_GREY
        } else {
            Color32::from_rgb(0x60, 0x65, 0x6A)
        }
    } else if active {
        Color32::from_rgb(0x2E, 0x33, 0x3A)
    } else {
        BUTTON_BLACK
    };

    // 3. Cast drop shadow under button cap (when not depressed)
    if !is_down {
        p.rect_filled(
            body_rect.translate(vec2(0.0, 2.0)),
            3.0,
            Color32::from_black_alpha(180),
        );
    }

    // 4. Button Cap Surface
    p.rect_filled(body_rect, 3.0, base_color);

    // 5. 3D Bevel Highlights & Shadows
    if !is_down {
        // Top edge light highlight
        p.line_segment(
            [
                pos2(body_rect.left() + 2.0, body_rect.top() + 1.0),
                pos2(body_rect.right() - 2.0, body_rect.top() + 1.0),
            ],
            Stroke::new(
                1.0_f32,
                Color32::from_white_alpha(if is_grey { 120 } else { 45 }),
            ),
        );
        // Left edge light highlight
        p.line_segment(
            [
                pos2(body_rect.left() + 1.0, body_rect.top() + 2.0),
                pos2(body_rect.left() + 1.0, body_rect.bottom() - 2.0),
            ],
            Stroke::new(
                1.0_f32,
                Color32::from_white_alpha(if is_grey { 80 } else { 30 }),
            ),
        );
        // Bottom edge shadow
        p.line_segment(
            [
                pos2(body_rect.left() + 1.0, body_rect.bottom() - 1.0),
                pos2(body_rect.right() - 1.0, body_rect.bottom() - 1.0),
            ],
            Stroke::new(1.2_f32, Color32::from_black_alpha(150)),
        );
    }

    // 6. Sculpted center finger pill recess
    let pill_rect = body_rect.shrink2(vec2(body_rect.width() * 0.14, body_rect.height() * 0.20));
    p.rect_filled(
        pill_rect,
        pill_rect.height() * 0.5,
        base_color.gamma_multiply(0.85),
    );
    p.rect_stroke(
        pill_rect,
        pill_rect.height() * 0.5,
        Stroke::new(
            1.0_f32,
            if is_down {
                Color32::from_black_alpha(100)
            } else {
                Color32::from_white_alpha(if is_grey { 90 } else { 30 })
            },
        ),
        StrokeKind::Inside,
    );

    // 7. Button Label
    if !label.is_empty() {
        label_3d(
            p,
            body_rect.center(),
            label,
            8.5,
            Color32::WHITE,
            Align2::CENTER_CENTER,
        );
    }

    resp.clicked()
}

/// Compatibility alias for `button_3d`.
pub fn nord_button(ui: &mut Ui, rect: Rect, id: &str, active: bool) -> bool {
    button_3d(ui, rect, id, "", active, false)
}

// -----------------------------------------------------------------------------
// 3D Rotary Knob with Knurled Grip & Sunken Indicator Cap
// -----------------------------------------------------------------------------

/// Authentic Nord 3D rotary knob with milled side ridges, metal collar, sunken cap, and pointer.
#[allow(clippy::too_many_arguments)]
pub fn knob_3d(
    ui: &mut Ui,
    center: Pos2,
    radius: f32,
    id: &str,
    value: &mut f32,
    min: f32,
    max: f32,
    label: &str,
) -> bool {
    let hit_rect = Rect::from_center_size(center, vec2(radius * 2.4, radius * 2.4));
    let resp = ui.interact(hit_rect, ui.id().with(id), Sense::click_and_drag());
    let mut changed = false;

    if resp.dragged() {
        let delta = -resp.drag_delta().y * 0.005 * (max - min);
        let new_val = (*value + delta).clamp(min, max);
        if new_val != *value {
            *value = new_val;
            changed = true;
        }
    }

    let p = ui.painter();
    let norm = ((*value - min) / (max - min).max(1e-6)).clamp(0.0, 1.0);
    let a0 = 0.75 * PI;
    let sweep = 1.5 * PI;

    // 1. Dial Scale Markings (Radial Ticks)
    for i in 0..=10 {
        let a = a0 + sweep * (i as f32 / 10.0);
        let r_in = radius + 3.0;
        let r_out = radius
            + if i == 0 || i == 5 || i == 10 {
                6.5
            } else {
                4.5
            };
        p.line_segment(
            [
                pos2(center.x + a.cos() * r_in, center.y + a.sin() * r_in),
                pos2(center.x + a.cos() * r_out, center.y + a.sin() * r_out),
            ],
            Stroke::new(1.2_f32, Color32::from_gray(215)),
        );
    }

    // 2. Cast Drop Shadow onto panel
    p.circle_filled(
        center + vec2(1.5, 3.0),
        radius + 1.5,
        Color32::from_black_alpha(150),
    );

    // 3. Knurled Grip Outer Skirt (Milled Ribs)
    let num_ribs = 28;
    for i in 0..num_ribs {
        let angle = (i as f32 / num_ribs as f32) * TAU;
        let c_rib = pos2(
            center.x + angle.cos() * radius,
            center.y + angle.sin() * radius,
        );
        // Highlight on top-left, shadow on bottom-right
        let shade = (angle.cos() * -0.5 - angle.sin() * 0.5).clamp(-0.4, 0.4);
        let rib_col = if shade > 0.0 {
            Color32::from_gray((42.0 + shade * 60.0) as u8)
        } else {
            Color32::from_gray((42.0 + shade * 30.0) as u8)
        };
        p.circle_filled(c_rib, 2.0, rib_col);
    }

    // 4. Main Knob Body
    p.circle_filled(center, radius, Color32::from_rgb(0x22, 0x24, 0x28));
    p.circle_stroke(
        center,
        radius,
        Stroke::new(1.0_f32, Color32::from_rgb(0x50, 0x56, 0x60)), // Top bevel
    );

    // 5. Sunken Inner Cap
    let inner_r = radius * 0.76;
    p.circle_filled(
        center - vec2(0.5, 0.5),
        inner_r + 0.5,
        Color32::from_rgb(0x10, 0x12, 0x14),
    );
    p.circle_filled(center, inner_r, Color32::from_rgb(0x28, 0x2B, 0x30));
    p.circle_stroke(
        center,
        inner_r,
        Stroke::new(1.0_f32, Color32::from_rgb(0x15, 0x18, 0x1A)),
    );

    // 6. Crisp White Indicator Pointer Notch (with its own drop shadow)
    let pointer_angle = a0 + sweep * norm;
    let ptr_dir = vec2(pointer_angle.cos(), pointer_angle.sin());
    let p_start = center + ptr_dir * (inner_r * 0.20);
    let p_end = center + ptr_dir * (inner_r - 1.5);

    // Pointer shadow
    p.line_segment(
        [p_start + vec2(0.8, 0.8), p_end + vec2(0.8, 0.8)],
        Stroke::new(2.5_f32, Color32::from_black_alpha(180)),
    );
    // Pointer line
    p.line_segment([p_start, p_end], Stroke::new(2.4_f32, Color32::WHITE));

    // 7. Parameter Label below knob
    if !label.is_empty() {
        label_3d(
            p,
            pos2(center.x, center.y + radius + 13.0),
            label,
            9.0,
            Color32::from_gray(230),
            Align2::CENTER_CENTER,
        );
    }

    changed
}

/// Compatibility alias for `knob_3d`.
#[allow(clippy::too_many_arguments)]
pub fn knob(
    ui: &mut Ui,
    center: Pos2,
    radius: f32,
    id: &str,
    value: &mut f32,
    min: f32,
    max: f32,
    param_label: &str,
) -> bool {
    knob_3d(ui, center, radius, id, value, min, max, param_label)
}

// -----------------------------------------------------------------------------
// 3D Layer Fader Strip (Vertical Slot + 11-Segment Green Ladder + Sculpted Cap)
// -----------------------------------------------------------------------------

/// 1:1 Nord Layer Mixer Strip with tactile fader cap, 11-segment LED ladder, and ON/OFF pill button.
pub fn layer_strip_3d(
    ui: &mut Ui,
    rect: Rect,
    id: &str,
    level: &mut f32,
    active: &mut bool,
    fader_tag: &str,
    _bus_label: &str,
) -> bool {
    let mut changed = false;

    // Fader track interaction area
    let fader_rect = Rect::from_min_max(
        pos2(rect.left(), rect.top() + 18.0),
        pos2(rect.left() + 20.0, rect.bottom() - 36.0),
    );
    let resp = ui.interact(fader_rect, ui.id().with(id), Sense::click_and_drag());
    if resp.dragged() || resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let n = 1.0
                - ((pos.y - fader_rect.top() - 8.0) / (fader_rect.height() - 16.0)).clamp(0.0, 1.0);
            *level = n;
            changed = true;
        }
    }

    // ON/OFF button at bottom
    let btn_rect = Rect::from_min_size(
        pos2(rect.left() - 4.0, rect.bottom() - 26.0),
        vec2(rect.width() + 8.0, 17.0),
    );
    if button_3d(ui, btn_rect, &format!("{id}_on"), "ON/OFF", *active, true) {
        *active = !*active;
        changed = true;
    }

    let p = ui.painter();

    // 1. Tag Label above fader (e.g. "A AUX KB")
    label_3d(
        p,
        pos2(fader_rect.center().x + 5.0, rect.top() + 4.0),
        fader_tag,
        9.0,
        Color32::WHITE,
        Align2::CENTER_CENTER,
    );

    // 2. Fader Vertical Track Slot (Recessed with metallic bevel)
    let slot_rect = Rect::from_min_max(
        pos2(fader_rect.center().x - 2.5, fader_rect.top()),
        pos2(fader_rect.center().x + 2.5, fader_rect.bottom()),
    );
    p.rect_filled(slot_rect, 2.0, Color32::from_rgb(0x0A, 0x0C, 0x0E));
    p.rect_stroke(
        slot_rect,
        2.0,
        Stroke::new(1.0_f32, Color32::from_rgb(0x2A, 0x30, 0x38)),
        StrokeKind::Inside,
    );

    // 3. 11-Segment Precision Green LED Ladder beside fader
    let ladder_x = fader_rect.right() + 4.0;
    let seg_w = 7.0;
    let seg_h = 4.5;
    let num_segs = 11;
    let total_h = fader_rect.height();
    let seg_spacing = total_h / num_segs as f32;

    for i in 0..num_segs {
        let seg_idx_from_bottom = num_segs - 1 - i;
        let seg_y = fader_rect.top() + (i as f32) * seg_spacing + 2.0;
        let seg_rect = Rect::from_min_size(pos2(ladder_x, seg_y), vec2(seg_w, seg_h));

        let threshold = (seg_idx_from_bottom as f32) / (num_segs as f32);
        let is_lit = *active && *level >= threshold;

        // Bezel housing
        p.rect_filled(
            seg_rect.expand(0.8),
            1.0,
            Color32::from_rgb(0x0C, 0x0E, 0x10),
        );

        let seg_col = if is_lit {
            if seg_idx_from_bottom >= 9 {
                LED_RED // Top two overload segments
            } else if seg_idx_from_bottom >= 7 {
                LED_ORANGE // Amber transition
            } else {
                LED_GREEN // Green signal
            }
        } else {
            Color32::from_rgb(0x14, 0x22, 0x18) // Unlit dim phosphor
        };

        p.rect_filled(seg_rect, 1.0, seg_col);

        if is_lit {
            p.rect_stroke(
                seg_rect,
                1.0,
                Stroke::new(1.0_f32, seg_col.gamma_multiply(0.4)),
                StrokeKind::Outside,
            );
        }
    }

    // 4. Metallic Sculpted Fader Cap with Grip Grooves
    let cap_w = 26.0;
    let cap_h = 15.0;
    let travel_range = fader_rect.height() - 16.0;
    let cap_y = fader_rect.bottom() - 8.0 - (*level * travel_range);
    let cap_rect = Rect::from_center_size(pos2(fader_rect.center().x, cap_y), vec2(cap_w, cap_h));

    // Drop shadow
    p.rect_filled(
        cap_rect.translate(vec2(1.5, 3.0)),
        2.5,
        Color32::from_black_alpha(170),
    );

    // Aluminum body
    p.rect_filled(cap_rect, 2.0, Color32::from_rgb(0xC8, 0xCE, 0xD4));

    // Metallic brushed horizontal bevel
    p.line_segment(
        [
            pos2(cap_rect.left() + 2.0, cap_rect.top() + 1.0),
            pos2(cap_rect.right() - 2.0, cap_rect.top() + 1.0),
        ],
        Stroke::new(1.2_f32, Color32::WHITE),
    );
    p.line_segment(
        [
            pos2(cap_rect.left() + 2.0, cap_rect.bottom() - 1.0),
            pos2(cap_rect.right() - 2.0, cap_rect.bottom() - 1.0),
        ],
        Stroke::new(1.2_f32, Color32::from_rgb(0x75, 0x7B, 0x82)),
    );

    // Center white index alignment notch
    p.line_segment(
        [
            pos2(cap_rect.left() + 3.0, cap_rect.center().y),
            pos2(cap_rect.right() - 3.0, cap_rect.center().y),
        ],
        Stroke::new(1.8_f32, Color32::WHITE),
    );

    // Active indicator LED under fader
    let led_pos = pos2(rect.center().x, rect.bottom() - 33.0);
    led_3d(p, led_pos, *active, LED_GREEN, LED_GREEN_DIM);

    changed
}

/// Compatibility alias for `layer_strip_3d`.
pub fn layer_strip(ui: &mut Ui, rect: Rect, id: &str, level: &mut f32, active: &mut bool) -> bool {
    layer_strip_3d(ui, rect, id, level, active, "", "")
}

// -----------------------------------------------------------------------------
// 3D Physical Drawbar (Stepped Cap + 8-Segment Red LED Ladder)
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawbarColorKind {
    White,
    Black,
    Brown,
}

/// Authentic Nord 3D Physical Drawbar with 8-segment red LED ladder and stepped 3D cap.
pub fn drawbar_3d(
    ui: &mut Ui,
    rect: Rect,
    id: &str,
    value: &mut u8,
    foot_label: &str,
    color_kind: DrawbarColorKind,
    harmonic_name: &str,
) -> bool {
    let mut changed = false;

    // Track interaction area
    let track_rect = Rect::from_min_max(
        pos2(rect.left(), rect.top() + 24.0),
        pos2(rect.right(), rect.bottom() - 18.0),
    );
    let resp = ui.interact(track_rect, ui.id().with(id), Sense::click_and_drag());
    if resp.dragged() || resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            // Drawbar is pulled DOWNWARDS for louder value (0..8)
            let n =
                ((pos.y - track_rect.top() - 12.0) / (track_rect.height() - 24.0)).clamp(0.0, 1.0);
            let new_val = (n * 8.0).round() as u8;
            if new_val != *value {
                *value = new_val;
                changed = true;
            }
        }
    }

    let p = ui.painter();

    // 1. Pipe Harmonic Name at top (e.g. "BASS16", "FLUTE8")
    label_3d(
        p,
        pos2(rect.center().x, rect.top() + 4.0),
        harmonic_name,
        7.5,
        Color32::from_gray(210),
        Align2::CENTER_CENTER,
    );

    // 2. Footage label at bottom (e.g. "16'", "8'")
    label_3d(
        p,
        pos2(rect.center().x, rect.bottom() - 6.0),
        foot_label,
        8.5,
        Color32::WHITE,
        Align2::CENTER_CENTER,
    );

    // 3. 8-Segment Red LED Ladder (Values 1 through 8)
    let ladder_x = rect.center().x - 6.0;
    let seg_w = 12.0;
    let seg_h = 5.0;
    let num_segs = 8;
    let ladder_top = track_rect.top() + 6.0;
    let ladder_h = track_rect.height() - 12.0;
    let seg_spacing = ladder_h / num_segs as f32;

    for i in 0..num_segs {
        let seg_val = (i + 1) as u8;
        let seg_y = ladder_top + (i as f32) * seg_spacing;
        let seg_rect = Rect::from_min_size(pos2(ladder_x, seg_y), vec2(seg_w, seg_h));

        let is_lit = *value >= seg_val;

        // Bezel
        p.rect_filled(
            seg_rect.expand(0.8),
            1.0,
            Color32::from_rgb(0x0C, 0x0E, 0x10),
        );

        let seg_col = if is_lit {
            LED_RED
        } else {
            Color32::from_rgb(0x28, 0x10, 0x12) // Dim unlit red phosphor
        };

        p.rect_filled(seg_rect, 1.0, seg_col);

        if is_lit {
            p.rect_stroke(
                seg_rect,
                1.0,
                Stroke::new(1.0_f32, LED_RED.gamma_multiply(0.4)),
                StrokeKind::Outside,
            );
        }

        // Value digit next to LED
        p.text(
            pos2(seg_rect.left() - 4.0, seg_rect.center().y),
            Align2::RIGHT_CENTER,
            format!("{seg_val}"),
            FontId::proportional(7.0),
            if is_lit {
                Color32::WHITE
            } else {
                Color32::from_gray(140)
            },
        );
    }

    // 4. Stepped 3D Drawbar Cap
    let cap_w = 26.0;
    let cap_h = 32.0;
    let travel_range = track_rect.height() - 36.0;
    let norm = (*value as f32) / 8.0;
    let cap_y = track_rect.top() + 16.0 + (norm * travel_range);
    let cap_rect = Rect::from_center_size(pos2(rect.center().x, cap_y), vec2(cap_w, cap_h));

    // Drawbar cap colors
    let (body_col, highlight_col, shadow_col) = match color_kind {
        DrawbarColorKind::White => (
            Color32::from_rgb(0xEA, 0xEB, 0xEE),
            Color32::WHITE,
            Color32::from_rgb(0xB0, 0xB6, 0xBE),
        ),
        DrawbarColorKind::Brown => (
            Color32::from_rgb(0x2A, 0x22, 0x1E),
            Color32::from_rgb(0x5A, 0x4B, 0x42),
            Color32::from_rgb(0x14, 0x10, 0x0C),
        ),
        DrawbarColorKind::Black => (
            Color32::from_rgb(0x1E, 0x20, 0x24),
            Color32::from_rgb(0x48, 0x4D, 0x54),
            Color32::from_rgb(0x0C, 0x0E, 0x10),
        ),
    };

    // Cast drop shadow below cap
    p.rect_filled(
        cap_rect.translate(vec2(1.5, 3.5)),
        3.0,
        Color32::from_black_alpha(175),
    );

    // Upper grip section
    let upper_rect = Rect::from_min_max(
        cap_rect.min,
        pos2(cap_rect.right(), cap_rect.top() + cap_h * 0.55),
    );
    p.rect_filled(upper_rect, 2.5, body_col);

    // Lower stepped nose section (slender forward projection)
    let lower_rect = Rect::from_min_max(
        pos2(cap_rect.left() + 2.0, cap_rect.top() + cap_h * 0.50),
        pos2(cap_rect.right() - 2.0, cap_rect.bottom()),
    );
    p.rect_filled(lower_rect, 2.5, body_col.gamma_multiply(0.92));

    // Top rim highlight
    p.line_segment(
        [
            pos2(upper_rect.left() + 2.0, upper_rect.top() + 1.0),
            pos2(upper_rect.right() - 2.0, upper_rect.top() + 1.0),
        ],
        Stroke::new(1.2_f32, highlight_col),
    );
    // Bottom rim shadow
    p.line_segment(
        [
            pos2(lower_rect.left() + 1.0, lower_rect.bottom() - 1.0),
            pos2(lower_rect.right() - 1.0, lower_rect.bottom() - 1.0),
        ],
        Stroke::new(1.2_f32, shadow_col),
    );

    // Vertical center groove
    p.line_segment(
        [
            pos2(cap_rect.center().x, cap_rect.top() + 3.0),
            pos2(cap_rect.center().x, cap_rect.bottom() - 3.0),
        ],
        Stroke::new(1.5_f32, shadow_col),
    );

    changed
}

/// Compatibility alias for `drawbar_3d`.
pub fn drawbar(ui: &mut Ui, rect: Rect, id: &str, value: &mut u8, is_white: bool) -> bool {
    let kind = if is_white {
        DrawbarColorKind::White
    } else {
        DrawbarColorKind::Black
    };
    drawbar_3d(ui, rect, id, value, "", kind, "")
}

// -----------------------------------------------------------------------------
// 3D OLED Display (Glossy Glass + Cyan Pixel Bloom)
// -----------------------------------------------------------------------------

/// Authentic Nord 128x64 OLED graphic display with beveled acrylic glass and pixel bloom.
pub fn oled_3d(p: &Painter, rect: Rect, id_hdr: &str, title: &str, lines: &[String]) {
    // 1. Recessed bezel cutout
    p.rect_filled(rect.expand(2.5), 3.0, Color32::from_rgb(0x06, 0x08, 0x0A));
    p.rect_stroke(
        rect.expand(2.5),
        3.0,
        Stroke::new(1.2_f32, Color32::from_rgb(0x2A, 0x30, 0x38)),
        StrokeKind::Inside,
    );

    // 2. Deep obsidian OLED screen glass
    p.rect_filled(rect, 2.0, OLED_BG);

    // 3. Subtle glass glare reflection (top-left sheen)
    p.line_segment(
        [
            pos2(rect.left() + 2.0, rect.top() + 1.5),
            pos2(rect.right() - 2.0, rect.top() + 1.5),
        ],
        Stroke::new(1.0_f32, Color32::from_white_alpha(35)),
    );

    // 4. Header title line with cyan glow
    let text_cyan = OLED_CYAN;
    let glow_cyan = text_cyan.gamma_multiply(0.35);

    let head_pos = pos2(rect.left() + 8.0, rect.top() + 6.0);
    p.text(
        head_pos + vec2(0.5, 0.5),
        Align2::LEFT_TOP,
        id_hdr,
        FontId::monospace(10.5),
        glow_cyan,
    );
    p.text(
        head_pos,
        Align2::LEFT_TOP,
        id_hdr,
        FontId::monospace(10.5),
        text_cyan,
    );

    // Title / program name
    p.text(
        pos2(rect.left() + 8.0, rect.top() + 20.0),
        Align2::LEFT_TOP,
        title,
        FontId::proportional(12.0),
        Color32::WHITE,
    );

    // Divider rule
    p.line_segment(
        [
            pos2(rect.left() + 6.0, rect.top() + 34.0),
            pos2(rect.right() - 6.0, rect.top() + 34.0),
        ],
        Stroke::new(1.0_f32, text_cyan.gamma_multiply(0.4)),
    );

    // 5. Text parameter lines
    let mut y = rect.top() + 38.0;
    for line in lines {
        p.text(
            pos2(rect.left() + 8.0, y),
            Align2::LEFT_TOP,
            line,
            FontId::monospace(9.5),
            text_cyan,
        );
        y += 13.0;
    }
}

/// Compatibility alias for `oled_3d`.
pub fn oled_display(p: &Painter, rect: Rect, id_hdr: &str, title: &str, lines: &[String]) {
    oled_3d(p, rect, id_hdr, title, lines);
}

/// Compatibility alias for `oled_3d`.
pub fn oled(p: &Painter, rect: Rect, id_hdr: &str, title: &str, lines: &[String]) {
    oled_3d(p, rect, id_hdr, title, lines);
}

// -----------------------------------------------------------------------------
// 3D Triangle Matrix Selector (Organ Model, Vib/Chorus, Piano Select)
// -----------------------------------------------------------------------------

/// Authentic Nord two-column LED matrix selector with red directional LED triangles and cycle button.
pub fn triangle_matrix_selector(
    ui: &mut Ui,
    rect: Rect,
    id: &str,
    col_left: &[&str],
    col_right: &[&str],
    active_idx: usize,
) -> Option<usize> {
    let mut selected = None;
    let total_items = col_left.len() + col_right.len();
    let rows = col_left.len().max(col_right.len());

    let p = ui.painter();

    // 1. Recessed Group Frame Socket
    p.rect_filled(rect, 4.0, Color32::from_rgb(0x18, 0x1E, 0x24));
    p.rect_stroke(
        rect,
        4.0,
        Stroke::new(1.0_f32, Color32::from_rgb(0x32, 0x3D, 0x47)),
        StrokeKind::Inside,
    );

    // Matrix content area (upper portion)
    let matrix_h = rect.height() - 26.0;
    let row_h = matrix_h / rows.max(1) as f32;
    let col_w = (rect.width() - 8.0) * 0.5;

    // Left column items: 0..col_left.len()
    for (r, &name) in col_left.iter().enumerate() {
        let item_idx = r;
        let is_active = item_idx == active_idx;
        let cell_rect = Rect::from_min_size(
            pos2(rect.left() + 4.0, rect.top() + 4.0 + (r as f32) * row_h),
            vec2(col_w, row_h),
        );

        let cell_resp = ui.interact(
            cell_rect,
            ui.id().with((id, "cell", item_idx)),
            Sense::click(),
        );
        if cell_resp.clicked() {
            selected = Some(item_idx);
        }

        // Red triangle indicator on the right side of the left column
        let tri_center = pos2(cell_rect.right() - 6.0, cell_rect.center().y);
        let tri_color = if is_active { LED_RED } else { LED_RED_DIM };
        // Draw right-pointing triangle ▶
        let tri_pts = vec![
            pos2(tri_center.x - 3.0, tri_center.y - 3.5),
            pos2(tri_center.x + 3.0, tri_center.y),
            pos2(tri_center.x - 3.0, tri_center.y + 3.5),
        ];
        p.add(egui::Shape::convex_polygon(
            tri_pts,
            tri_color,
            Stroke::NONE,
        ));
        if is_active {
            p.circle_filled(tri_center, 6.0, LED_RED.gamma_multiply(0.25));
        }

        // Text label
        label_3d(
            p,
            pos2(cell_rect.left() + 2.0, cell_rect.center().y),
            name,
            8.5,
            if is_active {
                Color32::WHITE
            } else {
                Color32::from_gray(160)
            },
            Align2::LEFT_CENTER,
        );
    }

    // Right column items: col_left.len()..total_items
    for (r, &name) in col_right.iter().enumerate() {
        let item_idx = col_left.len() + r;
        let is_active = item_idx == active_idx;
        let cell_rect = Rect::from_min_size(
            pos2(
                rect.left() + 4.0 + col_w,
                rect.top() + 4.0 + (r as f32) * row_h,
            ),
            vec2(col_w, row_h),
        );

        let cell_resp = ui.interact(
            cell_rect,
            ui.id().with((id, "cell", item_idx)),
            Sense::click(),
        );
        if cell_resp.clicked() {
            selected = Some(item_idx);
        }

        // Red triangle indicator on the left side of the right column
        let tri_center = pos2(cell_rect.left() + 6.0, cell_rect.center().y);
        let tri_color = if is_active { LED_RED } else { LED_RED_DIM };
        // Draw left-pointing triangle ◀
        let tri_pts = vec![
            pos2(tri_center.x + 3.0, tri_center.y - 3.5),
            pos2(tri_center.x - 3.0, tri_center.y),
            pos2(tri_center.x + 3.0, tri_center.y + 3.5),
        ];
        p.add(egui::Shape::convex_polygon(
            tri_pts,
            tri_color,
            Stroke::NONE,
        ));
        if is_active {
            p.circle_filled(tri_center, 6.0, LED_RED.gamma_multiply(0.25));
        }

        // Text label
        label_3d(
            p,
            pos2(cell_rect.left() + 14.0, cell_rect.center().y),
            name,
            8.5,
            if is_active {
                Color32::WHITE
            } else {
                Color32::from_gray(160)
            },
            Align2::LEFT_CENTER,
        );
    }

    // Step cycle button at the bottom of the matrix box
    let btn_rect = Rect::from_min_size(
        pos2(rect.center().x - 24.0, rect.bottom() - 20.0),
        vec2(48.0, 16.0),
    );
    if button_3d(ui, btn_rect, &format!("{id}_btn"), "", false, false) {
        selected = Some((active_idx + 1) % total_items.max(1));
    }

    selected
}

// -----------------------------------------------------------------------------
// 3D Pitch Stick and Modulation Wheel
// -----------------------------------------------------------------------------

/// Beech wood spring-loaded pitch stick and knurled steel modulation wheel.
pub fn pitch_and_mod_wheels_3d(
    ui: &mut Ui,
    rect_pitch: Rect,
    rect_mod: Rect,
    pitch: &mut f32,
    mod_wheel: &mut f32,
) -> bool {
    let mut changed = false;
    let p = ui.painter();

    // 1. Pitch Stick (Spring-loaded beech paddle)
    let resp_pitch = ui.interact(
        rect_pitch,
        ui.id().with("pitch_stick_3d"),
        Sense::click_and_drag(),
    );
    if resp_pitch.dragged() {
        let d = -resp_pitch.drag_delta().x * 0.02;
        *pitch = (*pitch + d).clamp(-1.0, 1.0);
        changed = true;
    } else if !resp_pitch.is_pointer_button_down_on() && pitch.abs() > 1e-4 {
        *pitch *= 0.45;
        if pitch.abs() < 1e-3 {
            *pitch = 0.0;
        }
        changed = true;
    }

    // Pitch stick rubber bezel cup
    p.rect_filled(rect_pitch, 4.0, Color32::from_rgb(0x0C, 0x0E, 0x10));
    p.rect_stroke(
        rect_pitch,
        4.0,
        Stroke::new(1.2_f32, Color32::from_rgb(0x28, 0x30, 0x38)),
        StrokeKind::Inside,
    );

    // Beech wood paddle with wood grain gradient
    let stick_w = 16.0;
    let stick_h = rect_pitch.height() * 0.74;
    let offset_x = *pitch * (rect_pitch.width() * 0.35);
    let stick_rect = Rect::from_center_size(
        pos2(rect_pitch.center().x + offset_x, rect_pitch.center().y),
        vec2(stick_w, stick_h),
    );

    // Wood shadow
    p.rect_filled(
        stick_rect.translate(vec2(1.0, 2.0)),
        3.0,
        Color32::from_black_alpha(160),
    );
    // Beech wood base
    p.rect_filled(stick_rect, 3.0, Color32::from_rgb(0xB8, 0x86, 0x4E));
    // Wood grain highlight
    p.rect_filled(
        Rect::from_min_size(stick_rect.min, vec2(stick_w, stick_h * 0.4)),
        2.5,
        Color32::from_rgb(0xD4, 0xA2, 0x6A),
    );
    p.rect_stroke(
        stick_rect,
        3.0,
        Stroke::new(1.0_f32, Color32::from_rgb(0x7A, 0x52, 0x2A)),
        StrokeKind::Inside,
    );

    // 2. Mod Wheel (Grooved Steel Knurled Cylinder)
    let resp_mod = ui.interact(
        rect_mod,
        ui.id().with("mod_wheel_3d"),
        Sense::click_and_drag(),
    );
    if resp_mod.dragged() {
        let d = -resp_mod.drag_delta().y * 0.01;
        *mod_wheel = (*mod_wheel + d).clamp(0.0, 1.0);
        changed = true;
    }

    // Wheel slot
    p.rect_filled(rect_mod, 4.0, Color32::from_rgb(0x0C, 0x0E, 0x10));
    p.rect_stroke(
        rect_mod,
        4.0,
        Stroke::new(1.2_f32, Color32::from_rgb(0x28, 0x30, 0x38)),
        StrokeKind::Inside,
    );

    let travel = rect_mod.height() * 0.68;
    let mod_y = rect_mod.bottom() - 12.0 - *mod_wheel * travel;
    let mod_cap = Rect::from_center_size(
        pos2(rect_mod.center().x, mod_y),
        vec2(rect_mod.width() * 0.80, 20.0),
    );

    // Steel wheel drop shadow
    p.rect_filled(
        mod_cap.translate(vec2(0.0, 2.5)),
        2.0,
        Color32::from_black_alpha(180),
    );
    // Steel body
    p.rect_filled(mod_cap, 2.5, Color32::from_rgb(0xC0, 0xC6, 0xCE));

    // Milled horizontal traction grooves
    for g in 0..4 {
        let gy = mod_cap.top() + 3.0 + (g as f32) * 4.2;
        p.line_segment(
            [
                pos2(mod_cap.left() + 2.0, gy),
                pos2(mod_cap.right() - 2.0, gy),
            ],
            Stroke::new(1.0_f32, Color32::from_rgb(0x70, 0x76, 0x80)),
        );
    }
    p.rect_stroke(
        mod_cap,
        2.5,
        Stroke::new(1.0_f32, Color32::from_white_alpha(200)),
        StrokeKind::Inside,
    );

    changed
}

/// Compatibility alias for `pitch_and_mod_wheels_3d`.
pub fn pitch_and_mod_wheels(
    ui: &mut Ui,
    rect_pitch: Rect,
    rect_mod: Rect,
    pitch: &mut f32,
    mod_wheel: &mut f32,
) -> bool {
    pitch_and_mod_wheels_3d(ui, rect_pitch, rect_mod, pitch, mod_wheel)
}

// -----------------------------------------------------------------------------
// Utilities
// -----------------------------------------------------------------------------

/// Cycles to the next element of `items` after `cur` (wraps).
pub fn cycle<T: Copy + PartialEq>(items: &[T], cur: T) -> T {
    let i = items.iter().position(|x| *x == cur).unwrap_or(0);
    items[(i + 1) % items.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_widget_color_constants() {
        assert_eq!(LED_RED.r(), 0xFF);
        assert_eq!(LED_GREEN.g(), 0xE6);
    }

    #[test]
    fn test_stroke_float_type_regression() {
        let s1 = Stroke::new(1.0_f32, Color32::BLACK);
        let s2 = Stroke::new(1.5_f32, Color32::WHITE);
        assert_eq!(s1.width, 1.0_f32);
        assert_eq!(s2.width, 1.5_f32);
    }

    #[test]
    fn test_cycle_wraps() {
        assert_eq!(cycle(&[1, 2, 3], 3), 1);
        assert_eq!(cycle(&[1, 2, 3], 1), 2);
    }

    #[test]
    fn test_drawbar_colors() {
        assert_eq!(DrawbarColorKind::White, DrawbarColorKind::White);
        assert_ne!(DrawbarColorKind::White, DrawbarColorKind::Black);
    }
}
