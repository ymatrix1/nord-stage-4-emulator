# Research: factory program coverage and render health

- **Checked:** 2026-11-14
- **Sources (local, never committed):** the factory restore backup `Nord Stage 4 Factory Restore RevK v1.66.ns4b` (384 `.ns4p` programs, its `.npno` and `.nsmp4` entries), `tools/program_sweep.py` driving `crates/nord-tools/src/bin/program_sweep.rs` (release build), the classification of paths from `docs/specs/*.md` and `docs/research/*.md` cited per row.
- **Question:** how much of the factory program space does the emulator cover, which engine paths do the 384 programs rely on, and which of them are firmware-verified, assumed or not modelled; does any program panic, go non-finite, fall silent or clip.

This is a coverage and risk map, not an audio-quality claim. All numbers are derived from the decoded programs and from output statistics; no audio, firmware or sample content is stored.

## 1. Method

- Layer used = bit set in the scene I or scene II mask the program stores (section enable AND layer enable, `ProgramRouting::exact_masks`).
- Render: `96000` frames at 48 kHz, notes 48, 55, 64, 72 (C3, G3, E4, C5) velocity 100, released at frame 72000, one engine per render, release build. The same note set drives every program, so a layer whose key zone excludes all four notes counts as silent here.
- `default` = engine defaults. `firmware` = `--firmware-all --firmware-routing` (firmware tone source, filter, strip chain, FX ports, piano level, program routing).
- Output health: NaN/Inf counted, silent = peak < 1e-4, clipped = |x| >= 1.0; clipping is reported at >= 0.1% of samples, level jump at >= 6 dB RMS difference.
- Status per path: `verified` = a firmware-verified algorithm runs; `assumed` = engine approximation or legacy model; `unmodelled` = pass-through, placeholder or decoded and ignored. The classification is a reading of the cited specs, not a measurement.

## 2. Engine usage

| Engine mix | Programs |
|---|---|
| piano+synth | 170 (44%) |
| synth | 95 (25%) |
| piano | 77 (20%) |
| organ+piano+synth | 14 (4%) |
| organ | 12 (3%) |
| organ+synth | 10 (3%) |
| organ+piano | 6 (2%) |

| Layers sounding | Programs |
|---|---|
| 1 | 100 |
| 2 | 111 |
| 3 | 92 |
| 4 | 61 |
| 5 | 18 |
| 6 | 1 |
| 7 | 1 |

Scene II used by 213 programs, split enabled in 48, non-default key zone code on a sounding layer in 172.

| Key zone code (sounding layers) | Programs |
|---|---|
| 0 | 39 |
| 1 | 4 |
| 10 | 3 |
| 2 | 2 |
| 3 | 1 |
| 4 | 9 |
| 5 | 256 |
| 6 | 36 |
| 9 | 119 |

## 3. Organ

| Path | Programs | Occurrences |
|---|---|---|
| `organ.model=B3` | 26 | 35 |
| `organ.model=B3 Bass` | 3 | 3 |
| `organ.model=Farfisa` | 2 | 3 |
| `organ.model=Pipe1` | 3 | 4 |
| `organ.model=Pipe2` | 4 | 5 |
| `organ.model=Vox` | 7 | 8 |
| `organ.rotary_speed=Fast` | 1 | 2 |
| `organ.rotary_speed=Slow` | 16 | 25 |
| `organ.rotary_speed=Stop` | 5 | 6 |
| `organ.vibrato_type=0` | 1 | 1 |
| `organ.vibrato_type=1` | 3 | 4 |
| `organ.vibrato_type=2` | 2 | 2 |
| `organ.vibrato_type=4` | 3 | 3 |
| `organ.vibrato_type=5` | 12 | 15 |
| `organ.percussion` (enabled) | 6 | 8 |
| `organ.rotary` (enabled) | 22 | 33 |
| `organ.vibrato` (enabled) | 21 | 25 |

## 4. Piano

| Path | Programs | Occurrences |
|---|---|---|
| `piano.category=Clav` | 12 | 12 |
| `piano.category=Digital` | 12 | 12 |
| `piano.category=Electric` | 59 | 63 |
| `piano.category=Grand` | 152 | 155 |
| `piano.category=Misc` | 8 | 9 |
| `piano.category=Upright` | 48 | 55 |
| `piano.sample=loaded` | 267 | 306 |
| `piano.class=1` | 5 | 5 |
| `piano.class=14` | 8 | 8 |
| `piano.class=15` | 4 | 4 |
| `piano.class=16` | 8 | 9 |
| `piano.class=2` | 52 | 55 |
| `piano.class=3` | 8 | 8 |
| `piano.class=4` | 9 | 9 |
| `piano.class=5` | 149 | 150 |
| `piano.class=6` | 48 | 55 |
| `piano.class=7` | 3 | 3 |
| `piano.string_res` (enabled) | 238 | 256 |

## 5. Synth

| Path | Programs | Occurrences |
|---|---|---|
| `synth.osc=Analog` | 193 | 314 |
| `synth.osc=Fm` | 25 | 38 |
| `synth.osc=Sample` | 119 | 215 |
| `synth.osc=Wavetable` | 12 | 14 |
| `synth.analog=MISC` | 8 | 8 |
| `synth.analog=MULTI` | 43 | 44 |
| `synth.analog=PURE` | 61 | 90 |
| `synth.analog=SHAPE` | 50 | 64 |
| `synth.analog=SHAPE SINE` | 30 | 32 |
| `synth.analog=SUB OSC` | 1 | 1 |
| `synth.analog=SUPER` | 47 | 57 |
| `synth.analog=SYNC` | 16 | 18 |
| `synth.wave=BELLS/TINES` | 3 | 3 |
| `synth.wave=DIGITAL` | 6 | 8 |
| `synth.wave=KEYS` | 2 | 2 |
| `synth.wave=ORGAN` | 1 | 1 |
| `synth.sample=loaded` | 119 | 215 |
| `synth.sample.kernel=0` (keys <= 47 reachable) | 115 | 208 |
| `synth.sample.kernel=1` .. `=5` (each) | 119 | 215 |
| `synth.filter=BandPass` | 18 | 24 |
| `synth.filter=HighPass` | 11 | 14 |
| `synth.filter=Ladder24` | 52 | 69 |
| `synth.filter=Lp12` | 132 | 218 |
| `synth.filter=Lp24` | 88 | 119 |
| `synth.unison=1` | 51 | 70 |
| `synth.unison=2` | 47 | 57 |
| `synth.unison=3` | 72 | 91 |
| `synth.lfo_dest=FilterCutoff` | 38 | 52 |
| `synth.lfo_dest=Off` | 226 | 425 |
| `synth.lfo_dest=OscCtrl` | 50 | 66 |
| `synth.lfo_dest=OscPitch` | 33 | 38 |
| `synth.arp` (enabled) | 32 | 49 |
| `synth.mono` (enabled) | 34 | 48 |

SAMPLE layer kernels (2026-11-14 re-run, 384 programs, 4,800 frames, `--frames 4800`): all 215 SAMPLES layers (119 programs) load their `.nsmp4`; over the key spans of their zones the firmware key rule ([synth-sample-kernel-rule](synth-sample-kernel-rule.md)) reaches kernels 1 to 5 in every layer and kernel 0 (keys up to 47) in 208 layers of 115 programs, so the six kernels are all in use and the ASSUMED ratio rule (which never picks kernels 0 and 1) differs from the firmware in every SAMPLES program. The 14 WAVE (Wavetable) layers in 12 programs (BELLS/TINES 3, DIGITAL 8, KEYS 2, ORGAN 1) stay unmodelled: no ARM path of the WAVE bank is decoded.

## 6. Effects

| Path | Programs | Occurrences |
|---|---|---|
| `fx.mod1=A-PAN` | 50 | 70 |
| `fx.mod1=A-WAH` | 6 | 7 |
| `fx.mod1=PUMP` | 9 | 19 |
| `fx.mod1=RM` | 3 | 3 |
| `fx.mod1=TREM` | 21 | 23 |
| `fx.mod1=WAH` | 2 | 2 |
| `fx.mod2=CHOR` | 93 | 154 |
| `fx.mod2=ENS` | 3 | 5 |
| `fx.mod2=FLANG` | 10 | 15 |
| `fx.mod2=PHAS` | 12 | 16 |
| `fx.mod2=SPIN` | 11 | 14 |
| `fx.mod2=VIBE` | 45 | 55 |
| `fx.amp=HP24` | 8 | 13 |
| `fx.amp=JC` | 1 | 1 |
| `fx.amp=JC II` | 3 | 3 |
| `fx.amp=LP24` | 19 | 26 |
| `fx.amp=SMALL` | 12 | 16 |
| `fx.amp=TO ROTARY` | 2 | 2 |
| `fx.amp=TWIN` | 4 | 4 |
| `fx.amp=TWIN II` | 19 | 20 |
| `fx.amp=VERY SMALL` | 5 | 6 |
| `fx.amp=off` | 214 | 387 |
| `fx.delay.mode=0` | 41 | 67 |
| `fx.delay.mode=1` | 73 | 111 |
| `fx.delay.mode=2` | 32 | 48 |
| `fx.delay.mode=3` | 17 | 24 |
| `fx.delay_fb=CHOR` | 22 | 28 |
| `fx.delay_fb=ENS` | 5 | 10 |
| `fx.delay_fb=FLAM` | 11 | 14 |
| `fx.delay_fb=SPACE` | 21 | 27 |
| `fx.delay_fb=VIBE` | 11 | 17 |
| `fx.reverb=BOOTH` | 193 | 358 |
| `fx.reverb=ROOM` | 150 | 323 |
| `fx.reverb=SPRING` | 57 | 71 |
| `fx.reverb.type=0` | 40 | 47 |
| `fx.reverb.type=1` | 17 | 24 |
| `fx.reverb.type=2` | 39 | 57 |
| `fx.reverb.type=3` | 158 | 301 |
| `fx.reverb.type=4` | 136 | 268 |
| `fx.reverb.type=5` | 28 | 55 |
| `fx.amp` (enabled) | 261 | 478 |
| `fx.comp` (enabled) | 117 | 202 |
| `fx.delay` (enabled) | 153 | 250 |
| `fx.eq` (enabled) | 265 | 501 |
| `fx.mod1` (enabled) | 90 | 124 |
| `fx.mod2` (enabled) | 158 | 259 |
| `fx.reverb` (enabled) | 358 | 752 |

## 7. Verification status of the paths in use

Uses count a path once per layer or enabled FX unit that needs it; one layer or unit can appear in several rows (for example `fx.amp` and `fx.amp=LP24`), so the totals below are classified path uses, not distinct layers. `default` is the engine default, `firmware-all` is `--firmware-all` (routing aside).

| Path | Programs | Uses | Default | Firmware-all | Basis |
|---|---|---|---|---|---|
| `fx.amp` | 261 | 478 | verified | verified | effects-and-routing: firmware drive stage and cabinet are the default amp path (words VERIFIED, voicing ASSUMED) |
| `fx.amp=HP24` | 8 | 13 | assumed | assumed | effects-and-routing: topology ASSUMED |
| `fx.amp=LP24` | 19 | 26 | assumed | assumed | effects-and-routing: topology ASSUMED |
| `fx.amp=TO ROTARY` | 2 | 2 | unmodelled | unmodelled | effects-and-routing: To Rotary is a pass-through |
| `fx.comp` | 117 | 202 | assumed | verified | effects-and-routing: firmware compressor routine VERIFIED, parameters and bus position ASSUMED |
| `fx.delay` | 153 | 250 | assumed | verified | effects-and-routing: firmware time/mix words VERIFIED (firmware_fx.delay_laws), loop is the engine's |
| `fx.delay_fb` | 67 | 96 | assumed | assumed | effects-and-routing: feedback effects approximated |
| `fx.eq` | 265 | 501 | assumed | assumed | effects-and-routing: RBJ biquads; gain step and Q ASSUMED |
| `fx.mod1` | 90 | 124 | assumed | assumed | effects-and-routing: Mod 1 types are engine models, constants ASSUMED |
| `fx.mod2` | 158 | 259 | assumed | assumed | effects-and-routing: Mod 2 types are engine models, constants ASSUMED |
| `fx.mod2=ENS` | 3 | 5 | assumed | assumed | effects-and-routing: Ensemble approximated (DSP not in ARM image) |
| `fx.mod2=SPIN` | 11 | 14 | assumed | assumed | effects-and-routing: Spin approximated (DSP not in ARM image) |
| `fx.reverb` | 358 | 752 | assumed | assumed | effects-and-routing: reverb network is the engine FDN; wrap routine not wired; mix law verified under firmware-all |
| `organ.drawbars` | 42 | 58 | assumed | assumed | organ-engine: drawbar scaling per model, Legacy for non-Pipe |
| `organ.model=B3` | 26 | 35 | assumed | assumed | organ-model-paths: B3 tone source not located; Legacy wheels; firmware-all forces the Pipe bank (attribution disproved) |
| `organ.model=B3 Bass` | 3 | 3 | assumed | assumed | organ-model-paths: raw model 5 decodes to B3 plus the bass flag (was folded into Pipe2); bass processor 0x122296 not bit-exact (OQ18) |
| `organ.model=Farfisa` | 2 | 3 | assumed | assumed | organ-model-paths: Farfisa tone source not located; Legacy wheels |
| `organ.model=Pipe1` | 3 | 4 | verified | verified | organ-model-paths: 92-wheel bank VERIFIED, role for the pipes ASSUMED |
| `organ.model=Pipe2` | 4 | 5 | verified | verified | organ-model-paths: 92-wheel bank VERIFIED, role for the pipes ASSUMED |
| `organ.model=Vox` | 7 | 8 | assumed | assumed | organ-model-paths: Vox bank is VERIFIED but opt-in (`--tone-source vox`), not in firmware-all |
| `organ.percussion` | 6 | 8 | assumed | assumed | organ-engine: percussion ASSUMED |
| `organ.rotary` | 22 | 33 | assumed | assumed | organ-engine: Rotary 122 model ASSUMED; Leslie routine not wired into the program path |
| `organ.vibrato` | 21 | 25 | assumed | assumed | organ-engine: scanner vibrato/chorus ASSUMED (B3 tail opt-in) |
| `piano.extra_voice=not_played` | 247 | 273 | assumed | assumed | piano-sound-load: classes 1,2,3,5,6 start an extra voice the engine does not play |
| `piano.sample=loaded` | 267 | 306 | verified | verified | piano-engine: .npno stream decode and firmware level law (D23 default) |
| `routing.split` | 48 | 48 | assumed | verified | morph-and-zones: splits/zones follow the program only with --firmware-routing (zone codes partly verified) |
| `synth.analog=MISC` | 8 | 8 | unmodelled | unmodelled | synth-wave-models: not modelled |
| `synth.analog=MULTI` | 43 | 44 | unmodelled | unmodelled | synth-wave-models: not modelled |
| `synth.analog=PURE` | 61 | 90 | assumed | verified | synth-wave-models: PURE saw/sine/triangle/pulse stages verified on the oracle; only the saw is wired (strip chain, firmware-all) |
| `synth.analog=SHAPE` | 50 | 64 | assumed | assumed | synth-wave-models: Shape Saw/Triangle stages verified, shape word unknown; opt-in `--synth-shape` |
| `synth.analog=SHAPE SINE` | 30 | 32 | unmodelled | unmodelled | synth-wave-models: not modelled |
| `synth.analog=SUB OSC` | 1 | 1 | unmodelled | unmodelled | synth-wave-models: not modelled |
| `synth.analog=SUPER` | 47 | 57 | assumed | assumed | synth-engine: Super Saw EMPIRICAL voice spread, stage not modelled |
| `synth.analog=SYNC` | 16 | 18 | unmodelled | unmodelled | synth-wave-models: not modelled |
| `synth.arp` | 32 | 49 | assumed | assumed | synth-engine: arpeggiator ASSUMED (pattern table not located) |
| `synth.envelopes` | 289 | 581 | assumed | verified | synth-engine: strip chain records VERIFIED bit exact, panel mapping ASSUMED |
| `synth.filter=BandPass` | 18 | 24 | assumed | verified | synth-engine: firmware BP4 stage VERIFIED, type map ASSUMED |
| `synth.filter=HighPass` | 11 | 14 | assumed | verified | synth-engine: firmware HP4 stage VERIFIED, type map ASSUMED |
| `synth.filter=Ladder24` | 52 | 69 | assumed | verified | synth-engine: ZDF filter by default; firmware LP4 stage VERIFIED, type map ASSUMED |
| `synth.filter=Lp12` | 132 | 218 | assumed | verified | synth-engine: ZDF filter by default; firmware LP4 stage VERIFIED, type map ASSUMED |
| `synth.filter=Lp24` | 88 | 119 | assumed | verified | synth-engine: ZDF filter by default; firmware LP4 stage VERIFIED, type map ASSUMED |
| `synth.osc=Analog` | 193 | 314 | assumed | assumed | synth-engine: VA waveforms approximate the strip stages |
| `synth.osc=Fm` | 25 | 38 | assumed | assumed | synth-engine: FM ratio index decoded, wave audio ASSUMED |
| `synth.osc=Sample` | 119 | 215 | verified | verified | synth-engine: .nsmp4 PCM played through the verified 12-tap sinc kernels (kernel choice ASSUMED) |
| `synth.osc=Wavetable` | 12 | 14 | unmodelled | unmodelled | synth-engine: WAVE bank is a synthetic 64-table placeholder |
| `synth.sample.kernel=0` | 115 | 208 | assumed | assumed | synth-sample-kernel-rule: kernel chosen by key, VERIFIED run of the ARM chain and key field (D46, default); zone pitch key and tune of the sample map ASSUMED |
| `synth.sample.kernel=1` | 119 | 215 | assumed | assumed | synth-sample-kernel-rule: kernel chosen by key, VERIFIED run of the ARM chain and key field (D46, default); zone pitch key and tune of the sample map ASSUMED |
| `synth.sample.kernel=2` | 119 | 215 | assumed | assumed | synth-sample-kernel-rule: kernel chosen by key, VERIFIED run of the ARM chain and key field (D46, default); zone pitch key and tune of the sample map ASSUMED |
| `synth.sample.kernel=3` | 119 | 215 | assumed | assumed | synth-sample-kernel-rule: kernel chosen by key, VERIFIED run of the ARM chain and key field (D46, default); zone pitch key and tune of the sample map ASSUMED |
| `synth.sample.kernel=4` | 119 | 215 | assumed | assumed | synth-sample-kernel-rule: kernel chosen by key, VERIFIED run of the ARM chain and key field (D46, default); zone pitch key and tune of the sample map ASSUMED |
| `synth.sample.kernel=5` | 119 | 215 | assumed | assumed | synth-sample-kernel-rule: kernel chosen by key, VERIFIED run of the ARM chain and key field (D46, default); zone pitch key and tune of the sample map ASSUMED |

| Configuration | verified | assumed | unmodelled |
|---|---|---|---|
| default | 1008 (15%) | 5656 (83%) | 119 (2%) |
| firmware-all | 2623 (39%) | 4041 (60%) | 119 (2%) |

Use-weighted: a path used by two layers counts twice. A `verified` row means the algorithm is firmware-verified, not that its parameters or the panel mapping are.

## 8. Render health

- `default`: 384 of 384 rendered without panic; non-finite 0, silent 0, any clipped sample 37, clipping >= 0.1%: 9.
- `firmware`: 384 of 384 rendered without panic; non-finite 0, silent 0, any clipped sample 40, clipping >= 0.1%: 9.

Level difference firmware minus default (RMS, dB) over 384 programs: min -7.54, p10 -2.09, median 0.39, p90 2.84, max 18.39, mean 0.42.

### Outliers

Counts: panic 0, non-finite 0, silent 0, clipping 18, level jump 11.

**Clipping (top 30)**

| entry | mode | peak | clip_fraction |
|---|---|---|---|
| Program/Bank D/Windhoek B3 II.ns4p | firmware | 1.0 | 0.135667 |
| Program/Bank B/Burning Lead.ns4p | firmware | 1.0 | 0.114094 |
| Program/Bank D/Windhoek B3 II.ns4p | default | 1.0 | 0.09574 |
| Program/Bank A/B3 Shade.ns4p | default | 1.0 | 0.037714 |
| Program/Bank A/B3 Shade.ns4p | firmware | 1.0 | 0.021974 |
| Program/Bank D/Vox Inna-Gadda.ns4p | firmware | 1.0 | 0.019714 |
| Program/Bank B/Solo n Walk B3.ns4p | default | 1.0 | 0.019104 |
| Program/Bank D/Door Knob.ns4p | default | 1.0 | 0.017057 |
| Program/Bank D/Vox Inna-Gadda.ns4p | default | 1.0 | 0.009443 |
| Program/Bank B/Burning Lead.ns4p | default | 1.0 | 0.008812 |
| Program/Bank C/LoFi Piano.ns4p | default | 1.0 | 0.008188 |
| Program/Bank E/Wah EP Ondre J.ns4p | firmware | 1.0 | 0.004646 |
| Program/Bank B/Q and A.ns4p | firmware | 1.0 | 0.004323 |
| Program/Bank C/Vox Like Me.ns4p | firmware | 1.0 | 0.002406 |
| Program/Bank C/TapePiano AT.ns4p | default | 1.0 | 0.001396 |
| Program/Bank E/Cinema Rise.ns4p | firmware | 1.0 | 0.001146 |
| Program/Bank C/Vox Like Me.ns4p | default | 1.0 | 0.001047 |
| Program/Bank B/Solo n Walk B3.ns4p | firmware | 1.0 | 0.001 |

**Level jumps (top 30)**

| entry | rms_delta_db |
|---|---|
| Program/Bank C/Farfisa II.ns4p | 18.39 |
| Program/Bank B/Fan Club Piano.ns4p | 9.22 |
| Program/Bank D/16th Note Bass.ns4p | 8.73 |
| Program/Bank A/Jazz Guitar II.ns4p | 8.64 |
| Program/Bank E/Snappy D6.ns4p | 8.35 |
| Program/Bank C/Bounce Clav.ns4p | 7.58 |
| Program/Bank B/Wormhole Organ.ns4p | -7.54 |
| Program/Bank C/LoFi Piano.ns4p | -7.47 |
| Program/Bank F/Resonant Peaks.ns4p | 6.66 |
| Program/Bank F/OD Rock Bass.ns4p | 6.46 |
| Program/Bank A/B3 Old Forest.ns4p | -6.39 |

## 9. Implications

(see the data file)

## 10. Follow-ups

-

Data: [data/program-sweep.json](data/program-sweep.json). Reproduce: `cargo build --release -p nord-tools --bin program_sweep` then `uv run python tools/program_sweep.py --ns4b ".reference/<backup>.ns4b"`.
