//! Master level, wheels, pitch stick and the Rotary Speaker box
//! (manual p.11 panel overview, p.53 Rotary Speaker).

use crate::types::{ControlEntry, ControlKind as K, Section};

pub fn controls() -> Vec<ControlEntry> {
    let s = Section::Master;
    let e = ControlEntry::new;
    vec![
        e("master.level", s, K::Knob, "MASTER LEVEL", 11)
            .assumed("panel overview figure on p.11; no running text")
            .bound("master_level"),
        e("master.mod_wheel", s, K::Wheel, "MODULATION WHEEL", 38)
            .assumed("named as a Morph source on p.38; physical wheel not described")
            .bound("mod_wheel"),
        e("master.pitch_stick", s, K::Stick, "PITCH STICK", 18)
            .ui("MIDI pitch bend, no host parameter; range set per layer by PSTICK"),
        e("rotary.drive", s, K::Knob, "ROTARY DRIVE", 53).bound("rotary_drive"),
        e("rotary.speed", s, K::Button, "SLOW/STOP - FAST", 53)
            .morph()
            .bound("rotary_speed_fast"),
        e("rotary.stop_mode", s, K::Button, "STOP MODE", 53).bound("organx_stop_mode"),
        e("rotary.stop_angle", s, K::Button, "ANGLE (rotor stop position)", 53)
            .shift()
            .bound("organy_stop_angle_{n}"),
        e("rotary.angle", s, K::Button, "ANGLE (mic separation)", 53)
            .shift()
            .assumed("manual p.53 names only the stop position list; wide/narrow mic is kept as a row of the ANGLE popup")
            .bound("organy_mic_angle_{n}"),
        e("rotary.organ", s, K::Button, "ORGAN (route to rotary)", 53)
            .bound("organy_rotary_on_{n}"),
        e("rotary.close_mic", s, K::Button, "CLOSE MIC", 53)
            .shift()
            .bound("organx_close_mic"),
        e("rotary.speed_leds", s, K::Led, "SLOW / FAST LEDs", 53)
            .assumed("LED labels from the panel photo")
            .ui("derived from the speed state"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn master_has_eleven_controls() {
        assert_eq!(controls().len(), 11);
    }

    #[test]
    fn rotary_speed_is_morphable() {
        let c = controls();
        assert!(c.iter().find(|x| x.id == "rotary.speed").unwrap().morphable);
    }
}
