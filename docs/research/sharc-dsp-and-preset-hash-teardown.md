# OS v1.66 `ARM0` Preset Name Hash & SHARC `ADSP-21489` Parameter Curve Teardown

**Status:** Verified empirically against `Update Nord Stage 4 OS v1.66.exe` (`bank0_CODE.bin`, `bank1_CODE.bin`) and all `384` factory `.ns4p` programs in `Nord Stage 4 Factory Restore RevK v1.66.ns4b`.

---

## 1. `ARM0` Preset Sample Link Hash (`0x0a5a44`)

Disassembly of `bank0_CODE.bin` (loaded at `0xC0000000`) isolated the sound-bank ID resolver at file offset `0x0a5a44..0x0a5b1c` (VA `0xC00A5A44`).

### 1.1 Algorithm Specification
- **Variant:** 32-bit **MurmurHash2** with **big-endian 4-byte chunk reads** (`REV` / `__builtin_bswap32` at `0x0a5a70..0x0a5a98`) and **little-endian byte-order tail**:
  - `M = 0x5bd1e995`, `R = 24`, initial seed `h = stem.len() as u32`.
  - For each 4-byte aligned block: `k = u32::from_be_bytes(chunk)`, `k *= M`, `k ^= k >> 24`, `k *= M`, `h *= M`, `h ^= k`.
  - Tail bytes `0..=2`: `if rem == 3 { h ^= rem[2] << 16 }`, `if rem >= 2 { h ^= rem[1] << 8 }`, `if rem >= 1 { h ^= rem[0]; h *= M }`.
  - Finalizer: `h ^= h >> 13; h *= M; h ^= h >> 15`.
- **Zero guard (`0 -> 1`):** Callers (`0x041390`, `0x06f0d4`, `0x0b924c`) execute `cmp r0, #1; movcc r0, #1`, reserving `0` as "unset / no sample assigned".
- **Case-insensitive fallback:** Registry lookup callers (`0x041440`, `0x066370`, `0x06c03c`) check the raw UTF-8/ASCII filename stem first and fall back to ASCII `toupper` (`0x259ac0`).

### 1.2 Input String & `.ns4p` Bitfield Alignment Fixes
- **Hashed input:** Exact filename stem without extension (e.g. `"Astoria Grand Lrg 6.1"` &rarr; `0x444ebb81`, `"Ac Guitar Finger_ST Mart 4.1"` &rarr; `0xa281b28c`).
- **Piano Layer `0x0B` (`unpack_piano_layer_block` / `pack_piano_layer_block`):**
  - `base + 28 .. base + 60`: 32-bit `name_hash` (earlier draft read from `base + 30`).
  - `base + 60 .. base + 62`: 2-bit `variation`.
- **Synth Layer `0x32` (`unpack_synth_layer` / `pack_synth_layer`):**
  - `layer_base + 26 .. layer_base + 58`: 32-bit `sample_id` (earlier draft read from `layer_base + 32`, losing the top 6 bits).
- **Standalone `.npno` stem reconstruction:** Verified `38/38` factory `.npno` files match from internal header fields `0x48..0x68` (`Name#Sub10Size3`), `0x68..0x88`, and `0x46` (`vmin`).
- **Factory verification (`384` `.ns4p` files):** `384/384` Piano A hits, `384/384` Piano B hits (`0` misses), and `0` Synth `Sample`-mode misses (`241`, `234`, `173` hits across Synth A/B/C).

---

## 2. SHARC (`ADSP-21489`) Partitions & `ARM0` DSP Parameter Curves

### 2.1 SHARC Partition Architecture (`bank1_CODE.bin`)
Inspection of `bank1_CODE.bin` (`0x08cb7c..0x09fe8`) confirms the multi-SHARC SPI flash/DMA layout targeting `ADSP-21489` L1/L2 regions (`0x0009xxxx..0x000cxxxx`):
- `DSPFxCore1` / `DSPFxCore2` (`L1 bl.0..3`, `L2`)
- `DSPOrgCore1` / `DSPOrgCore2` (`L1`, `L2`)
- `DSPSynCore1` / `DSPSynCore2` (`L1`, `L2`)
- `DSPSmp` (`block 0..3`), `DSPPiano` (`block 0..3`)
- Inter-DSP TDM SPORT routing slots (`Fx to Org`, `Org to Fx2Ap`, etc.).

While the raw SHARC `.ldr` machine-code blocks live on the mainboard SPI flash outside the `ARM0` OS update container, **`ARM0` (`bank0_CODE.bin`) stores all calibrated 128-entry UI/DSP translation tables and filter coefficients** sent over `DSP Comm` (`0x06cc40`).

### 2.2 Extracted 128-Step Parameter Tables (`0x53abe4..0x53b5e4` in `bank0_CODE.bin`)

| Pointer Table Offset | Parameter | Hardware Range (`idx 0..127`) | Previous `nord-core` Approx |
|---|---|---|---|
| `0x53abe4` | Synth Env Attack (`ENV_ATTACK_MS_TABLE`) | `0.5 ms .. 45.0 s` | `1.0 ms .. 5.0 s` |
| `0x53ade4` | Synth Env Decay (`ENV_DECAY_MS_TABLE`) | `3.0 ms .. 43.0 s` (`127 = Hold/Inf`) | `10.0 ms .. 10.0 s` |
| `0x53afe4` | Synth Env Release (`ENV_RELEASE_MS_TABLE`) | `3.0 ms .. 45.0 s` | `2.0 ms .. 10.0 s` |
| `0x53b1e4` | Synth Filter Cutoff (`FILTER_CUTOFF_HZ_TABLE`) | `14 Hz .. 32 kHz` (`idx 60 = 440 Hz`) | `80 Hz * 200^norm` |
| `0x53b3e4` | Synth LFO Rate (`LFO_RATE_HZ_TABLE`) | `0.03 Hz .. 523 Hz` (`idx 60 = 3.0 Hz`) | Linear `0.1 .. 20.0 Hz` |

### 2.3 Additional Verified Constants in `bank0_CODE.bin`
- **Rotary Crossover Frequency:** `f32(800.0)` Hz confirmed in literal pools at `0x0db130` and `0x0e0098` (`ROTARY_CROSSOVER_HZ = 800.0`).
- **Rotary Mic / Bass-Horn Ratios (`0x2c20c4..0x2c2df4`):** `Ambient` / `Close` placement and discrete balance ratios `70/30`, `60/40`, `50/50`, `40/60`, `30/70` (`ROTARY_BASS_HORN_BALANCES`).
- **Biquad / Resonance Tables (`0x2f730c..0x2f970c`, ref `0x1bf094`):** 18×128 `f32` biquad coefficient arrays plus `0.1..10.0` log Q scale at `0x2f970c`.
- **Stage Gain & Clock Division Tables:** `-12 dB .. +6 dB` 1 dB gain array at `0x2ae5b4`; clock divisions (`1/2`, `3/8`, `1/3`, ...) at `0x2b9a00`; polyphase sinc FIR coefficients at `0x50c428..0x516cf4`.
