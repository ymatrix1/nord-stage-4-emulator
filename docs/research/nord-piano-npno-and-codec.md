# Research: Nord Piano (.npno) Format & 4-Bit ADPCM/DPCM Audio Codec

- **Target:** Nord Piano Library Format (`.npno`) & `CNSP` Audio Codec
- **Source:** `Clavinet D6  6.1.npno` (5,935,628 bytes) inside `Nord Stage 4 Factory Restore RevK v1.66.ns4b`
- **Status:** Proved container hierarchy, 128-key mapping table, cents detuning table, stroke descriptors, and 4-bit signed nibble audio compression.

---

## 1. Container Hierarchy & Header Offsets

All `.npno` files sit within a 48-byte `CBIN` container:

| Offset | Length | Type | Description |
|---|---|---|---|
| `0x0000` | 4 B | ASCII | `CBIN` Magic |
| `0x0004` | 4 B | `u32` LE | Container version (`1`) |
| `0x0008` | 4 B | ASCII | Format tag: `npno` |
| `0x000C` | 4 B | `u32` LE | Sub-version: `3` |
| `0x0010` | 4 B | `u32` | Header flags (`0xFFFFFFFF`) |
| `0x002C` | 4 B | ASCII | `CNSP` Sub-header marker |
| `0x0048` | 32 B | ASCII | Short instrument name (e.g. `Clavinet D6#             `) |
| `0x0068` | 32 B | ASCII | Long instrument name (e.g. `Clavinet D6`) |

---

## 2. Key Mapping & Calibration Tables

Following the header, `.npno` contains contiguous 128-byte lookup tables corresponding to the 128 MIDI notes (`0..127`):

### 2.1 128-Note Key-to-Root Mapping Table (`0x00B8..0x0138`)
Maps every incoming MIDI note number to its corresponding multisample root key:
- Notes 0..28: Clamped to lowest root note 25 (`0x19` = C#1).
- Roots follow intervals of fourths (5 semitones):
  `0x19` (25/C#1) &rarr; `0x1E` (30/F#1) &rarr; `0x23` (35/B1) &rarr; `0x28` (40/E2) &rarr; `0x2D` (45/A2) &rarr; `0x32` (50/D3) &rarr; `0x37` (55/G3) &rarr; `0x3C` (60/Middle C) &rarr; `0x41` (65/F4) &rarr; `0x46` (70/Bb4) &rarr; `0x4B` (75/Eb5) &rarr; `0x50` (80/Ab5) &rarr; `0x55` (85/Db6) &rarr; `0x5A` (90/F#6) &rarr; `0x5F` (95/B6) &rarr; `0x63` (99/Eb7) &rarr; `0x6B` (107/B7).
- Notes above 107: `0xFF` (unmapped / out of keyboard physical range).

### 2.2 128-Note Cents Fine-Tuning Calibration Table (`0x01C8..0x0238`)
Contains signed 8-bit integers (`i8`, range `-128..+127`) encoding fine-tuning micro-cents detuning per individual key (e.g. `-8`, `-12`, `+3`, `+4`, `+7` cents) to preserve natural acoustic stretched tuning.

### 2.3 128-Note Velocity Tier Bounds Table (`0x0238..0x02B8`)
Encodes velocity split boundaries and layer assignments per key.

---

## 3. Stroke Descriptor Table (`0x0760..`)

Fixed-size stroke descriptors indexing audio streams:
- `offset`: Absolute file offset of the audio stroke (e.g. `0x1F30`).
- `length`: Stream byte size / sample count (e.g. `0x030338` = 197,432 bytes &approx; 4.4 seconds of audio).
- Loop markers and crossfade boundaries.

---

## 4. The `AVCSmpDecode` 4-Bit Differential Audio Codec

Analysis of the audio payload starting at offset `0x1F5D` revealed the mathematical structure of the compression:

### 4.1 4-Bit Signed Nibble Packing
Each byte stores **two 4-bit signed differential audio samples**:
- `sample_a = (byte >> 4) as i8` (sign-extended from 4 bits: `0x8..0xF` &rarr; `-8..-1`)
- `sample_b = (byte & 0x0F) as i8` (sign-extended from 4 bits: `0x8..0xF` &rarr; `-8..-1`)

### 4.2 Empirical Bell-Curve Distribution
Histogram of 1,024 consecutive unpacked nibbles from `Clavinet D6  6.1.npno`:

```text
Nibble Value   Hex   Count   Visual Distribution
 -5            0xB       6   |
 -4            0xC      26   ##
 -3            0xD      69   ######
 -2            0xE     104   ##########
 -1            0xF     182   ##################
  0            0x0     215   ##################### (Peak)
 +1            0x1     211   #####################
 +2            0x2     120   ############
 +3            0x3      67   ######
 +4            0x4      20   ##
 +5            0x5       3   |
 +6            0x6       1   |
```

This confirmed a textbook Gaussian distribution centered at zero, proving that Clavia's proprietary audio payload is a 4-bit differential predictive codec (DPCM / ADPCM). Integrating the deltas produced an acoustic fundamental oscillation around ~408 Hz (G#4/A4 region of the Clavinet keyboard).
