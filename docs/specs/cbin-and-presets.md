# Clavia Binary (`CBIN`), Backup (`.ns4b`), and Firmware Container Specification

## 1. Scope

Defines the binary file formats used by the Nord Stage 4 (`product_id=46`) for programs, engine presets, system settings, factory/user backups (`.ns4b`), and OS firmware updates (`Update Nord Stage 4 OS v1.66`).

Implemented in [`crates/nord-core/src/cbin.rs`](../../crates/nord-core/src/cbin.rs), [`crates/nord-core/src/firmware.rs`](../../crates/nord-core/src/firmware.rs), and [`tools/ns4_inspect.py`](../../tools/ns4_inspect.py).

---

## 2. Backup Container (`.ns4b`)

A `.ns4b` archive is a standard PKZIP (`50 4B 03 04`) archive containing a root `meta.xml` manifest and categorized folders:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<backup file_version="1" backup_format_version="1" product_id="46" product_version="166" product_build="2934" product_content_version="0" manager_version="918" manager_build="1444"/>
```

### Archive Directory Structure (Factory Restore RevK v1.66)

| Directory | Extension | Count | Exact Size (Bytes) | Description |
|---|---|---:|---:|---|
| `Program/Bank A..H/` | `.ns4p` | 384 | `868` | Full 7-layer Stage 4 Programs (8 Banks × 48 Programs) |
| `Live/Bank 1/` | `.ns4l` | 8 | `868` | Live buffer slots (`Live 0.ns4l` .. `Live 7.ns4l`) |
| `Synth Preset/Bank 1..8/` | `.ns4y` | 384 | `541` | Single-layer Synth Engine + Synth FX presets |
| `Piano Preset/Bank 1..2/` | `.ns4n` | 96 | `195` | Single-layer Piano Engine + Piano FX presets |
| `Organ Preset/Bank 1..2/` | `.ns4o` | 64 | `183` | Single-layer Organ Engine + Organ FX presets |
| `Settings/Bank 1/` | `.ns4t` | 1 | `124` | Global system/pedal/MIDI settings (`Settings.ns4t`) |
| `Piano/*/` | `.npno` | 38 | Variable | Nord Piano Library v6+ compressed multisamples |
| `Sample/*/` | `.nsmp4` | 195 | Variable | Nord Sample Library 4 compressed sample banks |

---

## 3. Clavia Binary (`CBIN`) 48-Byte Header Specification

Every `.ns4p`, `.ns4l`, `.ns4y`, `.ns4n`, `.ns4o`, and `.ns4t` file begins with a fixed **48-byte (`0x30`) header** followed by a bit-packed payload (`+0x30..EOF`).

| Offset | Size | Endian | Field | Description |
|---|---|---|---|---|
| `0x00` | 4 B | ASCII | `magic` | Always `"CBIN"` (`43 42 49 4E`) |
| `0x04` | 4 B | `u32` LE | `format_version` | Container version (`1` = `01 00 00 00`) |
| `0x08` | 4 B | ASCII | `tag` | 4-char type tag: `"ns4p"`, `"ns4l"`, `"ns4y"`, `"ns4n"`, `"ns4o"`, `"ns4t"`, `"npno"`, `"nsmp"` |
| `0x0C` | 2 B | `u16` LE | `bank_index` | Zero-based bank index (`0` = Bank A / Bank 1) |
| `0x0E` | 2 B | `u16` LE | `location_index` | Zero-based program/preset index within the bank (`0..47`) |
| `0x10` | 4 B | `u32` LE | `category_id` | Musical category enum (`0x00` = None/Live, `0x12` = Organ, `0x16` = Upright/Grand, `0x17` = EPiano, `0x2C` = Bass, `0x2D` = Synth, `0x2E` = Pad, `0x2F` = Pluck/Acoustic) |
| `0x14` | 4 B | `u32` LE | `schema_id_le` | Payload schema version (matches big-endian repeat at `0x2E`) |
| `0x18` | 4 B | `u32` LE | `checksum` | 32-bit payload checksum / CRC over `+0x2E..EOF` |
| `0x1C` | 18 B | `[u8; 18]` | `reserved` | All zeros (`0x00`) in preset files |
| `0x2E` | 2 B | `u16` BE | `schema_id_be` | Big-endian repeat of `schema_id` (`0x0139` for `ns4p`/`ns4l`, `0x00D0` for `ns4y`, `0x00CD` for `ns4o`, `0x00CB` for `ns4n`, `0x006C` for `ns4t`) |
| `0x30` | `N` B | Bitstream | `payload` | Bit-packed preset state (`820` B for `ns4p`/`ns4l`, `493` B for `ns4y`, `147` B for `ns4n`, `135` B for `ns4o`, `76` B for `ns4t`) |

### Schema ID Table

| File Type | Tag | Total File Size | Payload Size (`+0x30`) | `schema_id` (`+0x14` LE / `+0x2E` BE) |
|---|---|---:|---:|---|
| Program | `ns4p` | 868 B | 820 B | `0x0139` (313) |
| Live Slot | `ns4l` | 868 B | 820 B | `0x0139` (313) |
| Synth Preset | `ns4y` | 541 B | 493 B | `0x00D0` (208) |
| Piano Preset | `ns4n` | 195 B | 147 B | `0x00CB` (203) |
| Organ Preset | `ns4o` | 183 B | 135 B | `0x00CD` (205) |
| Settings | `ns4t` | 124 B | 76 B | `0x006C` (108) |

---

## 4. Bitstream Embedding Inside `.ns4p` / `.ns4l`

Empirical comparison between `Organ Preset/Bank 1/B3 Basic.ns4o` and `Live/Bank 1/Live 1.ns4l` (and factory `.ns4p` programs) shows that a full `.ns4p` program embeds the exact Organ, Piano, and Synth preset bitstreams:

1. **Global Program Header (`0x30..0x5C`, 45 bytes):**
   - Master Clock BPM, Vibrato/Transpose, Split Point enables (3 split points: Low, Mid, High) and crossfade widths (`Off`, `Small`, `Large`).
   - Layer Scene I and Layer Scene II active bitmasks (7 bits each: `Organ A/B`, `Piano A/B`, `Synth A/B/C`).
   - Global Master Compressor amount and mode.
2. **Organ Section (`0x5D..0xE3`, 135 bytes):**
   - Matches the 135-byte payload of `.ns4o` (`0x30..0xB7` of an `.ns4o` file):
   - Signature header `11 27 fb fb fb ff f7 f7 f7` followed by Organ model (`B3`, `Vox`, `Farf`, `Pipe1`, `Pipe2`), Layer A & Layer B 9-drawbar arrays (nibbles `0..=8`), percussion switches, vibrato/chorus selector, and per-layer Organ FX + Rotary 122 parameters.
3. **Piano Section (`0xE4..0x176`, 147 bytes):**
   - Matches the 147-byte structure of `.ns4n` for Piano Layers A and B (piano category, bank/sample hash ID, timbre preset, string resonance enable, pedal noise enable, dynamic compression, and per-layer Piano FX).
4. **Synth Section (`0x177..0x363`, 493 bytes):**
   - Matches the 493-byte structure of `.ns4y` across Synth Layers A, B, and C (oscillator category, waveform/wavetable ID, FM/detune, ZDF filter type/cutoff/resonance/kb-track, ADR envelopes, LFOs, arpeggiator, and per-layer Synth FX).

---

## 5. OS v1.66 Firmware Container Layout

The Windows updater (`Update Nord Stage 4 OS v1.66.exe`) embeds the firmware payload in PE Resource `RCDATA 101` (4,037,080 bytes = `0x3D99D8`).

### Top-Level Dual-Image Header (`0x0000..0x0020`)

| Offset | Size | Value (v1.66) | Meaning |
|---|---|---|---|
| `0x0000` | `u32` BE | `0x00000020` | Header size (`32` bytes) |
| `0x0004` | `u32` BE | `0x00368F86` | Image 0 (Main OS + DSP bundle) byte length (`3,575,686` bytes) |
| `0x0008` | `u32` BE | `0x00368FA6` | Image 1 (Boot/Updater OS) start offset (`0x20 + 0x368F86`) |
| `0x000C` | `u32` BE | `0x00070A32` | Image 1 byte length (`461,362` bytes; `0x368FA6 + 0x070A32 == 0x3D99D8`) |
| `0x0010` | 16 B | `0` | Zero padding |

### Image Segment Table (`+0x00` of each Image)

Each image begins with a 20-byte sub-header (`version` = `0x0064` = `100`, `segment_count` = `3` at `+0x10` BE), followed by three 44-byte (`0x2C`) segment descriptors:
1. **`SRAM`**: Target SRAM load address, size, and CRC.
2. **`CODE`**: Compressed/executable code region metadata.
3. **`ARM0`**: Primary ARM host controller image.

Within the `CODE` / `ARM0` data payload of Image 0 (around `0x003CF040`), the embedded Analog Devices SHARC DSP cores are indexed by their module tags:
- **`DSPF`** (`DSPFxCore1 L1 bl.0`): Effects & Piano Resonance DSP core.
- **`DSPO`** (`DSPOrg`): Organ Physical Modeling & Rotary Speaker DSP core.
- **`DSPS`** (`DSPSyn`): 3-Layer Nord Wave 2 Synth DSP core.
