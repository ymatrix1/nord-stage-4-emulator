//! Measured pixel rows of the real Program OLED (128 x 64 grid) and free-placed text lines.

/// Row geometry of the real Program home screen on its 128 x 64 pixel grid.
///
/// VERIFIED (frames sent by the ARM emulator, `tools/arm_screens.py`, measured in
/// `docs/research/arm-oled-render.md` section 9): the location title and the name line are both
/// 13 px proportional text at the left edge, text boxes at y = 3 and y = 18 (15 px pitch); a 1 px
/// rule at y = 33 spans the full width; the section rows below use the 8 px font from y = 36 every
/// 10 px. The sizes here are those pixels divided by 64, so they scale with the rectangle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grid {
    /// Top of the title text box.
    pub title_y: f32,
    /// Top of the name line text box.
    pub name_y: f32,
    /// The full-width rule between the header and the rows.
    pub rule_y: f32,
    /// Top of the first row.
    pub rows_y: f32,
    /// Distance between rows.
    pub row_pitch: f32,
}

/// Real pixel rows of [`Grid`] for a display `height` (in the unit of the caller).
pub fn grid(height: f32) -> Grid {
    let k = height / 64.0;
    Grid {
        title_y: 3.0 * k,
        name_y: 18.0 * k,
        rule_y: 33.0 * k,
        rows_y: 36.0 * k,
        row_pitch: 10.0 * k,
    }
}

/// Rows of the other layouts on the 128 x 64 grid.
///
/// VERIFIED (frames of `tools/arm_screens.py`, `docs/research/arm-oled-render.md` sections 9 and
/// 12): title-bar screens (menus, Preset, Layer Init, Morph assign, Section Edit) light rows 0 to
/// 10 and print the title unlit at x = 1, the page counter right-aligned; the label starts at
/// y = 15 and the value at y = 26; soft buttons are outlined boxes in rows 53 to 63 and the active
/// one is lit; parameter popups (Transpose, Master Clock Tempo, Nord `Nothing to Paste`) put label
/// and value at y = 45 and y = 55 under the home header.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bar {
    /// Height of the inverted title bar.
    pub bar_h: f32,
    /// Top of the label under the bar.
    pub label_y: f32,
    /// Top of the first body row (the value).
    pub value_y: f32,
    /// Top of the soft buttons.
    pub soft_y: f32,
    /// Tops of the popup label and value.
    pub popup_y: [f32; 2],
    /// Top of the first row of a program list (research note s16.5); rows are 10 px apart and the
    /// lit row is a block one pixel above its text, 10 px high.
    pub list_y: f32,
}

/// Real pixel rows of [`Bar`] for a display `height`.
pub fn bar(height: f32) -> Bar {
    let k = height / 64.0;
    Bar {
        bar_h: 11.0 * k,
        label_y: 15.0 * k,
        value_y: 26.0 * k,
        soft_y: 53.0 * k,
        popup_y: [45.0 * k, 55.0 * k],
        list_y: 13.0 * k,
    }
}

/// One text line placed freely under a title bar, for screens whose rows do not follow the
/// label / value grid (the Store screens).
///
/// `y` is the top of the glyph ink in pixels of the 128 x 64 grid (the same measure as
/// [`Bar::label_y`]); `big` selects the 13 px font, otherwise the 8 px font; `x` is the left edge
/// in pixels; `cursor` inverts that character cell (the name entry cursor).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Line {
    pub y: u8,
    pub x: u8,
    pub big: bool,
    pub text: String,
    pub cursor: Option<usize>,
}

impl Line {
    /// A line in the 13 px font at the left edge.
    pub fn big(y: u8, text: impl Into<String>) -> Self {
        Self {
            y,
            big: true,
            text: text.into(),
            ..Self::default()
        }
    }

    /// A line in the 8 px font at x pixels from the left edge.
    pub fn small(y: u8, x: u8, text: impl Into<String>) -> Self {
        Self {
            y,
            x,
            text: text.into(),
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_is_the_measured_pixel_rows_of_the_real_home_screen() {
        let g = grid(64.0);
        assert_eq!(
            (g.title_y, g.name_y, g.rule_y, g.rows_y, g.row_pitch),
            (3.0, 18.0, 33.0, 36.0, 10.0)
        );
        // The title and the name line are one 15 px pitch apart; the rule is two pitches down.
        assert_eq!(g.name_y - g.title_y, 15.0);
        assert_eq!(g.rule_y - g.title_y, 30.0);
        // It scales with the display height.
        let half = grid(32.0);
        assert_eq!((half.name_y, half.rule_y), (9.0, 16.5));
    }

    #[test]
    fn bar_rows_are_the_measured_pixel_rows_of_the_real_title_bar_screens() {
        let b = bar(64.0);
        assert_eq!(
            (b.bar_h, b.label_y, b.value_y, b.soft_y, b.popup_y, b.list_y),
            (11.0, 15.0, 26.0, 53.0, [45.0, 55.0], 13.0)
        );
        let half = bar(32.0);
        assert_eq!(
            (half.bar_h, half.soft_y, half.popup_y[1]),
            (5.5, 26.5, 27.5)
        );
    }

    #[test]
    fn line_constructors_pick_the_font_and_the_left_edge() {
        let big = Line::big(18, "A:11");
        assert!(big.big && big.x == 0 && big.cursor.is_none());
        let small = Line::small(39, 2, "Cat: <None>");
        assert!(!small.big && small.x == 2 && small.y == 39);
    }
}
