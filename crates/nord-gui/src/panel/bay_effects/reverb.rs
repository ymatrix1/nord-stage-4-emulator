//! Reverb unit (manual p.52-53): ON / GLOBAL, six-way type selector with VARIATION / CHORALE,
//! DRY/WET and the BRIGHT / DARK mode.

use super::ctx::Fx;
use super::ids::dim_reason;
use crate::panel::prelude::Stage4PanelState;
use crate::panel::ui_state::fx::{Tone, Unit, REVERB, REVERB_CHORALE_FROM};
use crate::style::{LED_RED, LED_RED_DIM};
use nord_panel::behaviour::{cycle, Trigger};

/// Next BRIGHT / DARK state: normal, bright, dark, normal.
pub(super) fn next_tone(t: Tone) -> Tone {
    match t {
        Tone::Normal => Tone::Bright,
        Tone::Bright => Tone::Dark,
        Tone::Dark => Tone::Normal,
    }
}

/// Reverb unit; edits go to the focused slot.
pub(super) fn reverb(f: &mut Fx, st: &mut Stage4PanelState, slot: usize) {
    let mut changed = false;
    let global = st.fx_ui.global_reverb.is_some();
    let mut toggle_global = false;
    {
        let c = &mut st.fx_ui.chains[slot];
        let on = f.rect("reverb.on");
        f.led_above(on, c.reverb_on, LED_RED, LED_RED_DIM);
        match f.button(
            "reverb.on",
            on,
            c.reverb_on,
            true,
            "REVERB ON. Shift+click or hold: GLOBAL",
        ) {
            Some(Trigger::Primary) => {
                c.reverb_on = !c.reverb_on;
                changed = true;
            }
            Some(Trigger::Shifted) => toggle_global = true,
            _ => {}
        }
        f.legend(
            "reverb.global",
            "GLOBAL",
            global,
            "GLOBAL (Shift + On): one reverb for all Layers, placed after the Rotary and the faders",
            None,
        );

        let ty = f.rect("reverb.type");
        let leds: Vec<(&str, bool, &str)> = REVERB
            .iter()
            .enumerate()
            .map(|(i, ch)| (ch.label, c.reverb_on && i == c.rev_sel, ch.missing))
            .collect();
        f.led_list("reverb.type", ty, &leds, &[2, 1, 0], &[3, 4, 5]);
        let alt = if c.rev_sel >= REVERB_CHORALE_FROM {
            "CHORALE"
        } else {
            "VARIATION"
        };
        let tip = format!(
            "REVERB TYPE: {}\nClick: next type. Shift+click or hold: {alt}",
            REVERB[c.rev_sel.min(5)].label
        );
        match f.button("reverb.type", ty, c.reverb_on, true, &tip) {
            Some(Trigger::Primary) => {
                c.rev_sel = cycle(c.rev_sel, REVERB.len(), true);
                changed = true;
            }
            Some(Trigger::Shifted) => {
                c.rev_alt = !c.rev_alt;
                changed = true;
            }
            _ => {}
        }
        let text = if c.rev_sel >= REVERB_CHORALE_FROM {
            "CHOR"
        } else {
            "VAR"
        };
        let why = dim_reason("reverb.variation", c, slot);
        f.legend(
            "reverb.variation",
            text,
            c.rev_alt,
            "VARIATION (Spring: longer tail; Booth, Room: longer pre-delay) or CHORALE (Stage, Hall, Cathedral: more pitch modulation): Shift + selector",
            why,
        );

        let tip = format!("REVERB DRY/WET ({:.0} %)", c.reverb_mix * 100.0);
        changed |= f.knob("reverb.dry_wet", &mut c.reverb_mix, (0.0, 1.0), &tip, None);
        f.name("reverb.dry_wet", "DRY WET");

        let bd = f.rect("reverb.bright_dark");
        let leds = [
            ("BRIGHT", c.rev_tone == Tone::Bright, ""),
            ("DARK", c.rev_tone == Tone::Dark, ""),
        ];
        f.led_list("reverb.bright_dark", bd, &leds, &[], &[0, 1]);
        let tip = "BRIGHT / DARK: click cycles normal, bright, dark (CHORALE replaces it on the engine's single colour setting)";
        if f.button(
            "reverb.bright_dark",
            bd,
            c.rev_tone != Tone::Normal,
            false,
            tip,
        )
        .is_some()
        {
            c.rev_tone = next_tone(c.rev_tone);
            changed = true;
        }
    }
    if toggle_global {
        let mut edits = st.fx_ui.toggle_global(Unit::Reverb, slot);
        edits.extend(st.fx_ui.chains[slot].edits(Unit::Reverb, slot as u8));
        f.send(edits);
    } else if changed {
        let edits = st.fx_ui.after_edit(slot, Unit::Reverb);
        f.send(edits);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tone_cycles_normal_bright_dark() {
        assert_eq!(next_tone(Tone::Normal), Tone::Bright);
        assert_eq!(next_tone(Tone::Bright), Tone::Dark);
        assert_eq!(next_tone(Tone::Dark), Tone::Normal);
    }
}
