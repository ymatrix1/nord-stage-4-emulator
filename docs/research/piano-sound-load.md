# Loading a piano sound object in the ARM emulation

- **Checked:** 2026-11-13
- **Sources:** `target/fw_unpacked/bank0_CODE.bin` and `bank0_SRAM.bin` (OS v1.66, local only); `Clavinet D6  6.1.npno` read from the local factory backup (`.reference/`, never committed); `tools/arm_piano_load.py` (this note's harness) on top of `tools/arm_keydown.py` and `tools/arm_steady.py`; [keydown-paths](keydown-paths.md) sections 4 and 5c (PR #137); [nord-piano-npno-and-codec](nord-piano-npno-and-codec.md).
- **Question:** can the harness load a real sound object so that a MIDI note-on reaches the piano voice start `0xC00A9274` and sends SMP/PNO messages, so that velocity -> level (call-frame word of `0xC00A1D5C`) can be compared with `piano/level_q23.rs`?

**Answer (second pass):** the load, the bind to the core and the voice start all run through real firmware code: stubs for the library volume, the firmware's own select method `0xC00AAA58`, and a DSP-ready model for one GPIO poll. A note-on 60 reaches `piano_voice_frame` and `piano_voice_start` (2 calls each, note-on and note-off) and sends 58 SMP path-1 messages. **Those messages are identical for velocities 1, 64, 127 and for notes 36, 60, 84**, and `0xC00A1D5C` is never entered, so no velocity -> level data exists yet and `piano/level_q23.rs` was not compared. Missing pieces are in section 4.

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

## 4. What is missing (stop point)

1. What fills `[P + 0x6424]` (kind, tested against 2, 5, 6, 1, 3 at `0xC00BCB28`) and the per-key tables the key path reads; the loader's 38 stroke records go to `P + 0x140..0x6A0` as small integers (3, 2, 1 at a 0x3C..0x40 stride) through `0xC00B4F4C` / `0xC00B551C`, which do not produce note-dependent output here. Forcing `[P + 0x6424]` to 0, 1, 2, 3 or 5 left the message list unchanged.
2. Why the velocity never reaches a message: the call frame of `0xC00A1D5C` was not captured; it has no static caller, so it is reached through a function pointer or table that this configuration does not use.
3. The real device header (`0..0xB0`) of a library file; the harness invents name and type word. The note path may depend on header fields (`D + 0xC` kind and the type word).
4. The DSP serial link is modelled as always ready; a closer model of the handshake might matter for later messages.

## 5. Reproduce

```bash
python tools/arm_piano_load.py target/fw_unpacked/bank0_CODE.bin target/fw_unpacked/bank0_SRAM.bin     --cache target/keydown.snap --ns4b "<backup>.ns4b" --npno "Piano/Clavinet/Clavinet D6  6.1.npno"     --note 60 --velocity 100
```

## Implications

- The chain load -> select (bind) -> note-on -> voice start now runs on real firmware code with two library stubs and one DSP-ready hook; the earlier blocker (`[core + 4]`) is closed.
- The remaining gap for the level comparison is data, not control flow: the piano object kind and per-key tables, or the missing trigger of the level procedure.

## Follow-ups

1. Trace readers of `[P + 0x6424]` and of the `P + 0x140..0x6A0` words to see which file field decides the kind; try the other `.npno` files (grand pianos, not the Clavinet) since a Clavinet may take a non-sample path.
2. Find the indirect path to `0xC00A1D5C` (function pointer tables in `0xC02A....` / vtables) and capture its call frame per velocity.
3. Replace the always-ready DSP model with the real handshake once its pin is identified.
