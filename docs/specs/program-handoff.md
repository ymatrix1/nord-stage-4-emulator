# Program Edit Hand-off (TD-12)

Status: v1.0. Implementation: `crates/nord-core/src/engine/handoff.rs`, `crates/nord-gui/src/audio/control/`.

Rule ([dsp-and-realtime](../../.agents/rules/dsp-and-realtime.md)): the audio callback never allocates, frees, locks or does I/O. Layer init, copy / paste / swap, Undo / Redo snapshots, STORE and recall build or drop whole layers, so they run on a control thread.

## Flow

```
UI / MIDI thread                                   audio callback
AudioHandle::send_command(cmd)
  ControlLink (Mutex, control threads only)
    plain command  -> replay on shadow, forward ------------------> apply_audio_command
    control op     -> shadow edit + capture
                      SwapState{Box<ProgramState>, mask} -------->  Stage4Engine::swap_state (mem::swap)
    Undo / Redo slots <---- Retired{old Box, ticket} (bounded) ----  try_send
```

| Item | Rule |
|---|---|
| Control ops (`ProgramEdit::is_control_op`) | `LayerInit`, `Undo`, `Redo`, `Copy`, `Paste`, `Swap`, `ClearMorph`, `Snapshot`, `Recall` |
| Shadow engine | `standalone_engine(sample_rate)` replays every command and every non-note MIDI event. Created when the audio thread knows the sample rate (`ControlLink::attach`); earlier commands are recorded (cap 8192) and replayed in order |
| `StateMask` | which parts a swap exchanges: per layer panel / fx / level (+ Synth cutoff), `MORPH` (all depths), `SCENE` (scene masks and split router), `PROGRAM` (transpose, level, tempo, compressor, FX sharing). A LAYER INIT of Piano B swaps only Piano B, so other layers keep sounding |
| Undo / Redo | single level as before. The box the audio thread returns holds the replaced parts (the exact truth, not the shadow's copy); the control thread releases its voices (`ProgramState::release_voices`) and stores it with its mask. Undo sends it back and receives the post-edit parts as the Redo snapshot. Tickets drop stale answers |
| Audio side | `SwapState::apply`: `swap_state`, `set_undo_hint`, `try_send` of the old box. If the bounded return channel is full the box is leaked (never freed on the audio thread) and `Stage4Engine::retire_overflow` counts it; a control op that reaches the callback anyway is dropped the same way |
| Undo availability | the engine's `undo_available` / `redo_available` report the control thread's `UndoHint` when its own slots are empty |
| Snapshot (STORE, Live auto-store) | answered immediately from the shadow; no audio round trip |
| Headless handles | `AudioHandle::new()` has no control link and forwards everything unchanged (tests); `AudioHandle::standalone()` is what `start_audio_and_midi` uses |

## Provenance

| Tag | Item |
|---|---|
| ASSUMED | `SETTLE_TIMEOUT` 250 ms (Undo waits for the returned snapshot), `SEND_TIMEOUT` 50 ms, return capacity 16, backlog cap 8192: sizes chosen for button-press rates, no measurement |
| VERIFIED | audio-side allocation freedom: `crates/nord-gui/tests/no_alloc_program_ops.rs` (79 allocator calls for the old path, 0 for every control op) |

## Limits

- The shadow only knows what commands and non-note MIDI tell it. A change made solely by the audio thread without a command is not in a STORE snapshot.
- `LoadProgram`, sound swaps and the critical-MIDI retry in `send_midi` are outside this spec (see TD-12 "Remaining").
