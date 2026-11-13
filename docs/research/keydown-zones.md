# Layer zones, virtual channels and the organ / synth key messages

Checked: 2026-11-13.

Sources: `target/fw_unpacked/bank0_CODE.bin` and `bank0_SRAM.bin` (firmware v1.66, local only); `tools/arm_zones.py`, `tools/arm_zone_capture.py` (new), `arm_keydown.py` (PR #133), `arm_model_paths.py`, `arm_edit_capture.py`; [keydown-paths](keydown-paths.md); [pno-full-pass](pno-full-pass.md) section 6; [organ-weights](organ-weights.md); [b3-tone-source](b3-tone-source.md); [synth-voice-control](synth-voice-control.md). Only addresses, offsets, derived values and message words are recorded; no firmware bytes.

Question: why did a note-on on MIDI channel 1 run only the piano key handler, which edit state makes the organ (`0xC00BF890`) and synth (`0xC01EC690`) key handlers run, and what DSP messages do they send for key-down and key-up (organ models B3, Vox, Pipe; synth)?

Tables: [keydown-organ-keys.csv](data/keydown-organ-keys.csv), [keydown-organ-models.csv](data/keydown-organ-models.csv), [keydown-synth-keys.csv](data/keydown-synth-keys.csv), [keydown-vox-flags.csv](data/keydown-vox-flags.csv), [keydown-velocity-curve.csv](data/keydown-velocity-curve.csv); replay scripts `data/keydown-{b3,vox,pipe,synth}-c4e4g4.script`.

## 1. Answer in short

| Question | Answer | Status |
|---|---|---|
| Why did only the piano run? | Three gates. (a) The mask test of the loop `0xC00D1EBC` passes for **every** layer client (only the non-layer client `0x90F92A90` fails). (b) The shared layer virtual call `0xC00C8740` -> `0xC00C859C` returns at once unless the **zone enable byte** of the layer is non-zero; only the piano zone byte was 1 (`0xC00C82D4` writes it). (c) Channel 1 is the only MIDI channel with a virtual channel in the channel map; all others are dropped before the mask test. The earlier note ("only the piano passes the 64-bit mask") was wrong: the piano was the only layer with an enabled zone | VERIFIED (run) |
| Which edit state enables the organ and synth zones? | The program **section table** `[[0xC055B144]] + 0x288`: three sections (organ, piano, synth) of `0x28` bytes, `(enable, layer mask, focus)` view at `+0` and scene A at `+0xC`. The harness leaves organ and synth at 0 (a program load fills them; no edit id writes the table: 400 ids x 2 values on layers 0, 4 wrote nothing there). With `(1, 3, 0)` organ / `(1, 7, 0)` synth the layer enable handlers (`Edt_OrgCom_kLayerEnableOrg_I` `0xAE`, `Edt_SynCom_kLayerEnableSyn_I` `0xA1`, ... dispatched through `arm_edit_capture.apply_value`) write zone byte 1 for organ A, B and synth A, B, C | VERIFIED (run) |
| Do other MIDI channels reach other layers? | Yes, through the **virtual channel**: `v = [manager + 0x3C + 4 (channel - 1)]`; client masks accept `v` = 1 (global), 6 / 7 / 8 (organ / piano / synth section), 9 .. 15 (organ A B, piano A B, synth A B C) and 16. The map is not written by any `Edt_Sys_kMIDI*` id in the harness (set to default `{ch1: 1, ch16: 16, index 16: 2}`); poking it routes channel 5 -> organ A, 6 -> synth A, 7 -> piano A and the key handlers follow | VERIFIED (run) |
| What do the key handlers send? | B3 54 messages per key-down, Pipe 27, Vox 4 (over 45 ticks), Farfisa 6; synth one message to PNO core 2 per key; the organ messages carry no velocity. Sections 4 to 7 | VERIFIED (run) |
| Vox key to oscillator map | `row = fold(note - 36 + I[slot])`, 20 footage slots, 96 rows; the 8' slot is row `note - 24`, so the ASSUMED `note - 24` of [pno-full-pass](pno-full-pass.md) is **correct** for the 8' footage | VERIFIED (run, 24 notes) |

## 2. The three gates (disassembly and runs)

### 2.1 Mask test

| Item | Value | Tag |
|---|---|---|
| Event descriptor | `0xC0571CAC {kind, class, aux, channel}`; the note-on handler `0xC00D1F48` writes `{3, 9, 0, v}` (`0xC00D2040..0xC00D2054`), `v` = virtual channel | VERIFIED (disassembly, write watch) |
| Event mask | `bit(22 + kind) \| bit(51 + class) \| bit(16 + aux) \| bit(27 + v)`; `{3, 9, 0, 1}` = `0x1000000012010000` | VERIFIED (6 observed masks, `test_event_mask_matches_the_masks_observed_in_the_run`) |
| Accept | `event & [client + 0x28] == event` (64-bit `ldrd`, `0xC00D1EC0..0xC00D1EE0`), then the virtual call `[vtable + 0x1C]` | VERIFIED (disassembly) |
| Client masks | organ A `0xFFF8081217FFFFFF`, B `...2217...`, piano A `0xFFF8084417FFFFFF`, B `...8417...`, synth A `0xFFF8090817FFFFFF`, B `...A0817...`, C `...C0817...`; the three `ext` clients (Edt_Sys / Edt_Ext layers) carry the synth masks; client `0x90F92A90` (vtable `0xC02B79B0`) has `0x10000000113FFFFF` and never accepts a note | VERIFIED (run, `arm_zones.MaskLog`) |
| Channels per client | organ A `{1, 6, 9, 16}`, organ B `{1, 6, 10, 16}`, piano A `{1, 7, 11, 16}`, B `{1, 7, 12, 16}`, synth A `{1, 8, 13, 16}`, B `{1, 8, 14, 16}`, C `{1, 8, 15, 16}` (bit `27 + v` of the high word) | VERIFIED (decoded masks) |

### 2.2 Zone object and the enable byte

| Item | Value | Tag |
|---|---|---|
| Virtual call | all layer classes share vtable slot 7 = `0xC00C8740`; the event type word `[evt] - 0x21` indexes a jump table: `0x27` (note-on) -> `0xC00C884C` -> `0xC00C859C(client, descriptor, slot, note, ...)` | VERIFIED (disassembly, run) |
| Zone pointer | `[client + 4 (0x16C0 + slot + 12 class) + 4]`, `slot = [client + 0x5AB4]` (organ / piano A 0, B 2; synth A 0, B 2, C 4), `class` 1 = note keys (`[descriptor + 0xC]`) | VERIFIED (run) |
| Zone fields | `+0` enable byte; `+4 - +8` first key and `+0x14 + +0x18` last key (21 and 108 on every layer, unchanged by all 12 `Edt_*_kKbZone` values, see section 8); `+0x30 + 4 note` key state (`-1` = up). `0xC00C859C` calls the layer key object (`[client + 0x5000 + 0x6C8 + 4 slot]`, virtual `+4`) only when the note is inside the range and the enable byte is non-zero; a second note-on of a held key is dropped | VERIFIED (run, disassembly) |
| Writer | `0xC00C82D4(this, 0, slot, class, enable)` (slot 0 of the second vtable of each layer class, pools `0xC0279510`, `0xC027A8BC`, `0xC027AD50`, `0xC027AE3C`, `0xC02B7CFC`); the layer enable handlers call it for classes 1, 6, 9, 16 (organ), 1, 7, 11, 16 (piano), 1, 8, 13, 16 (synth) | VERIFIED (run) |
| Enable value | the organ handler (`0xC0120100..0xC0120248`) reads the setup object (`[[0xC055B144]] + 0x508`, `+0x288`, `+0x294`, `[0xC055B144] + 4`) and passes the result as the fifth argument of `0xC00C82D4`; with the section view byte `[setup + 0x288] = 1` the organ A byte becomes 1, with `[setup + 0x2D8] = 1` the synth A byte (single-byte pokes, scan over `+0x288..+0x2DF`: only these two bytes flip the result) | EMPIRICAL (the exact boolean expression was not decoded) |
| Section table | `0xC00F8A50` (called with the setup object when `[setup + 0x508]` is 0) rebuilds the view `+0` from scene A `+0xC` (or scene B `+0x18` when `[setup + 0x314] == 1`); boot writes only piano (`0xC00F8AA8`: `[0x90D4FDB0] = 1`). Layout per section `(enable, layer mask, focus)` | VERIFIED (write watch over the boot) |

`arm_zones.enable_sections(emu)` sets organ `(1, 3, 0)`, piano `(1, 3, 0)`, synth `(1, 7, 0)` in view and scene A, then dispatches the layer enable ids on the stores of layers 0..6; zones of layers 0..6 read 1 afterwards and a note-on on channel 1 calls `0xC00BF890` twice (organ A, B), `0xC01EC690` three times (synth A, B, C) and `0xC00BC8AC` twice (piano A, B).

### 2.3 Virtual channel map

| Item | Value | Tag |
|---|---|---|
| Handler | `0xC00D1F48(manager, record)`: `v = [manager + 0x3C + 4 [record + 8]]` (`[record + 8]` = MIDI channel index, `[record + 0xC]` note, `[record + 0x10]` velocity); `v == 0` posts event class 4 (`0xC057605C`) and stops. Otherwise the note is shifted by the channel profile `[manager + 0xCC + 4 v]` (`+8` = transpose, result above 127 drops) and the velocity goes through the curve table | VERIFIED (disassembly, run) |
| Default map | `[1, 0 x 14, 16, 2]` (17 words at `manager + 0x3C`, manager `0x90F8CB10`): channel 1 -> global 1, channel 16 -> 16, index 16 -> 2 | VERIFIED (run) |
| Routing run | map poked `ch5 -> 9, ch6 -> 13, ch7 -> 11, ch8 -> 6, ch9 -> 7, ch10 -> 8`: handlers called per channel: ch5 organ, ch6 synth, ch7 piano, ch8 organ, ch9 piano, ch10 synth; `v = 16` and `v = 2` call none | VERIFIED (run) |
| Map writer | not found: `Edt_Sys_kMIDIOrgA` `0x61`, `kMIDIOrgB` `0x62`, `kMIDIPnoA/B` `0x64/0x65`, `kMIDISynA/B/C` `0x67..0x69`, `kMIDIExtA/B` `0x6B/0x6C`, `kMIDIGlobal` `0x5F` and the section enables `0x63/0x66/0x6A` dispatched with values 1 to 11 on the first store of layers 7, 8, 9 (the owners of the `Edt_Sys` ids) wrote nothing into the map (write watch over the 17 words). The writer runs in a path the harness does not reach (UI or MIDI setup apply) | open |

The `Edt_Sys_kMIDIExtA..C` defaults 13 / 14 / 15 equal the virtual channels 13..15 of synth A..C, which hints that the virtual channel numbers are the user channel numbers of the section / layer / ext destinations (ASSUMED).

### 2.4 Velocity

`0xC00D1F48` replaces the MIDI velocity by a signed 16-bit table entry `[0xC02B3128 + 0x18 + 2 (128 curve + vel)]`: nine curves 0..8; the curve is `[manager + 0x134]` (6 by default; the two channel profile types 1 and 2 use curves 2 and 1). Velocity 100 gives 416 (`0x1A0`), 30 gives 87, 127 gives 656 (observed as stack argument of both key handlers). Full table: [keydown-velocity-curve.csv](data/keydown-velocity-curve.csv) (derived values, 9 x 127). The selector source is open.

## 3. Capture method

`tools/arm_zone_capture.py` boots the settled state with the board-present flag and UART2 model of `arm_keydown.py`, enables the sections (section 2.2), freezes a snapshot, and for every point restores it, isolates one layer family (`arm_zones.force_zone` switches the other zones off), installs the organ model (`arm_model_paths.install_model`, `Edt_Org_kModel`), sends `9n kk vv`, runs 70 ticks (60 for the synth; 1 tick = 200 000 ARM instructions, ASSUMED 1 ms), sends the note-off, runs 70 ticks. The CSVs hold `kind,model,note,velocity,phase,tick,chip,core,path,dm_word,value` (derived values only). Points: 24 notes (21..108) at velocity 100, velocities 1, 32, 64, 127 at note 60.

Two traps found on the way (both recorded so that nobody repeats them):

- **Organ layer B writes the same words.** With both organ layers enabled every B3 burst appears twice (layer A with the installed model, layer B with its own default model B3), which looked like "two values per wheel". The committed tables are for layer A alone; [keydown-organ-b.csv](data/keydown-organ-b.csv) holds layer B (model B3, five points).
- **The 3-word idle burst** `DM 0xB2F26 = 0x26000`, `0xB2FA1 = 0x15D00`, `0xB2F25 = 0x22B00` (PNO core 1, every ~28 M instructions, [arm-organ-messages](arm-organ-messages.md) section 10) lands in some windows; it is not caused by a key.

## 4. Velocity

| Check | Result | Tag |
|---|---|---|
| Organ B3, Vox, Pipe1: messages for velocity 1, 32, 64, 127 against 100 (note 60, tick, DM word, value, both phases) | **identical** for all three models (102, 11, 57 messages) | VERIFIED (run) |
| Organ velocity argument | `0xC00BF890` receives the curve value (416 for velocity 100) like the synth event, but no sender uses it: the organ weights carry no velocity (the earlier `a / b / wB` formula with `(vel * 127) >> 4` of [organ-weights](organ-weights.md) section 2 is the bank engine key-down called directly with velocity; the real key path does not scale) | VERIFIED (negative, run) |
| Synth voice word | the velocity reaches the DSP: low 16 bits of the message (section 7) | VERIFIED (run) |

## 5. Vox (model 1): key to oscillator map and level slots

The Vox engine (`0x90217350`, vtable `0xC0526664`) keeps a key flag table `[engine + 0x294 + 0x14 row + slot]` (96 rows x 20 slots), 20 level slots `[engine + 0xD18 + 8 slot]` (23-bit fixed point, weight = value / 2^23) and taper words `[engine + 0xA14 + 8 row]` (the number of flagged slots of the row, 1..3). A key-down sets one flag per slot (ticks right after the note); the weights leave through a queue of 8 entries (`[engine + 0xE00 + 0x14 k]`: flag, DM word base, value, record index) that the drain `0xC0224360` empties, in three bursts (ticks 3, 17, 47 after the key-down in the CSV). The sender `0xC0224468` of [pno-full-pass](pno-full-pass.md) section 6 is reached through this queue (drain entered from the SRAM veneer `0x80000360`).

| Item | Result | Tag |
|---|---|---|
| Flag rows set by a key-down | `row = fold(note - 36 + I[slot])`, `I = [0, 7, 12, 19, 19, 24, 24, 28, 31, 31, 36, 36, 36, 40, 40, 43, 48, 48, 52, 60]`; `fold` adds 12 while `row < 0` and subtracts 12 while `row >= 96`. All 20 flags for each of 24 notes (21..108) match | VERIFIED (run, [keydown-vox-flags.csv](data/keydown-vox-flags.csv), 24 notes x 20 slots) |
| Footage reading | with row `r` = oscillator of pitch `24 + r` (ASSUMED, [pno-full-pass](pno-full-pass.md)), the 20 slots are the semitone offsets `-12, -5, 0, +7, +7, +12, +12, +16, +19, +19, +24, +24, +24, +28, +28, +31, +36, +36, +40, +48` of a Vox-style footage ladder (16', 5 1/3', 8', 2 2/3' x2, 4' x2, 3 1/5', 2' ... 1/2'); slot 2 (8') lands on row `note - 24`, so the **`note - 24` map of the Vox chord replay is correct** | VERIFIED (flags) / ASSUMED (pitch of row `r`) |
| Level slots in the default program | slots 0, 2, 5, 10 hold `2076180` (= `0x1FA814`, 0.24748), the other 16 hold 0 | VERIFIED (run) |
| Weight message | `DM(0x95E46 + row) = float(sum of level slots of the flagged slots of the row)`: `0x3E7D70A0` (0.24748) for one slot, `0x3EFD70A0` (0.49496) when two slots fold onto the row, `0x3F3E1478` for three. Key-up sends 0 to the same rows. Rows reached by the four level slots: `fold(note - 36 + I[j])`, j = 0, 2, 5, 10 (16', 8', 4', 2'), 24 of 24 notes match for key-down and key-up | VERIFIED (run, [keydown-organ-keys.csv](data/keydown-organ-keys.csv), model 1) |
| Timing | key-down: 1 row at tick 3, 2 rows at tick 17, 1 row at tick 47; key-up: ticks 1, 7, 37, then the idle burst at 52. Ticks are ASSUMED 1 ms | EMPIRICAL |
| Other messages | PNO core 2 has none for the organ; the key-up of Vox shows the idle burst at tick 52 | VERIFIED (run) |

## 6. Pipe (models 3, 4) and B3 (model 0)

### 6.1 Pipe1 and Pipe2

A key-down sends 27 messages at tick 3 (9 wheels x `a`, `b`, `wB`), a key-up 30: the same 27 (restoring `a`, `b` and zeroing `wB`) plus the idle burst. Pipe2 (model 4) is identical for note 60 (27 on, 30 off).

| Item | Result | Tag |
|---|---|---|
| Wheel set | `wheel = fold(note - 24 + off[j])`, `off = [-12, 7, 0, 12, 19, 24, 28, 31, 36]`, `fold` into 0..90 by octaves; DM words `0x9594F + wheel` (`a`), `0x95AE2 + wheel` (`b`), `0x957DE + wheel` (`wB`). Matches the organ-weights map for all 24 notes | VERIFIED (run, 24 notes) |
| Values | `a = 0x3C1DF380` (0.009641), `b = 0x3F7D8832` (0.990359), `wB = 0x3C7E0000` (0.015503) for one drawbar on the wheel, `0x3CFE0000` for two, `0x3D3E8000` for three when high notes fold several drawbars onto one wheel. Key-up: `a = 0x3B8BF800`, `b = 0x3F7EE810`, `wB = 0` | VERIFIED (run) |
| Velocity | none (section 4) | VERIFIED |

### 6.2 B3 (model 0, layer A)

54 messages at tick 3 for a key-down, 48 for a key-up (tick 1 and the idle burst at 52). All values are independent of the velocity. The 24-note sweep gives:

| Group | DM words | Count | Result | Tag |
|---|---|---|---|---|
| Wheel weights `W` | `0x95116 + wheel` (`0x95122 .. 0x9516E`) | 9 (one per drawbar `j`) | `wheel(j) = fold(note - 24 + off[j])` with `off = [-12, 7, 0, 12, 19, 24, 28, 31, 36]` and `fold` into **12 .. 88** by octaves (below 12 up, above 88 down). The multiset of written wheels equals the multiset of the nine `wheel(j)` for all 24 notes (several drawbars fold onto one wheel at the keyboard ends, e.g. wheel 12 three times for note 24, wheel 21 four times for note 21) | VERIFIED (run, 24 notes) |
| Values | same words | 9 | the **first** write to a wheel is a constant per wheel (44 wheels, [keydown-b3-wheels.csv](data/keydown-b3-wheels.csv): 0.2824 at wheel 37 .. 0.7214 at wheel 83; wheel 12 = 0.5556 = 5/9). Further writes to the same wheel in the same burst step down (note 24, wheel 12: 0.5556, 0.5328, 0.5179; wheel 19: 0.4098, 0.3908) while the three words `0x951F1 / F5 / FA` step up (0.016, 0.0378, 0.0468). No velocity | EMPIRICAL (values), VERIFIED (first writes constant over notes) |
| Key-specific weights | `0x951FD`, `0x95209`, `0x95210`, `0x95215` (note 60), `0xB44C9` | 3 to 4 | `0xB44C9` = 0.0169 (notes 21, 36), 0.0334 (60), 0.0395 (84), 0.0455 (108); role open (key click / percussion level?) | EMPIRICAL |
| Voice packets | `0x954E6 .. 0x954FF`, `0x95500 .. 0x955A3`, `0xB4246 .. 0xB425D` | 4 x 12 | 4 oscillator-voice records per key: constant record pointer words (`0xB33C2`, `0xB45AE`), oscillator index words `0x954EC / ED / F6 / F7 = 0xB3300 + k` (k = 31 and 24 for note 60), pitch-dependent words `0x954EA / EB / F4 / F5` (e.g. `0xBD16FDA0` at note 60) and gain bytes `0xB4246 .. = 0x7F00, 0x7F0000, 0x7F, 0x7F00` | EMPIRICAL (28 constant, 9 note-dependent words) |
| PNO core 2 | none | 0 | the organ sends nothing to core 2 | VERIFIED |

The `W(key, footage)` of the task: the B3 weight written for a key and footage `j` is the **per-wheel constant** of [keydown-b3-wheels.csv](data/keydown-b3-wheels.csv) at `wheel(j)` (same map as Pipe, folded into 12 .. 88). It is not scaled by velocity. The table belongs to the default program (drawbars of the settled state); ASSUMED to change with the drawbar edit ids, which were not swept here.

### 6.3 Other models (survey: note 36, 60, 84, velocity 100 and 32 / 127)

| Model | On | Off | What | Tag |
|---|---|---|---|---|
| 2 Farfisa | 6 | 9 | `DM(0x95FE4 + note) = 0x3CAAAA80` (0.02083) at tick 3, `+ 0x62` at tick 17, `+ 0xD0` at tick 47, each with a companion `DM(0xC2D58 / 59 / 5C) = 0xBC1D5580`; key-up zeroes them in reverse order (a fourth row `+ 0x135` at tick 1, zeroed only); no velocity | VERIFIED (run, 3 notes) |
| 4 Pipe2 | 27 | 30 | same words as Pipe1 | VERIFIED (run) |
| 5 B3 Bass | 98 | 74 | B3 words plus the bass wheels (regions `0x951 / 0x952 / 0x954 / 0x955 / 0xB42 / 0xB44`) | EMPIRICAL (not decoded) |

## 7. Synth voice event (`0xC01EC690`)

One message per key to PNO core 2, no other effect (the idle burst of section 3 appears now and then):

`DM(0xC1DCC + 3 voice + layer) = (layer << 29) | (0x0B on / 0x08 off) << 24 | ((note - 60) & 0xFF) << 16 | 0x80xx00`

| Field | Value | Tag |
|---|---|---|
| Word | `0xC1DCC + 3 v + L`: `L` = layer 0 / 1 / 2 (synth A / B / C), `v` = voice index in order of allocation (C4 E4 G4 chord: v = 0, 1, 2 at words `0xC1DCC / CF / D2` for A) | VERIFIED (chord run [keydown-synth-c4e4g4.script](data/keydown-synth-c4e4g4.script), 3 layers x 3 notes) |
| Byte 3 | `0x0B` key-down, `0x08` key-up, plus `L << 5` (`0x2B` for B, `0x4B` for C) | VERIFIED (run) |
| Byte 2 | `note - 60` as a signed byte (`0xD9` for 21, `0xE8` for 36, `0x00` for 60, `0x18` for 84, `0x30` for 108) | VERIFIED (24 notes x 3 layers, [keydown-synth-keys.csv](data/keydown-synth-keys.csv)) |
| Byte 1 | `0x80 + g(velocity)`, monotone from `0x80` (velocity 1) to `0xFF` (velocity 118 and up): `g` = 0, 3, 8, 17, 32, 58, 98, 127 at velocity 1, 16, 32, 48, 64, 80, 100, 127; 85 distinct values over 127 velocities ([keydown-synth-velocity.csv](data/keydown-synth-velocity.csv)). Not proportional to any of the 9 curves of section 2.4 (max deviation of a scaled curve 32 steps). `g` is a function of the **curve 6 value** `c` (section 2.4): `g = floor(127 s(c / 600))`, `s(x) = 3 x^2 - 2 x^3` (smoothstep, saturates at `c = 600`): 97 of 127 exact, max error 1 (`python tools/synth_velocity.py --check`; Rust table `synth/key_velocity.rs`) | VERIFIED (table), EMPIRICAL (S-curve fit, DSP-side use of `g` not followed) |
| Byte 0 | `0x00` | VERIFIED |
| Key-up | same word with `0x08`, same bytes 2 and 1 | VERIFIED |
| Layer B / C | same format; the three layers send their own word | VERIFIED |

The voice event record handed to the engine is `(engine 0x90673190, descriptor 0xC0571CAC, record 0x700EE5C0 on the stack, type 7)` with the curve value as stack argument; the DSP message above is the only downstream effect in 60 ticks.

## 8. What stays open

| Item | State |
|---|---|
| Piano | `[core + 4]` is still 0 ([keydown-paths](keydown-paths.md) section 4): with the zone enabled the key object returns before the voice. No piano message |
| Virtual channel map writer | not found; the `Edt_Sys_kMIDI*` ids do not write it in the harness. Virtual channel 16 (all) passes the mask test of every layer but no key handler is called for it (not decoded) |
| Zone key range vs `Edt_*_kKbZone` | all 12 zone values and the split ids (`0xE7..0xF1`) leave the range at 21 .. 108; the range is written by `0xC00C84A4` from constants, the zone setters `0xC00C843C / 0xC00C83FC` are not reached by the id dispatch |
| Exact enable expression | section 2.2 (`0xC0120100..0xC0120248`) |
| Velocity curve selector `[manager + 0x134]` | 6 by default; writer not found |
| Synth byte 1 formula | table only |
| B3 value table vs drawbar state; the multi-write ramp of low wheels; voice packet fields | EMPIRICAL |
| Timing of the Vox queue | ticks are ASSUMED 1 ms |

## 9. Reproduce

```bash
C=target/fw_unpacked
uv run --with unicorn python tools/arm_zones.py $C/bank0_CODE.bin $C/bank0_SRAM.bin --cache target/keydown.snap
uv run --with unicorn python tools/arm_zone_capture.py organ  $C/bank0_CODE.bin $C/bank0_SRAM.bin --cache target/keydown.snap --models 0 1 3 --csv docs/research/data/keydown-organ-keys.csv
uv run --with unicorn python tools/arm_zone_capture.py organ  ... --models 2 4 5 --short --csv docs/research/data/keydown-organ-models.csv
uv run --with unicorn python tools/arm_zone_capture.py organ  ... --models 0 --layer 1 --short --csv docs/research/data/keydown-organ-b.csv
uv run --with unicorn python tools/arm_zone_capture.py synth  ... --csv docs/research/data/keydown-synth-keys.csv
uv run --with unicorn python tools/arm_zone_capture.py synthvel ... --csv docs/research/data/keydown-synth-velocity.csv
uv run --with unicorn python tools/arm_zone_capture.py flags  ... --csv docs/research/data/keydown-vox-flags.csv
uv run --with unicorn python tools/arm_zone_capture.py velocity ... --csv docs/research/data/keydown-velocity-curve.csv
uv run --with unicorn python tools/arm_zone_capture.py scripts ... --out docs/research/data
```

`arm_zones.py` prints the zone bytes before and after, the channel map and the mask log for one note-on. The first run builds the `keydown.snap` cache (about 60 s), each organ point then takes 2 to 4 s.

Replay scripts (`sharc-org --msg-script`, `PASS DM VALUE`, `--msg-base 0`): `keydown-{b3,vox,pipe}-c4e4g4.script` (pass 0 = model install burst of `Edt_Org_kModel`, pass 1 = C4 E4 G4 key-down, pass 2 = key-up; PNO core 1) and `keydown-synth-c4e4g4.script` (PNO core 2, 3 layers x 3 notes). They feed the PNO core 1 oracle image with the real key-driven weights; the existing `pno-pass-*.script` files carry the ASSUMED weights that these supersede.

## Provenance summary

| Item | Status |
|---|---|
| Mask test passes for every layer client; zone byte gate; channel map | VERIFIED (run, disassembly) |
| Section table enables the organ and synth zones | VERIFIED (run); exact boolean EMPIRICAL |
| Virtual channel numbers and mask bit `27 + v` | VERIFIED (6 masks); user-channel meaning ASSUMED |
| Vox flag map and weights | VERIFIED (24 notes) |
| Pipe wheel map and values | VERIFIED (24 notes) |
| B3 wheel set | VERIFIED; values and voice packets EMPIRICAL |
| Synth word format | VERIFIED (24 notes x 3 layers, 127 velocities) |
| Velocity independence of the organ | VERIFIED (negative) |

## Implications

- The organ and synth key handlers are now exercised by real MIDI bytes through the real parser, event queue, mask loop and zone object; the Vox weights, the Pipe weights and the synth voice word come from the firmware, not from assumptions.
- The Vox oracle chord script can use `keydown-vox-c4e4g4.script` (rows `note - 36 + I[slot]` with the 8' on row `note - 24`).
- A Rust key path needs: wheel map `fold(note - 24 + off)` for B3 / Pipe (no velocity), the Vox 20-slot ladder, the synth word format.

## Follow-ups

1. Find the writer of the virtual channel map (UI / MIDI setup apply) and test `Edt_Sys_kMIDIOrgA` etc. end to end.
2. Decode the exact zone-enable expression and the `Edt_*_kKbZone` / split zone setters.
3. Sweep the drawbar and level edit ids with a key held to get the B3 value table as a function of the drawbar state and the ramp of low wheels.
4. Piano: load the sound object (`[core + 4]`, [keydown-paths](keydown-paths.md) follow-up 1), now that its key object is reachable.
