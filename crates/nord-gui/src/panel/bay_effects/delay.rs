//! Delay unit (manual p.50-51): ON / GLOBAL, TEMPO / MST CLK, FEEDBACK, DRY/WET, effects selector
//! with VARIATION, FILTER / PING PONG and TAP/SET / ANALOG.

use super::ctx::{from_log, to_log, Fx};
use crate::panel::param_hint::fmt;
use crate::panel::param_hint::table::DELAY_MS as TIME_MS;
use crate::panel::prelude::Stage4PanelState;
use crate::panel::ui_state::fx::{ChainUi, Unit, DELAY_FX, MST_LABELS};
use crate::style::{LED_RED, LED_RED_DIM};
use nord_core::fx::DelayFilterMode;
use nord_panel::behaviour::{cycle, Trigger};

/// Feedback filter order of the FILTER button: off, LP, HP, BP.
const FILTERS: [DelayFilterMode; 4] = [
    DelayFilterMode::Off,
    DelayFilterMode::Lp,
    DelayFilterMode::Hp,
    DelayFilterMode::Bp,
];

fn next_filter(cur: DelayFilterMode) -> DelayFilterMode {
    let i = FILTERS.iter().position(|m| *m == cur).unwrap_or(0);
    FILTERS[cycle(i, FILTERS.len(), true)]
}

/// TEMPO knob: shows the delay time, or the MST CLK division while the Master Clock is on.
/// Shift + turn (or any turn while MST CLK is on) selects the division; turning it fully
/// counter-clockwise returns to free time (manual p.51).
fn tempo(f: &mut Fx, c: &mut ChainUi) -> bool {
    let mut changed = false;
    let steps = (MST_LABELS.len() - 1) as f32;
    if c.mst > 0 || f.shift {
        let mut v = c.mst as f32;
        let tip = format!("MST CLK division: {}", MST_LABELS[c.mst]);
        if f.knob("delay.tempo", &mut v, (0.0, steps), &tip, None) {
            let next = v.round().clamp(0.0, steps) as usize;
            changed = next != c.mst;
            c.mst = next;
            f.hint("delay.tempo", MST_LABELS[next]);
        }
    } else {
        let mut n = to_log(c.tempo_ms, TIME_MS.0, TIME_MS.1);
        let tip = format!("DELAY TEMPO ({:.0} ms). Shift + turn: MST CLK", c.tempo_ms);
        if f.knob("delay.tempo", &mut n, (0.0, 1.0), &tip, None) {
            c.tempo_ms = from_log(n, TIME_MS.0, TIME_MS.1);
            changed = true;
            f.hint("delay.tempo", &fmt::time(c.tempo_ms / 1000.0));
        }
    }
    let readout = if c.mst > 0 {
        MST_LABELS[c.mst].to_string()
    } else {
        format!("{:.0} ms", c.tempo_ms)
    };
    let r = f.rect("delay.tempo");
    f.caption_at(
        r,
        super::ctx::caption_drop("delay.tempo"),
        0,
        &readout,
        super::ctx::CAPTION,
    );
    f.legend(
        "delay.mst_clk",
        "MST CLK",
        c.mst > 0,
        "MST CLK (Shift + Tempo knob): turn clockwise to sync the delay to the Master Clock",
        None,
    );
    changed
}

/// Delay unit; edits go to the focused slot.
pub(super) fn delay(f: &mut Fx, st: &mut Stage4PanelState, slot: usize) {
    let mut changed = false;
    let mut tapped = false;
    let global = st.fx_ui.global_delay.is_some();
    let mut toggle_global = false;
    {
        let c = &mut st.fx_ui.chains[slot];

        let on = f.rect("delay.on");
        f.led_above(on, c.delay_on, LED_RED, LED_RED_DIM);
        match f.button(
            "delay.on",
            on,
            c.delay_on,
            true,
            "DELAY ON. Shift+click or hold: GLOBAL",
        ) {
            Some(Trigger::Primary) => {
                c.delay_on = !c.delay_on;
                changed = true;
            }
            Some(Trigger::Shifted) => toggle_global = true,
            _ => {}
        }
        f.legend(
            "delay.global",
            "GLOBAL",
            global,
            "GLOBAL (Shift + On): this delay applies to all Layers of all Sections",
            None,
        );

        changed |= tempo(f, c);

        let tip = format!("DELAY FEEDBACK ({:.0} %)", c.feedback * 100.0);
        changed |= f.knob("delay.feedback", &mut c.feedback, (0.0, 0.95), &tip, None);
        f.name("delay.feedback", "FEEDBACK");
        let tip = format!("DELAY DRY/WET ({:.0} %)", c.delay_mix * 100.0);
        changed |= f.knob("delay.dry_wet", &mut c.delay_mix, (0.0, 1.0), &tip, None);
        f.name("delay.dry_wet", "DRY WET");

        // Feedback effect selector (BYP, ENS, VIBE, CHOR, FLAM, SPACE).
        let fx = f.rect("delay.effects");
        let leds: Vec<(&str, bool, &str)> = DELAY_FX
            .iter()
            .enumerate()
            .map(|(i, ch)| (ch.label, i == c.delay_fx, ch.missing))
            .collect();
        // LED 0 is BYPASS (no LED printed); CHOR, VIBE, ENS beside FLAM, SPACE.
        f.led_list("delay.effects", fx, &leds, &[3, 2, 1], &[4, 5]);
        let tip = format!(
            "DELAY EFFECT: {}\nClick: next. Shift+click or hold: VARIATION",
            DELAY_FX[c.delay_fx.min(5)].label
        );
        match f.button("delay.effects", fx, c.delay_fx > 0, true, &tip) {
            Some(Trigger::Primary) => {
                c.delay_fx = cycle(c.delay_fx, DELAY_FX.len(), true);
                changed = true;
            }
            Some(Trigger::Shifted) => {
                c.delay_fx_variation = !c.delay_fx_variation;
                changed = true;
            }
            _ => {}
        }
        f.legend(
            "delay.effects_variation",
            "VAR",
            c.delay_fx_variation,
            "DELAY EFFECT VARIATION (Shift + selector)",
            None,
        );

        // Feedback filter (LP / HP / BP) and PING PONG.
        let flt = f.rect("delay.filter");
        let leds = [
            ("LP", c.delay_filter == DelayFilterMode::Lp, ""),
            ("HP", c.delay_filter == DelayFilterMode::Hp, ""),
            ("BP", c.delay_filter == DelayFilterMode::Bp, ""),
        ];
        f.led_list("delay.filter", flt, &leds, &[1, 0], &[2]);
        match f.button(
            "delay.filter",
            flt,
            c.delay_filter != DelayFilterMode::Off,
            true,
            "DELAY FILTER: click cycles off, LP, HP, BP. Shift+click or hold: PING PONG",
        ) {
            Some(Trigger::Primary) => {
                c.delay_filter = next_filter(c.delay_filter);
                changed = true;
            }
            Some(Trigger::Shifted) => {
                c.ping_pong = !c.ping_pong;
                changed = true;
            }
            _ => {}
        }
        f.legend(
            "delay.ping_pong",
            "PING PONG",
            c.ping_pong,
            "PING PONG (Shift + Filter): repeats alternate between left and right",
            None,
        );

        // TAP/SET and ANALOG.
        let tap = f.rect("delay.tap_set");
        match f.button(
            "delay.tap_set",
            tap,
            false,
            true,
            "TAP/SET: tap the tempo. Shift+click or hold: ANALOG mode",
        ) {
            Some(Trigger::Primary) => tapped = true,
            Some(Trigger::Shifted) => {
                c.analog = !c.analog;
                changed = true;
            }
            _ => {}
        }
        f.name("delay.tap_set", "TAP/SET");
        f.legend(
            "delay.analog",
            "ANALOG",
            c.analog,
            "ANALOG (Shift + Tap/Set): repeats change pitch with the tempo and distort slightly",
            None,
        );
    }
    if tapped {
        if let Some(gap) = st.fx_ui.tap.tap(f.now_ms) {
            let c = &mut st.fx_ui.chains[slot];
            c.tempo_ms = gap.clamp(TIME_MS.0, TIME_MS.1);
            c.mst = 0;
            changed = true;
        }
    }
    if toggle_global {
        let mut edits = st.fx_ui.toggle_global(Unit::Delay, slot);
        edits.extend(st.fx_ui.chains[slot].edits(Unit::Delay, slot as u8));
        f.send(edits);
    } else if changed {
        let edits = st.fx_ui.after_edit(slot, Unit::Delay);
        f.send(edits);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_button_cycles_through_all_modes_and_back() {
        let mut m = DelayFilterMode::Off;
        let mut seen = Vec::new();
        for _ in 0..4 {
            m = next_filter(m);
            seen.push(m);
        }
        assert_eq!(
            seen,
            [
                DelayFilterMode::Lp,
                DelayFilterMode::Hp,
                DelayFilterMode::Bp,
                DelayFilterMode::Off
            ]
        );
    }

    #[test]
    fn time_range_is_the_hardware_range() {
        assert_eq!(TIME_MS, (20.0, 1500.0));
    }
}
