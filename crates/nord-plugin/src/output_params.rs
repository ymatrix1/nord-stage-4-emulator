//! Output menu host parameters (manual OS v1.6x p.59; spec `docs/specs/midi-and-control.md`
//! section 2a and `docs/specs/effects-and-routing.md`).
//!
//! IDs are additive under the `outx` prefix: `outx_mode`, `outx_busout_1..4` (Main, Sub I-III),
//! `outx_global_1..7` and `outx_bus_1..7` (Organ, Rotary, Piano A/B, Synth A/B/C) and
//! `outx_trim_1..4` (outputs 1-4). Defaults equal `OutputRouting::default()`. The plugin's
//! audio ports stay one stereo pair, which carries outputs 1-2 (Main); outputs 3-4 are reachable
//! through the engine's `process_block_multi`.

use nih_plug::prelude::*;
use nord_core::engine::{
    OutputRouting, RoutingMode, Stage4Engine, OUTPUT_SOURCES, OUTPUT_TRIM_DB_RANGE,
};

fn int(name: &str, default: i32, max: i32) -> IntParam {
    IntParam::new(name, default, IntRange::Linear { min: 0, max })
}

/// Output choice or bus per source.
#[derive(Params)]
pub struct OutputSourceParams {
    /// Global mode choice: 0 `1-2`, 1 `3-4`, 2 `1`, 3 `2`, 4 `3`, 5 `4`, 6 `12+34`, 7 arrow / Auto.
    #[id = "global"]
    pub global: IntParam,
    /// Per Program bus: 0 Main, 1 Sub I, 2 Sub II, 3 Sub III, 4 arrow / Auto.
    #[id = "bus"]
    pub bus: IntParam,
}

/// Output choice of one bus (page 2).
#[derive(Params)]
pub struct OutputBusParams {
    /// 0 `1-2`, 1 `3-4`, 2 `1`, 3 `2`, 4 `3`, 5 `4`, 6 `12+34`.
    #[id = "busout"]
    pub out: IntParam,
}

/// Level trim of one physical output.
#[derive(Params)]
pub struct OutputTrimParams {
    /// Trim in dB (+/-12, default 0).
    #[id = "trim"]
    pub db: FloatParam,
}

/// Output routing, buses and trims.
#[derive(Params)]
pub struct OutputParams {
    /// Output Routing Mode: 0 Global (default), 1 Per Program.
    #[id = "mode"]
    pub mode: IntParam,
    /// Main, Sub I, Sub II, Sub III output choices.
    #[nested(array, group = "Output Buses")]
    pub buses: [OutputBusParams; 4],
    /// Organ, Rotary, Piano A, Piano B, Synth A, Synth B, Synth C.
    #[nested(array, group = "Output Sources")]
    pub sources: [OutputSourceParams; OUTPUT_SOURCES],
    /// Outputs 1-4.
    #[nested(array, group = "Output Trims")]
    pub trims: [OutputTrimParams; 4],
}

impl Default for OutputParams {
    fn default() -> Self {
        let d = OutputRouting::default();
        Self {
            mode: int("Output Routing Mode", 0, 1),
            buses: std::array::from_fn(|i| OutputBusParams {
                out: int("Bus Output", i32::from(d.bus_outputs[i]), 6),
            }),
            sources: std::array::from_fn(|i| OutputSourceParams {
                global: int("Source Output", i32::from(d.global_out[i]), 7),
                bus: int("Source Bus", i32::from(d.bus[i]), 4),
            }),
            trims: std::array::from_fn(|_| OutputTrimParams {
                db: FloatParam::new(
                    "Output Trim",
                    0.0,
                    FloatRange::Linear {
                        min: -OUTPUT_TRIM_DB_RANGE,
                        max: OUTPUT_TRIM_DB_RANGE,
                    },
                )
                .with_unit(" dB")
                .with_step_size(0.5),
            }),
        }
    }
}

impl OutputParams {
    /// The routing these parameters describe.
    pub fn to_routing(&self) -> OutputRouting {
        OutputRouting {
            mode: if self.mode.value() == 1 {
                RoutingMode::PerProgram
            } else {
                RoutingMode::Global
            },
            bus_outputs: std::array::from_fn(|i| self.buses[i].out.value() as u8),
            global_out: std::array::from_fn(|i| self.sources[i].global.value() as u8),
            bus: std::array::from_fn(|i| self.sources[i].bus.value() as u8),
            trim_db: std::array::from_fn(|i| self.trims[i].db.value()),
        }
    }

    /// Copy every value into the engine (no allocation; audio-thread safe).
    pub fn sync_to_engine(&self, engine: &mut Stage4Engine) {
        engine.output_routing = self.to_routing();
    }
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use nord_core::midi::MidiEvent;

    #[test]
    fn defaults_equal_engine_defaults() {
        assert_eq!(
            OutputParams::default().to_routing(),
            OutputRouting::default()
        );
    }

    #[test]
    fn default_sync_renders_bit_identically() {
        let run = |sync: bool| {
            let mut e = Stage4Engine::new(44_100.0);
            if sync {
                OutputParams::default().sync_to_engine(&mut e);
            }
            for n in [48u8, 60, 67] {
                e.handle_midi(MidiEvent::NoteOn {
                    channel: 0,
                    note: n,
                    velocity: 100,
                });
            }
            (0..2048).map(|_| e.render_frame()).collect::<Vec<_>>()
        };
        assert_eq!(run(false), run(true));
    }

    #[test]
    fn every_value_maps_to_its_engine_field() {
        let mut p = OutputParams::default();
        p.mode = int("m", 1, 1);
        p.buses[1].out = int("b", 6, 6);
        p.sources[2].bus = int("s", 1, 4);
        p.sources[4].global = int("g", 5, 7);
        p.trims[3].db = FloatParam::new(
            "t",
            -3.0,
            FloatRange::Linear {
                min: -12.0,
                max: 12.0,
            },
        );
        let mut e = Stage4Engine::new(44_100.0);
        p.sync_to_engine(&mut e);
        let r = e.output_routing;
        assert_eq!(r.mode, RoutingMode::PerProgram);
        assert_eq!(r.bus_outputs, [0, 6, 4, 5]);
        assert_eq!((r.bus[2], r.global_out[4]), (1, 5));
        assert_eq!(r.trim_db, [0.0, 0.0, 0.0, -3.0]);
    }

    #[test]
    fn ids_unique_and_complete() {
        let ids: Vec<String> = OutputParams::default()
            .param_map()
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        // mode + 4 bus outputs + 7 x (global, bus) + 4 trims
        assert_eq!(ids.len(), 1 + 4 + 14 + 4);
        let mut s = ids.clone();
        s.sort();
        s.dedup();
        assert_eq!(s.len(), ids.len());
    }
}
