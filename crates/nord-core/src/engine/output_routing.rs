//! Output menu routing: which physical output (1-4) each section and layer feeds, and the output
//! level trims (manual OS v1.6x p.59, Output menu pages 1-5).
//!
//! The stereo [`Stage4Engine::render_frame`] keeps returning the Main pair (outputs 1-2);
//! [`Stage4Engine::render_frame_multi`] returns all four outputs. With the default routing every
//! source feeds outputs 1-2, so the Main pair is bit-identical to the engine before routing was
//! modelled (`tests::default_routing_is_bit_identical_to_the_stereo_mix`).
//!
//! Provenance: the choice lists, defaults and the arrow / Auto rules are `VERIFIED` against the
//! manual text on p.59 (checked by the tests below and by `nord_panel::menus`); the engine
//! treatment of a single output ("1".."4") as the mid signal `(L + R) / 2` is `ASSUMED` (the manual
//! says only that piano and stereo effects are adapted for mono, not how); the master processing
//! (compressor, EQ, Mono Output) acting on the Main pair only is `ASSUMED`.

use super::*;

/// Number of routable sources: Organ, Rotary, Piano A, Piano B, Synth A, Synth B, Synth C.
pub const OUTPUT_SOURCES: usize = 7;
/// Source index of the Organ section (both Organ layers).
pub const SRC_ORGAN: usize = 0;
/// Source index of the Rotary Speaker.
pub const SRC_ROTARY: usize = 1;
/// Source index of each layer `0..7` (`LayerId` order: Organ A/B, Piano A/B, Synth A/B/C).
pub(super) const LAYER_SOURCE: [usize; 7] = [0, 0, 2, 3, 4, 5, 6];
/// Output trim range in dB. `ASSUMED`: the manual has no output trim; +/-12 dB covers any
/// sensible level match between outputs.
pub const OUTPUT_TRIM_DB_RANGE: f32 = 12.0;

/// Output Routing Mode (Output menu page 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RoutingMode {
    /// Sections and layers feed the physical outputs directly (hardware default).
    #[default]
    Global,
    /// Sections and layers feed buses, the buses feed the outputs.
    PerProgram,
}

/// What a source or bus is assigned to; the index order is the menu choice list
/// `1-2, 3-4, 1, 2, 3, 4, 12+34`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutChoice {
    #[default]
    Pair12,
    Pair34,
    Out1,
    Out2,
    Out3,
    Out4,
    /// Both stereo pairs (a monitor copy of the signal).
    Both,
}

impl OutChoice {
    /// The choice at menu index `i` (clamped to the last one).
    pub fn from_index(i: u8) -> Self {
        match i {
            0 => Self::Pair12,
            1 => Self::Pair34,
            2 => Self::Out1,
            3 => Self::Out2,
            4 => Self::Out3,
            5 => Self::Out4,
            _ => Self::Both,
        }
    }

    /// Adds the stereo signal `(l, r)` to the four output accumulators.
    pub(super) fn add(self, acc: &mut [f32; 4], l: f32, r: f32) {
        match self {
            Self::Pair12 => {
                acc[0] += l;
                acc[1] += r;
            }
            Self::Pair34 => {
                acc[2] += l;
                acc[3] += r;
            }
            Self::Out1 => acc[0] += 0.5 * (l + r),
            Self::Out2 => acc[1] += 0.5 * (l + r),
            Self::Out3 => acc[2] += 0.5 * (l + r),
            Self::Out4 => acc[3] += 0.5 * (l + r),
            Self::Both => {
                acc[0] += l;
                acc[1] += r;
                acc[2] += l;
                acc[3] += r;
            }
        }
    }
}

/// Output menu state. Indices follow the menu choice lists, so the GUI writes them unchanged.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OutputRouting {
    /// Output Routing Mode (page 1).
    pub mode: RoutingMode,
    /// Output choice of the Main, Sub I, Sub II and Sub III bus, `0..=6` (page 2; defaults
    /// `1-2`, `3-4`, `3`, `4`, VERIFIED manual p.59).
    pub bus_outputs: [u8; 4],
    /// Global mode choice per source, `0..=6` or `7` = arrow / Auto (pages 3-5). Sources B, C and
    /// the Rotary default to the arrow / Auto.
    pub global_out: [u8; OUTPUT_SOURCES],
    /// Per Program bus per source, `0..=3` (Main..Sub III) or `4` = arrow / Auto.
    pub bus: [u8; OUTPUT_SOURCES],
    /// Level trim of outputs 1-4 in dB, applied after the master processing.
    pub trim_db: [f32; 4],
}

impl Default for OutputRouting {
    fn default() -> Self {
        Self {
            mode: RoutingMode::Global,
            bus_outputs: [0, 1, 4, 5],
            global_out: [0, 7, 0, 7, 0, 7, 7],
            bus: [0, 4, 0, 4, 0, 4, 4],
            trim_db: [0.0; 4],
        }
    }
}

/// The source an arrow / Auto follows (`None` for Organ, Piano A and Synth A).
fn follows(source: usize) -> Option<usize> {
    match source {
        SRC_ROTARY => Some(SRC_ORGAN),
        3 => Some(2),
        5 => Some(4),
        6 => Some(5),
        _ => None,
    }
}

impl OutputRouting {
    /// Where `source` goes, following arrows to the left. `rotary_follows` is the source the
    /// Rotary Auto setting follows (the Organ, or the first Piano / Synth layer feeding the rotary
    /// when the Organ does not use it; manual p.59).
    pub fn destination(&self, source: usize, rotary_follows: usize) -> OutChoice {
        let mut s = source.min(OUTPUT_SOURCES - 1);
        let arrow = match self.mode {
            RoutingMode::Global => 7,
            RoutingMode::PerProgram => 4,
        };
        for _ in 0..OUTPUT_SOURCES {
            let raw = match self.mode {
                RoutingMode::Global => self.global_out[s],
                RoutingMode::PerProgram => self.bus[s],
            };
            if raw < arrow {
                return match self.mode {
                    RoutingMode::Global => OutChoice::from_index(raw),
                    RoutingMode::PerProgram => {
                        OutChoice::from_index(self.bus_outputs[usize::from(raw.min(3))].min(6))
                    }
                };
            }
            // A, Organ and any source without a left neighbour fall back to the Main pair.
            let Some(next) = (if s == SRC_ROTARY {
                Some(rotary_follows.min(OUTPUT_SOURCES - 1))
            } else {
                follows(s)
            }) else {
                return OutChoice::Pair12;
            };
            s = next;
        }
        OutChoice::Pair12
    }

    /// Destinations of all seven sources.
    pub fn destinations(&self, rotary_follows: usize) -> [OutChoice; OUTPUT_SOURCES] {
        std::array::from_fn(|s| self.destination(s, rotary_follows))
    }

    /// Linear trim gains of outputs 1-4; exactly `1.0` at 0 dB, NaN counts as 0 dB.
    pub fn trim_gains(&self) -> [f32; 4] {
        self.trim_db.map(|db| {
            let db = if db.is_finite() { db } else { 0.0 };
            let db = db.clamp(-OUTPUT_TRIM_DB_RANGE, OUTPUT_TRIM_DB_RANGE);
            if db == 0.0 {
                1.0
            } else {
                10f32.powf(db / 20.0)
            }
        })
    }
}

impl Stage4Engine {
    /// Source the Rotary Auto setting follows: the Organ when it uses the rotary (or nothing else
    /// does), else the first Piano / Synth layer routed `TO ROTARY` in priority order.
    pub(super) fn rotary_follow_source(&self) -> usize {
        if self.organ_a.rotary_enabled || self.organ_b.rotary_enabled {
            return SRC_ORGAN;
        }
        (2..7)
            .find(|&i| self.fx_chains[i].amp_cabinet == AmpCabinet::ToRotary)
            .map_or(SRC_ORGAN, |i| LAYER_SOURCE[i])
    }

    /// Like [`Self::process_block`] but fills all four physical outputs. Outputs 1-2 equal what
    /// `process_block` writes. Allocation free.
    pub fn process_block_multi(&mut self, out: [&mut [f32]; 4]) {
        let frames = out.iter().map(|o| o.len()).min().unwrap_or(0);
        self.begin_block();
        let [o1, o2, o3, o4] = out;
        for i in 0..frames {
            let f = self.render_frame_multi();
            o1[i] = f[0];
            o2[i] = f[1];
            o3[i] = f[2];
            o4[i] = f[3];
        }
        self.publish_telemetry();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_manual() {
        let r = OutputRouting::default();
        assert_eq!(r.mode, RoutingMode::Global);
        let buses: Vec<_> = r
            .bus_outputs
            .iter()
            .map(|&i| OutChoice::from_index(i))
            .collect();
        // Main 1-2, Sub I 3-4, Sub II 3, Sub III 4 (manual p.59)
        assert_eq!(
            buses,
            [
                OutChoice::Pair12,
                OutChoice::Pair34,
                OutChoice::Out3,
                OutChoice::Out4
            ]
        );
        assert!(r
            .destinations(SRC_ORGAN)
            .iter()
            .all(|&d| d == OutChoice::Pair12));
        assert_eq!(r.trim_gains(), [1.0; 4]);
    }

    #[test]
    fn arrows_follow_the_layer_to_the_left() {
        let mut r = OutputRouting::default();
        r.global_out[2] = 1; // Piano A -> 3-4
        r.global_out[4] = 6; // Synth A -> 12+34
        assert_eq!(r.destination(3, 0), OutChoice::Pair34, "Piano B follows A");
        assert_eq!(r.destination(5, 0), OutChoice::Both, "Synth B follows A");
        r.global_out[5] = 4; // Synth B -> 3
        assert_eq!(r.destination(6, 0), OutChoice::Out3, "Synth C follows B");
        r.global_out[3] = 2; // explicit Piano B -> 1
        assert_eq!(r.destination(3, 0), OutChoice::Out1);
    }

    #[test]
    fn per_program_routes_through_the_buses() {
        let mut r = OutputRouting {
            mode: RoutingMode::PerProgram,
            ..OutputRouting::default()
        };
        r.bus[2] = 1; // Piano A -> Sub I -> 3-4
        r.bus[4] = 3; // Synth A -> Sub III -> 4
        assert_eq!(r.destination(2, 0), OutChoice::Pair34);
        assert_eq!(r.destination(3, 0), OutChoice::Pair34, "Piano B arrow");
        assert_eq!(r.destination(4, 0), OutChoice::Out4);
        r.bus_outputs[1] = 6; // Sub I -> 12+34
        assert_eq!(r.destination(2, 0), OutChoice::Both);
        // The settings of the other mode are preserved (manual p.59).
        r.mode = RoutingMode::Global;
        assert_eq!(r.destination(2, 0), OutChoice::Pair12);
    }

    #[test]
    fn rotary_auto_follows_the_organ_or_the_feeding_layer() {
        let mut r = OutputRouting::default();
        r.global_out[SRC_ORGAN] = 1;
        r.global_out[2] = 2;
        assert_eq!(r.destination(SRC_ROTARY, SRC_ORGAN), OutChoice::Pair34);
        assert_eq!(r.destination(SRC_ROTARY, 2), OutChoice::Out1);
        r.global_out[SRC_ROTARY] = 5; // explicit
        assert_eq!(r.destination(SRC_ROTARY, SRC_ORGAN), OutChoice::Out4);
    }

    #[test]
    fn trims_clamp_and_ignore_non_finite_values() {
        let r = OutputRouting {
            trim_db: [6.0, -40.0, f32::NAN, 0.0],
            ..OutputRouting::default()
        };
        let g = r.trim_gains();
        assert!((g[0] - 1.9953).abs() < 1e-3);
        assert!((g[1] - 10f32.powf(-12.0 / 20.0)).abs() < 1e-6);
        assert_eq!((g[2], g[3]), (1.0, 1.0));
    }

    #[test]
    fn add_sums_mono_outputs_and_the_both_copy() {
        let mut acc = [0.0f32; 4];
        OutChoice::Out2.add(&mut acc, 1.0, 0.0);
        assert_eq!(acc, [0.0, 0.5, 0.0, 0.0]);
        OutChoice::Both.add(&mut acc, 0.25, 0.75);
        assert_eq!(acc, [0.25, 1.25, 0.25, 0.75]);
    }

    fn chord(e: &mut Stage4Engine) {
        for n in [48u8, 60, 67] {
            e.handle_midi(MidiEvent::NoteOn {
                channel: 0,
                note: n,
                velocity: 100,
            });
        }
    }

    #[test]
    fn default_routing_is_bit_identical_to_the_stereo_mix() {
        let mut a = Stage4Engine::new(44_100.0);
        let mut b = Stage4Engine::new(44_100.0);
        b.output_routing = OutputRouting::default();
        chord(&mut a);
        chord(&mut b);
        for _ in 0..4096 {
            let s = a.render_frame();
            let m = b.render_frame_multi();
            assert_eq!([m[0], m[1]], s);
            assert_eq!([m[2], m[3]], [0.0, 0.0]);
        }
    }

    #[test]
    fn a_routed_piano_leaves_main_and_appears_on_outputs_3_4() {
        let render = |route: bool| {
            let mut e = Stage4Engine::new(44_100.0);
            e.scene.scene_1_mask = 1 << LayerId::PianoA as u8;
            if route {
                e.output_routing.global_out[2] = 1;
            }
            chord(&mut e);
            let mut peak = [0.0f32; 4];
            for _ in 0..4096 {
                let f = e.render_frame_multi();
                for k in 0..4 {
                    peak[k] = peak[k].max(f[k].abs());
                }
            }
            peak
        };
        let main = render(false);
        assert!(main[0] > 1e-4 && main[2] == 0.0, "{main:?}");
        let sub = render(true);
        assert!(sub[0] == 0.0 && sub[1] == 0.0, "{sub:?}");
        assert!(sub[2] > 1e-4 && sub[3] > 1e-4, "{sub:?}");
    }

    #[test]
    fn process_block_multi_matches_the_frame_api_and_the_stereo_block() {
        let mut a = Stage4Engine::new(44_100.0);
        let mut b = Stage4Engine::new(44_100.0);
        chord(&mut a);
        chord(&mut b);
        let (mut l, mut r) = (vec![0.0f32; 256], vec![0.0f32; 256]);
        a.process_block(&mut l, &mut r);
        let mut o = [(); 4].map(|_| vec![0.0f32; 256]);
        let [o1, o2, o3, o4] = &mut o;
        b.process_block_multi([o1, o2, o3, o4]);
        assert_eq!(l, o[0]);
        assert_eq!(r, o[1]);
        assert!(o[2].iter().chain(&o[3]).all(|&x| x == 0.0));
    }

    #[test]
    fn trim_scales_only_its_output() {
        let mut e = Stage4Engine::new(44_100.0);
        e.output_routing.trim_db[0] = -6.0;
        let mut r = Stage4Engine::new(44_100.0);
        chord(&mut e);
        chord(&mut r);
        for _ in 0..2048 {
            let t = e.render_frame_multi();
            let u = r.render_frame_multi();
            assert!((t[0] - u[0] * 10f32.powf(-6.0 / 20.0)).abs() < 1e-6);
            assert_eq!(t[1], u[1]);
        }
    }
}
