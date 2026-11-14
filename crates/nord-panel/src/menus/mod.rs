//! Data model of the Nord Stage 4 menus (manual OS v1.6x ch. 11, p.57-63; Organize p.45-46).
//!
//! Toolkit-free: every page of every menu is a `MenuPage` with `MenuItem`s that carry their kind,
//! range, default, manual page, provenance and a `Binding` (same scheme as the control inventory:
//! `Bound` host id or `engine:<path>` field, `UiOnly`, `Unsupported(bean)`). One file per menu;
//! `all_pages()` returns them in the order of the Program-button row. Spec:
//! `docs/specs/midi-and-control.md` section 5.

use crate::types::{Binding, Provenance};

mod aux_kb;
mod extern_menu;
mod midi;
mod organize;
mod output;
mod pedal;
mod sound;
mod system;
#[cfg(test)]
mod tests;

/// Item id of System page 1 "Memory Protect" (manual p.57); the Program bay gates STORE on it.
pub const ID_MEMORY_PROTECT: &str = "system.1.memory_protect";

/// The eight menus, reached with SHIFT + a Program button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Menu {
    System,
    Sound,
    Organize,
    AuxKb,
    Output,
    Pedal,
    Midi,
    Extern,
}

/// Kind and range of a menu item.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ItemKind {
    /// On / Off.
    Toggle,
    /// One of the listed choices (the default is an index).
    Enum(&'static [&'static str]),
    Int {
        min: i32,
        max: i32,
        step: i32,
    },
    Float {
        min: f32,
        max: f32,
        step: f32,
    },
    /// A soft button that triggers something (Reset, Swap, Move, ...).
    Action,
}

/// Default value of an item; its variant matches the item kind.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    Bool(bool),
    Index(usize),
    Int(i32),
    Float(f32),
    /// Actions have no value.
    None,
}

/// One setting on a menu page.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MenuItem {
    /// Stable id `menu.page.name`, lowercase ASCII; never renamed once shipped.
    pub id: &'static str,
    pub label: &'static str,
    pub kind: ItemKind,
    pub default: Value,
    /// Printed page of the OS v1.6x manual.
    pub manual_page: u16,
    pub provenance: Provenance,
    pub binding: Binding,
    /// Evidence for the provenance tag, units, and caveats (web sources are cited here).
    pub note: &'static str,
}

/// One page of a menu (`PAGE 3/4` buttons move between pages).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MenuPage {
    pub menu: Menu,
    /// 1-based page number.
    pub page: u8,
    pub title: &'static str,
    pub items: &'static [MenuItem],
}

const MANUAL: &str = "manual text";

impl MenuItem {
    const fn new(
        id: &'static str,
        label: &'static str,
        kind: ItemKind,
        default: Value,
        manual_page: u16,
        binding: Binding,
    ) -> Self {
        Self {
            id,
            label,
            kind,
            default,
            manual_page,
            provenance: Provenance::Verified,
            binding,
            note: MANUAL,
        }
    }

    /// On / Off item.
    pub(crate) const fn toggle(
        id: &'static str,
        label: &'static str,
        on: bool,
        page: u16,
        b: Binding,
    ) -> Self {
        Self::new(id, label, ItemKind::Toggle, Value::Bool(on), page, b)
    }

    /// Choice item; `default` is an index into `choices`.
    pub(crate) const fn choice(
        id: &'static str,
        label: &'static str,
        choices: &'static [&'static str],
        default: usize,
        page: u16,
        b: Binding,
    ) -> Self {
        Self::new(
            id,
            label,
            ItemKind::Enum(choices),
            Value::Index(default),
            page,
            b,
        )
    }

    /// Integer item.
    pub(crate) const fn int(
        id: &'static str,
        label: &'static str,
        (min, max, step): (i32, i32, i32),
        default: i32,
        page: u16,
        b: Binding,
    ) -> Self {
        Self::new(
            id,
            label,
            ItemKind::Int { min, max, step },
            Value::Int(default),
            page,
            b,
        )
    }

    /// Float item.
    pub(crate) const fn float(
        id: &'static str,
        label: &'static str,
        (min, max, step): (f32, f32, f32),
        default: f32,
        page: u16,
        b: Binding,
    ) -> Self {
        Self::new(
            id,
            label,
            ItemKind::Float { min, max, step },
            Value::Float(default),
            page,
            b,
        )
    }

    /// Soft-button action.
    pub(crate) const fn action(
        id: &'static str,
        label: &'static str,
        page: u16,
        b: Binding,
    ) -> Self {
        Self::new(id, label, ItemKind::Action, Value::None, page, b)
    }

    /// Marks the item `Assumed` and explains why.
    pub(crate) const fn assumed(mut self, why: &'static str) -> Self {
        self.provenance = Provenance::Assumed;
        self.note = why;
        self
    }

    /// Marks the item `Empirical` (from a non-manual source) and cites it.
    pub(crate) const fn empirical(mut self, source: &'static str) -> Self {
        self.provenance = Provenance::Empirical;
        self.note = source;
        self
    }

    /// Replaces the evidence note without changing the provenance.
    pub(crate) const fn note(mut self, note: &'static str) -> Self {
        self.note = note;
        self
    }
}

/// Every menu page, grouped by menu in Program-button order (System, Sound, Organize, Aux KB,
/// Output, Pedal, MIDI, Extern), pages ascending.
pub fn all_pages() -> Vec<MenuPage> {
    let mut v = Vec::new();
    v.extend_from_slice(system::PAGES);
    v.extend_from_slice(sound::PAGES);
    v.extend_from_slice(organize::PAGES);
    v.extend_from_slice(aux_kb::PAGES);
    v.extend_from_slice(output::PAGES);
    v.extend_from_slice(pedal::PAGES);
    v.extend_from_slice(midi::PAGES);
    v.extend_from_slice(extern_menu::PAGES);
    v
}

/// Pages of one menu, ascending.
pub fn pages_of(menu: Menu) -> Vec<MenuPage> {
    all_pages().into_iter().filter(|p| p.menu == menu).collect()
}

/// Every item of every page, with its page.
pub fn all_items() -> Vec<(MenuPage, MenuItem)> {
    all_pages()
        .into_iter()
        .flat_map(|p| p.items.iter().map(move |i| (p, *i)))
        .collect()
}

/// Looks an item up by its stable id.
pub fn find_item(id: &str) -> Option<MenuItem> {
    all_items().into_iter().map(|(_, i)| i).find(|i| i.id == id)
}

#[cfg(test)]
mod model_tests {
    use super::*;

    #[test]
    fn constructors_set_kind_default_and_provenance() {
        let t = MenuItem::toggle("a.b.c", "C", true, 57, Binding::UiOnly("x"));
        assert_eq!((t.kind, t.default), (ItemKind::Toggle, Value::Bool(true)));
        assert_eq!(t.provenance, Provenance::Verified);
        let i = MenuItem::int("a.b.d", "D", (0, 9, 1), 3, 57, Binding::UiOnly("x")).assumed("why");
        assert_eq!(i.default, Value::Int(3));
        assert_eq!((i.provenance, i.note), (Provenance::Assumed, "why"));
        let e = MenuItem::choice("a.b.e", "E", &["x", "y"], 1, 57, Binding::UiOnly("x"))
            .empirical("url");
        assert_eq!(e.provenance, Provenance::Empirical);
        assert_eq!(
            MenuItem::action("a.b.f", "F", 57, Binding::UiOnly("x")).default,
            Value::None
        );
    }

    #[test]
    fn find_item_resolves_ids() {
        assert!(find_item("system.2.transpose").is_some());
        assert!(find_item("nope").is_none());
        assert_eq!(pages_of(Menu::AuxKb).len(), 3);
        let mp = find_item(ID_MEMORY_PROTECT).expect("memory protect item");
        assert_eq!(
            mp.default,
            Value::Bool(true),
            "factory default is protected"
        );
    }
}
