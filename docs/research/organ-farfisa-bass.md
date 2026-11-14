# Research: Farfisa (`0x122A71` + `0x122B85`) and B3 Bass (`0x122296`) in the oracle

- **Checked:** 2026-11-13
- **Sources (local, never committed):** `target/fw_unpacked/bank0_CODE.bin` / `bank0_SRAM.bin` (OS v1.66), the PNO image `SRAM/core1_sram.bin` (`tools/ns4_dsp_images.py`, `tools/ns4_dsp_ldr.py --sram`) and its ADI `elfdump` listing (`tools/adi_dis.py`, byte = 2 pc), the oracles `sharc-pass` and `sharc-chain` (`crates/nord-sharc`), the booted-ARM harness (`tools/arm_edit_capture.py`, `arm_model_paths.py`, `organ_b3_pass.py`), `capstone` listing of the ARM code, the user manual (`.reference/`, p. 7, 18, 21). Builds on [b3-tone-source](b3-tone-source.md), [organ-model-paths](organ-model-paths.md), [pno-full-pass](pno-full-pass.md), [b3-generator](b3-generator.md), [organ-post-stage](organ-post-stage.md). Derived data: none new (all numbers are in the tables below).
- **Question:** what do the Farfisa generator `0x122A71` and processor `0x122B85` and the B3 Bass processor `0x122296` compute, can their pass be replayed with the ARM's own messages, and which part is exact enough to port (OQ18)? Also: who owns the two event-2 chain objects, and does the value order of `Edt_Org_kModel` follow the UI?

Tags follow `.agents/rules/provenance-in-code.md`: **VERIFIED** = reproduced by a command below, **EMPIRICAL** = fitted or read from data or a listing, **ASSUMED** = a reading.

## 1. Short answer

| Question | Result | Tag |
|---|---|---|
| Farfisa tone source | **The generator `0x122A71` is a free-running top-octave divider**: 12 phase accumulators (one per semitone), each with a ladder of 8 rows (9 for the two lowest), every row a soft pulse wave read from the doubled phase. It has no key input; the keys act in the processor `0x122B85` through ARM weights over the generator ring. Python model `tools/organ_farfisa_ref.py`: **bit-exact** against the oracle on 4 random scenarios and on 40 consecutive calls of the image state (7,680 + words, 0 differences). | VERIFIED (`organ_farfisa_oracle.py verify`) |
| Farfisa processor | 4 weighted sums of the ring (4 x 94 weights, `DM 0x95FFC..`), two short biquad groups, 8 lattice sections selected by a flag, 4 gains (`DM 0xC27E0..E6`), a DC blocker; two frames per pass. **Everything it needs is zero in the image and in every captured ARM burst** (weights `0x95FFC..0x96173`, lattice table `0x965FC..`): with the model install, the engine constructor and a chord the full pass is silent; with ASSUMED uniform weights it stays silent (the lattice table is zero too). The ARM writer of these words is open (OQ19). | VERIFIED (negative), structure EMPIRICAL |
| B3 Bass | Generator `0x121BBE` (the B3 wheels of [b3-generator](b3-generator.md)) + processor `0x122296`. **The complete pass sounds**: chord C4 E4 G4 with the ARM key-down gives lines at 262.0, 330.2 and 392.2 Hz (0, -3.9, -6.4 dB), the 8' partial of each key at the assumed frame rate, no line within 30 dB at 16' (the bass processor sums two short ring windows, notes `n` and `n + 12`, 13 and 16 loop iterations, not the whole ring; ASSUMED reading). | VERIFIED (run) |
| B3 (model 0) with the same ARM bursts | Silent: the dry gain `DM 0xB44C0` is 0 (`f0 += s DM(0xB44C0)`); poking `1.0` gives 131.8, 259.6 and 391.4 Hz for 16', 8' and 5 1/3' of key 60. The Bass has no such gain word, hence it sounds. | VERIFIED (run) |
| Why the previous replays were silent | The booted-ARM harness never ran the engine constructor `0xC002187C`, the only caller of the bulk sender of `DM 0x95471..` (33 / 61 / 14 tap tables and the per-model words, 15,289 words). With the constructor messages first, the B3 / Bass chains have their FIR and lattice data. | VERIFIED (run) |
| Event-2 chain objects | **The second store of each organ layer.** Every organ layer client has two stores; `[store + 0x98]` of the first is the chain object that `Edt_Org_kModel` writes (event = model; chan 0 and chan 2), `[store + 0x98]` of the second is a chain object whose event is **2 permanently** (chan 1 = `0x90fff660`, store `0x90ffd720`; chan 3 = `0x91021e90`, store `0x9101ff50`). They install the Farfisa generator in slot 0 and the Farfisa processors in slots 5 and 7 once, at boot. What the second store edits is not identified. | VERIFIED (run, section 6) |
| `Edt_Org_kModel` value order against the UI | The firmware string pool order is `B3, Vox, Farfisa, Pipe1, Pipe2, B3 Bass`. The manual lists the models in the same order (p. 7, 18: "tonewheel/B3 with a separate B3 Bass mode, two transistor organs (Vox and Farfisa), two Pipe organs"; p. 21: B3 Bass is the 16' and 8' bass drawbar model). Supports the order; the UI selector index itself is not read. | EMPIRICAL |
| Rust port | Opt-in `ToneWheelSource::FarfisaWheels` (`organ/farfisa_wheels.rs`): the generator, closed-form tables, 91-wheel bus. Against the oracle ring over 60,000 frames: max |diff| 4.0e-5, SNR >= 104.5 dB, spectral error 0.00 dB. The processor is not ported (no weights). | VERIFIED (section 5) |

## 2. The ARM side and the replay

```bash
uv run --with unicorn --with capstone --with numpy python tools/organ_fb_pass.py script \
    target/fw_unpacked/bank0_CODE.bin target/fw_unpacked/bank0_SRAM.bin fb_m5.script \
    --model 5 --keys 60,64,67 --cache target/settled.snap       # 23,255 words (model 2: 23,444)
python tools/organ_fb_pass.py run SRAM/core1_sram.bin fb_m5.script --exe target/release/sharc-pass
```

Order of the messages: the engine constructor `0xC002187C` (the only caller `0xC0248CFC`; it sends 32 words of taps `DM 0x95471..` through the bulk sender `0xC001C6A0`, the image defaults of the processors and 15,289 PNO core 1 words in all), the first windows of the booted main loop (default model B3 on both layers), `Edt_Org_kModel = N` on layer 0, the drawbar update of record 0, then the key-down of the drawbar and percussion items of each key (`0xC00BF890`). Model 2 install: 526 words, model 5: 337.

| Model | Slots after the install (layer A) | Output of the pass (set 2 pair 0, 8,000 passes) |
|---|---|---|
| 0 B3 | `0x122A71`, `0x121BBE`, idle, idle, `0x121E2B` (`I1 = 0xB472C`), `0x122B85`, `0x121E2B`, `0x122B85` | silent (dry gain 0) |
| 2 Farfisa | slot 4 = `0x122B85`, `I1 = 0xB474C` | silent (weights and lattice table zero) |
| 5 B3 Bass | slot 4 = `0x122296`, `I1 = 0xB472C` | lines 262.0, 330.2, 392.2 Hz, peak 0.0083, rms 0.0031; side lines about 7 Hz away (-15 dB) |

The frame rate behind the Hz values is the anchor of section 5 (43,402.6 Hz, ASSUMED). The side lines of the Bass (254.2 and 268.2 Hz around 262.0, same offsets around 330.2) are the tapped delay of section 4.2.

## 3. The Farfisa generator `0x122A71` (listing, model, tables)

Registers: SIMD pair `p` holds oscillator `2p` in PEx and `2p + 1` in PEy. Words: LFO rate `DM 0xC27D6`, depth `0xC27D7`, phases `0xC27D8 + lane`, fine word `0xC27D4` (image `0x40000000` = 0.5), polynomial `A, B, C` pairs at `0xC27DA..DF`, pointers (long words) T `0xC27D0` = `0xC2950`, counts `0xC27D1` = `0xC2C1A`, stream S `0xC27D2` = `0xC2AD8`, ring `0xC27D3` = `0xC2B52`. Verified (the model reproduces every word the routine writes):

```text
lfo (lane l)  ph_l += rate (lane 1 also uses lane 0's rate: `r11 <-> s1` runs in PEx only)
              tri = 2 |ph_l / 2^31| - 1;  lfo = A tri + B tri^3 + C tri^5;  m_l = 1 + lfo depth / 2^31
step          K_l = fix(fine m_l 2^31)        (lane 1 multiplies lane 0's fine word)
pair j, lane l, frame f in {0, 1}, with n = 2j + l:
              phase += ssf(K_l, inc_n)
              row r < N_j:   u = (|phase_r / 2^31| + off) slope_(n,r)
                             y = max(b_(n,r) clip(u) + a_(n,r) y_prev, clip(u))      -> ring_f[12 r + n]
                             phase_(r+1) = 2 phase_r + S_(n,r)                       (32 bit wrap)
              N_j = 9 for j = 0, else 8 (DM 0xC2C1A..1F; the loop counter is PEx r3)
```

`y_prev` is the same ring word of the *other* frame (`r2 = dm(m4, i5)` with `m4 = +0x62` in frame 0 and `m5 = -0x62` in frame 1): a one-pole release per slot. Frame 0 writes the ring at `0xC2B52`, frame 1 the ring at `0xC2BB4`, so one call advances the phases twice (2 frames per call, as everywhere in the pass). Memory use per pair: stream `4 + 2 N` words (`inc` x 2, phase x 2, then one offset pair per row, 22 words for pair 0, 20 for the others), table `8 N` words (`off, slope, a, b` as pairs).

Tables (fits against the 108 rows and 12 stream words of the image, `python tools/organ_farfisa_oracle.py tables SRAM/core1_sram.bin`):

| Item | Closed form | Error against the image | Tag |
|---|---|---|---|
| `inc_n` | `round(0x316160 2^(n/12))` (12-TET) | 1 unit on 4 of 12 words (3e-7, 0.0005 cent) | VERIFIED |
| offset of every row | `S = 0xAAAAAC11` (`-0.6667 2^31`) | exact | VERIFIED |
| `off` | `0xBF2AAAA7` (-0.66666645) | exact | VERIFIED |
| `slope_(n,r)` | `-1327.1669 2^-r 2^(-n/12)` | 1.0e-7 relative | VERIFIED (fit) |
| `a_(n,r)`, `b = 1 - a` | `exp(-0.0047343 2^r 2^(n/12))` for `r <= 6`; `r = 7`: exponent `2^11 2^(n/12)` (`a` of 6e-5 .. 1e-8); `r = 8`: `a = 0`, `b = 1` | 1.0e-6 on `a` | EMPIRICAL (fit) |

The pulse: `u = (2/3 - |x|) |slope|` is positive for `|x| < 2/3`, so a row is high for 2/3 of the cycle and low for 1/3, with edges `1 / |slope|` of a cycle wide (2 frames for the lowest row, constant in samples for every row because the slope halves as the frequency doubles). Row 0 of oscillator 0 is C0 = 16.35 Hz at the anchor rate; row `r` of oscillator `n` is MIDI note `12 + n + 12 r` (rows 0..7, rows 8 only for `n < 2`, up to note 109).

Ladder, edge and release together make the divider band limited by the slew of the edge (2 frames) and give the high rows a soft decay of the low level (the `a` weight of the previous frame, time constant `1 / (0.0047343 2^r 2^(n/12))` frames: 211 frames for C0, 3.3 frames at row 6).

Frame rate: oscillator 0 steps `0x18B0B0` units per frame (3.76737e-4 cycles); at C0 = 16.3516 Hz that is **43,402.6 Hz**. The same factor to the 43,156.4 Hz of the bank derivation is 0.99430 (-9.9 cent), the Vox offset of [pno-full-pass](pno-full-pass.md) s4: both models are consistent with a DSP frame rate of about 43,403 Hz, and the Pipe bank, tuned exactly at the 43,156.4 Hz of `lane_mix.rs`, would then be 9.9 cent sharp. ASSUMED (needs a recording), but two independent models agree.

## 4. The processors (listing structure, EMPIRICAL)

### 4.1 Farfisa `0x122B85` (359 instructions, `elfdump` listing)

| Part (listing pc) | What it does |
|---|---|
| `0x122B85..0x122B9B` | record `I1 = 0xB474C + k`: `{i2 = param block, i3 = state block, i10 = PM weights, i11 = PM filter table}` at `I1 - 0x20..-0x1D` (layer A: `0xC2D78`, `0xC2C40`, `0x95FFC`, `0x965FC`; the four records are the four processor slots 4..7: `0xB472C + 4 i`); ring pointer `DM 0xC2C16`, weight pointer `DM 0xC2C18`; frame loop x 2 |
| `0x122BAB..0x122C17` | four weighted sums over the ring pairs (hardware loops of 20, 23, 23 and 22 iterations of two pair steps with `pm(i10)` weights; the last window starts at `0xC2B56`); the weight array of a record is `0x178` words (record spacing `0x95FFC`, `0x96174`, `0x962EC`, `0x96464`), i.e. 4 x 94 |
| `0x122C18..0x122D12` | the four sums are weighted by the param block (`dm(i2 - 0x20..)`), lane sums, two short sections with states in `i3`, then `if ne jump 0x122D88` picks the lattice table `pm(i11 - 0x1E)` or `pm(i11 - 0x1C)` |
| `0x122D18..0x122E55` | lattice sections (`f8 = f0 f3; f13 = f0 f4; ... f1 = f3 - f4`, unrolled 4 or 8 per branch, coefficients `pm(i6)`, states `dm(i7)`) with the output gains `DM 0xC27E0 / E2 / E4 / E6` between the groups |
| `0x122E56..0x122EF5` | coupled sections with states in `dm(i4)`, `0.99800` (`0x3F7F6944`) as a DC-blocker-like constant, the final `dm(i0, m2) = r0` into the slot stream; ring pointer `+ 0x62` for frame 1 |

This table is a reading of the listing (EMPIRICAL), not a model. The weights, filter tables and the lattice selection flag are ARM programs (zero in every captured burst). A bit-exact model on random inputs is possible (the structure is regular) and is the next step; without real coefficients it cannot be compared with audio.

### 4.2 B3 Bass `0x122296` (367 instructions)

| Part | What it does |
|---|---|
| `0x122296..0x1222B0` | same record layout as the B3 processor; saves `I0` in `DM 0xB33C0`, the ring index `DM 0xB33BA` and the frame counter `0xB33BC` are zeroed and advance `+0x68` / `+2` per frame |
| `0x1222B3..0x122333` | two dot products of the B3 generator ring `0xB32E8 + index` (note `n`) and `0xB32F4 + index` (note `n + 12`) with `pm(i10)` weights (13 + 3 and 16 + 3 pair steps): only the 8' and 16' band of the ring contributes (the manual: B3 Bass = 16' and 8' bass drawbars, p. 21) |
| `0x122335..0x122343` | the two sums plus two words from the param block (`dm(m3, i6)`) |
| `0x122343..0x122376` | 33-tap FIR (`DM 0x95471..`, circular `l6 = 0x21`) |
| `0x122380..0x122498` | two lattice groups (`DM 0x95491`, `0x955B0`) writing a delay line at `DM 0xB46E6..` |
| `0x1224D2..0x122542` | 9 taps of that delay line at the indices `m3 = dm(0x10..0x18, i2)` of the param block, weighted by 9 coefficients at `DM 0xC0202..`: a tapped delay, i.e. the Hammond scanner vibrato / chorus (V1..V3, C1..C3), ASSUMED role; the replay shows side lines about 7 Hz from each chord tone |
| `0x122545..0x12261D` | three biquad sections, a DC blocker (`0x3F7D70A4`), the gain `dm(0x1E, i3)`, the store `dm(i0, m2) = r0` and the ring bookkeeping |

This also explains the "line box tail" of [b3-generator](b3-generator.md) s4: `DM 0x95471..0x954E5` are the FIR and lattice data of the **scanner** in the processor, written by the engine constructor.

## 5. Port and oracle comparison

```bash
python tools/organ_farfisa_oracle.py verify SRAM/core1_sram.bin --exe target/release/sharc-chain
python tools/organ_farfisa_oracle.py tables SRAM/core1_sram.bin
cargo run --release -p nord-tools --bin farfisa_ref -- 60000 farf_rust60k.csv
python tools/organ_farfisa_oracle.py compare SRAM/core1_sram.bin --exe target/release/sharc-chain \
    --frames 4000 --rust farf_rust.csv                      # Python model + Rust
```

| Check | Result |
|---|---|
| Python model against the oracle | 4 random scenarios (random tables, counts 2..8, phases, LFO): 0 differing words; image state, 40 consecutive calls: 0 differing words |
| Closed-form Python model against the oracle ring, 4,000 frames, 98 slots | max abs 1.2e-5, worst SNR 105.4 dB, spectral error 0.00 dB (bins within 60 dB of the peak) |
| Rust `FarfisaWheels` against the oracle ring, 60,000 frames, 98 slots | max abs **4.0e-5**, worst SNR **104.5 dB**, spectral error **0.00 dB** |
| Rust against the Python golden vectors (`farfisa_golden.rs`, 3 frames, 3 oscillators) | below 2e-4 |
| Default render | `render_hash` unchanged: `25baa387b0219c0835094150106a35ce751b9c16b68109d2a030596e78e7b59a` |

Host rate: the Rust source keeps the pitch (12-TET, A = 440) and the edge and release widths in samples at any sample rate (`rho = 43,402.6 / fs`). Wheel `w` of the bus is note `24 + w`: wheels 0..85 are filled, 86..90 (notes 110..114) are silent (the divider has no note there), the lowest row (notes 12..23) is dropped.

## 6. Owners of the event-2 chain objects (`python` one-off, `tools/arm_edit_capture.py`)

`aec.find_stores` of the settled state lists two stores per organ layer (`0x90ffab40` / `0x90ffd720` for layer 0, `0x9101d3d0` / `0x9101ff50` for layer 1). `[store + 0x98]`: `0x90ffca80` (event 0), `0x90fff660` (event 2), `0x9101f310` (event 0), `0x91021e90` (event 2). `Edt_Org_kModel` (`0xC011C9BC`) only writes the first store's chain. The boot slot table of [b3-tone-source](b3-tone-source.md) s4.1 follows: chan 1 and chan 3 write generator slot 0 and processor slots 5 and 7. The piano layers share one chain object (`0x90c81490`, event 22). What the second store represents (a second organ engine: percussion or the bass drawbars) is open.

## 7. Corrected-state oracle chords and the processor inputs (2026-11-13 follow-up)

Method: `tools/organ_proc_oracle.py` (script = full boot log without `DM 0xB2ECF` / `0xB2FF0` + model install + key-down words, [arm-boot-state](arm-boot-state.md) s4), `sharc-pass` PNO core 1, 3,000 passes, set 2 pair 0 lane L, lines at the 43,402.6 Hz anchor (bin width about 8 Hz, so the Hz values are quantised; use the table for levels). New oracle option `sharc-pass --rec ADDR:COUNT:FILE` writes DM words after every pass (ring, state, slot stream, for later model checks). Table: [organ-proc-oracle-lines.csv](data/organ-proc-oracle-lines.csv).

| Model, chord (keys) | Peak | Strongest lines (Hz at dB below the strongest) | Tag |
|---|---|---|---|
| Farfisa, 60 64 67 | 0.0867 | 258.7 (0), 392.3 (0), 133.5 (0), 166.9 (-1) | VERIFIED (run) |
| Farfisa, 48 55 64 | 0.0806 | 133.5 (0), 66.8 (-3), 100.2 (-3), 192.0 (-3) | VERIFIED (run) |
| Farfisa, 72 76 79 | 0.0693 | 784.6 (0), 659.4 (-1), 525.8 (-5), 1043.3 (-7) | VERIFIED (run) |
| Farfisa, single key 60 | 0.0405 | 131.8, 263.5, 523.2, 786.7 Hz (0, -1, -7, -9 dB) | VERIFIED (run, reproduces the cited figures) |
| B3 Bass, 60 64 67 | 0.0679 | 258.7 (0), 325.5 (-2), 392.3 (-2), 525.8 (-38) | VERIFIED (run) |
| B3 Bass, 48 55 64 | 0.0892 | 192.0 (0), 133.5 (-1), 325.5 (-3), 392.3 (-37) | VERIFIED (run) |
| B3 Bass, 72 76 79 | 0.0487 | 525.8 (0), 659.4 (-4), 784.6 (-5), 1043.3 (-40) | VERIFIED (run) |

What the Farfisa processor reads (VERIFIED by the runs above and by `sharc-chain` pokes):

| Input | Value / source |
|---|---|
| Record `I1 = 0xB474C` (layer A, slot 4) | `{i2, i3, i10, i11}` at `I1 - 0x20..-0x1D` = `0xC2D78, 0xC2C40, 0x95FFC, 0x965FC` (image defaults; boot, install and key-down never write the record) |
| Weights `DM 0x95FFC + 94 s + idx` (sum `s` = 0..3) | zero in image, boot and install; **key-down only**: key `n` writes 1/48 (`0x3CAAAA80`) at `0x95FE4 + n`, `+ 0x62`, `+ 0xD0` (sum 0 idx `n - 24`, sum 1 idx `n - 20`, sum 2 idx `n - 4`) and one bias word each at `0xC2D58, 0xC2D59, 0xC2D5C` (-0.00958, the same for every key; `keydown-organ-models.csv` notes 36, 60, 84). A probe with one weight and one ring slot set to 1.0 shows sum 0 idx 10 reads ring slot 10 and sum 1 idx 10 reads ring slot 18; sums 2 and 3 were not resolved (the output is delayed by the filters) |
| Lattice table (`i11 = 0x965FC`) | the processor reads only `pm(i11 - 0x20 .. -0x1C)`: a pointer table `0x965DC..` that is **in the image** (DM pointers such as `0xC2CD8, 0xC2920, 0xC27E8, 0xC2860`), not zero. The 204 boot words at `0x965FC..` belong to another consumer. The coefficient banks the pointers address (`0xC27E0..0xC2D78`) are written by the **boot** (272 words, 142 non-zero) and the **model install** (180 words, 129 non-zero); the state block `0xC2C40..` by the boot (46 words) |
| Output | `dm(i0, m2) = r0` once per frame (SIMD: PEx lane at `i0`, PEy lane at `i0 + 1`), two frames per call; from zero state the first calls are about 5e-6 |

Who writes "the rest of the lattice table" (OQ19): nothing is missing. The old silent Farfisa pass needed only the key-down weights on top of the corrected boot log; the claim of s1 that the table `0x965FC..` is zero concerned a range the processor does not read.

Not done in this follow-up (stop point at the 90 minute time box): bit-exact Python models of `0x122B85` / `0x122296` (SIMD listing with PEx / PEy swaps `r5 <-> s4`, circular buffers and a record-flag selected lattice; `sharc-chain --trace` gives every register per instruction, so a model can be checked step by step), the Rust processor stages, the line-by-line Rust comparison, `emu_vs_hw.py` for Farfisa and the video check. Defaults and `render_hash` are unchanged. The processors are linear in the ring and the key-down weights (the lattice flag is a record word, not a signal), so the measured-response option of `b3_tail.rs` applies to both; the exact model stays the target.

## 8. Measured line gain of the Farfisa processor (EMPIRICAL)

Tool: `tools/organ_farfisa_proc.py measure` (`sharc-chain`: generator `0x122A71` + processor `0x122B85` per call from the boot-plus-install state, all weights zero but one word 1.0, 4,000 calls, ring slot and output recorded together, Hann FFT, gain `|Y| / |R|` at every ring line within 50 dB of the peak, 22 slots per sum, binned to 1/6 octave). Data: [farfisa-proc-gain.csv](data/farfisa-proc-gain.csv) (sum, Hz, dB, spread). Rust: `organ/farfisa_proc_gain.rs` (`farfisa_proc_gain(sum, hz)`, test against the CSV), **not wired into the render path**.

| Check | Result |
|---|---|
| Linearity (weight 1.0 against 0.5, divided by the weight) | max difference 0.00 dB over 110 bins (VERIFIED) |
| Note independence | median spread of the slots inside one 1/6 octave bin 0.9 dB (it includes the slope of G inside the bin), so G depends on frequency, not on the note (EMPIRICAL) |
| Sums | sum 0 reads ring slot `idx`, sum 1 slot `idx + 8`; a key therefore feeds the 16' row (`note - 12`) through sum 0 and the 8' row (`note`) through sum 1 (the third key word, sum 2, and sum 3 give output 0.0 exactly with the installed gains, also with weight 1.0 at the key's own index) |
| Shape of G | sum 0: -1 dB at 60 Hz, -6 dB near 400 Hz, +1.2 dB at 900 Hz, then falling to -26 dB at 8.6 kHz; sum 1 has a similar shape (see the table) |
| Output scale | gain per unit weight is O(1): a weight of 1.0 gives rms 0.5; the key weight 1/48 gives the chord peaks of s7 |

Not done (time box): per-line comparison of a Rust render with the oracle chords (needs the Farfisa key to bus mapping with two filters per wheel: a wheel is the 8' row of key `n` through sum 1 and the 16' row of key `n + 12` through sum 0, so the bus-of-wheels path cannot apply one table per wheel; the filter has to act on the key mix or on two wheel copies), the B3 Bass table (its weights come from the ARM key-down `DM 0x951F1..` over two ring windows, not the Farfisa words), `emu_vs_hw.py` with `--organ-level firmware`, and the video check. Defaults and `render_hash` unchanged; the opt-in sources stay opt-in because the oracle criterion is not met (no processor in the render path) and the video check was not run.

## 9. Wiring of the Farfisa key stage and the B3 Bass status (2026-11-14)

Wired: `organ/farfisa_keys.rs`, opt-in `--farfisa-processor on` ([organ-engine](../specs/organ-engine.md) s3.1.8). Per key `n` the firmware key-down writes `1/48` into sum 0 (ring note `n - 12`) and sum 1 (note `n`); the stage multiplies the two wheels with the measured gains of s8 (two linear-phase FIRs). Check (`cargo test -p nord-core --lib farfisa_keys`): chord a, keys 60 64 67, generator `FarfisaWheels` at 48 kHz, 1 s Hann Goertzel at the note frequencies: C4 -43.0, G4 -43.8, C3 -45.9, E3 -45.7 dB (oracle 0, 0, 0, -1 dB re the strongest), peak 0.08835 against 0.08666 (+0.17 dB). Open: the 16' rows are about 2.5 dB lower than in the oracle (phase and the lattice are not modelled; the table is a median over 1/6 octave bins), the tab selection and the vibrato.

B3 Bass: **not ported.** What is known for a port: the oracle chords of s7 show only the 8' partial of each key (C4 E4 G4 at 0, -2, -2 dB, nothing at 16' within 38 dB), so the Bass processor reads the ring at note `n` and the note an octave above with weights that the ARM key-down writes at `DM 0x951F1..` (`keydown-organ-models.csv`, model 5, notes 36, 60, 84, index words at `DM 0xB4246..`); the sums feed the 33-tap FIR, two lattice groups, the 9-tap scanner delay and three biquads (s4.2). A line-gain stage like the Farfisa one needs the same `sharc-chain` measurement for the Bass record (`0xB472C`) on the B3 generator ring; it was not made, and the per-key weight words of model 5 were not decoded into a closed form. Until then `b3_bass_mode` keeps its ASSUMED pickup table ([organ-engine](../specs/organ-engine.md) s3.1.1).

## 10. B3 Bass key-down weights for every key, and why there is no line-gain table (2026-11-14)

Tools: `tools/organ_bass_keys.py capture` (the real ARM key-down of `Edt_Org_kModel = 5`, drawbars `888888888`, velocity 100, keys 24..108, one emulator session; data [bass-keydown.csv](data/bass-keydown.csv) = `key,window,index,value_bits`), `tools/organ_bass_proc.py chain` / `measure` (`sharc-chain`: generator `0x121BBE` + processor `0x122296` per call, ring slots of both frames at `+0x68`).

| Finding | Result | Tag |
|---|---|---|
| Weight windows (listing `0x122296`) | window 1 `DM 0x951F1 + k` (56 words, ring slot `0xB32E8 + k`), window 2 `DM 0x95241 + k` (68 words, ring slot `0xB32F4 + k`), both added | VERIFIED (listing, `adi_dis`) |
| Closed form of the key-down | the **percussion item** writes one word per key into window 2 at index `n - 24` (keys 24..91, 68 of 68 keys; keys 92..108 none, key 103 folds onto index 67); value = per-key taper table (0.00908 up to key 36, 0.0166 at key 66, 0.0116 at key 84), `organ/b3_bass_keys.rs` | VERIFIED (ARM run, test `table_matches_the_derived_csv`) |
| Window 1 (drawbar item, footages 16' / 5 1/3' / 8' at partial note `- 36`) | inaudible: with window 1 zeroed the chord `60 64 67` is bit-identical (`chain --zero 1`); with window 2 zeroed the output is 0 | VERIFIED (run) |
| Chord goldens of the oracle (`sharc-chain`, 8192-point Hann, line amplitude) | keys 60, 64, 67: 0.02548, 0.02449, 0.02050; the same keys alone: 0.0259, 0.02491, 0.02565 (-0.15, -0.15, -1.95 dB for the chord) | VERIFIED (run) |
| 2nd harmonic of the output | -38 dB below the fundamental (ring harmonic -15.7 dB): the processor attenuates harmonics | VERIFIED (run) |

**Why the Farfisa recipe (`G(f)` per weight window, `farfisa-proc-gain`) does not carry over.** The gain `|Y| / |R|` of one weight word at 1.0 and one ring slot is **state dependent**: for ring note 60 it is +16.1 dB after the chord script (3 keys) and +2.3 dB after the 85-key script (`measure`; linear in the weight in both). The difference is not the weight windows: the key-down also writes the lattice and filter words `DM 0x954E6..0x95529` (117 words differ between the two scripts: pointers `0xB33xx` into the generator ring, `0xB45xx` into the delay line, coefficients about -0.004 .. -0.024), i.e. per-key resonator sections (`arm_model_paths.py`: "resonator B") that the ARM allocates in key order. The output of a chord therefore depends on the slot a key lands in and on the A words that several keys share (C4 E4 G4 gives G4 -1.95 dB below its single-key level; C4's 5 1/3' partial rewrites the A word of note 67, ASSUMED cause).

Consequence: a Rust port that meets the 1 dB line criterion against the oracle needs the per-key section words (`0x954E6..`, `0x955B0..`) and the A-word accumulation rule, not a per-sum gain table. The weight table of this section is exact and committed (`b3_bass_keys.rs`) but **not wired**: `b3_bass_mode` keeps its ASSUMED pickup table, defaults and `render_hash` are unchanged, and no hardware clip comparison (`emu_vs_hw.py`) was made.

Follow-ups (superseded by s11: the section words are not the cause; s12 closes the ring weights): (1) decode the section words the key-down writes per key (slot allocation by key order, pointer pairs, coefficient closed form) from the same ARM capture; (2) the A-word accumulation (several footages and keys onto one ring note, `keydown-zones.md` s6.2); (3) then the per-key response, the Rust stage and chord goldens within 1 dB.

## 11. B3 Bass state: the "section words" are not the cause, `G` is (2026-11-14)

Tools: `tools/organ_bass_state.py capture` (the ARM key-down for keys 24..108, [bass-state.csv](data/bass-state.csv) = per-key increments of window 1 / 2 words and the step of `S`), `tools/organ_bass_proc.py measure` (now with `G` pinned to 0; [bass-proc-gain.csv](data/bass-proc-gain.csv)), `sharc-chain` chords with the ring recorded (goldens [bass-chord-goldens.csv](data/bass-chord-goldens.csv): 1, 3 and 10 keys, 3 key orders). Rust: `organ/b3_bass_state.rs` (not wired).

| Finding | Result | Tag |
|---|---|---|
| Section words `DM 0x954E6..0x9552F`, `0x9559C..0x955AF`, `0xB4246..` | Not the cause of the state dependence: zeroing `0x954E6..0x95530` changes the chord output by 4 % rms (0.3 dB), `0x9559C..` and `0xB4246..` by 2 %. They are per-key voice records (5 pair groups per key, a second key takes `0x95518..0x95549`, later keys rewrite group 0); the processor listing never addresses them. Correction of s10 | VERIFIED (run, `target` scripts of the commands below) |
| What flips the gain by 13.4 dB (3-key against 85-key script, ring slot 48) | One word, `DM 0xB44C9` (`G`): copying only that word of the 85-key state into the 3-key state reproduces it (-2.70 to -16.13 dB); `0xB44CA`, `0xB44CB` do not matter | VERIFIED (bisect over the 281 differing words) |
| Output law | output = `(1 - 0.84 G)` x linear part, to 0.01 at G = 0.2 .. 0.95 and at three ring slots (sweep of the word, G = 0.4 is -3.55 dB) | VERIFIED (run) |
| Accumulation of `G` | `G = phi(S)`, `S` = plain sum of per-key steps `inc0(key)` (0.0257 for keys 24..36, 0.0262 / 0.0258 / 0.0261 for 60 / 64 / 67), `phi` piecewise linear with slope `2^-n` on `S` in `[n/2, (n+1)/2)` (the ARM halves the step above `G` = 0.5, quarters it above 0.75, ...; a step that crosses a breakpoint is split, key 43 of the ascending run). Order independent. Six replays (1, 3 keys both orders, 10 keys, 85 keys): max error 5e-7 | VERIFIED (run), steps of keys 24..108 derived from one ascending run |
| Weight windows | **Additive** over the held keys, increments per key in `bass-state.csv` (reproduces every window word of 5 replays exactly). Window 1 is **audible** (zeroing it leaves 36 % of the output rms in the 3-key script; s10 claimed bit-identical, that held only for its script) | VERIFIED (run) |
| Generator weights `DM 0x95122..` (the ring is `wheel x weight`) | **State dependent and not additive**: a wheel written by a second key keeps a smaller value (0.5465 to 0.4367, 0.3532 to 0.2436, ratios 0.69 to 0.82); the ring amplitude of slot 48 is 0.262 after 3 keys and 0.061 after 85. The rule is not decoded, so the full stage cannot be built | VERIFIED (run), rule OPEN |
| Processor line gain at G = 0 | Per window a smooth function of the line frequency (bin spread 0.0 to 1.5 dB): window 1 +8.4 dB at 74 Hz, -5.9 at 260 Hz, -29.6 at 1.2 kHz; window 2 +9.1, -2.1, -15.6 | EMPIRICAL (1/6 octave bins) |
| Processor part against the oracle | `line = ring_amp x (W1 g1(f) + W2 g2(f)) x (1 - 0.84 G)` with the oracle's own ring line amplitudes: 45 lines of 6 chords (1, 3 and 10 keys, press orders ascending, descending, shuffled), max 1.45 dB, rms 0.4 to 0.65 dB, 42 of 45 within 1 dB; coherent sum of the two windows (power sum: 3.1 dB) | VERIFIED (run, `b3_bass_state` test `chord_goldens_match_the_oracle_lines_within_1p5_db`) |

```bash
python tools/ns4_dsp_images.py target/fw_unpacked/bank0_CODE.bin --out target/DSP
python tools/ns4_dsp_ldr.py target/DSP/dsp_image_1_PNO.bin --sram target/PNO
uv run --with unicorn --with capstone --with numpy python tools/organ_bass_state.py capture     target/fw_unpacked/bank0_CODE.bin target/fw_unpacked/bank0_SRAM.bin docs/research/data/bass-state.csv
python tools/organ_bass_proc.py measure target/PNO/core1_sram.bin <script> out.csv --exe target/release/sharc-chain
cargo test -p nord-core --lib b3_bass_state
```

**Not met: the <= 1 dB criterion on a full Bass stage.** The processor half (weights, `G`, line gain) is exact to 1.45 dB given the ring; the ring needs the generator weight rule above. Nothing is wired, `render_hash` is unchanged, and `emu_vs_hw.py` was not run (a stage that is not in the render path cannot change the clip; the bass band delta is therefore 0.00 dB).

## 12. B3 Bass: generator weight rule, key-up, and the wired opt-in stage (2026-11-14)

Tools: `tools/organ_bass_gen.py capture` (ARM key-down of keys 24..108, drawbars `888888888`; data [bass-gen-weights.csv](data/bass-gen-weights.csv) = table `T` and writes per wheel and key), `cap` scripts for the real MIDI path (`arm_zone_capture.prepare` + note-on / note-off, local only). Rust: `organ/b3_bass_gen.rs` (rule), `organ/b3_bass_stage.rs` (stage), `organ/gain_fir.rs` (FIR design shared with the Farfisa stage).

| Finding | Result | Tag |
|---|---|---|
| Rule of the generator weights `DM 0x95122 + (note - 36)` | The word of a wheel is a **pure function of the number `n` of earlier writes to that wheel still held**: `weight(note, n) = T[note][n]`, `T[note][0]` = the first-write amplitude `A`. Not additive; not dependent on the key, the footage item (drawbar or percussion), the press order or the drawbar levels (a zero drawbar still writes). 297 of 297 writes of 9 sequences (single key, 60 + 60, 60 + 72, 60 + 67, 67 + 60, 60 64 67, 8 keys, all 85 keys) match `T` bit for bit | VERIFIED (run, `tools/organ_bass_gen.py`, test `chord_words_match_the_arm_key_down`) |
| Size of the table | 79 wheels (notes 36..114), up to 27 writes per wheel at the top of the keyboard; the words of notes 12..35 stay 0 in the image, so ring slots 0..23 are silent | VERIFIED (image, run) |
| Shape of `T[note]` | falls monotonically; ratio `T[1]/T[0]` is 0.78 to 0.82 on wheels 60 to 96 and 0.94 to 0.96 on wheels 36 and 48, the ratios of one wheel approach 1 (wheel 79: 0.785, 0.820, 0.841, ...). No closed form found (reciprocal, power and exponential forms leave 0.3 to 1.3 % residual); the table is committed | VERIFIED (table), law OPEN |
| Key-up (real MIDI path, notes 60 64 67, key-up of 64) | The key's writes are removed one by one and the word becomes `T[note][held - 1]` (wheel 88 back to 0.54649, wheel 83 passes 0.37065 and ends at 0.46984, wheel 76 with no other holder returns to `T[0]` = 0.39976). Window words go back to the sum of the remaining keys (the burst of tick 117 rewrites them with float residues of 2e-5). `G` first drops by the step of the key (0.01346 to 0.00897) and then an idle rebuild sets 0.01071, 19 % above the replay (0.01 dB of output scale) | VERIFIED (one run), rebuild ASSUMED to be a resync |
| Real MIDI path against the direct item call | Same weights per wheel (first writes and the table agree), but the real path uses the default program's drawbars (8 footages written for key 60, `G` steps 0.0045 instead of 0.0262, 5 1/3' of key 64 at wheel 59). The key tables of s10 / s11 are the `888888888` direct call; the stage uses them for every drawbar setting | VERIFIED (run), consequence ASSUMED |
| Ring check | wheel 60 after 85 keys is 0.2313 of its first write (11 writes); with 0.262 after 3 keys this gives the 0.061 of s11 | VERIFIED (test `all_keys_pull_the_weights_down_by_the_arm_table`) |

**Stage** (`--b3-bass-processor on`, `OrganLayer::set_b3_bass_processor`): `ring[s] = wheel(note s + 12) * T`, `x1 = sum W1[k] ring[k]`, `x2 = sum W2[k] ring[12 + k]`, two 511-tap linear-phase FIRs from the measured window gains (`bass-proc-gain.csv`), output `* (1 - 0.84 G)`, folded into one coefficient per wheel and window and ramped over 2 ms. It needs `ToneWheelSource::B3Wheels` on a B3 Bass layer; turning it on selects the wheel tables without the B3 tail.

| Check | Result |
|---|---|
| Chord 60 64 67 against `bass-chord-goldens.csv` rows `g3` (6 lines, 1 s render at 48 kHz, Goertzel at the stage frequencies, `cargo test -p nord-core --lib chord_lines_follow_the_oracle_goldens`) | +1.06, +1.42, +1.55, +2.50, +1.67, +1.57 dB (stage louder). Mean +1.6 dB: the oracle values are 8192-point Hann lines and read up to 1.4 dB low (scalloping, the ring line of slot 48 reads 0.262 where `A x table` is 0.289, -0.85 dB); the rest is the magnitude-only FIR and the project tuning. **Not within the 1 dB bar** for slot 48 (2.5 dB) |
| Key-up, octave shift, ring-down | tests `release_rings_down_to_silence_and_stays_finite`, `octave_shift_registers_the_transposed_key_and_releases_it` |
| Default render | `render_hash` unchanged (flag off by default, no code path changes when off) |

**Clip comparison and default (D22, D41).** The B3 clip of [organ-presence-gap](organ-presence-gap.md) (`B3 Chords n Solo`) plays the **B3** model, not B3 Bass: the stage requires `b3_bass_mode`, so the bass band delta of `emu_vs_hw.py` with and without `--b3-bass-processor on` is **0.00 dB by construction**; no hardware clip of a B3 Bass program is available, and the chord check above is 1.1 to 2.5 dB. No default is flipped.

Still open: the stage does not model the drawbar and percussion settings (the key tables are the `888888888` key-down; the real default program writes smaller window steps, so levels differ), the bit-exact `0x122296` model (with the 1.45 dB residual of the processor part), the scanner inside the processor, the closed form of `T`, the `G` resync after a key-up, and a B3 Bass hardware clip.

## Implications

- The Farfisa generator is fully decoded and ported; the Farfisa audio is not reachable until the ARM weights and the filter table are captured (OQ19). The documented structure (four weighted sums, biquads, 8 lattice sections, four gains) gives the model target.
- The B3 Bass sounds with the ARM data we have; its processor holds the scanner. A bit-exact model of `0x122296` (and `0x121E2B`) is the path to the B3 / Bass default; the B3 wheels (`B3Wheels`) already supply the ring.
- `docs/research/b3-generator.md` follow-up 1 (writer of `DM 0x95471..`) is closed: the engine constructor `0xC002187C`.
- The DSP frame rate is probably about 43,403 Hz (Farfisa and Vox agree); the Pipe bank rate (43,156.4 Hz) is 9.9 cent off that reading (ASSUMED).

## Follow-ups

1. Find the ARM writer of the Farfisa weights `DM 0x95FFC + 0x178 i`, filter tables `0x965FC..` and the lattice flag (the sender `0xC0224468` family with the record table `0xC0282ED0`, [pno-full-pass](pno-full-pass.md) s6), then replay the Farfisa chord and port the processor.
2. Bit-exact Python models of `0x122296` and `0x121E2B` on random inputs (the layout is in section 4.2), then the scanner as a Rust stage on `B3Wheels`.
3. Identify what the second store of an organ layer edits (chan 1, 3).
4. Record the real frame rate (a C0 / C4 recording) and re-anchor `DSP_FRAME_RATE`.
