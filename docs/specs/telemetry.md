# Engine to GUI Telemetry

Status: v1.0 (Phase 6b B10, bean `ns4-gjy2`). Implementation: `crates/nord-core/src/engine/telemetry.rs`, `telemetry_cell.rs`; GUI side `crates/nord-gui/src/telemetry.rs`.

The panel LEDs must show what the engine does (morphed levels, real Aux flags, real MIDI activity) without locking the audio thread. The audio thread publishes a fixed-size plain-data snapshot once per block; GUI threads read it lock-free.

## Transport: seqlock over atomic words

| Item | Value |
|---|---|
| Cell | `TelemetryCell`: `seq: AtomicU32` + `[AtomicU32; TELEMETRY_WORDS]` (34 words, 136 bytes) |
| Writer | exactly one thread (the audio callback): `seq += 1` (odd), release fence, relaxed word stores, `seq += 1` (even, release) |
| Reader | `read()`: acquire `seq`, retry while odd, copy words, acquire fence, accept when `seq` is unchanged; up to 64 attempts, then `None` |
| Allocation / locks / I/O | none on either side; no `unsafe` (payload is atomics, so there is no data race to excuse) |
| Cost | `Stage4Engine::telemetry()` reads 7 layers + a 16x128x7 hold-count table (14 336 bytes); once per block, never per sample |
| Clone | `TelemetrySink` clones to "no sink": a cloned engine never becomes a second writer |

`Stage4Engine::process_block` publishes at its end; the standalone's CPAL callback (which renders per frame) calls `Stage4Engine::publish_telemetry()` after each buffer.

## Fields (`Telemetry`)

| Field | Source in the engine | Panel use |
|---|---|---|
| `layer_level[7]` | `layer_levels[i].evaluate(morph_controllers)` (fader plus Morph, before program level and master volume) | 11-segment ladders of Organ A/B, Piano A/B, Synth A/B/C |
| `active_mask` | `scene.is_layer_active` | layer ON LEDs; a layer that is off shows no ladder |
| `aux_mask` | `aux_kb.is_aux_layer` | Aux KB LEDs (`aux_flag_from_telemetry`) |
| `layer_held[7]`, `held_notes[4]` | `note_hold_counts` (all channels) | per-layer activity, held-key bitmap |
| `morph[3]` | `morph_controllers` (wheel, pedal, aftertouch) | Morph LEDs / meters |
| `split_enabled`, `split_zone_gain[4]` | `split_router` (`note_gain` at one probe note per zone) | L / M / H split LEDs |
| `midi_in_count` | incremented by `handle_midi` | MIDI LED blink |
| `midi_out_count`, `midi_out_dropped`, `midi_out_pending`, `extern_mask` | Extern `MidiOutQueue` (`pushed`, `dropped`, `len`), `extern_out.layers[i].enabled` | EXTERN LED blink, overflow counters |
| `organ_drawbars[2][9]` | `OrganLayer::active_drawbars` | drawbar LED ladders |
| `rotary_speed`, `rotary_stop` | `organ_a.rotary_speed`, `rotary_stop_mode` | SLOW/STOP and FAST LEDs |
| `scene_2`, `transpose`, `bpm` | scene state, `transpose()`, `master_clock.bpm` | OLED / LEDs |
| `arp_step` | `floor(master_clock.beat_phase * 4)` | clock step |
| `undo`, `redo` | `undo_status()` | UNDO soft button enabled, REDO offered |

Provenance:

| Tag | Item |
|---|---|
| ASSUMED | `organ_drawbars` equals the effective drawbar: the engine has no drawbar morph yet, so "morphed" and effective coincide. A future morph route changes only this field. |
| ASSUMED | `arp_step`: the engine has no arpeggiator; the field is the sixteenth step of the Master Clock beat an arpeggiator would use. |
| ASSUMED | `ACTIVITY_BLINK_MS = 120` for MIDI / EXTERN blinks (manual p.41 only says "short" and "longer"). |
| not published | per-layer *focus*: focus is panel-only state (the selected layer), the engine has no such concept. |

## GUI side

`AudioHandle` owns a `TelemetryHandle` (created in `AudioHandle::with_capacity`, so every clone shares one cell). `start_audio_and_midi` hands the cell to the engine (`attach_telemetry`). `render_stage4_panel` reads one `PanelTelemetry` per frame into `Stage4PanelState::telemetry`, advances the MIDI / EXTERN blink timers (`PanelLeds`) and requests a repaint every 33 ms while a snapshot exists. Every accessor of `PanelTelemetry` takes the value the UI would show on its own and returns it unchanged when no snapshot exists (no audio device, headless tests).

| Control | Reads |
|---|---|
| Organ / Piano / Synth ladders | `ladder_level(layer, ui_level, ui_active)` |
| Organ drawbar ladders | `drawbar(organ_b, i, ui_value)` (still unlit in Drawbar Live) |
| Aux KB LEDs | `aux(layer)`; the "not emulated" dimming applies only without telemetry |
| MIDI / EXTERN LEDs | `PanelLeds::midi_on / extern_on`, OR-ed with the key-press blink |
| L / M / H split LEDs | `split_leds(ui)` |
| Rotary SLOW / FAST LEDs | `rotary_slow / rotary_fast` (the button still toggles the UI value) |
| UNDO soft button / REDO | `undo_blocked()` disables UNDO, `redo_available()` enables soft button 2 (`ProgramEdit::Redo`) |

## Allocation proof

`crates/nord-core/tests/no_alloc_audio_path.rs` is its own test binary with a counting `#[global_allocator]` (delegates to `System`; counts `alloc`, `alloc_zeroed`, `realloc`, `dealloc` on a thread that armed a `const` thread-local flag, so counting never allocates). Setup and four warm-up blocks run unarmed. The measured region is the audio callback body: 400 blocks of 256 frames with note on/off, a mod-wheel CC, `process_block`, the telemetry publish and a concurrent-style `read()`. Result: **0 allocator calls** (`the_counter_sees_allocations_and_frees` proves the counter fires). `publishing_alone_is_allocation_free` covers 1000 bare publishes.

## Tests

| Test | Proves |
|---|---|
| `engine::telemetry_cell::tests::concurrent_readers_never_see_a_torn_snapshot` | 1 writer, 3 readers for 400 ms, every snapshot internally consistent |
| `engine::telemetry::tests::*` | wire round trip of every field, defaults, morphed level, held keys, counters, Aux / split / scene / rotary, Extern overflow, undo / redo, clock step, publish |
| `nord-gui telemetry::tests::*` | handle, ladder fallbacks, activity LED timing |
| `panel::telemetry_wiring_tests::*` | ladders, drawbar ladders, Aux LED, rotary LEDs, MIDI / EXTERN blink, split LEDs and the UNDO / REDO gating in headless egui frames |

## Not covered

- Plugin editor: the plugin crate is untouched; it can attach its own cell the same way (B11).
- Piano sound / sample swaps and program decoding on the audio thread are outside this spec (see `docs/TECH_DEBT.md`).
