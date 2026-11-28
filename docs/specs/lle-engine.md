# Unified LLE engine (`ns4-lle5`)

Status: implemented as an **offline, deterministic, slower-than-real-time** engine (Gate 1 found real-time exact LLE out of reach, [sharc-lle-realtime](../research/sharc-lle-realtime.md), D249). Code: `crates/nord-lle`; binary `lle_engine_run` in `crates/nord-tools`. Decision: D253. Measured run: [lle-engine-run](../research/lle-engine-run.md).

## Purpose

One Rust object, `LleEngine`, that boots ARM0 from the user's OS update, lets the firmware drive the five DSP images through the messages it emits, runs the DSP passes through the inter-chip audio bus and collects a host stereo buffer. Use: oracle, offline renders, null tests against the HLE engine, base for a hybrid. Not a playable instrument: no real time, OLED/LED and the panel key map are stubs.

## Modules

| Module | Role |
|---|---|
| `clock` | virtual time: ARM ticks to DSP passes |
| `arm` | `ArmHost` trait; `Arm0` (real, `nord-arm`) and `FakeArm` (scripted, tests) |
| `route` | ARM `Message` to `(comm, core)` node, L2 window forward, counters |
| `smp_refill` | `.npno` stroke table, descriptor detection, flash binding |
| `input` | `LleEvent` to ARM MIDI bytes / panel |
| `load` | directory layout, preset per image, default audio routes |
| `engine` | `LleEngine<A: ArmHost>`: composition, `step`, `render`, `digest`, `LleInput` / `LleTelemetry` |

Reused unchanged: `AudioBus` / `BusNode` / `CoreNode` / `SyNode` / presets (`crates/nord-sharc/src/runner/`), `Machine` (`crates/nord-arm`), `lle_io` rings and traits (`crates/nord-core`). Added to `nord-sharc`: `BusNode::post` (queue one ARM message for the node's next pass), `AudioBus::{post, every, node_ns, node_levels}` (per-node wall time and input/output `Level` = peak, RMS, first non-zero tick).

## Who owns time

The **ARM timer tick** (24 kHz, 41.67 us, D130) is the master clock. One `LleEngine::step` is one tick, in this fixed order:

1. `ArmHost::tick`: ARM0 runs `TICK_INSNS` (200,000, ASSUMED) with idle skipping, then the timer interrupt.
2. Messages completed in that tick are drained, each routed and `post`ed to its node.
3. `clock.tick()` returns the DSP passes owed: `floor(t * 1,388,875 / 1,536,000)` cumulative (0.9042 per tick, integer, `tools/arm_dsp_link.py`). Each is one `AudioBus::run_tick` (one pass period = 2 frames at `FRAME_RATE` 43,402.34375 Hz; FX cores every 2nd tick).

Messages posted in step 2 are delivered before the node's next pass (`before_pass`). A node that runs every pass sees a message within that tick's pass; an FX node (`every = 2`) can wait for its next pass, up to two ticks. A message's pass number is the node's own pass counter, not a wall clock.

The host buffer is at the **DSP frame rate**, not 48 kHz; resampling is not implemented.

## Message routing

`route::classify` (port of `tools/arm_dsp_route.py::Router`). Comm index FX 0, PNO 1, ORG 2, SY 3, SMP 4 (ASSUMED, `arm-param-messages.md` s3).

| Message | Destination |
|---|---|
| path 2, comm 0..2, `0x28000000 + 4w` | core 1, DM word `w` |
| path 2, comm 0..2, `0x28800000 + 4w` | core 2, DM word `w` |
| path 2, `0x20000000..0x28000000` (L2 window) | core 2 of the same chip, DM word `byte >> 2` |
| path 2, word `>= 2^20` | counted `skipped` |
| path 1, comm 3 or 4 | SY / SMP node (FIFO address, `SyModel::send`) |
| anything else, or no node owns it | counted (`router.counted`) |
| a bad SMP descriptor (`rec11` outside the directory) | counted `stats.bad_descriptors`, skipped, no fault |

Nodes are registered by `(comm, core)`; a missing image just leaves its messages counted. Driven in the run so far: PNO core 1 and 2, ORG core 2, SMP, FX 1, FX 2 (six nodes, `SLOTS`). **SY is loaded but idle** (no message addressed to comm 3). **ORG core 1 is not loaded** (under the `org` preset it does not return to the poll loop, 2,000,000 steps at pass 0) and its 912 messages are counted without a chip. Basis: of the 8 DSP cores (PNO 1 and 2, ORG 1 and 2, SY, SMP, FX 1 and 2), 7 are loaded, the ARM addresses 7 (not SY), 6 are driven, SY is idle, ORG core 1 is not loaded. Claiming "all six DSP cores driven" is wrong.

## DSP to ARM return path (`ns4-nds8`)

The three SPI read routines are answered inside `Machine` (`install_dsp_return`, installed by `Arm0`): layer 1 the SY idle voice mask, layer 2 the shadow of every word the ARM itself sent (`DspReturn::observe`, fed by `drain_messages_into`), else 0. **Not ported** (noted in `docs/research/arm0-rust-runtime.md`): the live-chip layer (read the word from the running DSP core) and the initial-image layer. Both need a synchronous call from inside an ARM hook into a DSP node; the engine runs the ARM first and the DSP afterwards, so a live read would see the state of the previous tick. Open.

## SMP page refill (`ns4-pnoc`)

`bind_smp(npno)` (file capped at 512 MB like `live_flash::load`) gives the SMP core the file as flash (`epchain` model, external-port DMA) and starts a `SmpRefill` watcher on every SMP message. A descriptor is `rec[-11]`, `rec[-10]`, `rec[-9]` in consecutive DM words with `rec[-9]` a stroke frame count; if `rec[-10]` obeys the page law (`smp-page-refill.md` s2) and `rec[-11]` is new, `live_flash::place` positions the file and writes the directory. A `rec11` that would overflow or leave the directory is counted as a bad descriptor and skipped (`live_flash::place` returns `Err`, no panic); `parse_strokes` uses checked arithmetic on firmware-controlled offsets. Without a bound file the SMP chip gets its messages but its page reads find no flash.

## Input and telemetry

`LleInput::handle_event`: note, CC, pedal, pitch bend, aftertouch, program change become raw MIDI bytes on ARM UART2 (`ArmHost::midi`). Panel events go to `ArmHost::panel`, which `Arm0` does not model (control id to key-matrix table not ported): counted as `events_unmodelled`. `LleTelemetry::fill_telemetry` fills the frame counter and ARM milliseconds only; OLED and LEDs stay zero (stub, `ns4-lle6`).

## Precision

`Precision::{Exact, Fast}` is a constructor parameter. Only Exact exists (`ns4-lle3` has no f32 mode): `Fast` is accepted, runs Exact and `effective_precision()` says so.

## Determinism rules

* single thread, fixed order (ARM, route, passes), no wall clock in any decision;
* pass count from integer arithmetic; maps are `BTreeMap`;
* DSP and ARM state are pure functions of images and event stream (`Machine::state_hash`, bit-exact SHARC core);
* `digest()` = FNV-1a over host buffers, ARM state hash, routing counters **and the state of every DSP core** (`engine::core_digest` over each node's `core_mut()`: PC, step counter, register files, MODE1/2 and the data window `DM 0x90000..0x100000` (read with `dm_peek`); ASSUMED: nothing the passes write lives outside it, PM, L2 and SDRAM are not hashed); wall times (`Stats`, `node_ns`) are excluded. Test: `real_nodes_are_deterministic_and_the_digest_sees_dsp_state` runs real `CoreNode`s twice on a synthetic program and compares the full digest;
* until a chip produces output the determinism result is "ARM state + DSP state + counters equal; audio all zero". It does not show that audio is reproducible;
* an error from ARM or a node is sticky: `step` keeps returning it, nothing runs on.

## Stubs and limits

Not real time; allocates while running (`Vec` in the bus and message queues); panel key map, OLED/LED, live-chip return layers, Fast precision, 48 kHz resampling, SY `epchain`, program-store wiring: not done. Default audio wiring (all tone chips into FX1, FX1 into FX2, FX2 to host) is ASSUMED.

## Image layout

See `crates/nord-lle/src/load.rs`: `DIR/arm/{bank0_CODE.bin,bank0_SRAM.bin,eeprom.bin}`, `DIR/<PNO|ORG|SY|SMP|FX>/core<N>_sram.bin`. Images are read at run time and never committed (D20, D128).

## Acceptance (`ns4-lle5`)

| Criterion | State |
|---|---|
| unified `LleEngine`: ARM0, 5 DSP images, return path, SMP refill | composed and run end to end on local images; return path partial (two layers), refill tested synthetically only |
| pre-allocated, no alloc/lock/IO in `process_block()` | **not met**: offline engine, no `process_block` |
| CLI render smoke test | `lle_engine_run`: boots, renders, per-stage peak/RMS; ARM + DSP state deterministic; **output all zero** in every run so far ([lle-engine-run](../research/lle-engine-run.md)); a fault exits non-zero |
| synthetic tests: lockstep scheduling, zero-alloc rendering | scheduling, routing, return, refill, determinism, fault: yes; zero-alloc: no |
