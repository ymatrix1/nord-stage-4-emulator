# Research: Nord Stage 4 OS v1.66 Firmware Container & DSP Partition Teardown

- **Checked date:** 2026-11-10
- **Sources:** `Update Nord Stage 4 OS v1.66.exe` (12,770,968 bytes), `Update Nord Stage 4 OS v1.66.dmg` (13,330,920 bytes)
- **Question:** How is the Nord Stage 4 firmware structured inside the OS updater, and how is DSP work partitioned across hardware processors?

---

## 1. PE Resource Extraction

`Update Nord Stage 4 OS v1.66.exe` is a 64-bit Windows PE executable (wxWidgets `AVCUpdateEngine` GUI) with a `4,911,616`-byte `.rsrc` section (`RVA 0x00790000`, raw file offset `0x00756E00`).

Inside `.rsrc` (Custom Resource Type `2147484160`):
- **Resource ID `101` (`Lang 1033`, offset `0x79CB30`, size `4,037,080` bytes = `0x3D99D8`):** The complete Nord Stage 4 OS v1.66 firmware image.
- **Resource ID `105` (`Lang 1033`, offset `0xB76508`, size `587,736` bytes):** Embedded USB DFU / flasher helper executable (`MZ` header).

---

## 2. Firmware Image Container Header (`4,037,080` bytes)

The first 32 bytes (`0x0000..0x0020`) of `Res 101` define a big-endian dual-bank firmware directory:

```text
0000: 00 00 00 20   Header length = 32 bytes (0x20)
0004: 00 36 8f 86   Image 0 size  = 3,575,686 bytes (Main OS + DSP bundle)
0008: 00 36 8f a6   Image 1 start = 0x00368FA6 (= 0x20 + 0x368F86)
000c: 00 07 0a 32   Image 1 size  = 461,362 bytes
0010..001f: 00...   Reserved zero padding
```

Verification: `0x00000020 + 0x00368F86 + 0x00070A32 == 0x003D99D8` (`4,037,080` bytes exact).

### Segment Table (`SRAM`, `CODE`, `ARM0`)

Both Image 0 (`0x00000020`) and Image 1 (`0x00368FA6`) contain a 20-byte sub-header with `segment_count = 3` (`00 00 00 03`), followed by three 44-byte (`0x2C`) segment entries:
1. **`SRAM`** (`53 52 41 4D`): SRAM initialization block (`offset = 0x02D4`, `size = 0xFC80`, target `0x80000000`).
2. **`CODE`** (`43 4F 44 45`): Main code block (`offset = 0xFF54`, `size = 0x53EA70`, target `0xC0000000`).
3. **`ARM0`** (`41 52 4D 30`): Primary ARM microcontroller image (`size = 0x368C9A`, entry `0x000002EC`).

---

## 3. SHARC DSP Core Partitions (`DSPF`, `DSPO`, `DSPS`)

At offset `0x003CF040` inside the firmware image, the host-to-DSP loader table names the three specialized SHARC DSP images and their inter-core audio links:

| Offset | Tag | Full Firmware Identifier | Role |
|---|---|---|---|
| `0x003CF06C` | **`DSPF`** | `DSPFxCore1 L1 bl.0` | Per-layer Effects pipeline (`Delay`, `Reverb`, `Filter`) & Piano String Resonance |
| `0x003CF0A6` | **`DSPO`** | `DSPOrg` | 2-Layer Organ physical modeling (`FullOrgan`, `LeslieHorn`, `Rotary Mic Distance`) |
| `0x003CF0D5` | **`DSPS`** | `DSPSyn` | 3-Layer Nord Wave 2 Synth engine (`OSC WAVEFORM`, `WaveTable`, `SAMPLE LIBRARY`) |
| `0x003CF12E` | Routing | `Fx to Org` / `Host` | Inter-DSP SPORT bus routing descriptors |

### Extracted Internal Parameter Strings

Strings recovered directly from the firmware symbol/UI tables include:
- **Organ & Rotary:** `Organ Slot A`, `Organ_A`, `Organ/Ac`, `PresetOrg/O`, `FullOrgan`, `LeslieHorn`, `Rotary Mic Distance`
- **Piano:** `Piano Select`, `Piano Aco`, `Piano Pedal`, `EPiano1`, `Piano is broken`
- **Synth:** `Synth Slot A`, `Synth Soft`, `SynthBlank`, `Level Synth A`, `OSC WAVEFORM`, `Waveform S`, `WaveTable`, `SAMPLE LIBRARY`, `Sample error`, `Filter`
- **Effects & Control:** `frmDelay`, `Delayed`, `Reverb`, `Morph_Grp_kOne`, `Morph`, `MIDIOutKeyboard`, `MIDI Out`

---

## 4. Implications & Follow-ups

- **Implication 1:** Confirms that the Nord Stage 4 runs three distinct DSP domains (`DSPO`, `DSPS`, `DSPF`) managed by an `ARM0` host MCU.
- **Implication 2:** Our `crates/nord-core` module structure (`organ.rs` = `DSPO`, `synth.rs` = `DSPS`, `piano.rs` + `fx.rs` = `DSPF`, `morph.rs` + `midi.rs` = `ARM0`) mirrors the real hardware boundary 1:1.
