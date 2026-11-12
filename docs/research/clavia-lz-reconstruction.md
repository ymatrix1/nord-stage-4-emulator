# Research: Clavia Custom LZ Format Reverse Engineering

- **Target:** Nord Stage 4 Firmware (AM1705 Host OS, `CODE` segments)
- **Source:** `Update Nord Stage 4 OS v1.66.exe` (PE Resource 101, offset `0x79CB30`, Image 1 `0xC42C`..`0x706FE`, Image 0 `0xFF54`..`0x368C9A`)
- **Status:** **Solved bit-exact (standard LZO1X).** Full-stream decompression verified 100% against both OS v1.66 `CODE` banks (`0xDE902503` and `0xFBB813F8`), plus ARM926EJ-S startup code disassembly.

---

## 1. Physical Location and Segment Memory Map

Resource 101 (`4,037,080` bytes) embeds dual firmware banks (Image 0 and Image 1). Segment `data_offset` values in the descriptor table are relative to each bank's base offset (`header_size = 0x20` for Bank 0; `image1_offset = 0x368FA6` for Bank 1):

| Segment | Offset in Image 1 | File Offset (v1.66) | Size in Image | Target Address | Expanded Size |
|---|---|---|---|---|---|
| `SRAM` | `0x0002D4` | `0x36927A` | `49,496` B (`0xC158`) | `0x80000000` | Uncompressed |
| `CODE` | `0x00C42C` | `0x3753D2` | `410,321` B (`0x642D1`, LZO1X) | `0xC0000000` | `798,912` B (`0xC30C0`, 1.947x) |
| `ARM0` | `0x0706FE` | `0x3D96A4` | `820` B (`0x0334`) | `0xFFFF0020` | Uncompressed |

Image 0 (Main Operating System Bank):

| Segment | Offset in Image 0 | File Offset (v1.66) | Size in Image | Target Address | Expanded Size |
|---|---|---|---|---|---|
| `SRAM` | `0x0002D4` | `0x79CE04` | `64,640` B (`0xFC80`) | `0x80000000` | Uncompressed |
| `CODE` | `0x00FF54` | `0x7ACB84` | `3,509,573` B (`0x358D45`, LZO1X) | `0xC0000000` | `5,499,504` B (`0x53EA70`, 1.567x) |
| `ARM0` | `0x368C9A` | `0xB057CA` | `748` B (`0x02EC`) | `0xFFFF0020` | Uncompressed |

*(Note: `0x00C42C + 0x0642D1 = 0x0706FD` + 1 byte alignment = `0x0706FE`; similarly `0x00FF54 + 0x358D45 = 0x368C99` + 1 byte alignment = `0x368C9A`.)*

---

## 2. Verified Compression Format: Standard LZO1X

Full-stream verification proved that Clavia's `CODE` compressor is **standard LZO1X** (the initial hypothesis of a custom 16-bit `(dist << 3) | (len - 2)` codec was superseded once we observed the trailing-literal bits and `11 00 00` EOF marker):

1. **Initial Literal Run (`00 1B`):**
   - When the first stream byte `t <= 17` and `t == 0`, LZO1X emits `3 + 15 + 0x1B = 45` literal bytes (`offset 0x02..0x2F`), NOT 44. That 45th literal byte (`0x1f`, part of the ARM immediate `bic r0, r0, #0x1f`) is immediately followed by `0x00` at offset `0x2F` and then M2/M3 tokens.
2. **M2 Short Matches (`t >= 64`):**
   - `length = ((t >> 5) - 1)` (3..8 bytes)
   - `trailing_literals = t & 0x03` (0..3 literals immediately after the match)
   - `distance = 1 + ((t >> 2) & 0x07) + (next_byte << 3)` (`1..2048` bytes)
   - Example at offset `0x36`: `[ed 02]` = `M2`:
     - `0xED >> 5 = 7` -> `length = 7 - 1 = 6` bytes
     - `0xED & 0x03 = 1` trailing literal (`0x17` for `msr cpsr_c`)
     - `distance = 1 + ((0xED >> 2) & 7) + (0x02 << 3) = 1 + 3 + 16 = 20` bytes!
   - Example at offset `0x39`: `[cd 02]` = `M2`:
     - `0xCD >> 5 = 6` -> `length = 6 - 1 = 5` bytes
     - `0xCD & 0x03 = 1` trailing literal
     - `distance = 1 + ((0xCD >> 2) & 7) + (0x02 << 3) = 1 + 3 + 16 = 20` bytes!
3. **M3 Matches (`32 <= t < 64`), M4 Matches & EOF (`16 <= t < 32`), and M1 / Extended Literal Runs (`t < 16`):**
   - End-of-stream is signaled by the standard LZO1X M4 sentinel `11 00 00` (`offset == 16384`).
   - Both OS v1.66 `CODE` banks terminate cleanly at `11 00 00` and produce exact runtime byte lengths and checksums.

---

## 3. Disassembly of the Initial 45-Byte ARM Reset Stub

The `CODE` segment begins with 45 uncompressed literal bytes (`0x02..0x2F`) initializing the AM1705 ARM926EJ-S CPU (`11` 32-bit instructions + first byte of the 12th):

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
bic r0, r0, #0x1F          ; Clear ARM processor mode bits (last literal byte = 0x1F)
```

---

## 4. Mode Setup Loop & Stack Pointer Literal Pool

Following the reset stub, LZO1X M2/M3 tokens reconstruct the stack pointer initialization across ARM modes (`0x13`, `0x17`, `0x1b`, `0x12`, `0x11`, `0x1f`) and jump to SRAM:
- `[ed 02] [17]` + `[cd 02] [60]` ... emits Abort (`0x17`, SP literal `0x004D2960`), Undefined (`0x1b`, `0x004D2950`), IRQ (`0x12`, `0x004D2940`), FIQ (`0x11`, `0x004D2930`), and System (`0x1f`, `0x004D2920`) mode setup instructions.

At offset `0x5C`..`0x6F`, literal instructions execute the jump to the main SRAM initialization routine:
```arm
add lr, pc, #0
ldr pc, [pc, #0x1C]
ldr r0, [pc, #0x1C]
bx r0
.word 0x8001FFB0   ; Jump target in AM1705 internal SRAM (0x80000000 base)
```

---

## 5. Decompression Architecture and Firmware Container Verification

1. **Updater Binary Inspection (`Update Nord Stage 4 OS v1.66.exe`):**
   - Resource 101 (`RCDATA 101`, offset `0x79CB30`) holds the complete firmware container.
   - Resource 105 (`RCDATA 105`, offset `0xB76508`, 587,736 B) is the embedded NSIS installer for the Clavia USB driver (`Nullsoft.NSIS.exehead`).
   - The x64 updater function at `0x140034510` validates image headers and segment checksums before transmitting blocks over USB MIDI.
   - Error string at `0x1404737C8`: `"ERROR: Cannot send compressed section %d to RAM."` confirms that compressed sections are sent directly to on-board Flash, where the bootloader unpacks them at power-on.

2. **SHARC DSP Partition Layout:**
   - SHARC DSP code images (`DSPO` Organ, `DSPS` Wave 2 Synth, `DSPF` Piano & Effects) are embedded within the unpacked `CODE` segment near runtime offset `0x003CF040`.
   - Complete partition symbol catalog recovered from container offset `0x003CF040`:
     - `DSPFxCore1`: Per-layer Effects pipeline (`Delay`, `Reverb`, `Filter`) & String Resonance.
     - `DSPOrg`: 2-Layer Organ physical modeling (`FullOrgan`, `LeslieHorn`, `Rotary Mic Distance`).
     - `DSPSyn`: 3-Layer Nord Wave 2 Synth engine (`OSC WAVEFORM`, `WaveTable`, `SAMPLE LIBRARY`).
     - SPORT bus routing tags: `Fx to Org`, `to Fx`, `Host`.
     - Domain identifiers: `ORG`, `PNO`, `SMP`, `SY1`, `SYL3`.

3. **Segment Checksum Algorithm (Proven Ground Truth):**
   - Each segment table entry contains a 32-bit one's-complement byte checksum:
     $$\text{checksum} = \text{0xFFFFFFFF} - \left( \sum_{i} \text{byte}_i \pmod{2^{32}} \right)$$
   - Verified 100% bit-exact on AM1705 firmware:
     - Image 0 `SRAM`: size 64,640 B, byte sum `0x000CE9E6`, checksum `0xFFF31619` (Exact match).
     - Image 0 `ARM0`: size 748 B, byte sum `0x00017530`, checksum `0xFFFE8ACF` (Exact match).
     - Image 1 `SRAM`: size 49,496 B, byte sum `0x0012F517`, checksum `0xFFED0AE8` (Exact match).
     - Image 1 `ARM0`: size 820 B, byte sum `0x0001A075`, checksum `0xFFFE5F8A` (Exact match).
   - For `CODE` segments, the checksum validates the uncompressed target memory block at `0xC0000000` (Image 0: `0xDE902503`, Image 1: `0xFBB813F8`).

---

## 6. Reference Implementation & Tests

Implemented in Rust within `crates/nord-core/src/firmware.rs` and exposed in `crates/nord-core/src/bin/ns4.rs`:
- `calculate_checksum(bytes: &[u8]) -> u32`
- `decompress_clavia_lz(input: &[u8], max_output_size: usize) -> Result<Vec<u8>, String>`
- `compress_clavia_lz(input: &[u8]) -> Vec<u8>`
- `FirmwareSegment::extract_payload(&self, raw: &[u8]) -> Result<Vec<u8>, String>`
- `FirmwareSegment::extract_payload_from(&self, bank_base: usize, raw: &[u8]) -> Result<Vec<u8>, String>`
- `FirmwareSegment::verify_checksum(&self, payload: &[u8]) -> bool`
- `FirmwareContainer::parse(bytes: &[u8]) -> Result<Self, String>`
- `FirmwareContainer::bank_base(&self, bank: u8) -> Result<usize, String>`
- `FirmwareContainer::extract_segment(&self, bank: u8, name: &str, raw: &[u8]) -> Result<Vec<u8>, String>`
- `FirmwareContainer::segment(&self, name: &str) -> Option<&FirmwareSegment>`
- `FirmwareContainer::segment_in_bank(&self, bank: u8, name: &str) -> Option<&FirmwareSegment>`
- `DspEngineType` & `DspPartition`

### Test Coverage
- `test_clavia_lz_roundtrip_synthetic`: lossless LZO1X round-trip.
- `test_clavia_lz_token_bit_structure`: verifies exact ARM926EJ-S LZO1X `00 1B` + M2/M3 prefix unpacking.
- `test_clavia_lz_all_match_lengths_3_through_8`: tests all LZO1X M2 match lengths (`3..=8`) and trailing literals.
- `test_clavia_lz_extended_literal_run`: verifies `00 1B` 45-byte extended literal run unpacking.
- `test_clavia_lz_bounds_protection` & `test_clavia_lz_truncated_token_error`: validates bounds protection and truncated stream errors.
- `test_clavia_lz_empty_and_single_byte`: checks empty and 1..3-byte edge cases.
- `test_extract_payload_bounds`, `test_extract_payload_code_segment_decompression`, and `test_extract_segment_respects_bank_base_offsets`: validates segment extraction across Bank 0 (`0x20`) and Bank 1 (`image1_offset`).
- `test_calculate_checksum_and_verify`: validates one's-complement byte checksum algorithm.
- `test_dsp_partitions_all_engines` & `test_parse_firmware_container_and_sharc_tags`: validates dual-bank container header, segment discovery, and SHARC DSP partition discovery.
- `test_cli_inspect_firmware_container` & `test_cli_inspect_firmware_verify_and_unpack`: validates `ns4 inspect --verify --unpack`.
