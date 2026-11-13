# FX delay time law: `kDlyRateClkDiv` to delay samples

- **Checked:** 2026-11-13
- **Sources (local, never committed):** `target/fw_unpacked/bank0_CODE.bin` and `bank0_SRAM.bin` (OS v1.66, ARM base `0xC0000000`), the `unicorn` ARM harness settled snapshot ([fx-contexts](fx-contexts.md) s2, `tools/arm_edit_capture.py`, `tools/fx_clients.py`), `capstone` listings of the handler, the FX Core 2 flat SRAM image (`tools/ns4_dsp_ldr.py --sram`), ADI `elfdump` listing (`tools/adi_dis.py 0x240000 0x250000`), the `sharc-fx` oracle. Derived data committed: [data/fx-delay-time.csv](data/fx-delay-time.csv) (264 captured `(value, word)` pairs) and the 1501-entry threshold list in `crates/nord-core/src/fx/fw_delay_table.rs`; no firmware bytes beyond that numeric list.
- **Question:** what does `Edt_Effects_kDlyRateClkDiv` (0..16383) do to the delay, i.e. what is the closed form from the UI value to the delay in samples, and what do the delay routine variants `0x1206B0` / `0x1208AF` / `0x120AB5` / `0x120D15` do with the three words `c[0x15]`, `c[0x16]`, `c[0x19]` the ARM writes?

Tags follow `.agents/rules/provenance-in-code.md`: **VERIFIED** = reproduced by a command below, **EMPIRICAL** = fitted or position only, **ASSUMED** = reading of a purpose.

## 1. Short answer

| Question | Result | Tag |
|---|---|---|
| UI value to time word | `c[0x15] = min(61438, floor(43.402339 * k))`, `k` = fractional index of `v` in a descending 1501-entry `i16` list (ARM `0xC02B8C44`); `v >= 16256` gives `k = 20` (868 samples, 18.1 ms), `v <= 0` gives `k = 1500` (clamped to 61438 samples, 1.280 s) | VERIFIED: 264 values, 0 differ (`tools/fx_delay_time.py check`) |
| Constant `K = 43.402339` | `floor(K k)` is consistent with every word only for `K` in `[43.402338, 43.402340]`; the firmware literal is not decoded (soft-float calls) | VERIFIED interval, closed form of `K` open |
| Table | irregular steps at musical index values (20, 30, 50, 75, 125, 150, ...): no closed form, kept as a derived value list; fingerprint sha-256 `797dcb4b...81d912` | VERIFIED (read from the handler's literal pool) |
| Which routine reads the time | only the ring buffer variant `0x120AB5` reads `c[0x15]`, `c[0x16]`, `c[0x17]`, `c[0x19]`; `0x1206B0` and `0x1208AF` use the offset words `c[0x10..0x13]` into the pass memory (`DM 0xC03D4/5`), `0x120D15` is the ring clear helper | VERIFIED (listing and oracle) |
| DSP time law | an input echoes `trunc(c[0x15]) + 2` samples later (impulse at 20, 21, 100, 513, 1000, 1501, 2000: always `+2`) | VERIFIED (oracle) |
| Time change | `c[0x16]` = previous time, `c[0x17]` = cross-fade weight (0..32768) that moves by `trunc(c[0x19])` per call (2 samples) from the old tap to the new one; the ARM sends `c[0x16] = old`, `c[0x19] = -32768` (snap to the old tap), `c[0x15] = new`, `c[0x19] = +30.199` (ramp: 1085 calls = 45.2 ms) | VERIFIED (ARM message order, oracle weights `0.366` and `0.329` at 12 and 22 calls) |

## 2. ARM half

The handler of `Edt_Effects_kDlyRateClkDiv` is `0xC010C514` (reached through the dispatch `0xC00CC4F0(unit, 0, id, &cell)`; the cell holds `v << 16`, Q16.16, [fx-contexts](fx-contexts.md) s2). Instruction trace of one dispatch: `tools` harness of the run below (47,980 instructions, 8 messages).

```
r5 = (cell + 0x8000) >> 16                      ; v
if cell + 0x8000 >= 0x3F800000: k = 20.0        ; v >= 16256   (0xC010C914)
if v <= 0:                      k = 1500.0      ; (0xC010C920)
i = 1500; while i != 28 and v > T[i - 46]: i -= 46      ; coarse, stride 0x5C bytes
while v > T[i]: i -= 1                                  ; fine, ldrsh [r4, #-2]!
k = i                         if v == T[i]
k = i + (T[i] - v) / (T[i] - T[i + 1])      (f64)       ; soft-float calls 0xC0270BD4 / E18 / A3C
```

`k` is stored as a double at `[obj + 0x138]` and handed to the clock object `0xC0024CC8`, which rounds it for its listeners and calls the delay unit's method `0xC01FF70C` (its integer argument is shifted right by 7 or 6 there); the unit then computes `[obj + 0x30]` (previous time) and `[obj + 0x34]` (new time) with soft-float arithmetic (`0xC01FF5E4`) and `0xC01FEE1C` sends four words through the setters `0xC0206888` (`DM +6` words), `0xC0206720` (`+7`), `0xC02062E8` (`+10`), `0xC0206180` (`+11`) of the delay row. In the captured row (organ unit L3, context origin `c[0]` = DM `0xB289A`):

| Order | DM word | Context word | Value | Reading |
|---|---|---|---|---|
| 1 | `0xB28B0` | `c[0x16]` | `10850.0` (the state's previous time, default 226 ms) | old tap time |
| 2 | `0xB28B3` | `c[0x19]` | `-32768.0` | cross-fade step: snap weight to 0 (old tap) |
| 3 | `0xB28AF` | `c[0x15]` | `min(61438, floor(K k))` | new tap time |
| 4 | `0xB28B3` | `c[0x19]` | `30.199293` (`0x41F19826`) | cross-fade step `+30` per call |

The two `c[0x19]` words do not depend on `v`. The value `c[0x15]` is an integer-valued float32 for every `v` (checked for the 264 points).

Reproduce (about 5 s per value, `tools/fx_clients.py` unit L3, id `kDlyRateClkDiv`):

```bash
uv run --with unicorn python tools/fx_clients.py target/fw_unpacked/bank0_CODE.bin target/fw_unpacked/bank0_SRAM.bin --cache target/settled.snap --units 3 --slots dly.csv --slot Dly
python tools/fx_delay_time.py check target/fw_unpacked/bank0_CODE.bin docs/research/data/fx-delay-time.csv
# table sha256 797dcb4b2e2cb14ec4e1eb240ee795c7450e1cc77a3022657dd41fc57081d912
# 264 captured points, 0 differ
```

Series (`value: c[0x15]`): `0..150: 61438`, `199: 60437`, `1024: 46498`, `2048: 32551`, `4096: 23460`, `8192: 17836`, `12288: 6510`, `14336: 2505`, `16256..16383: 868`.

The word is a **time**, not a rate: a larger id value is a shorter delay. In delay units, `f = 48000 / c[0x15]` is piecewise linear in `v` with slopes that are integer multiples of `1.44e-4 Hz` per count (2, 3, 4, 8, 16, 24, 80, 160 times): the list is the inverse of a hand-built tempo ladder.

## 3. DSP half: variants and the time words

| pc | Frames per call | Time words read | Reading |
|---|---|---|---|
| `0x1206B0` | 2 stereo | `c[0x10]`, `c[0x11]` (write and read offsets into `DM(0xC03D4)` / `DM(0xC03D5)`), `c[0x12]`, `c[0x13]` (fraction word offset and shift) | [fx-routine-models](fx-routine-models.md) s3: taps of a shared pass memory |
| `0x1208AF` | 2 x 2 stereo (loop of two blocks) | the same four words | same |
| `0x120AB5` | 2 stereo | `c[0x14]` (mode), `c[0x15]`, `c[0x16]`, `c[0x17]`, `c[0x19]`, ring `c[0x1A]` (base), `c[0x1B]` (write pointer), `c[0x21]` (length) | own ring buffer, two taps, cross-fade |
| `0x120D15` | none | `c[0x1A]`, `c[0x1B]`, `c[0x21..0x23]` | clears the ring (and the pass memory slots), resets the feedback state |

The shared prelude (ratio smoother, 8 parameter smoothers `0.9908`, feedback all-pass, level meters) is identical in the three; the variants differ only in how the packed 16-bit delay word is written and read. Per call a ring slot is two words (left and right, each holding two packed frames), the write pointer advances by 2 (`dm(i6, m2)`), and the layout of the in-place input and output is `[l0, r0, l1, r1]`.

`0x120AB5`, mode `c[0x14] = 0` (cross-fade taps, the one the ARM clock words drive):

```
x    = clamp(c[0x17] + trunc(c[0x19]), 0, 32768)            ; per channel (c[0x17], c[0x18])
tapA = ring[w - trunc(c[0x15])],  tapB = ring[w - trunc(c[0x16])]
out  = tapA * x / 32768 + tapB * (1 - x / 32768)            ; then damping filter, sends as 0x1206B0
```

Mode `c[0x14] != 0` (VERIFIED in the oracle on the recurrence, ARM driver not observed): the time is smoothed and read with fractional interpolation, `s' = clip(a s - (1 - a) T, +-L)` with `T = c[0x15]`, `a = c[0x19]` (a pole below 1, not the `30.2` of the clock writes), `s = c[0x17]` as float; with `s = -T` the echo sits at `T + 2` as well. The ARM clock words (`c[0x19] = -32768` and `+30.199`) only make sense in mode 0, so the delay clock path runs in mode 0 (ASSUMED: the ARM never writes `c[0x14]` in any capture).

Oracle experiments (`python tools/fx_delay_time.py oracle core2_sram.bin`, variant `0x120AB5`, ring `L = 2048`, unit send, feedback 0, damping `b0 = 1`), first echo sample against the tap word:

| `c[0x15]` | 2 | 3 | 20 | 21 | 100 | 513 | 1000 | 2000 |
|---|---|---|---|---|---|---|---|---|
| first echo (sample) | 4 | 5 | 22 | 23 | 102 | 515 | 1002 | 2002 |

A fractional word (`20.9`) gives the integer delay `22` (mode 0 truncates). Cross-fade check (`c[0x15] = 20`, `c[0x16] = 40`, `c[0x17] = 0`, `c[0x19] = 1000`): echo gains `0.309` at sample 22 (weight `12000 / 32768 = 0.366` times the unity gain `0.844`) and `0.277` at sample 42 (`1 - 22000 / 32768 = 0.329` times `0.844`), `c[0x17]` after calls 0..5 is `1000, 2000, ..., 6000`.

## 4. Closed form

```
k    = index(v)                      # fractional table index, section 2
c15  = min(61438, floor(43.402339 * k))     # words, 48 kHz
D    = trunc(c15) + 2                       # samples from input to first echo
D_ms = D / 48
```

Range `D = 870 .. 61440` samples = `18.1 ms .. 1.280 s`. Fingerprint for tests: sha-256 of the table `797dcb4b2e2cb14ec4e1eb240ee795c7450e1cc77a3022657dd41fc57081d912`, FNV-1a 64 `0xa3b95bd0d7d173d6`, 264 captured `(v, c15)` pairs. Rust: `fx::fw_delay_time::{delay_index, delay_word, echo_samples, echo_samples_at}`, wired behind `FirmwareFx::delay_time` (free-running delay only).

The previous engine table (`DELAY_TIME_MS`, 128 steps from `1500 ms` to `20 ms`, `ns4decode` index `v >> 7`) disagrees with the firmware: at `v = 4096` the engine reads step 32 and the firmware gives `23460 + 2` samples = `488.8 ms`; at `v = 0` the firmware stops at `1280 ms`, not `1500 ms`. The engine keeps its table by default.

## 5. Not verified

| Item | Why |
|---|---|
| Value of `c[0x14]` and the ring length `c[0x21]` in a running Delay | they are written at install (the effect enable path, [fx-contexts](fx-contexts.md) s7), not by the id handler; the law above holds for any `L > T` |
| Feedback loop period | only the first echo was measured; the sends reach the ring one call later and the loop adds the damping filter and the all-pass pair; the loop period is not measured |
| Tempo sync (`kDlyClkEnable`) | the same id passes through the master clock object `0xC0024CC8` with the tempo; not decoded |
| Which of `0x1206B0` / `0x1208AF` / `0x120AB5` the delay slot runs | the module lists all three (tags `0x100, 0x100, 0x200`); only `0x120AB5` consumes the ARM time words, so it is the time-keeping variant |
| Exact `K` | interval only (section 1) |
| Reverb tables at DM `0x9500A..` and the compressor `kCmpResponse` | not attempted in this change: `kCmpResponse` writes no word in any state tried ([fx-contexts](fx-contexts.md) s7), the reverb tables need the multi-tap module routines `0x12314A..` modelled first |

## Implications

- The delay time is now the third `FirmwareFx` switch with both halves verified; engine presets (`delay_rate_norm * 16383`) map exactly onto the 14-bit id.
- The firmware's longest delay is 1.28 s (not 1.5 s) and the knob is nonlinear in time (piecewise linear in frequency).
- The cross-fade of 45 ms is the firmware's declick on a time change; the engine's slewed read position is an approximation of it.

## Follow-ups

1. Capture the enable and install writes (`c[0x14]`, `c[0x1A]`, `c[0x21]`) to name the ring length and confirm mode 0.
2. Measure the feedback loop period with `kDlyFbkAmt` words set (oracle, several calls).
3. Decode the clock object path for the tempo-synced time.
4. Reverb table `DM 0x9500A..` (ns4-ca0i) and `kCmpResponse`.
