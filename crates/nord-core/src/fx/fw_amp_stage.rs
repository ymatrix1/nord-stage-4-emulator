//! Frame-by-frame adapter around the firmware Amp drive stage ([`super::FwAmpDrive`], routine
//! `0x1228CF`) with the real tables and the type words of `kAmpTypeNew` (opt-in).
//!
//! The routine runs once per pass on `ctx[0x15] = 2` blocks of two stereo frames, so the stage
//! collects 4 frames, processes them and releases them one block later: **latency 4 frames
//! (83 us at 48 kHz)**, the first 4 output frames are silence. Allocation free and `Copy`.
//!
//! The stage is the drive and voicing half of the firmware amp (two biquad sections around a
//! differentiating high-pass, a drive-dependent waveshaper mix and the output high-pass) followed
//! by the **cabinet** ([`super::fw_cabinet`], measured impulse response of `0x122A1D`, no extra
//! latency). The organ unit's tone routines are not modelled (`docs/research/fx-type-map.md` s8,
//! s10).

use super::fw_amp_drive::{DriveTables, FwAmpDrive};
use super::fw_amp_types::context_words;
use super::fw_cabinet::FwCabinet;
use super::fw_drive_tables::{A, B, C, C_CENTER};

/// Amp drive stage plus the 4-frame buffers.
#[derive(Debug, Clone, Copy)]
pub(super) struct FwAmpStage {
    drive: FwAmpDrive,
    cabinet: FwCabinet,
    kind: u8,
    drive_ui: f32,
    input: [[f32; 2]; 4],
    output: [[f32; 2]; 4],
    /// Cabinet output for the four frames of `output` (computed when the block is released, see
    /// [`FwCabinet::process_block4`]).
    cabinet_out: [[f32; 2]; 4],
    pos: usize,
}

impl FwAmpStage {
    /// Stage for amp type `kind` (`1..=6`) with `kAmpDrive = drive_ui` (`0..=127`), or `None` for a
    /// type that has no drive stage words.
    pub(super) fn new(kind: u8, drive_ui: f32) -> Option<Self> {
        let mut cabinet = FwCabinet::new(kind)?;
        // The first four frames feed the cabinet with the silent initial block.
        let silent = [[0.0; 2]; 4];
        let cabinet_out = cabinet.process_block4(silent);
        Some(Self {
            drive: FwAmpDrive::from_words(context_words(kind, drive_ui)?),
            cabinet,
            kind,
            drive_ui,
            input: silent,
            output: silent,
            cabinet_out,
            pos: 0,
        })
    }

    /// The type this stage was built for.
    pub(super) fn kind(&self) -> u8 {
        self.kind
    }

    /// Moves the drive target (the stage's own smoother glides there).
    pub(super) fn set_drive(&mut self, drive_ui: f32) {
        if drive_ui != self.drive_ui {
            super::fw_params::set_amp_drive(&mut self.drive, drive_ui);
            self.drive_ui = drive_ui;
        }
    }

    /// Pushes one frame and returns the frame of the previous block at the same position, through
    /// the cabinet. The cabinet input of the next four frames is the block released here, so the
    /// cabinet runs on the whole block at once; the result is the same as filtering frame by frame.
    #[inline]
    pub(super) fn process_frame(&mut self, frame: [f32; 2]) -> [f32; 2] {
        let out = self.cabinet_out[self.pos];
        self.input[self.pos] = frame;
        self.pos += 1;
        if self.pos == 4 {
            self.pos = 0;
            let mut block: [f32; 8] = std::array::from_fn(|i| self.input[i / 2][i % 2]);
            let tables = DriveTables {
                a: &A,
                b: &B,
                c: &C,
                c_center: C_CENTER,
            };
            self.drive.process(&tables, &mut block);
            self.output = std::array::from_fn(|k| [block[2 * k], block[2 * k + 1]]);
            self.cabinet_out = self.cabinet.process_block4(self.output);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Level change of the stage in dB for a sine of `freq` Hz and amplitude `amp`.
    fn gain_db(kind: u8, drive: f32, freq: f32, amp: f32) -> f32 {
        let mut st = FwAmpStage::new(kind, drive).unwrap();
        let (mut num, mut den) = (0.0f64, 0.0f64);
        for n in 0..24_000 {
            let x = amp * (std::f32::consts::TAU * freq * n as f32 / 48_000.0).sin();
            let y = st.process_frame([x, x])[0];
            if n > 4_000 {
                num += f64::from(y * y);
                den += f64::from(x * x);
            }
        }
        (10.0 * (num / den).log10()) as f32
    }

    /// Runs the bare drive stage (no 4 frame adapter) over the golden signal.
    fn run_golden(kind: u8, drive: f32, input: &[u32]) -> Vec<u32> {
        let mut stage = FwAmpStage::new(kind, drive).unwrap();
        let tables = DriveTables {
            a: &A,
            b: &B,
            c: &C,
            c_center: C_CENTER,
        };
        let mut out = Vec::new();
        for call in input.as_chunks::<8>().0 {
            let mut frames = call.map(f32::from_bits);
            stage.drive.process(&tables, &mut frames);
            out.extend(frames.map(f32::to_bits));
        }
        out
    }

    /// Worst absolute difference between two word vectors relative to the peak of `want`.
    fn worst_error(got: &[u32], want: &[u32]) -> f32 {
        let peak = want
            .iter()
            .map(|w| f32::from_bits(*w).abs())
            .fold(0.0, f32::max);
        let worst = got
            .iter()
            .zip(want)
            .map(|(&a, &b)| (f32::from_bits(a) - f32::from_bits(b)).abs())
            .fold(0.0, f32::max);
        worst / peak
    }

    #[test]
    fn real_context_and_tables_match_the_oracle_for_twin_and_jc() {
        use super::super::tests_amp_golden as g;
        // The oracle works in 40-bit registers with truncating stores, this port in f32: the high
        // pass differences cancel, so the bound is relative to the output peak (measured worst
        // case: Twin 1.4e-6, JC 1.7e-4, asserted at 1e-3).
        let twin = run_golden(1, 14.0, &g::TWIN_FRAMES_IN);
        let jc = run_golden(3, 100.0, &g::JC_FRAMES_IN);
        let (e_twin, e_jc) = (
            worst_error(&twin, &g::TWIN_FRAMES_OUT),
            worst_error(&jc, &g::JC_FRAMES_OUT),
        );
        assert!(
            e_twin < 1.0e-3,
            "Twin, kAmpDrive 14: {e_twin:e} of the peak"
        );
        assert!(e_jc < 1.0e-3, "JC, kAmpDrive 100: {e_jc:e} of the peak");
        assert!(
            twin.iter().any(|w| *w != 0) && jc != twin,
            "the vectors are not trivially silent or equal"
        );
    }

    #[test]
    fn drive_plus_cabinet_matches_the_oracle_chain_on_broadband_input() {
        use super::super::tests_cabinet_golden::CHAIN_OUT;
        // The seeded LCG of `tools/fx_amp_cabinet.py::chain_input`.
        let mut state: u32 = 12345;
        let mut input = Vec::new();
        for _ in 0..CHAIN_OUT.len() {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            input.push((f64::from(state >> 8) / f64::from(1u32 << 24) - 0.5) as f32 * 0.2);
        }
        let mut stage = FwAmpStage::new(1, 14.0).unwrap();
        let tables = DriveTables {
            a: &A,
            b: &B,
            c: &C,
            c_center: C_CENTER,
        };
        let mut got = Vec::new();
        for block in input.as_chunks::<8>().0 {
            let mut frames = *block;
            stage.drive.process(&tables, &mut frames);
            for pair in frames.as_chunks::<2>().0 {
                let y = stage.cabinet.process_frame(*pair);
                got.extend(y.map(f32::to_bits));
            }
        }
        let want = &CHAIN_OUT[..got.len()];
        let e = worst_error(&got, want);
        assert!(e < 2.0e-3, "drive + cabinet differs by {e:e} of the peak");
    }

    #[test]
    fn untypeable_amps_have_no_stage_and_output_is_delayed_by_one_block() {
        assert!(FwAmpStage::new(0, 0.0).is_none() && FwAmpStage::new(7, 0.0).is_none());
        let mut st = FwAmpStage::new(5, 0.0).unwrap();
        assert_eq!(st.kind(), 5);
        let outs: Vec<[f32; 2]> = (0..8)
            .map(|n| st.process_frame([0.01 * n as f32, 0.0]))
            .collect();
        assert!(outs[..4].iter().all(|f| *f == [0.0, 0.0]));
    }

    #[test]
    fn set_drive_moves_the_phase_target() {
        let mut st = FwAmpStage::new(1, 0.0).unwrap();
        st.set_drive(127.0);
        assert_eq!(
            st.drive
                .value(super::super::fw_amp_drive::ctx::PHASE_TARGET),
            1.0
        );
    }

    #[test]
    fn the_twin_stage_response_is_stable_and_finite() {
        for f in [120.0, 1000.0, 3000.0, 9000.0] {
            let g = gain_db(1, 14.0, f, 0.05);
            assert!(g.is_finite() && g.abs() < 40.0, "{f} Hz: {g} dB");
        }
    }
}
