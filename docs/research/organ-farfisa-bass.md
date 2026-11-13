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
cargo run --release -p nord-core --example farfisa_ref -- 60000 farf_rust60k.csv
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
