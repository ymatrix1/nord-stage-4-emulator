# Research: Are the SHARC DSP images in the OS v1.66 firmware?

- **Checked date:** 2026-11-12
- **Sources (local, never committed):** `.reference/Update Nord Stage 4 OS v1.66.exe`, `target/fw_unpacked/{bank0,bank1}_{CODE,SRAM,ARM0}.bin`, `target/fw_unpacked/fw_v1.66.bin` (PE resource 101), `.reference/sharc/ADSP-214xx_hwr_rev1.1.pdf` (SHARC hardware reference), `tools/fw_xref.py`, `tools/fw_dsp_scan.py` (new).
- **Question:** Are the `DSPO` / `DSPS` / `DSPF` SHARC ADSP-21489 images present and recoverable in the OS v1.66 update, and where?

## Verdict

**Not found in the container (v1.44, v1.46, v1.64, v1.66 all checked).** The update carries only ARM images (two banks: `SRAM`, `CODE`, `ARM0` each). The strings `DSPSyn` / `DSPOrg` / `DSPFxCore*` are memory-region names in a bank 1 flash/test tool, not image names. No 48-bit instruction stream, no ADI loader stream and no compressed blob of a plausible size exists in any segment of either bank. The SHARC code stays in the board SPI flash, as `sharc-dsp-and-preset-hash-teardown.md` says. The `0x3CF040` offset in older notes is in the *packed* container, not in `bank0_CODE`.

## 1. Where the strings are (task 1)

`DSPSyn`, `DSPOrg`, `DSPFxCore1 L1 bl.0` appear in exactly three places, all one table:

| File | Offset | Why |
|---|---|---|
| `Update ... v1.66.exe` | `0xB6BB9C` (`DSPFxCore1 L1 bl.0`), `0xB6BBD6` (`DSPOrg`), `0xB6BC05` (`DSPSyn`) | PE resource 101 starts at `0x79CB30`; `0xB6BB9C - 0x79CB30 = 0x3CF06C` |
| `fw_v1.66.bin` | `0x3CF06C`, `0x3CF0A6`, `0x3CF0D5` | Packed (LZ) stream of image 1 `CODE`: literal runs show as text between control bytes, which is why only the first name of each group is intact and the entropy dips to 5.5 there. Image 1 `CODE` spans `0x368FA6 + 0xC42C .. +0x642D1 = 0x3753D2..0x3D96A3`, which contains `0x3CF06C` |
| `bank1_CODE.bin` (unpacked, load base `0xC0000000`) | `0x8CB7C` first name, 46 names to `0x8CF0C` | Same table after decompression |

`bank0_CODE.bin` has none of them (`grep -a -b -o -E "DSP(Syn|Org|FxCore|Smp)"` returns nothing for bank 0 and for all `SRAM`/`ARM0` files).

Commands:

```bash
for f in target/fw_unpacked/*.bin; do echo "== $f"; grep -a -b -o -E "DSP(Syn|Org|FxCore|Smp)[A-Za-z0-9 .]{0,16}" $f | head; done
python tools/fw_xref.py target/fw_unpacked/bank1_CODE.bin --string "DSPFxCore1 L1 bl.0"   # @0x8cb7c, pointed to by ['0x9f18']
python tools/fw_xref.py target/fw_unpacked/bank1_CODE.bin --dump 0x9ee0 0x9fe8
```

Table at `bank1_CODE@0x9ee0..0x9fe8`: shuffled name pointers (`0xC008CB7C` ...) interleaved with words such as `0x000C0800`, `0x00090800`, `0x000B0400`, `0x00090500`, `0x0003FF00`. These look like SHARC internal-RAM region addresses (`0x90000`, `0xB0000`, `0xC0000`) plus a size/offset field (ASSUMED reading). The names (`... L1 bl.0..3`, `L2`, `DSPSmp block N`, `DSPPiano block N`) and the neighbours (`Fx to Org`, `Org to Syn`, `Smp to Org0Bs`, `DSP %d Comm Error`, `Mark : xxxx`, `Debug out`) are inter-DSP link and RAM-block labels. Bank 1 is the service tool: `Flash Tool`, `SPI Flash`, `Piano Flash`, `Sample Flash`, `Checksum Update`, `Micron N25Q032`/`MT25QL128A` (`bank1_CODE@0x903FC`, `0x91ACC`, `0x91BC0`). The table is read by memory/flash tests, not by a loader.

## 2. ARM side boot routine (task 2)

- `ERROR: Cannot send compressed section %d to RAM.` is in the **PC updater** (`.exe@0x4721CF`), not the keyboard firmware. It uploads the LZ-compressed ARM sections (`SRAM`, `CODE`, `ARM0`) to the synth RAM. It does not mention DSP images.
- Keyboard firmware strings about DSP boot: `DSP Boot Error` (`bank0_CODE@0x529688`, used only by a UI string table through the literal at `0x24BBA8`, no loader logic there), `DSP %d Comm Error` (`bank0@0x29ED9C`, `bank1@0x9F20`), `Piano Flash Error` / `Sample Flash Error` (`bank0@0x52646C`, `0x5264DC`).
- No `.ldr`, `Loader`, `HPI`, `SHARC` or `ADSP` strings in either bank.
- Peripheral literals in both banks include SPI0 `0x01C41000` and EMIFA CS3 `0x62004000` (10 uses in bank 1); not traced to a DSP loader in this time box.
- The SHARC hardware reference (`ADSP-214xx_hwr_rev1.1.pdf`, "Booting", p. 24-46) says the 214xx boots itself from external non-volatile memory after reset: the boot ROM loads a boot kernel into the IVT (about 256 x 48-bit words) which then loads the application. With the SPI flash on the SHARC boot bus, the ARM does not need to carry or send the images. This fits finding 1 (INFERRED, not verified on hardware). The reference does **not** define the `.ldr` block layout; that is in ADI tools documentation, not in `.reference/sharc/`.

## 3. Entropy and structure scan (task 3)

Commands (`tools/fw_dsp_scan.py`, unit tests in `tools/tests/test_fw_dsp_scan.py`):

```bash
python tools/fw_dsp_scan.py target/fw_unpacked/bank0_CODE.bin --window 0x40000
python tools/fw_dsp_scan.py target/fw_unpacked/bank1_CODE.bin --window 0x20000
python tools/fw_dsp_scan.py target/fw_unpacked/bank0_CODE.bin --ldr   # same for bank1
```

| Region (unpacked) | Entropy (bits/byte) | Reading |
|---|---|---|
| `bank0_CODE 0x000000..0x27FFFF` | 6.07..6.47 | ARM code and tables (per-byte period score lowest at 4/8/12, `p4=5.37`, `p6=5.98`) |
| `bank0_CODE 0x280000..0x2FFFFF` | 3.81..4.15 | low-entropy tables |
| `bank0_CODE 0x300000..0x43FFFF` | 7.23..7.58 | 16-bit PCM-like data: scores at periods 2/4/6/8/12 are equal (`7.27/7.27/7.27/7.26/7.26`), odd periods higher (`7.58`). Two-byte structure only, no 6-byte structure |
| `bank0_CODE 0x440000..0x53EA6F` | 6.1..6.9 | data |
| `bank1_CODE 0x00000..0x7FFFF` | 6.07..6.23 | ARM code and tables |
| `bank1_CODE 0x80000..0xC30BF` | 1.46..5.4 | pointer tables, strings, padding |
| `bank0_SRAM`, `bank1_SRAM`, `ARM0` (748/820 B) | 0..3.5 | init blocks and ARM entry stubs |

Sweep with 32 KB windows every 16 KB over both banks: **0 windows** where the 6-byte-period score beats periods 4, 5, 7 and 8 by 0.05 bit (a SHARC 48-bit stream would). Bank 0 contrast at period 6 is about 0.5 in ARM windows too (ARM word alignment), so that contrast alone is not a SHARC test; the sweep above is.

ADI-loader-shaped triples (`tag, count, address in 0x80000..0xFFFFF`, 32-bit words): bank 0 has 2897 little-endian and 36 big-endian candidates, bank 1 has 509 and 28 (`--ldr`). They are scattered across ARM pointer tables (for example bank 1 `0x9DDC..` sits in the descriptor table above) and no two form a chain of consecutive blocks. ASSUMED: header shape is a guess, so this only rules out obvious plain-loader streams, not all packings.

## 4. Decision (task 4)

| Option | Result |
|---|---|
| Plain `.ldr` / 48-bit code in `CODE` | **No** (sweep above) |
| Compressed with a known scheme | **No evidence.** The only compressed data are the ARM sections (custom LZ, already unpacked). The `0x300000..0x43FFFF` block is 16-bit-structured, not near-random (7.3 vs 8.0) |
| Encrypted / protected | **Not indicated, not tested.** Nothing to circumvent was found and no circumvention was attempted |
| In the container at all | **No.** Recovery needs the SPI flash dump of the board or a DSP image from another source |

## 5. Cross-version diff (v1.44, v1.46, v1.64, v1.66)

Added after the release history showed DSP-side fixes shipping in OS updates (v1.52 to v1.64: Ring Mod, Vox organ, Reverb after Rotary, 56 voices in v1.30). Inputs: the four updaters under `.reference/` (v1.44, v1.46, v1.64 as `.zip` holding the `.exe`; v1.66 `.exe`). Each firmware resource was cut at the `SRAM` header (`PE offset 0x7A3B30` for v1.44/1.46, `0x7A3730` for v1.64; length `0x20 + size0 + size1` from the big-endian container header) and unpacked with `ns4 inspect <fw> --verify --unpack <dir>`. All six segments per version report `checksum=OK`.

| Version | `bank0_CODE` | `bank1_CODE` | `bank1_ARM0` |
|---|---|---|---|
| 1.44 | 5,355,456 B | 779,424 B | 748 B |
| 1.46 | 5,356,616 B | 779,424 B | 748 B |
| 1.64 | 5,499,872 B | 796,288 B | 900 B |
| 1.66 | 5,499,504 B | 798,912 B | 820 B |

Method: fraction of 64-byte blocks of version A (per 256 KB region) that occur anywhere in version B (all offsets, so code that moves still matches).

- `bank0_CODE 0x300000..0x3FFFFF`: **100% identical in every pair** (1.44-1.46, 1.46-1.64, 1.64-1.66). This is the 1 MB 16-bit-like high-entropy block from section 3. It did not change across the releases that changed Ring Mod, Vox, Reverb routing and voice allocation, so it is not DSP algorithm code (or those fixes did not touch it).
- ARM code `0x000000..0x23FFFF`: 12% to 73% match between versions (code moved and edited, as expected).
- `0x440000..0x500000`, new in 1.64 vs 1.46: about 60 runs of 1 to 9 KB with entropy 2.1 to 6.6 bits/byte (structured tables, several runs repeat the same length, for example 7,936 B at `0x49DA00`, `0x4C1800`, `0x4E6100`). None is near-random, none shows 6-byte structure in the scan of section 3.
- `bank1_CODE` matches only 54% to 63% between versions; the DSP name table is the same.

Reading: no version carries a block that looks like a replaced DSP image, yet DSP behaviour did change. Either the DSP changes are in small tables and ARM-side parameters (SHARC code unchanged, which fits e.g. "Reverb placed last" being a routing change sent from the ARM), or the SHARC code is stored in a form this scan cannot see. ASSUMED: SHARC code entropy is not necessarily high (ARM code here is 6.1 to 6.5), so entropy alone never ruled it out; the 6-byte-period sweep is the real test and it found nothing.

### 5.1 Decode of the changed `0x440000..0x4FFFFF` runs (v1.64)

Commands: Python snippets over `bank0_CODE.bin` unpacked from the v1.64 updater (period scores with `tools/fw_dsp_scan.py`, word dumps, `struct`).

- Period scores for the nine biggest runs (`0x49DA00`, `0x4C1800`, `0x4E6100`, `0x473900`, `0x48E1C0`, `0x445500`, `0x449480`, `0x45DD80`, `0x4515C0`): the lowest scores are at periods 4, 8 and 12 (for `0x49DA00`: `p4=5.34`, `p6=5.39`, `p8=5.24`, `p12=5.10`; `p5=5.99`, `p7=5.98`). Period 6 is never below period 4. Result: 16/32-bit-structured data, not a 48-bit SHARC stream.
- The three 7,936-byte runs at `0x49DA00`, `0x4C1800`, `0x4E6100` are the same stream at different phase offsets (e.g. `96100c00 76141210 0c007a0b` appears at `0x49DA00+12` and at `0x4C1800`). It is a sequence of 16-bit items; `0x000C` recurs every 6 bytes and `0x3F68`, `0x3E7A`, `0x3E64`, `0x3F6A` recur often. No pointer in `bank0_CODE` references any run start (checked 9 run starts with base `0xC0000000`).
- Other runs hold float32 tables (`0x443780`: -2.0708, -2.5347 ...), zero-padded descriptor-like records (`0x4515C0`, `0x48E1C0` with 1.0f, 3.0f) and 16-bit item streams (`0x473900`).
- Self-similarity over `0x440000..0x4FFFFF` (byte equality at shift `s`): best `18.5%` at `0x24AE4`, `18.3%` at `0x23BEC`. So the region is several similar but variable-length chunks, not fixed copies.

Reading: ARM-side data (tables and 16-bit item streams, probably parameter or program/routing descriptors) that changed with the DSP-related releases. No SHARC code found. The stream format is not decoded (ASSUMED role: parameter data sent to the SHARCs). Still open: which code reads these streams.

### 5.2 Downloaded updaters and release-to-release diff (v1.44 to v1.66)

Official links are recorded in `tools/nord_os_urls.py` (document ids found by `HEAD` probes, `302` = exists): v1.44 `767`, v1.46 `783`, v1.52 `899`, v1.54 `906`, v1.60 `945`, v1.62 `947`, v1.64 `949`, v1.66 `1042` (`.dmg` `1041`). The zips are saved in `.reference/` (git-ignored); v1.62 was downloaded but not yet unpacked here.

Unpacked with `ns4 inspect <fw> --verify --unpack` (all checksums OK). Findings:

- `bank1_CODE` (flash tool and DSP region table) is **byte-identical in v1.52, v1.54, v1.60, v1.64** (SHA-1 prefix `f408734ae2`) and differs from v1.46 (`c55a9087b7`). So the DSP-related releases changed only the main OS image, not the DSP name table.
- `bank0_CODE 0x300000..0x3FFFFF` is 100% unchanged in every pair from v1.44 to v1.66.
- Data new in `bank0_CODE 0x300000+` per release: v1.46 to v1.52 about 98 KB, v1.52 to v1.54 about 22 KB, v1.54 to v1.60 about 96 KB (116 KB incl. gaps in 108 runs), v1.60 to v1.64 about 20 KB. The big ones (v1.52 Vox update, v1.60 reverb routing and voice allocation) match the releases that touched sound. All of it sits in the same `0x440000..0x4FFFFF` runs as section 5.1: 4-byte-structured tables (`p4`/`p8` lowest, `p6` never lowest), for example a float32 curve `-47.5, -43.6 ... 0.0` at `0x455480` in v1.60.
- No version shows a new block with 6-byte (48-bit) structure.

Reading: sound changes between releases land in ARM-side tables and code of the main image. This is consistent with the SHARC code being stored elsewhere (SPI flash) and the DSP being driven by data from these tables, but it does not prove it.

Not done: a run-by-run check of the new `0x440000+` runs for SHARC opcodes, and v1.60 / v1.62 (not available).

## Implications

- The 1:1 verdict does not change from "not currently feasible", but the reason is sharper: the DSP code is simply not in the OS update, so no amount of container work recovers it. Track B needs a hardware flash read (`Micron N25Q032` / `MT25QL128A`) or an image from another source.
- `docs/research/ns4-firmware-teardown.md` rows for `0x003CF06C..` describe packed-container offsets (bank 1), not DSP images. `docs/research/sharc-dsp-and-preset-hash-teardown.md` and `docs/TECH_DEBT.md` TD-06 are right that the `.ldr` is outside the update.
- `docs/research/1to1-emulation-feasibility.md` row "DSP images encrypted?" was misleading: the compression result concerns ARM sections only. Updated.
- Continue to tune the high-level engine against recordings and against firmware tables read from `bank0_CODE`.

## Follow-ups

- Trace `bank0_CODE` code that touches `0x62004000` / SPI0 `0x01C41000` to confirm whether the ARM ever streams data to the SHARC at runtime (DSP comm `0x6CA48` is the known sender of table words).
- If hardware access exists: dump the SPI flash and look for the ADI loader stream there; ADI's loader documentation (not in `.reference/sharc/`) is needed for the block layout.
- Check the `.dmg` updater for any additional payload (not done here).
- Decode the changed `0x440000+` runs between v1.46 and v1.64 (section 5); they are the only data that moved with the DSP-related releases.
- Remove or relabel the 16-bit PCM-like block in `OPEN-QUESTIONS.md` once identified.
