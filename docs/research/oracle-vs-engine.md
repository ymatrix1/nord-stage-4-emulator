# Oracle versus Rust engine: the SY1SY2 sample resampler

Checked: 2026-11-12.

Sources: the SY1SY2 image of OS v1.66 (`bank0_CODE@0x4FD980`, flat SRAM image from `tools/ns4_dsp_raw.py --sram`, see [sharc-sy1sy2-smp.md](sharc-sy1sy2-smp.md)); the SHARC core model `crates/nord-sharc` (`sharc-ref`, new); `crates/nord-core/src/sinc.rs` (`SincKernelSet`, PR #55); [sy-dsp-map.md](sy-dsp-map.md) sections 2 and 5; [sharc-float.md](sharc-float.md) (rounding rules measured on the ADI simulator); [dsp-rx-protocol.md](dsp-rx-protocol.md); [sharc-sequencer.md](sharc-sequencer.md).

Question: does the Rust engine reproduce what the firmware's Stage B resampler (`0x1C10EE`) computes, sample by sample, and where exactly do the two differ?

## 1. Result

| | Finding | Status |
|---|---|---|
| Kernel table | The 18,432 table values read through Stage B equal `SincKernelSet` within **1.19e-7** (2 ulp near 1.0); 4,153 of 18,432 are bit-identical. With the decimal cutoff in f64 the same table is within **5.95e-8** (the figure of `sy-dsp-map.md`). The extra 6e-8 is `kernel_value(fc: f32, ..)` widening `1.1f32` (= 1.10000002384) to f64 | VERIFIED (section 4) |
| Arithmetic | With the oracle's own table, an engine-side emulation of the DSP's 40-bit extended arithmetic reproduces **every output bit** of three renders (64, 240 and 360 stereo frames, both channels): max error **0**, SNR infinite. Plain sequential f32 is 1 to 232 ulp off (max abs 6e-8) | VERIFIED against the oracle; the oracle's arithmetic is VERIFIED against the ADI simulator ([sharc-float.md](sharc-float.md)); no hardware recording exists |
| Phase rule | The hardware uses the 8 phase bits (bits 24..17 of the position word) and **ignores the bits below**: floor, no interpolation between phases. `SincKernel::interpolate` rounds to the nearest phase. On a position with a fraction of 76.8 phases this costs **3.4e-3** abs error (SNR 41 to 45 dB) | VERIFIED (section 4) |
| Delay | 0.000 samples in every comparison once the position model of section 3 is used | VERIFIED |
| Engine path as called today | `kernel.interpolate(pcm, pos)` with an exactly representable phase: max abs error 5.96e-8 (one ulp at 0.5) against the oracle | VERIFIED (`tests/oracle_stage_b.rs`) |

## 2. Method

```text
scenario file --sharc-ref--> oracle dump (CSV + WAV, metadata header)
                                   |                  \
                                   |                   +--> sinc_ref (nord-core example) --> reference CSV
                                   +---- ns4_oracle_compare.py <---------------------------------+
```

| Piece | File | Role |
|---|---|---|
| Harness | `crates/nord-sharc/src/bin/sharc_ref.rs` + `sharc_ref/{script,signal,voice,exec,out}.rs` | Loads a flat SRAM image, runs a scenario on the core model, writes `PREFIX.csv` (`# key: value` metadata, one column per captured series, shortest round-trip float32 text) and with `--wav` a 32-bit float WAV |
| Scenarios | `tools/oracle_scenarios/*.scn` | Plain text, section 7 |
| Reference renderer | `crates/nord-core/examples/sinc_ref.rs` | Reads an oracle dump (metadata `ref.*` and the captured source ring), renders the same frames with `SincKernelSet`, prints a CSV. Options: `--accum f32|f64|ext40|ext40-near|ext40-trunc`, `--round floor|nearest`, `--kernel-from KERNEL_DUMP` (use the oracle's table to isolate arithmetic) |
| Comparator | `tools/ns4_oracle_compare.py` | Per series: max abs error and index, RMS, SNR, max ulp, bit-exact count, delay (cross-correlation with parabolic refinement), text plot and error profile. Reads CSV and WAV (PCM 16/24/32, float32). `--tol-abs`, `--tol-snr` give an exit status for scripts |
| Regression test | `crates/nord-core/tests/oracle_stage_b.rs` (+ `tests/support/`) | Embeds the 64-frame oracle answer and 24 weights as float32 bit patterns (derived results, no firmware code); asserts the bit-exact arithmetic and the engine tolerances |

Entry used for Stage B: `run 0x1C0758`, the Stage B loop of the audio ISR (`DM(0x93590)` frame pointer load, then 56 calls through the pointer at record word `+0x15`, then `RTI`; listing `0x380EB0..0x380F26`). It is entered like `sharc-run --enter` (`Core::enter_handler`) after the normal start-up (`warmup 0x1C0B2D 100000 flag12=1`, idle loop reached), so all registers the loop needs (`M2 = 2`, `M1 = 1`, ...) come from the firmware's own initialisation. Voices whose pointer is still `0x1C10ED` (an `RTS`) cost nothing, so one run is one Stage B call for the scripted voice. About 580 core steps per run.

## 3. What Stage B does, as measured

Found by poking each word of voice record 0 (`0x00..0x3B`, four values each) and by impulse responses (a unit impulse in the ring, one call, read the output). Word offsets are relative to the record base `0x92400 + 0x38 k`; the routine addresses them as `DM(q, I3)` with `I3 = base + 0x26`, so `q = offset - 0x26`.

| Offset | q | Role | Status |
|---|---|---|---|
| `0x08`, `0x09` | -30 | Gain applied to the 12-tap sum, left and right (PEx / PEy) | VERIFIED (output scales; `gain` 2.0 doubles it) |
| `0x15` | -17 | Stage B pointer; must be `0x1C10EE` | VERIFIED (sy-dsp-map.md) |
| `0x2B` | 5 | Position word `pos`: bits 31..25 frame index (ring wraps modulo the ring length), bits 24..17 phase index, bits 16..0 ignored | VERIFIED (impulse at frame `j`, `pos = 0` returns `h[j][0]`; `pos = 0x01000000` returns the half-frame value of a ramp) |
| `0x2D` | 7 | `step`: after the output `pos += 2 * step` (the routine adds `step` twice); `step = ratio * 2^24` gives `ratio` source frames per output frame | VERIFIED (`pos` 0 to `0x02000000` to `0x04000000` with step `0x01000000`) |
| `0x31` | 11 | Ring base (`B0`, `I2`); circular length `L0 = 0x100` words = 128 stereo frames, left at even words, right at odd words | VERIFIED |
| `0x33` | 13 | `M4`: word offset of the output frame accumulator | VERIFIED (a non-zero value moves the output; 0 is used) |
| `0x3B` | 21 | Kernel base pointer `0xE3400 + 0xC00 k` | VERIFIED (the six kernels are selected by this word alone) |
| frame `+0x28`, `+0x29` | | The second 12-tap pass at the other position lands here as bit patterns shifted into mask words (values around 1e19 or 1e-20 as floats). They are not audio | EMPIRICAL (nonzero, not decoded; role of the second position stays open, as in sy-dsp-map.md section 6) |

Model of one call, with the position word `p` read before the update:

```text
idx   = p >> 25                       ring frame of tap 0
phase = (p >> 17) & 0xFF
out[M4]     += gain_l * sum_{t=0..11} h[kernel][phase][t] * ring_l[(idx + t) mod 128]
out[M4 + 1] += gain_r * sum_{t=0..11} h[kernel][phase][t] * ring_r[(idx + t) mod 128]
p += 2 * step
```

The output word is read from the frame, the new value added and stored back (`F1 = F0 + F1`), so the frame is a mix bus; the harness clears it before each call. The centre tap is tap 5, so the signal position being reproduced is `idx + 5 + phase / 256`. A 120-frame block therefore takes 120 calls (this matches what the DSP does per output frame; why the surrounding ISR clears 120 frames once is not needed here).

## 4. Results

Run on the SY1SY2 image of OS v1.66. Errors are oracle minus reference, float32 values.

**Kernel table** (`sy_stage_b_kernel.scn`: 18,432 calls, impulse at each tap, each phase, each kernel; gain 1):

| Engine | n | max abs | at | rms | SNR | bit-exact |
|---|---|---|---|---|---|---|
| `SincKernelSet` | 18,432 | 1.192e-7 | index 4434 (kernel 1, phase 113, tap 6) | 1.74e-8 | 144.1 dB | 4,153 |
| same model, f64 with the decimal `fc` (1.1, 1.05, ...) | 18,432 | 5.95e-8 | | | | |

**Resampling** (kernel row: where the weights come from; arithmetic row: how the 12 products are summed). Values are max abs error left/right and SNR left/right.

| Case | Kernel | Arithmetic | max abs L / R | SNR L / R | bit-exact |
|---|---|---|---|---|---|
| golden, 64 frames, ratio 0.5, kernel 2 | engine | f32 sequential | 5.96e-8 / 8.94e-8 | 138.9 / 138.0 dB | no |
| | engine | 40-bit | 5.96e-8 / 5.96e-8 | 142.0 / 141.9 dB | no |
| | oracle table | f32 sequential | 5.96e-8 / 5.96e-8 | 143.1 / 141.2 dB | no |
| | oracle table | **40-bit** | **0 / 0** | inf | **yes** |
| resample, 240 frames, ratio 0.7312, pos 0.3, kernel 2 | engine | f32 sequential | 8.94e-8 / 8.94e-8 | 139.6 / 139.8 dB | no |
| | engine | 40-bit | 5.96e-8 / 2.98e-8 | 143.8 / 144.8 dB | no |
| | oracle table | f32 sequential | 5.96e-8 / 5.96e-8 | 142.3 / 142.5 dB | no |
| | oracle table | **40-bit** | **0 / 0** | inf | **yes** |
| sine, 360 frames, ratio 1.5, kernel 0, gain 0.5 | engine | f32 sequential | 5.96e-8 / 5.96e-8 | 143.0 / 141.2 dB | no |
| | engine | 40-bit | 2.98e-8 / 1.49e-8 | 149.2 / 155.3 dB | no |
| | oracle table | f32 sequential | 5.96e-8 / 5.96e-8 | 142.3 / 140.6 dB | no |
| | oracle table | **40-bit** | **0 / 0** | inf | **yes** |
| resample, phase rounded to nearest (`--round nearest`) | engine | f32 sequential | 3.56e-3 / 3.42e-3 | 44.3 / 44.9 dB | no |

Delay estimate: 0.000 samples in all rows. Reading:

* The engine is a 1 ulp (5.96e-8 at 0.5) approximation of the firmware. All visible audio differences are far below 24-bit resolution (1.2e-7 at full scale); nothing here is audible.
* Two things make up the difference: the **kernel table** (the engine builds it from an f32 cutoff widened to f64; the firmware table is the decimal-cutoff formula, plus the table's own rounding) and **summation precision** (the DSP rounds to 32 significant bits after every multiply and add, then the DM store truncates to 24; the engine's f32 accumulation rounds to 24 each step).
* The 40-bit rules below, with the oracle's table, reproduce every output bit: products and sums rounded to nearest even at 32 significant bits (1 + 31 fraction bits), sum started from zero with tap 0 first, gain multiplies the finished sum, the store drops the low 8 mantissa bits. `--accum ext40-near` (rounded store) and `--accum ext40-trunc` (truncated ops) each leave 1 ulp errors, so the rule set is the only one of the three that matches.

## 5. Differences found, and what to do about them

| # | Difference | Where | Size | Action |
|---|---|---|---|---|
| 1 | Phase: hardware floors to the 8-bit phase, `SincKernel::interpolate` rounds to nearest (ASSUMED in its doc comment) | `sinc.rs` `interpolate` | up to half a phase = 1/512 sample; 3.4e-3 abs on a 0.3-sample start (SNR 41 dB) | The ASSUMED can now be replaced by VERIFIED floor. Tracked in TD-08; not changed here (file owned by another lane). The test `known_difference_engine_rounds_the_phase_where_the_oracle_floors` pins the current behaviour and must be inverted with the fix |
| 2 | `kernel_value(fc: f32, ..)` computes `sinc(f64::from(fc) * d)` with `1.1f32` | `sinc.rs` | 1.19e-7 instead of 5.95e-8 against the table | Keep `CUTOFFS` as f64 literals (or compute in f64) |
| 3 | Unity DC gain: the engine multiplies the sum by `fc`; the firmware leaves the sum at `1 / fc` and applies the voice gain (word `+0x08`) afterwards | `sinc.rs` | gain choice, not an error | Voice code must apply `gain = fc * g` or equivalent |
| 4 | Accumulation precision (40-bit versus f32 sequential) | engine voice code | up to 1 ulp per output sample (5.96e-8 abs) | Not worth emulating for sound; a bit-exact build can use `tests/support/ext40.rs` |
| 5 | Ring: 128 stereo frames, circular; the engine's `interpolate` clamps at the edges | voice code | design | Keep a circular ring when `SAMPLES` is wired in |

## 6. Reproduce

```bash
python tools/ns4_dsp_images.py target/fw_unpacked/bank0_CODE.bin --out DIR
python tools/ns4_dsp_raw.py DIR/dsp_image_3_SY1SY2.bin --sram sy.bin
cargo build --release -p nord-sharc -p nord-core --example sinc_ref
mkdir -p target/oracle
for n in kernel golden resample sine; do
  target/release/sharc-ref sy.bin tools/oracle_scenarios/sy_stage_b_$n.scn --out target/oracle/$n --wav
done
target/release/examples/sinc_ref target/oracle/kernel.csv > target/oracle/kernel.ref.csv
python tools/ns4_oracle_compare.py target/oracle/kernel.csv target/oracle/kernel.ref.csv
for n in golden resample sine; do
  target/release/examples/sinc_ref target/oracle/$n.csv --accum ext40 --kernel-from target/oracle/kernel.csv > target/oracle/$n.ref.csv
  python tools/ns4_oracle_compare.py target/oracle/$n.csv target/oracle/$n.ref.csv --tol-abs 0
done
cargo test -p nord-core --test oracle_stage_b
```

The kernel scenario (18,432 calls) runs about 10.7 million core steps in half a second; the others are much shorter. Without the firmware only the embedded test and the unit tests run.

## 7. Scenario file format (for organ, piano and FX modules)

One command per line; `#` starts a comment. Numbers are decimal or `0x` hex; a data word may also be `f1.5` (float bit pattern). `ADDR` is a sum of numbers and `rec(K,OFF)` (word `OFF` of SY voice record `K`). The harness knows nothing about the SY module except the `voice` command and `rec(...)`; a new module adds a `voice`-like field table (`sharc_ref/voice.rs`) and scenarios, not new machinery.

| Command | Effect |
|---|---|
| `meta KEY VALUE...` | Copies a line into the dump header; `rate` sets the WAV rate; `ref.*` keys carry the parameters the reference renderer needs |
| `warmup ENTRY STEPS [flagN=1]...` | Sets the pc and runs the start-up to the idle loop |
| `poke ADDR VALUE` | One DM word |
| `fill ADDR COUNT KIND [stride=] [n0=] [key=val]...` | Test signal into `COUNT` words, `stride` words apart. Kinds: `sine` (freq, amp, phase), `sweep` (f0, f1, amp), `ramp` (start, step), `impulse` (at, amp), `dc`, `noise` (seed), `hash` (mul, add; exact in f32), `zero`. `n0` starts the signal at that index (for refilling a ring in a loop) |
| `voice K field=value...` | Fields of an SY voice record: `kernel=0..5`, `gain`, `ratio`, `pos_frames`, `stage_b=on`, raw `pos`, `step`, `buf`, `m4`, `gain_l`, `gain_r`, `kernel_ptr`, and `wN` for any word |
| `set REG VALUE` | `R`, `I`, `M`, `L`, `B` register |
| `run ENTRY MAX_STEPS` | Enters a handler body and runs until it returns (`RTI`/`RTS` to the caller) |
| `capture NAME ADDR COUNT [stride=]` | Appends DM words, as float32, to series `NAME`; one CSV column per name |
| `loop N [var=i]` ... `end` | Repeats the body; `{expr}` is replaced by an integer expression of the loop variable (`+`, `*`, `/`, `%`, left to right, no parentheses). No nesting |

To add a module (ORG tonewheel, PNO string, FX reverb): write the scenario with the module's entry (for ORG `sharc-org` already finds pass entries), `capture` the output buffer, add an `ref.mode` to the reference renderer of that module's Rust engine, and compare with the same tool. The comparator needs no change: it matches series by name.

## 8. Assumptions and their status

| Assumption | Status | Evidence |
|---|---|---|
| Stage B record layout and position model (section 3) | VERIFIED on the core model | impulse and poke scans, `tests/oracle_stage_b.rs` position test |
| Kernel table equals the Kaiser(8.5) sinc model | VERIFIED to 5.95e-8 (decimal `fc`) | this document, section 4; also `tools/ns4_sinc_check.py` |
| DSP arithmetic is 40-bit extended, nearest even, store truncates | VERIFIED on the oracle (bit-exact); the oracle's rules VERIFIED on the ADI simulator ([sharc-float.md](sharc-float.md)) | section 4; **not** verified on hardware |
| Firmware runs in reset MODE1 (`TRUNC = 0`, `RND32 = 0`) | VERIFIED by census (no static write), sharc-float.md | |
| Floor phase, no phase interpolation | VERIFIED on the oracle | section 4, `--round nearest` row |
| Which kernel the ARM picks for a given pitch | ASSUMED (`SincKernelSet::select`) | open; not exercised here |
| Role of the second 12-tap pass (mask words at frame `+0x28`) | open | section 3 |
| The ISR also runs Stage A and the per-voice slewing math before Stage B | not modelled here | the harness enters after them; gain, step and position are scripted |

## Implications

* The resampler core of the synth path can be replaced by `SincKernelSet` with 1 ulp fidelity once the phase rule is changed to floor and the cutoff precision is fixed (TD-08). A bit-exact mode is possible with the 40-bit rules.
* The harness gives every later DSP lane the same loop: scenario, oracle dump, Rust reference render, comparator.
* Section 5 items 1 and 2 are small, test-backed changes to `sinc.rs` for the TD-08 lane.

## Follow-ups

1. Apply section 5 items 1 and 2 in `sinc.rs` and invert the pinned test.
2. Decode the second FIR pass (mask words) with an input that makes it observable (two-position scenario).
3. Add Stage A (voice start, rate and gain slewing: `0x1C07B0`, ISR step 3) to a scenario so gain and step come from firmware, not from the script.
4. ORG tonewheel and FX scenarios on the same format, starting from the `sharc-org` pass entries.
