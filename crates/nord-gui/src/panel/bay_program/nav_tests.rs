//! More unit tests of `nav`.

use super::*;
use crate::audio::CopyItem;
use crate::audio::SplitCfg;
use crate::panel::bay_program::model::PresetKind;
use nord_core::engine::LayerInitTarget;
use nord_core::morph::MorphSource;

fn catalog_ui(n: usize) -> ProgramUi {
    let mut u = ProgramUi::default();
    let names: Vec<String> = (0..n)
        .map(|i| {
            format!(
                "Program/Bank {}/P{:03}.ns4p",
                (b'A' + (i / 64) as u8) as char,
                i
            )
        })
        .collect();
    u.sync_catalog(&names, &["Piano/a.npno".into()], &["Samp/b.nsmp4".into()]);
    u
}

#[test]
fn shift_dial_opens_list_and_defers_the_load_until_it_rests() {
    let mut u = catalog_ui(10);
    u.now_ms = 1000;
    u.dial(2, true);
    assert_eq!(u.screen, Screen::List);
    assert_eq!(u.list_cursor, 2);
    assert_eq!(u.list_load_due, Some((1200, 2)));
    assert!(u.requests.is_empty(), "nothing loaded while turning");
    u.soft_down(2); // Abc
    assert_eq!(u.list_mode, ListMode::Abc);
    u.soft_up(2);
    assert!(!u.list_letter_held);
}

#[test]
fn abc_hold_jumps_to_the_next_letter() {
    let mut u = ProgramUi::default();
    u.sync_catalog(
        &[
            "P/Apple.ns4p".into(),
            "P/Avocado.ns4p".into(),
            "P/Berry.ns4p".into(),
        ],
        &[],
        &[],
    );
    u.screen = Screen::List;
    u.list_mode = ListMode::Abc;
    u.list_letter_held = true;
    u.dial(1, false);
    assert_eq!(u.list_cursor, 2, "A.. -> B..");
}

#[test]
fn live_mode_saves_the_slot_it_leaves_and_recalls_the_target() {
    let engine = nord_core::engine::Stage4Engine::new(48_000.0);
    let mut u = ProgramUi::default();
    u.live_armed = true;
    assert!(
        u.program_button(2).is_empty(),
        "first slot: nothing to save"
    );
    assert_eq!(u.live.slot(), Some(1));
    u.library.put_live(2, engine.capture_state());
    u.live_armed = true;
    let out = u.program_button(3);
    assert_eq!(out.len(), 2, "snapshot of slot 2 and recall of slot 3");
    assert!(matches!(
        &out[0],
        AudioCommand::Program(ProgramEdit::Snapshot { .. })
    ));
    assert!(matches!(
        &out[1],
        AudioCommand::Program(ProgramEdit::Recall(_))
    ));
    // A normal program button leaves Live Mode and saves the slot.
    let out = u.program_button(1);
    assert!(!u.live.is_active());
    assert!(matches!(
        &out[0],
        AudioCommand::Program(ProgramEdit::Snapshot { .. })
    ));
}

#[test]
fn num_pad_takes_page_then_program() {
    let mut u = catalog_ui(64);
    u.num_pad = true;
    assert!(u.program_button(3).is_empty());
    assert_eq!(u.num_first, Some(3));
    u.program_button(5);
    assert_eq!(u.loc.label(), "A:35");
    assert_eq!(u.num_first, None);
}

#[test]
fn soft_buttons_drive_split_clock_undo_and_layer_init() {
    let mut u = ProgramUi::default();
    u.screen = Screen::Split;
    let out = u.soft_down(2);
    assert!(matches!(
        &out[0],
        AudioCommand::Program(ProgramEdit::Split(SplitCfg { .. }))
    ));
    u.screen = Screen::Clock;
    u.soft_down(1);
    assert_eq!(u.clock.kbs, super::super::model::KbsMode::On);
    u.screen = Screen::Undo;
    assert!(matches!(
        &u.soft_down(4)[0],
        AudioCommand::Program(ProgramEdit::Undo)
    ));
    assert_eq!(u.screen, Screen::Home);
    u.screen = Screen::LayerInit;
    u.focus = LayerId::PianoB;
    let out = u.soft_down(3);
    assert!(matches!(
        &out[0],
        AudioCommand::Program(ProgramEdit::LayerInit(LayerInitTarget::PianoB))
    ));
}

#[test]
fn copy_paste_swap_use_the_focused_layer() {
    let mut u = ProgramUi::default();
    u.screen = Screen::Paste;
    assert!(u.soft_down(1).is_empty(), "nothing copied yet");
    u.screen = Screen::Copy;
    u.focus = LayerId::SynthB;
    u.soft_down(1);
    assert_eq!(u.copied, Some(CopyItem::Layer(LayerId::SynthB)));
    u.screen = Screen::Paste;
    u.page(1, false); // focus -> Synth C
    assert_eq!(u.focus, LayerId::SynthC);
    let out = u.soft_down(2);
    assert!(matches!(
        &out[0],
        AudioCommand::Program(ProgramEdit::Swap(LayerId::SynthC, LayerId::SynthA))
    ));
    assert_eq!(sibling(LayerId::OrganB), LayerId::OrganA);
}

#[test]
fn preset_dial_sets_organ_model_and_queues_piano_loads() {
    let mut u = catalog_ui(1);
    u.screen = Screen::Preset {
        kind: PresetKind::Organ,
        single: false,
    };
    let out = u.dial(2, false);
    assert!(matches!(
        &out[0],
        AudioCommand::SetOrganModel(OrganModel::Farfisa)
    ));
    assert_eq!(u.organ_request, Some(OrganModel::Farfisa));
    u.screen = Screen::Preset {
        kind: PresetKind::Piano,
        single: true,
    };
    u.focus = LayerId::PianoB;
    u.dial(1, false);
    assert_eq!(
        u.take_requests(),
        vec![LoadRequest::Piano {
            entry: "Piano/a.npno".into(),
            slot_b: true
        }]
    );
    u.soft_down(3); // Cancel
    assert_eq!(u.screen, Screen::Home);
}

#[test]
fn exit_closes_screens_latches_and_a_running_store() {
    let mut u = ProgramUi::default();
    u.store.begin("x", ProgLoc::default());
    u.screen = Screen::StoreTo;
    u.section_latch.double_tap();
    u.morph.double_tap(MorphSource::ModWheel);
    u.exit();
    assert!(!u.store.is_active() && !u.section_edit_active());
    assert_eq!(u.morph.assigning(), None);
    assert_eq!(u.screen, Screen::Home);
}
