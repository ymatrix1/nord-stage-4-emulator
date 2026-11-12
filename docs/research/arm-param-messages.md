# ARM to DSP parameter messages: senders, channel tables, targets

- **Checked:** 2026-11-12
- **Sources (local, never committed):** `target/fw_unpacked/bank0_CODE.bin` (OS v1.66, ARM mode, base `0xC0000000`), `tools/arm_params.py` (new, tests in `tools/tests/test_arm_params.py`), `tools/arm_xref.py`, `capstone` 5 and `unicorn` 2 (`uv run --with capstone --with unicorn`), the DSP images from `tools/ns4_dsp_images.py` + `tools/ns4_dsp_ldr.py`, [arm-dsp-protocol](arm-dsp-protocol.md), [dsp-rx-protocol](dsp-rx-protocol.md), [org-stage-map](org-stage-map.md), [sy-dsp-map](sy-dsp-map.md). Derived data committed: [arm-param-table.csv](arm-param-table.csv) (sender census), [arm-chan-tables.csv](arm-chan-tables.csv) (channel records). No firmware bytes are committed (addresses and small integers only).
- **Question:** What concrete `(address, value)` messages does the ARM send to each DSP: which functions send them, what do they add to the channel base, where do the 12/14/48-entry channel tables come from at run time, which DSP does a channel address, and which senders carry the organ parameters?

Tags follow `.agents/rules/provenance-in-code.md`: **VERIFIED** = reproduced by the command shown, **EMPIRICAL** = measured but not proven, **ASSUMED** = structural reading.

## 1. Short answer

| Question | Result | Status |
|---|---|---|
| Message format | Two paths per sender, chosen by a per-target flag word: **path 1** (SY, SMP) `base + off`, `value` through `0xC006CA48` flow control to the EMIFA port; **path 2** (FX, PNO, ORG cores) `base + 4*off`, optional `0` pad, `value` to a poke port. `base` is a **system byte address** (`0x28xxxxxx`) on path 2 and a **DM word address** (`0x92406`) on path 1. | VERIFIED (code, static flags) |
| Channel tables | Not BSS-zero: a **static initialiser** (`0xC00002A4`, 44 k instructions, no external state) stores the 16-byte records `{target, base, size, tag}` of every channel table. Running it (two independent emulators, identical memory) recovers all of them, 78 tables, contiguous at `0xC0544618..0xC054B000`. | VERIFIED |
| Sender census | 438 functions, 904 flow-control sites, 434 calls of the 4 writer wrappers; 394 (function, table) rows with a table, 310 rows with confirmed `(a, 4a)` offset pairs. | VERIFIED |
| Targets | 8 targets (0..7) map to 5 comm objects `[0,0,1,1,2,2,3,4]`; targets 0..5 use path 2, 6 and 7 use path 1. Target 6 addresses SY (word addresses `0x92406..`, matching [sy-dsp-map](sy-dsp-map.md)). Targets 4 and 5 are ORG core 1/2, 2 and 3 PNO core 1/2, 0 and 1 FX core 1/2. | flags and comm map VERIFIED; chip names EMPIRICAL (section 3) |
| Organ drawbar / vibrato / percussion / rotary / key-on messages | **Not recovered.** Recovered instead: the ORG strip senders (23 functions, fields 5..60), the 48 "controller word" slots (section 6). | open |

## 2. Message format (VERIFIED, `python tools/arm_params.py senders ...`)

A sender picks `rec = table[ch]` (`ldr ip,[T, ch, lsl #4]`; `base = rec[1]`), `target = rec[0]`, then:

```
flag  = word[0xC0278F8C + 4*target]          ; 1 for targets 0..5, 0 for 6, 7 (static data, tools/arm_params.py flags)
comm  = word[0xC0278F6C + 4*target]          ; [0,0,1,1,2,2,3,4]
path 1 (flag == 0):  bl 0xC006CA48(comm.object, 1); port = *(comm+0x10): write base+off ; write value ; bookkeeping
path 2 (flag != 0):  port = [0xC055A198+0xFB8] + comm*4 + 0x40: write base+4*off ; [write 0 if byte[...+comm+0x90]] ; write value
```

The two writers are shared: `0xC0062E10` / `0xC0062E78` `(comm, word1, word2)` for path 1, `0xC0062D28` / `0xC0062D60` `(comm, address, value)` for path 2 (disassembly: `uv run --with capstone python tools/arm_xref.py dis target/fw_unpacked/bank0_CODE.bin 0xC0062D28 0xC0062EE0`). Every sender in the tables below is dual: the same function holds both blocks, so its immediates come in pairs `(a, 4a)` (for example `0x39` and `0xE4`), which is how `tools/arm_params.py` confirms a word offset. Because targets 0..5 all have flag 1, **the path-1 block of those senders is dead code** in the shipped static data; only SY/SMP channels ever take it.

The pad word: path 2 sends three words `(address, 0, value)` to targets whose pad byte is set. That matches the ORG Core1 3-word receive ring `(addr, x, data)` in [dsp-rx-protocol](dsp-rx-protocol.md) section 2, so the "2-word versus 3-word" open point of that note is resolved on the ARM side: **the ARM message is the 3-word poke `(system address, 0, data)`**. The pad byte array lives in a run-time object (`[0xC055A198+0xFB8]`, not in the image), so which targets pad is ASSUMED from the DSP side.

Address unit: `0xC0214058` (a pointer-install sender) converts a base to a DM word index with `(base - 0x28000000) >> 2` (core 1) or `(base - 0x28800000) >> 2` (core 2) and adds 9; `0x28380000 -> 0xE0000` reproduces the strip array of [dsp-rx-protocol](dsp-rx-protocol.md) (`DM(0xE0000 + 62 k)`). So `DM word w` is system address `0x28000000 + 4 w` on core 1 and `0x28800000 + 4 w` on core 2. The strip stride `62 * 4 = 0xF8` is exactly the stride of the recovered ORG bases (below). VERIFIED (code and CSV), mapping of word index to strip field ASSUMED.

## 3. Targets and channel tables (VERIFIED initialiser, EMPIRICAL chip names)

Recovery: `0xC00002A4` is a static initialiser that only writes constants (loops are CRC tables); every `bl` it makes is external and skipped. `uv run --with unicorn python tools/arm_params.py emulate bank0_CODE.bin MEM.bin` runs it; an independent first implementation gave a byte-identical 16 MB image. Reading the image: `python tools/arm_params.py tables MEM.bin --table 0xC05490B8:12`.

Static literal xref (`python tools/arm_xref.py literal ... 0xC05490B8`) finds only readers; the stores use base registers loaded from other pool words (for example `str lr, [r4, #-0xBC]` at `0xC0001608`), which is why the earlier notes could not find the filler.

| Table | Entries | Content |
|---|---|---|
| `0xC05490B8` ("B", 8 small senders + float setters + the apply method at `0xC02141E4`) | 12 | records 0..4 `{0, 0x282C9068 + 0x4E8 k, 0x1393, (k+1)<<24}`, 5..9 `{1, 0x28AC9068 + 0x4E8 k, 0x1393, 0xD..0x11 <<24}`, 10, 11 `{2, 0x282C3238 / 0x282C3720, ...}` |
| `0xC0548FD8` ("A", `0xC002AC5C`, `0xC02040DC`, parameter-group constructor `0xC0208B5C`) | 14 | `{0, 0x282C2120 / 0x282C2960 / 0x282C31A0 / 0x282C39D0 / 0x282C3F98}` then target 1 at `0x28AC....`, then target 2 `{0x2838C618, 0x2838D5F8, 0x2838DD28, 0x2838E2F0}` |
| `0xC0547288` / `0xC05473A8` / `0xC05474C8` (ORG/PNO channel strips, 43 + 33 + 21 sites) | 36 | **triples** per strip k: `{3, 0x28B80000 + 0xF8 k}`, `{4, 0x28380000 + 0xF8 k}`, `{5, 0x28B80000 + 0xF8 k}`, k = 0..11, size `0x3D2`, tag `0x15000000` |
| `0xC05495C8` (controller words) | 48 (+4) | `{3 / 4 / 5, 0x28B07730 + 4 i}` for i = 0..15 per target (`0x28307730 = DM(0xC1DCC)`), size `0x10`, tag `0x15000000` |
| `0xC0547F78`, `0xC05482F8` (SY) | 40+ | `{6, 0x92406 ...}`, `{6, 0x9219E ...}`: DM word addresses, path 1 |

Target to chip: strip default pattern `0x80120BF7` (org-dsp-map section 4) appears at the same offsets in **ORG core 1 and core 2 and in PNO core 2 only** (EMPIRICAL: byte search of `core{1,2}_sram.bin` of the FX, PNO and ORG images, hits at `0x304E20, 0x304E28, 0x304EC0 ...`; FX none, PNO core 1 none), which is exactly the pattern of the triples `(3, 4, 5)` = PNO core 2, ORG core 1, ORG core 2. With the comm map `[0,0,1,1,2,2,3,4]` and the download order (index 0 FX, 1 PNO, 2 ORG, 3 SY1SY2, 4 SMP), targets 0/1 = FX, 2/3 = PNO, 4/5 = ORG, 6 = SY, 7 = SMP. The comm index to image-index identity is ASSUMED (same order, no code proves it). Settling observation: capture one real frame on the SPI/EMIFA link per DSP, or log `comm` at `0xC006CA48` while changing a knob in an ARM emulator.

Two stats on the recovered tables: the table region is one contiguous array of records (the 78 "tables" are slices), and a logical channel on PNO/ORG owns **three** consecutive records, one per receiving core. A sender writes one record; the callers repeat it for the other two (the apply method `0xC0017088` makes 32 wrapper calls).

## 4. Sender census (VERIFIED, [arm-param-table.csv](arm-param-table.csv))

`python tools/arm_params.py senders bank0_CODE.bin --mem MEM.bin --csv docs/research/arm-param-table.csv`. Columns: `function`, `flow_control_calls`, `wrapper_calls`, `table`, `word_offsets` (path 1), `byte_offsets_path2` (`4 x`), `other_immediates` (unpaired adds, usually `base + n` index arithmetic), `first_record`.

| Family | Functions | Table | Offsets (word) | Value |
|---|---|---|---|---|
| Table B small senders `0xC00109EC .. 0xC001138C` | 8 | `0xC05490B8` | `0x39, 0x38, 0x46, 0x30, 0x2C, 0x26, 0x20, 0x4B` (path 2: `0xE4, 0xE0, 0x118, 0xC0, 0xB0, 0x98, 0x80, 0x12C`) | raw `r1` (int or float) |
| float setters `0xC020C4CC`, `0xC020C624` | 2 | B | `0x37` (path 2 `0xDC`) | bool to float: `0.0` when `r1 != 0`, `1.0f` (`0x3F800000`) when `r1 == 0` |
| int to float setters `0xC020C744..` | 1 | B | `0x32, 0x33` (path 2 `0xC8, 0xCC`) | `r0`, `r4` converted by an inlined int to float |
| apply method `0xC02141E4` | 1 | B | calls the 8 small senders with fields `[obj+0x48,0x4C,0x60,0x64,0x70,0x74,0x90,0x94,0xA0,0xBC,0xC0,...]`, channel `[obj+4] < 12` | |
| pointer install `0xC0214058` | 1 | B (ASSUMED, pool not decoded) | `base + 1` word `(base>>2) + 9` (path 2 `base + 4`) | word index of the strip |
| ORG strip fields | 23 | `0xC0547288 / 05473A8 / 05474C8` | single-field senders: `0x30, 0x32, 0x33` (`0xC01C3170, 0xC01C2FCC, 0xC01C2E30`), `0x35`, `0x39/0x3A`, `0x26`, `0x3C`, `0x0E`, `0x0A`, `0x0C`, `0x0D/0x13`, `0x05/0x09/0x25`; bulk `0xC0017088` | |
| controller words `0xC01CE034`, `0xC01CE128` | 2 | `0xC05495C8` | none (word 1 = `base`) | packed state word (section 6) |
| SY/SMP (path 1 live) | many | `0xC0547F78`, `0xC05482F8`, `0xC0558F98` | `base + off`, base a DM word address | |

Cross-check against the DSP: the ORG strip fields that the DSP setup code reads ([org-stage-map](org-stage-map.md): 2, 26, 28, 30, 32, 34, 37, 39, 40, 48..51, 53, 55, 57) intersect the ARM-written fields in `32, 34, 37, 48..51, 53, 57` (9 of 16). With ARM offsets spread over 5..60 this is only weak support (chance overlap would already be about 6), so strip field roles stay ASSUMED.

## 5. Organ messages: what could and could not be recovered

| Item | Result | Status |
|---|---|---|
| 9 drawbar levels | no sender identified. They are not plain strip fields: the DSP reads drawbar-like values from the packed controller words (`0xC1DCC`). The 48 controller slots (section 6) are the candidate carrier; which slot index is drawbar 1..9 needs the caller that fills `obj[0]` (index) and `obj[8]` (state). | open |
| Vibrato / chorus on, type | not identified. Candidate: one of the ORG single-field senders (`0x0A`, `0x0E`, `0x25`, ...). | open |
| Percussion on / third / fast / soft | not identified | open |
| Rotary speed | not identified; `0xC01C61F0` and `0xC01C6514` write one ORG strip field each (`0x0E`, `0x0A`) | open |
| Key-on | not found among the 904 + 434 sites. The keybed path (scan chip, UART/MIDI handlers) does not use these tables in the census; first guess is a separate voice-allocation sender that writes SY/SMP word addresses and the ORG tonewheel bank | open |

Why open: the call-site chain from the UI objects to the senders goes through virtual calls and an object graph built by the same static initialiser (parameter-group constructor `0xC0208B5C`, descriptors `{ptr, 0, 1, count}` at `0xC0442A28 + 0x6C ...`), not through direct `bl` chains, and no MIDI-CC (`CC 16..24`) handler was followed. The `.ns4p` parameter ids (`docs/specs/cbin-and-presets.md`: layer drawbars at `+40 + 19 i`, model, vibrato, percussion bits) are not referenced by immediates in the ARM code, so there is no static join key.

## 6. Controller words (VERIFIED code, meaning ASSUMED)

`0xC01CE034` (23 callers) and `0xC01CE128` (2 callers; `0xC01D04FC`, a third sender on the same table, has 4) take a controller object `{[0] = index 0..0x2F, [8] = state word}`, rewrite the state word and send `(table[index].base, state)`:

- `0xC01CE034`: `state = (state & ~0x7000000 & ~0xFF) | 0x4000000`, with bit 8 toggled when the result equals the old value;
- `0xC01CE128`: `state &= ~0x7000000`, bit 8 toggled when bits 24..26 were clear.

`table = 0xC05495C8`, so index `i` addresses `DM(0xC1DCC + i % 16)` of PNO core 2 (`0..15`), ORG core 1 (`16..31`) or ORG core 2 (`32..47`). The DSP side ([org-stage-map](org-stage-map.md): packed word `R11 = DM(0xC1DCC + ...)`, 7-bit value in bits 8..14, type in 5..7, slot in 28..30) agrees with the layout of this state word (type bits 5..7 are tested in the caller at `0xC01E3C90`). So the ARM forwards a panel-control state word unchanged as the "packed controller word". Which physical controls (drawbars, buttons) use which index is not decoded.

## 7. Reproduce

```bash
python tools/arm_params.py flags   target/fw_unpacked/bank0_CODE.bin
uv run --with unicorn python tools/arm_params.py emulate target/fw_unpacked/bank0_CODE.bin MEM.bin
python tools/arm_params.py tables  MEM.bin --table 0xC05490B8:12 --table 0xC0548FD8:14
python tools/arm_params.py senders target/fw_unpacked/bank0_CODE.bin --mem MEM.bin --csv docs/research/arm-param-table.csv
python tools/arm_params.py records target/fw_unpacked/bank0_CODE.bin --mem MEM.bin --csv docs/research/arm-chan-tables.csv
```

## Implications

- The emulator can model the ARM as a **3-word poke source**: message = `(0x28000000 + 4 w [+ 0x800000 for core 2], 0, value)`, with `w` the DM word. `sharc-org --msg W1=W2` takes DM word offsets from the strip base; a table row now gives the base per strip and core (`DM(0xE0000 + 62 k)` for ORG).
- The three-record structure means one logical strip write needs three pokes (PNO core 2, ORG core 1, ORG core 2); a harness driving only ORG Core2 should use the row with target 5.
- Controller words `DM(0xC1DCC + i)` are the unresolved link to the packed words the ORG stage code reads, so a faithful ORG patch needs the index to control map (Follow-up 1).
- SY/SMP messages are word-address plus value pairs, consistent with the SY FIFO decoder in [sy-dsp-map](sy-dsp-map.md).

## Follow-ups

1. Map controller index to control: find the writers of `obj[0]` for the objects whose callers include `0xC01CE034` (the large UI scan function around `0xC01E3BE0`, locals at `sp+0xA7C`), then match with panel indices; expected result: drawbar `i` has a fixed controller index with `state = (value7 << 8) | flags`.
2. Settle the run-time state with one observation: a RAM dump (JTAG or emulator) of `0xC0544618..0xC054B000` after boot, to confirm the initialiser values are not changed by a later configuration step (the flag words `0xC0278F8C ..` are in the image but are `.data` and could be patched).
3. Capture real frames: log the first word of 20 writes while moving a drawbar and a knob (logic analyser on the DSP link, or hook `0xC0062D28 / 0xC0062D60` and `0xC006CA48` in an ARM emulator). Expected for a controller word on ORG: 3 frames `(0x28307730 + 4 i, 0, state)`, `(0x28B07730 + 4 i, 0, state)` for the two cores.
4. Trace the keybed and MIDI note path (UART0/1/2 literals at `0xC00677A4..0xC006794C`) to the first sender that writes ORG/SY words with a note number.
5. Join the `.ns4p` parameter ids to the senders through the parameter descriptors at `0xC0442A28 + 0x6C ...` (`{ptr, 0, 1, count}` arrays registered by the group constructor).
