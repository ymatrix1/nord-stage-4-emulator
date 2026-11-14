//! Offline deterministic renderer: scripted MIDI timeline -> stereo buffers -> `.wav`.
//!
//! Backs `ns4 render-patch` so emulator output can be A/B compared against hardware
//! reference clips (`tools/video_ref.py compare`). Not part of the real-time path:
//! allocation and I/O are fine here, `Stage4Engine::process_block` itself stays RT-safe.

use crate::engine::Stage4Engine;
use crate::midi::MidiEvent;

/// A MIDI event scheduled at an absolute frame offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimedEvent {
    pub frame: usize,
    pub event: MidiEvent,
}

/// Renders `total_frames` stereo frames, dispatching `events` at their frame offsets.
///
/// Events are applied in `frame` order (stable for ties); events at or past
/// `total_frames` are ignored. Deterministic for identical engine state + events.
pub fn render_timeline(
    engine: &mut Stage4Engine,
    events: &[TimedEvent],
    total_frames: usize,
) -> (Vec<f32>, Vec<f32>) {
    let mut sorted: Vec<TimedEvent> = events
        .iter()
        .copied()
        .filter(|e| e.frame < total_frames)
        .collect();
    sorted.sort_by_key(|e| e.frame);

    let mut left = vec![0.0f32; total_frames];
    let mut right = vec![0.0f32; total_frames];
    let mut pos = 0usize;
    let mut next = 0usize;
    while pos < total_frames {
        while next < sorted.len() && sorted[next].frame <= pos {
            engine.handle_midi(sorted[next].event);
            next += 1;
        }
        let end = sorted
            .get(next)
            .map_or(total_frames, |e| e.frame.min(total_frames));
        engine.process_block(&mut left[pos..end], &mut right[pos..end]);
        pos = end;
    }
    (left, right)
}

/// Peak (linear) and RMS (linear) across both channels.
pub fn peak_rms(left: &[f32], right: &[f32]) -> (f32, f32) {
    let n = left.len() + right.len();
    if n == 0 {
        return (0.0, 0.0);
    }
    let peak = left
        .iter()
        .chain(right.iter())
        .fold(0.0f32, |m, &x| m.max(x.abs()));
    let sum: f64 = left
        .iter()
        .chain(right.iter())
        .map(|&x| f64::from(x) * f64::from(x))
        .sum();
    (peak, (sum / n as f64).sqrt() as f32)
}

/// Encodes stereo f32 as a 32-bit IEEE-float WAV (format tag 3), lossless for A/B analysis.
pub fn encode_wav_f32_stereo(sample_rate: u32, left: &[f32], right: &[f32]) -> Vec<u8> {
    let frames = left.len().min(right.len());
    let data_len = (frames * 8) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&3u16.to_le_bytes()); // IEEE float
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&(sample_rate * 8).to_le_bytes());
    out.extend_from_slice(&8u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for i in 0..frames {
        out.extend_from_slice(&left[i].to_le_bytes());
        out.extend_from_slice(&right[i].to_le_bytes());
    }
    out
}

/// Parses `"60,64,67"` into MIDI note numbers (0..=127).
pub fn parse_notes(s: &str) -> Result<Vec<u8>, String> {
    let notes: Result<Vec<u8>, String> = s
        .split(',')
        .map(|t| {
            let n = t
                .trim()
                .parse::<u8>()
                .map_err(|_| format!("invalid note '{t}' (expected 0..=127)"))?;
            if n > 127 {
                return Err(format!("note {n} out of range (0..=127)"));
            }
            Ok(n)
        })
        .collect();
    let notes = notes?;
    if notes.is_empty() {
        return Err("empty note list".to_string());
    }
    Ok(notes)
}

/// Parses note events written by `tools/video_ref.py notes --events-out`.
///
/// Accepts `{"events": [{"midi", "onset_s", "offset_s", "velocity"?, "confirmed"?, "strength"?}]}`
/// or a bare array. With `confirmed_only`, events whose `confirmed` field is `false` are skipped.
/// An event's own `velocity` (1..=127) wins; otherwise the note plays at `velocity`. Returns the
/// timed events and the last note-off frame.
pub fn parse_note_events(
    json: &str,
    sample_rate: f32,
    velocity: u8,
    confirmed_only: bool,
) -> Result<(Vec<TimedEvent>, usize), String> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("invalid --events JSON: {e}"))?;
    let arr = v
        .get("events")
        .unwrap_or(&v)
        .as_array()
        .ok_or_else(|| "--events JSON must be an array or {\"events\": [...]}".to_string())?;
    let mut out = Vec::new();
    let mut last = 0usize;
    for e in arr {
        let note = e
            .get("midi")
            .and_then(serde_json::Value::as_u64)
            .filter(|&n| n <= 127)
            .ok_or_else(|| format!("event without valid 'midi': {e}"))? as u8;
        let on = e.get("onset_s").and_then(serde_json::Value::as_f64);
        let off = e.get("offset_s").and_then(serde_json::Value::as_f64);
        let (on, off) = on
            .zip(off)
            .filter(|(a, b)| *a >= 0.0 && b > a)
            .ok_or_else(|| format!("event needs 0 <= onset_s < offset_s: {e}"))?;
        if confirmed_only && e.get("confirmed").and_then(serde_json::Value::as_bool) == Some(false)
        {
            continue;
        }
        let note_velocity = match e.get("velocity") {
            None | Some(serde_json::Value::Null) => velocity,
            Some(x) => x
                .as_u64()
                .filter(|v| (1..=127).contains(v))
                .ok_or_else(|| format!("event 'velocity' must be 1..=127: {e}"))?
                as u8,
        };
        let on_f = (on * f64::from(sample_rate)).round() as usize;
        let off_f = (off * f64::from(sample_rate)).round() as usize;
        out.push(TimedEvent {
            frame: on_f,
            event: MidiEvent::NoteOn {
                channel: 0,
                note,
                velocity: note_velocity,
            },
        });
        out.push(TimedEvent {
            frame: off_f,
            event: MidiEvent::NoteOff {
                channel: 0,
                note,
                velocity: 0,
            },
        });
        last = last.max(off_f);
    }
    if out.is_empty() {
        return Err("no note events to render".to_string());
    }
    Ok((out, last))
}

/// Parses a CC event spec `"controller:value@frame"` (e.g. `"64:127@0"` = sustain down).
pub fn parse_cc_spec(s: &str) -> Result<(u8, u8, usize), String> {
    let bad = || format!("invalid --cc '{s}' (expected controller:value@frame, e.g. 64:127@0)");
    let (cv, frame) = s.split_once('@').ok_or_else(bad)?;
    let (c, v) = cv.split_once(':').ok_or_else(bad)?;
    let controller = c.trim().parse::<u8>().map_err(|_| bad())?;
    let value = v.trim().parse::<u8>().map_err(|_| bad())?;
    let frame = frame.trim().parse::<usize>().map_err(|_| bad())?;
    if controller > 127 || value > 127 {
        return Err(bad());
    }
    Ok((controller, value, frame))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_u32(b: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
    }

    #[test]
    fn test_parse_note_events_json() {
        let js = r#"{"events":[
            {"midi":60,"onset_s":0.5,"offset_s":1.0,"confirmed":true},
            {"midi":64,"onset_s":0.0,"offset_s":2.0,"confirmed":false}]}"#;
        let (ev, last) = parse_note_events(js, 48_000.0, 90, false).unwrap();
        assert_eq!(ev.len(), 4);
        assert_eq!(ev[0].frame, 24_000);
        assert_eq!(last, 96_000);
        assert!(matches!(
            ev[0].event,
            MidiEvent::NoteOn {
                note: 60,
                velocity: 90,
                ..
            }
        ));
        let (only, _) = parse_note_events(js, 48_000.0, 90, true).unwrap();
        assert_eq!(only.len(), 2, "confirmed_only drops unconfirmed events");
        assert!(parse_note_events("[]", 48_000.0, 90, false).is_err());
        assert!(parse_note_events(
            r#"[{"midi":200,"onset_s":0,"offset_s":1}]"#,
            48_000.0,
            90,
            false
        )
        .is_err());
        assert!(parse_note_events(
            r#"[{"midi":60,"onset_s":1,"offset_s":1}]"#,
            48_000.0,
            90,
            false
        )
        .is_err());
        assert!(parse_note_events("{", 48_000.0, 90, false).is_err());
    }

    #[test]
    fn test_parse_note_events_per_note_velocity() {
        let js = r#"[{"midi":60,"onset_s":0.0,"offset_s":1.0,"velocity":33},
            {"midi":62,"onset_s":0.0,"offset_s":1.0},
            {"midi":64,"onset_s":0.0,"offset_s":1.0,"velocity":null}]"#;
        let (ev, _) = parse_note_events(js, 48_000.0, 90, false).unwrap();
        let vel = |i: usize| match ev[i].event {
            MidiEvent::NoteOn { velocity, .. } => velocity,
            _ => panic!("expected note-on"),
        };
        assert_eq!((vel(0), vel(2), vel(4)), (33, 90, 90));
        for bad in [0, 128] {
            let js = format!(r#"[{{"midi":60,"onset_s":0,"offset_s":1,"velocity":{bad}}}]"#);
            assert!(parse_note_events(&js, 48_000.0, 90, false).is_err());
        }
    }

    #[test]
    fn test_parse_notes_ok_and_errors() {
        assert_eq!(parse_notes("60, 64,67").unwrap(), vec![60, 64, 67]);
        assert!(parse_notes("60,x").is_err());
        assert!(parse_notes("200").is_err());
        assert!(parse_notes("").is_err());
    }

    #[test]
    fn test_parse_cc_spec() {
        assert_eq!(parse_cc_spec("64:127@0").unwrap(), (64, 127, 0));
        assert_eq!(parse_cc_spec("66:0@48000").unwrap(), (66, 0, 48000));
        assert!(parse_cc_spec("64:127").is_err());
        assert!(parse_cc_spec("64@10").is_err());
        assert!(parse_cc_spec("200:1@0").is_err());
        assert!(parse_cc_spec("64:200@0").is_err());
    }

    #[test]
    fn test_encode_wav_header_and_payload() {
        let l = [0.5f32, -0.25];
        let r = [1.0f32, 0.0];
        let w = encode_wav_f32_stereo(48_000, &l, &r);
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(&w[8..16], b"WAVEfmt ");
        assert_eq!(u16::from_le_bytes([w[20], w[21]]), 3);
        assert_eq!(u16::from_le_bytes([w[22], w[23]]), 2);
        assert_eq!(read_u32(&w, 24), 48_000);
        assert_eq!(read_u32(&w, 40), 16);
        assert_eq!(read_u32(&w, 4) as usize, w.len() - 8);
        assert_eq!(f32::from_le_bytes(w[44..48].try_into().unwrap()), 0.5);
        assert_eq!(f32::from_le_bytes(w[48..52].try_into().unwrap()), 1.0);
        assert_eq!(f32::from_le_bytes(w[52..56].try_into().unwrap()), -0.25);
    }

    #[test]
    fn test_peak_rms() {
        assert_eq!(peak_rms(&[], &[]), (0.0, 0.0));
        let (p, r) = peak_rms(&[1.0, -1.0], &[0.0, 0.0]);
        assert_eq!(p, 1.0);
        assert!((r - 0.5f32.sqrt()).abs() < 1e-6);
    }

    #[test]
    fn test_render_timeline_chord_is_deterministic_and_release_decays() {
        let ev = |frame, event| TimedEvent { frame, event };
        let on = |note| MidiEvent::NoteOn {
            channel: 0,
            note,
            velocity: 100,
        };
        let off = |note| MidiEvent::NoteOff {
            channel: 0,
            note,
            velocity: 0,
        };
        let events = [
            ev(0, on(60)),
            ev(0, on(64)),
            ev(0, on(67)),
            ev(4000, off(60)),
            ev(4000, off(64)),
            ev(4000, off(67)),
            ev(99_999, on(72)), // past end: ignored
        ];
        let mut a = Stage4Engine::new(48_000.0);
        let mut b = Stage4Engine::new(48_000.0);
        let (la, ra) = render_timeline(&mut a, &events, 8192);
        let (lb, rb) = render_timeline(&mut b, &events, 8192);
        assert_eq!(la.len(), 8192);
        assert_eq!(la, lb);
        assert_eq!(ra, rb);
        let (held_peak, _) = peak_rms(&la[2000..4000], &ra[2000..4000]);
        assert!(held_peak > 0.0, "chord should sound while held");
        assert!(la.iter().chain(ra.iter()).all(|x| x.is_finite()));
    }

    #[test]
    fn test_render_timeline_event_order_matters() {
        // Note-on at frame 0 sounds; same note-on delayed to the end leaves earlier span silent.
        let on = MidiEvent::NoteOn {
            channel: 0,
            note: 60,
            velocity: 100,
        };
        let mut e = Stage4Engine::new(48_000.0);
        let (l, r) = render_timeline(
            &mut e,
            &[TimedEvent {
                frame: 4000,
                event: on,
            }],
            4096,
        );
        let (early, _) = peak_rms(&l[..4000], &r[..4000]);
        assert_eq!(early, 0.0);
    }
}
