# DAW Plugin

## Scope

`nord-plugin` exports CLAP and VST3 wrappers around `Stage4Engine`. Hosts own and restore the NIH-plug parameter state. The plugin synchronizes those values into the engine before processing each audio block.

## Host parameters

- Global controls include master level, BPM, compressor amount, scene selection, split enable, three split notes, and three split crossfade widths.
- Organ A and B each expose nine drawbars. Shared organ, piano, and synth controls map into their respective engine layers.
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
