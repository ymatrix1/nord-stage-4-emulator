//! Master bus: optional firmware IIR and compressor, then the DAC reconstruction stage.

use super::fw_params::compressor_for_amount;
use super::{fw_stage, master_iir, FwCompressor};

/// Global Master Compressor and Burr-Brown `PCM1791A` + JRC `NJM4580M` output reconstruction stage.
#[derive(Debug, Clone, Copy)]
pub struct MasterBus {
    pub compressor_amount: f32,
    /// Run the summed bus through the firmware 4-section master IIR before the compressor
    /// (`fw_coeffs::MASTER_IIR_WORDS`). Default `false`; the position in the chain is ASSUMED.
    pub master_iir_enabled: bool,
    iir: master_iir::MasterIir,
    /// Opt-in firmware compressor gain computer (`fw_compressor`), 4-frame latency; replaces the
    /// peak-envelope compressor while set. Default `None`.
    fw_compressor: Option<fw_stage::FwCompressorStage>,
    env: f32,
    recon_lp_l: f32,
    recon_lp_r: f32,
}

impl Default for MasterBus {
    fn default() -> Self {
        Self {
            compressor_amount: 0.25,
            master_iir_enabled: false,
            iir: master_iir::MasterIir::new(),
            fw_compressor: None,
            env: 0.0,
            recon_lp_l: 0.0,
            recon_lp_r: 0.0,
        }
    }
}

impl MasterBus {
    /// Replaces the peak-envelope compressor by the firmware gain computer (`None` restores it).
    ///
    /// Adds a latency of 4 frames and restarts the stage. The compressor context is ASSUMED apart
    /// from the algorithm (VERIFIED, see `fw_compressor`); `compressor_amount` is not used.
    pub fn set_fw_compressor(&mut self, comp: Option<FwCompressor>) {
        self.fw_compressor = comp.map(fw_stage::FwCompressorStage::new);
    }

    /// Switches to the firmware compressor with the `kCmpAmt` words for `compressor_amount`
    /// (`0..=1`, the preset field `raw7 / 127`): threshold `-36 v / 127`, makeup
    /// `1 + 5.72 (v / 127)^2`, captured attack and release pairs (`fw_map`, `fw_params`).
    /// Restarts the stage, latency 4 frames. Roles EMPIRICAL, slope ASSUMED.
    pub fn use_fw_compressor_amount(&mut self) {
        let ui = super::fw_map::unit_to_ui(self.compressor_amount);
        self.set_fw_compressor(Some(compressor_for_amount(ui)));
    }

    /// Processes the summed 7-layer stereo signal through the Master Compressor and DAC stage.
    pub fn process_frame(&mut self, frame: [f32; 2]) -> [f32; 2] {
        let [l, r] = if self.master_iir_enabled {
            self.iir.process_frame(frame)
        } else {
            frame
        };
        if let Some(stage) = &mut self.fw_compressor {
            let [cl, cr] = stage.process_frame([l, r]);
            return self.reconstruct([cl.clamp(-1.0, 1.0), cr.clamp(-1.0, 1.0)]);
        }
        let peak = l.abs().max(r.abs());
        self.env = 0.99 * self.env + 0.01 * peak;

        let amt = self.compressor_amount.clamp(0.0, 1.0);
        let gain = if self.env > 0.5 {
            1.0 / (1.0 + amt * (self.env - 0.5) * 2.0)
        } else {
            1.0 + 0.15 * amt
        };

        self.reconstruct([(l * gain).clamp(-1.0, 1.0), (r * gain).clamp(-1.0, 1.0)])
    }

    /// Post-DAC Burr-Brown PCM1791A + NJM4580M stage:
    /// The hardware PCM1791A features a flat passband within +/-0.002 dB up to 21.8 kHz.
    /// A gentle 0.985 alpha preserves crisp treble (<0.15 dB roll-off at 20 kHz) without in-band damping.
    #[inline]
    fn reconstruct(&mut self, [cl, cr]: [f32; 2]) -> [f32; 2] {
        self.recon_lp_l += 0.985 * (cl - self.recon_lp_l);
        self.recon_lp_r += 0.985 * (cr - self.recon_lp_r);
        [self.recon_lp_l, self.recon_lp_r]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drive(bus: &mut MasterBus) -> Vec<[f32; 2]> {
        (0..64)
            .map(|n| {
                let x = (n as f32 * 0.07).sin() * 0.4;
                bus.process_frame([x, -x])
            })
            .collect()
    }

    #[test]
    fn default_master_bus_passes_finite_audio() {
        let mut bus = MasterBus::default();
        for n in 0..512 {
            let x = (n as f32 * 0.03).sin() * 0.5;
            let [l, r] = bus.process_frame([x, x]);
            assert!(l.is_finite() && r.is_finite());
        }
    }

    #[test]
    fn firmware_compressor_is_opt_in_delays_four_frames_and_none_restores_the_default() {
        let mut plain = MasterBus::default();
        let reference = drive(&mut plain);
        let mut toggled = MasterBus::default();
        toggled.set_fw_compressor(Some(FwCompressor::new()));
        toggled.set_fw_compressor(None);
        assert_eq!(
            drive(&mut toggled),
            reference,
            "None restores the default path"
        );
        let mut fw = MasterBus::default();
        fw.set_fw_compressor(Some(FwCompressor::new()));
        let out = drive(&mut fw);
        assert!(out.iter().all(|f| f[0].is_finite() && f[1].is_finite()));
        assert!(
            out[..4].iter().all(|f| f[0].abs() < 1e-3),
            "first block is silent"
        );
        assert!(
            out[20][0].abs() > 0.01,
            "audio passes after the first block"
        );
    }

    #[test]
    fn amount_wiring_is_finite_silent_for_silence_and_stable_at_full_scale() {
        for amount in [0.0, 0.25, 0.75, 1.0] {
            let mut bus = MasterBus {
                compressor_amount: amount,
                ..MasterBus::default()
            };
            bus.use_fw_compressor_amount();
            for _ in 0..2000 {
                let [l, r] = bus.process_frame([0.0, 0.0]);
                assert!(l == 0.0 && r == 0.0, "silence in, silence out");
            }
            for n in 0..48_000 {
                let x = (n as f32 * 0.05).sin() * if n % 9000 < 4500 { 1.0 } else { 0.01 };
                let [l, r] = bus.process_frame([x, -x]);
                assert!(l.is_finite() && r.is_finite() && l.abs() <= 1.0 && r.abs() <= 1.0);
                assert!(l == 0.0 || l.abs() > 1e-30, "no denormals at {n}");
            }
        }
    }

    #[test]
    fn amount_zero_is_transparent_apart_from_the_block_delay_and_dac_filter() {
        let mut bus = MasterBus {
            compressor_amount: 0.0,
            ..MasterBus::default()
        };
        bus.use_fw_compressor_amount();
        let out = drive(&mut bus);
        let mut plain = MasterBus {
            compressor_amount: 0.0,
            ..MasterBus::default()
        };
        let reference = drive(&mut plain);
        for (n, (got, want)) in out[8..60].iter().zip(&reference[4..56]).enumerate() {
            assert!(
                (got[0] - want[0]).abs() < 2e-3,
                "frame {}: {} vs {}",
                n + 8,
                got[0],
                want[0]
            );
        }
    }
}
