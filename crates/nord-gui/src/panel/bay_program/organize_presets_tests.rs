use super::*;
use crate::panel::bay_program::model::{MenuId, Screen};

fn entries(section: &str, ext: &str, names: &[(u16, &str)]) -> Vec<(u16, String)> {
    names
        .iter()
        .map(|(i, n)| (*i, format!("{section} Preset/Bank 1/{n}.{ext}")))
        .collect()
}

/// Preset page open, Memory Protect off, Organ presets at 0..2 and Synth presets at 0 and 70.
fn ui() -> ProgramUi {
    let mut u = ProgramUi::default();
    u.menu.set_memory_protect(false);
    u.open_menu(MenuId::Organize);
    u.menu.step_page(1);
    let p = &mut u.library.presets;
    p.set_factory(
        PresetSection::Organ,
        &entries("Organ", "ns4o", &[(0, "One"), (1, "Two"), (2, "Three")]),
    );
    p.set_factory(
        PresetSection::Synth,
        &entries("Synth", "ns4y", &[(0, "Pad"), (70, "Lead")]),
    );
    u
}

fn goto(u: &mut ProgramUi, i: u16) {
    let delta = i32::from(i) - i32::from(u.preset_organize_cursor());
    u.dial(delta, false);
    assert_eq!(u.preset_organize_cursor(), i);
}

fn name(u: &ProgramUi, s: PresetSection, i: u16) -> Option<String> {
    u.library.presets.name_at(s, i)
}

fn msg(u: &ProgramUi) -> &str {
    &u.message.as_ref().unwrap().0
}

#[test]
fn labels_read_bank_and_position_one_based() {
    assert_eq!(preset_label(0), "1:01");
    assert_eq!(preset_label(4), "1:05");
    assert_eq!(preset_label(64), "2:01");
    assert_eq!(preset_label(511), "8:64");
}

#[test]
fn the_page_is_reached_and_the_dial_moves_the_location() {
    let mut u = ui();
    assert!(u.on_preset_organize_page() && !u.on_organize_page());
    assert_eq!(u.screen, Screen::Menu(MenuId::Organize));
    goto(&mut u, 2);
    let t = compose(&u);
    assert_eq!(t.big, "PRESET ORGANIZE");
    assert_eq!(t.sub, "2/2 Preset Organize");
    assert_eq!(t.rows[0], ">1:03  Three");
    assert_eq!(t.rows[1], " Organ: Choose Swap or Move");
    assert_eq!(t.soft, ["Swap", "Move", "Ok", "Undo"]);
}

#[test]
fn swap_picks_source_then_target_and_ok_interchanges() {
    let mut u = ui();
    u.menu_soft(1);
    goto(&mut u, 2);
    assert_eq!(compose(&u).rows[1], " Organ: Swap 1:01 with 1:03");
    u.menu_soft(3);
    assert_eq!(msg(&u), "Swapped 1:01 <> 1:03");
    let o = PresetSection::Organ;
    assert_eq!(name(&u, o, 0).as_deref(), Some("Three"));
    assert_eq!(name(&u, o, 2).as_deref(), Some("One"));
    assert!(u.organize.preset.pending.is_none());
}

#[test]
fn move_shifts_in_between_and_undo_restores() {
    let mut u = ui();
    u.menu_soft(2);
    goto(&mut u, 2);
    u.menu_soft(3);
    assert_eq!(msg(&u), "Moved 1:01 -> 1:03");
    let o = PresetSection::Organ;
    let got: Vec<_> = (0..3).map(|i| name(&u, o, i).unwrap()).collect();
    assert_eq!(got, ["Two", "Three", "One"]);
    u.menu_soft(4);
    assert_eq!(msg(&u), "Undone");
    assert_eq!(name(&u, o, 0).as_deref(), Some("One"));
    u.menu_soft(4);
    assert_eq!(msg(&u), "Nothing to undo");
}

#[test]
fn shift_dial_steps_the_section_and_keeps_the_pointer_inside_it() {
    let mut u = ui();
    u.menu_soft(1); // pending Swap on Organ
    u.step_organize_section(1);
    assert_eq!(u.organize_section(), PresetSection::Piano);
    assert!(
        u.organize.preset.pending.is_none(),
        "the choice belongs to a section"
    );
    u.step_organize_section(1);
    u.step_organize_section(1); // no wrap
    assert_eq!(u.organize_section(), PresetSection::Synth);
    goto(&mut u, 70);
    assert_eq!(compose(&u).rows[0], ">2:07  Lead");
    u.step_organize_section(-1);
    u.step_organize_section(-1);
    assert_eq!(u.organize_section(), PresetSection::Organ);
    assert!(u.preset_organize_cursor() < 128, "clamped to 2 banks");
    // The shift dial reaches the same code through the panel dial entry point.
    u.dial(1, true);
    assert_eq!(u.organize_section(), PresetSection::Piano);
}

#[test]
fn synth_moves_across_banks_and_an_empty_target_works() {
    let mut u = ui();
    u.step_organize_section(1);
    u.step_organize_section(1);
    u.menu_soft(2);
    goto(&mut u, 5);
    u.menu_soft(3);
    let s = PresetSection::Synth;
    assert_eq!(name(&u, s, 5).as_deref(), Some("Pad"));
    assert_eq!(name(&u, s, 0), None);
    assert_eq!(u.library.presets.count(s), 2, "a move never loses a preset");
    u.menu_soft(2);
    goto(&mut u, 70);
    u.menu_soft(3);
    assert_eq!(name(&u, s, 70).as_deref(), Some("Pad"));
    assert_eq!(
        name(&u, s, 69).as_deref(),
        Some("Lead"),
        "shifted down by the move"
    );
}

#[test]
fn memory_protect_blocks_ok_and_undo_but_not_choosing() {
    let mut u = ui();
    u.menu.set_memory_protect(true);
    u.menu_soft(1);
    goto(&mut u, 1);
    u.menu_soft(3);
    assert_eq!(msg(&u), "Memory Protected");
    assert_eq!(name(&u, PresetSection::Organ, 0).as_deref(), Some("One"));
    assert!(u.organize.preset.pending.is_some());
    u.menu.set_memory_protect(false);
    u.menu_soft(3);
    assert_eq!(name(&u, PresetSection::Organ, 0).as_deref(), Some("Two"));
    u.menu.set_memory_protect(true);
    u.menu_soft(4);
    assert_eq!(msg(&u), "Memory Protected");
}

#[test]
fn ok_without_choice_same_location_and_cancel_say_so() {
    let mut u = ui();
    u.menu_soft(3);
    assert_eq!(msg(&u), "Pick Swap or Move first");
    u.menu_soft(1);
    u.menu_soft(3);
    assert_eq!(msg(&u), "Source and target are the same");
    u.menu_soft(1); // Swap again cancels
    assert!(u.organize.preset.pending.is_none());
    u.menu_soft(2);
    u.menu_soft(1); // the other operation replaces it
    assert_eq!(u.organize.preset.pending.unwrap().0, OrganizeOp::Swap);
    u.menu_soft(4); // Undo cancels a pending choice first
    assert_eq!(msg(&u), "Cancelled");
}

#[test]
fn without_a_backup_the_page_says_so_and_reset_forgets_the_undo() {
    let mut u = ProgramUi::default();
    u.open_menu(MenuId::Organize);
    u.menu.step_page(1);
    assert!(!u.library.presets.has_layout());
    assert_eq!(compose(&u).rows[1], " Organ: No presets: open a .ns4b");
    let mut u = ui();
    u.menu_soft(1);
    goto(&mut u, 1);
    u.menu_soft(3);
    u.organize.reset();
    assert!(u.organize.preset.pending.is_none());
    u.menu_soft(4);
    assert_eq!(msg(&u), "Nothing to undo");
}

#[test]
fn set_factory_reports_changes_and_drops_locations_past_the_capacity() {
    let mut l = PresetLibrary::default();
    let rows = entries("Organ", "ns4o", &[(0, "A"), (200, "TooFar")]);
    assert!(l.set_factory(PresetSection::Organ, &rows));
    assert!(!l.set_factory(PresetSection::Organ, &rows));
    assert_eq!(l.count(PresetSection::Organ), 1);
    assert_eq!(l.cells(PresetSection::Organ).len(), 128);
    assert!(l.has_layout());
}
