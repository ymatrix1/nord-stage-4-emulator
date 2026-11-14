//! Mod 1 and Mod 2 units (manual p.49-50): ON, six-way type selector with LEDs, VARIATION (Mod 1
//! Wah and Pump: PED) on Shift + selector, RATE and AMOUNT knobs. Shift + turn of the Mod 1 RATE
//! enters MST CLK (A-Pan, Trem, Wah, Pump).

use super::ctx::{from_log, to_log, Fx};
use super::ids::NO_CLOCK;
use crate::panel::param_hint::fmt;
use crate::panel::prelude::Stage4PanelState;
use crate::panel::ui_state::fx::{next_supported, ChainUi, Unit, MOD1, MOD2, MST_LABELS};
use nord_panel::behaviour::Trigger;

/// Ids and tables of one modulation unit.
struct Spec {
    unit: Unit,
    title: &'static str,
    on: &'static str,
    selector: &'static str,
    variation: &'static str,
    rate: &'static str,
    amount: &'static str,
    /// Shifted MST CLK legend (Mod 1 only).
    mst_clk: Option<&'static str>,
    /// `(label, missing)` per selector LED.
    choices: [(&'static str, &'static str); 6],
    next: fn(usize) -> usize,
    rate_range: (f32, f32),
}

/// Mutable view of the fields of one modulation unit in a chain.
struct View<'a> {
    on: &'a mut bool,
    sel: &'a mut usize,
    rate: &'a mut f32,
    amount: &'a mut f32,
    var: &'a mut bool,
    /// Mod 1 only: PED switch and static pedal position (Wah filter position, Pump pedal).
    ped: Option<(&'a mut bool, &'a mut f32)>,
    /// The selected type has a PED mode (Wah, Pump).
    has_ped: bool,
    /// Mod 1 only: MST CLK step, and whether the selected type can follow it.
    mst: Option<(&'a mut usize, bool)>,
    /// The selected type is A-WAH (the Rate knob is SENS).
    auto_wah: bool,
}

fn view(c: &mut ChainUi, unit: Unit) -> View<'_> {
    let (has_ped, clock) = (c.mod1_has_ped(), c.mod1_has_clock());
    let auto_wah = c.mod1_selected() == nord_core::fx::Mod1Type::AutoWah;
    match unit {
        Unit::Mod1 => View {
            on: &mut c.mod1_on,
            sel: &mut c.mod1_sel,
            rate: &mut c.mod1_rate,
            amount: &mut c.mod1_amount,
            var: &mut c.mod1_variation,
            ped: Some((&mut c.mod1_ped, &mut c.mod1_ped_pos)),
            has_ped,
            mst: Some((&mut c.mod1_mst, clock)),
            auto_wah,
        },
        _ => View {
            on: &mut c.mod2_on,
            sel: &mut c.mod2_sel,
            rate: &mut c.mod2_rate,
            amount: &mut c.mod2_amount,
            var: &mut c.mod2_variation,
            ped: None,
            has_ped: false,
            mst: None,
            auto_wah: false,
        },
    }
}

fn unit(f: &mut Fx, st: &mut Stage4PanelState, slot: usize, s: &Spec) {
    let mut changed = false;
    {
        let mut v = view(&mut st.fx_ui.chains[slot], s.unit);
        let on_rect = f.rect(s.on);
        f.led_above(
            on_rect,
            *v.on,
            crate::style::LED_RED,
            crate::style::LED_RED_DIM,
        );
        let tip = format!("{} ON", s.title);
        if f.button(s.on, on_rect, *v.on, false, &tip).is_some() {
            *v.on = !*v.on;
            changed = true;
        }

        let sel_rect = f.rect(s.selector);
        let leds: Vec<(&str, bool, &str)> = s
            .choices
            .iter()
            .enumerate()
            .map(|(i, (name, missing))| (*name, *v.on && i == *v.sel, *missing))
            .collect();
        // Printed as on the panel: the left column runs from the third LED (top) down to the
        // first, the right column continues downwards from the fourth.
        f.led_list(s.selector, sel_rect, &leds, &[2, 1, 0], &[3, 4, 5]);
        let shifted = if v.ped.is_some() && v.has_ped {
            "PED"
        } else {
            "VARIATION"
        };
        let tip = format!(
            "{} type: {}\nClick: next type. Shift+click or hold: {shifted}",
            s.title, s.choices[*v.sel].0
        );
        match f.button(s.selector, sel_rect, *v.on, true, &tip) {
            Some(Trigger::Primary) => {
                *v.sel = (s.next)(*v.sel);
                changed = true;
            }
            Some(Trigger::Shifted) => {
                match (&mut v.ped, v.has_ped) {
                    (Some((ped, _)), true) => **ped = !**ped,
                    _ => *v.var = !*v.var,
                }
                changed = true;
            }
            _ => {}
        }
        let (text, active) = match &v.ped {
            Some((ped, _)) if v.has_ped => ("PED", **ped),
            Some(_) => ("VAR/PED", *v.var),
            None => ("VAR", *v.var),
        };
        let tip = format!("{} {shifted} (Shift + type selector)", s.title);
        f.legend(s.variation, text, active, &tip, None);
        if let (Some(id), Some((mst, clock))) = (s.mst_clk, &v.mst) {
            let why = if *clock { None } else { Some(NO_CLOCK) };
            f.legend(
                id,
                "MST CLK",
                **mst > 0,
                "MST CLK (Shift + turn RATE): A-PAN, TREM, WAH and PUMP follow the Master Clock",
                why,
            );
        }

        // RATE: in PED mode the knob is the static pedal position (manual p.49); Shift + turn (or
        // any turn while MST CLK is on) selects the Master Clock division (manual p.50).
        match (v.ped, v.mst) {
            (Some((ped, pos)), _) if *ped && v.has_ped => {
                let tip = "PED position (no pedal attached: static position; Pump ducks when it rises through half)";
                if f.knob(s.rate, pos, (0.0, 1.0), tip, None) {
                    changed = true;
                    f.hint(s.rate, &format!("Pos {:.0} %", *pos * 100.0));
                }
                f.name(s.rate, "POS");
            }
            (_, Some((mst, true))) if *mst > 0 || f.shift => {
                let steps = (MST_LABELS.len() - 1) as f32;
                let mut x = *mst as f32;
                let tip = format!("{} MST CLK division: {}", s.title, MST_LABELS[*mst]);
                if f.knob(s.rate, &mut x, (0.0, steps), &tip, None) {
                    let next = x.round().clamp(0.0, steps) as usize;
                    changed |= next != *mst;
                    *mst = next;
                    f.hint(s.rate, MST_LABELS[next]);
                }
                f.name(s.rate, MST_LABELS[*mst]);
            }
            _ => {
                let mut n = to_log(*v.rate, s.rate_range.0, s.rate_range.1);
                let tip = if v.auto_wah {
                    format!(
                        "A-WAH SENS: sensitivity and range of the envelope follower ({:.2})",
                        *v.rate
                    )
                } else {
                    format!(
                        "{} RATE ({:.2} Hz). Shift + turn: MST CLK",
                        s.title, *v.rate
                    )
                };
                if f.knob(s.rate, &mut n, (0.0, 1.0), &tip, None) {
                    *v.rate = from_log(n, s.rate_range.0, s.rate_range.1);
                    changed = true;
                    let text = if v.auto_wah {
                        format!("Sens {:.2}", *v.rate)
                    } else {
                        fmt::hz(*v.rate)
                    };
                    f.hint(s.rate, &text);
                }
                f.name(s.rate, if v.auto_wah { "SENS" } else { "RATE" });
            }
        }
        let tip = format!("{} AMOUNT ({:.0} %)", s.title, *v.amount * 100.0);
        changed |= f.knob(s.amount, v.amount, (0.0, 1.0), &tip, None);
        f.name(s.amount, "AMOUNT");
    }
    if changed {
        let edits = st.fx_ui.after_edit(slot, s.unit);
        f.send(edits);
    }
}

/// Mod 1 (A-PAN, TREM, RM, A-WAH, WAH, PUMP).
pub(super) fn mod1(f: &mut Fx, st: &mut Stage4PanelState, slot: usize) {
    let spec = Spec {
        unit: Unit::Mod1,
        title: "MOD 1",
        on: "mod1.on",
        selector: "mod1.selector",
        variation: "mod1.variation",
        rate: "mod1.rate",
        amount: "mod1.amount",
        mst_clk: Some("mod1.mst_clk"),
        choices: MOD1.map(|c| (c.label, c.missing)),
        next: |i| next_supported(&MOD1, i),
        rate_range: (0.1, 20.0),
    };
    unit(f, st, slot, &spec);
}

/// Mod 2 (PHAS, FLANG, CHOR, VIBE, ENS, SPIN).
pub(super) fn mod2(f: &mut Fx, st: &mut Stage4PanelState, slot: usize) {
    let spec = Spec {
        unit: Unit::Mod2,
        title: "MOD 2",
        on: "mod2.on",
        selector: "mod2.selector",
        variation: "mod2.variation",
        rate: "mod2.rate",
        amount: "mod2.amount",
        mst_clk: None,
        choices: MOD2.map(|c| (c.label, c.missing)),
        next: |i| next_supported(&MOD2, i),
        rate_range: (0.1, 10.0),
    };
    unit(f, st, slot, &spec);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_mod1_has_a_ped_view_and_it_knows_about_wah() {
        let mut c = ChainUi::default();
        assert!(view(&mut c, Unit::Mod1).ped.is_some());
        assert!(!view(&mut c, Unit::Mod1).has_ped);
        for sel in [4, 5] {
            c.mod1_sel = sel;
            assert!(view(&mut c, Unit::Mod1).has_ped, "WAH and PUMP have PED");
        }
        c.mod1_sel = 3;
        assert!(view(&mut c, Unit::Mod1).auto_wah);
        let v = view(&mut c, Unit::Mod2);
        assert!(v.ped.is_none() && !v.has_ped && v.mst.is_none());
    }

    #[test]
    fn views_address_the_fields_of_their_own_unit() {
        let mut c = ChainUi::default();
        *view(&mut c, Unit::Mod2).amount = 0.9;
        *view(&mut c, Unit::Mod1).rate = 7.0;
        assert_eq!((c.mod2_amount, c.mod1_rate), (0.9, 7.0));
        assert_eq!((c.mod1_amount, c.mod2_rate), (0.4, 0.8));
    }

    #[test]
    fn six_choices_each_and_every_one_is_selectable() {
        for table in [
            MOD1.map(|c| (c.label, c.missing)),
            MOD2.map(|c| (c.label, c.missing)),
        ] {
            assert_eq!(table.len(), 6);
            assert!(table.iter().all(|(_, why)| why.is_empty()));
        }
    }
}
