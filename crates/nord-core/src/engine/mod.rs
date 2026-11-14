//! Unified 3-Engine, 7-Layer Multitimbral Nord Stage 4 DSP Coordinator (`Stage4Engine`).
//!
//! Spec: `docs/ARCHITECTURE.md`, `docs/specs/effects-and-routing.md`, `docs/specs/morph-and-zones.md`

use crate::fx::{
    AmpCabinet, DelayFilterMode, FxMorphSet, LayerFxChain, MasterBus, MasterClock, ReverbColor,
};
use crate::midi::{cc, MidiEvent};
use crate::morph::{
    KeybedSplitRouter, LayerId, LayerSceneState, MorphControllers, MorphParameter, MorphSource,
};
use crate::organ::{OrganLayer, RotarySpeed};
use crate::piano::PianoLayer;
use crate::synth::SynthLayer;

pub use aux_kb::{AuxKb, AuxVelocityCurve, NoteRoute};
pub use extern_menu::{ExternDevice, ExternDevices, ExternMenu};
pub use extern_out::{ExternLayer, ExternState, MidiOut, MidiOutQueue, MIDI_OUT_CAPACITY};
pub use foot_switch::{foot_switch_b_func, FootSwitchB};
pub use midi_menu::{MidiMenuSettings, MidiMode, ProgramRequest, TransposeAt};
pub use organ_trigger::{OrganTrigger, TriggerPoint, LOW_TRIGGER_DELAY_SEC};
pub use output_routing::{
    OutChoice, OutputRouting, RoutingMode, OUTPUT_SOURCES, OUTPUT_TRIM_DB_RANGE, SRC_ORGAN,
    SRC_ROTARY,
};
pub use pedal_assign::{
    PanelFunc, SustainPedalType, TripleFunc, TriplePedal, TripleState, TripleUnit,
};
pub use pedals::{
    ControlFunc, FootSwitchFunc, PedalSource, PedalState, RightPedal, RotorFunc, SwellFunc,
    B3_SWELL_FLOOR, HALF_PEDAL_MIN,
};
pub use pstick::{PSTICK_DEFAULT_RANGE, PSTICK_MAX_RANGE, PSTICK_MIN_RANGE};

mod aux_kb;
mod clipboard;
mod extern_menu;
mod extern_out;
mod foot_switch;
mod fx_params;
mod handoff;
mod layer_init;
mod live_slots;
mod midi_dispatch;
mod midi_menu;
mod midi_out;
mod mix;
mod morph_clip;
mod organ_modes;
mod organ_trigger;
mod output_routing;
mod pedal_assign;
mod pedals;
mod program;
mod program_export;
mod program_fx;
mod program_ops;
mod pstick;
mod routing;
mod section_edit;
mod settings;
mod sharing;
mod snapshot;
mod sound_init;
mod telemetry;
mod telemetry_cell;
#[cfg(test)]
mod tests_extern;
#[cfg(test)]
mod tests_firmware_routing;
#[cfg(test)]
mod tests_midix;
#[cfg(test)]
mod tests_pedals;
#[cfg(test)]
mod tests_program;
#[cfg(test)]
mod tests_routing;
mod undo;
mod undo_ops;
mod zone_assign;
pub mod zone_table;

pub use clipboard::{Clip, ClipboardError, LayerClip};
pub use fx_params::FxParams;
pub use handoff::{StateMask, UndoHint};
pub use layer_init::LayerInitTarget;
pub use live_slots::{LiveSlots, LIVE_SLOT_COUNT};
pub use morph_clip::MorphClip;
pub use program_ops::{PROGRAM_LEVEL_DB_RANGE, TRANSPOSE_RANGE};
pub use routing::{zone_leds, FirmwareRouting, ProgramRouting};
pub use section_edit::Section;
pub use settings::{
    GlobalSettings, Level3, TonewheelMode, FINE_TUNE_RANGE_CENTS, GLOBAL_TRANSPOSE_RANGE,
    PIANO_LEVEL_DB_RANGE,
};
pub use snapshot::ProgramState;
pub use telemetry::{Telemetry, TelemetrySink, WORDS as TELEMETRY_WORDS};
pub use telemetry_cell::TelemetryCell;
pub use undo::UndoOp;
pub use zone_assign::zone_code;
#[cfg(test)]
mod tests_sharing;
#[cfg(test)]
mod tests_telemetry;

pub use organ_modes::model_shares_vibrato;
pub use sharing::FxSharing;

/// Complete 7-layer Nord Stage 4 real-time DSP engine.
#[derive(Debug, Clone)]
pub struct Stage4Engine {
    pub sample_rate: f32,
    pub organ_a: OrganLayer,
    pub organ_b: OrganLayer,
    /// Vox / Farf vibrato shared by both organ layers (manual p.20-21); default off, see
    /// [`Self::apply_organ_sharing`].
    pub organ_shared_vibrato: bool,
    pub piano_a: PianoLayer,
    pub piano_b: PianoLayer,
    pub synth_a: SynthLayer,
    pub synth_b: SynthLayer,
    pub synth_c: SynthLayer,
    pub fx_chains: [LayerFxChain; 7],
    /// GLOBAL / GROUP effect modes (all off by default); applied by [`Self::apply_fx_sharing`].
    pub fx_sharing: FxSharing,
    /// Shared Reverb of GLOBAL mode (after the rotary and the level faders, manual p.52).
    global_reverb: Box<LayerFxChain>,
    /// Rotary Speaker instance for layers routed `TO ROTARY` (settings mirror `organ_a`).
    rotary_bus: Box<OrganLayer>,
    pub layer_levels: [MorphParameter; 7],
    pub synth_cutoff_morph: [MorphParameter; 3],
    pub morph_controllers: MorphControllers,
    pub scene: LayerSceneState,
    pub split_router: KeybedSplitRouter,
    /// Opt-in firmware layer routing mode (default `Off`) and the routing facts of the applied program.
    firmware_routing: FirmwareRouting,
    routing: ProgramRouting,
    pub master_bus: MasterBus,
    pub master_clock: MasterClock,
    /// MIDI channel (0..15) used for global keybed, controller, and program input.
    pub global_midi_channel: Option<u8>,
    /// Optional direct MIDI channel for each layer; these notes bypass keybed splits.
    pub layer_midi_channels: [Option<u8>; 7],
    /// Aux KB (auxiliary keyboard) configuration.
    pub aux_kb: AuxKb,
    /// Pedals, pitch stick and per-layer SUSTPED / PSTICK routing.
    pub pedals: PedalState,
    /// Global System/Sound/Output menu settings (apply with [`Self::set_global_settings`]).
    settings: GlobalSettings,
    /// Extern layer settings and the outgoing MIDI queue.
    pub extern_out: ExternState,
    /// Global MIDI menu settings (Local Control, Sect On, Ctrl / Dev, PC, Transpose MIDI At).
    pub midi_menu: MidiMenuSettings,
    midi_rt: midi_menu::MidiRuntime,
    /// Extern menu: Echo Global, Keyboard Velocity and Device MIDI filters of the Extern layers.
    pub extern_menu: ExternMenu,
    /// Output menu: routing of sections and layers to the four outputs, and output trims.
    pub output_routing: OutputRouting,
    /// Sound menu Organ Keyboard Trigger Point.
    pub organ_trigger: OrganTrigger,
    note_hold_counts: [[[u8; 7]; 128]; 16],
    /// Per-program transpose in semitones (`-6..=6`, manual p.40).
    transpose: i8,
    /// Transpose each held `(channel, note)` key started with, so note-off matches note-on.
    held_transpose: [[i8; 128]; 16],
    /// Program Levels offset in dB (manual p.46) and its linear gain (`1.0` when `0 dB`).
    program_level_db: f32,
    program_gain: f32,
    /// Copy/Paste buffer; survives program changes. Control thread only.
    pub clipboard: Option<Clip>,
    /// Single-level Undo snapshot. Control thread only.
    pub undo: undo::UndoSlot,
    /// Undo / Redo availability told by a control thread that owns the snapshots (see `handoff.rs`).
    undo_hint: UndoHint,
    /// Boxes leaked because the control thread's return queue was full (see `handoff.rs`).
    retire_overflow: u32,
    /// MIDI events handled since start (wraps); published as telemetry.
    midi_in_count: u32,
    /// Where [`Self::publish_telemetry`] stores the snapshot (see `telemetry.rs`).
    telemetry_out: TelemetrySink,
}

impl Stage4Engine {
    /// Creates a pre-allocated 7-layer Stage 4 DSP engine at `sample_rate`.
    pub fn new(sample_rate: f32) -> Self {
        let fs = sample_rate.max(8000.0);
        Self {
            sample_rate: fs,
            organ_a: OrganLayer::new(fs),
            organ_b: OrganLayer::new(fs),
            organ_shared_vibrato: false,
            piano_a: PianoLayer::new(fs),
            piano_b: PianoLayer::new(fs),
            synth_a: SynthLayer::new(fs),
            synth_b: SynthLayer::new(fs),
            synth_c: SynthLayer::new(fs),
            fx_chains: std::array::from_fn(|_| LayerFxChain::new(fs)),
            fx_sharing: FxSharing::default(),
            global_reverb: Box::new(LayerFxChain::new(fs)),
            rotary_bus: Box::new(OrganLayer::new(fs)),
            layer_levels: [MorphParameter::new(0.8); 7],
            synth_cutoff_morph: [MorphParameter::new(0.5); 3],
            morph_controllers: MorphControllers::default(),
            scene: LayerSceneState::default(),
            split_router: KeybedSplitRouter::default(),
            firmware_routing: FirmwareRouting::default(),
            routing: ProgramRouting::default(),
            master_bus: MasterBus::default(),
            master_clock: MasterClock::default(),
            global_midi_channel: Some(0),
            layer_midi_channels: [None; 7],
            aux_kb: AuxKb::default(),
            pedals: PedalState::default(),
            settings: GlobalSettings::default(),
            extern_out: ExternState::default(),
            midi_menu: MidiMenuSettings::default(),
            midi_rt: midi_menu::MidiRuntime::default(),
            extern_menu: ExternMenu::default(),
            output_routing: OutputRouting::default(),
            organ_trigger: OrganTrigger::default(),
            note_hold_counts: [[[0; 7]; 128]; 16],
            transpose: 0,
            held_transpose: [[0; 128]; 16],
            program_level_db: 0.0,
            program_gain: 1.0,
            clipboard: None,
            undo: undo::UndoSlot::default(),
            undo_hint: UndoHint::default(),
            retire_overflow: 0,
            midi_in_count: 0,
            telemetry_out: TelemetrySink::default(),
        }
    }

    /// Renders a single stereo frame `[left, right]` across all 7 layers + 7 FX racks + Master Bus.
    /// This is the Main pair (outputs 1-2); see [`Self::render_frame_multi`] for all four outputs.
    pub fn render_frame(&mut self) -> [f32; 2] {
        let out = self.render_frame_multi();
        [out[0], out[1]]
    }

    /// Feeds the Master Clock tempo to the Synth layers and applies their KB Sync clock resets.
    fn sync_synth_clock(&mut self) {
        let bpm = self.master_clock.bpm;
        let mut reset = false;
        for synth in [&mut self.synth_a, &mut self.synth_b, &mut self.synth_c] {
            synth.perf.master_bpm = bpm;
            reset |= synth.take_clock_reset();
        }
        if reset {
            self.master_clock.beat_phase = 0.0;
        }
    }

    /// Per-block control work shared by `process_block` and `process_block_multi`.
    pub(super) fn begin_block(&mut self) {
        self.extern_sync_volume();
        self.apply_fx_sharing();
        self.apply_organ_sharing();
    }

    /// Processes an interleaved or dual-channel block in place with zero heap allocations.
    pub fn process_block(&mut self, left: &mut [f32], right: &mut [f32]) {
        let frames = left.len().min(right.len());
        self.begin_block();
        for i in 0..frames {
            let [l, r] = self.render_frame();
            left[i] = l;
            right[i] = r;
        }
        self.publish_telemetry();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_engine_is_silent_and_finite() {
        let mut e = Stage4Engine::new(48_000.0);
        for _ in 0..32 {
            assert_eq!(e.render_frame(), [0.0, 0.0]);
        }
    }
}
