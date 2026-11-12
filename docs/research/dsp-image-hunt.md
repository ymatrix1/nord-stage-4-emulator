# Research: Are the SHARC DSP images in the OS v1.66 firmware?

- **Checked date:** 2026-11-12
- **Sources (local, never committed):** `.reference/Update Nord Stage 4 OS v1.66.exe`, `target/fw_unpacked/{bank0,bank1}_{CODE,SRAM,ARM0}.bin`, `target/fw_unpacked/fw_v1.66.bin` (PE resource 101), `.reference/sharc/ADSP-214xx_hwr_rev1.1.pdf` (SHARC hardware reference), `tools/fw_xref.py`, `tools/fw_dsp_scan.py` (new).
- **Question:** Are the `DSPO` / `DSPS` / `DSPF` SHARC ADSP-21489 images present and recoverable in the OS v1.66 update, and where?

## Verdict

**Update (section 5.5): SHARC VISA code is present in `bank0_CODE` at `0x440000..0x4FFFFF`.** Read as 16-bit little-endian parcels, the framed records tile exactly into ADSP-214xx VISA instructions (97 of 97, null streams 19%), and 96% of the loads into index registers carry SHARC internal-RAM addresses. The earlier conclusion below ("not in the container") was wrong for this region: my 6-byte (48-bit ISA) scan could not see 16/32/48-bit VISA code. Operand-level decoding, the full extent and the mapping to `DSPO` / `DSPS` / `DSPF` are still open.

Earlier verdict (sections 1 to 4, kept for the record): the update carries only ARM images (two banks: `SRAM`, `CODE`, `ARM0` each). The strings `DSPSyn` / `DSPOrg` / `DSPFxCore*` are memory-region names in a bank 1 flash/test tool, not image names. No 48-bit ISA instruction stream and no ADI loader stream exists in any segment of either bank. The `0x3CF040` offset in older notes is in the *packed* container, not in `bank0_CODE`.

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

### 5.3 v1.62 unpacked: diff against v1.60 and v1.64

Unpacked from `.reference/Update Nord Stage 4 OS v1.62.zip` (firmware cut at `SRAM` header, `ns4 inspect --verify --unpack`, all six checksums OK). SHA-1 prefixes of the unpacked segments:

| Segment | v1.60 | v1.62 | v1.64 |
|---|---|---|---|
| `bank0_CODE` | `0960cb9c` (5,498,016 B) | `e5a5f6f0` (5,499,872 B) | `933fe78d` (5,499,872 B) |
| `bank0_SRAM` | `7484db05` | `e2a775f3` | `e2a775f3` |
| `bank0_ARM0` | `65f0e1fd` | `726f8bd1` | `726f8bd1` |
| `bank1_CODE`, `bank1_SRAM`, `bank1_ARM0` | same | same | same |

- **v1.62 vs v1.60:** 1.15 MB of `bank0_CODE` differs (1,388 runs, mostly ARM code shifted by an edit) plus 20 KB of data at `>= 0x300000`; `bank0_SRAM` and `bank0_ARM0` changed. This is the reverb-tail fix on top of the v1.60 rebuild.
- **v1.62 vs v1.64:** only **3 bytes** differ in `bank0_CODE`:
  - `0x27CBB0`: `0xA2` to `0xA4`, and `0x27CBB4`: `0x0B64` to `0x0B66`. They sit in a structure of function pointers and small numbers right next to `0xC007A670` (ASSUMED: version `162`/`164` and build counter `2916`/`2918`).
  - `0x45D63C`: byte `0x74` to `0x70` (16-bit item `0x0074` becomes `0x0070`), inside the 16-bit item streams of section 5.1 (run near `0x45D640`). This single byte is the whole functional change of v1.64 ("program under rare conditions could become silent").
- So v1.64 fixed an audible bug by editing one byte in that data stream. A stream that can silence a program is not a passive table: it configures or drives the sound path. Hypothesis (ASSUMED, untested): the 16-bit-aligned items are parcels of SHARC VISA code (16/32/48-bit instructions, per the hardware reference's VISA description), or a command script interpreted on the ARM and sent to the SHARC. Test: decode `0x45D600..0x45D680` as VISA and as a command stream.

### 5.4 Decode attempt of the `0x45D600` stream (ARM, Thumb, records, SHARC addresses)

Bytes: v1.64 `bank0_CODE@0x45D5EA..0x45D8C0` (v1.62 differs only at `0x45D63C`). Scripts were ad hoc Python with `capstone` 5.0.7 (ARM and Thumb) and `struct`.

- **Not ARM, not Thumb:** ARM decode of `0x45D600..0x45D680` has invalid words and unrelated condition codes; Thumb decodes without errors but into noise (`ldr r6,[r7,#0x2c]`, `asrs r7,r7,#2`, `lsrs r4,r4,#0x1e`, branches into the data).
- **Record framing (VERIFIED by dump, 26 records between `0x45D3xx` and `0x45D900`):** every record starts with the 4 bytes `20 00 1a 72` and ends with `86 14 00 00`. Lengths 16, 38, 72, 74, 96 bytes; across `0x440000..0x4FFFFF` there are 98 headers with gaps 16 (x64), 72 (x12), 38 (x8), 96 (x4).
- **The v1.64 fix in context:** the 38-byte record at `0x45D630` carries `a40f XXXX` where `XXXX` is `0x0074` in v1.62 and `0x0070` in v1.64. The same field in the neighbouring records is `0x006c`, `0x0078`, `0x007a`. So the fix changes one operand/parameter of one record, not the stream layout.
- **SHARC-like addresses:** inside the stream, 16-bit parcel pairs `0c00 d803`, `0c00 da03`, `0c00 dc03`, `0c00 de03` (then `e003 .. e603`) read as `0x000C03D8` stepping by 2, i.e. addresses in the SHARC internal RAM range of the DSP region table (`0x90000`, `0xB0000`, `0xC0000`, section 1). Counting 16-bit parcels in `bank0_CODE`:

  | Region | Parcel `0x000C` | Share |
  |---|---|---|
  | `0x440000..0x4FFFFF` (item streams) | 6,835 of 393,216 | 1.74% |
  | `0x300000..0x43FFFF` (PCM-like block) | 180 of 655,360 | 0.03% |
  | `0x000000..0x26FFFF` (ARM code) | 3,141 of 1,277,952 | 0.25% |

  Prefixes `0x000B` (3,063), `0x0008` (917), `0x0009` (735), `0x000E` (829) are elevated in the same region as well. Uniform data would give 6 per prefix.
- **Verdict on the two hypotheses:** the stream is not ARM data and is framed like a command or code script that addresses SHARC internal RAM. Both "VISA code block" and "command stream sent to the SHARC" remain open. It could not be decided without SHARC opcode tables: the hardware reference in `.reference/sharc/` has no instruction encoding, and no open-source SHARC/VISA disassembler or SLEIGH spec was found (GitHub repository and code searches returned nothing; a web search found none).

Next step: obtain the ADI *ADSP-214xx SHARC Processor Programming Reference* (instruction set appendix with VISA opcode maps), check whether the `20 00 1a 72 ... 86 14 00 00` records decode as a consistent VISA instruction sequence (lengths 16/32/48 bits matching the record lengths) and what `0x7a1a`/`0x6afe`/`0x687e` parcels are.

### 5.5 The `0x440000+` streams decode as SHARC VISA code

Tool: `tools/sharc_visa.py` (tests in `tools/tests/test_sharc_visa.py`). Rules from the SHARC Programming Reference rev 2.4, chapter 10 (`.reference/adsp-2136x_2137x_214xx_pgr_rev2.4.pdf`, downloaded from analog.com): VISA instructions are 16, 32 or 48 bits (`c`/`b`/`a` types), decoded MSB-first from bit 47; 32-bit forms have `0111111` in bits 22..16. Parcels are read little-endian. The tool only recovers type and boundaries; operands are decoded just for Types 17a/17b (`Ureg = <data>`).

Commands:

```bash
python tools/sharc_visa.py target/fw_unpacked/bank0_CODE.bin 0x45d5ec 0x45d630
```

Evidence (all on `bank0_CODE` of v1.64):

| Check | Result |
|---|---|
| Records between headers `20 00 1a 72` tiled into instructions starting 2 bytes after the header | **97 of 97** tile exactly. Starting at the header (offset 0), -2 or +4: 0 of 97. +6 (3 parcels): 97 of 97 (instruction boundaries realign) |
| Null: random spans of ARM code or the PCM-like block with the same length distribution | 378 of 2000 (19%) and 403 of 2000 (20%) tile exactly |
| 32-bit forms end with `0x3F` in the low 7 bits of the second parcel | The second parcels `003f`, `00bf`, `043f`, `04bf`, `503f`, `e73f`, `143f` all do (that is how 5b/4b/7b are recognised) |
| `Ureg = data32` loads (Type 17a): 773 found | 296 target `I0..I15`; **284 (96%)** load values in `0x80000..0xFFFFF` (SHARC internal RAM), for example `I5 = 0xC04B0`, `0xC0458`, `0xC03D8`. Same range as the DSP region table of section 1 |
| `Ureg = data16` (Type 17b) | `0fa4 006c` is `M4 = 0x006C` (UREG code `0x24` is `M4` in Table 10-2); the v1.62 to v1.64 fix changes the immediate `0x0074` to `0x0070` in this instruction |
| Terminator `1486 0000 0020` | Type 18a (bit manipulation with a 32-bit immediate); every record ends with it. Operand meaning not decoded |

Sample (v1.64, `0x45D5EC`): `4a 6abe 1088 12ae | 4a 6abe 2108 13bf | 15b 9b08 0186 | 17b 0fa4 0028 (M4=0x28) | 7b 043e e73f | 17a 0f15 000c 03d8 (I5=0xC03D8) | 4b 687e 143f ...`. These are DAG setups (index and modifier registers) followed by moves.

Conclusions and limits:

- The Nord Stage 4 OS image **does contain SHARC code**, uncompressed, as 16-bit-aligned VISA, inside the ARM `CODE` segment. It is not in ADI `.ldr` form; the ARM presumably sends it to the DSPs (see `DSP Comm` at `bank0@0x29ED9C`, sender `0x6CA48`).
- ASSUMED: the instruction typing is a best-effort reading of the opcode tables (no ADI disassembler was available to cross-check). Full-register and operand decoding is not done. `??`/`21/22` counts include padding and data mixed in the long gaps between records, so the extent of code versus data is not measured.
- v1.64 changed one immediate of a DAG setup; plausible for a "program becomes silent" fix. v1.52 to v1.60 changed 20 to 98 KB of this region (section 5.2): that region is where the sound-engine changes live.

### 5.6 Operand decoder and code sweep

`tools/sharc_visa.py` now decodes operands (`decode`) and sweeps for code (`--sweep`, function `sweep`). Layout sources: SHARC Programming Reference rev 2.4, chapter 10 (type diagrams, Tables 10-2 and 10-4) and chapter 12 (ALU opcodes, Tables 12-3 and 12-4). Decoded fields: `COND` (bits 37-33), universal registers, `DM/PM` moves of types 3a/3b/4a/4b, `Ureg = Ureg` (5a/5b), immediate loads (17a/17b), `BIT` ops (18a), branches (8a), `RTS`/`RTI`, `DO` loops and the single-function ALU compute field. Other forms (multiplier, shifter, multifunction, 1a, 2b, 6a, 7x, 9x, 10a, 14a to 16b) print as `type? raw ...`.

Layout check on the data: the 5-bit window that is `11111` (TRUE, unconditional) most often is bits 37-33 for types 4a (98%), 3a (98%), 5a (99%), 7b and 9b (100%), 8a (87%); this matches the manual's `COND` position (`bank0_CODE` v1.64, cluster at `0x491800`). Another cross-check: the compute field of a `4a` instruction decodes as ALU opcode `1010 0001` = `Fn = PASS Fx` (Table 12-4), a plausible no-op/move.

Listing (v1.64, `python tools/sharc_visa.py target/fw_unpacked/bank0_CODE.bin 0x45d5ec 0x45d650`):

```text
0x45d5ec 4a  6abe 1088 12ae   DM(2, I5) = R1, F2 = F10 + F14
0x45d5fc 17b 0fa4 0028        M4 = 0x28
0x45d604 17a 0f15 000c 03d8   I5 = 0xc03d8
0x45d60a 4b  687e 143f        R8 = DM(2, I4)
0x45d61a 4a  6a3e 060a 1880   R12 = DM(0, I5), F8 = PASS F8
0x45d620 18a 1486 0000 0020   BIT TST ASTATx 0x20
0x45d626 5b  721a 043f        IF TF R4 = M0
```

Reading: DAG setup (`I5`, `M4`), data reads with float compute, then `BIT TST` followed by a conditional move on `TF`. The pair `BIT TST` then `IF TF` is the usual test-then-branch idiom, which supports the decode. ASSUMED: the operand fields of 5a/5b and the 4a/4b immediate offset are read from the diagrams; the `R4 = M0` register pairing in particular is not confirmed.

**Sweep** (`--sweep`, window 1 KB, hit = Type 17a load of a value in `0x80000..0xFFFFF` into `I0..I15`, merged within 8 KB): ARM code `bank0_CODE 0..0x26FFFF` gives **0** hits. The OS images from v1.44 to v1.66 all contain the same set of code-like blocks in `0x440000..0x52FFFF`. v1.64 (offsets `0x448000..`):

| Block | Span | Size | Hits | Notes |
|---|---|---|---|---|
| A | `0x448000..0x44C800` | 18 KB | 96 | 58% of its 32-byte windows also occur in B |
| B | `0x45CC00..0x464400` | 30 KB | 90 | contains the `20 00 1a 72` records of section 5.3 |
| C | `0x470400..0x474400` | 16 KB | 101 | 15 to 19% overlap with A and B |
| D | `0x491800..0x4A2400` | 67 KB | 228 | |
| E | `0x4B5800..0x4C6000` | 66 KB | 222 | 98 to 99% identical to D and F |
| F | `0x4DA000..0x4EA800` | 66 KB | 227 | |
| G, H | `0x517400..0x519000`, `0x523C00..0x525800` | 7 KB each | 26, 28 | 8% overlap with each other |

Smaller isolated hits (1 to 10) sit at `0x44EC00`, `0x477800`, `0x4AF000`, `0x4D3000`, `0x4F7800`, `0x4FE000`; they may be data that happens to decode or short stubs. Total code-like span is about 270 KB. D, E and F are three copies of nearly the same 66 KB program (E is at +`0x24000` from D, F at +`0x24800` from E).

Hypothesis (ASSUMED, untested): the bank 1 table lists six core images (`DSPFxCore1/2`, `DSPOrgCore1/2`, `DSPSynCore1/2`); six clusters A to F match that count, and the three near-identical D/E/F could be one program per instance. Mapping clusters to cores needs the ARM code that sends them (`DSP Comm`, sender `0x6CA48`) and the L1 addresses used.

### 5.7 Decoder coverage and the ARM sender (negative result)

**Decoder coverage** (v1.64, the six large blocks A to F, 56,663 instructions tiled from each block start): `tools/sharc_visa.py` now decodes **94.2%** of instructions (3,299 stay `type? raw`) and 86.8% including the compute field (4,207 compute fields stay raw). Added since 5.6: Types 14a/15a (`DM|PM(addr32) = ureg`, e.g. `DM(0xc02c0) = R3`, `R0 = DM(0x106be)`: the first is a SHARC RAM address, which supports the layout), 2b/2c (short compute: `F3 = F3 * F4`, `F0 = F0 + F4`), multiplier opcodes (Tables 12-5 to 12-7), dual ALU add/subtract, 3c and 15b. 15b field split (`I` 43..41, `D` 40, `DATA7` 29..23, `UREG` 22..16) is ASSUMED from bit-frequency statistics. Still raw: multifunction compute (MUL+ALU, MUL+dual ALU), shifter opcodes (Table 12-11), types 1a, 7a/7b, 9a/9b, 10a, 16a/16b, 19a, `??` and zero padding.

**ARM sender** (searched, not found). Results, all on `bank0_CODE` v1.64:

- `DSP Comm` (`0x6CA48`) is a message FIFO: it limits a message to `0x40` entries / `0x100` bytes, polls a ready bit in a peripheral register and advances a ring. It sends control and parameter messages, not code images.
- No aligned 32-bit word in the ARM code or tables points at the start of any code block A to H, and none equals a round boundary (`0x430000..0x440400` or `0x520000..0x526000`). The 793 pointers into `0x440000..0x52FFFF` from outside land in the data tables at `0x441xxx..0x446xxx` (floats, coefficient tables) and the string area.
- Data-processing immediates in `0x440000..0x52FFFF` (`0x500000`, `0x480000`, `0x4d0000`, `0x4a0000`) occur 11 times, none in a copy loop I could identify.

Reading: the blocks are not addressed by absolute pointers, so they are reached by base plus offset or are copied as a whole to a peripheral from a loop that uses a table elsewhere (for example the flash tool in bank 1, which programs the SPI flash; the SHARCs then boot from that flash, per the hardware reference). That would explain why the update carries the code as VISA: the OS update may write it to the SPI flash. Untested: trace the bank 1 `Flash Tool` / `Update Image Log` routines (`bank1_CODE@0x919F4`, `0x91A88`) for a copy from `0xC0440000..`.

### 5.8 Shifter and multifunction compute fields; more on the sender

`tools/sharc_visa.py` now also decodes the shifter opcodes (Table 12-11) and the multifunction fields (multiplier + ALU from Table 12-12, multiplier + dual ALU; layouts from the compute-field diagrams in chapter 12). Compute fields that stay raw in the six large blocks of v1.64: 70 (was 4,207). Coverage: 94.2% of the 56,663 instructions have a decoded type, 94.1% decode fully including compute. Examples seen in the blocks: `R9 = DM(60, I2), R1 = R1 * R5 (SSF)`, `R1 = DM(I4, M2), R0 = R0 * R12 (SSF)` (a multiply-accumulate-style filter loop body).

Raw instructions left (3,299): `??` with first byte `0x02` (1,103; type not in the opcode diagrams read so far), `21/22` with extra bits set (742), 19a (479), 1a (391), 9b (222), 7b (181), 1b (56), 10a (46), 6a (23), 7a (22), 16a (9).

Sender: `DSP Comm` (`0x6CA48`) has more than 1,000 `bl` call sites across `bank0_CODE`, so it is the generic parameter-write path (confirms section 5.7). It does not carry the code blocks. The strings near `0x5263A8..0x527278` ("Piano Flash Error", "Sample Flash Error", "DSP/Chip fail") belong to piano/sample flash handling, not to a DSP loader. The path that puts these blocks into the SHARCs is still unidentified.

### 5.9 The `0x02` form and coverage

**First byte `0x02` (undocumented in the reference I read).** 1,103 instructions in the six large blocks of v1.64 start with `0x02` (bits 47..40 = `000 00010`; Type 2a is `000 00001`). Evidence from the data (`bank0_CODE` v1.64): bits 39..32 are `0x3E` in 1,073 of 1,103 (so bits 37..33 = `11111` = unconditional `TRUE` and bit 32 = 0), and the low 23 bits are a valid compute field (multiplier forms such as `R0 = MRF + R0 * R2 (SUIR)`, `R0 = SAT MRF (UI)`). Bits 31..23 hold extra non-zero values (`0x00`, `0x10`, `0x78`, `0x30`, `0x20`, `0x40`) whose meaning is unknown. `decode` prints these as `IF cond compute  [extra bits 31..23 = 0x..]` (type name `2x`, ASSUMED to be a 48-bit conditional compute with an extra field). Not found in Table 10-1 to 10-4 or the type diagrams of rev 2.4.

**New decoding:** multiplier register ops (`SAT`, `RND`, `MRx = 0`; Table 12-5), index-register modify with immediate (Type 19a, `I4 = MODIFY(I1, 0x12)`; bit-reverse form too). Types 1a, 1b, 6a, 7a, 7b, 9a, 9b, 10a, 16a, 16b are now named by type (`TYPE_ONLY` in the tool) with raw operands, so they no longer count as unknown.

**Coverage** (v1.64, six large blocks, 56,663 instructions):

| Measure | Result |
|---|---|
| Type recognised | 98.7% (762 unnamed raw) |
| Operands fully decoded | 95.4% (955 type-only, 881 with raw compute) |
| Unnamed raw left | 742 `21/22` forms (first byte `0x00` with other bits set; look like inline data), 16 `??`, 3 `20a`, 1 `25` |

The `21/22` rows sit where the tiling sees `000 00000` first bytes with non-zero low bits (for example `0001 76be 803f`); the likely reading is data words inside the blocks, not instructions.

### 5.10 Blocks D, E, F compared as instruction streams

Method: tile each block with `tools/sharc_visa.py` (v1.64: D `0x491800..0x4A2400`, E `0x4B5800..0x4C6000`, F `0x4DA000..0x4EA800`) and align the instruction sequences with `difflib.SequenceMatcher` (a positional parcel diff is misleading, the copies are shifted by insertions).

| Pair | Instructions | Identical after alignment | Differing hunks |
|---|---|---|---|
| D vs E | 14,551 / 14,376 | 14,274 (98.1%) | 34 (mostly one instruction) |
| E vs F | 14,376 / 14,352 | 14,222 (98.9%) | 26 |

The differences are structured constants, not code changes:

- `USTAT1 = 0x200c0000` (D, F) vs `USTAT1 = 0x20080000` (E), and `USTAT1 = 0x200fa984` vs `0x200ba984`: one bit (bit 18) of a user-status value differs. D and F agree, E differs.
- Memory addresses with bit 7 flipped: `DM(0xb97c28a4, I5) = M4` (D) vs `M3 = DM(0xb97c2824, I6)` (E); `PM(0x28a40070, I12) = B0` vs `PM(0x28240070, I12) = B0`; `MODE2 = PM(0x28a4002c, I13)` vs `...2824002c...`. The index register changes with it (`I5` vs `I6`).
- One large hunk at each end of the block: the head (145 instructions in D, 50 in E, tiled as `21/22` and odd forms such as `0000 0b3e 0000` repeated) is data, not code, and differs per copy; the tail has start-up style code (`JUMP 0x125dad`, `BIT SET IMASK 0x800`, `BIT CLR IMASK 0x8`, `BIT CLR MODE1 0x202000`), i.e. interrupt-mask and mode setup. So each block starts with a per-instance data header and ends with initialisation code.

Reading (ASSUMED): D, E and F are one program assembled for different instances (core or bank variants) with a few per-instance constants. That fits the bank 1 table, which lists a Core1 and a Core2 image per DSP. It does not yet say which of Fx, Organ or Synth a given copy belongs to. The tidy single-bit differences are also evidence that the decoder reads the fields correctly (a wrong field split would not produce clean one-bit deltas).

### 5.11 Bank 1 carries five SHARC programs and the ARM routine that downloads them

Method: sweep with `tools/sharc_visa.py` on `bank1_CODE`, then read the ARM code that uses the pointers found in the span (capstone, ARM mode).

```bash
python tools/sharc_visa.py target/fw_unpacked/bank1_CODE.bin 0 0xC30C0 --sweep
# 0x092c00..0x09f800 size 0xcc00 index-register loads in SHARC RAM: 66   (bank0 sweep outside 0x440000..0x528000: none)
```

- **SHARC code in bank 1.** `bank1_CODE@0x92C00..0x9F800` (about 52 KB) decodes as VISA with SHARC RAM addresses (`I0 = 0x92196`, `I4 = 0xC0725`, `I1 = 0xC072F`).
- **Download routine (VERIFIED by disassembly).** The function starting at `bank1_CODE@0x480E0` is the bank 1 DSP self-test (the `DSP Quick/Total` entries of the test menu at `0x68B88`; strings `%s (%d) Boot`, `Err:%s Reset`, `Err:%s Mem %s`, `L1 bl.N` block names). A jump table (`ldrls pc, [pc, r5, lsl #2]` at `0x481A4`) selects one of five blobs; each case loads a pointer and a length from the literal pool (`0x486FC..0x48764`) and then calls a virtual method of the DSP driver object with the index, the pointer and the length (`blx` at `0x48718`), checking the status (`cmp r0, #6`). So the code copy is a driver call, not an inline loop.

| Case | Pointer | Length | End |
|---|---|---|---|
| 3 | `0xC00924F8` | `0x3AB4` | `0x95FAC` |
| 4 | `0xC0095FAC` | `0x3B14` | `0x99AC0` |
| 1 | `0xC0099AC0` | `0x1E00` | `0x9B8C0` |
| 2 | `0xC009B8C0` | `0x1C4C` | `0x9D50C` |
| 0 | `0xC009D50C` | `0x23F8` | `0x9F904` |

(Case numbers are the order of the code at `0x486FC..0x48760`; they are not checked against the jump table order.) The five blobs chain back-to-back, `0x924F8..0x9F904` (54,284 bytes), with no gaps; this is strong evidence that the pointer and length pairs are the image boundaries.

- **Decode quality.** Blobs 2 to 4 tile as VISA with 73 to 86% of instructions typed and start with `IF LT PM(I8, M8) = S8, ...` style code; blobs 0 and 1 begin with zero padding and tile less well (46 to 55% typed; data mixed in).
- **Relation to the bank 0 blocks (32-byte windows found in each block).** Blob 4: 40% in block A, about 15% in D/E/F. Blobs 2 and 3: about 18 to 20% in A and 25 to 28% in D/E/F. Blobs 0 and 1: 9 to 16% in G/H, about 0 elsewhere. So the bank 1 test programs and the bank 0 blocks share routines but are not copies.
- **What this settles.** The OS images do carry SHARC programs, and bank 1 shows how an image is handed to a DSP: pointer, length and index passed to the DSP driver. The bank 0 equivalent has not been found: no literal pool in `bank0_CODE` holds pointers to the A to H blocks (section 5.7), so bank 0 probably reaches them through an array of offsets or a generated table.

Follow-up (done in 5.12): the bank 0 caller is `bank0_CODE@0x241C58`. Still open: the driver method behind the virtual call.

### 5.12 The bank 0 caller: five DSP images, exact boundaries

The bank 0 counterpart of the bank 1 download routine is the function at `bank0_CODE@0x241C58`. It makes the same virtual driver call five times, `r0 = driver object`, `r1 = index`, `r2 = pointer`, `r3 = length`, via `ldr r2,[r4]; ldr r5,[r2]; blx r5`, and on a non-zero status (at most 6) maps the code through an error table at `0xC0444CA0 + 0x234 + 4 * code`. Found by searching `bank0_CODE` for pointer and length pairs where `pointer + length` equals another pointer value in the file (24 chains; the four relevant ones sit in the literal pool at `0x241DAC..0x241DD4`):

```bash
python - <<'PY'   # chain search used (bank0_CODE.bin, base 0xC0000000)
import struct, collections
d = open("target/fw_unpacked/bank0_CODE.bin", "rb").read(); B = 0xC0000000
W = [struct.unpack_from("<I", d, o)[0] for o in range(0, len(d) - 3, 4)]
P = collections.defaultdict(list)
for i, w in enumerate(W):
    if B + 0x1000 <= w < B + len(d): P[w - B].append(i)
for p, locs in P.items():
    for i in locs:
        for j in range(max(0, i - 4), min(len(W), i + 5)):
            if j != i and 0x400 <= W[j] <= 0x40000 and (p + W[j]) in P: print(hex(p), hex(W[j]))
PY
```

| Index | Pointer (`bank0_CODE`) | Length | End | DSP name (see below) | Code blocks inside |
|---|---|---|---|---|---|
| 0 | `0x44667C` | `0x28410` (164,880) | `0x46EA8C` | FX | A, B |
| 1 | `0x46EA8C` | `0x46824` (288,804) | `0x4B52B0` | PNO | C, D |
| 2 | `0x4B52B0` | `0x486D0` (296,656) | `0x4FD980` | ORG | E, F |
| 3 | `0x4FD980` | `0x1DBF4` (121,844) | `0x51B574` | SY1SY2 | G |
| 4 | `0x51B574` | `0x0AE34` (44,596) | `0x5263A8` | SMP | H |

The five images chain exactly (each end equals the next pointer) from `0x44667C` to `0x5263A8`, 916,780 bytes in total; `0x5263A8` is where the UI string area starts (`Layer Level Adjust`). The index order here is the one in the bank 1 jump table (`bank1_CODE@0x481AC`: index 0 to `0x9D50C`, 1 to `0x9B8C0`, 2 to `0x99AC0`, 3 to `0x95FAC`, 4 to `0x924F8`), so bank 1's small test programs (section 5.11) are the same five slots; their overlap with the bank 0 blocks agrees with the index mapping (index 0 blob: 40% of block A; indices 1 and 2: 25 to 28% of D/E/F; indices 3 and 4: 9 to 16% of G/H).

**DSP names (ASSUMED; supported by 5.13).** The name array at `bank1_CODE@0x5D78` is `[FXFX, PNO, ORG, SY1SY2, SMP]` (the test prints `%s (%d)` with name and id), so the five slots are the FX, piano, organ, synth and sample DSPs, in that order. That the download index equals this id is likely (the same index selects the blob and the name in the test) but not proven.

**Image structure.** Images 0 to 2 start with a header. First 24 bytes as stored (hex): FX `03 50 17 ac e3 11 12 00 00 00 00 00 44 4d 01 00 03 00 80 ac 00 40 00 20`, PNO `03 50 dc ac 07 10 12 00 00 00 00 00 08 2c 02 00 03 00 82 ac 00 40 00 20`, ORG `03 50 b5 ac 88 5c 12 00 00 00 00 00 c4 4a 02 00 03 00 62 ac 00 40 00 20`. The 32-bit little-endian word at byte 12 is `0x14D44` (FX), `0x22C08` (PNO), `0x24AC4` (ORG), growing with image size (a size or code-length field, ASSUMED); the bytes 4 to 5 and 0 to 3 differ per image. Images 3 and 4 start with zeros. Header fields are not decoded.

Consequences:

- The five images are the complete SHARC programs plus data (about 917 KB); the earlier A to H blocks are the code parts inside them. D, E and F (near-identical code) sit in the PNO, ORG images (D in PNO, E and F in ORG), i.e. shared engine code, not one DSP's cores.
- Mapping `DSPFxCore*` / `DSPOrgCore*` / `DSPSynCore*` names (bank 1 table) to code is still open: ORG holds two copies (E, F) which fits Core1/Core2.
- The driver method (virtual slot 0 of the object passed in `r0`) is the remaining unknown: it decides whether the images are sent as they are or wrapped in loader headers. Its address has not been found yet.

### 5.13 The five images across releases (v1.44 to v1.66)

Tool: `tools/ns4_dsp_images.py` (tests in `tools/tests/test_ns4_dsp_images.py`) finds the pointer and length chain of section 5.12 without fixed offsets (longest chain of pointer/length word pairs within two words of each other; trailing entries that start in the UI string table are dropped) and can export the images with `--out DIR` (never commit them). It finds exactly five contiguous images in every release from v1.44 to v1.66.

```bash
python tools/ns4_dsp_images.py target/fw_unpacked/bank0_CODE.bin
```

Sizes in bytes (SHA-1 prefix of the image):

| Release | FX (idx0) | PNO (idx1) | ORG (idx2) | SY1SY2 (idx3) | SMP (idx4) |
|---|---|---|---|---|---|
| v1.44 | 171,320 `b9d0cd36` | 281,128 `afb23fa8` | 296,264 `687cfa4f` | 121,844 `f8d50a75` | 45,140 `1a6aeda7` |
| v1.46 | 171,320 `b9d0cd36` | 281,156 `25b12e9f` | 296,320 `0e191d65` | 121,844 `f8d50a75` | 45,140 `1a6aeda7` |
| v1.52 | 171,320 `b9d0cd36` | 292,284 `dd470a56` | 296,656 `6b4ee1e6` | 121,844 `f8d50a75` | 44,596 `06dc48ee` |
| v1.54 | 171,320 `280ac5cb` | 292,284 `78269040` | 296,656 `f6441551` | 121,844 `f8d50a75` | 44,596 `06dc48ee` |
| v1.60 | 164,880 `df5d27e6` | 288,804 `c77d3567` | 296,656 `f6441551` | 121,844 `f8d50a75` | 44,596 `06dc48ee` |
| v1.62 | 164,880 `df5d27e6` | 288,804 `c77d3567` | 296,656 `f6441551` | 121,844 `f8d50a75` | 44,596 `06dc48ee` |
| v1.64 | 164,880 `b5f3bf40` | 288,804 `c77d3567` | 296,656 `f6441551` | 121,844 `f8d50a75` | 44,596 `06dc48ee` |
| v1.66 | 164,880 `b5f3bf40` | 288,804 `c77d3567` | 296,656 `f6441551` | 121,844 `f8d50a75` | 44,596 `06dc48ee` |

Changes between consecutive releases, against the vendor update history:

| Step | Image changes | Release notes (summary) | Fit |
|---|---|---|---|
| v1.44 to v1.46 | PNO +28, ORG +56 | Delay tempo fixes (Mst Clk) | small, plausible |
| v1.46 to v1.52 | PNO +11,128, ORG +336, SMP -544, FX same | Vox treble and Vox/Farf onset (organ), Flanger fix for Piano B, silent layer for a missing sample | ORG, PNO and SMP change; large PNO growth with the piano flanger fix |
| v1.52 to v1.54 | FX 12 B, PNO 14 B, ORG 16 B differ (same size) | Ring Mod optimisation, Vox hanging-note fix | Ring Mod patch in the three images that carry the effect rack; ORG for Vox |
| v1.54 to v1.60 | FX -6,440, PNO -3,480; ORG, SY1SY2, SMP same | Reverb placed last, after the level fader and the Rotary Speaker | reverb code in FX and PNO |
| v1.60 to v1.62 | none | Reverb tail with Rotary on program change | ARM-side fix |
| v1.62 to v1.64 | FX: 1 byte differs | Program under rare conditions could become silent | the one-byte edit of section 5.3 (`0x74` to `0x70`) lies inside the FX image |
| v1.64 to v1.66 | none | Production test routines only | consistent |

**Reading (EMPIRICAL).** Every sound-engine release note in the history has a matching change in exactly the images one would expect, and the releases without sound changes (v1.62, v1.66) leave all five images byte-identical. The SY1SY2 image is identical in all eight releases. This supports three things: the five images are the real DSP programs, the index to name order of the bank 1 name array (FX, PNO, ORG, SY1SY2, SMP) is right at least for FX (reverb, Ring Mod, the v1.64 silent-program fix), ORG (Vox) and SMP (missing sample), and the code blocks A and B of section 5.6 are FX code. The match of the v1.64 byte with the FX image is the strongest single link (the byte is at `bank0_CODE@0x45D63C`, inside `0x4467EC..0x46EBFC` of v1.64).

Not proven: that the synth DSP is idx3 (it is the only image that never changes, which is odd given the synth fixes in v1.52 and v1.60; those may have been ARM-side), and the Core1/Core2 split inside each image.

### 5.14 Lookup tables inside the five images

Tool: `tools/ns4_dsp_tables.py` (tests in `tools/tests/test_ns4_dsp_tables.py`) lists runs of smooth little-endian float32 values in an exported image. Heuristic (ASSUMED): at least 48 finite floats, a run is cut where the step jumps to more than six times the median of the previous nine steps or where zero padding starts; each table is labelled `linear`, `exponential` (constant ratio) or `curve`.

```bash
python tools/ns4_dsp_images.py target/fw_unpacked/bank0_CODE.bin --out DIR
python tools/ns4_dsp_tables.py DIR/dsp_image_0_FX.bin
```

Results on the v1.66 images:

| Image | Tables (>= 48 floats) | Floats in tables | Share of image | Duplicate tables (>= 100 entries) and offset between the copies |
|---|---|---|---|---|
| FX | 34 (18 curve, 14 exponential, 2 linear) | 8,472 | 21% | 15; delta `0x136AC` (13 tables) |
| PNO | 71 | 24,089 | 33% | 6; deltas `0x1D1E0`, `0x1C1DC` |
| ORG | 82 | 15,472 | 21% | 33; delta `0x23BEC` (all 33) |
| SY1SY2 | 66 (64 curve) | 18,356 | 60% | 2; delta `0x3000` |
| SMP | 0 | 0 | 0% | none |

Findings:

- **Two copies of the tables per image.** In FX and ORG nearly every large table appears twice at one fixed distance (FX `0x136AC` = 79,532, ORG `0x23BEC` = 146,412), i.e. each image holds a per-core copy of its data for the two cores (matching the `Core1`/`Core2` pairs of the bank 1 table). The images carry no second header; the image header field at byte 12 (`0x14D44` FX, `0x22C08` PNO, `0x24AC4` ORG) is not equal to the copy distance and is still unexplained.
- **A shared table library.** 43 tables of at least 100 entries occur in two or more images with identical length, first and last value (for example the 1,025-entry table `-0.9985 .. 0.9451` in ORG and PNO, the 512-entry pairs `-1 .. -0.0031` and `0.0031 .. 0.3125` in FX, ORG and PNO, the 255-entry curve `8.702 .. 3.394` in FX and PNO). These are common DSP library tables, not engine specific.
- **Table shapes.** The FX table block at image offset `0xC5C0` is not one curve: it is thirteen sub-tables, among them eight 129-point curves (128 intervals, typical of interpolated lookups) in pairs, one of which is an exact octave (`2.016 .. 4.032`, constant ratio `2^(1/128)`), and four 255-point curves between `8.7022` and `3.3938` (and their negatives).
- **SY1SY2 is mostly data.** 60% of the synth image is float tables; it holds one run of 18,432 smooth floats (`-0.18 .. 1.0`) at image offset `0x76A8`, plausibly oscillator or wave material. SMP has no float tables (integer data or code only).

Use for Track A: the 129-point exponential tables (time and rate curves), the 512-entry pairs, the 255-point curves and the filter-related tables are candidates for VERIFIED constants once their role is identified (for example by matching them against the verified `bank0_CODE` curves of `ns4-firmware-teardown.md`). Not done here.

### 5.15 The synth image holds six 12-tap sinc interpolation kernels

Tool: `tools/ns4_synth_kernels.py` (tests in `tools/tests/test_ns4_synth_kernels.py`).

```bash
python tools/ns4_dsp_images.py target/fw_unpacked/bank0_CODE.bin --out DIR
python tools/ns4_synth_kernels.py DIR/dsp_image_3_SY1SY2.bin
# 6 kernels of 12 taps x 256 phases at image offset 0x76a8
# kernel 0: fc = 1.1000 ... kernel 5: fc = 0.8950, rms error vs model 0.00027 .. 0.00033
```

The 18,432-float run of section 5.14 (`SY1SY2` image offset `0x76A8`, 73,728 bytes) is six blocks of 3,072 floats, each a polyphase interpolation kernel: 12 taps x 256 sub-sample phases, stored `[tap][phase]`.

**Model (fits every block with rms 0.000036, max error 0.000088, against a peak of 1.0):**

```text
value[tap][phase] = sinc(fc * d) * window(d),   d = tap - 5 - phase / 256,   |d| < 6
window(d)         = 0.4231 + 0.4991 cos(pi d / 6) + 0.0778 cos(2 pi d / 6)
```

The sum over the 12 taps is `1 / fc` for every phase (the kernels are not normalised to unity gain): block sums divided by 256 are 0.9091, 0.9524, 1.0000, 1.0417, 1.0811 and 1.1173. Cutoffs `fc` (relative to Nyquist, as the argument scale of `sinc`) of the six kernels: **1.100, 1.050, 1.000, 0.960, 0.925, 0.895**. The identical table is present in every release from v1.44 to v1.66 (checked with `ns4_synth_kernels.py` on the SY1SY2 image of v1.44, v1.52, v1.60 and v1.66; the image never changes).

Reading: the sample playback path of the synth DSP resamples with a 12-point windowed-sinc polyphase interpolator at 256 phases, and picks one of six cutoffs, plausibly by playback ratio (a higher ratio needs a lower cutoff to avoid aliasing; the selection rule is not decoded). The window is a three-term cosine sum of half-width 6 taps whose coefficients (0.4231, 0.4991, 0.0778, sum 1) were fitted jointly to all six blocks (EMPIRICAL); it is close to, but not exactly, the textbook Blackman window (0.42, 0.5, 0.08), which leaves rms 0.0003. Status: **VERIFIED** structure, cutoffs and table values (reproducible fit, same in all releases); the window coefficients are an EMPIRICAL fit; the assignment of kernels to pitch ratios is ASSUMED.

Track A: `crates/nord-core/src/sinc.rs` implements these kernels (golden-tested against nine values read from the firmware table; on a 0.3 fs sine its error is at least 5x below Catmull-Rom). `docs/specs/piano-engine.md` and the `SAMPLES` oscillator still use 4-point Catmull-Rom interpolation (TD-08). The hardware path is a longer sinc kernel; the difference matters for high-frequency fidelity at large transpositions.

### 5.16 dB taper tables (129-point exponential curves)

`python tools/ns4_dsp_tables.py DIR/dsp_image_N_NAME.bin --db` now prints the dB range of exponential tables. The images share 19 distinct 129-point (128-interval) exponential tables, i.e. a knob or parameter index 0..127 (plus a guard point for interpolation) mapped to a linear gain that is linear in dB. Endpoints, in dB (`20 log10 |value|`), by table (v1.66):

| First value | Last value | dB range | dB/step | Found in |
|---|---|---|---|---|
| 0.0004 | 0.4648 | -67.96 .. -6.66 | +0.479 | FX, PNO |
| 0.0008 | 1.1859 | -61.94 .. +1.48 | +0.496 | PNO |
| 0.0010 | 1.0235 | **-60.000** .. +0.20 | +0.470 | ORG, PNO |
| 0.0026 | 1.0000 | -51.70 .. **0.000** | +0.404 | FX, PNO |
| 0.0037 | 0.6980 | -48.64 .. -3.12 | +0.356 | FX, ORG, PNO |
| 0.0177 | 0.0025 | -35.04 .. -52.04 | -0.133 | ORG, PNO |
| 0.1260 | 0.0033 | -17.99 .. -49.63 | -0.247 | FX, ORG, PNO |
| 0.2346 | 44.6692 | -12.59 .. **+33.000** | +0.356 | FX, PNO |
| 0.4607 | 0.1382 | -6.73 .. -17.19 | -0.082 | ORG, PNO |
| 0.8193 | 33.5019 | -1.73 .. +30.50 | +0.252 | ORG, PNO |
| 0.9317 | 1.0000 | -0.61 .. 0.00 | +0.005 | ORG, PNO |
| 0.9652 | 1.0000 | -0.31 .. 0.00 | +0.002 | ORG, PNO |
| 1.0000 | 1.2589 | 0.00 .. **+2.000** | +0.016 | ORG, PNO |
| 1.1704 | 16.7509 | +1.37 .. +24.48 | +0.181 | ORG, PNO |
| 1.2899 | 0.1935 | +2.21 .. -14.27 | -0.129 | ORG, PNO |
| 1.9990 | 0.9765 | +6.02 .. -0.21 | -0.049 | ORG, PNO |
| 2.0000 | 0.6069 | +6.02 .. -4.34 | -0.081 | ORG, PNO |
| 2.0160 | 0.0527 | +6.09 .. -25.56 | -0.247 | FX, PNO |
| 2.0160 | 4.0320 | +6.09 .. +12.11 | +0.047 | FX, PNO |

Several endpoints are exactly round in dB (`-60.000`, `0.000`, `+2.000`, `+33.000`), so these are designed dB tapers, not fitted curves. Two tables share the slope 0.356 dB/step (`0.0037..0.698` and `0.2346..44.67`) and two share -0.247 dB/step (`0.1260..0.0033` and `2.016..0.0527`): they are the same taper at different offsets.

**What is open:** which parameter each table serves. The tables are indexed by the 7-bit parameter values, so candidates are level, drive, gain and mix knobs; the bank 0 ARM-side tables already decoded (cutoff, rotary, EQ) are separate curves. Matching a table to a parameter needs the VISA code that reads it (a `DM(table + index)` load) or a test against measured hardware gain. When a table is matched, its dB range can be tagged VERIFIED in `crates/nord-core/src/fx.rs`.

### 5.17 Image format: an ADI-style loader block stream with two core sections

Tool: `tools/ns4_dsp_ldr.py` (tests in `tools/tests/test_ns4_dsp_ldr.py`). Applies to the FX, PNO and ORG images; SY1SY2 and SMP have no such header (they start with data or zeros).

```bash
python tools/ns4_dsp_images.py target/fw_unpacked/bank0_CODE.bin --out DIR
python tools/ns4_dsp_ldr.py DIR/dsp_image_0_FX.bin              # summary
python tools/ns4_dsp_ldr.py DIR/dsp_image_0_FX.bin --blocks     # every block
python tools/ns4_dsp_ldr.py DIR/dsp_image_0_FX.bin --out SEG    # data segments (do not commit)
```

**Format (VERIFIED on the v1.66 FX, PNO and ORG images: every byte is consumed, 624 / 471 / 430 blocks, 0 header checksum failures).** A sequence of blocks, each a 16-byte little-endian header `tag, address, count, aux` plus a payload for data blocks:

| Field | Meaning |
|---|---|
| tag byte 3 | XOR of the other 15 header bytes (checksum of the header) |
| tag low 16 bits | `0x5003` core section header: `address` = entry value, `aux` = bytes in the section; `0x0003` data block, `count` payload bytes loaded at `address`; `0x0103` zero-fill, `count` bytes of zeros at `address`, no payload; `0x8003` final block of a section, `address` = entry value, `count` 0 |

Each image holds two core sections (Core1 first, Core2 second); the section header sizes (`aux`) add up to the image size minus the 16-byte headers. The kind names are my labels from the observed behaviour (ASSUMED); the byte layout and checksum are verified.

Load maps (v1.66; addresses are the loader addresses, their mapping to the SHARC address space is not decoded):

| Image | Core | Entry value | `0x20xxxxxx` (external) | `0x28xxxxxx` (internal) |
|---|---|---|---|---|
| FX | 1 | `0x1211E3` | 844 B data at `0x20004000`; 409,600 B zero-fill at `0x20010000` | 156 data blocks (79,464 B), 155 zero blocks (175,576 B), `0x28240000..0x2838FA0C` |
| FX | 2 | `0x1211E3` | 409,600 B zero-fill at `0x20080000` | 154 data (74,588 B), 153 zero (174,940 B), `0x28A40000..0x28B8FA0C` |
| PNO | 1 | `0x121007` | 332 B data; 655,360 B zero-fill in two blocks, `0x20004000..0x200A8000` | 127 data (137,932 B), 125 zero (132,760 B), `0x28240000..0x2838E778` |
| PNO | 2 | `0x125C88` | 240,000 B zero-fill at `0x200C0000` | 106 data (143,004 B), 106 zero (137,916 B), `0x28A40000..0x28B85420` |
| ORG | 1 | `0x125C88` | 428 B data; 240,000 B zero-fill, `0x20004000..0x200BA980` | 106 data (146,360 B), 106 zero (137,916 B), `0x28240000..0x28385420` |
| ORG | 2 | `0x125C88` | 240,000 B zero-fill at `0x200C0000` | 106 data (142,988 B), 106 zero (137,916 B), `0x28A40000..0x28B85420` |

Reading:

- **Core addressing.** Core1 sections load at `0x2824xxxx` (internal) and `0x2000xxxx..` (external), Core2 sections at `0x28A4xxxx` and a second external window (`0x2008xxxx`, `0x200Cxxxx`): the two cores use address bases `0x800000` apart. This explains the fixed distance between duplicated tables (section 5.14).
- **Shared organ program.** The PNO image's Core2 section has the ORG entry value `0x125C88` and almost the same block sizes as the ORG cores (143,004 B against 142,988 B of data, same 137,916 B of zero-fill). So the second DSP of the piano image runs the organ program, and the three near-identical blocks D, E and F of section 5.10 are exactly these three organ sections (PNO Core2, ORG Core1, ORG Core2). The piano program itself is PNO Core1 (entry `0x121007`).
- **External memory.** The zero-fill blocks at `0x2001xxxx`/`0x2008xxxx`/`0x200Cxxxx` are large RAM areas: FX 409,600 B per core (102,400 32-bit words, 2.13 s at 48 kHz, plausible delay and reverb memory, ASSUMED), ORG 240,000 B per core, PNO Core1 655,360 B. The 332 to 844 byte data block at `0x20004000` is a record table (20-byte records `0x2000xxxx, 0x283xxxxx, 0x0004422x, n, 4`) that looks like buffer descriptors; not decoded.
- **Entry values** (`0x1211E3` FX, `0x121007` PNO Core1, `0x125C88` ORG) look like a start address with a low flag bits; their meaning needs the address-space mapping.

Consequence: the load map needed by an emulator is complete in structure (what is loaded where, how much is zeroed). What is missing is the translation from these loader addresses to the SHARC address space (the code uses addresses such as `I5 = 0xC03D8` in the `0x80000..0xFFFFF` internal RAM range) and the core entry semantics.

### 5.18 Loader address to SHARC address mapping

Question left open by 5.17: how do the loader addresses (`0x2824xxxx`, `0x28A4xxxx`, `0x2000xxxx`) relate to the SHARC memory map? Method: map every internal block (top byte `0x28`) with `short-word address = (address & 0x7FFFFF) / 2` and compare with Table 5 of the ADSP-21489 datasheet (`.reference/sharc/adsp-21483_21486_21487_21488_21489.pdf`, internal memory, short-word column).

Result (**VERIFIED**): all internal blocks of all 24 core sections (FX, PNO and ORG, two cores each; 0 exceptions) fall inside the four SRAM blocks of the short-word space:

| SRAM block | Short-word range (datasheet) | Loader blocks fall in |
|---|---|---|
| Block 0 | `0x124000..0x13BFFF` (plus the reserved lead-in `0x120000..0x123FFF`, where the interrupt vectors sit) | `0x120000..0x13FFFF` |
| Block 1 | `0x164000..0x17BFFF` | `0x160000..0x17FFFF` |
| Block 2 | `0x180000..0x18FFFF` | yes |
| Block 3 | `0x1C0000..0x1CFFFF` | yes |

So the loader streams describe the whole internal SRAM of one chip as a byte image in short-word units: byte `i` of the image is short word `i / 2`, and normal-word address `w` is the same memory as short-word address `2w` (for example block 2: normal `0xC0000` is short `0x180000`, consistent with the `I5 = 0xC03D8` style loads seen in the code). **Bit 23 of the loader address is the chip**: 0 for Core1 sections, 1 for Core2 sections (`0x28A4xxxx` is `0x2824xxxx` on the second chip). So each image drives two DSP chips with their own SRAM contents, not two cores of one chip.

**Entry values.** The section entry (`0x1211E3` FX, `0x121007` PNO Core1, `0x125C88` the organ program) is a short-word address inside block 0. Disassembling at that address in the flat SRAM image shows start-up code: both FX Core1 and ORG Core1 begin with `BIT CLR MODE1 0x1000`.

Tool support: `tools/ns4_dsp_ldr.py --sram DIR` writes `core1_sram.bin` and `core2_sram.bin` (4 MiB flat images indexed by `address & 0x7FFFFF`); the disassembler reads them with byte offset `2 * short_word_address`:

```bash
python tools/ns4_dsp_ldr.py DIR/dsp_image_0_FX.bin --sram SRAM
python tools/sharc_visa.py SRAM/core1_sram.bin 0x2423c6 0x2423f0   # entry 0x1211E3 * 2 = 0x2423C6
```

The external window (`0x20xxxxxx`) is not mapped yet. Candidates are the SDRAM address space of the datasheet (Table 6/7, bank 0 starts at `0x00200000`); the zero-fill blocks of 409,600 B (FX), 240,000 B (ORG) and 655,360 B (PNO Core1) are delay or work RAM there.

Not done: a run-by-run check of the new `0x440000+` runs for SHARC opcodes, and v1.60 / v1.62 (not available).

## Implications

- The 1:1 verdict changes from "DSP code is not in the update" to "SHARC VISA code is in the update (`bank0_CODE@0x440000..0x4FFFFF`), uncompressed and 16-bit aligned". Track B no longer needs a hardware flash dump to start; it needs a SHARC/VISA disassembler (none exists openly) or a decoder written from the programming reference, plus a SHARC core model to run it.
- `docs/research/ns4-firmware-teardown.md` rows for `0x003CF06C..` describe packed-container offsets (bank 1), not DSP images. The bank 1 `DSP*` table is a region/test table. `docs/research/sharc-dsp-and-preset-hash-teardown.md` and `docs/TECH_DEBT.md` TD-06 ("`.ldr` outside the update image") need the same correction once the mapping to `DSPO` / `DSPS` / `DSPF` is known.
- The high-level engine stays the production path ([D2](../DECISIONS.md)). The VISA code is a new source of VERIFIED constants (coefficient addresses, DAG strides) once it can be read.

## Follow-ups

- Finish `tools/sharc_visa.py`: multifunction and shifter compute fields (Tables 12-11, 12-12 onward), types 1a/7/9/10a/16/19a, and validate the 5a/5b and 15b field pairing.
- Map code blocks to `DSPO` / `DSPS` / `DSPF` (which cores, which L1 blocks: DAG addresses `0xC03D8..` are block-1 RAM). The ARM sender is not `DSP Comm` (section 5.7); trace the bank 1 flash-tool routines for a copy from `0xC0440000..`.
- Check whether ADI's own disassembler (CCES `elfdump`/`dis21k`) can read a raw VISA blob for cross-checking (not downloadable without ADI tools).
- Trace `bank0_CODE` code that touches `0x62004000` / SPI0 `0x01C41000` to confirm whether the ARM ever streams data to the SHARC at runtime (DSP comm `0x6CA48` is the known sender of table words).
- If hardware access exists: dump the SPI flash and look for the ADI loader stream there; ADI's loader documentation (not in `.reference/sharc/`) is needed for the block layout.
- Check the `.dmg` updater for any additional payload (not done here).
- Decode the `0x440000+` item streams (section 5.1/5.3): VISA parcels or a command script? The v1.62 to v1.64 one-byte fix at `0x45D63C` is the best entry point.
- Remove or relabel the 16-bit PCM-like block in `OPEN-QUESTIONS.md` once identified.
