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

## 7. Where X comes from, three sounds, per-key sums (VERIFIED run / EMPIRICAL fit)

**Origin of X (EMPIRICAL, disassembly + write watch).** The level method is called from the voice-frame method `0xC00A8370` (return site `0xC00A8580`); `X` is the second output of the stroke selector (slot 1 of the class table, `0xC00A2A54`), computed as `clamp((this[0x28] + this[0x30]) [>> 2 when P[0x44] != 0], 0, 249)`. `this[0x28]` is written by the voice-init function `0xC00AA520` (`0xC00AA6BC`):

```text
T = curve[curveId][r8]           curveId = [ctx + 0x1C] (11), table 0xC0532664 + (curveId << 10), r8 = internal velocity 0..1023
X = clamp(d + (((128 - c) * T) >> 7), 0, 249)        (c, d) = bytes at 0xC02A1208 + 0xB80 + 2 * zone  (0x13, 0x00 for zone 1)
stroke selection index = T                            ([this + 0x24]; thresholds against the stroke records pick the layer)
```

Check, Clavinet at velocity 64: `r8 = 203`, `T = 78`, `X = (109 * 78) >> 7 = 66` (matches). Velocity 1 / 64 / 127 give `r8 = 17 / 203 / 656` and `T = 200 / 78 / 0`: `r8` is a convex function of the MIDI velocity computed before the voice frame (queue consumer, not traced). `curveId` and the zone were 11 and 1 for all three sounds in the default program.

**Sweeps.** One process per sound; after the first note a snapshot is restored for each velocity (the harness cannot play two notes in a row, see below). Notes 60, velocities 1..127 (Clavinet D6 every odd velocity plus the 17 runs of section 6; Bright Grand and Amber Upright every 4th plus 127):

| Item | Result |
|---|---|
| `X(velocity)` | identical for the three sounds at every common velocity (0 differences in 32 and 27 common points): 170 at 1, 66 at 64, 36 at 88, 9 at 112, 0 from 121 up |
| Level law | the stored amplitude equals `level_q23(rec + 2X + 60 - sum(b))` in every run with one constant `sum(b)` per sound at note 60 (40, 49, 54): 146 of 146 runs |
| Stroke offset `rec[0x5E]` | a step function of velocity, file specific: Clavinet 36 / 56 / 66 (steps between velocities 85..87 and 107..109), Bright Grand -10 / -1 / -2 / 8 / 19 / 26 / 34 (steps near 61, 69, 81, 93, 101, 117), Amber Upright -1 / 8 / 18 / 28 / 36 / 44 / 43 (steps near 49, 69, 77, 89, 109, 117) |
| Notes 24..108 step 12 at velocity 64 | `X` 66 for every note and sound; `rec[0x5E]` varies with the note (Clavinet 55, 11, 29, 36, 82, 79, 137, 147: it is the level of the selected stroke, i.e. of the root key), and the key byte sum varies too: Clavinet 40 at every swept note, Bright Grand 49 / 49 / 45 / 49 / 63 / 49 / 49 / 49, Amber Upright 49 / 49 / 49 / 54 / 63 / 58 / 19 / 49 |

So the velocity law is `X(velocity)` (key- and class-independent here) plus the stroke offset of the selected stroke record; both feed the proven level law. `piano/velocity_level.rs` holds the `X` table, the stroke step tables of the three sounds (note 60 only), the per-key sums and the interpolation helpers (opt-in).

**Playing several notes (fixed).** The second note-on blocked in `0xC00B64D8` (voice status read): it calls `0xC0066E64` (stubbed, returns 0 bytes) and repeats until the word at `[sp + 0x14]` is non-zero; the first note read stack garbage. `install_dsp_ready` now also writes 1 at `0xC00B658C`. A second note then starts, but its level call was not reached within 40 + 20 ticks and the main loop idled with no parser calls, so the sweeps use a state snapshot per velocity (about 4 s each). A full model of the SMP status handshake is still open.

**Not derived.** The internal velocity `r8(v)` (a table or curve of the MIDI velocity), the stroke-record thresholds that pick the velocity layer, how `rec[0x5E]` is built from the file's 118-byte stroke records (the raw file halfword at `+0x5E` is `128` in the Clavinet records, so the loader converts it), and the three per-key bytes from the file.

## 8. r8(velocity), the stroke record and rec[0x5E] (VERIFIED run / EMPIRICAL)

**r8 (VERIFIED, run + disassembly).** The key event handler passes `r8` as the first stack argument of the voice frame; it is stored by the note-on translation at `0xC00D206C` (`str r5, [r2, #0x18]`, event record `0xC0580A70`). For velocity `v` (clamped 1..127, 0 uses another word), `r5 = s16[0xC02B3128 + 0x18 + 2 * idx]` with `idx = v + 256` (curve type 1), `v + 128` (type 2) or `v + 128 * sel` (`[core + 0x134]`-style selector). The default program reads curve `sel = 6`: `r8(1) = 17`, `r8(64) = 203`, `r8(127) = 656` (0 for velocity 0 on another path). The table is 8 curves of 128 halfwords (curve 0 and 4 equal; curve 5 reaches 1016 at 127).

**X is a table lookup (VERIFIED).** `T = curve11[r8]` (byte table `0xC0532664 + 11 * 1024`), `X = clamp(0 + ((128 - 0x13) * T >> 7), 0, 249)`: with the two firmware tables read out of the image this reproduces all 79 swept `X` values exactly. The sampled `X_SWEEP` list can therefore be regenerated by `piano/velocity_curve.rs` from the 127-entry derived list `T_BY_VELOCITY` (`x_from_curve`); the two source tables are not copied. A closed form of `T(v)` was not found (the curves are tabulated).

**rec[0x5E] (VERIFIED, run).** The in-memory stroke records are 100 bytes (array base `[P + 0x5894]`, record `k` at `base + 100 k`) built from the file's 118-byte records; for all 38 Clavinet D6 records the halfword at record `+0x5E` equals the big-endian halfword at file-record `+0x34` (so the earlier worry about the raw `+0x5E` value 128 is moot: that is another field). The byte at record `+0x5A` equals the file halfword at `+4` (the record's own threshold F, 0 / 25 / 50 for the first group).

**Stroke selection (VERIFIED for Clavinet, open for the others).** The core holds a per-root-key table (`[core + 0x378]` after the select, block `366` bytes per key, three `122`-byte layer lists: `u16` count, then `(record index, threshold)` pairs). Clavinet D6, root key 60: layer 0 list `(21, 0) (22, 16) (23, 46)`, record offsets 66 / 56 / 36; picking the entry with the largest threshold not above `T` reproduces the stroke offset of all 65 Clavinet sweep runs (`select_stroke`, test `clavinet_selection_matches_the_swept_strokes`). Not reproduced: Bright Grand (lists of 8 / 0 / 6 entries, 21 of 33 runs differ: the boundaries lie 0 to 6 below the listed thresholds) and Amber Upright (a non-empty layer-1 list that replaces `T` by a record byte; 21 of 33 differ). The selector `0xC00A2A54` has branches by file format version (`[P + 0x64B8]`: 2 / 5 / 6) and by `[this + 0x20]` / `[this + 0x34]`, only the simplest was matched.

**The thresholds are not the file values.** The loader turns the file halfword F (`+4` of the record) into the list threshold: they differ by `-7..-16` (mean `-8.5`, std `2.4`, 228 pairs of three sounds) and the offset is not a function of F alone (F = 60 gives 45 / 46 / 51 / 53). Neither a linear nor a quadratic fit of the threshold on F, nor a single other file halfword, explains it (best correlation 0.82, residual up to 10). The conversion lives in the binder `0xC00A3988` (per-key block builder) and was not decoded.

**b1, b2, b3 (not derived).** They are read from the sound object at `P2 + key + 0xF3 / 0x3F3 / 0x473` (`P2 = P + 0x5EA0`): the first two at `[key]`, the third at a fixed index. They are filled by the loader from the information block (section 4 of the CNSP file) and differ per note (Bright Grand 49 / 45 / 63 at some notes, Amber Upright 19 at note 96); no file field was matched.

**Stop point.** `piano/velocity_curve.rs` computes `X`, `T`, the stroke level offset from a `.npno` record and the stroke choice for a given threshold list. A level for any note from the parsed `.npno` alone additionally needs the threshold conversion (binder `0xC00A3988`), the multi-stage selection of the grand / upright formats and the three per-key bytes. The model was therefore **not** wired into `PianoLayer`; the default render (`render_hash 25baa387...`) is untouched.

## 9. The full chain from the file (VERIFIED run; opt-in model `piano/npno_level.rs`)

Every field below was matched against the structures the firmware builds from the same `.npno` (backup offsets; the loader reads them `0x84` further on in the device layout).

| Quantity | Source in the file | Evidence |
|---|---|---|
| Root key of a note | `keymap[note]` at `0xB8` (128 bytes, `0xFF` unmapped) | block index of the firmware key table = root |
| Records per root | big-endian halfword block at `0x648`: `hw[2]` = total records, `hw[3 + root]` = records of that root; records follow in ascending root order | firmware counts `P + 0x14 + 12 * root`; lists equal for 3 of 3 sounds |
| Stroke layer | record byte `4`, `& 3` (0 / 1 / 2) | 402 of 402 Bright Grand records |
| Level offset `rec[0x5E]` | record big-endian `i16` at `0x34` | 38 of 38 Clavinet records, all sweeps |
| List threshold | record byte `0x74` (the loader copies it to `rec + 0x5B`; `0xC00AB2CC`, binder `0xC00A3C28`) | every `(record, threshold)` pair of the firmware key blocks of Clavinet, Bright Grand and Amber Upright (1547 pairs) equals the file byte |
| Record threshold byte `F` | record byte `0x33` (`rec + 0x5A`) | used by the layer-1 pre-selection |
| `b1[note]` | signed byte at `0x138 + note` | firmware `P2 + note + 0xF3` |
| `b2[note]` | signed byte at `0x3B8 + note` (all zero in the three sounds) | firmware `P2 + note + 0x3F3` |
| `b3` | signed byte at `0x438` (40 / 49 / 49) | firmware `P2 + 0x473` |

The earlier "threshold differs from the file by -8" finding was a misreading: thresholds are the byte at `0x74`, not the halfword at `+4`. `b1` and `b2` are indexed by the **note** (`this + 0x10`), not by the root (the level call argument is the root): with the root the Amber Upright disagreed at notes 72 and 84, with the note all 24 key-sweep cases of the three sounds match (`b1 + b2 + b3 == 40 / 49 / 49 + b1[note]`).

**Selection (VERIFIED).** The firmware starts several voices per note-on; each runs the selector `0xC00A2A54` with position `p = clamp(T(velocity) + this[0x34], 0, 249)` and picks, in the list of its layer (`this[0x20]`) for the note's root, the record with the largest threshold not above `p` (the first record when none qualifies):

| Voice | layer | `this[0x34]` bias | seen in |
|---|---|---|---|
| main voice (`this[0x4C] = 1`) | 0 | 0 | every sound (the only voice of the Clavinet) |
| extra first voice (`this[0x4C] = 4`) | 0 | 20 | Bright Grand, Wurlitzer |
| extra first voice | 1 | 0 | Amber Upright |

So the first level call of the earlier sweeps (section 6 and 7) was the extra voice: the stroke offsets seen there for the grand (bias 20) and the upright (layer 1) are reproduced by exactly these parameters (146 of 146 sweep runs, now computed from the file lists). Which voices a sound gets (bias 20 or layer 1) follows the layers present in the file (grand: layers 0 and 2; upright: 0, 1, 2) but the exact trigger was not decoded; for the model they are caller supplied.

**Validation with new random cases** (24 random notes 30..100 and velocities 1..127 per sound, a state snapshot per case, all level calls recorded; `tools/arm_piano_load.py` hooks):

| Sound | Main voice (last call) | First voice |
|---|---|---|
| Clavinet D6 | 24 of 24 | none (single voice) |
| Bright Grand | 24 of 24 | 24 of 24 as `bias 20` or `main` (identical where the two lists agree) |
| Amber Upright | 24 of 24 | 23 of 24 (`layer 1` or `main`; 1 case matched no candidate) |
| Wurlitzer 1 (class 3) | 24 of 24 | 24 of 24 as `bias 20` or `main` |

Main voice: **96 of 96**; extra voices 71 of 72. The unmatched upright case was not analysed.

**Rust.** `piano/npno_level.rs` parses the fields above from the raw `.npno` bytes (`NpnoLevel::parse`), `main_voice_q23(note, velocity)` returns the Q23 amplitude of the main voice and `amplitude_q23(note, velocity, layer, bias)` any voice. It is validated by synthetic images that carry the root-60 lists of the three swept sounds (`(rec[0x5E], threshold)` and key sums, derived values only) and reproduces the swept amplitudes of all three sounds (`X_SWEEP` velocities for the Clavinet, every 4th velocity plus 127 for the others). `NpnoSound::firmware_level` holds it for `.npno` sounds and `PianoLayer::firmware_velocity_level` (default `false`) switches the note-on gain to `main_voice_q23(note, v) / main_voice_q23(note, 127)` (the existing touch / compressor / una corda scaling stays; the multisample velocity shaping is skipped because the firmware law already contains the hammer dynamics). **It was not made the default**: the model is only the main voice's level (the extra voices and the absolute scale are not mixed) and the SMP loudness chain after the level word is not part of the engine, so the default render and its hash are unchanged.

**Open.** The trigger of the extra voices (bias 20 / layer 1 / layer 2), the one unmatched upright case, the other formats (version words `2 / 5`, `[this + 0x20] != 0` branches), `b2` (zero in all swept sounds, source assumed from the loader copy `0xC00B8848`), and the mapping from the Q23 level word to the engine's output gain.

## 10. Extra voices, gain word, and the decision on the default (VERIFIED run / EMPIRICAL)

**Extra voice rule (VERIFIED, 96 of 96 random notes on four sounds).** The file byte `0x44` (class) decides whether a note-on starts a first extra voice: classes 1, 2, 3, 5, 6 (EGrand, EP1, Wurlitzer 1, Bright / Velvet Grand, Amber Upright) do, classes 4, 7, 14, 15, 16 (Clavinet D6, harpsichord, DigiGrand, Ballad EP, Marimba, Vibraphone) have the main voice only. The extra voice is on **layer 1** when the root has layer-1 strokes (Velvet Grand, Amber Upright), at the position `F` = record byte `0x33` of the main voice's record (the two-stage selection of `0xC00A2A54`: the plain velocity position matched only 22 of 24 upright notes, `F` matches 24 of 24, which also resolves the one unmatched case of section 9); otherwise on layer 0 at `T + 20` (EGrand, Bright Grand, EP1, Wurlitzer; the `+20` is `this[0x34]`, the voice object field `+8`). `NpnoLevel::voices(note, velocity)` returns the voice levels in call order (extra voice first); full-note lists equal the firmware's level calls 96 of 96 (Clavinet 24, Bright Grand 24, Amber Upright 24, Wurlitzer 24). All 38 factory files carry the version word `0x464`, so the firmware branches for the words 2 and 5 never run on them (not modelled); `this[0x20] != 0` occurs only as the extra layer-1 voice.

**Gain word (VERIFIED, two velocities).** The ARM writes the float `level_q23 / 2^23` of a voice to the SMP voice record (DM `0x9252A`; Clavinet D6 velocity 20: `0x3CC68BC0` = 0.0242 = 203311 / 2^23, velocity 100: `0x3E800000` = 0.25 = 2097152 / 2^23). The SMP scales the decoded stroke by that gain (`smp-voice.md`: unity gain gives `bus = y << 17`, engine amplitude `bus / 2^30` = `y / 8192`), so the voice gain on the engine's integer PCM (`y / 8192`) is `level_q23 / 2^23`, absolute, not relative to velocity 127. Further SMP-side slews (the other float words of the record, `0.979`, `1.0`) were not traced; the master and mix scaling after the bus are those of the engine.

**Engine opt-in.** `NpnoSound::firmware_velocity_gain` is now this absolute main-voice gain. The engine still plays one stroke chosen by its own zones; playing the extra voice needs a second voice with its own stroke (the record index is in `VoiceLevel::record`) and was not done, so the opt-in underestimates sounds of the two-voice classes.

**Emulator against hardware (`tools/emu_vs_hw.py --only piano_dry`, Velvet Grand, A:61, 25 s, local video; variant `firmware-piano-level` = `ns4 render-patch --firmware-piano-level`):**

| Variant | RMS dB (emu - hw) | centroid Hz | 6-band max | score |
|---|---|---|---|---|
| default | +11.8 | -61 | +6.1 (air, excluded) / +5.6 brilliance | 3.00 |
| firmware-piano-level | +6.5 | -64 | +5.5 brilliance | 3.02 |

The firmware gain brings the level 5.3 dB closer to the hardware recording (the performance velocity is unknown and the audio is lossy YouTube audio, so the absolute value is shape evidence only); the spectral shape is unchanged.

**Decision: not the default.** The criteria were main voice, extra voices and the gain mapping verified across sounds. The levels (main + extra voices, 96 of 96) and the gain word are verified, but the engine does not yet play the extra voice (a second stroke per note on five classes), the SMP slews after the level word are not modelled, and the legacy render golden covers velocity shaping that the firmware law replaces; flipping the default would change every `.npno` piano render without the extra voice that the same law requires. The switch stays opt-in (`PianoLayer::firmware_velocity_level`, `ns4 render-patch --firmware-piano-level`); `render_hash` is unchanged.

## Implications

- The chain load -> select (bind) -> note-on -> voice start -> level method now runs on real firmware code with two library stubs and one DSP-ready hook; velocity reaches the level method as `X` and as the stroke record choice.
- `piano/level_q23.rs` needs only a velocity -> (stroke, X) front end to be complete; the law behind it matches the firmware for this file and key.

## Follow-ups

1. Decode the binder `0xC00A3988` (threshold conversion, key blocks, `b1..b3` from the information block) and the grand / upright branches of `0xC00A2A54`; then wire the level model into `PianoLayer` as an opt-in.
2. Sweep every key and layer to replace the interpolation (the key byte sum is not smooth: 19 at note 96 for one upright).
3. Find which sound class selects `0xC00A1D5C` (rows 1..3).
4. A second note blocks in the same process: model the DSP link handshake properly (pin and timing of `0xC006CCBC`).
