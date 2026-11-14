# Synth arpeggiator / gate pattern presets

- **Checked:** 2026-11-14 (first pass 2026-11-13: table not found; second pass: table recovered; third pass: gate lengths, dial value 0, program import, preset-edited marker, section 7; fourth pass: record player search, rate clock, arp mode bits, tie editor, section 8)
- **Sources:**
  - Local, never committed: `.reference/Nord Stage 4 Factory Restore RevK v1.66.ns4b` (384 `.ns4y` synth presets), `target/fw_unpacked/bank0_CODE.bin` and `bank0_SRAM.bin` (OS v1.66), `.reference/ns4decode_pkg/ns4decode_pkg/ns4decode.py` and `ns4maps.py` (third party, MIT), `.reference/Nord Stage 4 User Manual v1.6X-Edition-N.pdf`, `.reference/DSoundman-course/*.srt`.
  - In repo: [arm-edit-params.csv](arm-edit-params.csv), [arp-preset-table.csv](arp-preset-table.csv) (result), `tools/arm_xref.py`, `tools/arm_emu.py`, `tools/ns4_arp_patterns.py`, `tools/ns4_arp_presets.py` (tests in `tools/tests/test_ns4_arp_presets.py`), `tools/ns4_arp_gate.py` (third pass, `clock` command in the fourth, tests in `tools/tests/test_ns4_arp_gate.py`), `tools/ns4_arp_modes.py` (fourth pass, tests in `tools/tests/test_ns4_arp_modes.py`). Local only, fourth pass: `.reference/Nord Stage 4 User Manual v1.6X-Edition-N.pdf` (p.36 arpeggiator rate) and ns4decode `interpretArpRate` / `ns4names.ArpMode` (`.reference/ns4decode_pkg/`).
  - Web (text not copied): https://www.nordkeyboards.com/products/nord-wave-2/specifications/ ("30+ editable presets"), https://www.norduserforum.com/viewtopic.php?t=25610 (search snippet only: the Stage 4 arp pattern "cannot be edited" beyond the factory patterns), a search for the preset names (`Alien Font`, `Ringaracka`, `RWGate`, found by step 2 below) returned no page that lists them.
- **Question:** What are the real factory arpeggiator / gate pattern presets of the Nord Stage 4 synth (the engine's 8 presets in `synth/perf.rs` were `ASSUMED`), and how does a `.ns4y` store a pattern?

## 1. Result

**The table is recovered.** `Edt_Syn_kArpPtnPreset` (id `0x114`, range `0..28`) selects value 0 (default pattern) or one of **28 firmware table entries**. Each entry is a gate word and a pan word of 16 two-bit steps. They are in [arp-preset-table.csv](arp-preset-table.csv) (gate `E q Q .`, pan `C L R`, steps 8 or 16, accent always 0). Reproduce:

```bash
uv run --with unicorn python tools/ns4_arp_presets.py table target/fw_unpacked/bank0_CODE.bin \
    --sram target/fw_unpacked/bank0_SRAM.bin
python tools/ns4_arp_presets.py check ".reference/Nord Stage 4 Factory Restore RevK v1.66.ns4b" \
    docs/research/arp-preset-table.csv
```

## 2. How it was found (angle 1, the handler of `0x114`)

| # | Finding | Evidence |
|---|---|---|
| 1 | The registration of `0x114` passes `sl` as the third word of the info struct (`{1, 0, sl}` at `[sp,#0x30..0x38]`, `0xC01AA264..0xC01AA29C`); the other ids pass 0. In the registry CSV this is the last field (`1;5;1` for `0x114`, `0x115`, `0x10a`), probably a "not stored in the patch" flag. It is **not** a handler pointer (the first pass misread it). | EMPIRICAL (the flag meaning is inferred; the pointer reading is ruled out by the callee): `python tools/arm_xref.py dis target/fw_unpacked/bank0_CODE.bin 0xC01AA240 0xC01AA2D0` and the callee `0xC00F0A28` (stores the struct at `obj+0xC4`), [arm-edit-params.csv](arm-edit-params.csv) rows 327 to 342. |
| 2 | The consumer is the Glue object `GlueArpPtnPreset %c` (string `0xC02B75D8`, vtable `0xC02B7C10`). Its vtable method `0xC0102F30` is the change listener (ids `0x114`, `0x118`) and `0xC010FDAC` is the update method. With `[obj+0xEC] == 2` (preset changed) it reads the dial via `0xC00247E0` (param lookup), indexes a table and computes the words. | VERIFIED: vtable words at `0xC02B7C18..0xC02B7C24`; `python tools/arm_xref.py dis target/fw_unpacked/bank0_CODE.bin 0xC010FF50 0xC0110050`. |
| 3 | The table is **28 structs of 20 bytes at `0xC02BA860`** (`bank0_CODE.bin` file offset `0x2BA860`): `name*` (+0), `flag` (+4, byte), `steps` (+8, 8 or 16), `A*` (+12), `B*` (+16). `A` and `B` are strings with one character per step: `S` (short), `L` (long), other = rest (`_`). `B` is empty when `flag` = 0. The first pass missed it because it looked for the 32-bit words; the table stores them as text. | VERIFIED: `read_struct` in `tools/ns4_arp_presets.py` (test `test_read_struct_decodes_one_entry` on a synthetic image; real image: `table` command). |
| 4 | The routine turns the strings into three words: **Dur (`0x118`)**, **Pan (`0x119`)**, **Acc (`0x117`) = 0**, and writes Length (`0x116`) = steps - 1. Dur code per step: rest 3 (`.`); `S` 0 (`E`); `L` followed by a played step 2 (`Q`); `L` followed by a rest (or the last step) 1 (`q`). Pan comes from the merge of `A` and `B` (flag 1 only): 0 `C`, 1 `L`, 2 `R`. Re-implementing the merge was avoided: the tool **runs the real routine** from `0xC010FF50` to `0xC010FE00` on the ARM emulator (`tools/arm_emu.py`, `unicorn`) with the dial read stubbed, and reads r8 (Dur), r6 (Pan), r7 (Acc) and `[sp+4]`. | VERIFIED: the apply code `0xC010FE00..0xC010FEF4` stores r8 to `0x118`, r6 to `0x119`, r7 to `0x117`, `[sp+4]` to `0x116` (literal pools `0xC011069C..0xC01106A4`); emulation output in [arp-preset-table.csv](arp-preset-table.csv). |
| 5 | Dial value 0 is not a table entry. **Superseded by section 7.1:** the firmware treats 0 as "the saved custom pattern" (restored into the pattern words), not as a built-in default pattern. | VERIFIED (section 7.1) |
| 6 | Preset names are the string just before each `A` (struct field `name*`). They are not copied into the repo (short Nord-authored titles); `python tools/ns4_arp_presets.py table ...` prints them. | Tool output (column `name`). |

## 3. Cross-check against the factory programs (angle 2)

A program stores the result (gate, accent, pan words and length), not the preset index (layer block offsets in section 4). Comparing the 41 enabled patterns of the factory backup with the recovered table, `python tools/ns4_arp_presets.py check ...` finds, with the same steps, gate and pan words and no accent ("exact"), or the same gate only:

| Preset | Exact | Gate only |
|---|---|---|
| 0 (default `EEEEEEEE`) | 4 layers (Mirror Image Pad C, Unclocked Pad A, Water Ripples A and C) | 4 |
| 2 | Ghost Pad B | 1 |
| 4 | Dual Gates A | 1 |
| 5 | none | 3 (the factory pan `RCCLCCRCCCLCCCRC` is a hand edit) |
| 11 | FM Keys 2 A | 1 |
| 13 | Glitter Whl A, Tri Pump Arp A | 2 |
| 25 | Moving Chords A | 1 |
| 27 | none | 1 |

Five table presets (2, 4, 11, 13, 25) equal a factory layer in steps, gate and pan; four of them (2, 4, 13, 25) have a non-trivial 16-step pan word, so a chance match is impossible (4^16 words per field). Presets 5 and 27 match in gate only (the factory pan was edited). **VERIFIED** for the whole table: the same routine produces all 28 entries, and the exact matches prove that it computes the words the firmware stores into a patch. The remaining factory patterns (12 steps `EEE.E.EEE.E.` with accent, 9 steps `E..E..E..`, 4 steps, ...) have no table entry: they are user-style edits. Names, lengths and rhythms are not published (angle 3: no pattern name appears in the course subtitle files; angle 4: web search found no list, see Sources).

## 4. Storage in `.ns4y` (unchanged from the first pass)

Five fields in the `0x32` Layer block (bits relative to the layer base `248 / 656 / 1064`): `pattern_on` `+138` (1), `length` `+206` (4, steps = raw + 2), `accent` `+210` (32), `gate` `+242` (32), `pan` `+274` (32); each 32-bit word is 16 two-bit steps, step n at bits `2n..2n+1` from the LSB. Gate `0 E`, `1 q`, `2 Q`, `3 .` (rest); accent `1` = accent; pan `0 C`, `1 L`, `2 R`. EMPIRICAL (ns4decode ids `424-3`, `432-7`, `433-3`, `437-3`, `441-3`; `tools/ns4_arp_patterns.py`); the exact matches of section 3 confirm the gate and pan digit order and the `E q Q .` / `C L R` letters (`Q` and `.` occur in exact matches; `q` only in firmware presets 10, 12, 22 and 24, never in an exact factory match).

## 5. Engine change

`synth/pattern_presets.rs` (new): `PATTERN_PRESET_COUNT` 8 to **29**, `PATTERN_PRESETS[0..=28]` built from the gate and pan strings of the CSV, `GatePattern` gains a per-step `pan`; `perf.rs` plays the preset pan; `CustomPattern::from_preset` copies it. Host parameter `synthg_pat_preset_{1,2,3}` range 0..7 to 0..28 (no id removed; the step count in `param-ids.snapshot` changes 7 to 28). GUI: dial and `PatternPreset` clamp 28, OLED shows the hardware value (0..28).

| Item | Status |
|---|---|
| Steps, gate code and pan of presets 1..28 | VERIFIED (firmware routine; 5 presets equal factory layers exactly and 2 more in gate, section 3) |
| Preset 0 = the saved custom pattern (default: 8 steps `E`, centred) | VERIFIED (listener and update method replayed, section 7.1); the default pattern itself EMPIRICAL (`EEEEEEEE`, 4 factory layers) |
| Gate durations: `E` 4 eighths, `q` 8, `Q` 8 + tie into the next step (was ASSUMED 4, 6, 8) | VERIFIED relative to the firmware pattern compiler (section 7.2); one slot = half a step in time EMPIRICAL |
| Accent of presets | VERIFIED 0 (`r7` = 0 in the apply code) |

## 6. Implications and follow-ups (second pass, all four closed in section 7)

- The preset dial matches the hardware in count, order, length, rhythm and pan.
- Open after the second pass: the gate durations of `E q Q`, the meaning of dial value 0, importing factory patterns from programs, and the "preset edited" marker. Section 7 answers each.

## 7. Third pass

### 7.1 Dial value 0 (VERIFIED)

The listener `0xC0102F30` (events of `0x114` and `0x118`) and the update method `0xC010FDAC` keep these fields in the Glue object: `[+0xE8]` snapshot flag, `[+0xEC]` mode (1 restore, 2 apply preset), `[+0xF0]` the previous dial value, and a cache of the four pattern words (`[+0xF4]` Dur, `[+0xF8]` Pan, `[+0xFC]` Acc, `[+0x100]` Length, read from the registry slots `0x460`, `0x464`, `0x45C`, `0x458`). `python tools/ns4_arp_gate.py dial` runs both routines on the ARM emulator:

| Event | Snapshot | Mode | What the update method does |
|---|---|---|---|
| dial 0 to a preset (1..28) | 1 | 2 | caches the current words, then writes the table words (section 2) |
| preset to preset | 0 | 2 | writes the table words (the cached custom words stay) |
| dial to 0 | 0 | 1 | **writes the cached words back** to `0x118`, `0x119`, `0x117`, `0x116` and sets `0x114` to 0 |
| gate (`0x118`) edit while a preset is selected | 1 | 1 | caches the edited words, writes them back and **sets the dial to 0** |
| gate edit at dial 0 | 1 | 0 | nothing |

So 0 is the user's own pattern (the words a program stores), and a gate edit on a preset turns the dial to 0: that is the "preset edited -> custom" behaviour. Only `0x114` and `0x118` are listened to: pan, accent and length edits do not change the dial. Registry slot `0x450` is `0x114` (slots go in steps of 4 from `0x114`: Len `0x116` at `0x458`, then Acc, Dur, Pan).

### 7.2 Gate durations (VERIFIED relative, EMPIRICAL absolute)

Where the words are consumed: the Glue update method copies Dur, Pan and Acc into the per-layer arp object (`[obj+0x18/0x1C/0x20]`, table `0xC054CBD0`, `0xC011DF64..0xC011DF80`) and calls the **pattern compiler `0xC02424DC`**. `[obj+0x24]` is `steps * 2` (`0xC011C510..0xC011C524`): the compiler works in two **slots** per step, and the step code of slot `s` is `(Dur >> (s & ~1)) & 3`. It writes one record `{type, tick, pan, acc}` per slot: type 0 or 1 note on (1 = on without a preceding off), 2 hold, 3 note off; `tick` is the arp note index. `python tools/ns4_arp_gate.py gates` runs the real compiler. Result:

| Code | Slots on | Eighths of a step | Next step |
|---|---|---|---|
| `E` | 1 | 4 | independent note |
| `q` | 2 | 8 (a full step) | independent note |
| `Q` | 2, then held | 8 plus the next step's own gate | **tied**: the next step does not trigger and the arpeggio does not advance (same tick) |
| `.` | 0 | 0 | rest (note off at its first slot) |

Examples (type per slot, slot 0 first): `EE` = `0303`; `QE` = `0223` (one note of 3 slots = 12 eighths); `Qq.` = one note of 4 slots; `EEq.` = `03030232` (the `q` lasts two slots and ends at the rest). The first step of a cycle never continues a tie, so a `Q` on the last step lasts a full step. The per-step engine rule (`E` 4, `q` 8, `Q` 8 + tie, `.` 0) reproduces every note of the 29 presets and of all 1360 gate strings of 2 to 5 steps (0 differences, `check_gates`).

Limits: that **one slot is half a step in time** is structural (two slots per step in the compiler; the manual, p.36, says the arpeggiator plays 1/8 notes and one step is one 1/8 note), but the player that walks the records was not located (the ARM scan for readers of the Dur registry slot found only the Glue, the compiler input copy and the OLED grid drawer `0xC012CB2C`), so the absolute duration is EMPIRICAL. Presets 10, 12, 22 and 24 (`q`) become full-step notes instead of the earlier 6/8.

### 7.3 Importing factory patterns (done)

`cbin/synth_pattern.rs` decodes the fields of section 4 from the Layer block (`SynthLayerDecoded::pattern`, lossless pack) and `apply_decoded_layer` imports them: `CustomPattern::from_words(steps, gate, accent, pan)` (gate `E` 4, `q` 8, `Q` 8 + tie, `.` 0; accent digit 1; pan `C L R`), `pattern_on` from the enable bit, preset dial 0. A layer whose pattern bits are all zero (blank payload) keeps the engine default. Tests use synthetic payloads built in memory (the words of preset 13); no firmware or factory data is committed.

### 7.4 Preset edited marker (modelled)

`Edt_Syn_kArpPtnPresetDirty` (`0x115`) appears only as a parameter of the pattern page (`0xC01290D0`, page builders `0xC01A89D8` and `0xC01AA2CC`); no code that writes it was found, so its display role is ASSUMED. The behaviour that is VERIFIED is section 7.1: a gate edit turns the dial to 0. Engine: `SynthLayer::pattern_dial_value()` (0 while the custom pattern plays), `select_pattern_preset(v)`, and `set_step_gate` copies the playing preset into the custom pattern first (`customize_preset`). The GUI wiring of the dial readout (`panel/bay_program`, `panel/ctl.rs`) is not part of this change.

### 7.5 Implications and follow-ups

- Factory programs reproduce their stored pattern; the pattern dial reads 0 for them.
- Follow-ups: locate the record player to confirm the slot duration in time; show `pattern_dial_value()` in the OLED (GUI lane); decode the arp mode bits (`+136`, 2 bits) of a program so Poly / Gate programs load in their mode; the `q`-then-played-step records (type 1) only occur in user patterns and are played as a retrigger.

## 8. Fourth pass: record player search, rate clock, mode bits, tie editor

### 8.1 The record player was not found (negative result)

What the records are and who reads them, from the ARM code image (`tools/arm_xref.py`, a capstone scan of every function for the field offsets of the arp object, `tools/arm_emu.py`):

| Finding | Evidence |
|---|---|
| The per-layer arp pattern objects are built by one constructor (`0xC00FD740..0xC00FD858`): 6 clock objects (`0x58` bytes, vtable `0xC02B79EC`, table `0xC054CC10`) and 6 pattern objects (`0xE0` bytes, vtable `0xC02B79FC`, table `0xC054CBD0`). Each pattern object owns **32 records of 16 bytes** (`{type, tick, pan, acc}`, type initialised to 2 = hold), `[obj+0x2C]` = count, record pointers from `[obj+0x30]`, a step grid pointer at `[obj+0x10]` and a grid density at `[obj+0x08]` (default 4). | VERIFIED: `python tools/arm_xref.py dis target/fw_unpacked/bank0_CODE.bin 0xC00FD710 0xC00FD860`. |
| After the compiler, the pattern object only **publishes two numbers to its listeners**: the tick count `[obj+0xB4]` and an "has accent" flag `[obj+0xB8] != 0`, through the generic node update (`0xC023C984`, inlined in `0xC0242070`, listener list at `[obj+0xC0]`). The records themselves are handed to nobody. | VERIFIED by reading `0xC0242070..0xC02421C4` and `0xC02424DC`. |
| No other function reads the record array: the literal-pool scan finds the table `0xC054CBD0` only in the Glue constructor / update code (`0xC0119940`, `0xC011A520`, `0xC011C090`, `0xC011C510`, `0xC011DF68`, `0xC01232F0`); the OLED grid drawer reads the Dur / Pan registry words, not the records. | EMPIRICAL (negative: a function that reaches the object through a pointer copied elsewhere would not show up in a literal scan). |

So the slot duration in time still has no firmware proof. Two readings stay open, see 8.3 and the open question in `docs/OPEN-QUESTIONS.md`. Tags in code are unchanged (`EMPIRICAL`, "one slot = half a step").

### 8.2 The rate clock (VERIFIED, replayed)

The free-running arp clock is `0xC01E6368` (a periodic update over the 6 clock objects, `0xC054CC10`): it multiplies the time since its start by a rate (double, `[clock+0x38]`), wraps at `2**36`, takes the integer part as the tick and notifies its listeners (vtable word 1) when it changes. The rate is computed by the Glue update `0xC011A51C` from the rate dial value `v` (0..127) and the table `R[v]` at `0xC02BBF98 + 0x458` (128 ints):

`rate per time-base unit = R[v] / 60 * 524288 / 24000`

| Constant | Meaning | Evidence |
|---|---|---|
| `524288` = `2**19` | phase units per firmware beat | literal `0x41200000` of the update (double) |
| `24000` | time-base units per second of the counter at `0x80001DC0` | literal `0x40D77000`; every timeout added to the counter is a multiple of 24 (`+0x960` = 100 ms, `+0x258` = 25 ms, `+0x12C0` = 200 ms) |
| `R[0..127]` | `32, 32, 36, 40, 48 ... 2400` (duplicates sit on the "N.A." raw values of ns4decode's list; `R / 2` equals the BPM list there: 16 bpm at raw 0, 990 bpm near the end, `Fast1..5` above) | `uv run --with unicorn python tools/ns4_arp_gate.py clock target/fw_unpacked/bank0_CODE.bin` replays the update for all 128 values: **all 128 equal the formula** |

The step grid of the free-running arp (`0xC02BBF98`: count 8, entry k = `k * 2**18` phase units, `[arp+8]` = 2; the triplet table is `0xC02BC1A0`; the default object has 16 entries of `2**17` with `[arp+8]` = 4) is therefore **half a firmware beat per grid point = `15 / D` seconds** at the displayed BPM `D = R / 2` (dial 0: 0.9375 s, dial 127: 12.5 ms, printed by the tool).

### 8.3 What this does and does not say about the step length

- The engine (`SynthLayer::step_seconds`) plays one step per `15 / D` s (16th note at `arp_rate_bpm`). That equals the firmware grid spacing **if a grid point is a step**.
- The manual (p.36) says the rate is shown as quarter-note BPM and that "the arpeggiator plays 1/8 notes in the given tempo, which also equals one step of a pattern" (`30 / D` s). That equals **two grid points**, i.e. a grid point would be a slot (half a step): then "one slot = half a step" is right and the engine's step is twice as fast as the hardware.
- Without the consumer of the grid (the player) the two readings cannot be told apart. Needed: the player, or a hardware recording of the arp at a known dial value. Tracked in bean ns4-rwhu together with the rate mapping: the program decoder maps the stored 7-bit rate linearly to 30..300 BPM (`ASSUMED`) while the firmware table is exponential (16..1200 BPM displayed); the table itself is now VERIFIED by the replay.

### 8.4 Arp mode bits of a program (decoded)

`cbin/synth_arp.rs` reads the mode fields of the Layer block (offsets in the module doc, same base as the run switch `+135` and the rate `+175`): `+136` mode (2 bits: `0 Arp`, `1 Poly`, `2 Gate`), `+139` KB sync, `+140` range (7), `+171` direction (`0 UP`, `1 DOWN`, `2 UP/DOWN`, `3 RANDOM`), `+173` zig zag, `+174` Master Clock. `python tools/ns4_arp_modes.py "<backup>.ns4b"` over the factory backup (1152 layers):

| Field | Values found |
|---|---|
| run | 1051 off, 101 on |
| mode | 1099 Arp, 41 Poly, 12 Gate (no value 3) |
| direction | 1114 UP, 1 DOWN, 29 UP/DOWN, 8 RANDOM |
| zig zag / Master Clock / KB sync / pattern on | 17 / 86 / 4 / 41 layers |
| (run, mode) | Poly and Gate occur with run on (25, 8) and off (16, 4) |
| range | 0 on 1099 layers, 26 distinct values, max 127 on 5 |

Status: offsets EMPIRICAL (ns4decode map ids; the run switch and rate positions that share the base are already used by the decoder, and the pattern fields 4 bits further on are matched by 5 exact firmware presets, section 3); the mode and direction names come from ns4names; the range scale (octaves in thirds, Gate envelope) is ASSUMED. Engine: `synth_params_arp::apply_arp_modes` sets `arp_mode` (Poly / Gate / direction, `Off` while RUN is off), zig zag, KB sync, Master Clock sync with the subdivision `floor(9 * raw / 128)` of the nine `1/2 .. 1/32` entries (EMPIRICAL, ns4decode), range and Gate envelope. Not decoded: KB HOLD (a global bit outside the layer block).

### 8.5 Tie editor in the GUI

`PatternParam::Tie { step, on }` writes `SynthLayer::set_step_tie`; the panel mirror has `SynthLayerUi::step_tie` (copied from the preset in `select_preset`) and the GATE dial of ARP page 3 has one more detent after 8 eighths: the tied full step `Q` (the hardware's `E q Q .` cycle). `emit_pattern` sends the tie after the gate of every step (a rest drops it). There is no host parameter for the tie words, so the plugin editor maps it to nothing; adding `synthp_step_tie_*` ids is left for a later change.

### 8.6 Implications and follow-ups

- Slot / step duration: still `EMPIRICAL`; the manual and the firmware grid pull in different directions (ns4-rwhu).
- Rate mapping of programs: replace the linear 30..300 BPM by `R[v] / 2` (ns4-rwhu).
- OLED: show dial value 0 after a gate edit and the tie mark (ns4-xo2v, other lane).
- Find the player: the grid consumer is a listener of the clock objects (`0xC054CC10`); the listener registration is not in the literal scan, so look for the code that appends to the clock's list at `[clock+4]`.
