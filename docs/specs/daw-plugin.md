# DAW Plugin

## Scope

`nord-plugin` exports CLAP and VST3 wrappers around `Stage4Engine`. Hosts own and restore the NIH-plug parameter state. The plugin synchronizes those values into the engine before processing each audio block.

## Host parameters

- Global controls include master level, BPM, compressor amount, scene selection, split enable, three split notes, and three split crossfade widths.
- Organ A and B each expose nine drawbars. Shared organ, piano, and synth controls map into their respective engine layers.
- Morph assignment: `morph_assign` (module `morph_assign.rs`) exposes signed Wheel / Pedal / Aftertouch depths (`-1.0..=1.0`, default `0.0`) for the 7 layer levels (`morph_level_{wheel,pedal,at}_1..7`, order Organ A/B, Piano A/B, Synth A/B/C) and the 3 synth cutoff slots (`morph_cutoff_{wheel,pedal,at}_1..3`). `sync_to_engine()` writes only the deltas; the base values stay owned by the level / cutoff parameters. Depth range and default are `ASSUMED` (engine delta domain); hardware depth scaling is unverified.
- Per-layer FX: `layer_fx` (module `fx_params.rs`) exposes 11 controls for each of the 7 layers (`fx_{drive,eq_bass,eq_mid,eq_treble,comp_on,comp_amt,delay_on,delay_ms,delay_fb,delay_mix,rev_mix}_1..7`) mapped onto `Stage4Engine::fx_chains[i]`. EQ range +-16 dB is `VERIFIED` against the 7-bit x 0.25 dB preset field; delay 20-1500 ms follows the hardware table; defaults equal `LayerFxChain::new`. Because `sync_to_engine()` runs every block, host values override FX loaded from `.ns4p`.
- Per-layer Piano extras: `piano_layers` (module `piano_params.rs`) exposes `pianox_{soft_release,unison,dyn_comp,octave}_{1,2}` (Soft Release on/off, Unison 0-3, Dyn Comp 0-3, Octave Shift -2..2) and copies them to `engine.piano_a/b` in `sync_to_engine`. Ranges follow the engine fields; the manual (p.23-26) gives Unison 1-3 and Dyn Comp 1-3 plus Off. Ids are snapshotted in `param-ids.snapshot`.
- Per-layer Synth extras: `synth_layers` (module `synth_params.rs`) exposes `synthx_{unison,voice,priority,glide,kb_track,filter_env,osc_env,osc_env_pitch}_{1,2,3}`: Unison 0-3, Voice Poly/Mono/Legato, Note Priority Last/Lo/Hi, Glide 0-4 s/oct (scale `ASSUMED`, see `SynthLayer::glide_sec`), KB Track Off/1/3/2/3/1, bipolar Filter and Osc Env amounts, Env To Pitch. Defaults equal the engine defaults, so an untouched host leaves sounds unchanged. Test `unison_param_changes_the_rendered_signal` renders Synth A with unison 0 vs 3 and checks the output differs. The shared `synth_*` ids still drive all three layers (D16). LFO, Arp and Vibrato are in `synth_mod_params.rs` as `synthm_{lfo_shape,lfo_dest,lfo_rate,lfo_amount,arp_mode,arp_range,arp_rate,vib_mode,vib_rate,vib_depth,vib_delay}_1..3` (defaults equal `SynthLayer::new`; vibrato depth cents scale `ASSUMED`; tests render LFO-to-pitch and vibrato against Off). Not yet exposed: Gate mode, Pattern edit, KB Hold, KB Sync, Master Clock sync, Unison for samples.
- Per-layer Organ extras: `organ_layers` (module `organ_params.rs`) exposes `organx_{b3_bass,octave,scanner}_{1,2}` plus `organx_close_mic` and `organx_stop_mode` (Rotary, copied to both organ layers). Defaults equal the engine defaults. Test `scanner_vibrato_changes_rendered_organ_audio` renders Organ A with scanner Off vs C3. Not yet exposed: Preset/Drawbar Live, Poly percussion, key click, rotary angle.
- Per-layer FX selectors: `layer_fx_selectors` (module `fx_selectors.rs`) exposes 13 controls per layer (`fxsel_{mod1_type,mod1_rate,mod2_type,mod2_rate,cabinet,comp_fast,delay_sync,delay_div,delay_pp,delay_flt,delay_fx,rev_model,rev_color}_1..7`) through host-side mirror enums of the `nord_core::fx` enums (variant order is the host index: append only). Mod rate ranges (0.1-20 Hz / 0.1-10 Hz) are `ASSUMED`; defaults equal `LayerFxChain::new`. Mod amounts, EQ mid frequency, filter resonance, amp/delay-effect variation flags and `delay_analog` are exposed by `fx_extra_params.rs` as `fxx_{mod1_amount,mod2_amount,eq_mid_freq,filter_res,amp_variation,delay_analog,delay_fx_variation}_1..7` (defaults equal `LayerFxChain::new`; mid freq 200 Hz-8 kHz per manual p.51; test `mod1_amount_changes_the_processed_signal` runs a tremolo through the chain at amount 0 vs 1). Not yet exposed: Mod 1/2 variation flags, `delay_sync_beats`.
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
