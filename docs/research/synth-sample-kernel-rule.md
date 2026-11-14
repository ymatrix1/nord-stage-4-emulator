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
| Pitch | the same function computes a float from `((key + [voice + 0x18]) - 60) << 16`, the tune words and two 256-entry tables (`0xC02FA340`, `0xC02FA744`, `0xC02FAB44`), scales it by `0x34000000` (2^-23) and writes it to the pair block of the voice (`0xC05482F8`, `0x921BE + 0x16 k + (id & 1)`). The DSP side turns the pair-block word into the step ([sy-smp-audio](sy-smp-audio.md) section 4, six measured points). The two halves are not joined by a run: ratio law ARM side | EMPIRICAL (code shape) |
| Loop handling | no ARM-side loop code was found in the sender; the second read position of Stage B (mask words at frame `+0x28`) is the DSP's loop splice or crossfade, role open ([sy-dsp-map](sy-dsp-map.md) section 6). The engine loops with a crossfade (`sample_osc.rs`), unverified | open |
| Voice record fields written before prepare (`+0x0A`, `+0x0C..+0x0F`, `+0x13`, ...) | not decoded | open |
| WAVE bank (14 Wavetable layers in 12 factory programs) | no ARM path decoded; the engine plays a synthetic 64-table placeholder. Whether WAVE layers run on SY (kernel record), on SMP (6-tap Q31, [smp-voice](smp-voice.md)) or on the ORG / PNO strips is not known | open |

## 5. Program sweep coverage (SAMPLE layers)

`examples/program_sweep.rs` now lists per synth layer the kernels the key rule reaches over the key spans of the loaded zones (`synth_a_kernels` ..), and `tools/program_sweep.py` tags each as `synth.sample.kernel=K` (status assumed: the rule is VERIFIED, the key field is ASSUMED and the default still uses the ratio rule). Numbers of the 2026-11-14 sweep are in [program-coverage](program-coverage.md) section 5.

## 6. Reproduce

```bash
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

## Implications

- `SincKernelSet::for_key` (`sinc/key_rule.rs`) is the firmware rule; it replaces the ASSUMED ratio rule for the `SAMPLES` oscillator behind `--sample-kernel key` (opt-in, `SynthLayer::sample_kernel_by_key`). Kernels 0 and 1 (`fc` 1.10, 1.05) are used for keys up to B3; the legacy rule never picked them.
- The default stays on the ratio rule: the key field is ASSUMED, and the audible difference (a few tenths of a dB of brilliance between neighbouring kernels, DC gain `1/fc`) has no hardware clip that isolates it.

## Follow-ups

1. Boot the harness with a SAMPLES program, play a note and capture the message to `0x9243B + 0x38 k`: confirms the key field, transposition and the voice id to record map.
2. Run the pitch block `0xC01E14AC..0xC01E1598` for keys 0..127 and compare with the six measured steps of [sy-smp-audio](sy-smp-audio.md) section 4.
3. Decode the ARM writer of the other record fields and the SAMPLE / WAVE bank split between SY and SMP.
