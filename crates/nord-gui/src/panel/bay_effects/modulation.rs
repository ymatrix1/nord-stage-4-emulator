//! Mod 1 and Mod 2 units (manual p.49-50): ON, six-way type selector with LEDs, VARIATION (Mod 1
//! Wah: PED) on Shift + selector, RATE and AMOUNT knobs.

use super::ctx::{from_log, to_log, Fx};
use crate::panel::prelude::Stage4PanelState;
use crate::panel::ui_state::fx::{next_supported, ChainUi, Unit, MOD1, MOD2};
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
    /// Mod 1 only: PED switch and static Wah position.
    ped: Option<(&'a mut bool, &'a mut f32)>,
    wah: bool,
}

fn view(c: &mut ChainUi, unit: Unit) -> View<'_> {
    let wah = c.mod1_is_wah();
    match unit {
        Unit::Mod1 => View {
            on: &mut c.mod1_on,
            sel: &mut c.mod1_sel,
            rate: &mut c.mod1_rate,
            amount: &mut c.mod1_amount,
            var: &mut c.mod1_variation,
            ped: Some((&mut c.mod1_ped, &mut c.mod1_ped_pos)),
            wah,
        },
        _ => View {
            on: &mut c.mod2_on,
            sel: &mut c.mod2_sel,
            rate: &mut c.mod2_rate,
            amount: &mut c.mod2_amount,
            var: &mut c.mod2_variation,
            ped: None,
            wah: false,
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
        f.led_row(s.selector, sel_rect, &leds);
        let shifted = if v.ped.is_some() && v.wah {
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
                match (&mut v.ped, v.wah) {
                    (Some((ped, _)), true) => **ped = !**ped,
                    _ => *v.var = !*v.var,
                }
                changed = true;
            }
            _ => {}
        }
        let (text, active) = match &v.ped {
            Some((ped, _)) if v.wah => ("PED", **ped),
            Some(_) => ("VAR/PED", *v.var),
            None => ("VAR", *v.var),
        };
        let tip = format!("{} {shifted} (Shift + type selector)", s.title);
        f.legend(s.variation, text, active, &tip, None);

        // RATE: in Wah PED mode the knob is the static filter position (manual p.49).
        match v.ped {
            Some((ped, pos)) if *ped && v.wah => {
                let tip = "Wah PED position (no pedal attached: static filter position)";
                changed |= f.knob(s.rate, pos, (0.0, 1.0), tip, None);
                f.name(s.rate, "POS");
            }
            _ => {
                let mut n = to_log(*v.rate, s.rate_range.0, s.rate_range.1);
                let tip = format!("{} RATE ({:.2} Hz)", s.title, *v.rate);
                if f.knob(s.rate, &mut n, (0.0, 1.0), &tip, None) {
                    *v.rate = from_log(n, s.rate_range.0, s.rate_range.1);
                    changed = true;
                }
                f.name(s.rate, "RATE");
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
        assert!(!view(&mut c, Unit::Mod1).wah);
        c.mod1_sel = 4;
        assert!(view(&mut c, Unit::Mod1).wah);
        let v = view(&mut c, Unit::Mod2);
        assert!(v.ped.is_none() && !v.wah);
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
    fn six_choices_each_and_unsupported_ones_carry_a_reason() {
        for table in [
            MOD1.map(|c| (c.label, c.missing)),
            MOD2.map(|c| (c.label, c.missing)),
        ] {
            assert_eq!(table.len(), 6);
            assert_eq!(table.iter().filter(|(_, why)| !why.is_empty()).count(), 2);
        }
    }
}
