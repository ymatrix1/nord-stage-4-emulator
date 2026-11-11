# Research: `.nsmp4` and `.npno` `CNSP` Fixed-LPC Audio Codecs & `.ns4b` Archive Layout

- **Status:** Verified bit-exact across **all 195 `.nsmp4` files** (`7,418 / 7,418` strokes: `3,092` mono, `4,326` stereo) and **all 38 `.npno` files** (`1,430 / 1,430` page-count sums, `190 / 190` sampled stroke overlap checks across Grand, Upright, Electric, Digital, Clavinet, Harpsichord, and Misc; zero overlap errors, all decoded amplitudes bounded in `[-8191, +8191]`) inside `Nord Stage 4 Factory Restore RevK v1.66.ns4b`.
- **Supersedes:** Early 4-bit DPCM nibble hypothesis in [nord-piano-npno-and-codec.md](nord-piano-npno-and-codec.md).

---

## 1. `.ns4b` Backup Archive Container

A `.ns4b` backup is a standard **PKZIP** archive where all entries use compression method **`0` (`ZIP_STORED`, uncompressed)** (`{0}` across all entries in Factory Restore RevK v1.66):
- Root manifest: `meta.xml` (`BackupManifest` with `product_id="46"` for Nord Stage 4).
- Directory layout: `Program/`, `Organ/`, `Piano/`, `Synth/`, `Live/`, `Samp Lib/`.
- Because every `.npno` (`Compression=0`) and `.nsmp4` (`Compression=0`) entry sits stored verbatim in the ZIP stream, a zero-dependency local-header/central-directory parser can read or slice any asset directly from a user-supplied `.ns4b` file without external decompression crates.

---

## 2. Nord Sample Library 4 (`.nsmp4`) Codec (`NSMP` / `\0stk`)

Container prefix: 48-byte `CBIN` header (`kind = "nsmp"`), `NSMP` at `0x2C`, 12-byte prefix at `0x30..0x3C`, followed by big-endian tagged chunks (`tag: [u8; 4]`, `chunk_id: u32 BE`, `length: u32 BE`, `payload`).

### 2.1 Key & Zone Table (`\0map`)
- `+0x0006`: 128 entries &times; 10 bytes (`key_map[n] = payload[6 + n * 10 + 6]`).
- `+0x0524`: 16-byte big-endian zone records:
  - `rec[1]`: `vel_max`
  - `rec[2]`: `root_key`
  - `rec[3]`: `key_high`
  - `rec[4]`: `key_low`
  - `rec[10..14]`: `u32 BE` `stroke_id`
  - `rec[15]`: `vel_min`

### 2.2 Stroke Chunk (`\0stk`) Header (`+0x00..+0x44`, zero-padded to first nonzero frame word)
- `+0x00..+0x04` (`u32 BE`): `stroke_id` matching `\0map` zones.
- `+0x05` (`u8`): `root_key` (MIDI note).
- `+0x06..+0x08` (`u16 BE`): exact sample rate in Hz (observed `33,172 .. 37,056 Hz`, typically `~35,002 Hz`).
- `+0x08` (`u8`): channel count (`1` = mono, `2` = stereo).
- `+0x12..+0x16` (`u32 BE` `w1`), `+0x1B..+0x1F` (`u32 BE` `w2`), `+0x24..+0x28` (`u32 BE` `w3`), `+0x2D..+0x31` (`u32 BE` `w4`): flash `u32`-word offsets (`9`-byte stride blocks; relative byte offset inside `\0stk` after `hdr_end` is `(w_i - w1) * 4`).
- `+0x29..+0x2B` (`u16 BE`): `0x0000` on sustain-looped instruments; non-zero (e.g. `0x1118`, `0x1241`) on one-shot/percussive instruments.
- `+0x39..+0x3D` (`f32 BE`): gain in dB (`-8.734 .. +14.893`).
- `+0x3E..+0x42` (`f32 BE`): peak/scale metadata.
- Leading `0x00000000` padding words extend from `+0x44` until the first non-zero 32-bit BE frame header word (`hdr_end` observed `0x68..0xD0`).

### 2.3 Variable-Length Fixed-LPC Audio Frames (`hdr_end..EOF`)
Each frame starts with a 32-bit big-endian control word `w`:
- **End-of-stream sentinel:** Last 4 bytes of `\0stk` (`p + 4 == payload.len()`, typically `0x00800020` mono / `0x00800040` stereo).
- `ns = w & 0x3FFF`: sample count for this frame (`spc = ns` for mono; `spc = ns / 2` stereo pairs for stereo).
- `order = (w >> 14) & 0x7`: FLAC-style fixed polynomial LPC order `0..=4`.
- `(w >> 18) & 1`: loop-start seek flag (`b18 = 1` occurs cleanly on the `w3` frame boundary; LPC history stays continuous across it!).
- `nbits = ((w >> 19) & 0xF) + 1`: bits per signed two's-complement residual (`1..=16`).
- `(w >> 23) & 1`: boundary/seek marker (`fl`).

Words per channel: `wpc = (spc * nbits + 31) / 32`.
- **Mono (`ch = 1`):** `wpc` big-endian `u32` words packed **MSB-first** with `spc` signed `nbits`-wide two's-complement residuals.
- **Stereo (`ch = 2`):** `wpc` word pairs (`u32 BE` L word followed by `u32 BE` R word, totaling `wpc * 8` bytes), each channel stream unpacked independently **MSB-first** (`spc` residuals per channel).

Fixed-LPC reconstruction per channel (`history = [0, 0, 0, 0]` at frame `0`, continuous across all frames):
- `order == 0`: `pred = 0`
- `order == 1`: `pred = h[-1]`
- `order == 2`: `pred = 2 * h[-1] - h[-2]`
- `order == 3`: `pred = 3 * h[-1] - 3 * h[-2] + h[-3]`
- `order == 4`: `pred = 4 * h[-1] - 6 * h[-2] + 4 * h[-3] - h[-4]`
- `y = pred + residual`, normalized to `f32` via `y as f32 / 8192.0` (`±8191` peak full scale before `+0x39` dB gain).

### 2.4 Seamless Loop Structure
For sustained instruments (`payload[0x29] == 0 && payload[0x2A] == 0`) where `w3` resolves to sample index `s3 < total_frames`, the final 4+ samples ending at `s3` match the final 4+ samples ending at `total_frames` **bit-exact** (`out[s3 - 4..s3] == out[len - 4..len]`). Therefore looping `[loop_start = s3, loop_end = total_frames]` wraps seamlessly with matching 4th-order derivative state and zero click. One-shot percussive files (`payload[0x29..0x2B] != [0, 0]`) play straight through without looping.

---

## 3. Nord Piano Library (`.npno`) `CNSP` Paged Fixed-LPC Codec

### 3.1 Container Header & Note Mapping
- `0x0000..0x0030`: 48-byte `CBIN` header (`kind = "npno"`).
- `0x002C..0x0030`: `CNSP` marker (`CNSP` base offset = **`0x002C`**).
- `0x0048..0x0068`, `0x0068..0x0088`, `0x0088..0x00A8`: short name, long name, and variant label (ASCII, NUL-trimmed).
- `0x00B8..0x0138`: 128-byte `key_map[note]` mapping MIDI note `0..127` to sampled `root_key` (`0xFF` = unmapped). Verified `44.5..787.5 Hz` fundamentals via autocorrelation correspond to hardware/storage sample rate **`~35,000 Hz`** (matching `.nsmp4` ~35 kHz DSP internal rate; e.g., `Clavinet D6` `key_map = 65` F4 has lag `100` samples = `35,002 Hz / 100 = 350.0 Hz`).
- `0x01C8..0x0248`: 128-byte signed `i8` fine-tuning table (`tuning_cents`).
- `0x0238..0x02B8`: 128-byte `velocity_tiers` table.
- `0x064A` (`u16 BE`): channel count (`1` mono, `2` stereo).
- `0x064C` (`u16 BE`): `nstrokes` (`38`..`2196`).
- `0x064E..0x074E`: **`[u16 BE; 128]` stroke counts per MIDI key `0..127`**, verified `sum == nstrokes` across all 38 `.npno` files, matching the exact set of non-`0xFF` `root_key`s in `key_map`. Iterating key `n = 0..127` assigns each group of `counts[n]` sequential records in the stroke table directly to `root_key = n`.

### 3.2 `0x76`-Byte Stroke Descriptor Table (`0x0756 .. 0x0756 + nstrokes * 0x76`)
Each record `rec[k]` (`118` bytes):
- `+0x08..+0x0C` (`u32 BE`): offset relative to `0x002C` (`CNSP` base). Exact file byte offset = **`0x2C + u32_be(rec + 8)`**.
- `+0x0C..+0x0E` (`u16 BE`): stroke type & velocity threshold:
  - `flags_hi = u0c >> 8`: `0` = normal sustain stroke, `1` = damper-down/sympathetic variant, `2` = release / key-off stroke.
  - `vel_low = u0c & 0xFF` (ascending per velocity layer within a key group).
- `+0x0E..+0x12` (`u32 BE`): exact decoded sample frame count `total_frames` (`mono` samples or `stereo` sample pairs).
- `+0x12..+0x14` (`u16 BE`): `npages` (`page_bytes = 1022 * channels`; next offset - offset == `npages * page_bytes`).
- **`+0x14..+0x1C` (`4 x i16 BE`)**: **Left channel initial LPC history `(h[-4], h[-3], h[-2], h[-1])`** for Page `0`.
- **`+0x1C..+0x24` (`4 x i16 BE`)**: **Right channel initial LPC history `(h[-4], h[-3], h[-2], h[-1])`** for Page `0` (`0` for mono).
- `+0x24..+0x36`: release/loop segment boundary frame counts (`loop_page` at `+0x34`).

### 3.3 Paged `1022 * channels`-Byte Bitstream (`off .. off + npages * 1022 * channels`)
Each page begins with a 2-byte header `[b0, b1]`:
- `b0`: encoder attenuation/envelope tracking index in `0..=47` (~1 dB per step of note decay already present in the waveform; **no** per-page amplitude rescaling on decode — overlapping samples agree 1:1).
- `order = b1 >> 5`: fixed-LPC order `0..=4`.
- `nbits = b1 & 0x0F`: bits per residual (`3..=14`; `b1 & 0x10 == 0`).

Bit & sample packing:
- Payload (`1020` bytes mono / `2042` bytes stereo) is read as 16-bit **big-endian words `(d[i] << 8) | d[i+1]`** with bits extracted **LSB-first (`(w >> 0)` .. `(w >> 15)`)** across word boundaries.
- Frames per page: `nf = ((page_payload_bytes * 8) / channels) / nbits`.
- For stereo (`ch = 2`), residuals are **sample-by-sample interleaved `[L0, R0, L1, R1, ...]`**.
- **64-Frame Overlap Continuity:** Page `0` starts from `(hL, hR)` read from `rec[k] +0x14..+0x24`. Every subsequent page `p > 0` starts with the exact 4-sample LPC history ending 64 frames before the end of page `p - 1`, and its first 64 decoded frames reproduce those last 64 frames of page `p - 1` **bit-exact** (`0` mismatches across all 190 tested strokes). Dropping those first 64 overlap frames per `p > 0` yields `sum(nf_p - 64 for p > 0) + nf_0` frames, which truncates cleanly to `total_frames` (`+0x0E`).
- Peak integer range is `[-7782, +7782]`, normalized to `f32` via `y as f32 / 8192.0`.
