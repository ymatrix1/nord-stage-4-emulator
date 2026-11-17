# Research: PNO core 1 JIT lockstep, the ARM cost of a tick, and the other cores (D119 measurements)

- **Checked:** 2026-11-16
- **Sources (local, never committed):** the DSP images of OS v1.66 (`dsp_image_{0..4}_*.bin`, `tools/ns4_dsp_ldr.py --sram`), the ARM firmware under `target/fw_unpacked/` and the EEPROM, the factory program `Bank A/B3 Chords n Solo.ns4p`; `tools/arm_org_stream.py`, `tools/arm_profile.py` (new), `sharc-sys --jit-diff` (new), `sharc-pass`. Builds on [org-system-run](org-system-run.md) (D119, proposed), [pno-full-pass](pno-full-pass.md), [fx-dsp-map](fx-dsp-map.md), [sharc-sy1sy2-smp](sharc-sy1sy2-smp.md).
- **Question:** (1) Does the Cranelift JIT match the interpreter on PNO core 1 for 300 passes of the real ARM stream, and why does the output lane stay silent? (2) How much of the ARM cost per pass interval is Unicorn, Python hooks, or the tick loop, and is the 17x the ARM or the harness? (3) Which of the five DSP images run a pass with the same stream, and what is unmodelled per core?

Measurement only. Nothing changes a default; D119 stays **proposed**. Provenance: every figure below is a measured run (`VERIFIED` for the run, the host is noisy, so timings are `EMPIRICAL`); the ARM tick of 200,000 instructions and 2 frames per pass stay `ASSUMED` (inherited from [org-system-run](org-system-run.md)).

## 1. Short answer

| Question | Answer |
|---|---|
| JIT vs interpreter, PNO core 1, real stream | **316 of 316 passes match** (16 warm-up + 300) for load, +1 key and +8 keys; full state + SRAM every 1,000 steps, `--strict` (PEy flags, float extensions, ASTAT extras, STKY, interrupt latches), no `--lenient`; no divergence. 7,880 to 7,891 steps per pass, 2,233 distinct pcs in 7 extents |
| Why is lane `2:0` silent? | Not the pass start, the interrupt or the lane. The stream never writes the Vox/B3 weight words `DM(0x95E00..0x95FFF)` (0 of 8,247 messages) and the installed routines are B3 and Farfisa generators; the ARM weight sender `0xC0224468` is not reached by the harness. Root cause **not proven** (section 3) |
| Hardware-clip metric | none: no program gives a non-silent lane from the stream; the only audible runs are the manual Vox and Pipe tests of [pno-full-pass](pno-full-pass.md), for which no hardware clip exists |
| The 17x ARM | **mostly the harness.** One no-op Unicorn hook costs a third of the tick; the Python callback bodies are 9 %; Unicorn on the real firmware runs 17 to 24 MIPS against 184 MIPS on a plain loop. With the hooks removed the ARM is still 8x to 11x real time, because the ASSUMED tick is 200,000 instructions per millisecond |
| Other cores | 5 of the 6 SHARC cores of the three mailbox images run a pass (ORG c2, ORG c1, PNO c1, PNO c2, FX c1); FX c2 runs one pass and then falls off its poll loop; SY1SY2 and SMP have no pass (FLAG12 start, interrupt driven) |

## 2. PNO core 1: JIT vs interpreter lockstep

New module `crates/nord-sharc/src/runner/system_diff.rs` (feature `jit`, read-only use of `runner::jit_diff::diff_pass_with`): the passes of the system run (ARM messages before their pass, ping-pong toggle, P4I start) run on two cores booted identically, one interpreted, one with `Core::run_jit`; `compare_state` after every `--chunk` steps; stops at the first divergence. CLI: `sharc-sys --jit-diff [--chunk N] [--strict]`.

| Run | Messages | Passes matched | Steps per pass | Distinct pcs | First divergence |
|---|---:|---:|---|---:|---|
| load only | 7,340 | 316 / 316 | 7,880 .. 7,891 (mean 7,889.7) | 2,233 | none |
| load + 1 key (+126 at pass 16) | 7,466 | 316 / 316 | 7,880 .. 7,891 | 2,233 | none |
| load + 8 keys (+907 at pass 16) | 8,247 | 316 / 316 | 7,880 .. 7,891 | 2,233 | none |

Coverage extents (gaps up to `0x40` merged): `1200B7-1200BD`, `121063-121B41`, `121BBE-122295`, `122A71-122D85`, `122E0A-122EF5`, `1233A4-1233B5`, `12637B-12637C`. The three runs execute exactly the same pcs: **keys change data, not control flow.** Each lockstep run took about 15 s (release build, both cores compared every 1,000 steps). Numbers: `data/pno-system-jit-diff.txt`. The unit tests cover the range merge, the harness plumbing with identical cores, a wrong second core (divergence reported at pass 0) and the JIT on the tiny test core.

Caveat: this covers the JIT's agreement with the interpreter on this stream, not agreement with hardware. 2,233 pcs is about a fifth of the 10,602 decoded instructions of the image.

## 3. Why lane `2:0` stays silent

| Hypothesis | Evidence | Verdict |
|---|---|---|
| Pass not started | P4I start (vector 15, SEC status 1) runs 7,881 to 7,891 steps, all 8 slot calls, back in the poll loop; the same start path with the Vox install gives peak 0.117, rms 0.042 ([pno-full-pass](pno-full-pass.md) section 4) | ruled out |
| Wrong lane | `sharc-pass --raw` over all 6 pointer sets x 3 pairs, 600 passes after the load plus the ASSUMED Vox weights: sets 1 to 5 hold at most 2.2e-39 (denormal residue); set 0 pair 2 is the constant `-2574.236` every pass (stale data, not audio); no lane carries a signal | ruled out |
| Missing interrupt or DMA | 392 device reads and 16,694 device writes in 316 passes (the writes are the output pointers `0x28AC0000/01E0` outside SRAM, about 53 per pass); no skipped operation, no fault, JIT = interpreter | not the cause on the SHARC side; the audio-input DMA is only a stub, but the B3 generators in this stream do not read it before the weights gate them |
| Missing ARM message class | the 7,340 load and 907 key messages touch `0x94000..0x95DFF`, `0xB0000..0xB2FFF`, `0xC0000..0xC2FFF`, `0xE0000+` and others, **0 of them `0x95E00..0x95FFF`**, where the Vox weights `PM(0x95E46 + 0x60 i)` live; their ARM writer `0xC0224468` is not in the capture | **most likely**, not proven |
| Wrong model installed | call targets after the load are `0x1200B7 0x121B33 0x121BBE 0x121E2B 0x122A71 0x122B85 0x12637B` (B3 and Farfisa generators and processors); the Vox pair `0x12272D` + `0x1229B3` is not installed, so the documented Vox weights have nothing to gate (silent again, section above) | explains why adding the ASSUMED Vox weights does not help |
| Boot-time messages missing | a capture with the fake DSP ports re-armed during the boot (`boot` phase, discarded) saw 14 path-1 SMP messages and no path-2 message, but re-arming overwrote firmware state and the later load phase lost its path-2 messages, so it is not a clean test; the plain capture has 0 boot messages | **not resolved**; the gains `DM(0xB44C0)` and `0xB2ECF ..` remain unwritten |

What would settle it: capture the ARM key path of a program whose model uses the weight sender (Vox or Pipe), or run the ARM until `0xC0224468` is reached for a B3 key and log that message class (open question OQ18 for the B3 and Farfisa processor weights).

Lane metric against a hardware clip: **not computed.** No program in this stream yields a non-silent lane, and the audible Vox and Pipe test chords have no matching hardware clip (`tools/emu_vs_hw.py` compares rendered notes, not oracle lanes).

## 4. ARM cost of a tick

`tools/arm_profile.py` boots the machine as `arm_org_stream.py` does, settles 800 ticks, then times 60 ticks per mode. The per-hook timers wrap every `Uc.hook_add` callback; the wrapper costs two `perf_counter` calls, so callback seconds are an upper bound. Instructions per tick: **200,000** (`INSNS_PER_TICK`, one injected timer interrupt per tick). Two runs on the same noisy host:

| Quantity | Run 1 | Run 2 |
|---|---:|---:|
| Plain harness tick (`run_ticking`) | 23.71 ms (8.4 MIPS) | 18.14 ms (11.0 MIPS) |
| Same with every hook timed | 25.36 ms | 23.72 ms |
| Python callback bodies | 1.79 ms | 1.61 ms |
| of which `install_tick` read hook (7,411 calls per tick, no-op) | 1.53 ms | 1.43 ms |
| of which poll-breaker hook (22 calls per tick) | 0.20 ms | 0.14 ms |
| `inject_irq` (timer interrupt) | 0.49 ms | 0.46 ms |
| Tick with the no-op `install_tick` hook removed | n/a | **12.06 ms** (16.6 MIPS) |
| Unicorn alone on the real code, all 33 to 34 hooks removed | 18.6 MIPS (10.7 ms per 200k) | 24.1 MIPS (8.3 ms per 200k) |
| Unicorn on a synthetic `add/subs/bne` loop, no hooks | 183.6 MIPS | 186.3 MIPS |

The `install_tick` hook is a read hook on the 8-byte time base that does nothing once the real timer is enabled (`tick_state["step"] == 0`, asserted by the tool): it fires 7,411 times per tick and costs about 0.8 us per C-to-Python crossing (18.14 ms to 12.06 ms, 6.1 ms), of which only 1.4 ms is the Python body. Split of the 18.1 ms tick of run 2:

| Share | ms | % |
|---|---:|---:|
| Python callback bodies | 1.6 | 9 |
| ctypes crossing of the no-op tick hook | about 4.7 | 26 |
| Timer interrupt injection and loop | 0.5 | 3 |
| Unicorn executing the firmware with the remaining hooks | about 11.3 | 62 |

Instructions per tick (the harness constant) are 200,000; Unicorn on the real code reaches 17 to 24 MIPS where a plain loop gives 184 MIPS. The gap is not analysed here (memory-mapped device windows, translation-block invalidation and the unmapped-page hook are candidates).

**Is the 17x the ARM or the harness?** The harness. The 17.7 ms per tick of [org-system-run](org-system-run.md) is 18 ms to 24 ms here (host noise); removing one no-op hook gives 12 ms (12x real time), removing all hooks 8 to 11 ms (8x to 11x). The firmware at the ASSUMED 200,000 instructions per millisecond needs 200 MIPS; Unicorn gives 17 to 24 MIPS on this code, so even a hook-free harness stays about 8x to 11x short. Batching callbacks cannot go below that floor; the levers are fewer instructions per tick (fast-forwarding the idle polls, so the ASSUMED budget becomes a measured one) or a faster ARM engine. The "nohooks" figure runs 20 M instructions without the timer interrupt, so its code mix may be a poll loop: treat 8 ms to 11 ms as the floor, not as a prediction. Numbers: `data/arm-profile-run.txt`.

## 5. The other cores with the same stream

All load messages of the capture go before pass 0 (`--script-flat`), 300 passes after 16 warm-up, interpreter, `sharc-sys`. ORG and PNO core 2 use `--pre 0x3E021=1` (ASSUMED, [org-system-run](org-system-run.md) section 5). Message counts are by receiving chip and core (path 2); the comm-index-to-image map is `ASSUMED` ([arm_emu](../../tools/arm_emu.py)).

| Image, core | Entry | Poll loop | Mailbox / trigger | Pass start | Load messages | Steps per pass | Result |
|---|---|---|---|---|---:|---|---|
| ORG core 2 | `0x125C88` | `0x125DDF` | `DM(0xB00E2)` | mailbox write | 912 | 3,171 .. 6,377 (first 4,309) | all 316 passes idle |
| PNO core 2 (organ program) | `0x125C88` | `0x125DDF` | `DM(0xB00E2)` | mailbox write | 912 | 3,172 .. 6,378 | all idle |
| ORG core 1 | `0x125C88` | `0x125DF3` | `DM(0xB00E1)` | P4I status 1, gates, FLAG1 | 912 | 3,182 .. 6,388 | all idle |
| PNO core 1 (piano program) | `0x121007` | `0x121091` | `DM(0xB00E1)` | P4I status 1, gates, FLAG1, ping-pong | 7,340 (+126 / +907 for keys) | 7,881 .. 7,891 (first 7,880) | all idle |
| FX core 1 | `0x1211E3` | `0x12128B` | `DM(0xC02C1)` | P4I status `0x41`, gates, FLAG1, `DM(0x28B00B00)=1` | 1,840 | 3,367 .. 3,370 (first 3,366) | all idle |
| FX core 2 | `0x1211E3` | `0x12125B` | `DM(0xC02C2)` | mailbox write, `DM(0x28300B00)=1` | 1,905 | pass 0: 3,220; pass 1 never returns | falls into the zero-filled vector table at `0x120000` (the null-stub gap of [fx-dsp-map](fx-dsp-map.md)); the stream's installs do not fix it |
| SY1SY2 | `0x1C0B2D` | `JUMP (PC,0)` at `0x1C0B33` | none: start is FLAG12 high | interrupt driven | 0 path-2 | not a pass image; start-up 8,780 steps ([sharc-sy1sy2-smp](sharc-sy1sy2-smp.md)) | not run |
| SMP | `0x1C11E0` | `JUMP (PC,0)` at `0x1C11FD` | none: FLAG12 high | interrupt driven | 1,516 path-1 | not a pass image; start-up 8,620 steps | not run |

Per-pass sum of the five cores that complete a pass: 6,377 + 6,378 + 6,388 + 7,891 + 3,370 = **30,404 steps** (33,624 with the 3,220 of FX core 2) per 46.08 us pass, i.e. 660 Msteps/s (730 with FX core 2). The interpreter reaches about 12 Msteps/s at best on this host, a factor of about 55x. Arithmetic on measured step counts, not a measured end-to-end run.

Unmodelled per core (all cores): audio-input and output DMA (stub), external-port DMA registers (`DM(0x3E021)` and neighbours, ORG and PNO core 2 only so far), the SPI frame latency and ordering of ARM messages, cycle counts. Per core:

| Core | Unmodelled or open |
|---|---|
| ORG c2, PNO c2, ORG c1 | `DM(0x3E021)` loop count (ASSUMED 1); gates of the Core1 sections preset by hand; no key message reaches ORG (keys go to PNO c1) |
| PNO c1 | key to weight messages (`0xC0224468`), Vox/Pipe install burst for the chosen program, output DMA (output pointers write the device store) |
| FX c1 | the effect routines of the program are installed by 1,840 messages and the pass is stable at 3,370 steps, but the output stage is not captured; no audio claim |
| FX c2 | pass 1 loses the poll loop (zero vector table); needs the state pointer the ARM install normally provides |
| SY1SY2, SMP | no pass concept: FLAG12 start, interrupt dispatch from the audio DMA, path-1 block stream (1,516 words for SMP, none for SY in this capture); the oracle runs their ISRs only through `sharc-run --enter` |

Numbers: `data/org-system-cores.txt`. The all-cores capture (`tools/arm_org_stream.py`, `TARGETS`) now writes a script per receiving core and phase.

## Implications

- The JIT can be used on the PNO pass of a real stream with the same confidence as on the ORG proxy states: strict lockstep over 316 passes finds no divergence, and keys do not change which code runs.
- The silent lane is a missing ARM message class (weights), not a SHARC or pass-start problem; fix it on the ARM side (reach `0xC0224468` for a key) before spending time on the DSP.
- Real time for the ARM needs fewer instructions per tick, not a faster Python loop; the hook cleanup (dropping one no-op hook) alone is worth 1.5x and is safe because `tick_state["step"]` is 0.
- Five SHARC cores per pass interval need about 660 Msteps/s; the interpreter at about 12 Msteps/s is 55x short, so a multi-core real-time run depends on the JIT or AOT work in other sessions.

## Follow-ups

1. Find the ARM path that sends the Vox or B3 weights (`0xC0224468` caller, key to flag map) and log that message class in the capture; replay it into PNO core 1 and compare lane `2:0` with a hardware clip (numbers only).
2. Remove the no-op `install_tick` hook once the real timer is on (touches `tools/arm_boot.py`, `arm_steady.py`, not done here: measurement only) and re-measure; fast-forward the idle polls to cut the 200,000 ASSUMED instructions per tick.
3. Explain why Unicorn runs the firmware at 17 to 24 MIPS against 184 MIPS on a plain loop.
4. FX core 2: provide the state pointer so that pass 1 returns to the poll loop.
5. SY1SY2 and SMP: drive their ISRs from a real stream (FLAG12, audio interrupt) so that they get a steps-per-pass figure.
6. Repeat the ARM timings on an idle host (the two runs differ by 30 %).

## Reproduce

```bash
python tools/ns4_dsp_ldr.py DSP/dsp_image_0_FX.bin --sram FX      # and PNO, ORG as in org-system-run.md
cargo build --release -p nord-sharc --features jit --bin sharc-sys --bin sharc-pass
# ARM side, all receivers (about 6 minutes of CPU, writes target/org_system5)
cd tools && uv run --with unicorn python arm_org_stream.py C/bank0_CODE.bin C/bank0_SRAM.bin \
  --eeprom target/oled/eeprom.bin "factory_programs/Program/Bank A/B3 Chords n Solo.ns4p" --out target/org_system5
# 1. PNO core 1 lockstep (repeat with --script-at 16:...-key1 / -key8 script)
target/release/sharc-sys PNO/core1_sram.bin --pno --jit-diff --strict --chunk 1000 --passes 300 --warmup 16 \
  --script-flat target/org_system5/pno-sys-load-core1.script
# silent lane: all 18 lanes of 600 passes
target/release/sharc-pass PNO/core1_sram.bin --script flat.script --passes 600 --quiet --raw raw.bin --calls
# 2. ARM profile
cd tools && uv run --with unicorn python arm_profile.py C/bank0_CODE.bin C/bank0_SRAM.bin \
  --eeprom target/oled/eeprom.bin "factory_programs/Program/Bank A/B3 Chords n Solo.ns4p" --ticks 60
# 3. other cores (examples)
target/release/sharc-sys FX/core1_sram.bin --entry 0x1211E3 --trigger 0xC02C1 --idle-pc 0x12128B --irq P4I:0x41 --flag1 \
  --dev 0x3108c06c=0x400 --dev 0x3108c00c=4 --dev 0x28ac0380=1 --dev 0x28b00b00=1 --boot 3000000 \
  --script-flat target/org_system5/fx-sys-load-core1.script --passes 300 --warmup 16
target/release/sharc-sys FX/core2_sram.bin --entry 0x1211E3 --trigger 0xC02C2 --idle-pc 0x12125B --dev 0x28300b00=1 \
  --script-flat target/org_system5/fx-sys-load-core2.script --passes 300 --warmup 16
```
