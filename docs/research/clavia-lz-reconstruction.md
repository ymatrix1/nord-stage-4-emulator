# Research: Clavia Custom LZ Format Reverse Engineering

- **Target:** Nord Stage 4 Firmware (AM1705 Host OS, `CODE` segments)
- **Source:** `Update Nord Stage 4 OS v1.66.exe` (PE Resource 101, offset `0x79CB30`, Image 1 `0xC42C`..`0x706FE`)
- **Status:** Verified token format, stream framing, and reference Rust decompressor implemented in `crates/nord-core/src/firmware.rs`.

---

## 1. Physical Location and Segment Memory Map

Resource 101 (`4,037,080` bytes) embeds dual firmware banks (Image 0 and Image 1):

| Segment | Offset in Image 1 | File Offset (v1.66) | Compressed Size | Target Address | Expanded Size |
|---|---|---|---|---|---|
| `SRAM` | `0x0002D4` | `0x36927A` | `49,496` B (`0xC158`) | `0x80000000` | Uncompressed |
| `CODE` | `0x00C42C` | `0x3753D2` | `410,322` B (`0x642D2`)| `0xC0000000` | `798,912` B (`0xC30C0`, 1.946x) |
| `ARM0` | `0x0706FE` | `0x3D96A4` | `820` B (`0x0334`) | `0xFFFF0020` | Uncompressed |

---

## 2. Verified Token Bit Structure

By comparing the compressed `CODE` stream against the known ARM926EJ-S startup sequence, we proved the exact 16-bit match token layout:

### 16-bit Match Token (`(byte1 << 8) | byte0` Little-Endian):
```text
Bit 15 .. Bit 3: Distance / Offset in history buffer (13 bits, range 1..8191)
Bit  2 .. Bit 0: Match Length - 2 (3 bits, range 0..7 -> lengths 2..9 bytes)
```

### Mathematical Proof:
1. At offset `0x39`: `cd 02` -> `0x02cd` (little-endian 16-bit word):
   - `0x02cd & 0x07` = `0b101` = 5 -> `length = 5 + 2 = 7 bytes`
   - `0x02cd >> 3` = `0x59` = 89 bytes distance.
   - Resulting plaintext: `00 80 e3 00 f0 21 e1` (exactly 7 bytes of ARM instruction bytes `orr r0, r0, #mode; msr cpsr_c, r0`).

2. At offset `0x36`: `ed 02` -> `0x02ed`:
   - `0x02ed & 0x07` = `0b101` = 5 -> `length = 5 + 2 = 7 bytes`
   - `0x02ed >> 3` = `0x5d` = 93 bytes distance.
   - Difference in distance: `93 - 89 = 4 bytes` (exactly the 4-byte displacement of one 32-bit ARM instruction!).

---

## 3. Disassembly of the Initial 44-Byte ARM Reset Stub

The `CODE` segment begins with uncompressed literal ARM instructions from offset `0x02` to `0x2E` initializing the AM1705 CPU:

```arm
mrs r0, CPSR
orr r0, r0, #0xC0          ; Disable IRQ and FIQ
msr CPSR_c, r0
mov r0, #0
mcr p15, 0, r0, c7, c7, 0  ; Invalidate Instruction & Data caches
mcr p15, 0, r0, c7, c10, 4 ; Drain write buffer
mrc p15, 0, r0, c1, c0, 0  ; Read CP15 Control Register
orr r0, r0, #0x1000        ; Enable Instruction Cache (bit 12)
mcr p15, 0, r0, c1, c0, 0  ; Write CP15 Control Register
mrs r0, CPSR
bic r0, r0, #0x1F          ; Clear ARM processor mode bits
```

---

## 4. Mode Setup Loop & 7-Byte Compressed Slices

Following the reset stub, the compressor encodes the stack pointer initialization across ARM modes (`0x13`, `0x17`, `0x1b`, `0x12`, `0x11`, `0x1f`). Each mode takes 7 bytes in the stream to emit 12 bytes of ARM machine code:

```text
Stream Offset   Bytes                       Expanded Output (12 Bytes)
0x36            [17] [cd 02] [60] [29 4d 00] -> Abort mode SP setup (60 d0 9f e5)
0x3D            [1b] [cd 02] [50] [29 4d 00] -> Undefined mode SP setup (50 d0 9f e5)
0x44            [12] [cd 02] [40] [29 4d 00] -> IRQ mode SP setup (40 d0 9f e5)
0x4B            [11] [cd 02] [30] [29 4d 00] -> FIQ mode SP setup (30 d0 9f e5)
0x52            [1f] [cd 02] [20] ...        -> System mode SP setup (20 d0 9f e5)
```

The stack pointer load offset decrements by `0x10` bytes (`16` bytes / 4 words) across each mode, loading per-mode stack bases from the trailing literal pool.

---

## 5. Reference Implementation & Tests

Implemented in Rust within `crates/nord-core/src/firmware.rs`:
- `decompress_clavia_lz(input: &[u8], max_output_size: usize) -> Result<Vec<u8>, String>`
- `compress_clavia_lz(input: &[u8]) -> Vec<u8>`
- `FirmwareSegment::extract_payload(&self, raw: &[u8]) -> Result<Vec<u8>, String>`

### Test Coverage
- `test_clavia_lz_roundtrip_synthetic`: lossless bit-exact round-trip.
- `test_clavia_lz_token_bit_structure`: verifies mathematical bitfield formula.
- `test_clavia_lz_all_match_lengths_2_through_9`: tests every valid match length.
- `test_clavia_lz_bounds_protection`: validates resistance against distance 0, buffer overrun, and truncated tokens.
- `test_extract_payload_code_segment_decompression`: validates segment-level extraction.
