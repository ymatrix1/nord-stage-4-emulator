//! Synth section (photo crop x 1029..1401): faders, OLED with its three
//! dials, then the Arpeggiator/Gate, Voice, Vibrato, LFO, Oscillators,
//! Filter, Amp and Unison areas.
//!
//! ASSUMED: read from the 2000 px photo; higher-resolution photo confirms.

use super::{px, Rect};

pub fn rect(id: &str) -> Option<Rect> {
    Some(match id {
        // Header, layer strip.
        "synth.on" => px(1141.0, 121.0, 1164.0, 134.0),
        "synth.fx_focus_led" => px(1113.0, 127.0, 1118.0, 132.0),
        "synth.fader.a" => px(1033.0, 137.0, 1046.0, 205.0),
        "synth.fader.b" => px(1063.0, 137.0, 1077.0, 205.0),
        "synth.fader.c" => px(1093.0, 137.0, 1107.0, 205.0),
        "synth.ladder.a" => px(1051.0, 140.0, 1060.0, 200.0),
        "synth.ladder.b" => px(1082.0, 140.0, 1091.0, 200.0),
        "synth.ladder.c" => px(1111.0, 140.0, 1120.0, 200.0),
        "synth.focus_led.a" => px(1032.0, 209.0, 1038.0, 215.0),
        "synth.aux_led.a" => px(1050.0, 209.0, 1056.0, 215.0),
        "synth.focus_led.b" => px(1063.0, 209.0, 1069.0, 215.0),
        "synth.aux_led.b" => px(1080.0, 209.0, 1086.0, 215.0),
        "synth.focus_led.c" => px(1093.0, 209.0, 1099.0, 215.0),
        "synth.aux_led.c" => px(1111.0, 209.0, 1117.0, 215.0),
        "synth.layer.a" => px(1034.0, 225.0, 1057.0, 237.0),
        "synth.layer.b" => px(1065.0, 225.0, 1088.0, 237.0),
        "synth.layer.c" => px(1094.0, 225.0, 1118.0, 237.0),
        "synth.octave_down" => px(1038.0, 290.0, 1060.0, 303.0),
        "synth.octave_up" => px(1062.0, 290.0, 1084.0, 303.0),
        "synth.zone_leds" => px(1050.0, 309.0, 1073.0, 315.0),
        "synth.kb_hold" => px(1035.0, 259.0, 1058.0, 272.0),
        "synth.arp.run" => px(1065.0, 259.0, 1088.0, 272.0),
        // OLED, dials, mode, waveform.
        "synth.oled" => px(1143.0, 140.0, 1227.0, 183.0),
        "synth.dial.left" => px(1134.0, 212.0, 1156.0, 234.0),
        "synth.dial.middle" => px(1175.0, 212.0, 1197.0, 234.0),
        "synth.dial.right" => px(1215.0, 212.0, 1237.0, 234.0),
        "synth.mode" => px(1240.0, 160.0, 1263.0, 173.0),
        "synth.waveform" => px(1250.0, 200.0, 1264.0, 226.0),
        // Arpeggiator / Gate.
        "synth.arp.rate" => px(1278.0, 148.0, 1300.0, 170.0),
        "synth.arp.mode" => px(1312.0, 163.0, 1335.0, 176.0),
        "synth.arp.range" => px(1346.0, 148.0, 1368.0, 170.0),
        "synth.arp.menu" => px(1378.0, 152.0, 1392.0, 178.0),
        // Voice and Vibrato.
        "synth.voice.mode" => px(1277.0, 216.0, 1300.0, 229.0),
        "synth.voice.glide" => px(1312.0, 201.0, 1334.0, 223.0),
        "synth.vibrato.selector" => px(1348.0, 217.0, 1371.0, 230.0),
        "synth.vibrato.menu" => px(1378.0, 206.0, 1392.0, 232.0),
        // LFO.
        "synth.lfo.waveform" => px(1101.0, 254.0, 1124.0, 267.0),
        "synth.lfo.rate" => px(1104.0, 277.0, 1126.0, 299.0),
        "synth.lfo.mod_amt" => px(1142.0, 251.0, 1164.0, 273.0),
        "synth.lfo.dest" => px(1141.0, 300.0, 1164.0, 313.0),
        // Oscillators.
        "synth.osc.pitch_smp" => px(1182.0, 255.0, 1205.0, 268.0),
        "synth.osc.envelope" => px(1225.0, 255.0, 1248.0, 268.0),
        "synth.osc.ctrl" => px(1182.0, 283.0, 1206.0, 307.0),
        "synth.osc.env_amt" => px(1227.0, 284.0, 1247.0, 304.0),
        // Filter.
        "synth.filter.type" => px(1265.0, 255.0, 1288.0, 268.0),
        "synth.filter.envelope" => px(1300.0, 255.0, 1323.0, 268.0),
        "synth.filter.env_amt" => px(1335.0, 247.0, 1355.0, 269.0),
        "synth.filter.freq" => px(1270.0, 282.0, 1294.0, 306.0),
        "synth.filter.res" => px(1314.0, 284.0, 1334.0, 304.0),
        // Amp and Unison.
        "synth.amp.envelope" => px(1370.0, 255.0, 1393.0, 268.0),
        "synth.unison" => px(1369.0, 300.0, 1392.0, 313.0),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_dials_left_to_right_under_the_oled() {
        let o = rect("synth.oled").unwrap();
        let l = rect("synth.dial.left").unwrap();
        let m = rect("synth.dial.middle").unwrap();
        let r = rect("synth.dial.right").unwrap();
        assert!(l.x < m.x && m.x < r.x);
        assert!(l.y > o.bottom());
    }

    #[test]
    fn three_faders_for_layers_a_b_c() {
        let a = rect("synth.fader.a").unwrap();
        let c = rect("synth.fader.c").unwrap();
        assert!(a.x < c.x);
        assert!(rect("synth.fader.d").is_none());
        assert!(rect("organ.on").is_none());
    }
}
