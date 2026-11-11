# Research: Clavia Custom LZ Format Reverse Engineering

- **Target:** Nord Stage 4 Firmware (AM1705 Host OS, `CODE` segments)
- **Source:** `Update Nord Stage 4 OS v1.66.exe` (Resource 101, Image 1 `0xc42c`..`0x706fd`)
- **Status:** Token format and LZ back-reference structure proven against known ARM926EJ-S startup code.

---

## 1. Verified Discovery: Token Bit Structure

By comparing the compressed `CODE` stream against the known ARM926EJ-S startup sequence (PSR mode initialization, cache invalidation, and stack pointer setup across ARM modes `0x13`, `0x17`, `0x1b`, `0x12`, `0x11`, `0x1f`), we solved the exact 16-bit match token format:

### 16-bit Match Token (`(byte1 << 8) | byte0` or Little-Endian `[b0, b1]`):
```text
Bit 15 .. Bit 3: Distance / Offset in history buffer
Bit  2 .. Bit 0: Match Length - 2 (3 bits, values 0..7 -> length 2..9 bytes)
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

## 2. Stream Control Structure

- Stream begins with uncompressed literal block `00 1b` (27 bytes of ARM cache/MMU reset stub).
- The compressor mixes literal bytes and 16-bit back-references.
- Standard LZSS/UCL variants (NRV2B, NRV2D, Okumura LZSS) do not work out of the box because Clavia uses this tailored 13-bit distance + 3-bit length bit-packing scheme.
