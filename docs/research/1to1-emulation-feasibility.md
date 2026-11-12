# Research: 1:1 Low-Level Emulation Feasibility Checks

- **Checked date:** 2026-11-10
- **Goal:** decide whether a 1:1 (firmware-running) emulation track is feasible. See [D2](../DECISIONS.md).
- **Sources (local):** `Update Nord Stage 4 OS v1.66.exe` (`Res 101`), `Nord Stage 4 Factory Restore RevK v1.66.ns4b`, web searches.

| Blocker | Result | Status |
|---|---|---|
| DSP images in the update? | Compression covers the **ARM** sections only (custom LZ). Not encrypted as far as tested. SHARC VISA code is present, uncompressed, in `bank0_CODE@0x440000..0x4FFFFF` ([dsp-image-hunt.md](dsp-image-hunt.md) 5.5). `DSP*` names are region labels in the bank 1 flash tool, not images | **Found (five images, exact boundaries), partly disassembled** |
| Boot ROMs | Not obtainable without hardware; not in the update package | **Blocked** |
| Central unknown chip / buses | Not identifiable from the photos; web sources are generic and speculate an FPGA | **Open** |
| Sample codec (`CNSP`) | Opaque payload, no sub-structure found (section 3) | **Open** |
| SHARC emulator | No open-source simulator for the 32-bit floating-point SHARC family; only ADI's proprietary CCES simulator | **Confirmed gap** |
| Performance | Not testable yet | **Unverified** |

## 1. Firmware compression vs encryption

Method: signature scan, zlib trial decode at every header hit, entropy map in 64 KB steps, and a printable-string count against a byte-shuffled control.

- No gzip, xz, bzip2, zip or lz4 signatures. The 76 `lzma`-style and 31 zlib-style hits are chance 2 to 3 byte matches; **0 zlib streams decode**.
- Entropy map: first 64 KB is about 1.1 (empty); most of `0x010000..0x3BFFFF` is 7.1 to 7.7 bits per byte; the area near `0x3CF040` drops to about 5.5 and has readable strings (packed bank 1 `CODE` literals; it is not a DSP loader, see [dsp-image-hunt.md](dsp-image-hunt.md)).
- Printable strings in the high-entropy region exceed the shuffled control by 1x to 5x but contain no meaningful text, which is what structured compressed data produces.
- The beginning of the `CODE` segment decodes as ARM instructions (for example `msr cpsr_c`), so at least the start of the host image is plain.

**Conclusion:** the bulk of the image is neither standard compression nor readable code. It is either a custom compression scheme or encrypted, and these checks cannot tell which. No decryption or protection circumvention will be attempted. A custom-compression test (finding the decompressor in the plain ARM startup code) is the only remaining legitimate step.

## 2. Hardware identification

- **Host Processor:** **Texas Instruments AM1705** (Sitara ARM926EJ-S MPU, 375/456 MHz, ARMv5TEJ instruction set, 16 KB I-Cache, 16 KB D-Cache, MMU).
  - Confirmed via physical teardown photo of Nord Piano 4 (NP4) mainboard ([`.reference/mainboard/c0840e60-74b3-11eb-baf8-a528ac652aed.jpg`](../../.reference/mainboard/c0840e60-74b3-11eb-baf8-a528ac652aed.jpg)) where the AM1705 marking is clearly visible, and Nord Stage 3 mainboard ([`.reference/mainboard/others/nord-stage-3.jpeg`](../../.reference/mainboard/others/nord-stage-3.jpeg)).
  - **Memory Map Correspondence:**
    - On-chip L2 RAM base at `0x80000000`: exact target address of the `SRAM` segment in the firmware container (`0x2D4`, `0xFC80` bytes).
    - External SDRAM (EMIFB) base at `0xC0000000`: exact target address of the `CODE` segment in the firmware container (`0xFF54`, expanded to `5,499,504` bytes).
    - Exception Vector Base at `0xFFFF0020`: matches ARM926EJ-S High Exception Vectors (`CP15` register 1 `SCTLR.V = 1`).
    - Reset stub CP15 cache & TLB invalidation (`mcr p15, 0, r0, c7, c7, 0` and `mcr p15, 0, r0, c8, c7, 0`) perfectly matches ARM926EJ-S architectural initialization.
- **Audio DSPs:** **Analog Devices SHARC** (ADSP-21xxx floating-point 32-bit DSP series, 450 MHz class).
  - Clavia partitions DSP synthesis into three dedicated firmware images: `DSPO` (Organ tonewheel & rotary modeling), `DSPS` (Synth subtractive, wavetable, FM & sample playback), and `DSPF` (Piano multisample, modal string resonance & per-layer FX racks).
- **Communication Architecture:** The TI AM1705 acts as the master host controller (keybed triple-sensor scanning, OLED displays, rotary encoders/faders, program memory), streaming MIDI/control events and loading DSP partitions into the SHARC processors via high-speed serial/McASP/Host Port Interface (HPI).

## 3. `CNSP` sample format

- 48-byte `CBIN` header (`npno`), then `CNSP` marker at payload start.
- Plain header fields follow: type/version bytes, a 4-byte value, then the instrument name (`Clavinet D6`) padded with zeros.
- No repeating `CNSP` markers, no block headers found, tail is high-entropy. The audio payload is opaque and the codec is **unknown**.

## 4. Verdict

1:1 low-level emulation is **not currently feasible**: boot ROMs are unavailable, SHARC code is in the update as VISA inside `bank0_CODE` but not yet disassembled or mapped to the three DSPs (see [dsp-image-hunt.md](dsp-image-hunt.md) 5.5), the sample codec is unknown, and no SHARC simulator exists. None of that is proven impossible, so Track B stays a research option, not a plan. The shipping engine remains the high-level Rust implementation (Track A).

## 6. Deeper analysis of the firmware containers

Both firmware images share one layout (big-endian segment table of `SRAM`, `CODE`, `ARM0`):

| | Image 0 (main OS) | Image 1 (second bank) |
|---|---|---|
| `SRAM` segment | file `0x2D4`, `0xFC80` B, target `0x80000000` | file `0x2D4`, `0xC158` B, target `0x80000000` |
| `CODE` segment | file `0xFF54`, declared size `0x53EA70` (5,499,504 B), target `0xC0000000` | file `0xC42C`, declared size `0xC30C0` (798,912 B), target `0xC0000000` |
| `CODE` bytes actually in file | 3,510,322 | 411,142 |
| Expansion implied | about 1.57x | about 1.95x |
| `ARM0` | entry `0x2EC`, vector base `0xFFFF0020` | entry `0x334`, vector base `0xFFFF0020` |

Findings:
- The `SRAM` segment is **plain ARM code** (C++-style vtable calls, peripheral register tests); it is mostly zero padding by entropy.
- `0xC0000000` is the AM1705 external SDRAM base (EMIFB), which matches the chip identified from the photos.
- The `CODE` segment is declared larger than the bytes present in the file in both images, so it is **expanded at load time**.
- It starts with a 2-byte prefix and a 44-byte plain ARM reset stub (disable IRQ/FIQ, invalidate caches, enable I-cache, set mode). After that the stream is not valid ARM, but valid ARM instruction fragments appear verbatim inside it (for example `ldr sp,[pc,#0x70]`, `ldr pc,[pc,#0x1c]`, `bx r0`) together with repeating token patterns such as `cd 02 N0 29 4d 00 XX` with `N` counting down. Encryption would not leave instruction fragments in clear, so this is consistent with a **custom byte-oriented compressor or transform**, not encryption. This is an inference, not proven.
- Per-position entropy at periods 2, 4, 6 and 8 is flat everywhere in the high-entropy range, so it is not raw SHARC (48-bit) code or fixed-width PCM either; it is the compressed stream.
- Standard decoders (deflate, zlib, gzip, raw LZMA1 with all lc/lp/pb) all fail on the `CODE` stream at the tested offsets.

**Consequence:** the DSP images are most likely inside the expanded `CODE` and are recoverable once the expander is identified. The expander is not in the plain `SRAM` literals (no reference to `0xC0000000` or the declared sizes), so it is probably in the AM1705 boot path or in the Windows updater and flasher helper (`Res 105`, 587,736 B), which is the next place to look.

## 7. Where the expander lives (updater analysis)

The embedded helper (`Res 105`, 587,736 B) is an **NSIS installer** (`.ndata` section), most likely the Clavia USB driver installer, not a flasher. Dead end for the question.

The updater itself (`AVCUpdateEngine`, build path `clavia\build\nordgrand2\pc\osupdate`) contains the protocol strings:
- `Resource CAB-file does not validate` / `header is invalid` / `parse error`, and `OS BinFile seems broken. Checksum does not match.`
- `BIN-file code section started.` and **`ERROR: Cannot send compressed section %d to RAM.`**
- States for `Erase sector`, `CAckXfrBegin/Data/End`, and `Rebooting to update mode...`; the transport is a USB "IT-protocol".

So the PC sends compressed sections into the instrument RAM while it runs in update mode, and the **instrument expands them**. The updater never decompresses anything on the PC. Our extraction of `Res 101` therefore contains compressed data whose expander lives on the device (bootloader or ROM), which we do not have.

Searches of the plain `SRAM` segments of both images found no byte-copy decode loops, and LZ4, LZO, Snappy and LZF decoders all fail at offsets 0 to 63. Standard deflate, zlib and raw LZMA1 also fail.

**Consequence:** recovering the algorithm now means inferring the scheme from the compressed stream alone, using the ARM reset stub and the repeated ARM fragments as known plaintext. That is feasible in principle but is a research effort, not a quick extraction.

## 8. Clavia USB Driver installer v4.08 (checked, no help)

`Nord Keyboards Driver Installer v4.08.exe` (589,960 B) is an NSIS installer (different build from the 587,736 B copy embedded in the OS updater). Its decoded script lists only USB driver packages (`ClaviaCD`, `ClaviaUSB64` legacy, `ClaviaWinUSB`, `DriverTools.exe`, `.inf` and `.cat` files) and registry and start-menu handling for Nord Sound Manager and Nord Sample Editor. It contains no firmware, no decompressor and no IT-protocol logic: the device speaks the protocol, and the driver only provides the USB transport. It does not help with the `CODE` expander.

## 9. Clavia Custom LZ Format Solved (No Encryption)

By comparing the compressed `CODE` stream against the known ARM926EJ-S startup code (`mrs r0, CPSR`, `mcr p15` cache invalidation, and stack pointer setup across ARM modes `0x13`, `0x17`, `0x1b`, `0x12`, `0x11`, `0x1f`), the token math was completely solved:
- **16-bit match token:** `(b1 << 8) | b0` (little-endian).
- **Match Length:** `token & 0x07` + 2 (3 bits, 2..9 bytes).
- **Distance:** `token >> 3` (13 bits, up to 8192 bytes history buffer).
- For complete derivation and verified offsets, see [`clavia-lz-reconstruction.md`](clavia-lz-reconstruction.md).

## 5. Track B Action Plan (Firmware & `CNSP` Sample Codec Reverse Engineering)

Per [D12](../DECISIONS.md#d12-pursue-track-b-reverse-engineering-for-cnsp-sample-codec-and-firmware-decompression), Track B is now an active research & reverse-engineering roadmap:

1. **Step 1 — Complete Clavia Custom LZ Decompressor (`CODE` Segment):**
   - Resolve the exact control-byte / literal-run state machine connecting the `00 1b` initial literal block and the 16-bit `(distance << 3) | (length - 2)` match tokens in Image 1 (`0xC42C`, 411 KB -> 798 KB) and Image 0 (`0xFF54`, 3.51 MB -> 5.49 MB).
   - Extract the decompressed `ARM0` host OS image and embedded `DSPO`, `DSPS`, and `DSPF` SHARC DSP segments.
2. **Step 2 — `CNSP` Header & Payload Differential Analysis:**
   - Compare the 80-byte `CNSP` sub-header (`0x30..0x80` after the 48-byte `CBIN` header) across all 195 `.nsmp4` and 38 `.npno` files to map sample rate, bit depth, key-split zone table, loop points, and bitstream offset pointers.
   - Locate the `CNSP` parser and audio decompression routine inside the unpacked `ARM0` / `DSPF` / `DSPS` binaries (or Nord Sample Editor 4 binary symbols) to identify the exact entropy/predictor coding (e.g., Rice/Golomb + LPC/ADPCM predictor).
3. **Step 3 — Clean-Room `CNSP` Decoder Implementation:**
   - Implement a zero-allocation `CNSP` sample decoder in `crates/nord-core::cbin::cnsp` so users can point the emulator directly at their own `.ns4b` backup archive to load authentic `.npno` and `.nsmp4` instruments in place.
4. **Step 4 — Hardware Bus & SHARC Coefficient Extraction:**
   - Extract lookup tables, nonlinear saturation curves, and filter coefficients from the unpacked `DSPO`/`DSPS`/`DSPF` binaries to validate or refine `crates/nord-core` DSP routines.




