//! Organize (manual OS v1.6x p.45-46): Swap and Move over a bank array, as pure functions.
//!
//! A bank array holds one slot per location (`bank * 64 + page * 8 + program`, 512 locations for
//! programs). Programs and presets use the same functions: the manual says presets are organized
//! "in just the same way" (p.45).
//!
//! - **Swap** interchanges the content of two locations.
//! - **Move** takes the content of the source out and inserts it at the destination; every
//!   location between them shifts by one towards the source (down when moving up the list, up
//!   when moving down the list). Nothing is lost and no location becomes empty.
//!
//! Provenance: both semantics are `VERIFIED` against the manual text only (p.45-46); the hardware
//! behaviour at the ends of the list is `ASSUMED` to be the same as in the middle (no wrap).

use std::ops::RangeInclusive;

/// The two Organize operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrganizeOp {
    Swap,
    Move,
}

impl OrganizeOp {
    /// Past-tense word for the OLED message.
    pub fn done_label(self) -> &'static str {
        match self {
            Self::Swap => "Swapped",
            Self::Move => "Moved",
        }
    }
}

/// Swaps locations `a` and `b`. Returns `false` (nothing changes) when either is out of range or
/// they are the same location.
pub fn swap<T>(bank: &mut [T], a: usize, b: usize) -> bool {
    if a == b || a >= bank.len() || b >= bank.len() {
        return false;
    }
    bank.swap(a, b);
    true
}

/// Moves the content of `src` to `dst`, shifting the locations in between by one towards `src`.
/// Returns `false` (nothing changes) when either is out of range or they are the same location.
pub fn move_item<T>(bank: &mut [T], src: usize, dst: usize) -> bool {
    if src == dst || src >= bank.len() || dst >= bank.len() {
        return false;
    }
    if src < dst {
        bank[src..=dst].rotate_left(1);
    } else {
        bank[dst..=src].rotate_right(1);
    }
    true
}

/// Applies `op` to `src` and `dst`.
pub fn apply<T>(bank: &mut [T], op: OrganizeOp, src: usize, dst: usize) -> bool {
    match op {
        OrganizeOp::Swap => swap(bank, src, dst),
        OrganizeOp::Move => move_item(bank, src, dst),
    }
}

/// Locations whose content changes when `op` runs on `src` and `dst`.
pub fn affected(op: OrganizeOp, src: usize, dst: usize) -> RangeInclusive<usize> {
    // A swap changes only the two ends; the range is their hull, like a move's.
    let _ = op;
    src.min(dst)..=src.max(dst)
}

/// One-level Undo for Organize (the OLED "Undo" soft button): remembers the bank before the last
/// operation.
#[derive(Debug, Clone)]
pub struct OrganizeUndo<T> {
    snapshot: Option<Vec<T>>,
}

impl<T> Default for OrganizeUndo<T> {
    fn default() -> Self {
        Self { snapshot: None }
    }
}

impl<T: Clone> OrganizeUndo<T> {
    /// Applies `op` and remembers the previous state; `false` when the operation did nothing (the
    /// previous undo state is kept then).
    pub fn apply(&mut self, bank: &mut [T], op: OrganizeOp, src: usize, dst: usize) -> bool {
        let before = bank.to_vec();
        let changed = apply(bank, op, src, dst);
        if changed {
            self.snapshot = Some(before);
        }
        changed
    }

    /// Restores the bank before the last operation; `false` when there is nothing to undo.
    pub fn undo(&mut self, bank: &mut [T]) -> bool {
        match self.snapshot.take() {
            Some(prev) if prev.len() == bank.len() => {
                bank.clone_from_slice(&prev);
                true
            }
            _ => false,
        }
    }

    /// An operation can be undone.
    pub fn can_undo(&self) -> bool {
        self.snapshot.is_some()
    }

    /// Forgets the undo state (the library changed by other means).
    pub fn clear(&mut self) {
        self.snapshot = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bank() -> Vec<u32> {
        (0..8).collect()
    }

    #[test]
    fn swap_interchanges_two_locations_and_nothing_else() {
        let mut b = bank();
        assert!(swap(&mut b, 1, 6));
        assert_eq!(b, [0, 6, 2, 3, 4, 5, 1, 7]);
        assert!(swap(&mut b, 6, 1));
        assert_eq!(b, bank());
    }

    #[test]
    fn move_down_the_list_shifts_the_programs_in_between_up() {
        let mut b = bank();
        assert!(move_item(&mut b, 1, 4));
        assert_eq!(b, [0, 2, 3, 4, 1, 5, 6, 7]);
    }

    #[test]
    fn move_up_the_list_shifts_the_programs_in_between_down() {
        let mut b = bank();
        assert!(move_item(&mut b, 5, 2));
        assert_eq!(b, [0, 1, 5, 2, 3, 4, 6, 7]);
    }

    #[test]
    fn neighbours_move_like_a_swap_and_the_ends_work() {
        let mut b = bank();
        assert!(move_item(&mut b, 3, 4));
        assert_eq!(b, [0, 1, 2, 4, 3, 5, 6, 7]);
        let mut b = bank();
        assert!(move_item(&mut b, 0, 7));
        assert_eq!(b, [1, 2, 3, 4, 5, 6, 7, 0]);
        assert!(move_item(&mut b, 7, 0));
        assert_eq!(b, bank());
    }

    #[test]
    fn same_or_out_of_range_locations_change_nothing() {
        for op in [OrganizeOp::Swap, OrganizeOp::Move] {
            let mut b = bank();
            assert!(!apply(&mut b, op, 3, 3));
            assert!(!apply(&mut b, op, 3, 8));
            assert!(!apply(&mut b, op, 99, 0));
            assert_eq!(b, bank());
        }
        let mut empty: Vec<u32> = Vec::new();
        assert!(!swap(&mut empty, 0, 0));
    }

    #[test]
    fn move_keeps_every_item_and_works_with_empty_slots() {
        let mut b: Vec<Option<&str>> = vec![Some("a"), None, Some("c"), None, Some("e")];
        assert!(move_item(&mut b, 0, 3));
        assert_eq!(b, [None, Some("c"), None, Some("a"), Some("e")]);
        let mut sorted = b.clone();
        sorted.sort();
        assert_eq!(sorted, [None, None, Some("a"), Some("c"), Some("e")]);
    }

    #[test]
    fn affected_range_spans_source_and_destination() {
        assert_eq!(affected(OrganizeOp::Move, 5, 2), 2..=5);
        assert_eq!(affected(OrganizeOp::Swap, 1, 4), 1..=4);
        assert_eq!(OrganizeOp::Swap.done_label(), "Swapped");
        assert_eq!(OrganizeOp::Move.done_label(), "Moved");
    }

    #[test]
    fn undo_restores_the_last_operation_once() {
        let mut b = bank();
        let mut u = OrganizeUndo::default();
        assert!(!u.undo(&mut b) && !u.can_undo());
        assert!(u.apply(&mut b, OrganizeOp::Move, 0, 5));
        assert!(!u.apply(&mut b, OrganizeOp::Swap, 2, 2), "no-op keeps undo");
        assert!(u.can_undo());
        assert!(u.undo(&mut b));
        assert_eq!(b, bank());
        assert!(!u.undo(&mut b));
        assert!(u.apply(&mut b, OrganizeOp::Swap, 0, 1));
        u.clear();
        assert!(!u.can_undo());
    }
}
