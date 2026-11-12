# DAW Plugin

## Scope

`nord-plugin` exports CLAP and VST3 wrappers around `Stage4Engine`. Hosts own and restore the NIH-plug parameter state. The plugin synchronizes those values into the engine before processing each audio block.

## Host parameters

- Global controls include master level, BPM, compressor amount, scene selection, split enable, three split notes, and three split crossfade widths.
- Organ A and B each expose nine drawbars. Shared organ, piano, and synth controls map into their respective engine layers.
- Morph assignment: `morph_assign` (module `morph_assign.rs`) exposes signed Wheel / Pedal / Aftertouch depths (`-1.0..=1.0`, default `0.0`) for the 7 layer levels (`morph_level_{wheel,pedal,at}_1..7`, order Organ A/B, Piano A/B, Synth A/B/C) and the 3 synth cutoff slots (`morph_cutoff_{wheel,pedal,at}_1..3`). `sync_to_engine()` writes only the deltas; the base values stay owned by the level / cutoff parameters. Depth range and default are `ASSUMED` (engine delta domain); hardware depth scaling is unverified.
- Per-layer FX: `layer_fx` (module `fx_params.rs`) exposes 11 controls for each of the 7 layers (`fx_{drive,eq_bass,eq_mid,eq_treble,comp_on,comp_amt,delay_on,delay_ms,delay_fb,delay_mix,rev_mix}_1..7`) mapped onto `Stage4Engine::fx_chains[i]`. EQ range +-16 dB is `VERIFIED` against the 7-bit x 0.25 dB preset field; delay 20-1500 ms follows the hardware table; defaults equal `LayerFxChain::new`. Because `sync_to_engine()` runs every block, host values override FX loaded from `.ns4p`.
- Per-layer FX selectors: `layer_fx_selectors` (module `fx_selectors.rs`) exposes 13 controls per layer (`fxsel_{mod1_type,mod1_rate,mod2_type,mod2_rate,cabinet,comp_fast,delay_sync,delay_div,delay_pp,delay_flt,delay_fx,rev_model,rev_color}_1..7`) through host-side mirror enums of the `nord_core::fx` enums (variant order is the host index: append only). Mod rate ranges (0.1-20 Hz / 0.1-10 Hz) are `ASSUMED`; defaults equal `LayerFxChain::new`. Not yet exposed: mod amounts, EQ mid frequency, filter resonance, FX variation flags, `delay_analog`, `delay_sync_beats`.
- Parameter IDs are stable strings declared on `Stage4Params`; parameter ranges provide clamping in the host parameter system.
- `sync_to_engine()` maps split crossfade values 0, 1, and 2 to Off, Small, and Large. Split note values are MIDI notes 0–127.

## Preset and state handling

- `PluginStateSnapshot` is a JSON serialization format. New fields use defaults so older version-1 snapshots remain readable.
- Snapshot JSON is not a mechanism for writing into NIH-plug parameters. Host parameter restoration must use NIH-plug's host/context state interface.
- `load_ns4p_into_engine()` parses a CBIN program and applies it to a caller-owned engine. Callers must invoke it before moving the engine into audio processing; it does not update host automation values.
- Raw memory writes into NIH-plug parameter structs are prohibited.

## MIDI and host verification

- OmniSplit routes supported MIDI events through the core split router. Multitimbral mode maps zero-based channels 0–6 to Organ A/B, Piano A/B, and Synth A/B/C; channels 7–15 fall back to OmniSplit.
- Pitch bend is not implemented by the core MIDI event model and is not a supported plugin feature.
- Automated unit tests verify conversion, routing, parameter synchronization, state JSON compatibility, and program import. DAW host compatibility remains unverified until the plugin is loaded and exercised in each host.
- AUv2 is not exported or tested by this crate; external wrapper packaging is separate.
