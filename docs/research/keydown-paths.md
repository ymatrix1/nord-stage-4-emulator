# Key-down and key-up paths in the booted host firmware

Checked: 2026-11-13.

Sources: `target/fw_unpacked/bank0_CODE.bin` and `bank0_SRAM.bin` (firmware v1.66, local only); `tools/arm_keydown.py` (new), `arm_steady.py`, `arm_boot_full.py`, `arm_edit_capture.py`; [arm-organ-messages](arm-organ-messages.md) sections 9 and 10; [piano-dsp-path](piano-dsp-path.md) (PR #125, section 7); [synth-voice-control](synth-voice-control.md).

Question: how does a real note-on / note-off reach the layers in the emulated firmware, and why did earlier agents see no DSP message for the piano, synth and organ key paths?

## 1. The MIDI consumer was never reachable: the board-present flag

| Item | Result | Tag |
|---|---|---|
| UART2 ring consumer | The only code that reads the UART2 ring (`[ring + 8]` head against `[ring + 0xC]` tail) is the MIDI parser `0xC008D448(this)` with `this` = the UART driver object `0x90165730` (`[this + 0x24]` = ring `0x90165770`, `[this + 0x28]` = 0x100 bytes per call, `[this]` = the MIDI sink object `0x901687C0`). A scan of the whole image for `ldr rX, [rY, #0x24]` followed by loads of `[rX + 8]` and `[rX + 0xC]` finds only this function | VERIFIED (scan + run) |
| Why it never ran | No `bl` and no data word in the main image points at `0xC008D448`. The only reference is the SRAM veneer `0x80001D80` (`ldr pc, [pc, #-4]`), called twice by the SRAM routine `0x800015A8`. That routine is the **hook of slot 2** | VERIFIED (disassembly) |
| Hook registration | `0xC024DB74(cfg)` registers the main loop hooks with `0xC00A1204(slot, fn)` (stores `fn` at `[[0xC0597FC0 + 0x670] + 4 (slot + 2)]`). The real hooks are registered only when `0xC024BF8C(cfg)` is non-zero, which is `[cfg + 0x12] ? [cfg + 0x13] : 0`. Otherwise four tiny stubs (`0xC024D1E4`, `0xC024D200`, `0xC024D218`, `0xC024D21C`) are registered | VERIFIED (disassembly + run) |
| Why the flag stayed 0 | The boot stub `arm_boot.install_boot_stubs` replaces the board probe `0xC024BF70(cfg, i, &out)` (returns `[cfg + 0x12 + i]` and `[cfg + 0x14 + 4 i]`) and the routine that fills the bytes never runs. At the attach call `cfg = 0x90000210` holds `01 01 00 00 02` at `+0x10..+0x14` (run) | VERIFIED (run) |
| Fix | `arm_keydown.install_board_present`: a code hook at `0xC024DB74` writes `01 01` at `cfg + 0x12`. After the boot the hook table reads `1: 0x80000B30, 2: 0x800015A8, 3: 0x800013D8, 4: 0x80000AE0, 6: 0x80000D1C, 7: 0x80000AF4`. The settle takes 8 windows as before | VERIFIED (run, `hooks_are_real`) |
| Side effect | The parser runs once per hook round: 106 calls in 40 M instructions, about once per 2 ticks | VERIFIED (run) |

The real hooks are the same four things that a board with all peripherals would run each phase: slot 2 polls the panel/keybed device window (`0x60000000`, writes `0x60000010..0x60000024`) and then the MIDI parser on `[G + 0x20]` and `[[G + 0x28] + 0xC]`. The `0x60000000` device (EMIFA, probably a panel / keybed scanner) is **not modelled**: its register writes land in zero pages. Hooks 1, 3, 4, 6, 7 were entered in a 20 M instruction run (13, 50, 6, 1, 1 calls) without a fault.

## 2. UART2 model and the MIDI note path

UART2 is a TI 16550 at `0x01D0D000`. The receive ISR (irq 61, handler `0x80000434`, table `0xC055F674`) loops: `LSR` (`+0x14`) bit 0, `RBR` (`+0`), store into the ring at `head`, `head = (head + 1) % size` (helper `0x80001DA8`). `arm_keydown.Uart2` serves `RBR`, `LSR` (`0x61` ready, `0x60` idle) and `IIR` from a byte queue; `Uart2.push` queues the bytes and calls the ISR through `arm_steady.inject_irq`.

| Step | Address | Evidence |
|---|---|---|
| ISR stores `90 3C 64` | `0x80000434`; ring `(size, head, tail)` = `(4096, 3, 0)` | VERIFIED (run) |
| Parser pops the bytes (status 0x90 = note-on) | `0xC008D448`; ring `(4096, 3, 3)` after the next hook round | VERIFIED (run) |
| Parser calls the sink | `0xC0085630` (note-on, `r1` = channel, `r2` = note, `r3` = velocity) / `0xC00855B8` (note-off); each posts one 64-byte event record into the event ring `[sink + 8]` (record: `+0` event class literal, `+4` sink id, `+8/+0xC/+0x10` = channel / note / velocity) | VERIFIED (run, disassembly) |
| Event processor | the timer ISR drains the context rings through `0xC00A4720` (SRAM veneer `0x80001D70`): `0xC00A4808 -> 0xC00D2084 -> 0xC00116A8 (layer handler) -> 0xC00D1F00 (loop over the layer clients: mask test of a 64-bit mask at [client + 0x28], then virtual call [vtable + 0x1C]) -> 0xC00C8864 -> 0xC00C8734 -> 0xC00C794C / 0xC00C7844 -> 0xC01AC7B4 (forwarder `blx [vtable + 8]`)` | VERIFIED (stack scan `arm_piano_path.stack_return_candidates`) |
| Piano key objects | the forwarder reaches `0xC00BC7AC` (slot 3) and `0xC00BC8AC` (slot 2, key event) of key object `0x90C52F10` (instance 0 of four) | VERIFIED (run) |

Every note-on on channel 1 (`0x90`) calls `0xC0085630` once, `0xC00BC7AC` once (twice with the note-off) and `0xC00BC8AC` once. Channels 2..4 (`0x91..0x93`) reach only the sink: the global MIDI channel (`Edt_Sys_kMIDIGlobal` default 1) is the only one that the layers accept; the Ext channels default to 13..15.

## 3. Capture result: no DSP message follows a key

`python tools/arm_keydown.py ... --note N --velocity V` and the sweep of this session (`capture_note`: 30 ticks after the note-on, 30 after the note-off, settled state with the default program) for velocities 1, 32, 64, 100, 127 and notes 36, 60, 84: **0 key-induced DSP messages** on every chip. Three runs saw the 3-word idle burst of the firmware (PNO core 1 DM `0xB2F25`, `0xB2F26`, `0xB2FA1`, every 28 M instructions, [arm-organ-messages](arm-organ-messages.md) section 10), which is not caused by the key. The call counts per note are identical for all 15 runs: `midi_parser` 30, `midi_note_on_post` 1, `midi_note_off_post` 1, `piano_event_slot3` 2, `piano_key_event` 1, `piano_voice_frame` 0, `piano_voice_start` 0, `organ_key_down2` 0, `synth_voice_event` 0.

## 4. Why the piano key event stops: the sound object `[core + 4]`

`0xC00BC8AC(obj, ...)` begins with `r6 = [obj + 0xC]` (the core, `0x90BF5400` for instance 0) and `r0 = [r6 + 4]`; `cmp r0, #0; beq 0xC00BCE60` returns without a message. **`[core + 4]` is 0 for all four cores in the settled state**; it is written only with 0 (constructor `0xC00C12EC..0xC00C1420`, reset `0xC00AA9D4`) during the boot and the settle (write watch on `core + 4` of cores 0 and 1 over the whole boot: 5 stores, all value 0).

| Item | Result | Tag |
|---|---|---|
| What `[core + 4]` is | an object `P` of at least `0x6500` bytes with a vtable (`[P + 0]`, slots `+4 / +8 / +0xC` compared with function addresses in `0xC00B84CC`), a state word at `P + 0x64C0` (3 or 4: `0xC00B84CC` returns at once), per-key byte tables read by the key path (`[P + key + 0xF3]`, `+ 0x173`, `+ 0x3F3`, `+ 0x473`) and a flag byte at `P + 0x5F0E`. `0xC00B84CC` copies sections `0x8B7..0x909` to `0xEBE..0xF0F` of the same object (a loaded sound header, not a layer: the earlier test that wrote the layer pointer there only got past the first test) | EMPIRICAL (disassembly of `0xC00B84CC`, `0xC00BC8AC`) |
| Writers | the sound is bound by a loader that no id reaches: dispatching every `Edt_Pno*`, `Edt_PnoCom*`, `Edt_Com_k*` and `Edt_Sys_kMIDI*` id (6 values each, 130 ids, piano stores `0x9103FC50`, `0x91042670`, `0x91062120`, `0x91064B40`) never changes `[core + 4]` | VERIFIED (run) |
| Cause | ASSUMED: the sample set load (`Edt_Pno_kBank` / `kEntry` -> reload trigger `0x9403E`, preset position / status ids) is asynchronous and ends in the data downloads that `arm_boot.install_boot_stubs` and `arm_boot_full.install_ui_stubs` stub (`SPI_DOWNLOAD_BLOCK 0xC0240548`, `DSP_DOWNLOAD 0xC0241C58`, `SPI_TRANSFER 0xC0066E64` returning 0 bytes). What would confirm it: a trace of the first non-zero store into `core + 4` with the downloads fed from real data | open |

## 5. Organ and synth

With the section and layer enable ids set through the layer clients (`Edt_Com_kSectionEnableOrg_I` `0x9A`, `Edt_Com_kSectionEnableSyn_I` `0x9C`, `Edt_OrgCom_kLayerEnableOrg_I` `0xAE`, `Edt_SynCom_kLayerEnableSyn_I` `0xA1` = 1) a note-on on channels 1..3 still calls neither the organ second-family key-down `0xC00BF890` nor the synth voice engine event `0xC01EC690`: the layer loop `0xC00D1EBC..0xC00D1F00` tests a 64-bit mask `[client + 0x28]` against the event mask and only the piano client passes. The mask is built from the zone / enable state; why the organ and synth masks do not match is not decoded. **No organ or synth key message was captured.**

## 6. Reproduce

```bash
python tools/arm_keydown.py target/fw_unpacked/bank0_CODE.bin target/fw_unpacked/bank0_SRAM.bin \
    --cache target/keydown.snap --note 60 --velocity 100 --ticks 30
```

Prints whether the real hooks are registered (`True`), the ring `(size, head, tail)` after the ISR (`(4096, 3, 0)`) and after the run (`(4096, 3, 3)`), the function entry counts and the CSV rows (`phase,note,velocity,tick,chip,path,core,dm_word,value`; none for a key, see section 3). First run about 60 s (boot and settle), then 3 s from the cache.

## Implications

- The key path is no longer blocked by the harness: MIDI bytes now run the real parser, the real event queue and the real layer dispatch up to the piano key handler.
- The remaining blocker is data, not control flow: the sound objects (`[core + 4]` for the piano; the equivalent for synth and organ) exist only after the sample-set / program load, which needs the flash / SPI downloads the harness stubs.
- Any capture of velocity-dependent DSP messages needs a loaded sound object. The tools for the load trigger and the ring consumer are in place.

## Follow-ups

1. Find the first non-zero store into `core + 4` (write watch during a boot where `SPI_TRANSFER` and `SPI_DOWNLOAD_BLOCK` serve the sample header from the local `.ns4b` piano library, not committed) and decode the sound header layout of `P`.
2. Decode the 64-bit zone masks (`[client + 0x28]`, `0xC00D1EBC`) to see which program state routes a note to the organ and synth layers; then capture the organ `0xC00BF890` and synth `0xC01EC690` messages with `Rig.tick`.
3. Model the `0x60000000` panel / keybed device (hook 2 writes `0x60000010..0x60000024`) to see if the keybed scanner, not MIDI, feeds the same event ring.
4. Capture the message series over control-rate ticks once a note produces messages (`Rig.tick` returns `(tick, message)` pairs; `message_rows` writes the CSV).
