//! Keybed (manual p.15 split LEDs; 88-key Hammer Action per the product name).

use crate::types::{ControlEntry, ControlKind as K, Section};

pub fn controls() -> Vec<ControlEntry> {
    let s = Section::Keybed;
    vec![
        ControlEntry::new("keybed.keys", s, K::Key, "88 KEYS (Hammer Action)", 15)
            .assumed("key count from the model name; the manual text does not state it")
            .ui("notes enter through the engine MIDI path"),
        ControlEntry::new(
            "keybed.split_leds",
            s,
            K::Led,
            "SPLIT POSITION LEDs (3, green)",
            15,
        )
        .ui("green LEDs above the keyboard mark active split points"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keybed_has_keys_and_split_leds() {
        let c = controls();
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].kind, K::Key);
    }
}
