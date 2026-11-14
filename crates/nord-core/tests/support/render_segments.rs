//! Golden-render segments used by `tests/golden_render.rs`.
//!
//! One deterministic stereo f32 stream per engine (full engine, synth, organ, piano, FX sweep).
//! Concatenated in this order they are the `render_hash` file, so the SHA-256 of the segments
//! joined is the `render_hash` value. Derived numbers only; no audio is stored.
#![allow(dead_code)]

use nord_core::engine::Stage4Engine;
use nord_core::fx::{
    AmpCabinet, DelayFeedbackFx, DelayFilterMode, DelaySubdivision, LayerFxChain, MasterBus,
    Mod1Type, Mod2Type, ReverbColor, ReverbModel,
};
use nord_core::midi::MidiEvent;
use nord_core::organ::OrganLayer;
use nord_core::piano::PianoLayer;
use nord_core::render::{render_timeline, TimedEvent};
use nord_core::synth::{SampleTimbre, SynthLayer, SynthOscMode, SynthSampleSound};
use std::f32::consts::TAU;
use std::sync::Arc;

pub const FS: f32 = 48_000.0;

pub fn push(out: &mut Vec<u8>, frame: [f32; 2]) {
    out.extend_from_slice(&frame[0].to_le_bytes());
    out.extend_from_slice(&frame[1].to_le_bytes());
}

pub fn sample_sound() -> Arc<SynthSampleSound> {
    let pcm: Vec<f32> = (0..2048)
        .map(|i| {
            let ph = i as f32 / 40.0;
            (TAU * ph).sin() + 0.4 * (3.0 * TAU * ph).sin() + 0.1 * (11.0 * TAU * ph).sin()
        })
        .collect();
    Arc::new(SynthSampleSound::new_synthetic(
        "HashSample",
        60,
        &pcm,
        FS,
        Some((512, 1536)),
        32,
    ))
}

pub fn sample_layer(note: u8) -> SynthLayer {
    let mut s = SynthLayer::new(FS);
    s.osc_mode = SynthOscMode::Sample;
    s.sample_timbre = SampleTimbre::Bright;
    s.cutoff_hz = 16_000.0;
    s.load_sample(Some(sample_sound()));
    s.note_on(note, 110);
    s
}

fn engine_segment() -> Vec<u8> {
    let mut out = Vec::new();
    let mut engine = Stage4Engine::new(FS);
    let ev = |frame: usize, e: MidiEvent| TimedEvent { frame, event: e };
    let on = |n: u8| MidiEvent::NoteOn {
        channel: 0,
        note: n,
        velocity: 100,
    };
    let off = |n: u8| MidiEvent::NoteOff {
        channel: 0,
        note: n,
        velocity: 0,
    };
    let cc = |c: u8, v: u8| MidiEvent::ControlChange {
        channel: 0,
        controller: c,
        value: v,
    };
    let events = [
        ev(0, on(48)),
        ev(0, on(60)),
        ev(0, on(64)),
        ev(0, on(67)),
        ev(10_000, cc(1, 90)),
        ev(20_000, off(60)),
        ev(24_000, on(72)),
        ev(30_000, cc(64, 127)),
        ev(40_000, off(64)),
    ];
    let (l, r) = render_timeline(&mut engine, &events, 60_000);
    for (a, b) in l.iter().zip(&r) {
        push(&mut out, [*a, *b]);
    }
    out
}

fn synth_segment() -> Vec<u8> {
    let mut out = Vec::new();
    for mode in [
        SynthOscMode::Analog,
        SynthOscMode::Wavetable,
        SynthOscMode::Fm,
    ] {
        let mut s = SynthLayer::new(FS);
        s.osc_mode = mode;
        s.note_on(57, 100);
        s.note_on(64, 100);
        for _ in 0..12_000 {
            push(&mut out, s.render_frame());
        }
    }
    for note in [48u8, 60, 67, 72, 84] {
        let mut s = sample_layer(note);
        for _ in 0..12_000 {
            push(&mut out, s.render_frame());
        }
    }
    out
}

fn organ_segment() -> Vec<u8> {
    let mut out = Vec::new();
    let mut organ = OrganLayer::new(FS);
    organ.note_on(60);
    organ.note_on(67);
    for _ in 0..12_000 {
        push(&mut out, organ.render_frame());
    }
    out
}

fn piano_segment() -> Vec<u8> {
    let mut out = Vec::new();
    let mut piano = PianoLayer::new(FS);
    piano.note_on(60, 100);
    piano.note_on(72, 80);
    for _ in 0..12_000 {
        push(&mut out, piano.render_frame());
    }
    out
}

fn fx_segment() -> Vec<u8> {
    let mut out = Vec::new();
    fx_sweep(&mut out);
    out
}

/// Names of the segments in `render_hash` order.
pub const SEGMENT_NAMES: [&str; 5] = ["engine", "synth", "organ", "piano", "fx"];

/// Renders every segment (little-endian f32 stereo frames), in [`SEGMENT_NAMES`] order.
pub fn segments() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("engine", engine_segment()),
        ("synth", synth_segment()),
        ("organ", organ_segment()),
        ("piano", piano_segment()),
        ("fx", fx_segment()),
    ]
}

/// Sweeps every Mod1 x Mod2 pair while cycling amp, delay and reverb settings so each FX stage
/// is exercised with non-default parameters.
fn fx_sweep(out: &mut Vec<u8>) {
    let mods1 = [
        Mod1Type::Off,
        Mod1Type::Tremolo,
        Mod1Type::AutoPan,
        Mod1Type::RingMod,
        Mod1Type::Wah,
    ];
    let mods2 = [
        Mod2Type::Off,
        Mod2Type::Chorus,
        Mod2Type::Flanger,
        Mod2Type::Phaser,
        Mod2Type::Vibe,
    ];
    let amps = [
        AmpCabinet::Off,
        AmpCabinet::Small,
        AmpCabinet::Jc,
        AmpCabinet::Twin,
        AmpCabinet::ToRotary,
        AmpCabinet::LpFilter,
        AmpCabinet::HpFilter,
    ];
    let dfx = [
        DelayFeedbackFx::Off,
        DelayFeedbackFx::Ensemble,
        DelayFeedbackFx::Vibe,
        DelayFeedbackFx::Chorus,
        DelayFeedbackFx::Flam,
        DelayFeedbackFx::Space,
    ];
    let dfilt = [
        DelayFilterMode::Off,
        DelayFilterMode::Hp,
        DelayFilterMode::Lp,
        DelayFilterMode::Bp,
    ];
    let revs = [
        ReverbModel::Off,
        ReverbModel::Booth,
        ReverbModel::Room,
        ReverbModel::Stage,
        ReverbModel::Spring,
        ReverbModel::Hall,
        ReverbModel::Cathedral,
    ];
    let cols = [
        ReverbColor::Normal,
        ReverbColor::Bright,
        ReverbColor::Dark,
        ReverbColor::Chorale,
    ];
    let subs = [
        DelaySubdivision::Quarter,
        DelaySubdivision::DottedEighth,
        DelaySubdivision::Eighth,
        DelaySubdivision::Sixteenth,
    ];
    let mut bus = MasterBus::default();
    let mut k = 0usize;
    for m1 in mods1 {
        for m2 in mods2 {
            let mut fx = LayerFxChain::new(FS);
            fx.mod1_type = m1;
            fx.mod2_type = m2;
            fx.mod1_amount = 0.6;
            fx.mod2_amount = 0.6;
            fx.amp_cabinet = amps[k % amps.len()];
            fx.amp_variation = k % 2 == 1;
            fx.drive_amount = 0.2 + 0.1 * (k % 5) as f32;
            fx.eq_bass_db = 3.0;
            fx.eq_mid_db = -4.0;
            fx.eq_mid_freq_hz = 900.0;
            fx.eq_treble_db = 2.0;
            fx.comp_enabled = !k.is_multiple_of(3);
            fx.comp_amount = 0.5;
            fx.comp_fast = k.is_multiple_of(2);
            fx.delay_enabled = true;
            fx.delay_time_ms = 180.0 + 20.0 * (k % 7) as f32;
            fx.delay_clock_sync = k % 4 == 3;
            fx.delay_subdivision = subs[k % subs.len()];
            fx.delay_analog = k % 3 == 1;
            fx.delay_ping_pong = k.is_multiple_of(2);
            fx.delay_filter = dfilt[k % dfilt.len()];
            fx.delay_fx = dfx[k % dfx.len()];
            fx.delay_feedback = 0.4;
            fx.delay_mix = 0.35;
            fx.reverb_model = revs[k % revs.len()];
            fx.reverb_color = cols[k % cols.len()];
            fx.reverb_mix = 0.3;
            for n in 0..4000usize {
                let t = n as f32 / FS;
                let x = 0.4 * (TAU * 220.0 * t).sin() + 0.2 * (TAU * 1730.0 * t).sin();
                let y = if n % 997 == 0 { 0.8 } else { 0.0 };
                let f = fx.process_frame_with_bpm([x + y, 0.8 * x - y], 118.0);
                push(out, bus.process_frame(f));
            }
            k += 1;
        }
    }
}

/// SHA-256 of `data` as lowercase hex (FIPS 180-4; no dependency).
pub fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for (wi, b) in w.iter_mut().zip(chunk.as_chunks::<4>().0) {
            *wi = u32::from_be_bytes(*b);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let mut v = h;
        for (k, wi) in K.iter().zip(&w) {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7]
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(*k)
                .wrapping_add(*wi);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [
                t1.wrapping_add(t2),
                v[0],
                v[1],
                v[2],
                v[3].wrapping_add(t1),
                v[4],
                v[5],
                v[6],
            ];
        }
        for (hi, vi) in h.iter_mut().zip(v) {
            *hi = hi.wrapping_add(vi);
        }
    }
    h.iter().map(|x| format!("{x:08x}")).collect()
}
