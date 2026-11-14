//! OLED layout of the menu screens (manual OS v1.6x p.57-64).
//!
//! One generic page layout for all eight menus, built from the data model
//! (`nord_panel::menus`): the menu title, `n/N` and the page title, then one row per item of the
//! current block with the focus marker and the value. Unsupported items are dimmed (the row
//! starts with [`DIM`], the painter strips it) and name their bean instead of a value.

use super::model::MenuId;
use super::oled::OledText;
use crate::panel::ui_state::ProgramUi;
use nord_panel::menus::{ItemKind, MenuItem, Value};
use nord_panel::Binding;

/// First character of a row the painter draws dimmed (and then drops).
pub const DIM: char = '~';

/// Text of an item value as the display shows it.
pub fn format_value(item: &MenuItem, v: Value) -> String {
    match (item.kind, v) {
        (ItemKind::Toggle, Value::Bool(b)) => if b { "On" } else { "Off" }.into(),
        (ItemKind::Enum(choices), Value::Index(i)) => {
            choices.get(i).copied().unwrap_or("?").to_string()
        }
        // MIDI channels: 0 = Off.
        (
            ItemKind::Int {
                min: 0, max: 16, ..
            },
            Value::Int(0),
        ) => "Off".into(),
        // Extern bank select: -1 = "--".
        (ItemKind::Int { min: -1, .. }, Value::Int(-1)) => "--".into(),
        (ItemKind::Int { min, max, .. }, Value::Int(i)) if min < 0 && max > 0 => format!("{i:+}"),
        (_, Value::Int(i)) => i.to_string(),
        (ItemKind::Float { min, max, .. }, Value::Float(f)) if min < 0.0 && max > 0.0 => {
            format!("{f:+.1}")
        }
        (_, Value::Float(f)) => format!("{f:.1}"),
        _ => String::new(),
    }
}

/// One row: focus marker, label, value. Unsupported items are dimmed and show their bean.
fn row(ui: &ProgramUi, item: &MenuItem, focused: bool) -> String {
    let mark = if focused { '>' } else { ' ' };
    match item.binding {
        Binding::Unsupported(bean) => format!("{DIM}{mark}{} -- {bean}", item.label),
        _ if item.kind == ItemKind::Action => format!("{mark}[{}]", item.label),
        _ => format!(
            "{mark}{}: {}",
            item.label,
            format_value(item, ui.menu.value(item))
        ),
    }
}

/// Title shown in the bar of page `page_index` (titled `page_title`) of menu `m`.
///
/// VERIFIED (ARM frames, `tools/arm_screens.py`, research note s12): the first page of the Sound
/// menu is titled `PROGRAM LEVELS`, not `SOUND SETTINGS`; the last page of the Output, Pedal and
/// MIDI menus is titled `OUTPUT OVERVIEW`, `PEDAL OVERVIEW` and `MIDI OVERVIEW`; every other
/// page prints `<NAME> SETTINGS`. ASSUMED: later pages of the Sound menu keep the menu title
/// (frames of all 11 pages show `SOUND SETTINGS`).
pub fn bar_title(m: MenuId, page_index: usize, page_title: &str) -> String {
    if m == MenuId::Sound && page_index == 0 {
        "PROGRAM LEVELS".into()
    } else if page_title.ends_with("OVERVIEW") {
        page_title.into()
    } else {
        m.title().into()
    }
}

/// Text of the menu screen of `m` (the state is `ui.menu`).
pub fn compose(ui: &ProgramUi, m: MenuId) -> OledText {
    let menu = &ui.menu;
    if menu.menu() != m.model() {
        // Reached without `open_menu`: show the title only rather than another menu's page.
        return OledText {
            big: m.title().into(),
            sub: format!("manual p.{}", m.manual_page()),
            ..Default::default()
        };
    }
    if m == MenuId::Organize && ui.on_organize_page() {
        return super::organize::compose(ui);
    }
    if m == MenuId::Organize && ui.on_preset_organize_page() {
        return super::organize_presets::compose(ui);
    }
    let page = menu.page();
    let focus = menu.focus();
    let start = menu.block_start();
    let visible = menu.visible();
    let rows = visible
        .iter()
        .enumerate()
        .map(|(i, item)| row(ui, item, start + i == focus))
        .collect();
    let labels = ui.screen.soft_labels();
    let mut soft = [""; 4];
    for (s, l) in soft.iter_mut().zip(labels).take(visible.len()) {
        *s = l;
    }
    if menu.paged() {
        soft[3] = "More";
    }
    let sub = match &ui.message {
        Some((msg, _)) => msg.clone(),
        None => page.title.to_string(),
    };
    // VERIFIED: the title bar carries `n/N` right-aligned (`SYSTEM SETTINGS  1/6`); the lit soft
    // button is the focused item (`Type` / `Func` / `Gain` on the Pedal page).
    let counter = format!("{}/{}", menu.page_index() + 1, menu.page_count());
    OledText {
        big: bar_title(m, menu.page_index(), page.title),
        sub,
        rows,
        soft,
        bar: Some(counter),
        soft_on: focus
            .checked_sub(start)
            .filter(|&i| i < visible.len().min(4)),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nord_panel::menus::find_item;

    #[test]
    fn values_format_per_kind() {
        let f = |id: &str, v| format_value(&find_item(id).unwrap(), v);
        assert_eq!(f("system.1.memory_protect", Value::Bool(true)), "On");
        assert_eq!(f("sound.5.b3_tonewheel_mode", Value::Index(2)), "Vintage 2");
        assert_eq!(f("midi.3.organ_a", Value::Int(0)), "Off");
        assert_eq!(f("midi.3.organ_a", Value::Int(7)), "7");
        assert_eq!(f("extern.3.bank_msb_a", Value::Int(-1)), "--");
        assert_eq!(f("extern.5.program_a", Value::Int(12)), "12");
        assert_eq!(f("system.2.transpose", Value::Int(3)), "+3");
        assert_eq!(f("sound.2.piano_pedal_noise", Value::Float(-1.5)), "-1.5");
        assert_eq!(f("pedal.1.ctrl_gain", Value::Float(2.0)), "2.0");
        assert_eq!(f("system.5.reset_all", Value::None), "");
    }

    #[test]
    fn the_first_sound_and_the_overview_pages_have_their_own_bar_titles() {
        assert_eq!(
            bar_title(MenuId::Sound, 0, "Program Levels"),
            "PROGRAM LEVELS"
        );
        assert_eq!(bar_title(MenuId::Sound, 1, "Piano"), "SOUND SETTINGS");
        assert_eq!(
            bar_title(MenuId::Pedal, 0, "Control Pedal"),
            "PEDAL SETTINGS"
        );
        assert_eq!(
            bar_title(MenuId::Pedal, 6, "PEDAL OVERVIEW"),
            "PEDAL OVERVIEW"
        );
        let mut ui = ProgramUi::default();
        ui.open_menu(MenuId::Sound);
        let t = compose(&ui, MenuId::Sound);
        assert_eq!(t.big, "PROGRAM LEVELS");
        assert_eq!(t.bar.as_deref(), Some("1/11"));
        // The last Pedal page is the overview, titled as such in the bar.
        let mut ui = ProgramUi::default();
        ui.open_menu(MenuId::Pedal);
        for _ in 0..6 {
            ui.menu.step_page(1);
        }
        let t = compose(&ui, MenuId::Pedal);
        assert_eq!(
            (t.big.as_str(), t.bar.as_deref()),
            ("PEDAL OVERVIEW", Some("7/7"))
        );
    }

    #[test]
    fn unsupported_rows_are_dimmed_and_name_their_bean() {
        // No shipped item is `Unsupported` any more; the renderer still handles one.
        let mut item = find_item("organize.2.swap").unwrap();
        item.binding = Binding::Unsupported("ns4-zb9o");
        let mut ui = ProgramUi::default();
        let r = row(&ui, &item, true);
        assert!(r.starts_with(DIM) && r.contains("ns4-zb9o"), "{r}");
        ui.open_menu(MenuId::Output);
        ui.menu.step_page(1);
        ui.menu.soft(3); // More: bus_sub3 .. mono
        let t = compose(&ui, MenuId::Output);
        assert_eq!(t.soft, ["1", "2", "", "More"]);
        assert!(t.rows.iter().any(|r| r.contains("Mono Output: Off")));
    }
}
