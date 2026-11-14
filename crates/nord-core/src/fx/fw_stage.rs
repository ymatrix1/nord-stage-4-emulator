//! Frame-by-frame adapter around [`super::FwCompressor`] for the master bus (opt-in).
//!
//! The firmware routine works on blocks of 4 stereo frames, so the stage collects 4 frames,
//! processes the block and releases it one block later: **latency 4 frames (83 us at 48 kHz)**,
//! the first 4 output frames are silence. Allocation free and `Copy`.

use super::fw_compressor::FwCompressor;

/// Master-bus stage: 4-frame input buffer, 4-frame output buffer and the compressor.
#[derive(Debug, Clone, Copy)]
pub(super) struct FwCompressorStage {
    comp: FwCompressor,
    input: [[f32; 2]; 4],
    output: [[f32; 2]; 4],
    pos: usize,
}

impl FwCompressorStage {
    pub(super) fn new(comp: FwCompressor) -> Self {
        Self {
            comp,
            input: [[0.0; 2]; 4],
            output: [[0.0; 2]; 4],
            pos: 0,
        }
    }

    /// The compressor context (live parameter fills).
    pub(super) fn compressor_mut(&mut self) -> &mut FwCompressor {
        &mut self.comp
    }

    /// Pushes one frame and returns the frame of the previous block at the same position.
    #[inline]
    pub(super) fn process_frame(&mut self, frame: [f32; 2]) -> [f32; 2] {
        let out = self.output[self.pos];
        self.input[self.pos] = frame;
        self.pos += 1;
        if self.pos == 4 {
            self.pos = 0;
            let mut block: [f32; 8] = std::array::from_fn(|i| self.input[i / 2][i % 2]);
            self.comp.process_block(&mut block);
            self.output = std::array::from_fn(|k| [block[2 * k], block[2 * k + 1]]);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_is_the_input_delayed_by_one_block_at_unity_gain() {
        let mut st = FwCompressorStage::new(FwCompressor::new());
        let ins: Vec<[f32; 2]> = (0..12)
            .map(|n| [0.01 * n as f32, -0.02 * n as f32])
            .collect();
        let outs: Vec<[f32; 2]> = ins.iter().map(|&f| st.process_frame(f)).collect();
        assert!(outs[..4].iter().all(|f| *f == [0.0, 0.0]));
        for (n, (out, inp)) in outs[4..].iter().zip(&ins).enumerate() {
            for (o, i) in out.iter().zip(inp) {
                assert!((o - i).abs() < 1e-6, "frame {}", n + 4);
            }
        }
    }
}
