//! UI-only state of the Program area (widgets that have no engine field of their own).
//!
//! The behaviour lives in `panel/bay_program/` (`actions.rs` and friends); this file holds the
//! data and the small accessors other bays and the app glue need. `Stage4PanelState::program_ui`
//! holds the struct.

#![cfg_attr(test, allow(clippy::field_reassign_with_default))]

use crate::audio::{CopyItem, ProgramEdit};
use crate::panel::bay_program::catalog::{Catalog, LoadRequest};
use crate::panel::bay_program::clock::{ClockUi, TransposeUi};
use crate::panel::bay_program::model::{ListMode, ProgView, Screen};
use crate::panel::bay_program::morph::MorphUi;
use crate::panel::bay_program::split::SplitUi;
use crate::panel::bay_program::store::{PendingSnapshot, ProgLoc, StoreFlow, UserLibrary};
use crossbeam_channel::{Receiver, Sender};
use nord_core::engine::ProgramState;
use nord_core::morph::{LayerId, MorphSource};
use nord_core::organ::OrganModel;
use nord_panel::behaviour::{ButtonTracker, Latch, LiveMode};
use std::collections::BTreeMap;

/// UI state for the Program area.
#[derive(Debug)]
pub struct ProgramUi {
    /// What the OLED shows.
    pub screen: Screen,
    pub view: ProgView,
    /// PRESET NAME (Shift+Prog View): show the sound source instead of the preset name.
    pub preset_name: bool,
    /// SOLO state (monitor mode, not stored; no engine effect yet).
    pub solo: bool,
    pub list_mode: ListMode,
    /// Cursor into the ordered program catalog.
    pub list_cursor: usize,
    /// Catalog index waiting to load once the dial rests (ms timestamp), `None` when idle.
    pub list_load_due: Option<(u32, usize)>,
    /// Current program location and title.
    pub loc: ProgLoc,
    /// Page being browsed with PAGE / BANK (the slot is ignored).
    pub browse: ProgLoc,
    /// ABC / Cat soft buttons held on the LIST screen (dial jumps by letter / category).
    pub list_letter_held: bool,
    pub list_cat_held: bool,
    pub title: String,
    /// Last `prog_id` this area wrote to the panel state (detects external changes).
    pub last_prog_id: String,
    pub store: StoreFlow,
    pub library: UserLibrary,
    pub live: LiveMode,
    /// LIVE MODE pressed: the next Program button picks the Live slot.
    pub live_armed: bool,
    /// NUM PAD navigation on; `Some(page)` after the first digit.
    pub num_pad: bool,
    pub num_first: Option<u8>,
    /// SECTION EDIT: held, and sticky (double tap).
    pub section_held: bool,
    pub section_latch: Latch,
    pub morph: MorphUi,
    pub split: SplitUi,
    pub clock: ClockUi,
    pub transp: TransposeUi,
    pub scene2: bool,
    /// What MON/COPY copied last (the engine holds the data).
    pub copied: Option<CopyItem>,
    /// Layer the Copy / Paste / Single-layer screens act on.
    pub focus: LayerId,
    /// Backup library names and the requests waiting for the app.
    pub catalog: Catalog,
    pub requests: Vec<LoadRequest>,
    /// Cursor of the Preset Library browser.
    pub preset_cursor: usize,
    /// Organ model chosen in the Preset browser, for the panel to adopt.
    pub organ_request: Option<OrganModel>,
    /// MIDI / EXTERN LED blink end times (ms).
    pub midi_led_until: u32,
    pub extern_led_until: u32,
    /// Engine telemetry: nothing to undo (UNDO soft button disabled) / an undone edit to redo.
    pub undo_blocked: bool,
    pub redo_available: bool,
    /// Keys seen last frame (MIDI activity detection).
    pub prev_keys: [bool; 128],
    /// Transient OLED message and its end time (ms).
    pub message: Option<(String, u32)>,
    /// Button trackers by control id (hold / double tap recognition).
    pub trackers: BTreeMap<&'static str, ButtonTracker>,
    /// Frame time in ms.
    pub now_ms: u32,
    /// A SHIFT button was used as a modifier since it went down (its release then is no EXIT).
    pub shift_used: bool,
    /// Dial drag / scroll remainder in pixels, and the knob's drawn position in clicks.
    pub dial_accum: f32,
    pub dial_pos: i32,
    /// Snapshot replies: tickets, pending uses, channel.
    pub next_ticket: u32,
    pub pending: Vec<(u32, PendingSnapshot)>,
    pub reply_tx: Sender<(u32, Box<ProgramState>)>,
    pub reply_rx: Receiver<(u32, Box<ProgramState>)>,
    /// Inventory ids painted in the last frame (evidence for the "every control is drawn" test).
    pub painted: Vec<&'static str>,
}

impl Default for ProgramUi {
    fn default() -> Self {
        let (reply_tx, reply_rx) = crossbeam_channel::unbounded();
        Self {
            screen: Screen::Home,
            view: ProgView::Name,
            preset_name: false,
            solo: false,
            list_mode: ListMode::Num,
            list_cursor: 0,
            list_load_due: None,
            loc: ProgLoc::parse("A:11").unwrap_or_default(),
            browse: ProgLoc::parse("A:11").unwrap_or_default(),
            list_letter_held: false,
            list_cat_held: false,
            title: "Nord Stage 4".to_string(),
            last_prog_id: "A:11".to_string(),
            store: StoreFlow::default(),
            library: UserLibrary::default(),
            live: LiveMode::default(),
            live_armed: false,
            num_pad: false,
            num_first: None,
            section_held: false,
            section_latch: Latch::default(),
            morph: MorphUi::default(),
            split: SplitUi::default(),
            clock: ClockUi::default(),
            transp: TransposeUi::default(),
            scene2: false,
            copied: None,
            focus: LayerId::PianoA,
            catalog: Catalog::default(),
            requests: Vec::new(),
            preset_cursor: 0,
            organ_request: None,
            midi_led_until: 0,
            extern_led_until: 0,
            undo_blocked: false,
            redo_available: false,
            prev_keys: [false; 128],
            message: None,
            trackers: BTreeMap::new(),
            now_ms: 0,
            shift_used: false,
            dial_accum: 0.0,
            dial_pos: 0,
            next_ticket: 1,
            pending: Vec::new(),
            reply_tx,
            reply_rx,
            painted: Vec::new(),
        }
    }
}

impl ProgramUi {
    /// SECTION EDIT is active (held or sticky): edits should reach every Layer of the section.
    /// Bays apply it with `Stage4Engine::section_edit_*` (see `nord_core::engine::section_edit`).
    pub fn section_edit_active(&self) -> bool {
        self.section_held || self.section_latch.is_active()
    }

    /// Morph source being assigned right now (Morph Assign held or latched).
    pub fn assigning_morph(&self) -> Option<MorphSource> {
        self.morph.assigning()
    }

    /// The backup library was (re)opened or changed: hand its names to the panel.
    pub fn sync_catalog(&mut self, programs: &[String], pianos: &[String], samples: &[String]) {
        if self.catalog.set(programs, pianos, samples) {
            self.list_cursor = 0;
            self.preset_cursor = 0;
        }
    }

    /// Requests the app must pass to the backup library (drains the queue).
    pub fn take_requests(&mut self) -> Vec<LoadRequest> {
        std::mem::take(&mut self.requests)
    }

    /// Records that control `id` was painted this frame.
    pub fn mark(&mut self, id: &'static str) {
        self.painted.push(id);
    }

    /// Show `text` on the OLED for about two seconds.
    pub fn set_message(&mut self, text: impl Into<String>) {
        self.message = Some((text.into(), self.now_ms.saturating_add(2000)));
    }

    /// Records the edit a stored program needs: sends a snapshot request and remembers its use.
    pub fn request_snapshot(&mut self, use_for: PendingSnapshot) -> ProgramEdit {
        let ticket = self.next_ticket;
        self.next_ticket = self.next_ticket.wrapping_add(1).max(1);
        self.pending.push((ticket, use_for));
        ProgramEdit::Snapshot {
            ticket,
            reply: self.reply_tx.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_describe_program_a11_in_name_view() {
        let u = ProgramUi::default();
        assert_eq!(u.loc.label(), "A:11");
        assert_eq!(u.screen, Screen::Home);
        assert!(!u.section_edit_active());
        assert_eq!(u.assigning_morph(), None);
        assert!(u.requests.is_empty() && u.pending.is_empty());
    }

    #[test]
    fn section_edit_is_active_when_held_or_latched() {
        let mut u = ProgramUi::default();
        u.section_held = true;
        assert!(u.section_edit_active());
        u.section_held = false;
        u.section_latch.double_tap();
        assert!(u.section_edit_active());
    }

    #[test]
    fn catalog_sync_resets_cursors_and_requests_drain() {
        let mut u = ProgramUi::default();
        u.list_cursor = 5;
        u.sync_catalog(&["a.ns4p".to_string()], &[], &[]);
        assert_eq!(u.list_cursor, 0);
        u.requests.push(LoadRequest::Program("a.ns4p".into()));
        assert_eq!(u.take_requests().len(), 1);
        assert!(u.take_requests().is_empty());
    }

    #[test]
    fn snapshot_requests_get_increasing_tickets_and_messages_expire_later() {
        let mut u = ProgramUi::default();
        u.now_ms = 100;
        let a = u.request_snapshot(PendingSnapshot::Live(1));
        let b = u.request_snapshot(PendingSnapshot::Live(2));
        let (ProgramEdit::Snapshot { ticket: ta, .. }, ProgramEdit::Snapshot { ticket: tb, .. }) =
            (a, b)
        else {
            panic!("expected snapshots");
        };
        assert_eq!((ta, tb), (1, 2));
        u.set_message("hi");
        assert_eq!(u.message.as_ref().unwrap().1, 2100);
    }
}
