# Synth `0x32` / `0x2F` bit map: ns4decode cross-check

**Checked:** 2026-11-11
**Sources:** ns4decode 2.0 (`.reference/ns4decode_pkg/`, MIT, Randy 2024; `ns4maps.py`, `ns4names.py`, `ns4decode.py`); its regression package (`regressionTest.ns4p` + `regressionTest_synth.csv`, https://ns4decode.netlify.app/downloads/regressionTest.zip); `.reference/Nord Stage 4 Factory Restore RevK v1.66.ns4b` (384 `.ns4y`); firmware strings in `target/fw_unpacked/bank0_CODE.bin` (`ns4 inspect --unpack`); NS4 User Manual v1.6X pp. 28-37.
**Question:** Where do LFO shape/target, Mono/Legato/priority, Glide, Unison, Octave, pitch tune, oscillator envelope, amp/filter velocity and oscillator type/category/wave live in the Synth `0x32` Layer and `0x2F` Sound blocks, and how reliable is that map?

## Method

1. Converted every ns4decode synth id (`byte-bit`, 1-based) to a block-relative bit: `bit = (byte-1)*8 + (bit-1) - 3000 - base`, with `3000 = 375*8` (synth block start inside an `.ns4p`), `base = 248` (`0x32`) or `1472` (`0x2F`). Sanity: `layer on/off 377-5` lands on synth bit 12 (`enable_i[A]`), `samples/analog 408-1` on `0x32 +8`, filter env attack `599-7` on `0x2F +318` (found independently by `tools/ns4_bitscan.py`).
2. Decoded the regression `.ns4p` with `tools/ns4_synth_fields.py` and compared every comparable field with ns4decode's CSV:

```bash
python tools/ns4_crosscheck.py --program regressionTest.ns4p --csv regressionTest_synth.csv
```

3. Scanned all 1152 factory layers for illegal values:

```bash
python tools/ns4_crosscheck.py --corpus ".reference/Nord Stage 4 Factory Restore RevK v1.66.ns4b"
```

4. Compared firmware strings with ns4decode's name lists:

```bash
strings -n 2 -t x target/fw_unpacked/bank0_CODE.bin | awk 'length($1)==6 && $1>="2d7d44" && $1<="2d8400"'
```

## Findings

| Check | Result |
|---|---|
| Regression program vs ns4decode CSV | 75/75 fields equal (octave, mono, legato, priority, glide, unison, amp/filter/osc ADR times, amp velocity, LFO target/shape/clk, env-to-pitch, osc/filter velocity, filter on, pitch fine/coarse) |
| Factory corpus | 1152 layers, 0 violations: octave only `-2..+2`, priority only `0/2`, unison `0..3`, LFO target `0..3`, shape `0,1,3,4`, WAVE category always `<= 4`, mono and legato never both set, pitch coarse `-24..+24` (manual range) |
| Independent bit scan | `tools/ns4_bitscan.py` placed filter ADR `+318`, osc ADR `+71`, amp velocity `+216`, unison `0x32 +306` from data alone, equal to the ns4decode map |
| Firmware strings | `bank0_CODE.bin @0x2d7d44` holds the wave names in ns4decode's per-category order (`Pure: Sine, Triangle, Saw, Square, Pulse 33, Pulse 10, White Noise`; `Bells/Tines: Bell, Bar 1, Bar 2, Tines, Marimba, Tubular Bells`; `Acoustic`, `Digital 2nd..8th Spectra, Saw Random, Saw NoFund, Ice 1, Ice 2, Triplets`, `Organ`, `Keys: EPiano 1..Clavinet 3`, plus `Harmonic %c` (FM-H) and `Inharmonic %c` (FM-I)). `Voice Mode (Poly, Legato)`, `Voice Priority`, `Glide Rate`, `LFO Destination`, `LFO Master Clock`, `Oscillator Env Amount` exist as parameter names (`0x2cd540..0x2cd6b0`) |

## Firmware order and width check (OQ7, part 1)

The firmware keeps one field-name table per program buffer (`Buffer_kSynLay`, `Buffer_kSynSnd`; also `kOrg`, `kPno`, `kFx`...) in bitstream order. `tools/ns4_fw_order_check.py` compares our offsets with it:

```bash
python tools/ns4_fw_order_check.py target/fw_unpacked/bank0_CODE.bin
# Buffer_kSynLay: 16 fields checked, 0 problems
# Buffer_kSynSnd: 34 fields checked, 0 problems
```

- Order: every field sorted by our offset matches the firmware name order (`OscCtrl, TuneFine, TuneCoarse, OscUnison_obsolete, OscEnvAtk..Rls, OscEnvAmt, OscEnvDst, OscEnvVel, SmpDynamic, LFODst, LFOWave, LFOClkEnable, LFORateClkDiv, LFOAmt, AmpEnv*, Flt*, VibRate, VibAmt, SmpTimbre`; layer: `KbZone, OctaveShift, PStick*, Sustain, VibMode, VibDly_obsolete, VoiceMode, VoicePrio, GlideRate, ..., ArpRun, ArpKbdSync, ..., OscUnison, VibDly`).
- Contiguity: from `+25` (`OscCtrl`) to `+354` (`SmpTimbre`) every field starts exactly where the previous one ends (morph = 31 bits), so the widths of all 33 fields between the independent anchors `+25`, `+133`, `+195`, `+318` are forced. The `0x32` run `+58..+88` is gapless too.
- Firmware shows `VoiceMode` as one enum (Poly / Mono / Legato): ns4decode's separate `legato` (+77) and `mono` (+78) bits are its two bits; they are never both set in the corpus.
- Firmware has two `Unison` fields: `OscUnison_obsolete` (`0x2F +69`, 2 bits, still in the layout) and the live `OscUnison` (`0x32 +306`).
- Not covered: absolute positions (the table has names only), the `OscBank/OscCat/OscWave` split (the firmware widths are not in the strings; the type/category/wave decomposition is supported by the corpus invariant "WAVE category <= 4"), and every value scale.

## Status

- Wave **names and within-category order**: confirmed by firmware strings (command above, no numeric indices in strings).
- Field **order and widths** (`+25..+354`, layer `+58..+88`): `VERIFIED` against the firmware name tables by `tools/ns4_fw_order_check.py`.
- Absolute **offsets / category indices**: `EMPIRICAL` (ns4decode, hand-checked by its author against the instrument display; corroborated by the corpus and bitscan). The firmware strings carry names only, not bit positions.
- Value scales ASSUMED: glide seconds per octave, vibrato depth/rate mapping to DSP, osc envelope depth, unison cents, LFO pitch depth, the 64-table wavetable bank content.

## Implications

- `cbin/synth_preset.rs` decodes and packs every field above; unmapped bits are preserved from the raw payload so `.ns4y`/`.ns4p` round trips stay lossless.
- OQ4 (wavetable index offsets) is answered: the oscillator is `type (2b @+10) / category (3b @+15) / wave (6b @+19)`, not the former `bank/cat/wave10` guess.
- The old `ns4_inspect` filter-env amount (`raw-64`) was bipolar by mistake; ns4decode and the manual treat it as unipolar `0..10` and the decoder now does too.

## Follow-ups

- Locate the `ARM0` code that serialises `Buffer_kSynSnd` and its glide / vibrato / osc-env scale tables (OQ7). Capstone is available; the name tables sit at `0x2d70b8` and `0x2d751c`, referenced from the buffer descriptor tables near `0x190300`.
- Record hardware sweeps for unison cents and glide time.
