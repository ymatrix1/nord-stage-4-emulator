//! Program locations, the in-memory user library and the STORE state machine (manual p.41-42).

use super::name::{suggest_name, NameEditor};
use nord_core::engine::ProgramState;
use std::collections::BTreeMap;

/// Program banks.
///
/// ASSUMED: 8 banks (`A`..`H`), the clamp `Stage4PanelState::apply_program` already uses; the
/// manual only says a bank holds 8 pages of 8 programs (p.41).
pub const BANKS: u8 = 8;
/// Pages per bank and programs per page (manual p.41).
pub const PAGES: u8 = 8;
/// Programs per bank.
pub const PER_BANK: u16 = 64;
/// Total program locations.
pub const TOTAL: u16 = BANKS as u16 * PER_BANK;

/// A program location such as `C:87` (bank C, page 8, program 7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct ProgLoc {
    pub bank: u8,
    /// Page `0..8` (shown as 1..8).
    pub page: u8,
    /// Program button `0..8` (shown as 1..8).
    pub slot: u8,
}

impl ProgLoc {
    /// Flat index `0..TOTAL`.
    pub fn index(self) -> u16 {
        u16::from(self.bank) * PER_BANK + u16::from(self.page) * 8 + u16::from(self.slot)
    }

    /// Location of flat index `i` (wraps).
    pub fn from_index(i: u16) -> Self {
        let i = i % TOTAL;
        Self {
            bank: (i / PER_BANK) as u8,
            page: ((i % PER_BANK) / 8) as u8,
            slot: (i % 8) as u8,
        }
    }

    /// Label as the display shows it: `A:11`.
    pub fn label(self) -> String {
        format!(
            "{}:{}{}",
            (b'A' + self.bank.min(BANKS - 1)) as char,
            self.page + 1,
            self.slot + 1
        )
    }

    /// Parses `A:11`; `None` when malformed or out of range.
    pub fn parse(s: &str) -> Option<Self> {
        let b = s.as_bytes();
        if b.len() != 4 || b[1] != b':' {
            return None;
        }
        let bank = b[0].checked_sub(b'A')?;
        let page = b[2].checked_sub(b'1')?;
        let slot = b[3].checked_sub(b'1')?;
        (bank < BANKS && page < PAGES && slot < 8).then_some(Self { bank, page, slot })
    }

    /// Move by `delta` programs (wraps over all banks).
    pub fn step(self, delta: i32) -> Self {
        let i = (i32::from(self.index()) + delta).rem_euclid(i32::from(TOTAL));
        Self::from_index(i as u16)
    }

    /// PAGE buttons: previous/next page inside the bank (wraps).
    pub fn step_page(self, delta: i32) -> Self {
        let page = (i32::from(self.page) + delta).rem_euclid(i32::from(PAGES)) as u8;
        Self { page, ..self }
    }

    /// BANK buttons: previous/next bank (wraps).
    pub fn step_bank(self, delta: i32) -> Self {
        let bank = (i32::from(self.bank) + delta).rem_euclid(i32::from(BANKS)) as u8;
        Self { bank, ..self }
    }

    /// Same page, program button `n` (1..=8); other values keep the slot.
    pub fn with_button(self, n: u8) -> Self {
        if (1..=8).contains(&n) {
            Self {
                slot: n - 1,
                ..self
            }
        } else {
            self
        }
    }
}

/// A program stored by the user into the in-memory library.
#[derive(Debug, Clone)]
pub struct UserProgram {
    pub name: String,
    /// Index into `name::CATEGORIES`.
    pub category: usize,
    pub state: Box<ProgramState>,
}

/// In-memory program storage: banks plus the eight Live slots. Nothing is written to disk.
#[derive(Debug, Clone, Default)]
pub struct UserLibrary {
    pub(super) programs: BTreeMap<u16, UserProgram>,
    live: [Option<Box<ProgramState>>; 8],
    /// Archive entry that sits at each location of the opened `.ns4b` (flat index -> entry path),
    /// read from the entry headers; Organize moves these along with the user programs.
    pub(super) factory: BTreeMap<u16, String>,
}

impl UserLibrary {
    /// Stored program at `loc`.
    pub fn get(&self, loc: ProgLoc) -> Option<&UserProgram> {
        self.programs.get(&loc.index())
    }

    /// Store (overwrite) a program.
    pub fn put(&mut self, loc: ProgLoc, p: UserProgram) {
        self.programs.insert(loc.index(), p);
    }

    /// Number of stored programs.
    pub fn len(&self) -> usize {
        self.programs.len()
    }

    /// No program stored.
    pub fn is_empty(&self) -> bool {
        self.programs.is_empty()
    }

    /// A program of this exact name is stored or sits in the opened archive.
    pub fn name_exists(&self, name: &str) -> bool {
        self.programs.values().any(|p| p.name == name)
            || self.factory.iter().any(|(i, f)| {
                !self.programs.contains_key(i) && super::catalog::display_name(f) == name
            })
    }

    /// Live slot `0..8`.
    pub fn live(&self, slot: u8) -> Option<&ProgramState> {
        self.live.get(usize::from(slot))?.as_deref()
    }

    /// Store into Live slot `0..8` (ignored when out of range).
    pub fn put_live(&mut self, slot: u8, state: Box<ProgramState>) {
        if let Some(s) = self.live.get_mut(usize::from(slot)) {
            *s = Some(state);
        }
    }
}

/// What a snapshot reply from the DSP thread is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingSnapshot {
    /// STORE into a bank location.
    Bank {
        loc: ProgLoc,
        name: String,
        category: usize,
    },
    /// Live Mode auto-store into slot `0..8`.
    Live(u8),
}

/// Where a STORE writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreTarget {
    /// A program bank location.
    Bank(ProgLoc),
    /// A Live program location (LIVE MODE pressed during Store).
    Live(u8),
}

/// STORE progress: Idle, naming (STORE AS), choosing a location.
#[derive(Debug, Clone, Default)]
pub struct StoreFlow {
    pub stage: StoreStage,
    pub target: Option<StoreTarget>,
    pub editor: NameEditor,
}

/// Stage of a Store operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StoreStage {
    #[default]
    Idle,
    /// STORE PROGRAM AS: editing the name.
    Naming,
    /// STORE PROGRAM TO: choosing the location; STORE confirms.
    Choosing,
}

impl StoreFlow {
    /// STORE pressed: start choosing a location, keeping the current name.
    pub fn begin(&mut self, name: &str, here: ProgLoc) {
        self.editor = NameEditor::new(name);
        self.target = Some(StoreTarget::Bank(here));
        self.stage = StoreStage::Choosing;
    }

    /// STORE AS pressed: name first, proposing `name` (numbered when `taken`, see
    /// [`suggest_name`]).
    pub fn begin_as(&mut self, name: &str, here: ProgLoc, taken: impl Fn(&str) -> bool) {
        self.begin(&suggest_name(name, taken), here);
        self.stage = StoreStage::Naming;
    }

    /// Naming done (STORE pressed): go on to choose the location.
    pub fn finish_naming(&mut self) {
        if self.stage == StoreStage::Naming {
            self.stage = StoreStage::Choosing;
        }
    }

    /// Shift/Exit.
    pub fn cancel(&mut self) {
        self.stage = StoreStage::Idle;
        self.target = None;
    }

    /// A store is in progress.
    pub fn is_active(&self) -> bool {
        self.stage != StoreStage::Idle
    }

    /// Current target as a bank location (Live slots map to `None`).
    pub fn bank_target(&self) -> Option<ProgLoc> {
        match self.target? {
            StoreTarget::Bank(l) => Some(l),
            StoreTarget::Live(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn location_labels_round_trip() {
        let l = ProgLoc::parse("C:87").unwrap();
        assert_eq!((l.bank, l.page, l.slot), (2, 7, 6));
        assert_eq!(l.label(), "C:87");
        assert_eq!(ProgLoc::from_index(l.index()), l);
        assert_eq!(ProgLoc::parse("A:11").unwrap().index(), 0);
        assert_eq!(ProgLoc::parse("Z:11"), None);
        assert_eq!(ProgLoc::parse("A:91"), None);
        assert_eq!(ProgLoc::parse("A:1"), None);
    }

    #[test]
    fn steps_wrap_and_page_bank_stay_in_range() {
        let first = ProgLoc::default();
        assert_eq!(first.step(-1).index(), TOTAL - 1);
        assert_eq!(ProgLoc::from_index(TOTAL - 1).step(1), first);
        assert_eq!(first.step_page(-1).page, 7);
        assert_eq!(first.step_bank(-1).bank, BANKS - 1);
        assert_eq!(first.with_button(5).slot, 4);
        assert_eq!(first.with_button(0), first);
    }

    #[test]
    fn user_library_stores_programs_and_live_slots() {
        let engine = nord_core::engine::Stage4Engine::new(48_000.0);
        let mut lib = UserLibrary::default();
        assert!(lib.is_empty());
        let loc = ProgLoc::parse("B:23").unwrap();
        lib.put(
            loc,
            UserProgram {
                name: "Mine".into(),
                category: 3,
                state: engine.capture_state(),
            },
        );
        assert_eq!(lib.len(), 1);
        assert_eq!(lib.get(loc).unwrap().name, "Mine");
        assert!(lib.get(ProgLoc::default()).is_none());
        lib.put_live(7, engine.capture_state());
        lib.put_live(8, engine.capture_state());
        assert!(lib.live(7).is_some() && lib.live(0).is_none() && lib.live(8).is_none());
    }

    #[test]
    fn name_exists_sees_stored_programs_and_archive_entries() {
        let engine = nord_core::engine::Stage4Engine::new(48_000.0);
        let mut lib = UserLibrary::default();
        assert!(!lib.name_exists("Mine"));
        lib.put(
            ProgLoc::default(),
            UserProgram {
                name: "Mine".into(),
                category: 0,
                state: engine.capture_state(),
            },
        );
        assert!(lib.name_exists("Mine") && !lib.name_exists("mine"));
        lib.set_factory(&[(5, "Program/Bank A/Grand.ns4p".into())]);
        let shown = lib.name_at(ProgLoc::from_index(5)).unwrap();
        assert!(lib.name_exists(&shown));
    }

    #[test]
    fn store_flow_goes_idle_naming_choosing_and_cancels() {
        let mut f = StoreFlow::default();
        assert!(!f.is_active());
        f.begin("Rusty PWM", ProgLoc::parse("C:87").unwrap());
        assert_eq!(f.stage, StoreStage::Choosing);
        assert_eq!(f.bank_target().unwrap().label(), "C:87");
        f.begin_as("Rusty PWM", ProgLoc::default(), |_| false);
        assert_eq!(f.stage, StoreStage::Naming);
        assert_eq!(f.editor.name(), "Rusty PWM");
        f.begin_as("Rusty PWM", ProgLoc::default(), |n| n == "Rusty PWM");
        assert_eq!(f.editor.name(), "Rusty PWM 2");
        f.finish_naming();
        assert_eq!(f.stage, StoreStage::Choosing);
        f.target = Some(StoreTarget::Live(2));
        assert_eq!(f.bank_target(), None);
        f.cancel();
        assert!(!f.is_active() && f.target.is_none());
    }
}
