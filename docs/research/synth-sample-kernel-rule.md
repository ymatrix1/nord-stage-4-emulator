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
| The rule | A compare chain at `0xC01E1434..0xC01E1470` on `key = [[0xC0563664 + 4 + 4 id] + 8]` (`id` = voice id `[voice + 0]`, `<= 0x37`): `key > 95` kernel 5, `> 83` kernel 4, `> 71` kernel 3, `> 59` kernel 2, `> 47` kernel 1, else kernel 0 (first key of each run 0, 48, 60, 72, 84, 96). It is **a function of the key, not of the playback ratio**; the root key of the sample does not enter. | VERIFIED (the firmware's own instructions run for keys 0..127, `tools/arm_sy_kernel_rule.py`), field meaning ASSUMED (section 3) |
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

`examples/program_sweep.rs` now lists per synth layer the kernels the key rule reaches over the key spans of the loaded zones (`synth_a_kernels` ..), and `tools/program_sweep.py` tags each as `synth.sample.kernel=K` (status assumed: the rule is VERIFIED, the key field is ASSUMED and the default still uses the ratio rule). Numbers of the 2026-11-14 sweep are in [program-coverage](program-coverage.md) section 5.

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

Consequence: the structural evidence says the chain key is the layer note without the `+0x18` transposition, which is what `Voice::note` holds, but the event-record to object link was not run. D22 therefore keeps `--sample-kernel key` opt-in (D35).

## 9. Loop handling, record fields, WAVE bank

| Item | Result | Tag |
|---|---|---|
| Loop and second read position | no ARM-side loop code in the sender (unchanged); the voice update writes `+0x0A`, `+0x0C .. +0x0F`, `+0x13` from stack fields it copies out of the event record (`0xC01E0334..0xC01E0378`, `0xC01E09B8..0xC01E0A48`): roles of the stack slots not decoded | open |
| WAVE (Wavetable) layers | in the harness the oscillator edits (`kMode` 0..3 x `kOscBank` 0..3, `kOscWave` applied) send only PNO core 1 / core 2 and ORG messages (24 or 32 words each, identical for every bank), **no SY or SMP word**; banks 1 and 2 only change the strip type index to 49..53 ([synth-wave-stages](synth-wave-stages.md): stages `0x124269`, `0x12453F`). So the 14 WAVE layers are most likely strip stages on PNO core 2 plus a table stored on the host, not SY / SMP voices; the table read path is not decoded | EMPIRICAL (negative run) |

## Implications

- `SincKernelSet::for_key` (`sinc/key_rule.rs`) is the firmware rule; it replaces the ASSUMED ratio rule for the `SAMPLES` oscillator behind `--sample-kernel key` (opt-in, `SynthLayer::sample_kernel_by_key`). Kernels 0 and 1 (`fc` 1.10, 1.05) are used for keys up to B3; the legacy rule never picked them.
- `sinc::pitch_law` is the ARM exponent law (bit-identical, section 7), opt-in `--sample-pitch fw`.
- The default stays on the ratio rule: the key field is structurally the played key (section 8) but no run with a SAMPLES layer exists, and the audible difference (a few tenths of a dB of brilliance between neighbouring kernels, DC gain `1/fc`) has no hardware clip that isolates it.

## Follow-ups

0. (open after this PR) Load a SAMPLES program into the harness (sample header download served from the local `.ns4b`) and capture `0x9243B + 0x38 k` plus the pair-block word; that closes section 8 and the scale of section 7.

1. Boot the harness with a SAMPLES program, play a note and capture the message to `0x9243B + 0x38 k`: confirms the key field, transposition and the voice id to record map.
2. Run the pitch block `0xC01E14AC..0xC01E1598` for keys 0..127 and compare with the six measured steps of [sy-smp-audio](sy-smp-audio.md) section 4.
3. Decode the ARM writer of the other record fields and the SAMPLE / WAVE bank split between SY and SMP.
