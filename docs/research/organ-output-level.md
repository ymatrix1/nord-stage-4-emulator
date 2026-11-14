# Organ output level: what the B3 hardware clip gap is made of

- **Checked:** 2026-11-13
- **Sources (local, never committed):** [b3-tail-bootstate](b3-tail-bootstate.md), [arm-boot-rederive](arm-boot-rederive.md), `tools/emu_vs_hw.py` (new `--note-range`), the hardware clip of the Part A video (`_zksT69H30c`, 238 s to 263 s, local only), the factory `.ns4b`, `ns4 inspect`, the ARM harness, `sharc-pass`. The Pipe finding (PR #191, `pipe-video-gap.md`) is quoted from the coordinator, not re-measured here.
- **Question:** the corrected B3 tail leaves RMS +8.0 dB, sub +16.5 dB, presence -6.7 dB, crest -3.5 dB against the clip. Which of it is organ output staging, and which is the way the clip is replayed?

Tags: **VERIFIED** = reproduced by a command, **EMPIRICAL** = measured, **ASSUMED** = reading.

## 1. Short answer

| Question | Result | Tag |
|---|---|---|
| Is the sub / RMS gap a missing low cut or an output level? | **Neither.** The program ([B3 Chords n Solo](#2-the-program)) has two organ layers, A (octave +2, drawbars 808020000) and B (octave 0, drawbars 888633150, 16' at 8). **With layer B switched off the clip lands within about 3 dB:** sub **-1.4** dB (was +16.5), low +1.1, mid +2.1, brilliance -0.8, crest **15.8 dB** (hardware 16.3), RMS **+3.6** dB (was +8.0), presence -5.3 | VERIFIED (run) |
| Why is layer B in the render? | the Rust engine renders layer B because `scene_1_mask = 3` (bits 0 and 1) and `layer_b_enabled`; the program also has `section_enable_ii = 0` and the organ `enable_ii_mask = 0`. The reading that layer B belongs to **section II, which is off**, fits the clip (layer B silent on the hardware) and is **ASSUMED**: the mapping of layers to sections is not decoded and the engine does not read `section_enable_ii` | ASSUMED |
| Low notes | notes below the organ zone also feed the sub: the confirmed notes are mostly 43..47 (G2..B2, 66 %); `split_points = [48, 65, 84]`, `split_zone_enabled = [false, true, false]`, which the engine does not apply to organ notes. Dropping notes below 52 brings the 160 Hz to 1.3 kHz level to within 0.6 dB of the hardware (without layer B removed: +7.3 dB at all notes, +2.5 at notes >= 48, +0.6 at >= 52, 0.0 at >= 53) | EMPIRICAL (run) |
| Rotary | with the rotary off the emulator is 14 to 21 dB too quiet and the crest 21.5 dB: the rotary and its drive (`rotary_drive 2.44`) carry the loudness; presence -3.0 dB without it | VERIFIED (run) |
| Layer level law (127 to DM word) | not found. The handler `0xC011AB8C` (cases `0xB1` / `0xB2`) needs its `this` to have `[this + 0x64]` in {0, 1} (id `0xB1`) or {2, 3} (id `0xB2`) and `[event + 0xC]` = 9 or 10; the layer and client objects of the harness hold other values there (227 and 174 on the clients), so the real `this` is a per-layer controller object that I did not find. Direct calls on both gave no message | VERIFIED (negative) |
| Farfisa in the oracle | **sounds** with the key-down words of `keydown-organ-models.csv` (section 4) | VERIFIED (run) |
| Vox +4.2 dB | see section 5 | |
| Default flip (D22) | **not done** (section 6) | decision |

## 2. The program

`ns4 inspect` of `Program/Bank A/B3 Chords n Solo.ns4p` (fields only, no firmware): organ model B3 on both layers, `rotary_enabled`, `rotary_speed Slow`, `rotary_drive 2.44`, `vibrato_type 5`, `layer_a_level = layer_b_level = 0.795` (101 / 127), `layer_a_octave 2`, `layer_b_octave 0`, drawbars A `8 0 8 0 2 0 0 0 0`, B `8 8 8 6 3 3 1 5 0`, `section_enable_i 4`, `section_enable_ii 0`, `scene_1_mask 3`, split points 48 / 65 / 84 with zone flags `[false, true, false]`.

## 3. Clip metrics with the pieces separated (VERIFIED, `emu_vs_hw.py --only organ_b3 --confirmed-only`, `b3wheels` with the corrected tail)

Emulator minus hardware, dB re RMS (variants made with `edits` on the program, a custom spec; all notes):

| Variant | sub | low | mid | pres | brill | RMS | crest (hw 16.3) |
|---|---|---|---|---|---|---|---|
| default engine | +17.5 | -0.2 | +0.3 | -5.7 | +0.5 | +14.9 | 5.9 |
| b3wheels | +16.5 | +1.2 | -1.1 | -6.7 | -2.2 | +8.0 | 12.8 |
| b3wheels, rotary off | +16.8 | +1.2 | -1.4 | -3.0 | +2.5 | -8.8 | 21.5 |
| **b3wheels, layer B off** | **-1.4** | +1.1 | +2.1 | -5.3 | -0.8 | **+3.6** | **15.8** |
| b3wheels, layer A off | +20.1 | +0.9 | -10.8 | -11.9 | -6.2 | +4.3 | 14.3 |

With the note range cut to the zone `48..64` (`--note-range 48 64`, new option): b3wheels RMS +2.7, mid -2.0, presence -3.9, brilliance +2.4, sub +14.9 (layer B still in); layer B off: sub -11.6, mid +0.6, RMS -1.7, presence -5.5, brilliance +0.8.

So the **sub +16.5 dB and most of the +8 dB RMS are layer B (and the low notes)**, not a missing low cut and not an output gain. After layer B is removed the remaining gap is presence -5.3 dB and RMS +3.6 dB, which is of the size of the unity-rotary assumption and the `L` of OQ19.

## 4. Farfisa (VERIFIED run, oracle only)

The live key-down capture (`organ_fb_pass.capture_chord`, model 2) never writes the Farfisa weights `DM 0x95FFC..0x96173`: the 857-word chord has none of them. The key-down words of `keydown-organ-models.csv` (model 2, zones enabled; per key three words of `0x3CAAAA80` = 1/48 at `0x96020 / 0x96082 / 0x960F0` for key 60) do: the full boot log (the two layer-level sentinels removed), the model 2 install and those words sound:

| Keys | Lines (lane L, 6,000 passes) | Peak |
|---|---|---|
| 60 | 131.8 Hz (0 dB), 263.5 (-1), 523.2 (-7), 786.7 Hz (-9) | 0.0405 |
| 36, 60, 84 | 1046.3 (0 dB), 65.9 (-1), 523.2 (-5), 131.8 Hz (-6) | 0.1079 |

(Hz at the ASSUMED frame rate 43,402.6 Hz of [organ-farfisa-bass](organ-farfisa-bass.md); C3 = 131.8 Hz is 0.75 % above 130.8 Hz.) A dry gain word is not needed (`0xB44C0 = 1.0` makes no difference). The silence of earlier notes was the missing key-down weights, not a lattice table or gain: the lattice words `0x965FC..` that the boot writes are enough. There is **no Rust processor to compare**: `FarfisaWheels` is the generator only ([organ-farfisa-bass](organ-farfisa-bass.md) s5), so no line-by-line table exists; the oracle chord above is the reference for porting it.

## 5. Vox level (+4.2 dB)

Documented Vox chord (`pno-pass-install-vox.script` + the 12 key weights) with the full boot log in front: Rust is **+4.18 to +4.32 dB** louder on every fundamental (row 24: oracle 0.01011, Rust 0.01636; coincident rows within 1 dB), shape unchanged. The delta is **not** the layer-level sentinel family: dropping all 19 words written by `0xC022E3E8` leaves it identical (+4.18 dB), as does dropping only `0xB2ECF` / `0xB2FF0`. Candidates among the boot words (final values near 0.63 to 0.64, i.e. -3.9 to -4.0 dB, **ASSUMED**, not bisected): `DM 0xB4E30` and `0xB4E3A` (0.6345, sender `0xC001509C`), `0xB4EDC` (0.6383) and `0xB4F4A` (0.6401, sender `0xC00152E0`), all rewritten several times by the main loop. A word of about 0.63 is also what the layer level 101 / 127 gives squared, `(101 / 127)^2 = 0.6325`, the default program level, so the Vox level word is likely the squared layer level; the 0.2 dB residual is within the line-measurement scatter. This is for the Vox agent; no Vox file was changed here.

## 6. Decision (D22)

| Criterion | B3 as the engine plays the program | B3 with layer B off |
|---|---|---|
| Oracle chords within 1 dB per line | holds on 2 of 3 chords | same (tone source unchanged) |
| Mid within ~3 dB | -1.1 dB, holds | +2.1 dB, holds |
| Crest within ~3 dB | -3.5 dB, borderline | -0.5 dB, holds |
| RMS within ~3 dB | **+8.0 dB, fails** | +3.6 dB, just over |
| Sub within ~3 dB | **+16.5 dB, fails** | -1.4 dB, holds |

`B3Wheels` **stays opt-in**: with the program as the engine reads it (layer B sounding) RMS and sub fail; the engine's section routing (layer B in section II, off in this program) is the thing to fix first, in the engine's program/section code, not in the tone source. No level law was found, so no organ output-level function was added. **Pipe (PR #191)**: the Legacy organ bus 0.08 is +14.39 dB above the ARM weight unit 0.01526 with `L = 1.0` assumed, and the Pipe bank and mix stage match the oracle to 0.00 dB. The B3 result here says the organ-to-output level for the B3 clip is consistent with `L` near 1.0 once layer B is removed (RMS +3.6 dB with the rotary at unity); that is one clip and one program, so it does not fix `L` for Pipe or Vox.

## Implications

- The B3 hardware gap that looked like an output-stage or low-cut problem is mostly a layer-routing problem (layer B in section II) plus notes outside the organ zone.
- A single organ output-level constant should wait for a second program with one layer.

## Follow-ups

1. Make the engine honour section I / II for organ layers (`section_enable_ii`, organ `enable_ii_mask`) and the zone flags for organ notes, then re-run `emu_vs_hw.py --only organ_b3`; flip B3Wheels (+tail) only if RMS lands within 3 dB.
2. Find the per-layer controller object whose `[this + 0x64]` selects the `0xB1` / `0xB2` cases and record the 127-to-gain law.
3. Port the Farfisa processor against the oracle chord of section 4.
