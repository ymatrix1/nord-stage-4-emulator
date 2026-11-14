# SY sample voices: the ARM kernel-selection rule (TD-08, OQ22)

- **Checked:** 2026-11-14
- **Sources (local, never committed):** `target/fw_unpacked/bank0_CODE.bin` and `bank0_SRAM.bin` (OS v1.66, ARM mode, base `0xC0000000`); `capstone` 5 and `unicorn` 2 (`uv run --with capstone --with unicorn`); `tools/arm_emu.py` (static initialiser `0xC00002A4`), the new `tools/arm_sy_kernel_rule.py` (tests in `tools/tests/test_arm_sy_kernel_rule.py`); prior notes [sy-dsp-map](sy-dsp-map.md), [sy-smp-audio](sy-smp-audio.md), [oracle-vs-engine](oracle-vs-engine.md), [arm-param-messages](arm-param-messages.md), [smp-voice](smp-voice.md).
- **Question:** which rule picks one of the six SY1SY2 12-tap kernels for a sample voice (ARM literal `0xC0001BAC`), and what else of the SAMPLE / WAVE voice start can be decoded on the ARM side?

Tags follow `.agents/rules/provenance-in-code.md`: **VERIFIED** = reproduced by a command below, **EMPIRICAL** = read from code shape or measured, not proven, **ASSUMED** = reading of the purpose.

## 1. Result

| Item | Result | Tag |
|---|---|---|
| `0xC0001BAC` | **Not a sample-start path and not a selector.** It is one store of the static initialiser `0xC00002A4`: `ldr lr, =0xE3400` followed by `str lr, [r4, #0x344]`, the last of seven 16-byte channel records `{target 6, DM base, size, tag}` at `0xC0544D38..0xC0544D98` (`0xB2000` size `0x40000`, then `0xE4000`, `0xE4C00`, `0xE5800`, `0xE6400`, `0xE7000`, `0xE3400`, each size `0xC000`). Reading the six kernel bases = reading these records. | VERIFIED (run: `r4 = 0xC0544A58`, records dumped after the initialiser) |
| Who reads the records | The pool at `0xC01E0FB8..0xC01E0FCC` of the voice update function `0xC01E00AC` (stack frame `0x7A0`, 3,000 instructions, the same function whose senders write the SY prepare / go states, [sy-smp-audio](sy-smp-audio.md) section 3). | VERIFIED (literal scan, listing) |
| The rule | A compare chain at `0xC01E1434..0xC01E1470` on `key = [[0xC0563664 + 4 + 4 id] + 8]` (`id` = voice id `[voice + 0]`, `<= 0x37`): `key > 95` kernel 5, `> 83` kernel 4, `> 71` kernel 3, `> 59` kernel 2, `> 47` kernel 1, else kernel 0 (first key of each run 0, 48, 60, 72, 84, 96). It is **a function of the key, not of the playback ratio**; the root key of the sample does not enter. | VERIFIED (the firmware's own instructions run for keys 0..127, `tools/arm_sy_kernel_rule.py`), field meaning ASSUMED in this section, VERIFIED in section 12 |
| Kernel to DM base | kernel `k` = `0xE3400 + 0xC00 k` (`fc` 1.100, 1.050, 1.000, 0.960, 0.925, 0.895): the record bases equal the DM layout of [sy-dsp-map](sy-dsp-map.md) section 2. Higher keys use a lower `fc` (more anti-alias), the direction the old ASSUMED rule had, but the old rule used `fc = 1.00` or lower for every ratio and never chose kernels 0 and 1. | VERIFIED |
| Delivery | `sl = [voice table entry + 4] + 0x35` is the DM address `0x92406 + 0x35 + 0x38 k` = record word `+0x3B` (the kernel pointer, [oracle-vs-engine](oracle-vs-engine.md) section 5, VERIFIED to select the kernel alone); the value `r5` is the record's base word, sent through the flow-controlled path-1 sender `0xC006CA48` (`0xC01E1798..0xC01E17D8`). The address conversion `0xC01E1484..0xC01E149C` leaves a path-1 (SY) base unchanged. | VERIFIED (listing), message not captured at run time |
| Oracle side | The six kernels equal the firmware table to one ulp and the Stage B arithmetic reproduces the DSP (`tests/oracle_stage_b.rs`, `tests/oracle_sy_voice.rs`, [oracle-vs-engine](oracle-vs-engine.md)). Together with the ARM run above the whole chain key -> kernel base -> record word `+0x3B` -> taps is covered. | VERIFIED |

## 2. Listing (OS v1.66)

```text
c01e1424  ldr r3, =0xc0563664      ; table of voice objects
c01e1428  add r3, r3, sl, lsl #2   ; sl = voice id (<= 0x37)
c01e142c  ldr r3, [r3, #4]
c01e1430  ldr r3, [r3, #8]         ; key
c01e1434  cmp r3, #0x5f ; ldrgt r2, =0xc0544d88 ; bgt c01e1470    ; kernel 5, DM 0xE7000
c01e1440  cmp r3, #0x53 ; ldrgt r2, =0xc0544d78 ; bgt c01e1470    ; kernel 4, DM 0xE6400
c01e144c  cmp r3, #0x47 ; ldrgt r2, =0xc0544d68 ; bgt c01e1470    ; kernel 3, DM 0xE5800
c01e1458  cmp r3, #0x3b ; ldrgt r2, =0xc0544d58 ; bgt c01e1470    ; kernel 2, DM 0xE4C00
c01e1464  cmp r3, #0x2f ; ldrgt r2, =0xc0544d48 ; ldrle r2, =0xc0544d98   ; kernel 1 / 0
c01e1470  ldr r3, =0xc027aa40 ; ldm r2, {r1, r5}   ; r1 = target (6), r5 = DM base
c01e1478  ldr r2, [r3, r1, lsl #2] ; cmp r2, #0 ; beq c01e14a4   ; path flag 0 (path 1): base kept
...
c01e1798  add sl, r3, #0x35        ; field address 0x92406 + 0x35 (+ 0x38 k)
c01e17a0  bl 0xc006ca48 ; str sl, [port] ; str r5, [port]       ; message (address, kernel base)
```

## 3. What is open about the key field

`[object + 8]` is compared with `0x2F, 0x3B, 0x47, 0x53, 0x5F` = 47, 59, 71, 83, 95: octave-spaced keys (B2, B3, B4, B5, B6) with the kernel changing between B and C. The pitch code of the same function (`0xC01E14AC..0xC01E1598`) subtracts `0x3C` = 60 from `[voice + 8]` (plus `[voice + 0x18]`), which is why the field is read as the MIDI key. Two things are not separated: key against key plus transposition (`[voice + 0x18]`, tune words), and whether the table at `0xC0563664` (built at run time, empty after the static initialiser) holds the same objects as the voice list. The Rust rule uses the played note (`Voice::note`, no transposition): **ASSUMED** until a boot with a played SAMPLES layer shows the record word `+0x3B` write.

## 4. SAMPLE / WAVE layer start: what is and is not decoded

| Part | State | Tag |
|---|---|---|
| Voice states and their senders | prepare `0x1C07CD` (ARM `0xC01E1198`, `0xC01E193C`), go `0x1C084C` (`0xC01E2C2C`, `0xC01E2F54`), reset `0x1C0794` (`0xC01D66C4`, `0xC01D6730`), message `(0x92406 + 0x38 k + 0xE, pointer)`: [sy-smp-audio](sy-smp-audio.md) section 3 | VERIFIED (earlier work) |
| Kernel pointer | this note (section 1) | VERIFIED |
| Pitch | decoded and run in section 7: integer exponent tables (closed forms), the result is `float(int) * 2^-23`; the ARM-side law is VERIFIED, its join with the DSP step table is open | VERIFIED (law), open (join) |
| Loop handling | no ARM-side loop code was found in the sender; the second read position of Stage B (mask words at frame `+0x28`) is the DSP's loop splice or crossfade, role open ([sy-dsp-map](sy-dsp-map.md) section 6). The engine loops with a crossfade (`sample_osc.rs`), unverified | open |
| Voice record fields written before prepare (`+0x0A`, `+0x0C..+0x0F`, `+0x13`, ...) | not decoded | open |
| WAVE bank (14 Wavetable layers in 12 factory programs) | no ARM path decoded; the engine plays a synthetic 64-table placeholder. Whether WAVE layers run on SY (kernel record), on SMP (6-tap Q31, [smp-voice](smp-voice.md)) or on the ORG / PNO strips is not known | open |

## 5. Program sweep coverage (SAMPLE layers)

`examples/program_sweep.rs` now lists per synth layer the kernels the key rule reaches over the key spans of the loaded zones (`synth_a_kernels` ..), and `tools/program_sweep.py` tags each as `synth.sample.kernel=K` (sweep status `assumed`; since section 12 the rule and the key field are VERIFIED and the rule is the default). Numbers of the 2026-11-14 sweep are in [program-coverage](program-coverage.md) section 5.

## 6. Reproduce

```bash
uv run --with unicorn python tools/arm_sy_pitch_law.py target/fw_unpacked/bank0_CODE.bin target/fw_unpacked/bank0_SRAM.bin   # section 7
cargo test -p nord-core sinc::pitch_law
python tools/ns4_dsp_images.py target/fw_unpacked/bank0_CODE.bin --out DIR   # only for the oracle tests
uv run --with unicorn python tools/arm_sy_kernel_rule.py \
    target/fw_unpacked/bank0_CODE.bin target/fw_unpacked/bank0_SRAM.bin
# keys   0.. 47  kernel 0  DM 0xe3400
# keys  48.. 59  kernel 1  DM 0xe4000
# keys  60.. 71  kernel 2  DM 0xe4c00
# keys  72.. 83  kernel 3  DM 0xe5800
# keys  84.. 95  kernel 4  DM 0xe6400
# keys  96..127  kernel 5  DM 0xe7000
cargo test -p nord-core sinc::key_rule
```

## 7. Pitch law (ARM `0xC01E14AC..0xC01E15A4`), run for the first time

The block is integer code (the ARM926 has no FPU) with two soft-float calls (`0xC0270BD4` int to float, `0xC0270C80` multiply, here by `0x34000000` = 2^-23). `tools/arm_sy_pitch_law.py` runs the real instructions on a mocked voice object (`r6`) and layer object (`r0 = [r6 + 0x34]`):

| Step | Result | Tag |
|---|---|---|
| Inputs | `key = [r6 + 8]`, `transpose = [r6 + 0x18]` (added only when `[layer + 0x6C] == 1`, else `[layer + 0x60]` replaces the sum), cents `([r6 + 0x14] + [r6 + 0x1C])` (hundredths of a semitone, `(c << 16) * 0x51EB851F >> 37`, truncated toward zero), fine `[r6 + 0x74]` (Q16 semitones), tune `[layer + 0x10]` (Q8) | VERIFIED (listing and run) |
| Exponent | `x = ((key + transpose - 60) << 16) - (tune << 8) - (cents + fine)`; fraction table `T1[i] = floor(2^23 * 2^(i/3072))` (257 words at `0xC02FA340`, all equal), semitone table: 12 mantissas repeating (`0xC02FA744`, period 12 over 256 entries) with shift `17 - ceil(i/12)` (`0xC02FAB44`, all 256 equal), index `clamp((x >> 16) + 128, 0, 255)` | VERIFIED (table dump against the closed forms) |
| Result | `word = float(int) * 2^-23`; `word(60) = 0.0120558` (`0x3C458580`), doubles per octave. Integer quantisation at low keys: worst deviation from `2^(n/12)` 0.34 cent (key 4); keys 60 and above below 0.011 cent | VERIFIED |
| Port | `crates/nord-core/src/sinc/pitch_law.rs`: **bit-identical** to the firmware instructions for 23 inputs (every field incl. negative cents, fine and tune; test `matches_the_firmware_block_for_23_inputs`) | VERIFIED |
| Engine | `SynthLayer::sample_pitch_fw` / `--sample-pitch fw` (opt-in): `step = sr_zone / sr_engine * word(key - root + 60) / word(60)`. Differs from exact `2^(n/12)` by at most 0.34 cent (2e-4), e.g. -0.05 cent at key 24 | VERIFIED (law), ASSUMED (root key as plain key offset) |
| What is not joined | The six measured steps of [sy-smp-audio](sy-smp-audio.md) section 4 (`e = 0x0800 .. 0x1800` give ratios `0.625 .. 2.5`) are DSP-side packed floats fed by hand; the ARM word is sent as a DM value through the port object (`[[0xC055B150] + 4 * id + 0x40]`) and the DSP routine that turns it into the packed float of `DM(0x9354A + index)` was not traced. So the absolute scale (`word(60) = 0.0120558` against step 1.0) and the place where the root key and the sample rate enter (the cents and fine fields are the candidates) are open. The law itself (the shape of the key-to-ratio curve) does not depend on that scale | open |

## 8. The key field `[voice + 8]`: what is and is not shown

| Observation | Result | Tag |
|---|---|---|
| Same object and field | the kernel chain (`0xC01E1424..0xC01E1430`) and the pitch block read `[+8]` of the same object (`table 0xC0563664 + 4 + 4 * id`, `id = [obj]`), and the pitch block adds the transposition `[+0x18]` **afterwards**, so `[+8]` is the key before the transposition field, not key plus transposition | VERIFIED (listing) |
| Writer | the voice update stores `[obj + 8] = [event + 4]` and `[obj + 0x18] = [event + 0x34]` (`0xC01E0A00`, `0xC01E0A10`) from the 32-byte event record of the ring `0xC05613AC + 4 type` | VERIFIED (listing) |
| Played note reaches the synth event | in the booted harness a MIDI note-on (60, 61, 72, 48) calls the synth voice event `0xC01EC690` with the unshifted note as first record word (`0x3C, 0x3D, 0x48, 0x30`, second word `0x10000`) | VERIFIED (run, `keydown-zones` rig) |
| Run with a SAMPLES layer | **not obtained**. `Edt_Syn_kMode / kOscBank` sweeps (4 x 4, `kOscWave` applied) produce no SY or SMP message and `0xC01E00AC` / the chain `0xC01E1434` are not reached on a note-on: the sample sound objects come from the program / sample-set load, which needs the flash downloads the harness stubs ([keydown-paths](keydown-paths.md) section 4). The kernel record write `0x9243B + 0x38 k` could therefore not be captured | open |

Consequence: the structural evidence says the chain key is the layer note without the `+0x18` transposition, which is what `Voice::note` holds, but the event-record to object link was not run. D22 therefore keeps `--sample-kernel key` opt-in (D35). **Superseded by section 12 (D46):** the store was captured and the kernel default flipped.

## 9. Loop handling, record fields, WAVE bank

| Item | Result | Tag |
|---|---|---|
| Loop and second read position | no ARM-side loop code in the sender (unchanged); the voice update writes `+0x0A`, `+0x0C .. +0x0F`, `+0x13` from stack fields it copies out of the event record (`0xC01E0334..0xC01E0378`, `0xC01E09B8..0xC01E0A48`): roles of the stack slots not decoded | open |
| WAVE (Wavetable) layers | in the harness the oscillator edits (`kMode` 0..3 x `kOscBank` 0..3, `kOscWave` applied) send only PNO core 1 / core 2 and ORG messages (24 or 32 words each, identical for every bank), **no SY or SMP word**; banks 1 and 2 only change the strip type index to 49..53 ([synth-wave-stages](synth-wave-stages.md): stages `0x124269`, `0x12453F`). So the 14 WAVE layers are most likely strip stages on PNO core 2 plus a table stored on the host, not SY / SMP voices; the table read path is not decoded | EMPIRICAL (negative run) |

## 10. SAMPLES layer in the ARM harness: what was tried (2026-11-14, `tools/arm_sample_probe.py`)

Goal: serve a sample set from the local `.ns4b` so that a SAMPLES layer loads and plays, then capture the kernel record write, the pitch message and the record fields. **Not reached with the FAT loader; section 11 finds the real sample loader and binds a loaded sample to the layer.** No default changed (D37).

| Item | Result | Tag |
|---|---|---|
| Sample files in the backup | all 195 `Samp Lib/**/*.nsmp4` are `CBIN` kind `nsmp` with `NSMP` at `0x2C`; chunks `\0hdr` (112 bytes, name at payload `+10`), `\0cat` (8), then `\0map`, `\0stk` ... (`survey`: 195 / 195 headers, 195 / 195 names) | VERIFIED (run) |
| The FAT library loader is not the sample path | serving a real `.nsmp4` as device image through `LibraryStub` + `0xC00C0444(mgr, 1, 1)` with each type word of the compare chain of `0xC00B92E4` (`onpn`, `nsp`, `npdl`, `map`, `abc`) returns status 0 but binds no object: pool object state 3 (`onpn`, `nsp`; `npdl` also 3 after a 3,084-byte read), 4 (`map`), 7 (`abc`); `CNSP` is not found at `0xB0`. The loader has no `nsmp` type word, and the string table lists `nsmp` only as a browser file kind (`0xC0444B60`) next to `Samp Lib (Native)` | VERIFIED (run, `try_load`) |
| Where the sample set lives | the boot stages `SampLib`, `Smp Populate`, `Pno Populate` (`0xC0526530..`) and `Sample Flash Error` name a flash partition read by the SMP populate step, not the FAT volumes; the sample status strings (`Sample is ok`, `Sample type error`, `Sample parse error`, `Unsupported format`, `Sample is broken`, `Out of memory`, table `0xC02FA318`, users `0xC01D4640 / 0xC01D46E0 / 0xC01D680C / 0xC01D80A0`) belong to a sample object class in `0xC01D0000..0xC01D8100` that is fed by a different loader | EMPIRICAL (string table, literal scan; loader not decoded) |
| Why the synth voice event stops | with all three sections enabled (`arm_zones.enable_sections`) and only synth layer A kept, a note-on and note-off enter `0xC01EC690` and `0xC01E72C4` three times in total and run only the blocks `0xC01E72C4..0xC01E7314, 0xC01E7394..0xC01E73C8, 0xC01E7318..0xC01E7328, 0xC01E7374..0xC01E7380, 0xC01E7544..0xC01E760C, 0xC01E7D64..`: `[[layer + 0x4C0]] == 0` (the layer's sample slot list is empty) takes `0xC01E7324 -> 0xC01E7374`, and the allocator counters `0xC05B0C78 + 0x78/0x7C/0x80` are 0. `0xC01E00AC`, `0xC01EC790`, the key store `0xC01E0A00` and the kernel chain `0xC01E1434` are never entered. For each of the five layer objects of `0xC05B028C` (`synth_gate`) the slot list pointer is set and its first slot is 0 | VERIFIED (run) |
| Consequence | a SAMPLES layer needs a sample slot object (class check `[[obj]] == 0xC01D0800`, value `[obj + 4]` non-zero) in `[[layer + 0x4C0]]`; it is produced by the sample loader, which needs the SampLib flash contents, not a FAT image. Forging the object would not prove the key field | EMPIRICAL |

### Pair block and the pitch word (SHARC oracle, run)

The SY image was run with `sy_voice_sine.scn` (control: `poke 0x93580 0x0800` gives step `0xA00000`, ratio 0.625, unchanged) and a pair-block dump `capture pb0 0x9219e 0x30`. A pair is 22 words with the two voices interleaved at stride 2; per voice word 0 = `0x04000036` (index `0x36` in the low half = table entry `DM(0x9354A + 0x36) = DM(0x93580)`), word 1 = gain 1.0, words 2..5 = constants `0x3B98676A` (x2), `0x3D4CCCCC`, `0x3BA00000`. The ARM pitch sender (`0xC01E15B0..0xC01E160C`) writes the float word to `[0xC05482F8 + 16 voice + 4] + 0x20`, i.e. `0x9219E + 0x20 = 0x921BE` for voice 0 (table content after boot: voice 0 `0x9219E`, voice 1 `0x9219F`, voice 2 `0x921B4`, voice 55 `0x923F1`). That address is word 10 of the **next** pair block (`0x921B4`), not the table entry `0x93580` and not a word of voice 0's own pair. So the float32 `word(key)` (`0.0120558` at key 60) and the 16-bit packed table entry of [sy-smp-audio](sy-smp-audio.md) section 4 are different objects and the DSP routine that maps one to the other was **not** found (poking `0x3C458580` at `0x921BE` leaves the step at 0 and disturbs the gain word of the other pair). The six measured steps therefore cannot be compared with the law (address corrected in section 12: the message goes to `0x921A6 = 0x9219E + 8`): the join and the scale of section 7 stay open, as does the place where root key and sample rate enter. | VERIFIED (run, oracle); join open |

Run it: `uv run --with unicorn python tools/arm_sample_probe.py <CODE> <SRAM> --cache target/keydown.snap --ns4b <backup> --member "Samp Lib/Accordion_Harm/Carina Accordion 1_PS 4.1.nsmp4"` (tests: `tools/tests/test_arm_sample_probe.py`).

### What remains (replaces follow-up 0 below)

1. (loader done, section 11) Register the voice groups of the handler instance `0x9062E330` (or run the ring consumer task) so that `0xC01E00AC` reaches the key store `0xC01E0A00`; then the three captures are one note-on: record write `0x9243B + 0x38 k`, pitch message, fields `+0x0A, +0x0C .. +0x0F, +0x13`.
2. Decode the DSP side of the pair-block write `0x9219E + 0x20` (which SY routine reads it and fills `DM(0x9354A + index)`), then compare the six measured steps with `sinc::pitch_law`.
3. WAVE bank: unchanged (section 9, negative run on PNO core 2); the strip stages `0x124269 / 0x12453F` and the table read path are not decoded.

## 11. The sample loader found and driven (2026-11-14, `tools/arm_sample_probe.py`)

| Item | Result | Tag |
|---|---|---|
| Status table | `0xC02FA318` is a table of 10 string pointers (`Sample is ok`, `Sample type error`, `Sample parse error`, `Unsupported format`, `Sample is broken`, `Out of memory`, ...) followed by an exponent table the class math also uses; `0xC01D0BB0` returns the string of `[obj + 0xC]` | VERIFIED (listing) |
| Class and pool | the sample object is `0x9D8` bytes, class table `0xC02FB2C0` (slot 0 `0xC01D0800` returns `[obj + 4]`, the sample id), constructed by `0xC01DAE64`; `[0xC055D9B0]` is the manager: `[0]` its own device object, `[4]` the pool (count `0x190`, vector `[+8 .. +0xC]`) | VERIFIED (listing, run) |
| The loader | `0xC01D710C(manager, {u16 lib, u16 entry}, {u16, u16} skip)`: key `entry or lib << 15`, returns `0xA` when the id is already in the pool, else takes a free pool object, runs `0xC01D6EEC`, looks the entry up in the device directory `[dev + 0x60]` (`0xC00507A4`), reads the header with `0xC0095C0C`, compares the type word with `nsmp` (`0xC01D73D4`), parses the file name (`Name_Category Vx.y` into `+0x8C / +0xAD / +0xCE`), sizes the file with `0xC0096264` / `0xC00961B0` and walks the chunk stream with `0xC01D1910` (tags `NSMP`, `NSW`) | VERIFIED (listing, run) |
| Run | with the real file read and directory lookup stubbed (`serve_sample`: device header `0x74` bytes = `0x68` name + type word `0x6E736D70` + two words, then the backup bytes from `NSMP`; descriptor `+4` = file length; one directory entry with the id at `+4`) `0xC01D710C` returns 0, `[obj + 0xC] == 0` ("Sample is ok"), `[obj + 4] == 1`, and the file is read in 256-byte pieces across the `hdr`, `map` and `stk` chunks. The failed attempts show the failure modes: an empty directory returns 1, a descriptor with `+4 == 0` gives status 5 ("Unsupported format"), a wrong type word gives status 3 ("Sample type error") | VERIFIED (run, Carina Accordion 1) |
| Binding to a layer | `Edt_Syn_kSmpId` (`0x147`) and `Edt_Syn_kMode = 0` (`0xFA`, SAMPLES) through the layer client set `[layer + 0x4BC] = 1` (`0xC00171D4`: `value == 0x39`) and, with `0xC01E5978(slot list, id)` run directly, slot 0 of `[[layer + 0x4C0]]` becomes the pool object. The gate of `synth_gate` is then open and a note-on passes the sample check of `0xC01E72C4` | VERIFIED (run) |
| Note-on with a loaded sample | `0xC01E7C64 .. 0xC01E7D14` runs: the voice event is posted (class `0xC02FB318`, `{vtable, 4, layer, event index}`) to the ring `[[sp + 0x4C] + 0xC]`, and one SY message `0x92414 = 0x1C0794` is sent; the pitch message and the key store `0xC01E0A00` are **not** reached, because the voice update `0xC01E00AC` (slot 1 of the handler class `0xC02FDB40`, instance `0x9062E330`) is run by a ring consumer task the harness does not run | VERIFIED (run) for the post; EMPIRICAL for the consumer |
| Direct voice update | calling `0xC01E00AC(0x9062E330, {0xC02FB318, 4, 0, 60})` spins in the voice group loops `0xC01E0130..0xC01E0228` and `0xC01E0588..0xC01E07CC` (5,000,000 instructions, no store at `0xC01E0A00`): the voice groups of the handler are not registered in the settled state | EMPIRICAL (run) |

Consequences: the loader question of section 10 is closed (it is `0xC01D710C`, not the FAT loader, and no flash is needed once the directory entry exists). The three captures stay open behind the voice group registration of the handler instance and the ring consumer; no default changes (D38). The DSP reader of `0x9219E + 0x20` was not decoded in this run.

## 12. The voice update runs: key store, kernel record, pitch word, record fields (2026-11-14, `tools/arm_sy_voice.py`)

Setup: `arm_sample_probe` loaded `Carina Accordion 1` and bound it to slot 0 of synth layer A (**table index 0** of `0xC05B028C`, zone layer 4 of the edit store), a note-on on channel 1 (velocity 100) runs `0xC01EC690 -> 0xC01E72C4`, which sends the reset `0x92414 = 0x1C0794` and posts two events to the handler queue.

| Item | Result | Tag |
|---|---|---|
| Event delivery | The handler instance `0x9062E330` (class `0xC02FDB40`) is its own queue: `[+4]` count, `[+8]` head, `[+0xC]` tail, items from `+0x10`, `0x200` slots. An event is `{class, type, layer, key}`; `[[event]]` is a thunk that calls one handler slot with `(handler, event)`: class `0xC02FB328` (type 2, thunk `0xC01DB690`) calls slot 0 `0xC01E31C8` (voice start, called **directly** by `0xC01DB6A8` at `0xC01DB870`: it creates the voice object and one group element, `[voice + 0x58] = 1`, `[voice + 0x5C] = element`), class `0xC02FB338` (type 3, thunk `0xC01DBA54`) calls slot 1 `0xC01E00AC` (voice update), class `0xC02FB318` (type 4, thunk `0xC01DB3EC`) calls slot 2 `0xC01E24F0` (in one run it sent `0x9219E = 0xFFFFFFFF`). A note-on leaves `[type 3, type 4]` in the queue; the drain loop `0xC01DEFEC` (callers `0xC01DFF8C`, `0xC01E73D8`) runs the events that match a `(layer, key)` filter (listing), the task that drains the whole queue was not found. `pump` in the tool pops head first and calls the thunk, as the drain loop does | VERIFIED (run), consumer task not found |
| Why the direct call of section 11 spun | Not missing voice groups: slot 0 registers the group at note-on (above). The update reaches the claim loop `0xC01E0588..0xC01E07CC` and waits at `0xC01E0694 / 0xC01E06A4` until the DSP sets bit `[r5 + 8]` of the status word `[0xC05B0454 + 4 [r5 + 4]]`, resending the prepare message each pass (20,404 passes in 3 M instructions). The harness has no DSP: `DspAck` sets that bit (ASSUMED meaning "SY voice ready"); the update then runs 2,067 instructions to the end and sends 17 messages | VERIFIED (run); bit meaning ASSUMED |
| (1) Voice KEY field | `0xC01E0A00` stores `[voice + 8] = [rec + 4]` and `0xC01E0A10` stores `[voice + 0x18] = [rec + 0x34]` (record `rec` of the ring `0xC05613AC`). Captured stores for notes 48, 59, 60, 71, 72, 83, 84, 95, 96: `[voice + 8]` = the played note in every run and `[voice + 0x18] = 0` | VERIFIED (run, 9 keys) |
| Key with transposition | The ring record `[rec + 4]` is the played key **plus** the user's Octave Shift (`Edt_Syn_kOctaveShift`) and global transpose (`Edt_Sys_kTransposeGlobal`): octave +1 gives key 72 for note 60, octave -2 gives 36, transpose +2 gives 62, both 74; `[rec + 8]` holds the total in Q16 semitones (`0xC0000`, `0xFFE80000`, `0x20000`, `0xE0000`). So the voice key is the **effective key**, not the raw MIDI note; `[voice + 0x18]` stays 0 (not a user transposition). The voice stores for those edits were not captured (note-on with the edit applied sent no SY message; cause not analysed), only the record | VERIFIED (run, record), voice store with edit open |
| Kernel record write | `0x9243B` (`0x92406 + 0x35`, voice 0) = `0xE4000` for keys 48, 59 (kernel 1), `0xE4C00` for 60, 71 (kernel 2), `0xE5800` for 72, 83 (kernel 3), `0xE6400` for 84, 95 (kernel 4), `0xE7000` for 96 (kernel 5): exactly `sinc::key_rule::kernel_for_key` on the stored key (59 stays kernel 1, 60 is kernel 2: the key is not shifted) | VERIFIED (run, 9 keys; `0xE3400 + 0xC00 k`) |
| (2) Pitch message | `0x921A6` (SY, path 1) = float bits of the law, sent once per note-on after the kernel record: key 48 `0x3BF46900`, 59 `0x3C66A400`, 60 `0x3C748500`, 71 `0x3CE68980`, 72 `0x3CF430C0`, 83 `0x3D5D2960`, 84 `0x3D8409F0`, 95 `0x3DD772D0`, 96 `0x3E023460`. The inputs at `0xC01E14B8` are the voice fields (`[+8]` key, `[+0x18]` 0, cents `[+0x14] + [+0x1C]` 0, fine `[+0x74]` 0) and the **zone object** `r0 = [voice + 0x34]` with `[r0 + 0x6C] == 0`: the block then uses `[r0 + 0x60]` (a per-zone key: 48, 59, 60, 71, 72, 82, 85, 94, 97 for these notes) instead of key plus transposition, and `[r0 + 0x10]` (tune, Q8 semitones: -944, -943, -946, -941, -940, -1013, -1031, -897, -969). The real instructions (`arm_sy_pitch_law.run_block` on the captured inputs) reproduce all nine words **bit for bit**. So the root key and the sample rate enter through the per-zone `[zone + 0x60]` and `[zone + 0x10]` (about +3.7 semitones for the first zones), not through cents or fine tune; inside one zone the word follows the law (59 to 60: +1.011 semitones, 71 to 72: +0.996, tune differs by a few counts per key) | VERIFIED (run, bit-exact, 9 keys); source of the zone fields (sample map) not decoded |
| Pitch message address | The message goes to `0x921A6` = `0x9219E + 8`, word 8 of voice 0's **own** pair block (the earlier reading `0x9219E + 0x20` in section 10 was a miscount). The DSP step join is still open: replaying the full ARM message list (reset, gains, record fields, prepare, kernel, pitch, `0x9219E = 0xFFFFFFFF`) in `sharc-ref` on the SY image left the step word `0x9242D` at 0 and the table entry `0x93580` empty, so the six measured steps of [sy-smp-audio](sy-smp-audio.md) section 4 still cannot be compared with the law | VERIFIED (run, address); join open |
| (3) Record words before prepare | DM words `0x92400 + 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F, 0x13` (key: `+0x0A, +0x0B, +0x0C, +0x0D, +0x0E, +0x0F, +0x13`): 48: `0xD 0xC00 0xD 0x2320 0xD 0x2380 0x0`; 59: `0xD 0x30E0 0xE 0x480 0xE 0x4C0 0x0`; 60: `0xC 0x2C60 0xD 0xB40 0xD 0xBE0 0x7`; 71: `0xD 0x23A0 0xD 0x30A0 0xD 0x30C0 0x4`; 72: `0xC 0x1CA0 0xC 0x2BA0 0xC 0x2C40 0x12`; 83: `0x2 0x10A0 0x2 0x1CA0 0x2 0x1D20 0x16`; 84: `0x1 0x1140 0x2 0x1000 0x2 0x1080 0x2`; 95: `0x0 0xFE0 0x0 0x37A0 0x0 0x38E0 0x15`; 96: `0x0 0xDE0 0x0 0xEC0 0x0 0xFC0 0x2`. The words come in three (count or shift, offset) pairs `+0x0A/+0x0B`, `+0x0C/+0x0D`, `+0x0E/+0x0F`; the first elements are equal or differ by one and fall from 13 at key 48 to 0 at key 96; the offsets are multiples of `0x20`; `+0x0F - +0x0D` is `0x20 .. 0x140`. They change with the zone of the key, so they come from the sample, not from a fixed rule; the stack slots they are copied from are not decoded. The loop role (second read position, mask words `+0x28` of Stage B) is **not** established: no field changes with a loop flag in these runs | EMPIRICAL (values); roles open |

Consequences. The key field is no longer ASSUMED: the kernel follows the stored voice key, which is the played note plus the user's transposition, and the record write matches the rule for all nine keys. D46 flips `--sample-kernel` to `key`. The pitch law is bit-exact on the real flow, but the engine's `--sample-pitch fw` still models the root as `key - root + 60` while the ARM uses a per-zone key and tune, and the DSP join is open, so D22 keeps `--sample-pitch fw` opt-in (D47).

Reproduce:

```bash
uv run --with unicorn python tools/arm_sy_voice.py target/fw_unpacked/bank0_CODE.bin \
    target/fw_unpacked/bank0_SRAM.bin --cache target/keydown.snap \
    --ns4b ".reference/Nord Stage 4 Factory Restore RevK v1.66.ns4b" \
    --member "Samp Lib/Accordion_Harm/Carina Accordion 1_PS 4.1.nsmp4" --notes 48 59 60 71 72 83 84 95 96
```

## Implications

- `SincKernelSet::for_key` (`sinc/key_rule.rs`) is the firmware rule. Since section 12 it is the **default** for the `SAMPLES` oscillator (`--sample-kernel key`, D46): the record write `0x9243B` was captured for nine keys and equals the rule on the stored voice key. `--sample-kernel ratio` and `--legacy-all` select the old ASSUMED ratio rule. Kernels 0 and 1 (`fc` 1.10, 1.05) are used for keys up to B3; the legacy rule never picked them.
- `sinc::pitch_law` is the ARM exponent law (bit-identical, section 7, and bit-exact on nine words of the real note-on, section 12), opt-in `--sample-pitch fw`: the ARM feeds it a per-zone key `[zone + 0x60]` and tune `[zone + 0x10]` that the engine does not model, and the DSP join of the word to the step is open (D47).
- The sample kernel flip changes only the `SAMPLES` oscillator: `render_hash` changes in the synthetic sample section (96,002 differing floats of 560,000, largest difference 1.6e-4; before `b1c91b53...`, after `31577ee1...`).

## Follow-ups

1. (done, section 12) Boot the harness with a SAMPLES layer, play a note and capture the key store and the message to `0x9243B + 0x38 k`.
2. DSP side of the pitch word: which SY routine reads `0x921A6` (word 8 of the pair block `0x9219E`) and fills `DM(0x9354A + index)`; a `sharc-ref` replay of the full ARM message list leaves the step at 0, so a message or a table fill is still missing (the ARM writes `0x9219E = 0xFFFFFFFF` in the type 4 event). Then compare the six measured steps with `sinc::pitch_law`.
3. Source of the per-zone fields `[zone + 0x60]` and `[zone + 0x10]` in the sample map (root key, sample rate, tuning); then rebuild `sample_pitch_fw` on them and flip it.
4. The stack words `0x92400 + 0x0A..0x13` and the loop role (second read position); the Octave Shift / transpose scenarios of the voice update (record shows the effective key, voice store not captured); the SAMPLE / WAVE bank split between SY and SMP.
5. The task that drains the whole handler queue (the harness delivers the events itself), and what the DSP status bit `[0xC05B0454 + ..]` means.
