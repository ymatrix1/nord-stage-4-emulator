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

#### 2.1 Manifest Parsing Robustness
The manifest parser in [`crates/nord-core/src/cbin/manifest.rs`](../../crates/nord-core/src/cbin/manifest.rs) (`BackupManifest::parse_xml`):
- Requires the root `<backup ...>` element tag, rejecting non-manifest XML documents.
- Supports both double-quoted (`"46"`) and single-quoted (`'46'`) attribute values.
- Permits arbitrary whitespace around assignment operators (e.g. `product_id = '46'`).
- Validates that all integer attributes are non-negative decimal strings and that quote delimiters are strictly matched.

#### 2.2 Bitstream Reader and Writer Invariants
The bit-level streaming codecs in [`crates/nord-core/src/cbin/bitstream.rs`](../../crates/nord-core/src/cbin/bitstream.rs) (`BitReader`, `BitWriter`):
- Use checked arithmetic (`checked_add`, `checked_mul`) for all bit-offset calculations to guarantee immunity against integer overflow on 32-bit and 64-bit architectures.
- Validate cursor seeks (`set_bit_pos(pos)`) against the total bit length of the payload, returning `CbinError::BitstreamError` if `pos > total_bits`.
- Guard reads (`read_bits(count)`) and writes (`write_bits(value, count)`) against buffer overruns and enforce the 32-bit word boundary limit (`count <= 32`).

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

### 4.1 Bit-Field Decoding & Mapping Table

Implemented in [`crates/nord-core/src/cbin.rs`](../../crates/nord-core/src/cbin.rs) (`BitReader`, `BitWriter`, `ProgramDecoded`) and [`tools/ns4_inspect.py`](../../tools/ns4_inspect.py):

#### Organ Payload (`135` Bytes)
- **Signature (`0..71` bits):** Fixed 9-byte sequence `11 27 FB FB FB FF F7 F7 F7`.
- **Model (`72..74` bits, 3 bits):** `0 = B3`, `1 = Vox`, `2 = Farfisa`, `3 = Pipe1`, `4 = Pipe2`.
- **Percussion (`75..78` bits, 4 bits):** `[on: 1b, soft: 1b, fast: 1b, third: 1b]`.
- **Vibrato/Chorus (`79..82` bits, 4 bits):** `[on: 1b, type: 3b]` (`0..5` -> `V1, V2, V3, C1, C2, C3`).
- **Rotary 122 (`83..91` bits, 9 bits):** `[speed: 2b]` (`0=Stop, 1=Slow, 2=Fast`) + `[drive: 7b]` (`0..127` -> `0.0..10.0`).
- **Layer A (`92..138` bits, 47 bits):** `[enabled: 1b, level: 7b, octave: 3b (-2..+2), drawbars: 9 * 4b (0..8)]`.
- **Layer B (`139..185` bits, 47 bits):** `[enabled: 1b, level: 7b, octave: 3b (-2..+2), drawbars: 9 * 4b (0..8)]`.

#### Piano Payload (`147` Bytes)
- **Signature (`0..71` bits):** Fixed 9-byte sequence `0C 27 1B FB FB FD A7 F7 F7`.
- **Layer A & Layer B (each 37 bits):**
  - `enabled: 1b`, `level: 7b`, `octave: 3b (-2..+2)`
  - `category: 3b` (`Grand, Upright, Electric, Clav, Digital, Misc`)
  - `model_id: 8b`, `timbre: 3b` (`Off, Soft, Mid, Bright, Dyno1, Dyno2`), `touch: 2b` (`Light, Med, Heavy`)
  - `string_res: 1b`, `pedal_noise: 1b`, `soft_release: 1b`, `dyn_comp: 2b (0..3)`, `unison: 2b (0..3)`.

#### Synth Payload (`493` Bytes)
- **Signature (`0..71` bits):** Fixed 9-byte sequence `1E 08 71 BF BF BF D2 7F 7F`.
- **Layer A, B, C (each 83 bits):**
  - `enabled: 1b`, `level: 7b`, `octave: 3b (-2..+2)`
  - `osc_mode: 2b` (`Analog, Wavetable, FM, Sample`), `waveform: 2b` (`Saw, Pulse, Triangle, Sine`), `wave_id: 8b`, `detune: 7b` (`0..10.0`)
  - `filter_model: 3b` (`LP12, LP24, Ladder24, BP, HP`), `cutoff: 7b` (`20 Hz..20 kHz`), `resonance: 7b`, `drive: 7b`, `kb_track: 2b`
  - `env_attack: 7b` (`1..5000 ms`), `env_decay: 7b` (`10..10000 ms`), `env_release: 7b` (`10..10000 ms`)
  - `lfo_rate: 7b` (`0.1..30 Hz`), `arp_enabled: 1b`, `arp_bpm: 8b` (`30..300 BPM`).

#### Global Program Header (`45` Bytes)
- **Signature (`0..7` bits):** Fixed byte `0x2C`.
- **Master Tempo (`8..16` bits, 9 bits):** `30..300 BPM`.
- **Master Compressor (`17..23` bits, 7 bits):** `0.0..10.0`.
- **Transpose (`24..27` bits, 4 bits):** `-6..+6` semitones.
- **Split Zones (`28..55` bits, 28 bits):** `split_enabled: 1b`, 3x `[split_note: 7b, crossfade: 2b (Off/Small/Large)]`.
- **Layer Scenes (`56..69` bits, 14 bits):** `scene_1_mask: 7b`, `scene_2_mask: 7b` (`Organ A/B, Piano A/B, Synth A/B/C`).

### 4.2 CLI Patch Conversion & Inspection

```bash
# Convert a binary .ns4p program to human-readable JSON:
ns4 convert patch.ns4p --to-json patch.json

# Convert JSON back to a valid, checksummed .ns4p binary:
ns4 convert patch.json --to-ns4p patch.ns4p

# Inspect presets with decoded fields:
ns4 inspect patch.ns4p --json
python tools/ns4_inspect.py patch.ns4p --decode
```

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

### 5.1 Clavia 16-Bit LZ Stream Compression (`CODE` Segments)

The `CODE` segments inside OS firmware images are compressed using Clavia's proprietary 16-bit LZ sliding-window scheme (`crates/nord-core/src/firmware.rs`).

#### 16-Bit Match Token Bitfield Layout (Little-Endian `u16`)

```text
 15                                      3   2        0
┌─────────────────────────────────────────┬────────────┐
│ Distance in History Buffer (13 bits)    │ Length - 2 │
│ Range: 1 .. 8191 bytes                  │ 3 bits     │
│ (distance = token >> 3)                 │ 2..9 bytes │
└─────────────────────────────────────────┴────────────┘
```

- **Bits 2..0 (3 bits):** Match length minus 2 (`length = (token & 0x07) + 2`), encoding lengths from `2` to `9` bytes.
- **Bits 15..3 (13 bits):** Sliding-window back-reference distance (`distance = token >> 3`), allowing lookup up to `8,191` bytes behind the current write cursor.

#### Stream Framing & Flag Bytes
Stream framing groups up to 8 operations per 1-byte header:
- **Flag Bit = `1`:** 1 uncompressed literal byte follows.
- **Flag Bit = `0`:** 1 16-bit match token (2 little-endian bytes) follows.
- **Initial Extended Literal Run:** At stream start, a `0x00, L` sequence encodes an extended literal run of `17 + L` bytes (e.g. `00 1B` for the 44-byte ARM926EJ-S reset stub).
- Overlapping runs (where `length > distance`) dynamically repeat bytes in run-length fashion.

#### Checksum Algorithm
Each segment descriptor stores a 32-bit one's-complement byte checksum:
```text
checksum = 0xFFFFFFFF - (sum(segment_bytes) & 0xFFFFFFFF)
```
For uncompressed segments (`SRAM`, `ARM0`), the checksum matches the raw file segment payload. For compressed segments (`CODE`), the checksum is computed over the fully decompressed memory image at runtime (`target_address = 0xC0000000`).

#### Rust API (`crates/nord-core/src/firmware.rs`)
- `calculate_checksum(bytes: &[u8]) -> u32`
- `decompress_clavia_lz(input: &[u8], max_output_size: usize) -> Result<Vec<u8>, String>`
- `compress_clavia_lz(input: &[u8]) -> Vec<u8>`
- `FirmwareSegment::extract_payload(&self, raw: &[u8]) -> Result<Vec<u8>, String>`
- `FirmwareSegment::verify_checksum(&self, payload: &[u8]) -> bool`
- `FirmwareContainer::segment(&self, name: &str) -> Option<&FirmwareSegment>`
- `FirmwareContainer::segment_in_bank(&self, bank: u8, name: &str) -> Option<&FirmwareSegment>`
- `DspEngineType` (`Organ`, `Synth`, `Effects`, `Routing`, `CoreIdentifier`)
- `DspPartition` (full partition descriptors with engine categorization)

### 5.2 Nord Piano (`.npno`) Binary Specification & 4-Bit DPCM Audio Codec

Nord Piano files (`.npno`) store high-fidelity multisampled acoustic, electric, and digital pianos inside a 48-byte `CBIN` container header (`kind = "npno"`):

#### Layout of `.npno` Header & Tables
- `0x0000..0x0030`: 48-byte `CBIN` container header (`tag = "npno"`).
- `0x002C`: `CNSP` sub-header signature.
- `0x0048..0x0068`: Short instrument name (ASCII, 32 bytes).
- `0x0068..0x0088`: Long instrument name (ASCII, 32 bytes).
- `0x00B8..0x0138`: 128-byte lookup table mapping every MIDI note (`0..127`) to its multisample root key.
- `0x01C8..0x0248`: 128-byte lookup table of signed 8-bit integers (`i8`) representing micro-cents fine-tuning calibration per key.
- `0x0238..0x02B8`: 128-byte lookup table of velocity tier boundaries per key.
- `0x0760..`: Multisample stroke descriptor table (`data_offset`, `data_size`, `loop_start`, `loop_end`).

#### 4-Bit Differential PCM Audio Codec (`AVCSmpDecode`)
Audio payloads encode two 4-bit signed delta samples per byte:
```text
byte = (sample_hi << 4) | (sample_lo & 0x0F)
s[n] = s[n-1] + delta * scale
```
- `sample_hi`: High nibble (`(byte >> 4) & 0x0F`), sign-extended (`0x8..0xF` &rarr; `-8..-1`).
- `sample_lo`: Low nibble (`byte & 0x0F`), sign-extended (`0x8..0xF` &rarr; `-8..-1`).
- Compression ratio: 4:1 vs 16-bit PCM, 6:1 vs 24-bit PCM.

#### Rust API (`crates/nord-core/src/cbin/piano_sample.rs`)
- `NpnoInstrument::parse(bytes: &[u8]) -> Result<NpnoInstrument, String>`
- `NpnoInstrument::root_key_for_note(&self, note: u8) -> Option<u8>`
- `NpnoInstrument::cents_detune_for_note(&self, note: u8) -> i8`
- `decode_4bit_dpcm(raw_payload: &[u8], scale: f32) -> Vec<f32>`
- `encode_4bit_dpcm(pcm: &[f32], scale: f32) -> Vec<u8>`

### 5.3 NSMP Sample Library Metadata (`.nsmp4`)

The `.nsmp4`/`nsmp` assets store the `NSMP` marker at offset `0x2C`, then a 12-byte prefix at `0x30..0x3B`. The current metadata parser starts its tagged chunk stream at `0x3C`. Each chunk has a 4-byte tag, a big-endian `u32` chunk ID, a big-endian `u32` payload length, and that many payload bytes. The parser checks chunk bounds and rejects incomplete trailing headers.

The parser currently recognizes:

- `\0hdr`: name at payload offset 10 (up to 32 bytes); the parser also attempts to read an author string at offset 74 (up to 32 bytes), which is not yet independently confirmed.
- `\0map`: a 128-entry table of 10-byte records starting at offset 6, followed by 16-byte zone records starting at `0x524`.
- `\0stk`: stroke ID in the first four payload bytes; payload audio remains opaque.

These offsets are an initial metadata mapping based on the locally inspected `Kalimba_KG 4.1.nsmp4` sample from the factory backup and deterministic synthetic fixtures. They are not a complete NSMP or sample-audio specification. The parser preserves no raw chunk data in its high-level result, and does not decode audio. See [`cbin::sample`](../../crates/nord-core/src/cbin/sample.rs).
