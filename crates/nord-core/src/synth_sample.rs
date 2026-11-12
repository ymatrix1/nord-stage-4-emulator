//! Pre-decoded multisample instruments for the Synth `SAMPLES` oscillator.

use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Timbre brightness damping curve for `SAMPLES` oscillator playback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SampleTimbre {
    #[default]
    Off,
    Soft,
    Bright,
}

/// Pre-loaded single-zone multisample waveform ready for lock-free real-time synthesis.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadedSynthSample {
    pub root_key: u8,
    pub key_low: u8,
    pub key_high: u8,
    pub sample_rate: f32,
    pub pcm: Arc<[f32]>,
    pub pcm_right: Option<Arc<[f32]>>,
    pub loop_start: Option<usize>,
    pub loop_end: Option<usize>,
    pub loop_crossfade: usize,
}

/// Pre-decoded playable multisample instrument for `SynthOscMode::Sample`.
#[derive(Debug, Clone, PartialEq)]
pub struct SynthSampleSound {
    pub name: String,
    pub zones: Vec<LoadedSynthSample>,
}

impl SynthSampleSound {
    /// Creates a synthetic single-zone multisample fixture for deterministic rendering/tests.
    pub fn new_synthetic(
        name: &str,
        root_key: u8,
        pcm: &[f32],
        sample_rate: f32,
        loop_range: Option<(usize, usize)>,
        loop_crossfade: usize,
    ) -> Self {
        let (loop_start, loop_end) = match loop_range {
            Some((s, e)) if e > s && e <= pcm.len() => (Some(s), Some(e)),
            _ => (None, None),
        };
        Self {
            name: name.to_string(),
            zones: vec![LoadedSynthSample {
                root_key,
                key_low: 0,
                key_high: 127,
                sample_rate: sample_rate.max(8000.0),
                pcm: pcm.to_vec().into(),
                pcm_right: None,
                loop_start,
                loop_end,
                loop_crossfade,
            }],
        }
    }

    /// Creates a multi-zone sample sound from pre-decoded key/velocity zones.
    pub fn from_zones(name: &str, zones: Vec<LoadedSynthSample>) -> Self {
        Self {
            name: name.to_string(),
            zones,
        }
    }

    /// Parses and decodes a clean-room `.nsmp4` byte slice into a playable `SynthSampleSound`.
    pub fn from_nsmp4_bytes(bytes: &[u8]) -> Result<Self, crate::cbin::CbinError> {
        struct DecodedStrokeCache<'a> {
            id: u32,
            left: Arc<[f32]>,
            right: Option<Arc<[f32]>>,
            meta: &'a crate::cbin::Nsmp4DecodedStroke,
        }

        let inst = crate::cbin::Nsmp4Instrument::parse(bytes)?;
        let decoded = inst.decode_strokes(bytes)?;
        let mut by_id = Vec::with_capacity(decoded.len());
        for stroke in &decoded {
            let g = stroke.gain_linear();
            let left: Vec<f32> = stroke.left.iter().map(|&s| s * g).collect();
            let right: Option<Arc<[f32]>> = stroke
                .right
                .as_ref()
                .map(|r| r.iter().map(|&s| s * g).collect::<Vec<f32>>().into());
            by_id.push(DecodedStrokeCache {
                id: stroke.stroke_id,
                left: left.into(),
                right,
                meta: stroke,
            });
        }

        let mut zones = Vec::with_capacity(inst.zones.len());
        for z in &inst.zones {
            if let Some(entry) = by_id.iter().find(|e| e.id == z.stroke_id) {
                let root_key = if z.root_key > 0 {
                    z.root_key
                } else {
                    entry.meta.root_key
                };
                zones.push(LoadedSynthSample {
                    root_key,
                    key_low: z.key_low,
                    key_high: z.key_high,
                    sample_rate: f32::from(entry.meta.sample_rate.max(8000)),
                    pcm: Arc::clone(&entry.left),
                    pcm_right: entry.right.clone(),
                    loop_start: entry.meta.loop_start,
                    loop_end: entry.meta.loop_end,
                    loop_crossfade: 0,
                });
            }
        }

        if zones.is_empty() {
            for entry in &by_id {
                zones.push(LoadedSynthSample {
                    root_key: entry.meta.root_key,
                    key_low: 0,
                    key_high: 127,
                    sample_rate: f32::from(entry.meta.sample_rate.max(8000)),
                    pcm: Arc::clone(&entry.left),
                    pcm_right: entry.right.clone(),
                    loop_start: entry.meta.loop_start,
                    loop_end: entry.meta.loop_end,
                    loop_crossfade: 0,
                });
            }
        }

        Ok(Self {
            name: inst.name,
            zones,
        })
    }

    /// Resolves the best zone index for a given MIDI note (exact range first, nearest root fallback).
    pub(crate) fn zone_for_note(&self, note: u8) -> Option<usize> {
        if let Some(pos) = self
            .zones
            .iter()
            .position(|z| note >= z.key_low && note <= z.key_high && !z.pcm.is_empty())
        {
            return Some(pos);
        }
        self.zones
            .iter()
            .enumerate()
            .filter(|(_, z)| !z.pcm.is_empty())
            .min_by_key(|(_, z)| (i16::from(z.root_key) - i16::from(note)).unsigned_abs())
            .map(|(i, _)| i)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_sound_clamps_rate_and_validates_loop() {
        let pcm = [0.0, 0.5, -0.5, 0.25];
        let looped = SynthSampleSound::new_synthetic("loop", 60, &pcm, 100.0, Some((1, 3)), 0);
        assert_eq!(looped.zones[0].sample_rate, 8000.0);
        assert_eq!(looped.zones[0].loop_start, Some(1));
        assert_eq!(looped.zones[0].loop_end, Some(3));
        let bad = SynthSampleSound::new_synthetic("bad", 60, &pcm, 44_100.0, Some((3, 9)), 0);
        assert_eq!(bad.zones[0].loop_start, None);
        assert_eq!(bad.name, "bad");
    }

    #[test]
    fn zone_for_note_picks_the_zone_covering_or_nearest_to_the_note() {
        let zone = |root, lo, hi| LoadedSynthSample {
            root_key: root,
            key_low: lo,
            key_high: hi,
            sample_rate: 48_000.0,
            pcm: vec![0.0f32; 4].into(),
            pcm_right: None,
            loop_start: None,
            loop_end: None,
            loop_crossfade: 0,
        };
        let sound = SynthSampleSound::from_zones("two", vec![zone(48, 0, 59), zone(72, 60, 127)]);
        assert_eq!(sound.zone_for_note(40), Some(0));
        assert_eq!(sound.zone_for_note(90), Some(1));
        assert_eq!(
            SynthSampleSound::from_zones("none", vec![]).zone_for_note(60),
            None
        );
    }

    #[test]
    fn sample_timbre_defaults_off() {
        assert_eq!(SampleTimbre::default(), SampleTimbre::Off);
    }
}
