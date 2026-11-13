# ARM edit registry, organ layer and partial boot: what was reached for the organ messages

- **Checked:** 2026-11-12
- **Sources (local, never committed):** `target/fw_unpacked/bank0_CODE.bin` and `bank0_SRAM.bin` (OS v1.66), `tools/arm_boot.py` (new, tests in `tools/tests/test_arm_boot.py`, hand-assembled ARM snippets, no firmware needed), `tools/arm_emu.py`, `capstone` 5, `unicorn` 2.1. Derived data (ids, names, small integers only): [arm-edit-params.csv](arm-edit-params.csv). Prior notes: [arm-fw-emulation](arm-fw-emulation.md), [arm-param-messages](arm-param-messages.md), [preset-dsp-map](preset-dsp-map.md).
- **Question:** Can the edit framework and the organ layer be brought up far enough to set `Edt_Org_Preset_kDb1..9` (`0x16D..0x175`) and the other organ ids and capture the resulting ARM to DSP messages (drawbar encoding, level curve)?

Tags follow `.agents/rules/provenance-in-code.md`: **VERIFIED** = reproduced by the command shown, **EMPIRICAL** = measured but not proven, **ASSUMED** = structural reading.

## 1. Short answer

| Question | Result | Status |
|---|---|---|
| Edit registry built by the real firmware code | Routine `0xC01A44C8` runs unmodified on a zeroed context (50 M instruction budget, allocator `0xC025D56C` stubbed) and registers **422 edit parameters** with name, id, domain, min, max, default, widths. `python tools/arm_boot.py registry target/fw_unpacked/bank0_CODE.bin target/fw_unpacked/bank0_SRAM.bin --csv docs/research/arm-edit-params.csv` | VERIFIED |
| Organ layer object | Constructor `0xC0019BAC` runs unmodified (after a zeroed dispatcher is installed at `[0xC055B160]`) and subscribes to **109 edit ids**, among them all nine `0x16D..0x175`, `0x16B`, `0x16C`, `0x166`, `0x177..0x179` (`python tools/arm_boot.py layer ...`). | VERIFIED |
| Drawbar value domain | 4 bit, **0..8**, default `8 8 8 8 0 0 0 0 0` (a `888000000` registration), widths `(4, 4, 0)`; the handler stores the plain integer `(q16 + 0x8000) >> 16` (section 3). The 7-bit scaling of [preset-dsp-map](preset-dsp-map.md) H1 (`16 d`) is **not** seen at this stage. | VERIFIED (registry, handler disassembly) |
| Drawbar level curve (dB table) | **Not recovered.** The curve lives downstream of the callback in section 3. | open |
| ARM to DSP messages for drawbars, vibrato/chorus, percussion, rotary, organ model, key split, key-on | **Not captured.** No organ message table, no replay. Section 4 gives the exact missing object and where the partial boot stops. | open |
| Partial boot | Reaches about 383.7 M instructions with a tick model, a GPIO handshake model and six stubs (section 5), then calls a garbage function pointer. | VERIFIED (reproducible stop) |

## 2. Edit registry (VERIFIED, `arm-edit-params.csv`)

`0xC01A44C8(ctx, cfg, registry_obj)` stores `[ctx] = 0`, `registry_obj[0x1C] = ctx`, copies three words of `cfg` (`[0]` layer count, default 6) into the globals `0xC052B8BC`, `0xC059B210`, `0xC059B214`, then calls the three registrars `0xC00F10C0` (buffer), `0xC00F1030` / `0xC00F0FFC` (scalar) which call `0xC00F0A28`. That allocates a `0xD4`-byte descriptor, appends it to `ctx[1 + n]` and stores its position at `ctx[0x233 + id]` (byte offset `0x8CC + 4 id`, `-1` = none). Descriptor layout read back (`tools/arm_boot.py` `ENTRY_*`):

| Offset | Field | Evidence |
|---|---|---|
| `+0x00` | edit id | `0x16D` for `Edt_Org_Preset_kDb1` |
| `+0x04` | domain (`0xB` organ common, `0xC` drawbar preset, `0xD` vibrato / percussion preset) | CSV column `domain` |
| `+0x08` | pointer to a `0x80`-byte copy of the name | strings in CSV |
| `+0x10 / +0x14 / +0x18` | min / max / default (signed) | `Fnc_Pan` `-64 / 63 / 0`, `Edt_Org_kOctaveShift` `-6 / 6 / 0` |
| `+0x70 .. +0x78` | three words copied from the registration call: `(1, 7, 0)` for MIDI-like 7-bit values, `(4, 4, 0)` for drawbars, `(1, 1, 0)` for switches | CSV column `w70;w74;w78`; meaning ASSUMED (preset field width in bits) |

The organ block of the registry (all VERIFIED from the run):

| Id | Name | Domain | Range | Default |
|---|---|---|---|---|
| `0x160` | `Edt_Org_kPresetEnable` | `0xB` | 0..1 | 1 |
| `0x161` | `Edt_Org_kKbZone` | `0xB` | 0..11 | 5 |
| `0x162` | `Edt_Org_kOctaveShift` | `0xB` | -6..6 | 0 |
| `0x164` | `Edt_Org_kSustainEnable` | `0xB` | 0..1 | 0 |
| `0x166` | `Edt_Org_kModel` | `0xB` | 0..5 | 0 |
| `0x16D .. 0x175` | `Edt_Org_Preset_kDb1 .. kDb9` | `0xC` | 0..8 | `8 8 8 8 0 0 0 0 0` |
| `0x16C` | `Edt_Org_Preset_kVibratoEnable` | `0xD` | 0..1 | 0 |
| `0x16B` | `Edt_Org_Preset_kB3PercEnable` | `0xD` | 0..1 | 0 |
| `0x177` | `Edt_Org_Preset_kB3PercHarmonic` | `0xD` | 0..1 | 0 |
| `0x178` | `Edt_Org_Preset_kB3PercSpeed` | `0xD` | 0..1 | 0 |
| `0x179` | `Edt_Org_Preset_kB3PercLevel` | `0xD` | 0..1 | 0 |

`kModel` has six values (0..5); the names of the six are not in the registry. The domain meanings are ASSUMED from the grouping of the names. Vibrato / chorus *type* and rotary parameters are not `Edt_Org_*` ids: the CSV has `Edt_OrgCom_*` (`0xAD..0xB0`, layer focus / enable) and the `Edt_Effects_*` block (rotary speaker, `0x1A7` amp type etc.); find them with `grep -i "rotary\|vib" docs/research/arm-edit-params.csv`.

## 3. Drawbar change handler (disassembly, VERIFIED reading; not executed)

`0xC011AB8C(this, r1 = event, r2 = edit id, r3 = &q16 value)` is the change handler of a class whose vtable is at `0xC02BC73C` (the same table holds the store setter `0xC00D051C` at slot 8, so the two classes share a base). For `r2 = 0x16D + k` (`sub r1, r2, #0x16C ; sub r1, r1, #1 ; cmp r1, #8` at `0xC011ABC4`) it does:

```
value = (q16 + 0x8000) >> 16          ; plain integer 0..8, no scaling
engine = [this + 0x98]
engine[0x14 + k] = value              ; nine consecutive words at byte +0x50 .. +0x70
tail-call the pointer-to-member stored at engine + 0x48 (function) / +0x4C (this adjust)
        with (engine + adjust, k, value)
```

So the drawbar reaches the sound engine as `(k, value 0..8)`; the code that turns it into DSP words is the target of that member pointer and was **not** located (the pointer is filled by a constructor of the engine class, not by a literal that a text search finds). Other facts from the same reading:

- The central notify path is `0xC00D2B94` (set a value and notify), which calls the dispatcher `0xC00D2868`: a binary search in the client table `[G + 0x150 + 4 n]` (count `[G + 0x14C]`, `G = [0xC055B160]`) and a virtual call (`vtable[1]`) on every enabled client; edit ids `0x31..0x230` take the branch at `0xC00D2A2C`. Layer constructors append their client to that table (`0xC0019BF8..0xC0019C14`). VERIFIED by disassembly, executed only as far as the constructor.
- The organ layer's own handler `0xC011CB60` only special-cases `0x8E, 0x35, 0x8D, 0x166, 0x16C`; for other ids (drawbars) it forwards through the sub-object table at `[layer + 0x5000 + 0x6C8 + 4 base]` (vtable slot 8 = `0xC00D051C`) when a flag byte is set. The flag pointer table at `layer + 4 (base + 12 slot + 0x15D4) + 4` is zero after the constructor (filled by code that did not run), so the forward does not happen in the harness.
- The UI side `0xC0195BCC` creates one `0xA0`-byte widget per drawbar id at x offsets `0xE0, 8, 0x28, 0x48, ...`: screen drawing, not DSP.

## 4. Why no messages: the missing object

The layer constructor's base `0xC0015B48` fetches a per-layer program object `r4 = [[0xC055A198 + 0xFC8] + 0x17C + 4 layer]` (the same table family as the port object at `[0xC055A198 + 0xFB8]`) and calls its virtual methods 0 and 1 to build the layer's store (`layer + 0x5AB4`). In isolation these objects do not exist: the six null calls of the constructor are exactly there (`0xC00157DC`, `0xC00157FC`, `0xC0015A54`, `0xC0015A74`, `0xC0015BAC`, `0xC0015BC8`), the store has no tables, and `0xC00D051C(store, 0, {0x16D, 1}, 5)` returns without a single call or message (measured in the session with a call tracer; not a committed test, it needs the firmware). Building that object is the job of the application constructor, which is why the partial boot below matters. Calling the layer handler directly (`0xC011CB60`) gives the same silence (flag byte null).

Leads, all **ASSUMED**: (a) the nine strip gain words 26..34 of [arm-fw-emulation](arm-fw-emulation.md) section 3 (`-2^(2n+1)` for integer `n`, driven by strip ids `0x0B, 0x0C, 0x0E, 0x12, 0x13, 0x15, 0x18, 0x19, 0x1B`) is a nine-element array like the nine drawbars, but the callers that set these ids are the strip default routine `0xC01EE590` (constants) and `0xC011B338`; no link to `0x16D..0x175` was found; (b) the controller words `DM(0xC1DCC..)` are front-panel controls (UI manager loop `0xC01EE3B0`), not edit values.

## 5. Partial boot (VERIFIED run, every stub is a FAKE)

`python tools/arm_boot.py boot target/fw_unpacked/bank0_CODE.bin target/fw_unpacked/bank0_SRAM.bin --insns 400000000` (about 2 minutes). Setup: static initialiser `0xC00002A4` (real), then the application constructor thunk `0xC025D524`.

| Item | Treatment | Real or fake |
|---|---|---|
| 64-bit tick `0x80001DC0` (`ldrd` at `0xC0080684`, `0xC0080690`) | `install_tick`: `+5000` per read | modelled (fake time base) |
| DSP block transfer `0xC006CCBC` (waits for a GPIO input bit to equal the toggled output bit; SET at `bank + 8`, CLR at `bank + 0xC` from the `OUT_DATA` pointer, IN at `bank + 0xC`) | `install_gpio_echo`: every IN_DATA bit follows the last SET/CLR | modelled (assumed AM1808 GPIO layout, `GPIO_BASE = 0x01E26000`) |
| cp15 control write at `0xC0024624` (MMU enable) | skipped | fake |
| `operator new` `0xC025D56C` | bump allocator | fake |
| DSP image download `0xC0241C58`, SPI block download `0xC0240548` (retries until the SPI slave answers `1`; the harness SPI returns zero) | return 0 | fake |
| Board configuration query `0xC024BF70` | writes 0, returns 1 | fake |
| Logger `0xC0080314` | returns (kind 8 blinks an LED forever; the failed SPI transfer reaches it through a counter pair at `[obj + 0x218/0x21C]` in the SPI driver) | fake |
| PSC / other device polls | `enable_poll_breaker` | fake |
| Null vtable calls (217 by the stop) | `guard_null_calls`; low memory re-zeroed every 50 000 instructions, because stores through a null `this` otherwise leave data that later null reads follow | fake |

Not modelled: the SPI peripheral (so the DSP download and every SPI device probe is skipped), the AINTC / IRQ vector and RTOS timer tick, the RTOS scheduler. The boot nevertheless passes the delay loops, the soft-float table builds (158 k calls of the double add `0xC025C740` in the first 20 M instructions) and a large `memmove` insertion loop (`0xC0256488` from `0xC002319C`).

**Stop point (reproducible):** after 383.7 M instructions the run executes at `pc = 0x90E7C878` (a heap address) with `lr = 0xC001B830`, i.e. `blx r3` at `0xC001B82C` calls through a vtable slot that holds garbage because an earlier object was not built (the 217 null calls before it are the same symptom). The next step is to decode which object `0xC001B7xx` calls; no further stub was added.

## 6. Second pass: the real notify chain runs (VERIFIED run, `tools/arm_boot.py` `boot_until`)

Section 4 was wrong about the cause: the per-layer objects are not missing from the firmware, they were missing from the *isolated* harness. `boot_until(emu, G_CTOR_PROGRESS)` runs the real boot to `0xC00FC9F4` (about 371 M instructions, edit controller constructor `0xC00FB85C`, called from `0xC024A88C`). There the firmware itself has built the dispatcher `G = [0xC055B160]`, the registry (422 parameters again) and the program objects `[G + 0x17C + 4 n]` (16-byte objects, n = 0..5 and 7 non-null). In that state:

1. `0xC0019BAC` (organ layer constructor) runs with **zero null calls** and registers its client in `G` (`[G + 0x14C] = 1`).
2. `0xC00D051C(store, 0, &{0x16D, 1}, v)` on the layer's store reaches the real chain `0xC00D0578 -> 0xC00D04D0`, `0xC00D0960 -> 0xC00CC4F0 -> 0xC00CDAD0 -> 0xC00CD97C`, then `0xC00CC5A8 -> 0xC011AB8C` (the drawbar change handler of section 3), `0xC011AC0C -> 0xC022AB2C` (the pointer-to-member callback, no null call anywhere).
3. The callback is five instructions: `base = [engine + 0x78]; base[(table[k] + 0x10)] = value; byte base[0x6C] = 1`. `table` (`0xC02BE988`) is the identity `0, 1, ..., 8` (read from the image). So drawbar `k` (edit id `0x16D + k`) lands as the plain integer 0..8 in **word `0x10 + k` (byte `0x40 + 4k`) of an engine record** and sets that record's dirty byte `+0x6C`. The record address is `[0xC055F4B8 + 4 * layer]` copied to `engine + 0x78` by the engine constructor (`0xC0122280..0xC01222F8`). VERIFIED by `set_drawbar` for k = 0, 2, 8 and v = 0..8 (the word reads back v). **This settles the encoding at this stage: the 4-bit integer 0..8, no 7-bit scaling and no table lookup.**
4. The record is consumed by `0xC00BE348(record)` (needs byte `+0x6D` set, `run_record_update` sets it): clears the dirty byte, walks the sub-records `[record + 0x28]`, `[record + 0x2C]` with a double-precision table lookup (`0xC0270BD4` int to double, `0xC025D11C`, `0xC0270C80` multiply, `0xC0270F78` double to float) and calls `0xC022DDC8(selector_a, selector_b, index, float)`.
5. `0xC022DDC8` is a jump table over two selectors (0..3 each) that adds a per-selector base to `index` and tail-calls the **real sender `0xC001FF18(word_address, int_value)`**: it converts the int to a float, forms the system address `(w + 0x0A000000) << 2 = 0x28000000 + 4 w` (this is the first direct firmware confirmation of the `DM word <-> 0x28000000 + 4 w` mapping used by [arm-param-messages](arm-param-messages.md)), and stores `(address, [0 pad], value)` to `[[port object] + 0x44]` = the PNO comm port (comm 1, **core 1 addresses `0x2825....` and `0x282D....`**).

So the organ path reaches **PNO core 1**, the core that holds the tonewheel bank ([organ-tonewheel-bank](organ-tonewheel-bank.md)). Captured (VERIFIED run; the harness re-installs the fake DSP ports after the boot because the boot rebuilds `[0xC055B150]`):

| Trigger | Messages (PNO core 1, path 2) |
|---|---|
| `0xC00BE348(record)` after `set_drawbar(k, v)`, any k in {0, 2, 8}, any v in 0..8 | `DM 0x951F1`, `0xB44C9`, `0x95241`, `0xB44CB` (system addresses `0x282547C4`, `0x282D1324`, `0x28254904`, `0x282D132C`), each with value `0` |

These four messages **do not depend on k or v**, so they are not the drawbar levels: they come from the curve rows of the record (`[record + 0x28]`) that the update walks while the double table at `[0xC0599950 + 0x38] -> 0xC027C2B0` (all `1.0` in the image, read from the firmware) and the unbuilt part of the record leave at zero. The drawbar words at record `+0x40..+0x60` are **written** by the callback but no reader of them was found yet: the updater `0xC022F060(engine)` (large, per-layer, reads `[engine + 0x34]` and the record dirty/busy bytes at `0xC022FED8`) ran without producing a drawbar message either. Not captured therefore: the dB curve, vibrato/percussion/rotary/model messages (the ids `0x16B/0x16C/0x166` go through other branches of `0xC011CB60`, not tried).

Next concrete steps: (a) find the reader of `record[0x10..0x18]` (watch the nine words with a memory-read hook while calling `0xC022F060`, `0xC00BE348` and the other per-layer periodic functions); (b) run the boot further than `0xC00FC9F4` once the garbage vtable call at `0xC001B82C` (event insert `0xC001B67C`, called from `0xC0168658` through the UI/display setup) is skipped, since the real updater tasks need the rest of the object graph.

## 7. Drawbar level curve and the first ARM message to DSP audio chain (VERIFIED)

**Drawbar curve.** The reader of the nine drawbar words is the record update `0xC00BE348` itself (read hook: `0xC00BE3A8` and the copy loop `0xC00BE7B8..0xC00BE80C`). It maps each edit value `v` (0..8) through the 9-entry integer table at **`0xC02B0EC0`** into a 9-word array `[record+0x28]+0x18` of 7-bit values:

| Drawbar `v` | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 |
|---|---|---|---|---|---|---|---|---|---|
| 7-bit value | 0 | 3 | 6 | 13 | 22 | 39 | 62 | 87 | 127 |
| dB re 127 | off | -32.5 | -26.5 | -19.8 | -15.2 | -10.3 | -6.2 | -3.3 | 0 |

(`python tools/arm_bank.py <bank0_CODE.bin> <bank0_SRAM.bin>` prints it; run check: drawbars set to `1..8, 0` give the array `3 6 13 22 39 62 87 127 0`, all eight set to 8 give `127` everywhere.) So the answer to "is the drawbar value 7-bit scaled?" is: **yes, by a nonlinear table, about -3 dB per step at the top and steeper below** (neither the linear `d/8` of `organ.rs` nor `16 d`). Status: the table is VERIFIED (image bytes + run); that the 7-bit array is the final gain seen by the DSP is ASSUMED, because the messages that this routine sends in the harness (the four PNO words of section 6) did not change with the array, and the sender of the array itself was not found. The 7-bit range matches the controller-word state `(value7 << 8) | 0x20` of [arm-fw-emulation](arm-fw-emulation.md) section 4, which is the likely next hop.

**Bank programming (`tools/arm_bank.py`).** `0xC022D9F0(this, level)` writes 184 phase increments (`word << 8`) to PNO core 1 DM words `0x955B6 .. 0x9566D` through the inline port store (system address `0x282556DC + 4 i`; table at `0xC0444188` for the exact 12-TET bank, the detuned bank for level 1) and passes four gains to a virtual setter (indices 5..8). Run with a fake object graph (the setter logs the calls; the increments are the real firmware's stores):

| `level` | byte `this+0x4E` | gains 5, 6, 7, 8 | Increment table |
|---|---|---|---|
| 1 (vibrato enable) | 0 | `0x600000, 0x600000, 0x300000, 0x300000` | detuned |
| 1 | 1 | `0x600000, 0, 0, 0x600000` | detuned |
| 0, 2, 3 | 0 | `0x7FFFFF, 0, 0x7FFFFF, 0` | exact 12-TET |
| 0, 2, 3 | 1 | `0x7FFFFF, 0, 0, 0` | exact 12-TET |

All 184 increments for both banks are in [data/arm-bank-increments.csv](data/arm-bank-increments.csv) (`0x00635300` is the first exact-12-TET word, `0x00635000` the detuned one).

**Replay (VERIFIED).** The increments become `sharc-bank` pokes (`bank_script`, lines `CALL ADDR VALUE`):

```bash
python tools/ns4_dsp_images.py target/fw_unpacked/bank0_CODE.bin --out DSP
python tools/ns4_dsp_ldr.py DSP/dsp_image_1_PNO.bin --sram SRAM
python tools/arm_bank.py target/fw_unpacked/bank0_CODE.bin target/fw_unpacked/bank0_SRAM.bin --level 1 --script l1.script
target/release/sharc-bank SRAM/core1_sram.bin --calls 2 --script l1.script --out l1.out --dump 0x955B6:4
```

`--dump 0x955B6:4` reads back `635300 635300 693b00 693b00` for level 0 and `635000 633e00 691900 695700` for level 1 (the pokes land), and the two output arrays of the 2 calls (`2 x 2 x 368` words) differ in **260 of 1472 words** between the two banks. That is the first complete chain *ARM routine -> port stores -> DSP words -> oscillator bank output* from real firmware messages. The four gains are not poked (their setter target was not decoded), so both runs use the image's default weights.

Not done: the organ vs piano vs synth reachability of `0xC022D9F0` / `0xC0236410` (static callers only: organ apply `0xC01209C4/0xC0120C60`, part routing `0xC0236410`; no piano id was driven), the vibrato / percussion / rotary / model ids through `0xC011CB60`, and the weights/post-stage words.

## 8. Id sweep through the notify path and the piano layer (VERIFIED run, mostly negative)

In the booted state (`boot_until(G_CTOR_PROGRESS)`, organ layer built by `0xC0019BAC`), every `Edt_Org_*`, `Edt_OrgCom_*`, `Edt_Sys_kLeslie*`, `Edt_Pno_*` and rotary id of `arm-edit-params.csv` (67 ids, values 1 and 2) was set through the real store setter `0xC00D051C(store, 0, &{id, 1}, v)` with a call tracer and the fake DSP ports:

| Ids | Reached code | Messages |
|---|---|---|
| `Edt_Org_Preset_kDb1..9` (`0x16D..0x175`) | `0xC01E8968 -> 0xC011AB8C -> 0xC022AB2C` (record word `0x10 + k`, section 6) | none directly (the record update sends, section 6/7) |
| `Edt_OrgCom_kLayerLevelOrgA/B` (`0xB1`, `0xB2`) | `0xC01E8968 -> 0xC011AB8C` (the float branch at `0xC011AC38`) | none |
| vibrato enable `0x16C`, percussion `0x16B/0x177..0x179`, `Edt_Org_kModel` `0x166`, key zone/octave/sustain, all `Edt_OrgCom_*` vibrato/Leslie ids, `Edt_Sys_kLeslie*` | no handler code reached | none |
| all 15 `Edt_Pno_*` ids (`0x14D..0x15E`) | no handler code reached | none |

So in this harness only the drawbar and layer-level ids have a live handler: the others need clients (vibrato bank programmer `0xC022D9F0` is called from the organ apply `0xC01209C4/0xC0120C60` with the `Edt_Org_Preset_kVibratoEnable` flag, Leslie from the effects layer) that are created by later parts of the boot. **Not captured:** vibrato / percussion / model / rotary messages. Their DSP effect for the bank is covered by section 7 (`0xC022D9F0` with level 1 versus others).

**Piano layer.** The piano layer constructor is `0xC00194B8(layer, index, 0, 0)` (subscriptions `0xC0019634..0xC0019A50`, ids `0x14D..0x15E` among others, vtable `0xC027ACA0`). It runs with zero null calls in the booted state, but its edit store is not at `[layer + 0x5000 + 0x6C8]` (that pointer is 0 for the piano layer) and setting the piano ids through the organ layer's store reaches no handler either. The organ-versus-piano comparison of PNO core 1 usage is therefore **not decided here**; the static fact from section 7 stays: the organ drawbar record path and the bank programmer both write PNO comm 1 core 1 words.

**Sender of the 7-bit array.** Not found. The array `[record+0x28]+0x18` is read by `0xC00BE348` itself (section 7) and no other reader fired in the harness.

## Implications

- `arm-edit-params.csv` is the authoritative id, name and range list for the 422 edit parameters; the organ drawbar field is a 0..8 integer at the ARM edit layer, which settles the "scaled 7-bit?" question for *that* layer only.
- The remaining work is the object graph below the application constructor: the per-layer program object (section 4) and the engine's member-pointer callback (section 3).

## Follow-ups

1. Identify the call at `0xC001B82C` and what the harness did not build; add the missing SPI model (the SPI driver `0xC0066E64` is a TI SPI master: TXFULL `0x2000` and RXEMPTY `0x8000` at `+0x42`, data at `+0x40`/`+0x3C`) so the DSP downloads can run instead of being skipped.
2. After the application constructor returns, call `0xC00D2B94` (set and notify) for `0x16D + k` and read `emu.messages`; if the engine callback sends nothing itself, trace the member pointer stored at `engine + 0x48`.
3. Find the constructor that fills `engine + 0x48 / 0x4C` (store with a literal function address into a `0x50`-byte-plus object whose words `0x14..0x1C` are the drawbars).

## 9. Boot past `0xC00FC9F4` (VERIFIED runs, `tools/arm_boot_full.py`)

Checked 2026-11-12. Sources: `bank0_CODE.bin` v1.66 disassembly, `unicorn` runs. Question: why does the boot die at `blx r3` `0xC001B82C`, and how far can it go.

**Root cause 1: the harness skipped the C++ static initialisers.** `__rt_lib_init` (`0xC026BA04`) calls every word of `[0xC052B890, 0xC052B8A0)` = `0xC00002A4, 0xC024D1A0, 0xC024DC64, 0xC025DBBC`. `arm_emu.run_static_init` ran the first with every `bl` skipped. Run for real (all four, `arm_boot_full.run_init_array`, instant) they fill BSS tables the constructors read, for example the edit id table `0xC0572EC0` (`0xE8, 0xEB, ...`). With it zero, a client constructor (`0xC0149B94`, called from `0xC019FA24`) passed id 0 to the subscribe routine `0xC00DCA30`, whose range check (`id - 0x31 < 0x200`) failed into the compiler trap at `0xC00DCC5C` (`mov r3,#0; ldr r3,[r3,#0x14]; udf`). That was the second stop after the stubs below.

**The `blx r3` at `0xC001B82C` itself** is not a garbage vtable. `0xC001B67C(queue, a, b, desc)` is a timer event queue insert (sorted by the time word `[ev + 8]`, queue count at `[queue + 0x18]`). It is called by `0xC001ED84` (five call sites `0xC0168658..0xC0168724`, UI/display setup) with `desc = [[0xC055EF70 + 0x308] + 0x28 + 4 i]`. `[0xC055EF70 + 0x308]` is never written before this point, so `desc` is null and `[desc]` (the vtable) reads 0. Fix: `EVENT_INSERT` is a documented no-op stub (ASSUMED harmless: it only schedules display refresh). The text measure routines (`0xC001B9D0`) dereference a null font glyph table (`[font + 8]`, LCD driver not modelled): stubbed to return 0, and the page `0xFFFF0000` is mapped so the resulting `0xFFFFFFFC` read returns zero.

**Further stubs found while iterating** (`arm_boot_full.install_ui_stubs`): the SPI transfer `0xC0066E64` (fifth stack argument receives the slave's answer; the poll breaker fed `0x3F0077`, and the caller at `0xC0228200` then allocated 4.1 M objects of 0x14 bytes, exhausting the heap) is stubbed to answer 0, and the count check at `0xC02281F0` forces r5 = 0 (the stub alone left `-1` in r5; ASSUMED: "nothing to allocate" is a legal answer). With both, the allocation loop is skipped.

**How far the boot gets (measured, 12 M instructions/s up to 0.4 B, 1 M/s after):**

| Instructions | State |
|---|---|
| 0.38 B | previous stop point (`0xC001B82C`) passed |
| 0.40 B | SPI answer loop at `0xC02281FC` (fixed above) |
| 0.43 B to 0.48 B | still running real boot code (`0xC0228B68`, `0xC00EBBA8..0xC00EBBC4`); the earlier 3-instruction wait loop at `0xC0075D40..D48` (`ldr r3,[r4,#0x1c]; cmp r3,#0; bne`, a join on a worker task flag that no scheduler clears) was seen by the probe `p3` only before the SPI count fix; with the fix the boot moves on |
| beyond 0.5 B | not reached within the session time (the run is slow after 0.4 B: roughly 1 M instructions/s) |

**Superseded by section 10.1: the boot does reach its steady state (main loop, 433 M instructions).** What follows is the state of the earlier session. The boot has therefore **not been shown to reach its steady state**; it passes the former stop and is still executing real code at 0.48 B. The RTOS task scheduler, AINTC and SPI are not modelled; the likeliest next blockers are task-join wait loops of the form above (model: clear the flag word, or run the worker).

**Sweep (`tools/arm_sweep.py`, `docs/research/arm-sweep.csv`).** 116 ids (`Edt_Org*`, Leslie, `Edt_Pno*`, rotary, `Edt_Effects*`) x values 1 and 2 through `0xC00D051C(layer + 0x5AB4, 0, &{id, 1}, v)` at 0.47 B instructions: **0 messages and 0 null calls**, the same silence as section 8 at the `0xC00FC9F4` state. So the vibrato/percussion/model/Leslie/Effects clients are still not constructed at 0.47 B; the sweep has to be re-run once the boot goes further (`python tools/arm_sweep.py <bank0_CODE.bin> <bank0_SRAM.bin> --boot-insns N`).

**Organ vs piano (PNO core 1):** not decided. No new evidence beyond section 7 (organ drawbar record path and bank programmer both write PNO comm 1 core 1); the piano layer edit store was not located.


## 10. Steady state, timer interrupt and the edit-id capture (VERIFIED runs, `tools/arm_steady.py`, `tools/arm_edit_capture.py`)

Checked 2026-11-13. Sources: `bank0_CODE.bin` / `bank0_SRAM.bin` v1.66 disassembly (capstone), `unicorn` runs, firmware-derived values only as addresses and message words. Question: does the boot reach a steady state, and which ARM to DSP messages do the organ, piano and effects ids produce there.

### 10.1 The boot does reach the main loop (corrects section 9)

| Item | Result | Status |
|---|---|---|
| Speed | 15 to 20 M instructions/s for the whole run (3.0 B instructions in 201 s). The "1 M/s after 0.4 B" of section 9 does not reproduce; it came from running extra Python hooks during sweeps. No speed work was needed. | VERIFIED (run) |
| Main loop reached | `arm_steady.boot_to_main_loop` stops at `0xC00A0248` after **433 M instructions (36 s)**; no further stub than section 9 | VERIFIED (run) |
| Dispatcher clients at that point | 10 layer clients registered in `[0xC055B160] + 0x150 + 4 n` (count at `+0x14C`): the organ layer constructor `0xC0019BAC` ran at 393 M, the piano one at 394 M | VERIFIED (run) |
| "Join on a worker task" `0xC0075D40` | entered 4 times, **with `r4 = 0`**: the flag is `[0 + 0x1C]` in the null-call guard page, and the loop (about 60 000 iterations each) ends only because the harness re-zeroes low memory every chunk. It is an artefact of an object that was never built, not a worker task | VERIFIED (run, hook on `0xC0075D40` and a write watch on `0x1C`) |

**There is no preemptive scheduler to model.** The firmware is a cooperative event loop (routine containing `0xC00A0248`, started from `0xC024AE3C`): `ring = [0xC05613F4 + 4 * ([0x8000C000] + [0xC0539570])]`; head `[ring]`, tail `[ring + 4]`, mask `[ring + 0xC]`, 64-byte messages at `[ring + 0x10]`; an empty ring falls through to the tick branch (`0xC00A03CC`), which compares the 64-bit tick `0x80001DC0` with the last value `[0xC0598640]` and, when it changed, pops and calls the next entry of the tick queue (`0xC0052AA4`). A post (`0xC00A05D4`) increments the pending count and writes `0x80000000 | level` to `0xFFFEF104` around the update; that address is `AINTC + 0x1104`, the host interrupt nesting level register of the AM1705 AINTC, so it is an interrupt-level critical section and not a context switch.

**Interrupts.** The AINTC attach routine `0xC005012C(dev, handler, channel)` (irq = `[dev + 4]`) fills the handler table `0xC055F674`. After the boot: irq 14 -> `0x80000AD8`, irq 22 -> `0x800007D8`, irq 58 -> `0x80000770`, irq 61 -> `0x80000434` (the SRAM image holds the handlers). Irq 22 is the 1 kHz timer (`0xC005019C` programs `24 MHz / 0x5DC0` = 1 ms; Timer0 `0x01C20000`): `0x800007D8` does `tick64 += 1`, sets the context index `[0x8000C000] = 3` and drains the context 3 ring. Irq 61's handler stores received bytes into a ring (ASSUMED: MIDI input UART).

**Harness.** `arm_steady.run_ticking` runs the main loop in chunks of `INSNS_PER_TICK = 200 000` instructions (ASSUMED: about 200 MIPS, one virtual millisecond per chunk) and calls the registered irq 22 handler between chunks (`call_preserving`: registers and CPSR are restored, the stack below sp is used). `enable_real_timer` switches off the +5000-per-read tick hack of `arm_boot.install_tick` once the real tick runs.

**Initial DSP push.** In the first 140 M instructions after the main loop starts the firmware sends its whole initial state to every DSP, then goes silent (messages per 20 M-instruction window): 4402, 6486, 3059, 1014, 12, 320, 3, 0. `arm_steady.settle` runs until a window is silent (8 windows) and `arm_steady.snapshot` saves the result (`target/settled.snap`, 3.6 MB compressed, restore 2 s). An idle window still emits 3 words (PNO core 1 DM `0xB2F25`, `0xB2F26`, `0xB2FA1`) every 28 M instructions; `arm_edit_capture.baseline_keys` collects them (12 windows) and drops them.

### 10.2 Layers, stores and where the values live

The 10 clients of the dispatcher (`[0xC055B160] + 0x150 + 4 n`, client = layer + `0x8788`) are the layers; the layer vtable `[layer]` and the ids each layer owns identify them (EMPIRICAL, counted with `value_cell`):

| Clients | `[layer]` | Owns (value cells) | Role |
|---|---|---|---|
| 0, 1 | `0xC027AD8C` | 12 `Edt_OrgCom`, 4 `Edt_Org`, 14 `Edt_Org_Preset`, 3 `Edt_Effects` + `Fnc/Kbd/Morph/Edt_Com` | organ A / B (constructor `0xC0019BAC`) |
| 2, 3 | `0xC027ACA0` | 7 `Edt_PnoCom`, 15 `Edt_Pno`, 5 `Edt_Effects` | piano A / B (constructor `0xC00194B8`) |
| 4, 5, 6 | `0xC02B7C4C` | 65 `Edt_Syn`, 10 `Edt_SynCom`, 5 `Edt_Effects` | synth (store class `0xC011AED0`, vtable `0xC02BC9FC`) |
| 7, 8, 9 | `0xC027A80C` | 17 `Edt_Sys`, 15 `Edt_Ext`, 11 `Edt_Syn`, 9 `Edt_SynCom` | not decoded (ASSUMED system / external controller layers) |

**Effects are not here.** Of the 37 `Edt_Effects_*` ids only `Fx1Enable`, `Fx1ClkEnable`, `Fx1Type`, `AmpEnable` and `AmpTypeNew` have a value cell, and no client owns any Fx2, Dly, Rvb, Cmp, Leslie (`Edt_OrgCom_kEffectsLeslie*`, `Edt_Sys_kLeslie*`) or `Edt_Com_kEffects*Global` id. Their client is not in the dispatcher's list at the main loop (not found; needs a search for a second registry or a later constructor).

**Why section 8 saw nothing.** Each layer has two store objects (`layer + 0x56C8 / 0x56CC` and the following slots); the setter `0xC00D051C` only accepts the ids that the store's flag array (`[store + 0x3C]`, byte per id, mask 6) marks: 11 ids (`0xB1, 0xB2, 0x16D..0x175`) in the organ store, 2 in the piano store. The other ids never pass the setter. The client dispatch `0xC00CC4F0(store, 0, id, &value)` does reach every handler, but the handlers ignore the pointer: they read the value with the store getter `0xC00D04D0`, which forwards to the parent object's getter `0xC00CE594`, i.e. to a value table: **value of id `i`, index `k` = `[[[[P] + 4i]] + 4k]`** with `P = [[store + 0x14] - 0x9C]` (VERIFIED: 64 to 85 of the registry defaults per layer read back through this path, only `Fnc_*` and the key position differ). `arm_edit_capture.apply_value` writes the cell and then dispatches.

### 10.3 Captured messages (118 658 messages from 34 of the 116 swept ids)

```bash
python tools/arm_edit_capture.py target/fw_unpacked/bank0_CODE.bin target/fw_unpacked/bank0_SRAM.bin \
    --cache target/settled.snap --csv full.csv --summary summary.csv   # about 25 minutes
```

Method: from the settled snapshot, each value of each id (every value of a range of up to 12, else minimum, quartiles, maximum) is applied to every layer that owns the id, from the settled state (the state is restored after any value that spoke); the main loop then runs 4 M instructions with the timer. Idle traffic (3 PNO core 1 words) is removed. "call" = sent during the dispatch, "tick" = sent by the following window. Tables: [data/arm-edit-capture-summary.csv](data/arm-edit-capture-summary.csv) (all ids, per value), [data/arm-edit-capture-detail.csv](data/arm-edit-capture-detail.csv) (every message for the organ ids of this section on organ layer 0 and the piano ids on piano layer 2; addresses and values only).

| Id (value range) | Destination | DM words | Messages (VERIFIED run) |
|---|---|---|---|
| `Edt_Org_kModel` `0x166` (0..5) | PNO comm 1 core 1 | `0x955B6..0x9566D` (the 184 bank increments), `0x951F1..0x95C6B`, `0xC27D6/7` | model 3 and 4: 1504 messages each, 1484 non-zero, **all 184 bank words equal to the exact 12-TET column of [data/arm-bank-increments.csv](data/arm-bank-increments.csv)** (8 rounds x 184, 1472 of 1472 match, 0 match the detuned column); model 2: 2 words at `0xC27D6/7`; model 5: 4 zero words at `0xB44AE..0xB4530`; model 0 (default): 350 messages, 30 non-zero (a re-apply); model 1: none |
| `Edt_Org_Preset_kDb1..9` `0x16D..0x175` (0..8) | PNO comm 1 core 1 | `0x951F1..0xB44CB` (160 words) or `..0xB454B` (320 words), by the tick | **every value is 0 for every drawbar value 0..8** (same words for k = 1..9) |
| `Edt_Org_Preset_kVibratoEnable` `0x16C` (0, 1) | PNO comm 1 core 1 | value 1: `0xB44AE`, `0xB452E`; value 0: 160 words `0x95331..0xB454B` | all zero |
| `Edt_OrgCom_kVibratoTypeB3/Vox/Farfisa` `0x168..0x16A` | PNO comm 1 core 1 | `0xB44AE, 0xB44B0, 0xB452E, 0xB4530` | all zero (4 words, 2 for the last value) |
| `Edt_Org_Preset_kB3PercEnable/Harmonic/Speed/Level` `0x16B, 0x177..0x179` (0, 1) | PNO comm 1 core 1 | `0x951F1..0xB4540` (about 250 words) | 30 to 36 non-zero values per value (`0x3F800000` = 1.0, `0x3A694800`, `0x3F7FC654`) |
| `Edt_OrgCom_kPolyPercEnable_revB` `0xE1` | PNO comm 1 core 1 | `0x95241..0x953CF` (158 words) | all zero |
| `Edt_OrgCom_kLayerFocus/EnableOrg_I/II_private`, `kOrgPStickEnable`, `Edt_Org_kSustainEnable`, `Edt_OrgCom_kAuxKbdControl` | PNO comm 1 core 1 | 4 words `0xB33B8..0xE20E0` | 12 per value |
| `Edt_Pno_kPosStatus, kBank, kEntry, kVariation, kPedalNoise, kEq` `0x152..0x155, 0x158, 0x15C` | **SMP comm 4, path 1** | `0x9403E`, `0x94046` | 2 per value, **always `0x3F800000` (1.0) whatever the value** (a trigger, not the parameter) |
| `Edt_Pno_kUnison` `0x15D`, `kAcoustic`, `kDynComp`, `kKbdTouch`, `kSustainEnable`, `kKbZone`, `kOctaveShift`, `kNameHash_Private`, `Edt_PnoCom_*` | none | none | silent in this state |
| `Edt_Effects_kAmpEnable` `0x19E` (0, 1), `kAmpTypeNew` `0x1A7` (0..9) | PNO comm 1 core 1 (+ SMP path 1) | the same 184 bank words `0x955B6..0x9566D`, the `0x95xxx` block up to `0xC2CB7`, SMP `0x94086..0x9408D` | 6208 / 31 040 messages on the piano layers (9312 / 46 560 on the synth layers); the bank words equal the exact 12-TET column in 100 % (2944 of 2944 per type); this is the part routing `0xC0236410` re-programming every part |
| The 82 other ids of the filter (Leslie, `Edt_Effects` Fx2 / Dly / Rvb / Cmp / Amp tone controls, `Edt_PnoCom_*`, `Edt_Org_kKbZone`, ...) | none | none | silent: no layer owns them (effects, Leslie) or the handler sends nothing here |

Statuses: destinations, DM words, counts and the bank equality are **VERIFIED** (reproducible with the command above). The **meaning** of the zero drawbar / vibrato words is **ASSUMED**: the record update `0xC00BE348` forms products whose weight tables come from data the harness does not load (the SPI downloads are stubbed), so every product is 0. The drawbar encoding (7-bit curve, section 7) therefore still cannot be read from a message.

### 10.4 Organ versus piano, and the PNO core 1 bank

| Question | Evidence | Verdict |
|---|---|---|
| Do organ and piano address the same comm / core? | All 25 482 organ messages go to **PNO (comm 1) core 1 over path 2**. All 56 messages of the piano ids `Edt_Pno_*` go to **SMP (comm 4) over path 1** (`0x9403E`, `0x94046`). | **No.** The piano layer does not talk to PNO core 1 at all in this capture. |
| Does the organ program the oscillator bank (`0x122620`)? | `Edt_Org_kModel` = 3 or 4 writes the 184 phase increments `0x955B6..0x9566D` (the table that `0x122620` consumes, [organ-tonewheel-bank](organ-tonewheel-bank.md)) with the exact 12-TET values: 1472 of 1472 words equal the firmware table. | **Yes** (VERIFIED: ARM organ id to bank table). |
| Is the bank exclusive to the organ? | The same 184 words are rewritten by `Edt_Effects_kAmpEnable` / `kAmpTypeNew` on the piano and synth layers (part routing). | **No**: other layers re-program it through routing. |
| Is the bank therefore the organ tone engine? | Organ ids reach only PNO core 1; the piano ids reach only SMP; model 3/4 select the bank tuning. The string-resonance reading of [pno-module-chain](pno-module-chain.md) would need piano ids to address PNO core 1, and none does (`kUnison`, `kAcoustic`, `kDynComp` are silent here). | **Supported, not proven.** ASSUMED that the bank is the organ tonewheel source; a loaded preset giving non-zero drawbar words, or the audio path from the bank to the output, would confirm it. |

### 10.5 Replay into the PNO core 1 image (`sharc-bank --script`, VERIFIED run)

```bash
python tools/ns4_dsp_images.py target/fw_unpacked/bank0_CODE.bin --out DSP
python tools/ns4_dsp_ldr.py DSP/dsp_image_1_PNO.bin --sram SRAM
cargo build --release -p nord-sharc --bin sharc-bank
target/release/sharc-bank SRAM/core1_sram.bin --calls 4 --out base.out --dump 0x955B6:184
target/release/sharc-bank SRAM/core1_sram.bin --calls 4 --script docs/research/data/arm-replay-organ-model3.script --out m3.out --dump 0x955B6:184
```

| Script | Pokes | DM words that change | Oscillator output (4 calls, 736 words each) |
|---|---|---|---|
| [arm-replay-organ-drawbar-vibrato.script](data/arm-replay-organ-drawbar-vibrato.script): drawbar 1 set to 4 (160 words, before call 1), vibrato on (before call 2), vibrato off (before call 3) | 322 (321 words) | **1**: `0xB44AE` goes from `0x3F800000` (1.0) to 0; the 320 other words are 0 in the image already | 0 of 736 words differ in every call |
| [arm-replay-organ-model3.script](data/arm-replay-organ-model3.script): organ model 3 (before call 1) | 752 (200 words) | **194**: the bank increments `0x955B6..0x9566D` go from 0 to the exact 12-TET table (`0x00635300, 0x00635300, 0x00693B00, ...`) plus 10 further words | **736 of 736 words differ in each call after the poke** (the bank is silent with zero increments and runs with them) |

So the one message burst that changes the bank output is the model selection (it installs the increments); the drawbar and vibrato bursts only write zeros in this state and change one flag word.

### 10.6 Limits and next steps

- The 10 clients are the ones the boot builds by 433 M instructions; the effects / Leslie client (Fx2, Dly, Rvb, Cmp, Leslie ids) is not among them. Next: find where it is constructed (a second registry? the owners of `Edt_Com_kEffects*Global`) and add it to `LAYER_KINDS`.
- Drawbar and vibrato levels need the weight tables that the stubbed SPI downloads would load; with them the 160-word blocks should carry gains. Next: model the SPI flash read for the organ tables, or load a `.ns4o` through the preset-load function.
- `INSNS_PER_TICK` (200 000) is ASSUMED. The captured messages do not depend on it except for the idle words and the time at which "tick" messages appear; a 4 M-instruction window is 20 ticks.
- The role of clients 7, 8, 9 and the meaning of the SMP words `0x9403E / 0x94046` are not decoded.
