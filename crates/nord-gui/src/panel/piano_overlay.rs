//! LIST and INFO overlays of the Piano Select dial (manual p.25), drawn under the Piano section.

use super::bay_piano_select::TYPE_NAMES;
use super::layout_px::to_px;
use super::piano_logic as logic;
use super::piano_widgets as w;
use super::prelude::*;
use super::ui_state::PianoOverlay;
use nord_panel::{section_rect, Section};

/// File stem of a library entry name.
fn stem(name: &str) -> &str {
    std::path::Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(name)
}

/// LIST / INFO overlay under the bay (manual p.25).
pub(super) fn draw_overlay(ui: &mut Ui, p: &Painter, o: egui::Pos2, state: &mut Stage4PanelState) {
    let mode = state.piano_ui.overlay;
    if mode == PianoOverlay::None {
        return;
    }
    let sec = to_px(section_rect(Section::Piano), o);
    let rect = Rect::from_min_size(
        pos2(sec.right() - 190.0, sec.bottom() + 4.0),
        vec2(190.0, 84.0),
    );
    let resp = ui.interact(rect, ui.id().with("piano_overlay"), Sense::click());
    p.rect_filled(rect, 4.0, OLED_BG);
    p.rect_stroke(
        rect,
        4.0,
        Stroke::new(1.0_f32, Color32::from_gray(90)),
        StrokeKind::Inside,
    );
    let cat = TYPE_NAMES[logic::category_index(state)];
    let n = state.piano_ui.models.len();
    let idx = state.piano_ui.model_idx.min(n.saturating_sub(1));
    let cur = state.piano_ui.models.get(idx).map(|m| stem(m).to_string());
    let x = rect.left() + 6.0;
    let mut y = rect.top() + 9.0;
    if mode == PianoOverlay::List {
        let head = format!("Piano {cat}   {}/{}", if n == 0 { 0 } else { idx + 1 }, n);
        w::small_text(p, pos2(x, y), &head, OLED_FG, Align2::LEFT_CENTER);
        if n == 0 {
            w::small_text(
                p,
                pos2(x, y + 22.0),
                "No piano library loaded",
                Color32::from_gray(150),
                Align2::LEFT_CENTER,
            );
        }
        let first = idx.saturating_sub(2);
        for (row, i) in (first..n.min(first + 5)).enumerate() {
            let ry = rect.top() + 22.0 + row as f32 * 12.0;
            let band = Rect::from_min_size(
                pos2(rect.left() + 3.0, ry - 5.5),
                vec2(rect.width() - 6.0, 11.0),
            );
            if i == idx {
                p.rect_stroke(band, 2.0, Stroke::new(1.0_f32, OLED_FG), StrokeKind::Inside);
            }
            let name: String = stem(&state.piano_ui.models[i]).chars().take(30).collect();
            w::small_text(p, pos2(x + 2.0, ry), &name, OLED_FG, Align2::LEFT_CENTER);
        }
        if resp.clicked() {
            if let Some(pos) = resp.interact_pointer_pos() {
                let row = ((pos.y - (rect.top() + 16.5)) / 12.0).floor() as isize;
                let first = idx.saturating_sub(2) as isize;
                let pick = first + row;
                if row >= 0
                    && pick >= 0
                    && (pick as usize) < n
                    && (pick as usize) < first as usize + 5
                {
                    state.piano_ui.model_idx = pick as usize;
                    state.piano_ui.model_request =
                        Some(state.piano_ui.models[pick as usize].clone());
                }
            }
        }
        let scroll = if resp.hovered() {
            ui.input(|i| i.raw_scroll_delta.y)
        } else {
            0.0
        };
        if scroll != 0.0 {
            logic::model_step(state, scroll < 0.0);
        }
    } else {
        w::small_text(p, pos2(x, y), "Piano info", OLED_FG, Align2::LEFT_CENTER);
        y += 14.0;
        let name = state
            .loaded_piano_name
            .clone()
            .or(cur)
            .unwrap_or_else(|| "(no model loaded)".to_string());
        let lines = [
            format!("Model: {}", name.chars().take(28).collect::<String>()),
            format!("Type: {cat}   {}/{}", if n == 0 { 0 } else { idx + 1 }, n),
            "Version, size: not exposed by the library".to_string(),
        ];
        for line in lines {
            w::small_text(p, pos2(x, y), &line, OLED_FG, Align2::LEFT_CENTER);
            y += 12.0;
        }
    }
    let close = Rect::from_min_size(
        pos2(rect.right() - 14.0, rect.top() + 2.0),
        vec2(12.0, 12.0),
    );
    let cr = ui.interact(close, ui.id().with("piano_overlay_close"), Sense::click());
    w::small_text(p, close.center(), "x", OLED_FG, Align2::CENTER_CENTER);
    if cr.clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        state.piano_ui.overlay = PianoOverlay::None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stem_strips_directory_and_extension() {
        assert_eq!(stem("Piano/Grand/White Grand XL.npno"), "White Grand XL");
        assert_eq!(stem("plain"), "plain");
    }
}
