# Research: where the Nord Stage 4 organ tone is generated (candidate sweep, ARM wiring of the PNO Core1 bank)

> **Correction (2026-11-13, model attribution):** [organ-model-paths](organ-model-paths.md) (VERIFIED, run) shows that the 92-wheel bank `0x122620`, its mix stage `0x12268F` and the weights / drawbar footage mapping are installed by the **Pipe organ models (Pipe1 / Pipe2)**, not by the B3 model: wherever this note says "B3 tonewheel bank" or "B3 tonewheels", read "Pipe organ bank (models Pipe1 / Pipe2)". The bank algorithm, ladder, mapping and weights stay VERIFIED; only the model attribution changes (the model order matches the UI: ASSUMED). B3, Farfisa and B3 Bass install no generator in PNO core 1 (tone source not located, [OPEN-QUESTIONS](../OPEN-QUESTIONS.md)). Rust: `ToneWheelSource::ModelDefault` applies the bank to Pipe1 / Pipe2 only.

- **Checked:** 2026-11-12
- **Sources (local, never committed):** `target/fw_unpacked/bank0_CODE.bin` (ARM926, loads at `0xC0000000`, OS v1.66) with a `capstone` listing of `0..0x279000` (`uv run --with capstone python tools/arm_xref.py dis ... 0xC0000000 0xC0279000`), the DSP images (`tools/ns4_dsp_images.py`, `tools/ns4_dsp_ldr.py --sram`), an ADI `elfdump` listing of PNO Core1 (`NS4_CCES`, `tools/adi_dis.py SRAM/core1_sram.bin 0x240400 0x24c700`, listing address + `0x120200` = pc), `tools/organ_tone_source.py` (new). Builds on [organ-tonegen](organ-tonegen.md), [organ-tonewheel-bank](organ-tonewheel-bank.md), [pno-module-chain](pno-module-chain.md), [org-stage-map](org-stage-map.md), [org-dsp-map](org-dsp-map.md), [arm-organ-messages](arm-organ-messages.md), [arm-edit-params.csv](arm-edit-params.csv).
- **Question:** Where does the organ actually make its tones: the SY1SY2 or SMP image in an organ mode, software oscillators on the ARM, run-time wave tables, the PNO Core1 bank (which ARM objects drive it), or the pipe samples of the Synth library?

Tags follow `.agents/rules/provenance-in-code.md`: **VERIFIED** = reproduced by a command shown here, **EMPIRICAL** = structure of code or data read from a listing, **ASSUMED** = reading of a purpose.

## 1. Short answer

| Question | Result | Tag |
|---|---|---|
| Best-supported source of organ tones | The **PNO image, Core1**: the 184-element wavetable bank at pc `0x122620` plus the post stage `0x12272D`. New evidence moves it from "about 35 % B3, 65 % piano string resonance" to **about 70 % organ (B3 and probably the other tonewheel-like models), 20 % piano resonance only, 10 % something not yet seen**. | EMPIRICAL |
| Why | (1) The only ARM code that programs the bank (`0xC022D9F0`: 184 increments + group gains) is called from the **organ layer apply function** (`0xC01209C4`, `0xC0120C60`, inside `0xC011FFB8`, the code that handles the `Edt_Org_*` ids) with the **organ VibratoEnable flag**, and from a part-routing function (`0xC0237080..`); no piano-layer code calls it. (2) The organ vibrato-type and Fine Tune handlers write words that the post stage `0x12272D` reads. (3) The piano string-resonance gain is consumed by per-note voice code, not by a window write. (4) The ORG image holds channel strips only. | VERIFIED (call sites, literals), role EMPIRICAL |
| Is anything sampled or ARM-synthesised? | No evidence for either; see section 3. | VERIFIED (absence) |
| What single observation would settle it | Run the real ARM organ layer far enough to send a note-on with `Edt_Org_kModel = 0` and capture which DSP words change (needs the partial boot of [arm-organ-messages](arm-organ-messages.md) s5 to get past `0xC001B82C`), or measure the pitch/timbre of the organ on hardware against `x` and `y` of the bank. Cheaper: find the ARM writer of the weight arrays `DM(0x9566E..0x95949)` and see whether its caller is the organ key handler. | open |

## 2. New ARM facts (VERIFIED unless marked)

### 2.1 Who programs the bank

`python tools/organ_tone_source.py callers bank0_CODE.bin 0xC022D9F0` prints exactly six call sites:

| Site | Function / context | Argument r1 |
|---|---|---|
| `0xC01209C4`, `0xC0120C60` | organ layer apply function (`0xC011FFB8..`, the code with the ids `0x165, 0x166, 0x16B, 0x16C, 0x177, 0x179`), loop over the client objects `0xC05A0424 + 4 (i + 1)`, count `[[this+0xB8]+0xC]` | the layer's vibrato-enable byte `[0xC059FF5C + 0x458 / 0x459]` (written by the `0x16C` branch of the organ handler `0xC011CB60`: `(value == 1)`) |
| `0xC0237080`, `0xC0237090`, `0xC02371C8`, `0xC02371D8` | part-routing function `0xC0236410` (writes `0x800000` / 0 into `DM(0xB44EC, 0xB452C, 0xB456C, 0xB45AC)` and `DM(0xC2C44.. 0xC2CB7)` per part 0..3) | the level stored earlier in `[obj+0x50]` (re-apply) |

The piano layer handlers (`0xC011C798` string resonance and pedal noise; `0xC011EC2C..0xC011FFB8` with the ids `0x151..0x15D`) contain **no** call to `0xC022D9F0` and **no** DSP window literal (`python tools/organ_tone_source.py window bank0_CODE.bin 0xC011C798 0xC011CB60` and `... 0xC011EC2C 0xC011FFB8` print nothing in the piano ranges).

### 2.2 What `0xC022D9F0(obj, level)` writes

Disassembly `0xC022D9F0..0xC022DD30`. It stores `level` at `[obj+0x50]`, then:

| `level` | Group gains (groups 5, 6, 7, 8 of the setter object, 24-bit) | Increment table (184 words written to `DM(0x955B6 + j)` as `value << 8`) |
|---|---|---|
| 1 | `0x600000, 0x600000, 0x300000, 0x300000` (when `[obj+0x4E] == 0`; the other branch is not decoded) | `bank0_CODE@0x443EA8` (table A) |
| not 1 | `0x7FFFFF, 0, 0x7FFFFF, 0` | `bank0_CODE@0x444188` (table B), a **second** table that the earlier notes did not find |

`python tools/organ_tone_source.py tables bank0_CODE.bin`:

```text
table A: copy spread rms 1.90 cent, semitone step 99.99 +- 1.59 cent
table B: copy spread rms 0.00 cent, semitone step 100.00 +- 0.01 cent
B against A: mean -0.11 cent, sd 1.41 cent
```

So the bank has two modes selected by the ARM: B = exact 12-TET semitone ladder, both copies identical, one copy at full gain (`0x7FFFFF`) and the other muted; A = the detuned pair (about 2 cent random per copy) with both copies at 0.75 / 0.375. The earlier "random detune" reading of [organ-tonewheel-bank](organ-tonewheel-bank.md) s4 describes table A only. For the organ layer, **vibrato/chorus enabled selects the detuned-double mode** (ASSUMED: a chorus-like thickening by two detuned copies; the scanner itself is the post stage). For a piano string-resonance bank there would be no reason to tie either mode to an organ edit id.

### 2.3 Organ and master-tune handlers write the post-stage words

`python tools/organ_tone_source.py window bank0_CODE.bin 0xC011CB60 0xC0121C00` (DM words of core 1 of the addressed DSP) and the PNO Core1 listing:

| ARM literal (pool) | DM word | Writer (edit id) | Reader in PNO Core1 |
|---|---|---|---|
| `0xC011D1F4` | `0xB33B8` (two objects) | Fine Tune `0x35`, handler tail `0xC011CD08..0xC011D040`: value `table[0xC027D9D8][cents*64/100 + base + 0x1F3] << 8` | pc `0x121BC6` (`r7 = dm(0xb33b8)`, routine `0x121BBE`) |
| `0xC011D20C` | `0xE20E0` | Fine Tune | pc `0x122778` (`r3 = dm(0xe20e0)`, post stage `0x12272D`) |
| `0xC011D214` | `0xC27D4` | Fine Tune | pc `0x122AAA` (`r0 = dm(0xc27d4)`, listing `0x28aa`) |
| `0xC0120F80` | `0xC27D7` (next to the read of `0xC27D6`) | organ Farfisa vibrato type `0x16A` (4 values): `0x2C3DB00` or `0x1600900` | post stage reads `0xC27D6`, `0xC27D8`; `0xC27D7` is the SIMD partner word (ASSUMED) |
| `0xC0120F88` | `0xE20E3` | organ Vox vibrato type `0x169` (3 values): `[obj+4] = 0xE464 / 0x1782D / 0x240B7`, written `<< 8` | pc `0x12276E` (`r6 = dm(0xe20e3)`, post stage `0x12272D`) |
| (via `0xC022A8AC` -> `0xC022AA48`) | `0xC27C5` | Fine Tune | bank rate factor, pc `0x122620` |

So the same Fine Tune write tunes the bank (`0xC27C5`) and the post stage; the organ-only vibrato-type ids parameterise the post stage, whose triangle LFOs and 19-tap delay loops are a scanner-vibrato candidate ([organ-tonegen](organ-tonegen.md) s3). Fine Tune alone does not separate organ from piano (any pitched generator needs it); the vibrato-type writes do.

### 2.4 The four system sound ids `0x8B..0x8E`

| Id | Name | Handler | Effect (VERIFIED by disassembly) |
|---|---|---|---|
| `0x8B`, `0x8C` | `kPedalNoiseLevel`, `kStringResonanceLevel` | `0xC011C798` (piano layer) | `gain = (int)(10^(dB/20) * 2^23)` stored in `0xC0539580 / 0xC0539584` per layer; read by `0xC00C0740` and `0xC00C0940` inside a per-note level routine (`0xC00C07D8`: loop of 0x72 events, velocity tables). No DSP window write. |
| `0x8D` | `kToneWheelMode` (Clean / Vintage 1 / Vintage 2, default 1) | `0xC011CB60` (organ layer), jump table `0xC011CC24` with `map[v] = {0, 1, 2, 0}` (`0xC02BBC30`) | stores the mode 0..2 into `[[obj+0x34]+0x1C]` of every object in the list `0xC055F504` (count `0xC055F500`) and sets the dirty byte `[obj+0x38] = 1`. It is a parameter of the per-channel objects, **not** a table or routine swap. Where the dirty flag is consumed is not decoded. |
| `0x8E` | `kOrganClickLevel` | `0xC011CB60` | calls `0xC00C25EC(0xC055F4B4, k, mode)` for k = 0..3 with `mode = {1, 2}[v]` (`0xC02BBF98 + 0x6D4`); that function rescales the channel parameter tables (double tables `0xC02B0B78`, `0xC02B0838`, `0xC027C2B0`) and sends them with `0xC022DDC8`. |

The tonewheel mode therefore reaches the same per-channel objects as the click level and looks like a per-channel parameter (3 values), not a choice of oscillator table.

## 3. Candidate table

| # | Candidate | Evidence for | Evidence against | Confidence it is the organ tone source |
|---|---|---|---|---|
| a | SY1SY2 / SMP image in an "organ mode" via the command FIFO opcodes | Both images exist and are sample voices; 6 pipe/flute samples in the Synth library. | No oscillator-like loop, no sine or note table, SMP only has the Q31 interpolation kernel ([organ-tonegen](organ-tonegen.md) s2); `Organ Slot A/B` are UI routing names; the ARM edit layer for `kModel` / drawbars never targets those images (section 2). | 5 % for Pipe only |
| b | ARM926 software oscillators streamed to the DSPs | none found | The ARM writes only parameters through the window port (`0xC055B150`+`0x44`); no ring buffer or audio block writer in the organ layer; ARM926 has no FPU and the same layer code already addresses a DSP bank. | under 3 % |
| c | Run-time wave tables written by the ARM | The ARM does write 184 increments, 4 gains and 7 tune words | The wave tables themselves (`x`, `y`, 4096 points) are initial data of the PNO Core1 image, not ARM output; the 128-entry and 84-entry ARM tables are pitch/tune tables. No ARM loop generates sines or harmonics. | 5 % (for the weight arrays only) |
| d | Model / mode selection (`kModel`, `kToneWheelMode`, vibrato types) | Model, vibrato type and tonewheel mode all land in organ-only handlers; vibrato types write post-stage words | The model id `0x166` is stored in `[0xC059FF5C + 0x450 + 4 layer]` and consumed by branches that compare it with 4 and with the other layer's model; **no routine or table is swapped by model in the code decoded here**. The DSP-side difference between B3, Vox, Farfisa, Pipe is, as far as seen, data: vibrato constants (Vox 3, Farf 4, B3 6 types) and the bank mode. | context, not a source |
| e | Pipe samples of the Synth library | The six `Organ/` samples (Cathedral x2, Gamba 8, Organ Flute 3, Principal 8, Rorflojt 4) fit "pipe" | They live in the Synth sample library for the Synth section; the Organ section has its own `.ns4o` presets (183 B, parameters only) and the manual calls Pipe "non-imitative". | 5 % |
| f | **PNO Core1 bank `0x122620` + post stage `0x12272D`** | section 2: programmed only by organ-layer code, post stage fed by organ vibrato ids, 92 chromatic fixed pitches with the `-19 / -36 / -44 / -48 dB` wave, fixed pitches (notes act through weights) like tonewheels | Lives in the image labelled PNO ([pno-module-chain](pno-module-chain.md) s2 proves the target-to-image mapping, not the name); the two copies / semitone ladder also fit unison strings; the weight arrays (`0x9566E..`, `0x957DE..`) and their senders are not decoded; the ARM class holding the bank has no RTTI. | about 70 % |
| g | ORG strip program (2 cores of ORG + PNO Core2) | Receives the 40-word buffers `0xB0000..` and has LFOs, delay modules | Channel-strip engine only: 74 type indices -> 56 routines with no oscillator ([org-stage-map](org-stage-map.md)); in PNO Core2 the same program runs after the piano chain | 0 % as generator, 90 % as post-processor (EQ, vibrato LFO, delay) |

## 4. Best-supported answer

The B3 (and very probably Vox / Farfisa) tones are produced by the **fixed-pitch wavetable oscillator bank in PNO Core1** (92 semitone positions x 2 copies, no per-note oscillators; notes and drawbars act through weight arrays the ARM writes), followed by the mix stage `0x12268F` and the vibrato/scanner post stage `0x12272D`; the **ORG image runs the channel-strip program** (EQ, filters, LFOs, delays) on the result, which is consistent with the Core2 of the PNO image running the same strip program. The image names `PNO` / `ORG` are positional (download order from the ARM name array), so "the organ chip" may physically be the one called PNO; this changes no conclusion above.

Open and not established: how the piano uses the same bank (if at all: the same ARM object class has four clients, one per part A0/A1/B0/B1, and the part-routing function is common); the weight senders; the frame rate; what the Pipe models use.

## 4a. Cross-check with the captured drawbar path (PR #89, `arm-organ-messages.md` s6)

PR #89 runs the real chain: drawbar edit `0x16D + k` -> `0xC011AB8C` -> callback `0xC022AB2C` -> engine record word `0x10 + k` -> `0xC00BE348` -> jump table `0xC022DDC8` -> sender `0xC001FF18` -> PNO comm 1, core 1. Checked here against the bank (static, `0xC022DDC8` disassembly, PNO Core1 listing):

| Question | Result | Tag |
|---|---|---|
| Which DM words can `0xC022DDC8` reach? | Eight arrays at `0x951F1, 0x95241, 0x95291, 0x952E1, 0x95331, 0x95381, 0x953D1, 0x95421` (spacing 0x50 = 80 words) plus one `0x282D1A20` literal. The four captured words `0x951F1`, `0x95241` fall in this family; `0xB44C9`, `0xB44CB` are in the part-routing block (`0xB44EC`, `0xB452C`, ... of section 2.1). | VERIFIED (code) |
| Do they hit the bank arrays? | **No.** Increments are `0x955B6..0x9566D`, weights `0x9566E..0x95949`, selector table `0xC022A594` bases `0x957DE..0x95C6E`; the captured family sits below `0x95500`. The PNO Core1 routines that index near `0x95100` (listing `0x1145`, `0x1631`, `0x188E`, ...) are the shared library routines of [pno-module-chain](pno-module-chain.md), not `0x122620`. | VERIFIED (addresses), consumer role EMPIRICAL |
| Is the path organ specific? | No. `0xC022DDC8` is also the sender of `0xC00C25EC` (Fine Tune / click, section 2.4), so the engine record path is a generic per-layer channel-parameter path; I did not decode the piano or synth layer records, so whether they send to the same comm/core is **open** (the one check that would settle it: run `set` for a piano id such as `0x15D` in the PR #89 harness and read `emu.messages`). | EMPIRICAL |

Weighing: the organ layer's own parameters go to PNO core 1 (comm 1) and no captured organ message targets the ORG chip. That supports "PNO core 1 carries the organ engine", but it does not by itself link the drawbars to the bank weights, and the shared path means it cannot yet exclude the piano using the same core for its bank. Net change: organ share of the bank role about **65 % -> 70 %**; piano-resonance-only 25 % -> 20 %.

## 5. Implications for `crates/nord-core` (no code changed here)

- Keep the architecture of `organ/firmware_bank.rs`: fixed 92-wheel bank with two copies per wheel. Add the **second increment table** (exact 12-TET, identical copies, one copy muted) as the "vibrato/chorus off" mode and make the detuned pair (table A, gains `0.75 / 0.375` of full scale, i.e. `0x600000 / 0x300000` over `0x7FFFFF`) the "vibrato/chorus on" mode. Tag the table values `VERIFIED` (ARM image offsets above, `tools/organ_tone_source.py tables`) and the organ interpretation `EMPIRICAL`.
- Treat `kToneWheelMode` as a per-channel parameter scale (3 values), not a different waveform; the second wave `y` stays a candidate for the worn-wheel colouring (open).
- Vibrato type constants: Vox `0xE464, 0x1782D, 0x240B7` (`<< 8` into `DM(0xE20E3)`), Farfisa `0x2C3DB00 / 0x1600900` (into `DM(0xC27D6/7)`); useful once the post stage is modelled.
- Do not plan an organ sample player; no ARM-side oscillator is needed.

## Follow-ups

1. Decode the senders of the weight arrays (`DM(0x9566E..0x95949)`, selector table `0xC022A594`) and check that their callers are the organ key handler (settles section 4 "single observation").
2. Decode the `[obj+0x4E] != 0` branch of `0xC022D9F0` (`0xC022DD30..`) and the consumer of the dirty byte `[obj+0x38]` set by `kToneWheelMode`.
3. Find who calls the part-routing function `0xC0236410` and with which layer type (0..13 jump table), to see whether piano parts also enable bank clients.
4. Identify the model-4 branches (`cmp r0, #4` at `0xC01208AC` and siblings): the Pipe models are the most likely users of a different bank mode.
5. Run the post stage `0x12272D` in `sharc-bank` with the Vox / Farfisa constants above to see the scanner delay change.

## Reproduce

```bash
python tools/ns4_dsp_images.py target/fw_unpacked/bank0_CODE.bin --out DIR
python tools/ns4_dsp_ldr.py DIR/dsp_image_1_PNO.bin --sram SRAM
python tools/organ_tone_source.py tables  target/fw_unpacked/bank0_CODE.bin
python tools/organ_tone_source.py callers target/fw_unpacked/bank0_CODE.bin 0xC022D9F0
python tools/organ_tone_source.py window  target/fw_unpacked/bank0_CODE.bin 0xC011CB60 0xC0121C00
uv run --with capstone python tools/arm_xref.py dis target/fw_unpacked/bank0_CODE.bin 0xC022D9F0 0xC022DD30
# PNO Core1 listing (needs CCES): readers of DM(0xE20E3), DM(0xE20E0), DM(0xC27D4..D8)
python tools/adi_dis.py SRAM/core1_sram.bin 0x240400 0x24c700 | grep -i "e20e3\|e20e0\|c27d"
```
