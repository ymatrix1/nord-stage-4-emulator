# Engine layer routing: section enables, layer zones and the B3 clip

**Checked:** 2026-11-13
**Sources:** `docs/research/keydown-zones.md`, `docs/research/emulator-vs-hardware-audio.md`, the B3 finding of PR #194 (`feat/organ-output-level`, `organ-output-level.md`, not merged when this was written), the 384 factory programs of `Nord Stage 4 Factory Restore RevK v1.66.ns4b` (local, git-ignored), `target/fw_unpacked/bank0_{CODE,SRAM}.bin` (local, v1.66), `tools/arm_program_routing.py` (new), `tools/arm_zones.py`, `tools/arm_edit_capture.py`, `tools/emu_vs_hw.py`, `ns4 render-patch --firmware-routing` (new). Only addresses, offsets, derived values and counts are recorded; no firmware bytes.
**Question:** The B3 clip (`B3 Chords n Solo`) sounds organ layer B in the engine but not on the hardware. Which fields of the program decide which layers sound and which keys they accept, what does the firmware do with them, and does an engine that follows them close the gap?

Tags: **VERIFIED** = reproduced by a command, **EMPIRICAL** = fitted to factory data or recordings, **ASSUMED** = reading.

## 1. Answer in short

| Question | Answer | Tag |
|---|---|---|
| Is layer B of `B3 Chords n Solo` silent because it belongs to section II? | **No.** Section I is organ only (`section_enable_i = 0b100`) with both layers on (organ `enable_i = 0b11`), section II is empty. On the ARM the zone enable byte of both organ layers is 1 in scene I (section 2). Layer B is silent on the hardware because of its **keyboard zone**: the layer blocks store `Edt_Org_kKbZone` = 0 for A and 6 for B, and the Mid split point (65) is the only active one, so A plays the keys below 65 and B the keys from 65 | VERIFIED (ARM zone bytes), EMPIRICAL (zone codes, section 3) |
| Which fields decide which layers sound? | `section_enable_i / ii` (global bits `284..290`), per-section layer enables `enable_i / ii` (organ and piano 2 bits at `9` / `12`, synth 3 bits at `10` / `15`), the active scene. Zone byte of layer `i` = section enable AND layer-mask bit `i` | VERIFIED (ARM, 30 rows, 0 mismatches) |
| Which fields decide which keys a layer accepts? | `split_enabled` and the three point flags `split_zone_enabled` (bits `8`, `9..11`; the firmware ids `Edt_Com_kSplitEnabled / Low / Mid / High`), the point notes and crossfades, and a **4-bit zone code per layer** (`kKbZone`: organ / piano layers `+8`, synth layers `+58`). `octave` is the 4-bit field after it | VERIFIED (field names and order), EMPIRICAL (code meaning) |
| Does the engine now route like that? | Yes, behind `FirmwareRouting::On` (`--firmware-routing`); default `Off`, `render_hash` unchanged | built, tested |
| Does it close the B3 clip gap? | see section 5 | EMPIRICAL |

## 2. ARM check: the zone enable byte follows the program's section / layer enables

`tools/arm_program_routing.py` decodes scene I and II of a `.ns4p` (section enable bit and layer mask of the organ, piano and synth sections), writes `(enable, layer mask, focus 0)` into the section table `[[0xC055B144]] + 0x288` (view and scene A), lets the layer enable handlers (`Edt_OrgCom_kLayerEnableOrg_I` `0xAE`, ..., [keydown-zones](keydown-zones.md) section 2.2) run for every layer, and reads the zone byte back.

First the truth table with synthetic views (one dispatch per layer, layers of one section):

| view (enable, mask `CBA`) | organ A B | piano A B | synth A B C |
|---|---|---|---|
| (1, 001) | 1 0 | 1 0 | 1 0 0 |
| (1, 010) | 0 1 | 0 1 | 0 1 0 |
| (1, 011) | 1 1 | 1 1 | 1 1 0 |
| (1, 100) | 0 0 | 0 0 | 0 0 1 |
| (1, 111) | 1 1 | 1 1 | 1 1 1 |
| (0, 011) | 0 0 | 0 0 | 0 0 0 |
| (0, 111) | 0 0 | 0 0 | 0 0 0 |

The dispatched value of the layer enable id (0..3) changes nothing: the byte depends only on the section view. So **zone byte of layer `i` = section enable AND bit `i` of the layer mask** (VERIFIED, run).

Then decoded factory programs, both scenes, three sections each (`python tools/arm_program_routing.py ... <programs>`): `B3 Chords n Solo`, `B3 Jazz Split`, `B3 Old Forest`, `Astoria Grand`, `12dB Sweep`: 30 section rows, **0 mismatches** between the decode (`scene_1_mask` / `scene_2_mask` rule of the engine) and the ARM zone bytes. For `B3 Chords n Solo` scene I the organ zones read `[1, 1]` (A and B on), scene II everything 0. What is not covered: the writer that fills the section table from the program at load time is not run (the harness leaves the table to the test), so the file bit to table field mapping is by name (`SectionEnable_I` `[Org, Pno, Syn]`, `Edt_*Com_kLayerEnable*_I`), not by execution.

## 3. The zone code of a layer

Each layer block starts `tag (8 bits)`, then the firmware field-name table of the layer buffers lists `KbZone, OctaveShift, ...` ([synth-0x2f-ns4decode-crosscheck](synth-0x2f-ns4decode-crosscheck.md): `Edt_Syn_kKbZone` at `+58`, 4 bits, the octave right after it at `+62`). The organ and piano layers have the same chain with the octave at `+12` (the decoded octave field), so `kKbZone` is the 4 bits at `+8` (VERIFIED order, EMPIRICAL offset). `Edt_*_kKbZone` has range `0..11`, default 5 (`arm-edit-params.csv`).

Values over the 384 factory programs (count of layers):

| Layer | codes found |
|---|---|
| Organ A | 0: 10, 5: 234, 6: 5, 9: 133, 10: 2 |
| Organ B | 0: 7, 5: 238, 6: 6, 9: 133 |
| Piano A / B | 5: 251 / 249, 9: 126 / 132, 0, 4, 6 rare |
| Synth A / B / C | 5: 235 / 238 / 236, 9: 115 / 114 / 119, 0: 9 / 14 / 10, 1, 2, 3, 4, 6, 10 rare |

Only 5 and 9 occur on most layers (programs without a split); the others occur in programs whose `split_enabled` is set (48 programs). In the 2-zone programs (only the Mid point flag set) the pairs are always `0` / `6`: `B3 Chords n Solo` A 0 B 6, `B3 Jazz Split` A 0 B 6, `B3 Bassline` A 0 B 6, `B3 Praise Split` A 6 B 0, `B3 Praise Bass` A 6 B 0, `Quintet Bass Spl` synth A 6 B 0, `Funk Hit Split` synth A 6 B 0. Reading `0` = the lower region and `6` = the upper region fits the program names (the bass part of a bass split is `0`) and the hardware clip. Codes `1`, `2`, `3` appear as single zones of 3 and 4 zone splits (`Choral Pad`, `Silver Rush`, `Guitar Concerto`), `4` as another lower half (`Night Gaunts Whl`, `Horizon Whl`: synth A 4, B and C 6).

The engine reads them as: `0..3` = LED zone 1..4, `4` = zones 1-2, `6` = zones 3-4, `7` = zones 1-3, `8` = zones 2-4, `5`, `9`, `10`, `11` = all four. Only `0`, `4` and `6` are tied to hardware evidence (the clip); `1`, `2`, `3` are plausible, `5` as "all" is the default, `7`, `8`, `10`, `11` are unobserved or ambiguous (**ASSUMED**). The firmware writer of the key range from the code was not found: the setters `0xC00C843C` (set range) and `0xC00C83FC` (reset range, `0 .. 0x7F`) are vtable slots 2 and 3 of the layer classes (vtables at `0xC0279510`, `0xC027A8BC`, `0xC027AD50`, `0xC027AE3C`, `0xC02B7CFC`), without direct callers; no static `bl` reaches them (OQ23).

## 4. Engine

`engine/routing.rs`: `FirmwareRouting::{Off, On}`, `ProgramRouting` (exact scene masks, legacy masks, codes, point flags), `zone_leds(code)`. `apply_program` records the routing facts of the program; `set_firmware_routing` installs the legacy or the exact scene masks, so the switch can be set before or after the program is applied. `handle_midi` asks `keybed_gain(layer, note)`: `Off` the old `KeybedSplitRouter::note_gain`, `On` the region weights of section 6 of [morph-and-zones](../specs/morph-and-zones.md). The decode reads the seven codes into `GlobalProgramDecoded::kb_zone` (read-only, lossless pack). Units: 13 new tests (code table, Mid-only halves, joined zones with an inactive Low point, crossfade unity sum, no split, scene II, mode swap, before / after order, direct layer channel, `kb_zone` decode, flag parsing).

Layers sound only in their zone (bit-identical self-consistency check: the render of the whole program with `--firmware-routing` equals the legacy render of the same program with the layers outside the played zone switched off by editing `layer_*_enabled`; SHA-256 prefixes of the WAVs, `ns4 render-patch --notes N`, velocity 100):

| Program (codes) | Note | Sounding layers | Routing render = render with the others disabled |
|---|---|---|---|
| `B3 Chords n Solo` (A 0, B 6, Mid 65) | 44 | A | identical (`5452c4bfdd4f`; whole program, both layers: `093a1d27fb94`) |
| | 84 | B | identical (`c09d4949c092`; both layers: `725951ffdfaf`) |
| `B3 Praise Split` (A 6, B 0, Mid 60) | 44 | B | identical (`ea1b7e2f44fa`) |
| | 84 | A | identical (`4017618e85db`) |
| `Funk Hit Split` (synth A 6, B 0, Mid 65) | 44 | B | identical (`b8c774adb00d`) |
| | 84 | A | identical (`86b63ea59fd0`) |

This proves the engine implements the rule, not that the rule is the hardware's (section 3).

## 5. Clip comparison with the routing on

`tools/emu_vs_hw.py --confirmed-only` (same clips, notes and 100 velocity as [emulator-vs-hardware-audio](emulator-vs-hardware-audio.md); variants `routing` = `--firmware-routing`, `b3wheels+routing` = `--tone-source b3wheels --firmware-routing`; new rows in `emu_vs_hw_segments.json`, raw rows in [emu_vs_hw_routing.json](data/emu_vs_hw_routing.json)). Emulator minus hardware, dB re RMS (RMS in dB):

| Segment | Variant | sub | low | mid | pres | brill | RMS | score |
|---|---|---|---|---|---|---|---|---|
| organ_b3 | default | +17.5 | -0.2 | +0.3 | -5.7 | +0.5 | +14.9 | 4.83 |
| organ_b3 | routing | **-1.2** | -0.4 | +3.5 | -11.0 | -9.0 | +11.5 | 5.01 |
| organ_b3 | b3wheels | +16.5 | +1.2 | -1.1 | -6.7 | -2.2 | +8.0 | 5.54 |
| organ_b3 | **b3wheels+routing** | **-1.6** | +1.1 | +2.2 | -12.5 | -4.7 | **+3.8** | **4.42** |
| organ_vox / organ_pipe / piano_dry / synth_lead / fx_amp | routing | identical to default (every row, 13 columns) | | | | | | |

Level and crest of the B3 clip (mono mix RMS, peak over both channels, share of samples at full scale; hardware RMS -21.4 dBFS, peak -2.9 dBFS):

| Variant | RMS dBFS | crest dB | full scale |
|---|---|---|---|
| hardware | -21.4 | 18.6 | 0 % |
| default | -5.9 | 5.9 | 15.4 % |
| routing | -9.5 | 9.5 | 0.72 % |
| b3wheels | -12.8 | 12.8 | 0.30 % |
| **b3wheels+routing** | **-17.2** | **17.2** | **0 %** |

What it says:

- The sub excess is the layer split. Of the 101 confirmed notes 97 lie below 65 (39..62, mostly 43..47) and only 4 at or above (66, 79, 79, 91), so on the hardware the clip is layer A (zone 0, octave +2) alone; layer B (16' bar at 8, octave 0) belongs to the keys from 65. With the routing the sub falls from +16.5 / +17.5 to -1.6 / -1.2 dB, the RMS gap from +8.0 to +3.8 dB (b3wheels) and the crest from 12.8 to 17.2 dB (hardware 18.6), the full-scale share from 0.3 % to 0. This is the PR #194 result (layer B off: sub -1.4, RMS +3.6, crest within 0.5 dB with its own crest measure) reached by the program's own fields instead of by switching a layer off.
- The cause named in PR #194 ("layer B belongs to section II, which is off") is not what the firmware does (section 2); the zone code is. The note "notes below 52 dropped gives mid within 0.6 dB" is the same thing seen from the other side: the low notes are the part that only layer A plays, at octave +2 and 8 0 8 0 2 bars.
- Presence and brilliance stay dark (-12.5 / -4.7 dB with `b3wheels`, -11.0 / -9.0 with the Legacy source): this is the open tone / rotary part, not routing. The spectrum score improves for `b3wheels` (5.54 to 4.42, better than the Legacy default 4.83) but not for the Legacy source (4.83 to 5.01), because removing layer B takes away the only energy that was filling presence and brilliance there.
- The other five segments are single-layer or unsplit sections: the routing leaves them identical (the legacy decode already gave the same layers there), so nothing regressed.

## 6. Decision (D22)

| Criterion | Status |
|---|---|
| (1) algorithm verified | the rule (enable AND mask) is VERIFIED on the ARM; the region weights are the existing crossfade law (ASSUMED shape) |
| (2) mapping verified, not assumed | **not met**: the code to zone-set table is EMPIRICAL (clip plus factory names), the firmware key-range writer is not found |
| (3) re-baseline `render_hash` | not needed: default stays `Off`, hash unchanged |

`FirmwareRouting` therefore stays **opt-in** and is not part of `--firmware-all`. To flip the default: find the writer of the zone key range (OQ23) or a second clip of a split program (a `0` / `6` or `1` / `2` / `3` program with hardware audio), then re-run section 5 and re-baseline.

## Implications

- The B3 clip gap that looked like an output stage problem is the layer zone: with the routing the sub, the crest and the clipping match the hardware and the RMS is within 4 dB. `B3Wheels` (+ tail) with `--firmware-routing` is now the closest B3 path; the remaining B3 gap is presence / brilliance (-12.5 / -4.7 dB) and the level law `L` (OQ19).
- Factory programs that split (48 of 384) now sound their layers only in their zone with `--firmware-routing`; programs that do not split are unchanged.
- The legacy empty-mask fallback (`apply_program`) hid programs whose scene masks are all zero; with the routing on they are silent as on the hardware.

## Follow-ups

1. Find the writer of the zone key range from `kKbZone` (vtable slot 2 of the layer classes, `0xC00C843C`, no static callers; likely the program load or the KB ZONE button path) and replace the ASSUMED table by the firmware's.
2. Run the program load path of the firmware in the harness (section table filled by the load, not by the test) so the file bit to table field mapping is executed, not by name.
3. Octave range limit of a zone, Live-mode scene focus, `Edt_Sys_kMIDIOrgSectionEnable` (MIDI section enables), the virtual channel map (OQ23).
4. A second hardware clip of a split program to confirm the codes `1`, `2`, `3` and `4`.

## Reproduce

```bash
cargo build --release -p nord-core --bin ns4
uv run --with unicorn python tools/arm_program_routing.py target/fw_unpacked/bank0_CODE.bin target/fw_unpacked/bank0_SRAM.bin --cache target/keydown.snap "factory_programs/Program/Bank A/B3 Chords n Solo.ns4p" "factory_programs/Program/Bank A/B3 Jazz Split.ns4p" "factory_programs/Program/Bank A/B3 Old Forest.ns4p" "factory_programs/Program/Bank A/Astoria Grand.ns4p" "factory_programs/Program/Bank A/12dB Sweep.ns4p"
uv run python tools/emu_vs_hw.py --video <part A webm> --backup <factory .ns4b> --workdir <scratch> --confirmed-only --only organ_b3
ns4 render-patch "Program/Bank A/B3 Chords n Solo.ns4p" --backup <factory .ns4b> --notes 44 --firmware-routing --wav out.wav
```
