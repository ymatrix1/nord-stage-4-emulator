# Loading a piano sound object in the ARM emulation

- **Checked:** 2026-11-13
- **Sources:** `target/fw_unpacked/bank0_CODE.bin` and `bank0_SRAM.bin` (OS v1.66, local only); `Clavinet D6  6.1.npno` read from the local factory backup (`.reference/`, never committed); `tools/arm_piano_load.py` (this note's harness) on top of `tools/arm_keydown.py` and `tools/arm_steady.py`; [keydown-paths](keydown-paths.md) sections 4 and 5c (PR #137); [nord-piano-npno-and-codec](nord-piano-npno-and-codec.md).
- **Question:** can the harness load a real sound object so that a MIDI note-on reaches the piano voice start `0xC00A9274` and sends SMP/PNO messages, so that velocity -> level (call-frame word of `0xC00A1D5C`) can be compared with `piano/level_q23.rs`?

**Answer:** the load half works with two stubs; the bind-to-core half and the per-key table half do not. No voice start, no SMP/PNO voice message, no velocity comparison. Missing pieces are listed in section 4.

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

## 2. Binding: nothing in the key path writes `[core + 4]`

`[core + 4]` with core `0x90BF5400` is read by `0xC00BC8AC`. Searches over the whole code image:

| Search | Result |
|---|---|
| `str rX, [rY, #4]` in `0xC00A9000..0xC00C2000` | 63 hits; none stores a sound object pointer into the core (all are list / queue / float / reset stores; the reset `0xC00AA9D4`, `0xC00C1420` store 0) |
| `str rX, [rY, #4]` within 8 instructions after `ldr rY, [rZ, #0xC]`, whole image | 28 hits, none in the piano manager or core code (the one near `0xC00A1D5C`, `0xC00A1ADC`, is a message-queue record) |

So the binder uses a store form not found here (`stm`, `strd`, a write through a helper) or sits outside the ranges searched. EMPIRICAL (disassembly scan, no hit). Not decoded.

## 3. Experiment: bind by hand, then play a note (INCONCLUSIVE)

After the real load (kind 0) `[core + 4]` was set to the loaded object by the harness (not a firmware path), a note-on 60 / velocity 100 was sent over the UART2 model and 40 ticks were run (`arm_keydown.capture_note`):

| Result | Value |
|---|---|
| Function entries | `piano_key_event` 2, `piano_event_slot3` 1, `midi_note_on_post` 1; `piano_voice_frame` 0, `piano_voice_start` 0 |
| DSP messages | 2 identical `SMP` path-1 words, `dm_word 0x90000000`, value `0x03992772` (ticks 3 and 4) |
| Per-key tables of the object (`P + key + 0xF3 / 0x173 / 0x3F3 / 0x473`, key 60) | 1 to 2 non-zero bytes out of 128 per table; zero at key 60 |

The `0x90000000` address is an ARM heap address, not an SMP DM word, so the two messages are the key handler running on an object whose per-key tables are empty; they are **not** a voice start and carry no level information. The per-key tables are not filled by the load (kind 0 or 2); a later step (activation of the sound for a program, or the binder of section 2) must build them from the information block. Not found.

## 4. What is missing (stop point)

1. The routine that binds a loaded object to the core (`[core + 4]`), and what triggers it (program load / piano select). Not in the load chain or the edit-id handlers tried in [keydown-paths](keydown-paths.md) 5c.
2. The routine that fills the per-key tables (`P + 0xF3 ...`) from the information block `P + 0x5899`. The tables are read by the level procedure `0xC00A1D5C` (`ldrsb [r2, #0xF3]`).
3. The real device header (`0..0xB0`) of a library file; the harness invents name and type word.
4. The sample data path: the stroke audio offsets (`u32` at `0x7E2 + 0x76 k`) are not read by the loader; the voice start probably reads them later through `0xC0094EA0`, which the stub already serves from the same image.

Without 1 and 2 the voice start `0xC00A9274` is not reached, so velocity -> X (call frame of `0xC00A1D5C`) was not captured and `piano/level_q23.rs` was not compared (it stays tagged as in [piano-dsp-path](piano-dsp-path.md)).

## Implications

- Sound files can be served to the real loader with only two stubs, so the library volume and its directory are not the blocker any more; the blocker is the select/bind and activation code between the pool object and the core.
- The pool is a 120-object array created at boot; op 4 of the manager rebuilds it by scanning both volumes, which would also need a directory listing stub (`0xC0096264` over an index) if the scan path is wanted.

## Follow-ups

1. Find the binder: trace writes to `[core + 4]` (`0x90BF5404`) while calling manager op 4 with a stubbed two-file library, or follow the callers of the piano client's select handler.
2. Trace readers of the pool object's `+0x5899` block to find the per-key table builder, then call it after `load_sound`.
3. With both done, run `arm_keydown.capture_note` and decode the `0xC00A1D5C` call-frame word per velocity.
