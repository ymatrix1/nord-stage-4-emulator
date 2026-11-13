# Loading a piano sound object in the ARM emulation

- **Checked:** 2026-11-13
- **Sources:** `target/fw_unpacked/bank0_CODE.bin` and `bank0_SRAM.bin` (OS v1.66, local only); `Clavinet D6  6.1.npno` read from the local factory backup (`.reference/`, never committed); `tools/arm_piano_load.py` (this note's harness) on top of `tools/arm_keydown.py` and `tools/arm_steady.py`; [keydown-paths](keydown-paths.md) sections 4 and 5c (PR #137); [nord-piano-npno-and-codec](nord-piano-npno-and-codec.md).
- **Question:** can the harness load a real sound object so that a MIDI note-on reaches the piano voice start `0xC00A9274` and sends SMP/PNO messages, so that velocity -> level (call-frame word of `0xC00A1D5C`) can be compared with `piano/level_q23.rs`?

**Answer (third pass):** the chain load -> select -> note-on -> voice start runs on real firmware code, and the level method of the piano class is entered with a velocity-dependent call-frame word. For Clavinet D6, note 60, velocities 1..127 (17 runs) the stored Q23 amplitude equals `level_q23(n)` with `n = rec[0x5E] + 2 X + 60 - 40` **exactly in 17 of 17 runs** (section 6). The routine that runs is not `0xC00A1D5C` but its sibling `0xC00A1EB4` (class table `0xC02A1D20`); every piano class tried (class bytes 1..7, 0xE, 0xF, 0x10) takes it, and `0xC00A1D5C` (class table `0xC02A1CD8`) is never entered. The SMP messages of the first 40 ticks do not depend on velocity or note (they depend on the class). Missing pieces are in section 4.

## 1. What the stubs give (VERIFIED, run)

`LibraryStub` replaces the file read `0xC0094EA0(storage, entry, offset, buf, len)` (`len` on the stack) and the directory lookup `0xC0096264(storage, entry, D)` (fills a 24-byte descriptor head; word `D + 0xC` is the file kind). The rest of the FAT code, the volume locks (`[storage + 0x28]`) and the loader run unchanged.

Command: `python tools/arm_piano_load.py ... --ns4b <backup> --npno "Piano/Clavinet/Clavinet D6  6.1.npno"`, from the settled state (cache of `arm_keydown`):

| Item | Result | Tag |
|---|---|---|
| Managers | two objects with vtable `0xC02B06D0` (slot 0 = `0xC00C0444`): `0x90BF53D0` (volume 0, `[mgr + 8]` = storage `0x9017D860`, `[mgr + 0xC]` = 0) and `0x90BF53E0` (volume 1, storage `0x9017DA30`); `[mgr + 4]` = pool `0x906A3010` | VERIFIED (run: RAM scan for the vtable word) |
| Pool | `[pool + 4]` = 0x78 = 120 pre-allocated sound objects (vtable `0xC02AE808`, stride `0x64D0`, first `0x90901240`), array pointer chain `0x906A3018 -> 0x906A3020 -> 0x906A3040` | VERIFIED (run) |
| Storage objects | `[storage] = 1` (mounted), `[storage + 0x6C] = 0xA4` (header bytes), already built by the settled firmware | VERIFIED (run) |
| `0xC00C0444(0x90BF53D0, 1, 1)` | returns 0 (loader success); the loader creates object `0x90901240` through `0xC00B90F8` then `0xC00B84CC` | VERIFIED (run) |
| Op meaning | op 1 = load one file (`0xC00BA99C`: lookup, read `0xB0` header bytes, copy to `D + 0x18`, call `0xC00B92E4`); op 2 is **remove** (erases the object whose `[P + 0x6420]` matches); op 4 (`0xC00BA74C`) resets every pool object and then **scans each volume** (`0xC00BA1F8`) to load all files, i.e. the library directory is built by scanning, not by a descriptor list the harness must supply | EMPIRICAL (disassembly, op 1 run) |
| `[core + 4]` after the load | still 0: the load fills the pool, it does not bind the object to the core | VERIFIED (run) |

Descriptor and file layout the loader needs (all derived from disassembly and the read log of the run; offsets are file offsets of the served image):

| Offset / len | Use | Tag |
|---|---|---|
| `0, 0xB0` | header block read by `0xC0095C0C`; bytes `0..0xA3` become `D + 0x18..`; `[0xA4..0xA7]` = type word, compared as LE word with `0x6E706E6F` (piano); the `.`-in-name test is skipped when the type word matches | EMPIRICAL (read log + disassembly of `0xC00B92E4`) |
| `0xB0, 0x16` | `CNSP` magic (`0x50534E43` LE word), then BE halfwords; the first halfword is the format version, compared with the ranges `0x407..0x42F`, `0x430`, `0x440`, `0x450` and `0x464` | EMPIRICAL |
| `0xC6, 2` and `0x6CC, 2` | two more BE version halfwords (`0x0464` in the Clavinet file), checked by `0xC00B276C` | EMPIRICAL |
| `0xC6, 0x606` | the information block, copied to `P + 0x5899` (the `0x606`-byte area `0xC00BA74C` clears) | EMPIRICAL |
| `0x6CC, 0x116` then `0x7E2 + 0x76 k`, 38 records | descriptor records read in kind 0; kind 2 stops after the header block | EMPIRICAL (read log) |

Layout equivalence (ASSUMED, confirmed only by the loader accepting it): the backup file carries `CNSP` at `0x2C` (CBIN header); the loader reads it at `0xB0`. Serving `0xB0` device-header bytes (name, type word) followed by the backup bytes from `0x2C` makes every loader offset land on the matching backup field: version `0x0464` at `0x0464 @ 0x30 -> 0xB4` and `@ 0x42 -> 0xC6` and `@ 0x648 -> 0x6CC`; the first stroke record `0x75E -> 0x7E2` (the `0x760` table of [nord-piano-npno-and-codec](nord-piano-npno-and-codec.md), `u32` offset `0x1F30`). What the real device header holds beyond name and type word is unknown.

Descriptor `D` head written by the lookup stub: `u16` volume, `u16` entry, two `u32` zero, `u32` kind at `+0xC`, `u32` size at `+0x10`. Kind (`[D + 0xC]`) 2 sets object state `[P + 0x64C0] = 2`; any other value gives state 0 (read by `0xC00B9DAC..0xC00B9DB4`: `sb = [D + 0xC]; cmp sb, #2; movne sb, #0`). What kinds 1, 3, 4 mean is not decoded.

## 2. Binding: found (VERIFIED, run)

`0xC00AAA58(core, id, arg)` is the core's select method (core `0x90BF5400`, `[core + 0x14]` = pool). It scans the pool for the object whose `[P + 0x6420]` equals `id` (the loader stores `entry | volume << 14`, so id 1 for entry 1 on volume 0), requires object state 0, and tail-calls the binder `0xC00A3988(core, layer, P, arg)` with `layer = [P + 0x64C0] = 0`. The binder stores `P` at `[core + 4 + 4 * layer]` (`str r3, [r6, #4]` at `0xC00A3A40`; this is why a scan for stores at `[core + 4]` in the key-path ranges found nothing: the binder lives at `0xC00A3988`) and clears per-layer tables (`core + 0x46 + 256 layer`, the `[core + 0x378]` record array). After `select_sound(emu, 1)`: `[core + 4] = 0x90901240`, real firmware path, no hand write. A second call with a different `arg` changed `[core + 0xC]`, so call it once. Callers of the select method: `0xC00BB4D4`, `0xC00BB610`, `0xC011FDF8` (not decoded: what program event triggers them). Manager op 4 with a stubbed two-level directory scan (storage `+0x60` table: count `+0x14`, start `+0x10`, vector `+0x2C` of entry objects with the id as `u16` at `+4`) also loads the file through the real scan `0xC00BA1F8`, but binds nothing (write watch on `core + 4`: 0 writes).

## 3. Note-on with the bound object (EMPIRICAL, run)

The first note-on blocked: `0xC00B6948` sends the DSP words `0x90000000, 0x03992772` and `0xC006CCBC` then polls a GPIO input bit (`0xC006CD60`) until a timeout of 4800 timer counts (4800 ticks in the harness). `install_dsp_ready` sets the polled bit to the expected level (ASSUMED model: the DSP answers at once). Then, note-on 60 / velocity 100 over the UART2 model, 40 ticks (`--note 60 --velocity 100`):

| Result | Value |
|---|---|
| Function entries | `midi_note_on_post` 1, `piano_event_slot3` 1, `piano_key_event` 2, `piano_voice_frame` 2, `piano_voice_start` 2 |
| Messages | 58 on tick 3 and 4: `SMP` path 1, core 0: `0x90000000/0x03992772` and `0xA0000000/0` (link frame words), then groups of four (`0x93936, 0x93934, 0x93933, 0x93938` and the same `+0x36` for each of four records) with floats `0x3F800000, 0, 0, 0x3F7AB576` (1.0, 0, 0, 0.9793), later `0x925F6`, `0x92617`, `0x9252E..0x92530`, `0x92526`, and at tick 4 `0x93BF4 = 0x3F800000` |
| Velocity 1 / 64 / 127 | all 58 rows byte-identical |
| Notes 36 / 60 / 84 | all 58 rows identical |
| `0xC00A1D5C` | never entered (also no direct caller in the image) |

Reading: the key path allocates and initialises voice records, but nothing in these messages depends on the note or the velocity, and the level procedure is not reached. The object's per-key tables (`P + key + 0xF3 / 0x173 / 0x3F3 / 0x473`) hold 1 to 2 non-zero bytes each, and the information block flag `P + 0x5F0E` is 0 (copied by `0xC00B8784` from the file, byte `0x135`), so the key path takes the branch `0xC00BCB14` (`[P + 0x6424]` kind test) for an object of an unknown kind. Whether this voice start is a real note voice or a default / placeholder allocation is not decided.

## 4. Class kind and the real header (EMPIRICAL, run)

`[P + 0x6424]` is the byte at `P + 0x5EA2`, written by `0xC00B8998` from the first-read information block: it is the backup-file byte at offset `0x44` (`0x42..0x47` hold `04 64 | kind 01 01 02` after the version halfword). Values over the 38 factory files: 1 (EGrand), 2 (EP), 3 (Wurlitzer), 4 (Clavinet D6), 5 (grands, 6 files), 6 (uprights), 7 (harpsichords), 0xE, 0xF (digital), 0x10 (marimba, vibraphone). `[P + 0x5F0E]` is 0 for all files tried.

| Kind | Level method called | Message list of the 40 ticks (hash of tick/chip/path/dm/value) |
|---|---|---|
| 4 (Clavinet), 7, 0xF, 0x10 | `0xC00A1EB4` x 2 | `72eda6a1` (58 messages), independent of the file |
| 5, 6 (grand, upright) | `0xC00A1EB4` x 4 | `c1a68e07` (94) |
| 1, 2, 3 | `0xC00A1EB4` x 4 | `57f826ee` (76) |
| 0xE | `0xC00A1EB4` x 2 | `dd44cb44` (52) |

The class tables are vtable-like rows `{type check 0xC00A1B5C, select, 0xC00A2114.., pitch 0xC00A1C60, level, 0xC00A1BF4, end}` at `0xC02A1CB4` (level `0xC00A210C`), `0xC02A1CD8` (`0xC00A1D5C`), `0xC02A1CFC` (`0xC00A1E00`), `0xC02A1D20` (`0xC00A1EB4`); the voice-frame method builds a temporary object whose vtable pointer is the row (`str ..., [sp, #0x28]` at `0xC00A8BF8..`, `0xC00AA150`). Only the `0xC02A1D20` row was read at run time (slots 0..6, `0xC02A1D48`, `0xC02A1D58`). Rows 1..3 and `0xC00A1D5C` belong to another sound class that these piano files do not select; which one is not decided. The earlier listing-based derivation of the level law (PR #125) is for `0xC00A1D5C`; `0xC00A1EB4` uses the same index and table, with extra branches (class 5/6 attenuation at `0xC00A20C4`, stereo / mode variants at `0xC00A1F8C`).

The real device header (bytes `0..0xB0`) is still invented by the harness (name and type word); the class byte comes from the body, so the header was not needed for these runs.

## 5. Reproduce

```bash
python tools/arm_piano_load.py target/fw_unpacked/bank0_CODE.bin target/fw_unpacked/bank0_SRAM.bin     --cache target/keydown.snap --ns4b "<backup>.ns4b" --npno "Piano/Clavinet/Clavinet D6  6.1.npno"     --note 60 --velocity 100
```

Run one velocity per process: a second note in the same run blocks (the DSP link send `0xC00B6948` waits).

## 6. Velocity -> X and the level law (VERIFIED for the law, EMPIRICAL for X)

`tools/arm_piano_load.py` hooks `0xC00A1EB4` (`hook_level_calls`): call frame `(this, P + 0x5EA0, rec, key, X, arg6, out0, out1)`, first output pointer receives the Q23 amplitude. Clavinet D6, note 60 (`key` = 60), one run per velocity; the per-key bytes `b1 + b2 + b3` (`P2 + key + 0xF3`, `+ 0x3F3`, `+ 0x473`) sum to 40 at this key (implied by every run, below):

| Velocity | X | `rec[0x5E]` | Amplitude (Q23) | `n = rec + 2X + 60 - 40` |
|---|---|---|---|---|
| 1 | 170 | 36 | 0x151CB | 396 |
| 8 | 156 | 36 | 0x1D2CD | 368 |
| 16 | 140 | 36 | 0x2A396 | 336 |
| 24 | 126 | 36 | 0x3A59A | 308 |
| 32 | 112 | 36 | 0x50A28 | 280 |
| 40 | 100 | 36 | 0x6A660 | 256 |
| 48 | 88 | 36 | 0x8C64C | 232 |
| 56 | 77 | 36 | 0xB504F | 210 |
| 64 | 66 | 36 | 0xE966A | 188 |
| 72 | 56 | 36 | 0x126111 | 168 |
| 80 | 45 | 36 | 0x17B293 | 146 |
| 88 | 36 | 56 | 0x172803 | 148 |
| 96 | 27 | 56 | 0x1C823E | 130 |
| 104 | 18 | 56 | 0x231930 | 112 |
| 112 | 9 | 66 | 0x267F2E | 104 |
| 120 | 1 | 66 | 0x2E5007 | 88 |
| 127 | 0 | 66 | 0x2F6526 | 86 |

- **Level law: VERIFIED (run).** `round(2^23 * 2^(-(n mod 60) / 60)) >> (n div 60)` equals the stored amplitude for all 17 rows with the same offset 40 (test `test_level_amplitude_reproduces_the_captured_levels`; the run script computed 17 of 17 equal). This is the law of `piano/level_q23.rs` (table and octave shift); the "read from the listing, not run end to end" tag there can be upgraded for the Clavinet class-4 path, with the caveat that the routine run is `0xC00A1EB4`, not `0xC00A1D5C`.
- **X(velocity): EMPIRICAL.** X falls from 170 (v = 1) to 0 (v = 127) with local slopes of -2, -2, -1.75, -1.75, -1.5, -1.5, -1.375, -1.375, -1.25, -1.375, -1.125 (per velocity step between the sample points), i.e. multiples of 1/8: a table, not a formula. A quadratic `X = 171.4 - 2.007 v + 0.00551 v^2` fits v = 1..80 within 1.06 index steps (0.11 dB); a cubic within 0.40. X is not the whole velocity effect: the stroke record changes too (`rec[0x5E]` 36 -> 56 at v = 88 and -> 66 at v = 112), i.e. the velocity layer picks a different stroke record whose level offset is 20 / 30 steps higher (2.0 / 3.0 dB) while X continues to fall. The total index `rec + 2X + 20` runs from 396 (v = 1) to 86 (v = 127), a range of 310 steps = 31.1 dB.
- Where X is computed (the caller of `0xC00A1EB4` is `0xC00A8580`, inside the voice-frame method `0xC00A8370`) and whether the curve differs per class or per file was not traced; only Clavinet D6 was swept.

## Implications

- The chain load -> select (bind) -> note-on -> voice start -> level method now runs on real firmware code with two library stubs and one DSP-ready hook; velocity reaches the level method as `X` and as the stroke record choice.
- `piano/level_q23.rs` needs only a velocity -> (stroke, X) front end to be complete; the law behind it matches the firmware for this file and key.

## Follow-ups

1. Trace the origin of X: read the caller `0xC00A8580` and the velocity-to-X table (sampled above), check whether it lives in the information block (per file) or in the firmware, and sweep a grand and an upright.
2. Sweep the key (b1 + b2 + b3 per key) and the layers `rec[0x5E]` for notes 24..96 to get the per-key level offsets.
3. Find which sound class selects `0xC00A1D5C` (rows 1..3).
4. A second note blocks in the same process: model the DSP link handshake properly (pin and timing of `0xC006CCBC`).
