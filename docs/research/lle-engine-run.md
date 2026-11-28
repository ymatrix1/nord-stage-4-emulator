# LLE engine run: ARM0 plus the DSP cores on one virtual clock (`ns4-lle5`)

- **Checked:** 2026-11-27
- **Sources (local, never committed):** OS v1.66 `target/fw_unpacked/bank0_CODE.bin`, `bank0_SRAM.bin`; EEPROM `eeprom.bin` (factory restore, no program) and the same image with the factory program "B3 Chords n Solo" recalled (`tools/arm_program_load.build_image`); flat SRAM images `PNO/core1_sram.bin`, `PNO/core2_sram.bin`, `ORG/core2_sram.bin`, `SY`, `SMP`, `FX/core{1,2}_sram.bin` (`tools/ns4_dsp_ldr.py ... --sram`, laid out as in `crates/nord-lle/src/load.rs`). Code: `crates/nord-lle`, `lle_engine_run`. Background: [org-system-run](org-system-run.md), [pno-full-pass](pno-full-pass.md), [arm0-rust-runtime](arm0-rust-runtime.md), [sharc-lle-realtime](sharc-lle-realtime.md), [smp-page-refill](smp-page-refill.md), [dsp-to-arm-return](dsp-to-arm-return.md).
- **Question:** Does the composed engine (ARM0 boot, messages routed to the loaded DSP cores, passes through the audio bus to a host buffer) run without a fault, with equal ARM and DSP state over repeated runs, and does any stage produce audio?

Commands (`IMG` = image directory):

```text
lle_engine_run --dir IMG --ticks 4000 --runs 2 --note-on 1500:60:100 --note-off 3000:60
lle_engine_run --dir IMG_B3 --ticks 12000 --runs 1 --note-on 6000:60:100 --note-off 10000:60
```

## Result (summary)

**Audio: none.** Every node input and output is 0.0 in every run, host buffer all zero. **What is established** is "ARM state + DSP core state + counters equal across runs; audio all zero". It does **not** establish that audio is reproducible.

| Item | Result | Tag |
|---|---|---|
| ARM0 boot | 232,822,686 virtual instructions to the main loop, 1.3 to 2.7 s wall | EMPIRICAL (busy host) |
| DSP boots | all seven loaded cores boot under their presets in 0.15 to 0.31 s | EMPIRICAL |
| Faults | none in 4,000 ticks (3,616 passes, factory EEPROM, 2 runs) and in 12,000 ticks (10,850 passes, B3 EEPROM, 1 run); the old defaults faulted ORG core 2 at pass 19 (`DM(0x3E021)`), fixed by the `epdma` model for the tone chips | EMPIRICAL (run) |
| Cores | **driven:** PNO core 1, PNO core 2, ORG core 2, SMP, FX 1, FX 2. **SY loaded, idle** (no message addressed to comm 3). **ORG core 1 not loaded:** under the `org` preset it does not return to the poll loop (2,000,000 steps at pass 0); its 912 messages are counted without a chip. The first version of this note and of the PR said "six DSP cores" and dropped 1,824 messages (ORG core 1 and PNO core 2, 912 each); loading PNO core 2 leaves 912 | EMPIRICAL (run) |
| Messages (4,000 ticks, factory EEPROM) | 15,335; routed `(comm, core)`: FX1 1,844, FX2 1,905, PNO1 7,334, PNO2 912, ORG2 912, SMP 1,516; counted without a chip 912 | EMPIRICAL (run); agrees with the per-chip counts of [org-system-run](org-system-run.md) s2 |
| Key event | `NoteOn` through the `lle_io` ring reaches UART2; PNO core 1 gets 42 extra messages afterwards (factory EEPROM) | EMPIRICAL (run) |
| Equality over runs | two full runs (boot included) give digest `a27fa5bec6dce499` (re-run 2026-11-28 after the digest read `dm_peek` over `DM 0x90000..0x100000`; the earlier `a47d8a49fd98e499` used another window); the digest covers host buffers, ARM state hash, counters and each core's registers and data window `DM 0x90000..0x100000` (PM, L2 and SDRAM not hashed, ASSUMED) | EMPIRICAL (run); unit test `real_nodes_are_deterministic_and_the_digest_sees_dsp_state` on real `CoreNode`s |
| Wall time (factory EEPROM, 4,000 ticks) | 17.1 and 18.0 s = 102x and 108x slower than real time (DSP bus 16.4 / 17.4 s, ARM 0.62 / 0.63 s); B3 EEPROM run 47 s for 4,000 ticks (283x), 78 s for 12,000 (155x) on a host at 85 to 100 % CPU from other sessions | EMPIRICAL, one or two runs each, not a median: read as an order of magnitude |
| FX input | 4 frames short at the first FX pass of each core (one hop of latency) | EMPIRICAL (run) |

## Silence bisect

Per-stage telemetry (peak / RMS / first non-zero tick of each node's input and output) is in the bin output.

| Run | Input | Per-stage peak | Host |
|---|---|---|---|
| factory EEPROM, 7 chips, 4,000 ticks | key at 1,500 | all 0.0 (pno1, pno2, org2, sy, smp, fx1, fx2, in and out) | 7,232 frames, 0 non-zero |
| EEPROM with B3 program recalled, 4,000 ticks | key at 1,500 | all 0.0 | 7,232 frames, 0 non-zero |
| same, 12,000 ticks (0.5 s virtual) | key at 6,000 to 10,000 | all 0.0 | 21,700 frames, 0 non-zero |
| **standalone** PNO core 1, known Vox message script (`lle_sharc_bus` path), same preset and tap | scripted key and weights | pno1 out peak 0.117, rms 0.042 | audible |

Findings:

1. **Candidate 0 (missing cores) is not the cause.** Loading PNO core 2 changed nothing; ORG core 1 cannot be loaded under the `org` preset.
2. **Tap is right.** The tone chips now tap `OutTap::Lane{set 2, pair 0}` (`DM(0xB015B)`, VERIFIED in [pno-full-pass](pno-full-pass.md) s1); the standalone PNO core 1 with that tap and preset is audible. The earlier tap (set 0) was wrong; correcting it did not make the engine audible.
3. **The zeros are real zeros, not NaN.** The stage lines carry a non-finite counter (`nf`): 0 at every node input and output in the 4,000-tick run, so no stage reads as silent because its samples were dropped as NaN or infinite.
4. **Zero at the chip output**, not only at the host or the FX input: PNO core 1's own output is 0.0 for the whole run, so FX and host are zero because their input is.
5. **The difference is the message stream.** In the standalone run the script sends the key flag and weight words to PNO core 1. In the ARM-driven stream no such word arrives after the key: the extra messages after `NoteOn` are periodic housekeeping triples. SY gets no message at all.
6. **EEPROM state is not the whole answer.** Recalling the B3 program into the EEPROM changed the message counts (PNO core 1 7,334 to 7,693 in 4,000 ticks) but not the silence, also not in 0.5 s of virtual time.

## What is missing to get sound (data or work, not a plain bug)

* The ARM-side writer of the key-flag and weight words for the Vox / B3 path is an open item of [pno-full-pass](pno-full-pass.md). The engine does not reproduce it; either the ARM stream needs an input it does not get (panel or keybed scan, which `Arm0` does not model: UART2 MIDI is the only input), or the sender lives in a part of the firmware not reached in 0.5 s of virtual time.
* A recall that reaches the tone chips (program change through the ARM menu or MIDI program change) instead of an EEPROM-stored program; not tried.
* The ARM tick budget `TICK_INSNS` (200,000, ASSUMED) sets how much ARM work happens per DSP pass; if the sender is timing sensitive this matters. Not varied.

No audio claim is made. A non-silent deterministic render does not exist yet.

## Implications

* The engine composes, boots seven cores and reproduces ARM and DSP state over runs; it is the base for a null test once a chip produces audio.
* At 100x to 280x over real time on this host the engine is an oracle, as Gate 1 said; the DSP bus is 96 % of the wall time, the ARM 4 %.

## Follow-ups

1. Find the ARM-side key-flag and weight sender (`pno-full-pass.md` open item) and feed it the missing input; add a MIDI program-change recall run.
2. Vary `TICK_INSNS` and the run length to see whether the stream changes.
3. Live-chip and initial-image return layers.
4. Panel key map for `PanelEvent`, OLED/LED telemetry, 48 kHz resampling.
5. `.npno` refill against a real SMP run (only synthetic here).
6. ORG core 1 start-up under its own preset.
