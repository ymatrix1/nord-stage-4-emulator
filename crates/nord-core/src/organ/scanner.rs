//! Hammond scanner vibrato / chorus and the shared interpolated ring-buffer read.
//!
//! Spec: `docs/specs/organ-engine.md` section 5. Depths are ASSUMED.

use super::types::SCANNER_BUFFER_SIZE;
use super::{OrganLayer, ScannerMode};
use crate::phase_wrap::wrap_pos;
use std::f32::consts::TAU;

impl OrganLayer {
    /// Reads a linearly interpolated delayed sample from a circular ring buffer.
    #[inline]
    pub(super) fn read_ring_interpolated(buf: &[f32], write_idx: usize, delay_samples: f32) -> f32 {
        let cap = buf.len();
        let d = delay_samples.clamp(0.0, (cap - 2) as f32);
        let read_pos = (write_idx as f32 - d + cap as f32) % (cap as f32);
        let idx0 = (read_pos.floor() as usize) % cap;
        let idx1 = (idx0 + 1) % cap;
        let frac = read_pos - read_pos.floor();
        buf[idx0] * (1.0 - frac) + buf[idx1] * frac
    }

    /// Processes Hammond 6.85 Hz LC Scanner Vibrato / Chorus (`V1..V3`, `C1..C3`).
    pub(super) fn process_scanner(&mut self, dry: f32, inv_fs: f32) -> f32 {
        if self.scanner_mode == ScannerMode::Off {
            return dry;
        }
        let (depth_ms, mix_wet) = match self.scanner_mode {
            ScannerMode::Off => (0.0, 0.0),
            ScannerMode::V1 => (0.75, 1.0),
            ScannerMode::V2 => (1.50, 1.0),
            ScannerMode::V3 => (2.50, 1.0),
            ScannerMode::C1 => (0.75, 0.5),
            ScannerMode::C2 => (1.50, 0.5),
            ScannerMode::C3 => (2.50, 0.5),
        };

        self.scanner_phase = wrap_pos(self.scanner_phase + TAU * 6.85 * inv_fs, TAU);
        let mod_delay_samples =
            (depth_ms * 0.001 * self.sample_rate) * (0.5 + 0.5 * self.scanner_phase.sin());

        self.scanner_buffer[self.scanner_write_idx] = dry;
        let delayed = Self::read_ring_interpolated(
            self.scanner_buffer.as_ref(),
            self.scanner_write_idx,
            mod_delay_samples,
        );
        self.scanner_write_idx = (self.scanner_write_idx + 1) % SCANNER_BUFFER_SIZE;

        (1.0 - mix_wet) * dry + mix_wet * delayed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_read_interpolates_between_samples() {
        let mut buf = [0.0f32; 8];
        buf[3] = 1.0;
        buf[4] = 3.0;
        // write_idx 5, delay 1.5 -> position 3.5
        let v = OrganLayer::read_ring_interpolated(&buf, 5, 1.5);
        assert!((v - 2.0).abs() < 1e-6);
    }

    #[test]
    fn scanner_off_is_transparent_and_chorus_mixes() {
        let mut o = OrganLayer::new(48_000.0);
        assert_eq!(o.process_scanner(0.5, 1.0 / 48_000.0), 0.5);
        o.scanner_mode = ScannerMode::C3;
        let mut differs = false;
        for i in 0..SCANNER_BUFFER_SIZE {
            let dry = (i as f32 * 0.1).sin();
            let y = o.process_scanner(dry, 1.0 / 48_000.0);
            assert!(y.is_finite());
            differs |= (y - dry).abs() > 1e-4;
        }
        assert!(differs);
    }
}
