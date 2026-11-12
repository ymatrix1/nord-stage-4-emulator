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

## ARM serializer search (OQ7, part 2: negative result)

Goal: find the table or code that gives the firmware's own bit positions and value scales. Helper: `tools/fw_xref.py`.

| Attempt | Result |
|---|---|
| Descriptor tables for `Buffer_kSynLay` / `Buffer_kSynSnd` (words pointing at the `Edt_Syn_k*` strings, `0x1aa4a4..0x1aa6c0` in `bank0_CODE.bin`) | Alternating name pointers and small ids (`0x11a` LFODst, `0x11b` LFOWave, `0x11f..0x123` osc env, `0x131` amp velocity, `0x133..0x13b` filter ...), also the ids used by the UI/morph tables near `0x190300` and `0xfc8bc`/`0xfdf28`. They are dense parameter ids, not bit offsets or widths. |
| Tables of block-relative or absolute bit offsets (`56, 63, 69, 71, 78, 85, 92...` as u8/u16/u32 with strides up to 6 words, plus `+248`, `+1472`, `+3000`) | No match |
| Tables of widths (`7,6,2,7,7,7`, `7,7,7,1,1,7,6`, `4,4,1,4,1,3,2,2,2,7`, strides 1..12 words) | No match for sequences longer than 3 (3-element sequences match noise everywhere) |
| ARM immediates (`mov`/`movw`/`add`/`cmp`) equal to our absolute positions, clustered within 1.5 KB | 357 isolated hits, 0 clusters of 8 or more distinct positions |

Conclusion: the layout is not stored as a plain offset/width table and is not hard-coded as clustered immediates. It is probably derived from a generic bitstream reader/writer driven by per-field type descriptors, which needs a call-graph trace of the buffer reader (look for the function that takes `Buffer_k*` and a field index, `Capstone` is available). Still unconfirmed by firmware code: absolute offsets and all value scales.

Next steps (OQ7): trace callers of the `Buffer_kSynSnd` descriptor at `0x1aa59c`, identify the field-descriptor struct (the interleaved ids suggest `{name, id, morph ids}` plus a separate type/width map), then read the glide/vibrato/osc-env scale routines that consume the raw 7-bit values.

## Firmware value-scale tables (OQ7, part 3)

Scanned `bank0_CODE.bin` for monotonic `f32` arrays of 16..1000 entries, then compared them with the repo tables.

| Scale | Firmware | Result |
|---|---|---|
| **LFO pitch depth table**: LFO amount (7b) to semitones | `@0x2ca2c0`, 128 `f32`, `0.0 .. 48.0`, monotonic | **VERIFIED** and now used by the engine (`synth_tables.rs::LFO_PITCH_SEMITONES`, `python tools/ns4_fw_tables.py target/fw_unpacked/bank0_CODE.bin`: max err 3.8e-7). ns4decode's `48*(i/127)^2.5` fit is off by up to 0.41 semitone near the top, so the table replaces both it and our old fixed `+/-5%` |
| Env attack / decay / release, filter cutoff, LFO rate (`ENV_*`, `FILTER_CUTOFF_HZ_TABLE`, `LFO_RATE_HZ_TABLE`) | no `f32` table within 5% of any of them | not stored as float tables (computed or stored as integers); remain as before (see their own provenance) |
| Glide, vibrato rate/amount/delay, osc env amount | no table (searched linear/short `f32` runs of 16..100) | computed analytically in code, still `ASSUMED` (`2 + 6*raw/127` Hz etc. are ns4decode formulas) |

Other 128/129-entry curves in `0x2f7000..0x2fbf00` and `0x453c44..` (`0.0093 .. 3.76`, `0.1 .. 10`, ...) belong to DSP filter/Q/compressor curves studied in `sharc-dsp-and-preset-hash-teardown.md`, not to the program bitstream.

## ARM display-string tables (OQ7, part 4)

The five `0x53abe4..0x53b3e4` "tables" in `bank0_CODE.bin` are arrays of 128 pointers to 8-byte **display strings** (`"0.5 ms"`, `"748 ms"`, `"43 s"`/`"Inf"`, `"14 Hz"`..`"32 kHz"`, `"0.03 Hz"`..`"523 Hz"`), not DSP values. `python tools/ns4_fw_tables.py target/fw_unpacked/bank0_CODE.bin` compares them with `ENV_ATTACK/DECAY/RELEASE_MS_TABLE`, `FILTER_CUTOFF_HZ_TABLE` and `LFO_RATE_HZ_TABLE`: 128/128 entries match for every table within the rounding of the displayed digits, so those five inherited tables are now `VERIFIED` as *displayed* values. The scales that act on the sound (glide rate, unison detune, osc-env pitch depth, velocity curves, filter-env depth) are most likely applied inside the SHARC DSP code. **Not proven that the update does not contain it**: see the next section.

## Is the DSP code in the update file? (open, not proven either way)

Checked 2026-11-12:

| Check | Result |
|---|---|
| Installer resources (`7z l "Update Nord Stage 4 OS v1.66.exe"`) | `BINA` (4,037,080 B, the firmware we unpack = `os.cab` in the macOS `.dmg`) and `BINE` (587,736 B): a 32-bit NSIS installer signed by Clavia (USB driver setup), not DSP code. No other firmware resource |
| `DSP*Core* L1 bl.N` / `DSPSyn L2` / `DSPSmp block N` names (`bank1_CODE.bin @0x8cb7c`, table `@0x9ee0..0x9fe0`) | next to the `Micron MT25QL128A` string and `DSP Test` / `Possible DSP err`; the table pairs names with SHARC-style address ranges (`0x90000..0xC0000`). They look like **memory-block test / SPI-flash region descriptors**, not code images. The earlier teardown reading "loader table naming images" is not supported by the table contents |
| Decompressed `bank0_CODE.bin` (5.5 MB) and `bank1_CODE.bin` (0.8 MB), 64 KB windows | ARM code (`0..0x278000`, entropy 6.1-6.4, byte lag-4 repeats), float / pointer tables, a 1.1 MB high-entropy block at `0x300000..0x420000` with 16-bit-sample-like correlation (lag-2 0.05-0.27, not 48-bit instructions); no region with SHARC 6-byte periodicity; no zlib/gzip/bzip2/ELF streams |
| OS history | Nord OS updates changed Synth behaviour (glide fixes, Synth preset format 2.06), which is easier to explain if DSP code can be updated too |

Reading: no DSP code recognised in what we unpack, but the container may still carry it in a form we do not recognise (the second compressed image, a SHARC `.ldr` stream inside the data blocks, or a custom packing). Until the full LZ decompressor is verified end to end and the data blocks are classified, "firmware cannot give these scales" stays a hypothesis.

Lead: the `0x300000..0x420000` block is embedded audio, see the next section; it is not yet mapped to the 46 WAVE waveforms.

## Embedded audio block (`bank0_CODE.bin @0x300000`)

`python tools/fw_audio_scan.py scan target/fw_unpacked/bank0_CODE.bin` finds three int16 audio-like regions: `0x2b3000..0x2b6000` (12 KiB), **`0x2fe000..0x441000` (1.29 MiB)** and `0x529000..0x53a000` (68 KiB).

| Fact | Evidence |
|---|---|
| Little-endian signed 16-bit PCM | adjacent-sample correlation `0.85` (LE) vs `0.04` (BE) over `0x300000..0x420000`; many 8 KiB windows at `0.95..0.99` |
| Four interleaved channels | rows of 4 samples; each phase is smooth on its own (e.g. rows start `-1332 -1471 -1471 -359 / -1426 -1434 -1434 -476 ...`); phases correlate `0.87..0.97` with each other; `phase1 == phase2` in about 19% of rows near the start |
| Periodic, slowly evolving waveform | steep edge repeats every about 519-524 rows (autocorrelation peak at 524 rows: `f0 = 91.6 Hz` if the rate were 48 kHz); the shape changes cycle to cycle, so these are recordings or analysis data, not exact single-cycle wavetables |
| Peaks per 1024 samples | spiky, roughly regular segments in the first third, then a flatter `~17000` plateau: several different items concatenated |
| Directory table | none points into `0x300000..0x420000`. The only table-like pointer cluster (135 `{pointer, length}` pairs at `0x2fd5c0`, lengths `8`, `0x200`, `0xcf5`, `0x6e7`, `0x4381`) targets the tail `0x426d1c..0x437004`, which is a different, smoother data set |

Not established: what the data is (WAVE-type oscillator tables, sample-rate-converter / reverb impulse data, or DSP sample ROM), the sample rate, the channel meaning, and any mapping to the 46 waves. Exports for listening: `python tools/fw_audio_scan.py export <image> <dir> --offset 0x300000 --length 0x120000 --channels 4`. Keep exported audio out of git (Nord content).

### Listening and spectrum result (2026-11-12)

- **Listening test (user, 4 exported channels at 48 kHz):** not tonal; described as a slow "wobble" noise, not grain noise.
- **Layout:** the first 9 records repeat at exactly 19,052 int16 samples (38,104 bytes): a full-scale spike sits at sample 14,691 of the block and recurs every 19,052 samples; only 12 samples in the whole block reach `|x| >= 32000`.
- **Per-channel spectral slope** (16384-row chunks, 48 kHz assumed): `0x300000` about `-22 dB/decade` with strong narrow peaks (tonality 38-40 dB); `0x340000` `-14`; `0x380000..0x420000` about `-9 dB/decade` (weak peaks, tonality 13-21 dB). White noise is about 0, pink about -10, red/brown about -20 dB/decade.
- **Reading:** the front is red-noise-like with a periodic component (the slow wobble), the rest is pink-noise-like. That matches noise sources rather than the instrument-like WAVE waves (`Bell`, `Flute`, `EPiano` ...). Hypotheses: the MISC `Pink Noise` / `Red Noise` tables and similar random/modulation sources, or noise-shaped reverb/diffusion data. Unproven.
- **Consequence:** this block does not replace our synthetic wavetable bank. The 46 WAVE waveforms are still not located; the lead is demoted.


Next: segment by waveform edges per channel and compare each segment's harmonic content with the manual's wave descriptions; look for the loader that DMA-copies `0x300000..` to the DSP (a `{src, length}` word pair in ARM code).

### Other audio-like regions and float arrays (2026-11-12): no WAVE waves found

| Where | What it is | WAVE waves? |
|---|---|---|
| `bank0_CODE.bin @0x2b3000..0x2b6000` (12 KiB int16) | repeated exponential "charge" ramps rising `0 -> 32767` (about 520 samples each) plus one linear ramp (about 1000); looks like curve / shape lookup tables | no |
| `bank0_CODE.bin @0x529000..0x53a000` (68 KiB) | 16-bit values that jump between 0 and a few fixed levels (`~+30000`, `-22000`, `-16000`), odd/even interleave; bitmap- or font-like data, not audio | no |
| `0x300000..0x420000` | noise-like tables, see above | no |
| 68 smooth `f32` runs of 128+ values in `0x2f7000..0x516000` (e.g. `0x489bc0`, `0x48e4b4`, `0x4a6da4`, `0x4ab72c` x3 identical copies, `0x505028`, `0x50dc30`, `0x514284`) | plotted: S-curve and pulse-shaped coefficient / interpolation curves in `[-1, 1]` (some copies per DSP core), consistent with the filter coefficient and polyphase FIR tables already documented | no |
| `bank1_CODE.bin` | only 3 short float runs (under 260 values) and no int16 audio region | no |

Conclusion: the 46 WAVE waveforms (`Bells/Tines ... Keys`) are not in the ARM `CODE` images we unpack, in int16 or float32 form. They are most likely stored with the DSP data (SPI flash, loaded to the SHARC) or inside the unrecognised part of the container, so the wavetable bank stays ASSUMED. Remaining unchecked formats: int8 / 24-bit / compressed tables, and additive (harmonic amplitude) tables stored as small integers.

### Factory `.nsmp4` samples are not the WAVE waves (2026-11-12)

The factory backup `Nord Stage 4 Factory Restore RevK v1.66.ns4b` has 195 `.nsmp4` files under `Samp Lib/<category>/`: `Accordion_Harm`, `Bass`, `Brass Ensemble/Solo`, `Choir`, `Guitar_Plucked`, `Mellotron`, `Orchestral`, `Organ`, `Percussion Tuned`, `Strings Analog/Ensemble/Solo`, `Wind Ensemble/Solo`. They are the multisampled **Sample Library** used by the `Samples` oscillator mode (layer `+8 is_osc_mode = 0`, `sample_id` = name hash), 87 KB to tens of MB each (smallest `Kalimba_KG 4.1` at 86,960 B, typical 1-10 MB). Their names do not match the 46 WAVE-type items (`Bar 1`, `Bar 2`, `Tines`, `2nd Spectra`, `Saw NoFund`, `Ice 1`, `Triplets`, `Squabble`, `EPiano 1`, `DX 1`, `Clavinet 1` ...): only generic words coincide (`Bells`, `Marimba`, `Tubular Bells`, `Flute`, `Clarinet`, `Principal`). No file is small enough to be a single-cycle or short wave. The WAVE waves therefore are in neither the ARM `CODE` images nor the factory backup; they remain with the DSP-side data (see the DSP-code section) and our wavetable bank stays ASSUMED.

## Container accounting and block classification (DSP code search, 2026-11-12)

Result: **no DSP code in the update container; it almost certainly lives in the mainboard flash and is not shipped by OS updates.** Evidence, from our docs plus new checks:

1. **Every byte of the container is accounted for** (`docs/research/clavia-lz-reconstruction.md`, LZO1X verified 100% on both banks). Image 0 (`3,575,686 B`) = `0x2D4` header + `64,640` SRAM + `3,509,573` compressed `CODE` + `1` alignment + `748` ARM0 stub; image 1 (`461,362 B`) = `0x2D4` + `49,496` + `410,321` + `1` + `820`. Nothing is left over, so DSP code could only sit inside the decompressed `CODE`. The installer's only other resource is the NSIS USB driver setup.
2. **Block classification** (`python tools/fw_classify.py target/fw_unpacked/bank0_CODE.bin`, 4 KiB blocks): ARM code `2528 KiB` (`0..0x279000`), int16 audio-like `1204 KiB` (noise tables, ramp curves, bitmap-like data, see above), float tables `388 KiB`, pointer tables `116 KiB`, text `60 KiB`, zero `16 KiB`, other `1060 KiB`. The `other` blocks (`0x447000..0x526000`, `0x2a1000`, `0x2d8000`) are float/int coefficient tables, structured int tables and string tables (spot-checked: `0x3ad6b228..` increasing floats, `0x2d8000` strings). No region has the structure of SHARC 48-bit code.
3. **DSP boot path in ARM code:** `"DSP Boot Error"` (`@0x529688`, literal pool `0x24bba8`) sits in a routine that configures the memory-mapped DSP host interface (`0x62000010`, `0x6200400c/18/1c`) and waits for the DSP; there is no loop copying a code image. The names `DSP*Core* L1 bl.N` are memory-test descriptors (see the table section above).
4. **Bank 1 is the bootloader / flash tool:** strings `Flash Tool`, `SPI Flash`, `Piano Flash`, `Sample Flash`, `Checksum Update`, `Update Image Log`, `Flash Erase`, `Micron MT25QL128A`. It programs flash from data the host sends; the OS update file only carries the two ARM images, so the SHARC images stay in flash.

Consequences:

- Glide, unison, osc-envelope pitch depth and velocity curves run inside the DSP code we cannot read, unless the ARM computes the DSP parameter from the 7-bit value before sending it. Forum reports that glide changed between OS versions (1.06 vs later) suggest at least the glide **knob-to-DSP mapping is in the ARM**.
- Next lead: find the ARM routine that sends `Edt_Syn_kGlideRate` (descriptor `0x1aa4a4`) to the DSP and read its math (soft-float constants / table). The other session already traced one such sender (`0x1be108`, biquad indices via double math).

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
- Hardware route for the remaining ASSUMED scales: `docs/research/synth-hardware-sweep-plan.md` (analysis tool `tools/sweep_analyze.py`).
