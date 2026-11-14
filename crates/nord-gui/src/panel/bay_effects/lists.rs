//! Printed variation lists of the Layer Effects selectors: a two-column matrix of mini LEDs above
//! the selector button with the names printed beside it, as the panel prints them (photo
//! `NS4-Panel-effects`: RM / TREM / A-PAN beside A-WAH / WAH / PUMP, ROOM / BOOTH / SPRING beside
//! STAGE / HALL / CATH, ...). Names come from the manual (p.49-53); each LED keeps its tooltip.

use super::ctx::{mini_led, Fx, CAPTION};
use crate::style::*;
use crate::widgets::label;
use egui::{pos2, vec2, Align2, Color32, Pos2, Rect};

/// Pitch (px) of the list rows (the 7.5 px minimum font plus a 2 px gap, audit rule `V`).
pub(super) const ROW: f32 = 9.5;
/// Distance (px) of a matrix LED column from the middle of the selector.
pub(super) const COL_DX: f32 = 3.5;
/// Space (px) between an LED and the start of its name.
const NAME_GAP: f32 = 4.5;
/// Distance (px) from the top of the selector to the centre of the lowest row.
const BOTTOM_ROW: f32 = 6.5;

/// Which side of the LED matrix a name is printed on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Side {
    Left,
    Right,
}

/// One printed entry: its LED centre, the side of its name and the index into the LED data.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Cell {
    pub led: Pos2,
    pub side: Side,
    pub index: usize,
}

/// Cells of a list above `rect`: `left` and `right` hold the LED indices of each column from top
/// to bottom; both columns end on the same bottom row. A list with only a `right` column is a
/// single column with its LED at the left edge of the selector and the names to its right.
pub(super) fn cells(rect: Rect, left: &[usize], right: &[usize]) -> Vec<Cell> {
    let rows = left.len().max(right.len());
    let y = |col_len: usize, i: usize| {
        let row = rows - col_len + i;
        rect.top() - BOTTOM_ROW - ROW * (rows - 1 - row) as f32
    };
    let both = !left.is_empty() && !right.is_empty();
    let (lx, rx) = if both {
        (rect.center().x - COL_DX, rect.center().x + COL_DX)
    } else {
        (rect.center().x, rect.left() + 3.0)
    };
    let mut out = Vec::new();
    for (i, &index) in left.iter().enumerate() {
        out.push(Cell {
            led: pos2(lx, y(left.len(), i)),
            side: Side::Left,
            index,
        });
    }
    for (i, &index) in right.iter().enumerate() {
        out.push(Cell {
            led: pos2(rx, y(right.len(), i)),
            side: Side::Right,
            index,
        });
    }
    out
}

impl Fx<'_> {
    /// Variation list of selector `id` above `rect`: `leds` is `(name, lit, missing)` per LED in
    /// engine order, `left` / `right` the LED indices printed in each column (top to bottom). An
    /// LED whose type the engine lacks stays dark amber-grey and its tooltip names what is missing.
    pub fn led_list(
        &mut self,
        id: &'static str,
        rect: Rect,
        leds: &[(&str, bool, &str)],
        left: &[usize],
        right: &[usize],
    ) {
        for cell in cells(rect, left, right) {
            let Some(&(name, lit, missing)) = leds.get(cell.index) else {
                continue;
            };
            let supported = missing.is_empty();
            let (col, off) = if supported {
                (LED_RED, LED_RED_DIM)
            } else {
                (LED_AMBER, Color32::from_gray(52))
            };
            mini_led(self.p, cell.led, lit && supported, col, off);
            let (x, al) = match cell.side {
                Side::Left => (cell.led.x - NAME_GAP, Align2::RIGHT_CENTER),
                Side::Right => (cell.led.x + NAME_GAP, Align2::LEFT_CENTER),
            };
            let ink = if !supported {
                dim_legend(CAPTION)
            } else if lit {
                Color32::WHITE
            } else {
                CAPTION
            };
            label(self.p, pos2(x, cell.led.y), name, 6.5, ink, al);
            let hit = Rect::from_center_size(cell.led, vec2(6.0, ROW));
            let text = if supported {
                name.to_string()
            } else {
                format!("{name}: not available, {missing}")
            };
            self.hover(hit, &format!("{id}{}", cell.index), &text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selector() -> Rect {
        Rect::from_min_size(pos2(100.0, 50.0), vec2(40.0, 22.0))
    }

    #[test]
    fn two_columns_put_the_leds_either_side_of_the_selector_middle() {
        let c = cells(selector(), &[2, 1, 0], &[3, 4, 5]);
        assert_eq!(c.len(), 6);
        assert!(c[..3]
            .iter()
            .all(|k| k.side == Side::Left && k.led.x < 120.0));
        assert!(c[3..]
            .iter()
            .all(|k| k.side == Side::Right && k.led.x > 120.0));
        // Top to bottom, one row pitch apart, the last row just above the selector.
        assert!((c[1].led.y - c[0].led.y - ROW).abs() < 1e-4);
        assert!((c[2].led.y - (50.0 - BOTTOM_ROW)).abs() < 1e-4);
        assert_eq!(c[0].index, 2);
    }

    #[test]
    fn a_shorter_column_is_bottom_aligned() {
        let c = cells(selector(), &[3, 2, 1], &[4, 5]);
        let bottom = |side| {
            c.iter()
                .filter(|k| k.side == side)
                .map(|k| k.led.y)
                .fold(f32::MIN, f32::max)
        };
        assert_eq!(bottom(Side::Left), bottom(Side::Right));
        let right_top = c.iter().find(|k| k.index == 4).unwrap().led.y;
        assert!((right_top - (50.0 - BOTTOM_ROW - ROW)).abs() < 1e-4);
    }

    #[test]
    fn a_right_only_list_is_a_single_column_at_the_selector_edge() {
        let c = cells(selector(), &[], &[0, 1]);
        assert!(c.iter().all(|k| (k.led.x - 103.0).abs() < 1e-4));
        assert_eq!(c.len(), 2);
    }
}
