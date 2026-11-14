# ARM to DSP transfers the harness stubbed or dropped, and the boot message log

- **Checked:** 2026-11-13
- **Sources (local, never committed):** `target/fw_unpacked/bank0_CODE.bin` / `bank0_SRAM.bin` (OS v1.66), `tools/arm_bulk_tables.py` (new, `tools/tests/test_arm_bulk_tables.py`), the booted harness (`arm_boot_full.py`, `arm_steady.py`, `arm_edit_capture.py`), the oracles `sharc-pass` and `sharc-org` (`crates/nord-sharc`), the DSP images cut by `tools/ns4_dsp_images.py` / `ns4_dsp_ldr.py --sram`. Prior notes: [arm-dsp-protocol](arm-dsp-protocol.md), [arm-fw-emulation](arm-fw-emulation.md), [arm-organ-messages](arm-organ-messages.md), [organ-farfisa-bass](organ-farfisa-bass.md), [keydown-paths](keydown-paths.md). Vibrato note read from the open PR branch `research/b3-vibrato-tail` (`b3-vibrato-tail.md`), not modified. Derived data (counts, addresses, SHA-256 prefixes, no firmware bytes): [data/arm-bulk-tables.json](data/arm-bulk-tables.json).
- **Question:** which ARM to DSP transfers does the harness stub or drop, which data source does each read, can the table words that the real boot sends reach the oracle, and what do the vibrato type words become once the missing state exists?

Tags follow `.agents/rules/provenance-in-code.md`: **VERIFIED** = reproduced by the command shown, **EMPIRICAL** = measured, reading not proven, **ASSUMED** = structural reading.

## 1. Short answer

| Question | Result | Tag |
|---|---|---|
| Is "SPI-downloaded tables are not loaded" the blocker? | **No, for everything traced.** The three stubbed SPI routines carry no table data to a DSP: the image download (`0xC0241C58`) sends the five DSP images from `bank0_CODE@0xC044667C..0xC05263A8`, which the oracle already loads from the same bytes; `0xC0066E64` is the DSP **read-back** channel (DSP to ARM, address words `0x92519`, `0x940A1`, `0x9409F` of the SMP comm); `0xC0240548` sends a descriptor in `bank0_CODE@0xC0444E68`. | VERIFIED (run, section 2), role of `0xC0240548` ASSUMED |
| What did the harness actually drop? | **The whole boot.** The real boot writes to the real EMIFA ports (`0x62004014/18/1C` path 2, `0x6200000C/10` path 1), the fake ports are installed only after the boot, and `settle` / `clear_messages` discard the rest. The boot sends **80,815 messages** (242,131 raw port stores) that no capture had: 28,375 to PNO core 1, 24,417 to ORG core 1, 12,597 each to ORG core 2 and PNO core 2, 1,655 / 860 to FX, 290 to SY and 24 to SMP. | VERIFIED (run) |
| Where do the table words come from? | Tables sent by the bulk senders live in `bank0_CODE` (rodata): `0xC001C6A0` (2 calls: 32 words from `0xC027C10C`, 60 words from `0xC027C1BC`) and a block copier `0xC01C0B3C..` that reads int16 pairs through `[object + 0x44]` = `0xC043FB54` (also `bank0_CODE`) and writes **5,910 words to the DSP L2 window `0x200C0000..`** (PNO once, ORG twice). None reads NAND, `.ns4b` or the SPI flash. | VERIFIED (run) |
| Does the oracle accept the boot state? | PNO core 1: 8,306 distinct DM words replayed with `sharc-pass`, **8,300 read back identical** after 1 and after 40 passes; the 6 others are the DSP mailbox words `0xB00FE..0xB0103` that the pass itself rewrites. ORG core 1 and core 2: 12,102 words each with `sharc-org --msg-base 0 --msg-script`, **12,102 of 12,102 identical**, strict mode "skipped ops 0". FX: the 2,560 words are accepted by `sharc-fx --poke @FILE` but that tool dumps only after a routine call, so equality is **not** checked. The L2 stores are not applied (the runners take DM word addresses). | VERIFIED (PNO, ORG), open (FX, L2) |
| Vibrato type words with the state they need | `0xB44AE` = `0x3F7FFFFE` (the int `0x7FFFFF` as a float) when the layer's vibrato enable byte is set, else 0; `0xB44B0` = 0 **for every type** (B3 0..5, Vox 0..2, Farfisa 0..3) and every slot. The type is stored in the layer object and never sent as a value. No table; the tail does not depend on the type, but every type message sets `0xB44B0` to 0, which that note measured as -0.9 to -1.8 dB on the lines of 36..88 (section 4). | VERIFIED (run, section 4) |

## 2. Census of the stubbed and dropped transfers

```bash
uv run --with unicorn python tools/arm_bulk_tables.py capture \
    target/fw_unpacked/bank0_CODE.bin target/fw_unpacked/bank0_SRAM.bin --out target/bulk
```

About 80 s. It wraps the stubs of `arm_boot` / `arm_boot_full`, hooks `0xC001C6A0` and `0xC0094EA0`, keeps every store to the real ports and to the fake ports in one raw log (phases `init`, `boot`, `settle N`), groups it once (a message split by a phase boundary is no longer lost) and writes `boot_log.jsonl`, `<chip>_core<N>.script`, `<chip>_l2.script` and `summary.json` under `--out`. The committed copy of `summary.json` is [data/arm-bulk-tables.json](data/arm-bulk-tables.json).

| Transfer (stub) | Calls in the boot | Caller (`lr`) | What it is | Data source | Consequence |
|---|---|---|---|---|---|
| DSP image download `0xC0241C58` (vtable `0xC029EF1C`, method `0xC0064E7C`) | 1 (five images) | `0xC024BB2C` | SPI slave boot of FX, PNO, ORG, SY1SY2, SMP | **`bank0_CODE@0xC044667C..0xC05263A8`** ([arm-dsp-protocol](arm-dsp-protocol.md) s2) | none: the oracle images are cut from the same bytes |
| SPI block download `0xC0240548` | 2 + 2 | `0xC024888C`, `0xC02495D8` | retry loop until the slave answers 1; object `[0x9017DE50] = 4`, descriptor pointer `0xC0444E68` | descriptor in `bank0_CODE`; payload not resolved | ASSUMED not a table source (two calls on each side of the engine constructor, no message depends on it in the run) |
| SPI master transfer `0xC0066E64` | 30,021 (30,003 from the wait loop `0xC004BC8C`) | `0xC004B7DC`, `0xC004BA50`, `0xC004BC8C`, `0xC0062E00`, `0xC0248A50` | **read-back from the SMP DSP**: comm 4 object `0x9000FD30` (port `0x62000010`, pins 9 / 8 / 15), arg 2 is a DSP DM address (`0x92519` polled until it reads 1, `0x940A1`, `0x9409F`), answer written to the stack pointer | the DSP's own state | data flows DSP to ARM; the stub answers 0, so the wait at `0x92519` times out (30,000 tries) instead of finishing. Not a table |
| File read `0xC0094EA0` (FAT sector reads) | 140 (139 from `0xC0095D5C`) | storage objects | the factory-library scan (`.ns4b` content: piano, sample and program files) | **user / flash content**, not in `bank0_CODE` | NOT available to the harness; [piano-sound-load](piano-sound-load.md) serves single files from a local `.ns4b` |
| Bulk table sender `0xC001C6A0(dm, src, n)` | 2 | `0xC00228F8`, `0xC0022CF0` (engine constructor `0xC002187C`) | int words to float, then PNO core 1 DM `0x95471..` (32) and `0x9549D..` (60) | `bank0_CODE@0xC027C10C`, `@0xC027C1BC` (SHA-256 prefixes `24fbc5991d4edf26`, `efadd984cd270256`) | in the boot log |
| Block copier `0xC01C0B3C..` | 5,910 words per target | `0xC01C0B70` (stores) | int16 pairs from a segment list (`0xC0440924`) over `0xC043FB54` to L2 `0x200C0000..` | `bank0_CODE` | in the boot log, **L2 delivery open** |
| Device polls rotated by `enable_poll_breaker` | 557,652 reads | PSC, GPIO, SPI flags | hardware status words | none | harness limit, no data |
| Null vtable calls (`guard_null_calls`) | 782 | UI and edit objects | objects the partial boot did not build | none | harness limit |

NAND: no `bank0_CODE` path reads NAND in this boot; the only flash-backed reads are the 140 FAT sector reads above (user content). The product's data is in `.ns4b` / `.ns4p` files, which are inputs, not a firmware blob.

### Boot log by phase (VERIFIED, `summary.json`)

| Target | boot messages | distinct DM words | L2 stores | settle windows (earlier captures) |
|---|---|---|---|---|
| PNO core 1 | 28,375 | 11,852 | 5,910 | 7,295 |
| PNO core 2 | 12,597 | 11,280 | | 912 |
| ORG (both cores counted as core 1 when the address is below `0x28800000`) | 24,417 + 12,597 | 23,100 / 11,280 | 11,820 (two copies of 5,910) | 912 + 912 |
| FX core 1 / core 2 | 1,655 / 860 | 975 / 810 | | 1,844 / 1,905 |
| SY (path 1) | 290 | 63 | | |
| SMP (path 1) | 24 | 12 | | 1,516 |

The final state (last value per DSP word) has 8,306 words for PNO core 1, 12,102 for PNO core 2 and for each ORG core, 2,560 / 2,555 for FX core 1 / 2; fingerprints in `data/arm-bulk-tables.json` (`state`). The L2 stores are fingerprinted as `l2` (ORG `4daf976efb9cdd4a`, PNO `6764c691be215713`); ORG gets the table twice, PNO once (the target core of each copy is not carried by the port address: ASSUMED ORG core 1 then core 2).

## 3. Replay (VERIFIED)

```bash
python tools/ns4_dsp_images.py target/fw_unpacked/bank0_CODE.bin --out DSP
python tools/ns4_dsp_ldr.py DSP/dsp_image_1_PNO.bin --sram SRAM_PNO     # and 2_ORG
sharc-pass SRAM_PNO/core1_sram.bin --script target/bulk/pno_core1.script --passes 1 --quiet --dump LO:N
sharc-org  SRAM_ORG/core2_sram.bin --dev 0x282c0380=1 --dev 0x3108d008=5 --passes 2 \
    --msg-base 0 --msg-script target/bulk/org_core2.script --dump LO:N
```

Compared against `final_state` of the boot log: PNO core 1 8,300 of 8,306 (99.93 %), ORG core 1 and core 2 12,102 of 12,102. The scripts replay the whole log in send order (boot then settle), so the engine constructor, the strip defaults of all targets and the model installs are in the oracle before any experiment; earlier replays added the constructor by hand (`organ_fb_pass.capture_ctor`) and dropped everything else the boot sent. The PNO core 1 boot state has 11,852 distinct words against 4,105 in the settle log alone.

Not done: the L2 block (needs an oracle that maps the L2 byte window or a conversion to the word address the runners take), FX equality, and re-running the organ chord and tail experiments on top of the full boot log.

## 4. Vibrato type words (VERIFIED, from the settled snapshot)

`Edt_OrgCom_kVibratoType*` (`0x168..0x16A`) reach `0xC0120DB0..` (store type in `layer + 0x74`), then `0xC0121050` calls `0xC00C89F4(this, 0, type)`; `0x16C` (enable) is handled in `0xC011CB60`, which stores `(value == 1)` at `0xC059FF5C + [layer + 0x876C] + 0x458` (`strb` at `0xC011CBB8`).

| Item | Result |
|---|---|
| Why the harness saw 0 | The enable byte is written with 0 during the boot (default program, VERIFIED write watch: `0xC011CBB8` stores 0 for both layers) and the harness' dispatch of `0x16C` goes to a different client handler (`0xC011FFB8`) that does not write it. The words are not formed from a table |
| `0xB44AE` | `0x7FFFFF` through `0xC001FF18` = **`0x3F7FFFFE`** when the byte is 1; 0 when 0 (disassembly `0xC01208D8..0xC01208E4`, run) |
| `0xB44B0` | **0 for every type and every slot**: `0xC00C89F4` with r1 = 0 sends the constant `mov r1,#0; str r1,[port]` (`0xC00C8B14..0xC00C8B18`) whatever the type; the type is not an argument of any store |
| Slot | `[chain object + 0x24]` (0 after the constructor `0xC0021EEC`) is the slot index: the word pair moves by `0x40` per slot (`0xB44AE/B0`, `0xB44EE/F0`, `0xB452E/30`, `0xB456E/70`); the second layer of the harness uses slot 2 |
| Vox, Farfisa types | one word, `0xB44AE`, same gate |

The measured B3 types 0..5, Vox 0..2 and Farfisa 0..3 with enable 0 and 1 and slots 0..3 (209 rows) give only these two words. `b3-vibrato-tail.md` (PR #158) measured `0xB44AE = 1.0` as bit-identical and `DM 0xB44B0` from the constructor's 1.0 to 0 as -0.9 to -1.8 dB on the lines of 36..88 (+0.9 at 440 Hz). The ARM value is exactly that 0, for every type: after any vibrato type message the real chain is the "`0xB44B0 = 0`" column of that note, not the 1.0 baseline of `b3-tail-gain.csv`. The correction is the same for all types (state independent), small, and not applied to the Rust table here. The type-5 hypothesis of `b3-vox-video-gap.md` (PR #156) is not supported by any ARM word: what the type does must be on the DSP side or in words sent by a path the capture does not reach (the layer object stores the type; the periodic updater `0xC022F060` and the key-down paths were not run with it set).

## 5. Piano, FX, synth type words

Not re-run. The boot log now holds the FX type routines' initial install (1,655 / 860 boot messages, 2,560 / 2,555 final words), the SY boot words (63) and the SMP boot words (12) that the earlier captures started after. Type words that depend on **user content** (sample set, program) still need the `.ns4b` file reads (140 FAT reads, above): that is the real remaining data dependency, not an SPI table.

## Implications

- Remove "SPI tables not loaded" as the stated cause: the dropped data is the boot's own message stream, and the downloaded images are already the oracle's images.
- Replay `<chip>_core<N>.script` of the boot log before any ARM capture experiment; PNO core 1 and ORG reproduce the boot state word for word.
- The vibrato words are a gate and a constant; the organ chain does not read a vibrato type table from the ARM.
- The L2 table (5,910 words, PNO and ORG) is new ARM-supplied data that no oracle run has had.

## Follow-ups

1. Apply the L2 stores (`pno_l2.script`, `org_l2.script`) in the oracle (map the L2 window or convert to the runner's address form), then re-measure the B3 chord and tail on top of the full boot log.
2. Verify the FX replay (`sharc-fx` call that dumps) and add PNO core 2 / SY / SMP to the equality check.
3. Resolve the payload of `0xC0240548` and find which edit id or program event sets the vibrato enable byte for a loaded preset (needs the `.ns4p` apply path).
4. Serve the 140 FAT reads from a local `.ns4b` (as `arm_piano_load.LibraryStub` does for one file) and capture the piano, synth and FX type words of a real program.
