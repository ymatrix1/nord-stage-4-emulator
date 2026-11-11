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

1. **Global Program Header (`0x30..0x5C`, 45 bytes, tag `0x2C`):**
   - Split Zone enables, split note indices (`0..9 -> [41, 48, 53, 60, 65, 72, 77, 84, 89, 96]`), crossfade widths (`Off`, `Small`, `Large`), and `MasterClockRate` (`30 + u8` BPM).
   - `SectionEnable_I` (`284..287`, 3b `[Org, Pno, Syn]`) and `SectionEnable_II` (`287..290`, 3b `[Org, Pno, Syn]`) combined with each section's per-layer enables to derive the 7-bit `scene_1_mask` and `scene_2_mask`.
2. **Organ Section (`0x5D..0xE3`, 135 bytes, tag `0x11`):**
   - Matches the 135-byte payload of `.ns4o` (`0x30..0xB7` of an `.ns4o` file):
   - Two `0x03` Organ Layer sub-blocks (bits `144..391` and `392..639`), Organ Level A/B `7b` morph parameters, Rotary 122 (`enabled`, `speed`, `drive`), `VibratoTypeB3` (`0..5 -> V1, C1, V2, C2, V3, C3`), and 1× shared `0x36` Organ FX block (bytes `80..135`).
3. **Piano Section (`0xE4..0x176`, 147 bytes, tag `0x0C`):**
   - Matches the 147-byte structure of `.ns4n`: two `0x0B` Piano Layer sub-blocks (bits `104..199` and `200..295`), Level A/B `7b` morphs, and 2× `0x36` Piano FX blocks (bytes `37..92` and `92..147`).
4. **Synth Section (`0x177..0x363`, 493 bytes, tag `0x1E`):**
   - Matches the 493-byte structure of `.ns4y`: Level/Pan A/B/C, three `0x32` Synth Layer blocks (bits `248`, `656`, `1064`, stride `408` bits), three `0x2F` Synth Sound blocks (bits `1472`, `1856`, `2240`, stride `384` bits), and 3× `0x36` Synth FX blocks (bytes `328..383`, `383..438`, `438..493`).

### 4.1 Hardware Bitstream & Morph Encoding (OS v1.66)

Implemented in [`crates/nord-core/src/cbin/presets.rs`](../../crates/nord-core/src/cbin/presets.rs) (`BitReader`, `BitWriter`, `ProgramDecoded`) and [`tools/ns4_inspect.py`](../../tools/ns4_inspect.py):

#### `4W+3` Morph Encoding (MSB-first)
Morphable parameters of bit width `W` occupy `4W + 3` bits:
- `base: W` bits (`0..=max_val`, where `max_val = (1 << W) - 1` or `8` for drawbars).
- `wheel_delta: W+1` bits, `aftertouch_delta: W+1` bits, `ctrl_pedal_delta: W+1` bits, each stored biased by `+max_val` (`zero = max_val`, i.e., `8` for `W=4` drawbars, `127` for `W=7`, `16383` for `W=14` Delay Rate; signed normalized delta = `(raw - max_val) / max_val`).

#### Global Program Header (`0x2C`, `45` Bytes)
- `0..8`: block tag `0x2C`.
- `8`: `split_enabled` (1b); `9..12`: `split_zone_enabled[0..3]` (3×1b).
- `12..24`: 3×4b split note indices (`[41, 48, 53, 60, 65, 72, 77, 84, 89, 96]`).
- `24..30`: 3×2b crossfade width (`0=Off, 1=Small, 2=Large`).
- `35..43`: `MasterClockRate` (8b; BPM = `30 + val`).
- `284..287`: `SectionEnable_I` (`bit2=Org, bit1=Pno, bit0=Syn`); `287..290`: `SectionEnable_II`.

#### Organ Block (`0x11`, `135` Bytes = 2×`0x03` + `0x36`)
- `0..8`: `0x11`; `9..11`: `enable_i[B:A]`; `12..14`: `enable_ii[B:A]`.
- `14..45`: `level_a` (`W=7` morph); `45..76`: `level_b` (`W=7` morph).
- `80`: `rotary_enabled` (1b); `81..83`: `leslie_speed` (`0|2=Stop, 1=Slow, 3=Fast`); `92..99`: `rotary_drive` (7b); `130..133`: `VibratoTypeB3` (`0..5 -> V1, C1, V2, C2, V3, C3`).
- `0x03` Layers at bit `144` (A) and `392` (B): `+12..+16` signed 4b `octave` (`-2..+2`); `+17..+20` `model` (`0=B3, 1=Vox, 2=Farfisa, 3=Pipe1, 4=Pipe2`); `+40 + i*19` (`i=0..8`) drawbars (`W=4` morph, `max_val=8`); `+232` `vibrato_on`; `+233..+237` percussion `[on, third, fast, soft]`.
- Bytes `80..135`: `0x36` Organ FX chain.

#### Piano Block (`0x0C`, `147` Bytes = 2×`0x0B` + 2×`0x36`)
- `0..8`: `0x0C`; `9..11`: `enable_i[B:A]`; `12..14`: `enable_ii[B:A]`; `14..45`: `level_a` (`W=7` morph); `45..76`: `level_b` (`W=7` morph).
- `0x0B` Layers at bit `104` (A) and `200` (B): `+12..+16` signed 4b `octave`; `+18..+21` `category` (`0..5`: Grand, Upright, Electric, Clav, Digital, Misc); `+21..+26` `model_id`; `+28..+30` `variation`; `+30..+62` 32b `name_hash`; `+62..+64` `acoustic_level` (string resonance `0..3`); `+64` `pedal_noise`; `+65..+67` `kb_touch` (`0=Light, 1=Medium, 2=Heavy`); `+67..+69` `unison`; `+69..+71` `dyn_comp`; `+71..+74` `timbre` (`0=Off, 1=Soft, 2=Mid, 3=Bright, 4=Dyno1, 5=Dyno2`).
- Bytes `37..92` & `92..147`: `0x36` Piano FX chains A & B.

#### Synth Block (`0x1E`, `493` Bytes = 3×`0x32` + 3×`0x2F` + 3×`0x36`)
- `0..8`: `0x1E`; `10..13`: `enable_i[C:B:A]`; `15..18`: `enable_ii[C:B:A]`.
- `18, 49, 80`: `level_{a,b,c}` (`W=7` morphs); `111, 142, 173`: `pan_{a,b,c}` (7b).
- `0x32` Layer blocks (`248, 656, 1064`, stride `408`): `+8` `is_osc_mode` (1=Osc, 0=Sample); `+32..+64` 32b `sample_id`; `+135` `arp_run`; `+175..+206` `arp_rate` (`W=7` morph, `30..300 BPM`).
- `0x2F` Sound blocks (`1472, 1856, 2240`, stride `384`): `+8..+11` `osc_bank`; `+11..+15` `osc_cat` (`8=FM`); `+15..+25` `osc_wave` (10b); `+25..+56` `osc_ctrl` (`W=7` morph); `+133..+164` `lfo_rate` (`W=7` morph); `+164..+195` `lfo_amount` (`W=7` morph); `+195..+216` ADR env (`3×7b`); `+218..+221` `filter_type` (`0|3=LP12, 1=LP24, 2=Ladder24, 4=BP, 5=HP`); `+221..+252` `filter_freq` (`W=7` morph, `80 Hz * 200^norm`); `+252..+283` `filter_res` (`W=7` morph); `+283..+285` `kb_track`; `+285..+287` `filter_drive`; `+287..+318` `filter_env_amt` (`W=7` morph).
- Bytes `328..383`, `383..438`, `438..493`: `0x36` Synth FX chains A, B, C.

#### Per-Layer FX Block (`0x36`, `55` Bytes = `440` Bits)
- `0..8`: `0x36`; `8`: `mod1_on`; `9`: `mod1_sync`; `10..41`: `mod1_rate` (`W=7` morph); `41..72`: `mod1_amt` (`W=7` morph); `72..76`: `mod1_type` (`0..11`: AutoPan Soft/Hard, Trem Soft/Hard, RingMod/St, A-Wah Cry/Env, Wah LFO/Pedal, Pump LFO/Pedal).
- `76`: `mod2_on`; `77..108`: `mod2_rate` (`W=7` morph); `108..139`: `mod2_amt` (`W=7` morph); `139..143`: `mod2_type` (`0..11`: Phaser I/II, Flanger St/Mono, Chorus Analog, Vibe St/Mono, Ensemble St/Mono, Spin Treble/Bass/Both).
- `143`: `amp_eq_on`; `144..151`, `151..158`, `158..165`: signed 7b EQ Treble/Mid/Bass (`* 0.25` dB, `-15..+15 dB`); `165..196`: `mid_freq` (`W=7` morph); `196..227`: `drive` (`W=7` morph); `408..417`: `amp_type_new`.
- `231`: `comp_on`; `232..239`: `comp_amt` (7b); `239`: `comp_fast`.
- `240`: `delay_on`; `241`: `delay_sync`; `242..301`: `delay_rate` (`W=14` morph, `zero=16383`); `301..332`: `delay_mix` (`W=7` morph); `332..334`: `delay_mode`; `334`: `ping_pong`; `335`: `fb_filter`; `336..367`: `delay_fb` (`W=7` morph); `367..371`: `fb_fx_type`.
- `371`: `reverb_on`; `372..403`: `reverb_amt` (`W=7` morph); `403..405`: `reverb_color`; `405..408`: `reverb_type` (`0..5`: Spring, Booth, Room, Stage, Hall, Cathedral).

> All decoded structs retain their original `raw_payload` bytes when packed back, guaranteeing bit-identical round-tripping for unedited fields and reserved firmware state.

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
1. **`SRAM`**: Target SRAM load address, size, and one's-complement checksum (uncompressed).
2. **`CODE`**: **LZO1X**-compressed ARM926EJ-S & SHARC executable memory image (`target_address = 0xC0000000`).
3. **`ARM0`**: Primary ARM host controller bootstrap/stub image (uncompressed).

Segment `data_offset` values are relative to each bank's base offset in the firmware container:
- **Bank 0 (Image 0):** relative to `header_size` (`0x0020`).
- **Bank 1 (Image 1):** relative to `image1_offset` (`header_size + image0_size`, `0x368FA6` in OS v1.66).

Within the decompressed `CODE` payload of Image 0 (and mirrored references in the container), the embedded Analog Devices SHARC DSP cores are indexed by their module tags:
- **`DSPF`** (`DSPFxCore1 L1 bl.0`): Effects & Piano Resonance DSP core.
- **`DSPO`** (`DSPOrg`): Organ Physical Modeling & Rotary Speaker DSP core.
- **`DSPS`** (`DSPSyn`): 3-Layer Nord Wave 2 Synth DSP core.

### 5.1 Clavia LZO1X Stream Compression (`CODE` Segments)

The `CODE` segments inside OS firmware images are compressed using standard **LZO1X** (`crates/nord-core/src/firmware.rs`), verified bit-exact against both OS v1.66 banks:
- **Bank 0 `CODE`:** `0x358D45` compressed bytes &rarr; `5,499,504` (`0x53EA70`) decompressed bytes, checksum `0xDE902503` (`OK`).
- **Bank 1 `CODE`:** `0x0642D1` compressed bytes &rarr; `798,912` (`0x0C30C0`) decompressed bytes, checksum `0xFBB813F8` (`OK`).

#### LZO1X Token Encoding Summary
- **First byte (`t > 17`):** Initial literal run of `t - 17` bytes at stream start.
- **`0..15` (`M1` / literal run):** `t == 0` encodes an extended literal run (`18 + sum(0xFF * 255) + next_nonzero` bytes; e.g. `00 1B` = `18 + 27 = 45` bytes for the ARM926EJ-S exception vectors + constant pool header); `1..=15` encodes a short M1 back-reference when preceded by a match, or a `3 + t` literal run otherwise.
- **`16..31` (`M4` / EOF):** `length = (t & 7)` (extended if `0`), `offset = 16384 + ((t & 8) << 11) + ((le16 >> 2) & 0x3FFF)`; `offset == 16384` (`11 00 00`) marks end-of-stream.
- **`32..63` (`M3`):** `length = (t & 31)` (extended if `0`) `+ 2`, `offset = 1 + ((le16 >> 2) & 0x3FFF)`.
- **`64..255` (`M2`):** `length = ((t >> 5) - 1)`, `offset = 1 + ((t >> 2) & 7) + (next_byte << 3)` (`1..=2048` bytes).
- **Trailing literal nibble:** The low 2 bits (`last & 0x03`) of the two-byte match operand supply `0..=3` trailing literal bytes after every M2/M3/M4 match; an M2 match uses `last = next_byte` in the same way? Specifically, `t & 0x03` of the first M2 byte (`byte0 & 3`) gives trailing literals, whereas M3/M4 use the low 2 bits of the 16-bit LE operand's low byte (`op0 & 3`).

#### Checksum Algorithm
Each segment descriptor stores a 32-bit one's-complement byte checksum:
```text
checksum = 0xFFFFFFFF - (sum(segment_bytes) & 0xFFFFFFFF)
```
For uncompressed segments (`SRAM`, `ARM0`), the checksum matches the raw file segment payload. For compressed segments (`CODE`), the checksum is computed over the fully decompressed memory image at runtime (`target_address = 0xC0000000`).

#### Rust API (`crates/nord-core/src/firmware.rs`) & CLI
- `calculate_checksum(bytes: &[u8]) -> u32`
- `decompress_clavia_lz(input: &[u8], max_output_size: usize) -> Result<Vec<u8>, String>`
- `compress_clavia_lz(input: &[u8]) -> Vec<u8>`
- `FirmwareContainer::bank_base(&self, bank: u8) -> Result<usize, String>`
- `FirmwareContainer::extract_segment(&self, bank: u8, name: &str, raw: &[u8]) -> Result<Vec<u8>, String>`
- `FirmwareSegment::extract_payload(&self, raw: &[u8]) -> Result<Vec<u8>, String>` (Bank 0 base `0x20`)
- `FirmwareSegment::extract_payload_from(&self, bank_base: usize, raw: &[u8]) -> Result<Vec<u8>, String>`
- `FirmwareSegment::verify_checksum(&self, payload: &[u8]) -> bool`
- `FirmwareContainer::segment(&self, name: &str) -> Option<&FirmwareSegment>`
- `FirmwareContainer::segment_in_bank(&self, bank: u8, name: &str) -> Option<&FirmwareSegment>`
- `DspEngineType` (`Organ`, `Synth`, `Effects`, `Routing`, `CoreIdentifier`)
- `DspPartition` (full partition descriptors with engine categorization)
- CLI: `ns4 inspect <fw_bin> --verify --unpack <out_dir>` verifies every segment's checksum and extracts `bank{0,1}_{SRAM,CODE,ARM0}.bin`.

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
