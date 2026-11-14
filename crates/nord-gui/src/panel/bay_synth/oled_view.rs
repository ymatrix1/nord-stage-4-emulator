//! What the Synth OLED shows: header, graphic and three soft labels, per page (manual p.27-38).
//!
//! `view_of` is pure (state in, description out) so the page contents are unit-tested; the
//! painter in `oled.rs` and `graphics.rs` only draws the description.

use super::oled_types::{Graphic, Icon, OledView, Soft};
use crate::panel::ui_state::{
    dial_defs, SynthPage, SynthUi, FILTER_TYPES, LFO_SHAPES, VIBRATO_DELAYS, WAVE_TYPES,
};

pub use crate::panel::param_hint::fmt::time as fmt_time;

fn icon_of(sui: &SynthUi) -> Icon {
    let l = sui.layer();
    match l.wave_type {
        1 | 2 => Icon::Fm,
        3 => Icon::Table,
        _ => match l.analog_entry() {
            Some(e) => match e.wave {
                nord_core::synth::VaWaveform::Sine => Icon::Sine,
                nord_core::synth::VaWaveform::Triangle => Icon::Triangle,
                nord_core::synth::VaWaveform::Saw => {
                    if e.exact {
                        Icon::Saw
                    } else {
                        Icon::Noise
                    }
                }
                nord_core::synth::VaWaveform::SuperSaw => Icon::Multi,
                nord_core::synth::VaWaveform::Pulse => Icon::Pulse(e.pulse.unwrap_or(0.5)),
            },
            None => Icon::Saw,
        },
    }
}

/// Three rows around the selected entry of the dial list in LIST view.
fn list_rows(sui: &SynthUi) -> [String; 3] {
    let l = sui.layer();
    let at = |d: i32| {
        let mut t = l.clone();
        t.wave_idx = (i32::from(l.wave_idx) + d).clamp(0, i32::from(l.wave_count()) - 1) as u8;
        t.wave_name()
    };
    [at(-1), at(0), at(1)]
}

fn soft(value: impl Into<String>, label: &'static str, dim: bool) -> Option<Soft> {
    Some(Soft {
        value: value.into(),
        label,
        dim,
    })
}

/// Build the OLED description for the current page.
pub fn view_of(sui: &SynthUi, sample_name: Option<&str>) -> OledView {
    let l = sui.layer();
    let defs = dial_defs(sui.page, l.osc_mode());
    let dim = |i: usize| defs[i].is_some_and(|d| d.unsupported.is_some());
    let label = |i: usize| defs[i].map_or("", |d| d.label);
    let on = |i: usize, v: String| defs[i].and_then(|_| soft(v, label(i), dim(i)));
    // Library browsing state of the focused layer; layer A falls back to the name the app loaded.
    let lib = &sui.samples;
    let picked = lib.name(sui.focus);
    let name = picked
        .clone()
        .or_else(|| sample_name.filter(|_| sui.focus == 0).map(str::to_string))
        .unwrap_or_else(|| "No .nsmp4 loaded".to_string());
    let cat = lib.category(sui.focus).unwrap_or("-").to_string();
    let pos = lib
        .position(sui.focus)
        .map_or_else(|| "-".to_string(), |(p, n)| format!("{p}({n})"));
    let bright = if l.sample_bright { "ON" } else { "OFF" }.to_string();
    let (title, graphic, vals): (&'static str, Graphic, [String; 3]) = match sui.page {
        SynthPage::Waveform if l.sample_mode && sui.info_view => (
            "SAMPLE INFO",
            Graphic::Lines(vec![
                name,
                format!("Category: {cat}"),
                "Nord Sample Library 4".to_string(),
            ]),
            [bright, cat, pos],
        ),
        SynthPage::Waveform if l.sample_mode && sui.list_view && picked.is_some() => (
            "SAMPLE LIBRARY",
            Graphic::List(lib.rows(sui.focus)),
            [bright, cat, pos],
        ),
        SynthPage::Waveform if l.sample_mode => (
            "SAMPLE LIBRARY",
            Graphic::Wave {
                name,
                ctrl: String::new(),
                icon: Icon::Sample,
            },
            [bright, cat, pos],
        ),
        SynthPage::Waveform if sui.list_view => (
            "OSC WAVEFORM",
            Graphic::List(list_rows(sui)),
            [
                WAVE_TYPES[usize::from(l.wave_type) % 4].to_string(),
                l.cat_name(),
                format!("{}({})", l.wave_idx + 1, l.wave_count()),
            ],
        ),
        SynthPage::Waveform => (
            "OSC WAVEFORM",
            Graphic::Wave {
                name: l.wave_name(),
                ctrl: format!("OSC CTRL: {:.1}", l.osc_ctrl * 10.0),
                icon: icon_of(sui),
            },
            [
                WAVE_TYPES[usize::from(l.wave_type) % 4].to_string(),
                l.cat_name(),
                format!("{}({})", l.wave_idx + 1, l.wave_count()),
            ],
        ),
        SynthPage::FilterType => {
            let ty = &FILTER_TYPES[usize::from(l.filter_type) % FILTER_TYPES.len()];
            (
                "FILTER",
                Graphic::Filter {
                    ty: l.filter_type,
                    cutoff: (l.cutoff_hz / 40.0).max(1.0).ln() / (15_000.0_f32 / 40.0).ln(),
                    res: l.resonance,
                },
                [
                    ty.name.to_string(),
                    ["OFF", "1/3", "2/3", "3/3"][usize::from(l.kb_track) % 4].to_string(),
                    ["OFF", "1", "2", "3"][usize::from(l.drive) % 4].to_string(),
                ],
            )
        }
        SynthPage::EnvOsc | SynthPage::EnvFilter | SynthPage::EnvAmp => {
            let e = match sui.page {
                SynthPage::EnvOsc => l.osc_env,
                SynthPage::EnvFilter => l.filter_env,
                _ => l.amp_env,
            };
            (
                sui.page.title(),
                Graphic::Env {
                    a: e.attack,
                    d: e.decay,
                    r: e.release,
                },
                [fmt_time(e.attack), fmt_time(e.decay), fmt_time(e.release)],
            )
        }
        SynthPage::LfoWave => (
            "LFO WAVEFORM",
            Graphic::Lfo(l.lfo_shape),
            [
                LFO_SHAPES[usize::from(l.lfo_shape) % 5].0.to_string(),
                String::new(),
                String::new(),
            ],
        ),
        SynthPage::Arp(n) => super::oled_arp::arp_page(sui, n),
        SynthPage::Vibrato => (
            "VIBRATO",
            Graphic::Vibrato {
                rate: l.vib_rate,
                amount: l.vib_amount,
            },
            [
                format!("{:.1} Hz", l.vib_rate),
                format!("{:.1}", l.vib_amount),
                fmt_time(VIBRATO_DELAYS[usize::from(l.vib_delay) % 4]),
            ],
        ),
        SynthPage::PitchSmp => (
            "OSC PITCH",
            Graphic::Big(format!("{:+} st {:+} ct", l.pitch_semi, l.fine_cents)),
            [
                format!("{:+} st", l.pitch_semi),
                format!("{:+} ct", l.fine_cents),
                ["Natural", "No Dyn", "Fast Atk"][usize::from(l.sample_dyn) % 3].to_string(),
            ],
        ),
        SynthPage::Extern => (
            "EXTERN",
            Graphic::Lines(vec![
                "MIDI out to external gear".to_string(),
                if l.extern_on {
                    "layer is Extern"
                } else {
                    "layer is internal"
                }
                .to_string(),
            ]),
            l.extern_vals.map(|v| v.to_string()),
        ),
    };
    OledView {
        title,
        layer: ['A', 'B', 'C'][sui.focus.min(2)],
        graphic,
        soft: [
            on(0, vals[0].clone()),
            on(1, vals[1].clone()),
            on(2, vals[2].clone()),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn soft_labels(v: &OledView) -> Vec<&'static str> {
        v.soft.iter().flatten().map(|s| s.label).collect()
    }

    #[test]
    fn default_view_is_the_analog_waveform_page() {
        let v = view_of(&SynthUi::new(), None);
        assert_eq!(v.title, "OSC WAVEFORM");
        assert_eq!(v.layer, 'A');
        assert_eq!(soft_labels(&v), ["TYPE", "CAT", "WAVE"]);
        assert_eq!(v.soft[0].as_ref().unwrap().value, "ANALOG");
        assert_eq!(v.soft[1].as_ref().unwrap().value, "PURE");
        assert_eq!(v.soft[2].as_ref().unwrap().value, "3(7)");
        assert!(matches!(&v.graphic, Graphic::Wave { name, .. } if name == "Saw"));
    }

    #[test]
    fn multi_saw_page_matches_the_manual_example() {
        let mut s = SynthUi::new();
        s.turn_dial(1, 5, false); // MULTI is the sixth category (SYNC sits at 2)
        s.turn_dial(2, -9, false);
        s.turn_dial(2, 1, false);
        let v = view_of(&s, None);
        assert_eq!(v.soft[1].as_ref().unwrap().value, "MULTI");
        assert!(
            matches!(&v.graphic, Graphic::Wave { name, .. } if name.starts_with("Multi Saw 8ve"))
        );
    }

    #[test]
    fn sample_mode_uses_bright_cat_sample_labels_and_no_dial_is_dimmed() {
        let mut s = SynthUi::new();
        s.layer_mut().sample_mode = true;
        let v = view_of(&s, Some("Grand Piano"));
        assert_eq!(v.title, "SAMPLE LIBRARY");
        assert_eq!(soft_labels(&v), ["BRIGHT", "CAT", "SAMPLE"]);
        assert!(v.soft.iter().flatten().all(|x| !x.dim));
        assert!(
            matches!(&v.graphic, Graphic::Wave { name, .. } if name == "Grand Piano"),
            "layer A shows the sample the app loaded"
        );
        assert_eq!(v.soft[1].as_ref().unwrap().value, "-");
        s.info_view = true;
        assert_eq!(view_of(&s, None).title, "SAMPLE INFO");
    }

    #[test]
    fn sample_pages_show_category_sample_list_and_info_from_the_library() {
        let mut s = SynthUi::new();
        s.samples.set_catalog(&[
            "Samp Lib/Bass/Fretless.nsmp4".into(),
            "Samp Lib/Bass/Slap.nsmp4".into(),
            "Samp Lib/Bass/Upright.nsmp4".into(),
        ]);
        s.layer_mut().sample_mode = true;
        s.turn_dial(2, 2, false);
        let v = view_of(&s, Some("Stale"));
        assert_eq!(v.soft[1].as_ref().unwrap().value, "Bass");
        assert_eq!(v.soft[2].as_ref().unwrap().value, "2(3)");
        assert!(matches!(&v.graphic, Graphic::Wave { name, .. } if name == "Slap"));
        s.turn_dial(2, 0, false);
        s.turn_dial(2, 1, true); // Shift+dial: LIST view
        let v = view_of(&s, None);
        assert_eq!(
            v.graphic,
            Graphic::List(["Slap".into(), "Upright".into(), String::new()])
        );
        s.list_view = false;
        s.info_view = true;
        let v = view_of(&s, None);
        assert_eq!(v.title, "SAMPLE INFO");
        assert!(
            matches!(&v.graphic, Graphic::Lines(l) if l[0] == "Upright" && l[1].contains("Bass"))
        );
        s.focus_layer(1);
        s.layer_mut().sample_mode = true;
        s.info_view = true;
        assert!(
            matches!(&view_of(&s, Some("Stale")).graphic, Graphic::Lines(l) if l[0] == "No .nsmp4 loaded")
        );
    }

    #[test]
    fn list_view_shows_three_rows_around_the_selection() {
        let mut s = SynthUi::new();
        s.turn_dial(2, 1, true);
        let v = view_of(&s, None);
        assert_eq!(
            v.graphic,
            Graphic::List(["Saw".into(), "Square".into(), "Pulse 33".into()])
        );
    }

    #[test]
    fn filter_page_shows_type_kbd_track_and_drive() {
        let mut s = SynthUi::new();
        s.open(SynthPage::FilterType);
        s.turn_dial(1, 2, false);
        let v = view_of(&s, None);
        assert_eq!(v.title, "FILTER");
        assert_eq!(v.soft[0].as_ref().unwrap().value, "LP M");
        assert_eq!(v.soft[1].as_ref().unwrap().value, "2/3");
        assert_eq!(soft_labels(&v), ["TYPE", "KBD TRK", "DRIVE"]);
    }

    #[test]
    fn envelope_pages_show_attack_decay_release_times() {
        let mut s = SynthUi::new();
        for (p, t) in [
            (SynthPage::EnvOsc, "OSC ENVELOPE"),
            (SynthPage::EnvFilter, "FILTER ENVELOPE"),
            (SynthPage::EnvAmp, "AMP ENVELOPE"),
        ] {
            s.page = p;
            let v = view_of(&s, None);
            assert_eq!(v.title, t);
            assert_eq!(v.soft[0].as_ref().unwrap().value, "5 ms");
            assert_eq!(v.soft[1].as_ref().unwrap().value, "300 ms");
            assert!(matches!(v.graphic, Graphic::Env { .. }));
        }
    }

    #[test]
    fn vibrato_page_matches_the_manual_values() {
        let mut s = SynthUi::new();
        s.open(SynthPage::Vibrato);
        let v = view_of(&s, None);
        assert_eq!(soft_labels(&v), ["RATE", "AMOUNT", "DLY TIME"]);
        assert_eq!(v.soft[0].as_ref().unwrap().value, "5.0 Hz");
        assert_eq!(v.soft[2].as_ref().unwrap().value, "200 ms");
    }

    #[test]
    fn pitch_page_and_lfo_page_and_extern_page() {
        let mut s = SynthUi::new();
        s.open(SynthPage::PitchSmp);
        s.turn_dial(0, 3, false);
        let v = view_of(&s, None);
        assert_eq!(v.soft[0].as_ref().unwrap().value, "+3 st");
        assert!(v.soft[2].is_none(), "third dial unused in ANALOG mode");
        s.open(SynthPage::LfoWave);
        let v = view_of(&s, None);
        assert_eq!(v.soft[0].as_ref().unwrap().value, "Triangle");
        assert!(v.soft[1].is_none() && v.soft[2].is_none());
        s.open(SynthPage::Extern);
        let v = view_of(&s, None);
        assert_eq!(v.title, "EXTERN");
        assert!(v.soft.iter().flatten().all(|x| !x.dim), "Extern dials work");
        assert_eq!(v.soft[2].as_ref().unwrap().label, "PROGRAM");
    }

    #[test]
    fn time_format_matches_the_manual_style() {
        assert_eq!(fmt_time(0.103), "103 ms");
        assert_eq!(fmt_time(1.5), "1.5 s");
        assert_eq!(fmt_time(11.0), "11 s");
    }

    #[test]
    fn focus_letter_follows_the_focused_layer() {
        let mut s = SynthUi::new();
        s.focus_layer(2);
        assert_eq!(view_of(&s, None).layer, 'C');
    }
}
