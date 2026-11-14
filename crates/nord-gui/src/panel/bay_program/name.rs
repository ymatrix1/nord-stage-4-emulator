//! Program naming for STORE AS (manual p.42): ABC / Cat / Ins / Del soft buttons and the dial.

/// Longest program name.
///
/// ASSUMED: 16 characters, the usual Clavia name length; the manual shows no limit.
pub const MAX_NAME: usize = 16;

/// Characters the ABC soft button offers, in dial order.
///
/// VERIFIED (order up to `-`): STORE PROGRAM AS strip in the DSoundman piano-pad video
/// (`hR31DQrxfT4`, 5:45-6:00) reads blank, `a..z`, `A..Z`, `0..9`, `-` left to right.
/// ASSUMED: the tail `_.!&` after `-` (the strip was cut off at the display edge).
pub const CHARSET: &str = " abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_.!&";

/// Program categories offered by the Cat soft button.
///
/// ASSUMED: names modelled on the Nord Sound Manager categories; the manual does not list them.
pub const CATEGORIES: [&str; 16] = [
    "None",
    "Acoustic",
    "Bass",
    "Clav",
    "Digital",
    "E.Piano",
    "FX",
    "Guitar",
    "Layers",
    "Organ",
    "Pad",
    "Piano",
    "Strings",
    "Synth Lead",
    "Synth Pad",
    "Vocal",
];

/// Cursor-based name editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameEditor {
    chars: Vec<char>,
    cursor: usize,
    /// ABC soft button is held: the dial picks the character at the cursor.
    pub abc_held: bool,
    /// Cat soft button is held: the dial picks the category.
    pub cat_held: bool,
    /// Index into [`CATEGORIES`].
    pub category: usize,
}

impl Default for NameEditor {
    fn default() -> Self {
        Self::new("")
    }
}

impl NameEditor {
    /// Editor for `name` (cut to [`MAX_NAME`]); the cursor starts on the first character.
    pub fn new(name: &str) -> Self {
        Self {
            chars: name.chars().take(MAX_NAME).collect(),
            cursor: 0,
            abc_held: false,
            cat_held: false,
            category: 0,
        }
    }

    /// The name without trailing spaces.
    pub fn name(&self) -> String {
        self.chars.iter().collect::<String>().trim_end().to_string()
    }

    /// Cursor position (`0..MAX_NAME`).
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// The raw text including blanks (for drawing).
    pub fn display(&self) -> String {
        self.chars.iter().collect()
    }

    /// ABC pressed: the cursor position gets a blank to edit when it lies past the end.
    pub fn press_abc(&mut self) {
        self.abc_held = true;
        while self.chars.len() <= self.cursor && self.chars.len() < MAX_NAME {
            self.chars.push(' ');
        }
    }

    /// ABC released: the cursor moves to the next position.
    pub fn release_abc(&mut self) {
        if self.abc_held {
            self.abc_held = false;
            self.cursor = (self.cursor + 1).min(MAX_NAME - 1);
        }
    }

    /// Turn the dial by `delta` steps: character (ABC held), category (Cat held) or cursor.
    pub fn dial(&mut self, delta: i32) {
        if self.abc_held {
            let set: Vec<char> = CHARSET.chars().collect();
            if let Some(c) = self.chars.get_mut(self.cursor) {
                let i = set.iter().position(|x| x == c).unwrap_or(0) as i32;
                *c = set[(i + delta).rem_euclid(set.len() as i32) as usize];
            }
        } else if self.cat_held {
            let n = CATEGORIES.len() as i32;
            self.category = (self.category as i32 + delta).rem_euclid(n) as usize;
        } else {
            self.cursor = (self.cursor as i32 + delta).clamp(0, MAX_NAME as i32 - 1) as usize;
        }
    }

    /// Ins: insert a blank at the cursor.
    pub fn insert(&mut self) {
        if self.cursor <= self.chars.len() {
            self.chars.insert(self.cursor, ' ');
            self.chars.truncate(MAX_NAME);
        }
    }

    /// Del: delete the character at the cursor.
    pub fn delete(&mut self) {
        if self.cursor < self.chars.len() {
            self.chars.remove(self.cursor);
        }
    }

    /// Category label.
    pub fn category_label(&self) -> &'static str {
        CATEGORIES[self.category.min(CATEGORIES.len() - 1)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abc_hold_edits_the_character_and_release_advances() {
        let mut e = NameEditor::new("Rusty");
        e.press_abc();
        e.dial(1); // R -> S
        e.release_abc();
        assert_eq!(e.name(), "Susty");
        assert_eq!(e.cursor(), 1);
        assert!(!e.abc_held);
    }

    #[test]
    fn dial_without_a_button_moves_the_cursor_within_bounds() {
        let mut e = NameEditor::new("ab");
        e.dial(-5);
        assert_eq!(e.cursor(), 0);
        e.dial(100);
        assert_eq!(e.cursor(), MAX_NAME - 1);
    }

    #[test]
    fn insert_and_delete_shift_the_text() {
        let mut e = NameEditor::new("abc");
        e.dial(1);
        e.insert();
        assert_eq!(e.display(), "a bc");
        e.delete();
        assert_eq!(e.display(), "abc");
        e.dial(10);
        e.delete(); // past the end: nothing to delete
        assert_eq!(e.display(), "abc");
    }

    #[test]
    fn cat_hold_selects_a_category_and_wraps() {
        let mut e = NameEditor::new("x");
        e.cat_held = true;
        e.dial(-1);
        assert_eq!(e.category, CATEGORIES.len() - 1);
        e.dial(1);
        assert_eq!(e.category_label(), "None");
    }

    #[test]
    fn name_is_cut_to_the_maximum_and_trailing_blanks_are_trimmed() {
        let e = NameEditor::new("0123456789ABCDEFGHIJ");
        assert_eq!(e.display().chars().count(), MAX_NAME);
        let mut e = NameEditor::new("hi");
        e.dial(2);
        e.press_abc(); // pads a blank past the end
        e.release_abc();
        assert_eq!(e.name(), "hi");
    }
}
