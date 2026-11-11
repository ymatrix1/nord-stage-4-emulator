# Research: 1:1 Low-Level Emulation Feasibility Checks

- **Checked date:** 2026-11-10
- **Goal:** decide whether a 1:1 (firmware-running) emulation track is feasible. See [D2](../DECISIONS.md).
- **Sources (local):** `Update Nord Stage 4 OS v1.66.exe` (`Res 101`), `Nord Stage 4 Factory Restore RevK v1.66.ns4b`, web searches.

| Blocker | Result | Status |
|---|---|---|
| DSP images encrypted? | Inconclusive (details in section 1) | **Open** |
| Boot ROMs | Not obtainable without hardware; not in the update package | **Blocked** |
| Central unknown chip / buses | Not identifiable from the photos; web sources are generic and speculate an FPGA | **Open** |
| Sample codec (`CNSP`) | Opaque payload, no sub-structure found (section 3) | **Open** |
| SHARC emulator | No open-source simulator for the 32-bit floating-point SHARC family; only ADI's proprietary CCES simulator | **Confirmed gap** |
| Performance | Not testable yet | **Unverified** |

## 1. Firmware compression vs encryption

Method: signature scan, zlib trial decode at every header hit, entropy map in 64 KB steps, and a printable-string count against a byte-shuffled control.

- No gzip, xz, bzip2, zip or lz4 signatures. The 76 `lzma`-style and 31 zlib-style hits are chance 2 to 3 byte matches; **0 zlib streams decode**.
- Entropy map: first 64 KB is about 1.1 (empty); most of `0x010000..0x3BFFFF` is 7.1 to 7.7 bits per byte; the loader table area near `0x3CF040` drops to about 5.5 and has readable strings.
- Printable strings in the high-entropy region exceed the shuffled control by 1x to 5x but contain no meaningful text, which is what structured compressed data produces.
- The beginning of the `CODE` segment decodes as ARM instructions (for example `msr cpsr_c`), so at least the start of the host image is plain.

**Conclusion:** the bulk of the image is neither standard compression nor readable code. It is either a custom compression scheme or encrypted, and these checks cannot tell which. No decryption or protection circumvention will be attempted. A custom-compression test (finding the decompressor in the plain ARM startup code) is the only remaining legitimate step.

## 2. Hardware identification

- Host: TI AM1705 (ARM926EJ-S, no DSP core), identified from the chip marking.
- DSPs: Analog Devices; the 32-bit floating-point SHARC family has 450 MHz class parts with on-chip RAM and ROM and a DAI routing fabric (web summary, not a datasheet read).
- Web results claim an FPGA on Stage 3 boards but cite no verifiable teardown, so this is **not** treated as fact.

## 3. `CNSP` sample format

- 48-byte `CBIN` header (`npno`), then `CNSP` marker at payload start.
- Plain header fields follow: type/version bytes, a 4-byte value, then the instrument name (`Clavinet D6`) padded with zeros.
- No repeating `CNSP` markers, no block headers found, tail is high-entropy. The audio payload is opaque and the codec is **unknown**.

## 4. Verdict

1:1 low-level emulation is **not currently feasible**: boot ROMs are unavailable, DSP images are unreadable, the sample codec is unknown, and no SHARC simulator exists. None of that is proven impossible, so Track B stays a research option, not a plan. The shipping engine remains the high-level Rust implementation (Track A).

## 5. Remaining legitimate steps

1. Read the plain ARM startup code to locate any decompressor routine and identify the scheme.
2. Search for a zero-header block structure in `CNSP` payloads (compare two instruments of different lengths).
3. Obtain a Stage 4 teardown or FCC internal photos for the central chip.
