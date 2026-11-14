//! State machine of the menu screens (System, Sound, Organize, Aux KB, Output, Pedal, MIDI,
//! Extern; manual OS v1.6x p.57-64).
//!
//! `MenuUi` owns the current menu, page and focused item plus the value of every item of the
//! model (`nord_panel::menus`). It is toolkit-free and returns what changed; `ProgramUi`
//! (`bay_program/menu_actions.rs`) turns an edit of a `Bound` item into an audio command.
//! Unsupported items never change: they render dimmed with their bean id.

use nord_panel::menus::{
    all_items, pages_of, ItemKind, Menu, MenuItem, MenuPage, Value, ID_MEMORY_PROTECT,
};
use nord_panel::Binding;
use std::collections::BTreeMap;

/// Items the engine has one state for but the model lists on two pages.
const ALIASES: [(&str, &str); 1] = [("pedal.3.right", "pedal.4.right")];

/// What a soft button press on the menu screen did.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SoftResult {
    /// No item under that soft button.
    None,
    /// The item is now focused; the dial edits it.
    Focused,
    /// The item is an action (Reset, Swap, ...): the caller performs it.
    Action(MenuItem),
    /// The "More" soft button scrolled to the next block of items.
    More,
}

/// Items shown at once on a page of at most four items (one per soft button).
const ROWS: usize = 4;
/// Items shown at once on a longer page: the fourth soft button becomes "More".
const ROWS_PAGED: usize = 3;

/// Current menu, page, focus and the values of all items.
#[derive(Debug, Clone)]
pub struct MenuUi {
    menu: Menu,
    pages: Vec<MenuPage>,
    page: usize,
    focus: usize,
    values: BTreeMap<&'static str, Value>,
}

impl Default for MenuUi {
    fn default() -> Self {
        Self {
            menu: Menu::System,
            pages: pages_of(Menu::System),
            page: 0,
            focus: 0,
            values: all_items()
                .into_iter()
                .map(|(_, i)| (i.id, i.default))
                .collect(),
        }
    }
}

impl MenuUi {
    /// Opens `menu` at its first page with the first item focused.
    pub fn open(&mut self, menu: Menu) {
        if self.menu != menu || self.pages.is_empty() {
            self.menu = menu;
            self.pages = pages_of(menu);
        }
        self.page = 0;
        self.focus = 0;
    }

    pub fn menu(&self) -> Menu {
        self.menu
    }

    /// Number of pages of the open menu.
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// 0-based index of the shown page.
    pub fn page_index(&self) -> usize {
        self.page
    }

    /// The shown page.
    pub fn page(&self) -> &MenuPage {
        &self.pages[self.page.min(self.pages.len() - 1)]
    }

    /// Index of the focused item on the shown page.
    pub fn focus(&self) -> usize {
        self.focus.min(self.page().items.len().saturating_sub(1))
    }

    /// The focused item.
    pub fn focused_item(&self) -> &MenuItem {
        &self.page().items[self.focus()]
    }

    /// PAGE button: moves by `delta` pages, stopping at the first and last. The first item of the
    /// new page is focused.
    pub fn step_page(&mut self, delta: i32) {
        let last = self.pages.len().saturating_sub(1) as i32;
        self.page = (self.page as i32 + delta).clamp(0, last) as usize;
        self.focus = 0;
    }

    /// Soft buttons / rows of the shown page: 4 items, or 3 items plus "More" on a page with
    /// more than four items (Output pages 2 and 5).
    pub fn block_size(&self) -> usize {
        if self.page().items.len() > ROWS {
            ROWS_PAGED
        } else {
            ROWS
        }
    }

    /// Whether the shown page needs the "More" soft button.
    pub fn paged(&self) -> bool {
        self.block_size() == ROWS_PAGED
    }

    /// Index of the first item of the block that holds the focus.
    pub fn block_start(&self) -> usize {
        self.focus() / self.block_size() * self.block_size()
    }

    /// The items of the current block.
    pub fn visible(&self) -> &[MenuItem] {
        let items = self.page().items;
        let start = self.block_start();
        &items[start..(start + self.block_size()).min(items.len())]
    }

    /// Soft button `n` (0-based): focus that item, report the action it triggers, or scroll.
    pub fn soft(&mut self, n: usize) -> SoftResult {
        let len = self.page().items.len();
        if self.paged() && n == ROWS - 1 {
            let next = self.block_start() + ROWS_PAGED;
            self.focus = if next >= len { 0 } else { next };
            return SoftResult::More;
        }
        let idx = self.block_start() + n;
        let Some(item) = self.page().items.get(idx).copied() else {
            return SoftResult::None;
        };
        self.focus = idx;
        if item.kind == ItemKind::Action {
            SoftResult::Action(item)
        } else {
            SoftResult::Focused
        }
    }

    /// Value of `item` (its default until edited).
    pub fn value(&self, item: &MenuItem) -> Value {
        self.values.get(item.id).copied().unwrap_or(item.default)
    }

    /// Stores `value` for `id` (and for the item it shares its engine state with).
    pub fn set(&mut self, id: &'static str, value: Value) {
        self.values.insert(id, value);
        for (a, b) in ALIASES {
            if id == a {
                self.values.insert(b, value);
            } else if id == b {
                self.values.insert(a, value);
            }
        }
    }

    /// Dial: steps the focused item by `delta` clicks. Returns the item and its new value when
    /// the value changed; unsupported items and actions never change.
    pub fn dial(&mut self, delta: i32) -> Option<(MenuItem, Value)> {
        let item = *self.focused_item();
        if matches!(item.binding, Binding::Unsupported(_)) {
            return None;
        }
        let old = self.value(&item);
        let new = step_value(&item, old, delta);
        if new == old {
            return None;
        }
        self.set(item.id, new);
        Some((item, new))
    }

    /// Memory Protect (System page 1): `true` blocks STORE to a bank location (manual p.57).
    pub fn memory_protect(&self) -> bool {
        matches!(self.values.get(ID_MEMORY_PROTECT), Some(Value::Bool(true)))
    }

    /// Switches Memory Protect (System page 1) on or off without going through the dial.
    pub fn set_memory_protect(&mut self, on: bool) {
        self.set(ID_MEMORY_PROTECT, Value::Bool(on));
    }

    /// Resets every item to its default. Returns the `Bound` items so the caller can send them.
    /// Memory Protect is kept: a reset must not unlock the memory (manual p.57 lists the reset
    /// separately from the protect switch; ASSUMED).
    pub fn reset_all(&mut self) -> Vec<(MenuItem, Value)> {
        let protect = self.memory_protect();
        let mut out = Vec::new();
        for (_, item) in all_items() {
            if item.kind == ItemKind::Action {
                continue;
            }
            self.values.insert(item.id, item.default);
            if matches!(item.binding, Binding::Bound(_)) {
                out.push((item, item.default));
            }
        }
        self.values.insert(ID_MEMORY_PROTECT, Value::Bool(protect));
        out
    }
}

/// `item` stepped by `delta` clicks from `v`, clamped to its range. A toggle follows the sign.
pub fn step_value(item: &MenuItem, v: Value, delta: i32) -> Value {
    match (item.kind, v) {
        (ItemKind::Toggle, Value::Bool(b)) => Value::Bool(match delta.signum() {
            1 => true,
            -1 => false,
            _ => b,
        }),
        (ItemKind::Enum(c), Value::Index(i)) => {
            Value::Index((i as i32 + delta).clamp(0, c.len() as i32 - 1) as usize)
        }
        (ItemKind::Int { min, max, step }, Value::Int(i)) => {
            Value::Int(i.saturating_add(delta.saturating_mul(step)).clamp(min, max))
        }
        (ItemKind::Float { min, max, step }, Value::Float(f)) => {
            // Round to the step grid so repeated clicks do not drift.
            let n = ((f - min) / step).round() + delta as f32;
            Value::Float((min + n * step).clamp(min, max))
        }
        _ => v,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nord_panel::menus::find_item;

    fn open(m: Menu) -> MenuUi {
        let mut u = MenuUi::default();
        u.open(m);
        u
    }

    #[test]
    fn open_shows_page_one_item_one_and_pages_clamp() {
        let mut u = open(Menu::Sound);
        assert_eq!((u.page_index(), u.page_count()), (0, 11));
        assert_eq!(u.page().items[0].id, "sound.1.program_levels");
        u.step_page(-1);
        assert_eq!(u.page_index(), 0);
        u.step_page(100);
        assert_eq!(u.page_index(), 10);
        u.open(Menu::System);
        assert_eq!(
            (u.menu(), u.page_index(), u.page_count()),
            (Menu::System, 0, 6)
        );
    }

    #[test]
    fn soft_buttons_focus_items_and_report_actions() {
        let mut u = open(Menu::Pedal);
        u.step_page(1); // Swell Pedal: Type, Func, Gain
        assert_eq!(u.soft(1), SoftResult::Focused);
        assert_eq!(u.focused_item().id, "pedal.2.swell_func");
        assert_eq!(u.soft(3), SoftResult::None, "only three items");
        assert_eq!(u.focus(), 1, "a missing item keeps the focus");
        let mut o = open(Menu::Organize);
        assert!(matches!(o.soft(1), SoftResult::Action(i) if i.id == "organize.1.swap"));
    }

    #[test]
    fn dial_steps_each_kind_and_clamps() {
        let mut u = open(Menu::System);
        u.step_page(1);
        let (item, v) = u.dial(3).unwrap();
        assert_eq!((item.id, v), ("system.2.transpose", Value::Int(3)));
        assert_eq!(u.dial(100).unwrap().1, Value::Int(6));
        assert!(u.dial(1).is_none(), "already at the maximum");
        let mut s = open(Menu::Sound);
        s.step_page(4); // tonewheel
        assert_eq!(s.dial(5).unwrap().1, Value::Index(2));
        s.step_page(-3); // piano pedal noise, 0.5 dB steps
        assert_eq!(s.dial(3).unwrap().1, Value::Float(1.5));
        assert_eq!(s.dial(-1).unwrap().1, Value::Float(1.0));
        let mut o = open(Menu::Output);
        o.step_page(1); // five items: three per block plus More
        assert_eq!(o.soft(3), SoftResult::More);
        assert_eq!(o.soft(1), SoftResult::Focused); // mono toggle, item 5
        assert_eq!(o.focused_item().id, "output.2.mono");
        assert_eq!(o.dial(-1), None, "already off");
        assert_eq!(o.dial(2).unwrap().1, Value::Bool(true));
    }

    #[test]
    fn unsupported_items_and_actions_never_change() {
        let mut u = open(Menu::Organize);
        assert!(matches!(u.focused_item().binding, Binding::Unsupported(_)));
        assert!(u.dial(1).is_none());
        let before = u.value(u.focused_item());
        assert_eq!(before, u.focused_item().default);
        let mut o = open(Menu::Organize);
        o.soft(1);
        assert!(o.dial(1).is_none());
    }

    #[test]
    fn shared_pedal_right_item_stays_in_sync() {
        let mut u = open(Menu::Pedal);
        u.step_page(2);
        u.soft(3); // page 3 right
        u.dial(2);
        let p4 = find_item("pedal.4.right").unwrap();
        assert_eq!(u.value(&p4), Value::Index(2));
    }

    #[test]
    fn memory_protect_defaults_on_and_survives_a_reset() {
        let mut u = open(Menu::System);
        assert!(
            u.memory_protect(),
            "factory default is protected (manual p.57)"
        );
        u.dial(-1);
        assert!(!u.memory_protect());
        u.set_memory_protect(true);
        assert!(u.memory_protect());
        u.set_memory_protect(false);
        u.step_page(1);
        u.dial(2);
        let sent = u.reset_all();
        assert!(!u.memory_protect(), "reset keeps the memory protect choice");
        assert!(sent
            .iter()
            .any(|(i, v)| i.id == "system.2.transpose" && *v == Value::Int(0)));
        assert!(sent
            .iter()
            .all(|(i, _)| matches!(i.binding, Binding::Bound(_))));
        let p = find_item("system.2.transpose").unwrap();
        assert_eq!(u.value(&p), Value::Int(0));
    }

    #[test]
    fn long_pages_scroll_in_blocks_of_three_with_more() {
        let mut u = open(Menu::Output);
        u.step_page(4); // Synth: six items
        assert!(u.paged());
        assert_eq!((u.block_start(), u.visible().len()), (0, 3));
        assert_eq!(u.soft(3), SoftResult::More);
        assert_eq!((u.block_start(), u.visible().len(), u.focus()), (3, 3, 3));
        assert_eq!(u.soft(3), SoftResult::More);
        assert_eq!(u.block_start(), 0, "More wraps to the first block");
        let mut p = open(Menu::Pedal);
        assert!(!p.paged() && p.visible().len() == 3);
        p.step_page(2);
        assert_eq!((p.block_size(), p.visible().len()), (4, 4));
    }
}
