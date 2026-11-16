# Emulation coverage: oracle and audio emulator per subsystem

- **Checked:** 2026-11-15
- **Sources:** [sharc-census-diff](sharc-census-diff.md), [sharc-routine-diff](sharc-routine-diff.md), [sharc-leftovers](sharc-leftovers.md), [sharc-sy1sy2-smp](sharc-sy1sy2-smp.md), [sy-smp-audio](sy-smp-audio.md), [sy-pitch-entry-writer](sy-pitch-entry-writer.md), [synth-sample-kernel-rule](synth-sample-kernel-rule.md), [oracle-vs-engine](oracle-vs-engine.md), [arm-fw-emulation](arm-fw-emulation.md), [arm-organ-messages](arm-organ-messages.md) s10, [arm-bulk-tables](arm-bulk-tables.md), [program-coverage](program-coverage.md) with [data/program-sweep.json](data/program-sweep.json) (2026-11-14), `synth-stage-models*.md`, [synth-validation](synth-validation.md), [audio-remaining](audio-remaining.md), `crates/nord-core/src/render_opts.rs` (defaults), `docs/DECISIONS.md` (D22 to D116), `docs/TECH_DEBT.md` (TD-14, TD-15), `docs/OPEN-QUESTIONS.md`. D113 to D116 (PRs #437 to #440) merged into `main` after the first draft of this note; the rows below were updated on 2026-11-16 (merge of `origin/main` up to PR #448). The `sharc-21571-handover` session note (PRs #118, #131, #142) is a sibling note, weighed as data.
- **Question:** per subsystem, what does the project reproduce 1:1 (verified against firmware or oracle), what is fitted to hardware clips, what is assumed, what is missing, what is the default versus opt-in state, and what does closing each gap cost and change audibly?

Tags: **VERIFIED** = matches firmware or oracle by a reproducible check, **EMPIRICAL** = fitted to a recording, **ASSUMED** = chosen without evidence, **unmodelled** = pass-through or absent. A default flips only when verified end to end (D22). Effort (low / medium / high) and audible impact are **estimates** (ASSUMED), not measurements. A number the repo does not hold is written `unknown`.

## 1. Short answer

| Question | Answer |
|---|---|
| Is the emulation complete? | **No.** Every engine family sounds; the oracle runs a large part of the firmware; both have named gaps below. |
| Oracle instruction level | 8,287 distinct firmware instructions run on the model and on the ADSP-21489 simulator: 49,722 cases, **18 differ** in 6 shapes. The census is not yet run on the 21571 (SHARC+) simulator. |
| Oracle routine level | PNO bank `0x122620` and mix `0x12268F` bit-exact on the 21571 simulator; output stage `0x1229B3` and post stage `0x12272D` DM compared in session notes (PRs #131). SY1SY2 resampler `0x1C10EE` and SMP voice loop bit-exact against the Rust engine. |
| ARM harness | Boots the real firmware to the main loop in **433 M instructions (36 s)**; full settle 3.0 B instructions in 201 s. The DSP side is absent: `DspAck` stands in for it. |
| Factory programs (384) | Path uses classified: default **1,015 verified (15 %), 5,677 assumed (83 %), 119 unmodelled (2 %)**; `--firmware-all` **2,634 / 4,058 / 119** (39 % / 60 % / 2 %). Render health 384 of 384 without panic, NaN or silence. |
| Hardware evidence | 22 Part A segments (`emu_vs_hw_segments.json`: 3 organ, 5 piano, 11 synth, 1 piano + pad, 2 FX), 17 synth validation clips (6 fit, 11 hold-out). One chorus clip for FX (`fx-chorus-hw-clip`). No FM, Wavetable or init-patch clip. |
| Real time | Default path meets the 0.8 ms block budget for a typical program (0.67 to 0.70 ms); worst case 2.2 to 2.5 ms (TD-14). B3 Bass (3.3 to 3.6 s CPU per s) and Farfisa (about 2 s per s) firmware stages are offline only (TD-15, D71, D74). |

## 2. ROADMAP and PRD view

Counts are `- [x]` against all `- [ ]` and `- [x]` lines under each heading of `docs/ROADMAP.md` on this branch (nested sub-items count), computed by script, 2026-11-15, recomputed 2026-11-16 after the merge of `origin/main`. The Phase 2 header says its ticks mean "the model exists and is tested"; parity with the firmware is tracked in Phase 7b.

### 2.1 Progress per phase and lane

| Phase / lane (bean) | Done / total | Subsystems covered | Status against the evidence | Open gap | Effort | Impact |
|---|---|---|---|---|---|---|
| 0 Reverse engineering (`ns4-3387`) | 7 / 7 | specs, teardown | complete | none | none | none |
| 1 Formats (`ns4-cbin`), Lane A | 11 / 11 | CBIN, `.ns4b`, presets | VERIFIED: 544 presets and 384 programs re-pack to their own bits (OQ31); OQ7, OQ26, OQ27, OQ30 still open | some field meanings and write-back | medium | low |
| 2 Organ DSP (`ns4-orgn`), Lane B | 7 / 7 | B3, Vox, Farfisa, Pipe, rotary models | model exists; firmware parity only for Pipe and opt-in paths (7b.3) | section 4.1 | high | high |
| 3 Synth DSP (`ns4-synt`), Lane B | 8 / 8 | analog, wavetable, FM, sample, LFO, arp | model exists; strip chain is the default (D59); Wavetable and FM firmware paths opt-in (7b.6) | section 4.3 | medium | medium |
| 4 Piano DSP (`ns4-pno4`), Lane B | 26 / 29 | sample voice, level law, slew, Touch, transcription | VERIFIED level law and voice; slew and Touch opt-in | SMP bus receiver, reconcile two clips, absolute level chain | medium | high |
| 5 Routing, morph, FX (`ns4-fxrt`), Lane B | 7 / 7 | routing, zones, morph, FX racks, MIDI | engine models exist; routing VERIFIED (D73); FX types mostly assumed | section 4.4 | medium | medium |
| 6 Panel GUI and plugin (`ns4-707f`), Lane C | 4 / 12 | panel, standalone, CLAP/VST3 | not audio | panel bays 1 to 6 | high | none for audio |
| 6b Hardware parity and panel (`ns4-xc5n`), Lane C2 | 11 / 19 | control inventory, renderer, editor | not audio | renderer, goldens, host tests | high | none for audio |
| 7 Firmware and CNSP codec (`ns4-trkb`), Lane D | 9 / 9 | LZ unpack, CNSP, benchmark loop | complete; PRD NG2 respected | none | none | none |
| 7b.1 SHARC oracle, Lane E | 8 / 12 | core, census, routines | VERIFIED (section 3.1) | P2..P8 probes, SPORT/DAI, aliases, speed | medium | none today |
| 7b.2 ARM harness, Lane E | 14 / 17 | boot, capture, tables | VERIFIED runs; the DSP side is absent | key-down capture, SY/SMP equality, real flash reads, Vox level word | high | medium |
| 7b.3 Organ parity, Lane E | 22 / 30 | B3, Vox, Pipe, Farfisa, Bass, Leslie | mixed (section 4.1) | B3 processor, percussion, real time, Pipe clip | high | high |
| 7b.4 Effects parity, Lane E | 23 / 28 | Mod 1, Mod 2, amp, delay, reverb | mixed (section 4.4) | FX wiring, enable gating, hardware clips (RM, WAH, PUMP, ENS), reverb tail clip | medium | high |
| 7b.5 Piano parity, Lane E | 4 / 9 | transcription, velocity, levels | Felt Upright and transcription done | pitch and level laws into `PianoLayer`, Transkun adapter, velocity law, stream edges | medium | medium |
| 7b.6 Synth parity, Lane E | 45 / 53 | stages, filters, envelopes, velocity, FM, WAVE, SAMPLE | 312 of 312 stage models; P rows EMPIRICAL | pitch entry `e`, parity remainder, hardware sweeps | high | medium |
| **Total** | **206 / 258** (7b: 116 / 149) | | | | | |

Lane E beans (`ns4-36hw`, `ns4-doy1`, `ns4-vnp2`, `ns4-qll9`, `ns4-ca0i`, `ns4-af6y`, `ns4-wzyz`) map to 7b.1 to 7b.6; this note does not read the bean files, so per-bean state is `unknown`.

### 2.2 PRD goals

| Goal | State | Evidence |
|---|---|---|
| G1 Format interoperability | met for the factory set | Phase 1 11 / 11; OQ31 |
| G2 Authentic 3-engine, 7-layer pipeline | partly: every engine sounds; 83 % of default path uses assumed | section 5 |
| G3 Zero-allocation real-time path, under 3 ms per 128 frames | met for the default path (typical 0.67 to 0.70 ms, worst case 2.2 to 2.5 ms, TD-14); Bass and Farfisa firmware stages offline (TD-15) | `ns4 bench` |
| G4 Morph and split matrix | engine done (Phase 5, 7 / 7); morph slots unmodelled against the firmware | section 4.5 |
| G5 MIDI CC parity | decoder and encoder done (Phase 5) | [midi-and-control](../specs/midi-and-control.md) |
| G6 100 % tested | met: `check_tests.py` 306 of 306 files covered, baseline 0 | CI |

### 2.3 ROADMAP versus reality

**Ticked, evidence does not fully support the wording**

| Tick | Where | What the evidence says |
|---|---|---|
| Wavetable ("64 band-limited mipmapped single-cycle Wavetables") | Phase 3 | the hardware has 46 firmware tables; the engine plays a placeholder unless `--synth-wave-bank` (12 programs) |
| FM router | Phase 3 | wave audio ASSUMED; firmware FM stages opt-in; no FM hardware clip |
| `Super Saw` (7 detuned PolyBLEP saws) | Phase 3 | the default is the firmware SUPER stage (D59); the PolyBLEP saw is the legacy chain |
| Reverb FDN for six types | Phase 5 | firmware tails are 0.5 to 1.35 s against engine 3.4 and 7.9 s (OQ21c); the FDN is the default but not a parity model |
| Organ models | Phase 2 | its header says the ticks mean "model exists"; B3, Vox, Farfisa, Bass parity is open in 7b.3 |

**Delivered but unticked (fixed in this PR)**

- The 7b.2 item "Engine: honour section I / II and zone flags ... Farfisa processor port; layer level law; ..." was one line with three delivered parts (routing D73, Farfisa PR #376 and D74, layer level law D25 and D87). It is split: the delivered part is ticked; the organ_b3 re-run, the Vox level word and real flash reads stay open.
- The 7b.6 item "Remaining stage models (then SAW / SQUARE / ...) bit exact" repeated work that the ticks below it record (312 of 312 ANALOG layers). The stale half is removed; the synth parity remainder stays open.

**Merged since the first draft (PRs #437 to #440):** the ENS item in 7b.4 is now ticked by #438 (D114). The Transkun adapter item in 7b.5 stays open although D115 (#439) records the adapters as not adopted: reword or close it.

**Scope that grew beyond the ROADMAP**

| Area | Evidence | ROADMAP state |
|---|---|---|
| Transcription tooling (Transkun, MuScriptor, adapter plan and training) | PRs #381, #407, #424, #439; open #425, #430 | two lines in 7b.5 only |
| Hardware validation clips (22 Part A segments, 17 synth clips, `emu_vs_hw.py`) | `emu_vs_hw_segments.json`, `synth_validation.json` | scattered ticks, no phase |
| Listening packs (WAV A/B sets for the user) | the `listening-wavs` session; no committed file on `main` | not in the ROADMAP |
| Firmware-derived opt-in paths (about 35 fields in `FirmwareOptions`, D22) | `render_opts.rs` | each has a tick, no summary of default flips |
| GUI keyboard and MIDI port (B13b, B13c), mouse velocity (D84) | PRs #379, #393 | B13b and B13c ticked; mouse velocity absent |
| Piano and synth velocity (D84, D88, D99, D104) | decisions | ticks in 7b.5 and 7b.6; Touch default flip absent |

**Proposed checkboxes added to `docs/ROADMAP.md` in this PR** (unticked, marked "Proposed 2026-11-15"): Mod 1 PUMP clock source and reverb tail clip (7b.4); extra piano voices and Touch default flip (7b.5); envelope pass and ARM tick, arpeggiator gate, Wavetable default, FM level clip (7b.6); B3 processor and Leslie ring DMA (7b.3).

### 2.4 Gap to ROADMAP map

| Gap | ROADMAP item |
|---|---|
| Organ `K`, level chain | 7b.6 tick "Organ level K = 2.8" (audit done); open part: 7b.3 "Pipe-only hardware clip, organ level L"; 4 "Close the absolute level chain" |
| Reverb tails | 7b.4 "FX wiring ... reverb tables" (open); tail clip: proposed in this PR |
| Touch default | 7b.5 Touch tick; flip: proposed in this PR |
| Piano extra voices | proposed in this PR |
| Sample pitch entry `e` | 7b.6 "SAMPLES remainder" (open) |
| B3 processor, Leslie | 7b.3 "B3 / Vox presence", "Real time firmware organ stages", "Organ unit EQ routines, Leslie rotor" (open) |
| Mod 1 PUMP clock | proposed in this PR |
| Filter factors, envelope pass | 7b.6 "synth parity remainder" (open); envelope pass, tick: proposed |
| FM level, Wavetable default | 7b.6 FM and WAVE ticks; flips proposed |
| Bass and Farfisa real time | 7b.3 "Real time firmware organ stages", "B3 Bass: ... a real time path" (open) |
| Oracle gaps (DMA, ring consumer, loop-end call, vectors) | 7b.1 "Model the 0x28xxxxxx core aliases, SPORT/DAI"; 7b.2 "Key-down capture", "SY / SMP equality"; loop-end call and vectors 1 and 12: not in the ROADMAP |
| 21571 census | 7b.1 "Oracle speed" (open); the census itself on 21571: not in the ROADMAP |

## 3. Part A: the oracle (firmware emulation)

### 3.1 SHARC core model (`crates/nord-sharc`)

| Area | State | Evidence | Tag |
|---|---|---|---|
| VISA decoder | Matches ADI `elfdump` on the code of all five images; `adi_compare.py` 0 differences for types 8a, 9a/9b, 11a/11c, 12a, 19a on SY1SY2 and FX, PNO, ORG code ranges | [sharc-leftovers](sharc-leftovers.md) s1 | VERIFIED |
| Executor, census | 8,287 distinct instructions (after dropping what ADI decodes differently, control flow, timer, stack, interrupt registers and MODE1 writes) x 6 random states. First run 1,875 of 50,052 differ; after 12 fixes **18 of 49,722** (6 shapes: `STKYx = DM (LW)`, conditional multiply + dual add/subtract + swap, SIMD per-element conditions on shifter flags, `MR1F` readback, SIMD `FPACK`) | [sharc-census-diff](sharc-census-diff.md); 1,104 cases replayed by `census_regress.rs` | VERIFIED (21489 simulator) |
| Excluded from the census | control flow, `DO UNTIL`, stacks, timers, interrupts, MODE1 writes: covered by separate probes (`sharc-seq/loops`, `stack`, `nested`, `entry`, `simd`), all with committed measured fixtures | `docs/research/sharc-seq/` | VERIFIED |
| SHARC+ (21571) | Write masks of ASTATy, STKYx, STKYy measured; condition-code sweep of 40 cases agrees; **no instruction census on 21571** (about 0.3 s per case after a 140 s start-up, about 4 h; session note) | [sharc-plus](sharc-plus.md), [sharc-routine-diff](sharc-routine-diff.md) | partial |
| Known simulator mismatch | `DO (PC, 0xFF800055)` (SHARC+ loop form): the 21489 simulator does not honour it; the model keeps the ADI listing semantics | [sharc-routine-diff](sharc-routine-diff.md) | VERIFIED on 21571 for `0x122620` |
| Peripherals | SEC0, SPI2, DMA, TRU, LP1, L2 under `--sc57x`; SPORT/DAI audio DMA and the `0x28xxxxxx` core aliases not wired; simulator probes P2..P8 unmeasurable ("Unimplemented MMR", PR #142) | ROADMAP lines 219, 220; PR #142 | open |
| Speed | long passes and the 21571 census need a faster step loop before CI use | ROADMAP line 221 | open |
| Unit tests | 353 `#[test]` functions in `crates/nord-sharc/src` | `grep -c "#\[test\]"` | counted |

Effort to close the 18 census cases: low. Effort to run the 21571 census: medium (machine time, no new code). Audible impact: none today (no firmware routine in use is known to hit them); it only widens the trust base.

### 3.2 Which images, cores and firmware paths run

| Image / core | Chip (D17) | What runs in the oracle | Not run or not equal | Tag |
|---|---|---|---|---|
| PNO core 1 | 2157x | oscillator bank `0x122620`, mix `0x12268F` (21571 bit-exact, 40 of 40), organ post `0x12272D`, output `0x1229B3` (session note: output 0 of 20 differ, post DM 20 of 20 identical, R14/R15 differ in 6), Farfisa generator `0x122A71` and processor `0x122B85`, 18-module chain in `sharc-pass` | B3 processor `0x121E2B` and B3 Bass `0x122296` not bit-exact (OQ18); `0x125371` / `0x1235AA` blocked (D98) | VERIFIED as listed |
| PNO core 2 / ORG core 2 | 2157x | synth strips and 312 of 312 ANALOG stages, 442 of 442 filter stages, WAVE stage `0x1247B6`, FM `0x124269` / `0x12453F` word for word, velocity pass (768 of 768 words within 1 ulp, D104) | SHAPE word law verified at the idle value only; short-word loads zero-extended (ASSUMED) | VERIFIED (oracle runs) |
| ORG core 1 | 2157x | strips, SPI2 peek port | key-down link (UART2 ring) not emulated | partial |
| FX | 2157x | compressor, delay, reverb, Mod 1, Mod 2, amp, Leslie `0x1256AD` routines run one at a time (`sharc-fx`) | no whole-chain run with the ARM module install: ENS tap registers (OQ21a, D109, D114) | partial |
| SY1SY2 | 2148x | boot kernel to its final `IDLE` (171,826 steps); voice start (FIFO reset vector 13, prepare, go); resampler `0x1C10EE` 0 error on 240 of 240 frames (real accordion sample) | external-port DMA from SDRAM only as an ideal stream model (D112); unpacker handlers other than 9 and 15 bit untested on real data; vectors 1 and 12 wait; loop-end call at `0x1C064B` replaced by the loop start (core left unchanged, hardware behaviour unmeasured); opcode handlers 1 to 15 run with set-up registers, not with real DMA state | VERIFIED for resampler |
| SMP | 2148x | audio interrupt, FIFO, voice loop `0x1C07D8` plays real `CNSP` pages bit-exact against the Rust model; record dispatcher `0x1C058C` table read from the image | the ring that feeds the SMP serial transmit buffer and its receiver | VERIFIED for the voice |

### 3.3 What the SY harness does not model (the sample pitch entry hunt)

| Gap | Evidence | Decision |
|---|---|---|
| External-port DMA that refills the 128-frame source ring from SDRAM | ring pre-filled; refill modelled as ideal stream in `sharc-ref stream` | D111, D112 |
| ARM ring consumer `0xC01CE7D8` | run twice: returns 1, sends 393 port messages, 0 to SY / SMP | [sy-pitch-entry-writer](sy-pitch-entry-writer.md) |
| Loop-end call `0x1C064B` | `finish_step` replaces the call target by the loop start; the firmware pattern is outside the documented loop rules | D111 follow-up (PR #435) |
| Vectors 1 and 12 | vector 1 is reset code with no `RTI`; vector 12 is a one-instruction `RTI` stub the harness misses | D112 |
| Opcode 9 (SPI DMA) with a flash model | not run | open |
| ARM state after the DSP answers "prepare" | status bit set by hand (`DspAck`, meaning ASSUMED "SY voice ready") | [synth-sample-kernel-rule](synth-sample-kernel-rule.md) s12 |
| Result | The writer of the pitch entry `e` at `DM(0x9354A + index)` is **not found** after D89, D105, D111, D112; `--sample-pitch fw` stays opt-in | D65, OQ22 |

### 3.4 ARM harness (`tools/arm_*.py`, Unicorn ARM926EJ-S)

| Item | State | Evidence | Tag |
|---|---|---|---|
| Harness | 50 `tools/arm_*.py` scripts; unit tests with hand-assembled ARM words (no firmware needed) | [arm-fw-emulation](arm-fw-emulation.md) | VERIFIED (tests) |
| Static initialisers, edit registry | run unmodified; 422 edit parameters registered with name, id, domain, range, default | [arm-organ-messages](arm-organ-messages.md) s1 | VERIFIED |
| Boot | reaches the cooperative main loop at `0xC00A0248` after 433 M instructions (36 s); 3.0 B instructions in 201 s (the "about 4 min" of the brief is this full run plus capture); cold-boot capture about 80 s; settled snapshot restores in 2 s | [arm-organ-messages](arm-organ-messages.md) s10.1, [arm-bulk-tables](arm-bulk-tables.md) | VERIFIED (run) |
| Message capture | 80,815 boot messages from the real EMIFA ports, fingerprinted and replayed into `sharc-pass` / `sharc-org` | ROADMAP line 231 | VERIFIED |
| Timer | the 1 kHz timer (irq 22) is injected by `run_ticking` every 200,000 instructions (about 200 MIPS ASSUMED); tick rate 1 kHz versus 24 kHz open (OQ22) | arm-organ-messages s10.1 | ASSUMED |
| Stubs | `DspAck` (voice-ready bit), SPI transfer `0xC0066E64`, flow control `0xC006CA48`, allocator, poll breaker, UI stubs, null-call guard; the harness has **no DSP**, so every DSP answer is a stub | arm-fw-emulation s2, arm-organ-messages s5 | FAKE by design |
| Not run | the real keybed link (UART2 ring), timer tasks beyond the tick, ring consumer behind real DMA, the "go" state after the DSP answers, per-key velocity source | ROADMAP line 230, OQ19, OQ23 | open |
| Used for | layer-level law (D87), Touch curves (D88, D99), Mod 2 laws (D90), reverb colour words (D91), FM generator (D107), FX defaults (D101), velocity (D104) | decisions | VERIFIED |

Effort to close Part A gaps: the pitch entry writer is **high** (four searches without a hit; needs a DSP model for the ARM to talk to); SPI DMA with a flash model is **medium**; the 21571 census is **medium** (machine time). Audible impact of the pitch entry: **medium to high** for the 119 programs with SAMPLE layers (the engine plays equal temperament steps by default; the verified law is opt-in).

## 4. Part B: the audio emulator per subsystem

Status columns: **Fw** = verified against firmware or oracle, **Clip** = fitted to hardware clips, **Asm** = assumed, **None** = unmodelled. Program counts from [program-coverage](program-coverage.md) (2026-11-14; classification not regenerated for D87 onwards, section 5 lists the stale rows).

### 4.1 Organ (44 of 384 programs have an organ layer: 12 organ, 6 organ + piano, 10 organ + synth, 16 organ + piano + synth; sum of the engine mix table)

| Subsystem | Programs | Status | Default or opt-in (D22) | Evidence | Open gap | Effort | Audible impact |
|---|---|---|---|---|---|---|---|
| B3 | 27 (36 layers) | Asm (tone source not located; scanner vibrato ASSUMED) with Clip checks | default = legacy float wheels; `--tone-source b3wheels` and `--b3-tail` opt-in | Part A clip `organ_b3`; hold-out chord counts 10 / 8 / 6 of 13 / 13 / 11 lines within 1 dB; clip mid band -11.9 dB | B3 processor `0x121E2B` (OQ18), dry gain `DM 0xB44C0` (OQ16), 12 dB dark at 0.4 to 1.5 kHz | high | high |
| Vox | 7 (8) | Fw bank (VERIFIED) but opt-in `--tone-source vox` | opt-in | clip `organ_vox`; `K` 1.7 to 1.8 (D106) | key flag and taper writers (OQ17), 1.5 to 8 kHz deficit, level stage | medium | medium |
| Pipe 1 / Pipe 2 | 3 + 4 | Fw: 92-wheel bank VERIFIED; its role for the pipes ASSUMED | default | clip `organ_pipe`; `K` 3.6 to 6.2 (D106) | Pipe weight scale (Legacy +14.4 dB over the ARM unit), wheel steps | medium | medium |
| Farfisa | 2 (3) | Fw processor `0x122B85` and generator `0x122A71` bit-exact; tone source Asm | opt-in `--farfisa-processor`; **offline only** (about 2 s CPU per s, D74, TD-15) | oracle bit-exact | real time path, Farfisa state | high | low (2 programs) |
| B3 Bass | 4 (4) | Fw generator decoded; processor `0x122296` not bit-exact | opt-in `--b3-bass-processor`; **offline only** (3.3 to 3.6 s per s, D71, TD-15) | chord sounds in `sharc-pass` | drawbar and percussion dependent key tables, key-up resync, hardware clip | high | low (4 programs) |
| Percussion | 6 (8) | Asm | default | none | second engine family `0xC00BF890`, `0xC00BE348` (OQ19) | high | low |
| Vibrato / chorus | 21 (25) | Asm (B3 tail opt-in) | default | [b3-vibrato-tail](b3-vibrato-tail.md) | enable and type words change the oracle | medium | low |
| Leslie / rotary | 24 (35) | Asm engine rotary; Fw Leslie `0x1256AD` ported | firmware Leslie opt-in `--leslie firmware`; drive `--leslie-drive program` opt-in (D98) | oracle; presence +1.5 to +2.8 dB (inside 3 dB) with 96 sustained keys (D94) | history ring DMA (`0xB0078`), mid +3 and brilliance +5 dB | medium | medium |
| Organ level `K = 2.8` | all organ | **EMPIRICAL** | default | no lane word in image or ARM code (D85); per engine K measured (D106); one `K` stands under band, gate and matched-range controls, the spread is the replay stimulus (D113) | the firmware gain (OQ24) | medium | high (about 8.9 dB organ against piano) |
| Drawbars | 44 (60) | Asm (scaling per model) | default | | dB curve not recovered | medium | low |

### 4.2 Piano

| Subsystem | Programs | Status | Default or opt-in | Evidence | Open gap | Effort | Audible impact |
|---|---|---|---|---|---|---|---|
| Classes (`.npno` sample path) | 272 (311 layers) load samples | Fw: stream decode and level law (D23) | default | `CNSP` voice bit-exact against `smp_voice.rs` | classes 1, 2, 3, 5, 6 start an extra voice the engine does not play: 250 programs (276 layers) | medium | medium |
| Categories | Grand 153, Electric 59, Upright 50, Digital 14, Clav 12, Misc 8 programs | | | | Clav EQ unmodelled (OQ31) | low | low |
| SMP slew / chain | all piano | Fw (voice-gain slew rule D103, rate word D110); chain 0.7464 per `y / 8192` VERIFIED | **opt-in** (`--piano-slew-fw`, `--firmware-piano-chain`) | oracle chain equals emulator (D110); Felt Upright score 2.18 to 1.31, Velvet 3.25 to 3.74 (D103) | pedal-down bypass, open bus scale (OQ20) | medium | medium (attack 47 to 70 ms against 56 to 64 ms on hardware) |
| Decay / T60 | all | gap was a replay artefact (D86) | | [piano-decay-attack](piano-decay-attack.md) | none known | | |
| Resonance (string res.) | 240 (258) | Asm (modal model) | default | | no hardware clip | medium | unknown |
| Pedal noise, unison, dyn comp | unknown | Asm | default | | | | unknown |
| Level | all | Fw level law default; 25 dB piano / organ spread under the common path open | default | White Grand, DSoundman, Audible Genius clips | absolute chain (ROADMAP line 126) | medium | high |

### 4.3 Synth

| Subsystem | Programs | Status | Default or opt-in | Evidence | Open gap | Effort | Audible impact |
|---|---|---|---|---|---|---|---|
| ANALOG stage families | 193 programs, 312 layers | **Fw: 312 of 312 layers have a bit-exact stage model** (`synth-stage-models-12` s5; the batches list which stage covers which wave; this note does not re-derive the per-wave split); levels per wave EMPIRICAL (saw 0.4, pulse 0.27, SHAPE 0.6, MULTI 0.26, SUPER 2.0) | stage chain default since D59 (`synth_chain` FirmwareStrip, `synth_stages` true); P table EMPIRICAL | 15 clips: mean score 6.6 to 4.1; 9 better, 3 equal, 3 worse | SHAPE word law at idle only; P rows for waves without a clip: unknown count | medium | medium |
| Filters per type | 132 Lp12, 88 Lp24, 52 Ladder24, 18 BandPass, 11 HighPass (programs) | stages Fw (442 of 442 filter layers bit-exact); type map and per-type cutoff factors **EMPIRICAL** (D108: factors are not stage physics; derived replacement scores worse, 3.88 / 3.20 against 2.18 on 12 clips) | default ZDF filter (`filter_engine` None); firmware filter opt-in | cutoff law of a voice VERIFIED, exponential (D102) | stage call rate per frame, resonance word, `kFltType` link; LP24 SUPER factor 4.4 does not hold on three more clips (D116); opt-in `--synth-super-lp24` | medium | medium |
| Envelopes | 289 (581) | Fw records bit-exact; panel mapping Asm | default (knob laws on since D57) | | pass = 2 SY frames ASSUMED; ARM tick 1 or 24 kHz (OQ22) | medium | medium (envelope times) |
| LFO | 38 filter, 50 OscCtrl, 33 pitch (programs) | pitch-depth table VERIFIED; rate and amount scales Asm (OQ7) | default | | | medium | low |
| Arpeggiator | 32 (49) | Asm (pattern table not located) | default | [synth-arp-patterns](synth-arp-patterns.md) | gate 2/3 versus 1/2 step (OQ32); clock 24 kHz (OQ22) | low | low to medium |
| FM | 25 (38) | Fw tables, generator and stages decoded and ported (D92, D96, D107) | **opt-in** `FmVoice::select` | oracle, 30 table variants bit exact | no hardware clip: level unknown (FM level, `x`, layer hookup) | medium | medium (25 programs) |
| Wavetable | 12 (14) | Fw stage `0x1247B6` and 46 tables decoded; engine plays a 64-table placeholder | **opt-in** `--synth-wave-bank` (needs the user's own OS image) | 320 of 320 oracle words | default flip needs the table source decision | low | medium (12 programs; program-coverage still counts it unmodelled) |
| SAMPLE | 119 (215) | Fw 12-tap sinc kernels, key rule (D46) VERIFIED, resampler 0 error | kernel by key default; `--sample-pitch fw` opt-in | oracle 240 of 240 bit-exact | pitch entry writer not found (D89, D105, D111, D112); sample map zone pitch key ASSUMED | high | medium |
| Unison | 51 / 47 / 72 programs at 1 / 2 / 3 | Asm (SUPER voice spread EMPIRICAL) | default | Super Saw level flat within 1.2 dB over Detune 0 to 10 (hardware flat) | | medium | low |
| Layer level law | all | Fw `(v / 127)^2` (D87) | **default** | handler `0xC011BB48` run live; Leaky Stabs II 3.19 to 1.95 | Reverb GLOBAL, DSoundman taper | | |
| Sustain pedal (SUSTPED) | 381 of 384 set | Fw bit decoded (D97) | opt-in `--synth-sustain-pedal` | | ARM hold logic | low | low |
| Velocity | 213 of 287 synth programs sensitive | Fw laws (D104) | **opt-in** `--firmware-synth-velocity` | 768 of 768 words within 1 ulp | file bits of the three velocity fields, `kOscEnvVel`, a clip | medium | medium |
| Not modelled | MISC 8, MULTI 43, SHAPE SINE 30, SYNC 16, SUB OSC 1 programs per program-coverage | stale: stage models exist (section 5) | | | regenerate the classification | low | none (bookkeeping) |

### 4.4 Effects

| Subsystem | Programs | Status | Default or opt-in | Evidence | Open gap | Effort | Audible impact |
|---|---|---|---|---|---|---|---|
| Amp | 261 (480) | Fw drive stage and cabinet (words VERIFIED, voicing ASSUMED); HP24, LP24 Asm; To Rotary pass-through (2, unmodelled) | **default** firmware amp for Twin, JC, Small | Vox score 3.98 to 1.30; clip `fx_amp` | JC and Small no clip | low | low |
| Compressor | 117 (204) | Fw routine, parameters and bus position Asm | opt-in (`--firmware-fx`) | | | medium | low |
| EQ | 265 (503) | Asm (RBJ biquads, gain step and Q) | default | FREQ2 `20 * 1000^(v/1e6)` Hz EMPIRICAL (6 %) | | low | low |
| Mod 1 | 90 (124) | laws captured at 44 values (D100); engine models, constants Asm; PUMP is a verified module (`0x122D2C`, [fx-mod-types](fx-mod-types.md) s7) but its clock: hardware 2.11 Hz against 6.1 Hz free-run is reported by the coordinating session and **not recorded in a repo file** (unverified here) | default | | PUMP clock source, RM and A-WAH hardware clips | medium | medium (PUMP 9 programs, A-PAN 50) |
| Mod 2 | 158 (260): CHOR 93, VIBE 45, PHAS 12, SPIN 11, FLANG 10, ENS 3 | CHOR, VIBE, FLANG, PHAS 0 run the firmware routines behind `FirmwareFx::mod2`; SPIN and ENS wired through the law stage; ENS tap registers closed, no taps needed (D114) | opt-in | laws on a dense ARM grid (D90); chorus clip `fx-chorus-hw-clip` | only chorus has a clip; per-layer-kind words | medium | medium |
| Delay | 155 (252); feedback FX 68 | Fw time and mix words; loop is the engine's | opt-in `delay_laws` | | feedback FX approximated | medium | low |
| Reverb | 358 (756): BOOTH 195, ROOM 150, SPRING 57 | engine FDN default; per-type colour words decoded and ported (D91); firmware tails 0.5 to 1.35 s | opt-in `--firmware-reverb` | `fx_reverb` clip | hardware tail clip (Hall, Cathedral), type 6, Spring colour (OQ21c) | medium | high (all reverbs) |
| FX defaults | | words derived (D101) | opt-in `--fx-defaults firmware` | only chorus has a clip | init-patch clip (OQ34) | low | low |
| Rotary | 24 | see 3.1 Leslie | | | | | |

### 4.5 Routing, layers, velocity, MIDI and GUI, real time

| Subsystem | State | Default or opt-in | Evidence | Open gap | Effort | Impact |
|---|---|---|---|---|---|---|
| Layer routing, zones | Fw: zone code to key range map equals the ARM zone objects for 384 of 384; empty layer mask reads as layer A | **default** since D73; splits in 48 programs | [zone-key-ranges](zone-key-ranges.md), [engine-routing](engine-routing.md) | OQ33 (organ heard in piano programs): audit finds no routing cause | low | medium |
| Morph slots, split crossfade | Asm / partial | default | [morph-and-zones](../specs/morph-and-zones.md) | morph slots unmodelled | medium | low |
| Piano Touch curves | Fw curves (D88, D99) | **opt-in** `--firmware-piano-touch` | ARM run | default stays off: the program file bits are not verified | low | high (key force changes the piano a lot) |
| Mouse velocity | position along the key, 20 to 127, ASSUMED (D84) | default in the GUI | none | listening check | low | low |
| MIDI | note, CC, pedals, bend; queue `bounded(1024)` | default | [midi-and-control](../specs/midi-and-control.md) | | | |
| GUI | 6 panel bays partly drawn; ROADMAP section B items open | | [ROADMAP](../ROADMAP.md) | panel renderer B6 to B13 | high | none for audio |
| Real time | typical program 0.67 to 0.70 ms per 128-frame block (met); worst case 2.2 to 2.5 ms (TD-14); Bass, Farfisa offline (TD-15); opt-in ports (firmware Leslie, FM, Wavetable, SUPER 2..4, firmware reverb): per-stage cost `unknown` | | `ns4 bench` | benchmark each opt-in port before any default flip | medium | none |

## 5. Program coverage, as the repo holds it

| Configuration | verified | assumed | unmodelled | Source |
|---|---|---|---|---|
| default | 1,015 (15 %) | 5,677 (83 %) | 119 (2 %) | program-sweep.json `status_totals` |
| `--firmware-all` | 2,634 (39 %) | 4,058 (60 %) | 119 (2 %) | same |
| paths (rows) default / firmware-all | 5 / 15 | 39 / 29 | 7 / 7 | recomputed from `classification` (51 rows) |

Counts are **path uses**, use-weighted (a path used by two layers counts twice), over 51 classified paths. By family under default: FX 480 verified, 2,254 assumed, 2 unmodelled; organ 9 / 179 / 0; piano 311 / 276 / 0; synth 215 / 2,920 / 117; routing 0 / 48 / 0. Under firmware-all: FX 936 / 1,798 / 2; synth 1,330 / 1,805 / 117; routing 48 verified.

The 119 unmodelled uses: Wavetable 14 (12 programs), SHAPE SINE 32 (30), MULTI 44 (43), SYNC 18 (16), MISC 8 (8), SUB OSC 1 (1), To Rotary 2 (2). **Programs with at least one unmodelled path: `unknown`** (the sweep file stores per-path program counts, not per-program path sets; lower bound 43, upper bound 112 by sum).

**Stale rows (raise, do not pick):** program-coverage lists MULTI, SHAPE SINE, SYNC, MISC and SUB OSC as unmodelled, while `synth-stage-models-2` to `-12` report all 312 ANALOG layers with a bit-exact stage model and `render_opts.rs` sets `synth_stages: true`. Either the sweep classification predates the ports or those stages are not on the default path for these waves. Regenerating the sweep settles it (see Follow-ups). Likewise `sharc-routine-diff.md` still lists the post and output stages as "not yet compared (stall)" although PR #131 landed that comparison.

## 6. Open gaps ranked

| Rank | Gap | Effort | Audible impact | Needs hardware | Rows |
|---|---|---|---|---|---|
| 1 | Organ `K` (OQ24) and the absolute piano / organ level chain | medium | high | clip helps | [audio-remaining](audio-remaining.md) 3 |
| 2 | Reverb tails per type | medium | high (358 programs) | tail clip | 7 |
| 3 | Touch velocity curves default | low | high | listening | 4 |
| 4 | Piano extra voices (classes 1, 2, 3, 5, 6; 250 programs) | medium | medium | no | 11 |
| 5 | Sample pitch entry `e` | high | medium (119 programs) | no | 2 |
| 6 | B3 processor and Leslie presence | high | high (27 programs) | one clip | 9 |
| 7 | Mod 1 PUMP clock (2.11 Hz against 6.1 Hz) | medium | medium (9 programs) | clip | 6 |
| 8 | Synth filter type factors and envelope pass | medium | medium | clips | 12 |
| 9 | FM level and Wavetable default | medium | medium (37 programs) | FM clip | 10 |
| 10 | Bass and Farfisa real time | high | low | no | 15 |

## Implications

- The oracle is strong at the instruction and routine level and weak where the firmware talks to the outside (DMA, SPI, the ARM state after a DSP answer). Every remaining parity gap sits there or in a hardware-only quantity (levels, tails, clocks).
- 83 % of default-path uses are still assumed (60 % under `--firmware-all`). The audio is not done; the defaults move only under D22.
- Four ranked gaps (1, 2, 6, 7) cannot be closed from the firmware alone and need one hardware clip each.

## Follow-ups

1. Regenerate `program-sweep.json` on current `main` and store per-program path sets, so the count of programs with an unmodelled path is known and the stale rows of section 4 are corrected.
2. Correct `sharc-routine-diff.md` for PR #131 (post and output stages) and move the session-note numbers into a research file.
3. Run the instruction census on the 21571 simulator (about 4 h).
4. Benchmark the opt-in ports in real time before any default flip.
