# Research: factory program coverage and render health

- **Checked:** 2026-11-14
- **Sources (local, never committed):** the factory restore backup `Nord Stage 4 Factory Restore RevK v1.66.ns4b` (384 `.ns4p` programs, its `.npno` and `.nsmp4` entries), `tools/program_sweep.py` driving `crates/nord-core/examples/program_sweep.rs` (release build), the classification of paths from `docs/specs/*.md` and `docs/research/*.md` cited per row.
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
| piano+synth | 168 (44%) |
| synth | 96 (25%) |
| piano | 76 (20%) |
| organ+piano+synth | 14 (4%) |
| organ+synth | 9 (2%) |
| organ | 8 (2%) |
| none | 7 (2%) |
| organ+piano | 6 (2%) |

| Layers sounding | Programs |
|---|---|
| 0 | 7 |
| 1 | 95 |
| 2 | 110 |
| 3 | 92 |
| 4 | 60 |
| 5 | 18 |
| 6 | 1 |
| 7 | 1 |

Scene II used by 210 programs, split enabled in 48, non-default key zone code on a sounding layer in 165.

| Key zone code (sounding layers) | Programs |
|---|---|
| 0 | 39 |
| 1 | 4 |
| 10 | 3 |
| 2 | 2 |
| 3 | 1 |
| 4 | 9 |
| 5 | 255 |
| 6 | 35 |
| 9 | 113 |

## 3. Organ

| Path | Programs | Occurrences |
|---|---|---|
| `organ.model=B3` | 26 | 35 |
| `organ.model=B3 Bass` | 3 | 3 |
| `organ.model=Farfisa` | 2 | 2 |
| `organ.model=Pipe1` | 3 | 4 |
| `organ.model=Pipe2` | 4 | 5 |
| `organ.model=Vox` | 2 | 3 |
| `organ.rotary_speed=Fast` | 1 | 2 |
| `organ.rotary_speed=Slow` | 16 | 25 |
| `organ.rotary_speed=Stop` | 5 | 6 |
| `organ.vibrato_type=0` | 1 | 1 |
| `organ.vibrato_type=1` | 2 | 3 |
| `organ.vibrato_type=4` | 3 | 3 |
| `organ.vibrato_type=5` | 12 | 15 |
| `organ.percussion` (enabled) | 6 | 8 |
| `organ.rotary` (enabled) | 22 | 33 |
| `organ.vibrato` (enabled) | 18 | 22 |

## 4. Piano

| Path | Programs | Occurrences |
|---|---|---|
| `piano.category=Clav` | 12 | 12 |
| `piano.category=Digital` | 12 | 12 |
| `piano.category=Electric` | 59 | 63 |
| `piano.category=Grand` | 150 | 153 |
| `piano.category=Misc` | 8 | 9 |
| `piano.category=Upright` | 47 | 54 |
| `piano.sample=loaded` | 264 | 303 |
| `piano.class=1` | 4 | 4 |
| `piano.class=14` | 8 | 8 |
| `piano.class=15` | 4 | 4 |
| `piano.class=16` | 8 | 9 |
| `piano.class=2` | 52 | 55 |
| `piano.class=3` | 8 | 8 |
| `piano.class=4` | 9 | 9 |
| `piano.class=5` | 148 | 149 |
| `piano.class=6` | 47 | 54 |
| `piano.class=7` | 3 | 3 |
| `piano.string_res` (enabled) | 237 | 255 |

## 5. Synth

| Path | Programs | Occurrences |
|---|---|---|
| `synth.osc=Analog` | 191 | 312 |
| `synth.osc=Fm` | 25 | 38 |
| `synth.osc=Sample` | 119 | 215 |
| `synth.osc=Wavetable` | 12 | 14 |
| `synth.analog=MISC` | 8 | 8 |
| `synth.analog=MULTI` | 42 | 43 |
| `synth.analog=PURE` | 61 | 90 |
| `synth.analog=SHAPE` | 49 | 63 |
| `synth.analog=SHAPE SINE` | 30 | 32 |
| `synth.analog=SUB OSC` | 1 | 1 |
| `synth.analog=SUPER` | 47 | 57 |
| `synth.analog=SYNC` | 16 | 18 |
| `synth.wave=BELLS/TINES` | 3 | 3 |
| `synth.wave=DIGITAL` | 6 | 8 |
| `synth.wave=KEYS` | 2 | 2 |
| `synth.wave=ORGAN` | 1 | 1 |
| `synth.sample=loaded` | 119 | 215 |
| `synth.filter=BandPass` | 18 | 24 |
| `synth.filter=HighPass` | 11 | 14 |
| `synth.filter=Ladder24` | 51 | 68 |
| `synth.filter=Lp12` | 132 | 218 |
| `synth.filter=Lp24` | 87 | 118 |
| `synth.unison=1` | 51 | 70 |
| `synth.unison=2` | 47 | 57 |
| `synth.unison=3` | 72 | 91 |
| `synth.lfo_dest=FilterCutoff` | 38 | 52 |
| `synth.lfo_dest=Off` | 225 | 424 |
| `synth.lfo_dest=OscCtrl` | 49 | 65 |
| `synth.lfo_dest=OscPitch` | 33 | 38 |
| `synth.arp` (enabled) | 32 | 49 |
| `synth.mono` (enabled) | 33 | 47 |

## 6. Effects

| Path | Programs | Occurrences |
|---|---|---|
| `fx.mod1=A-PAN` | 50 | 70 |
| `fx.mod1=A-WAH` | 6 | 7 |
| `fx.mod1=PUMP` | 9 | 19 |
| `fx.mod1=RM` | 3 | 3 |
| `fx.mod1=TREM` | 20 | 22 |
| `fx.mod1=WAH` | 2 | 2 |
| `fx.mod2=CHOR` | 93 | 153 |
| `fx.mod2=ENS` | 3 | 5 |
| `fx.mod2=FLANG` | 9 | 13 |
| `fx.mod2=PHAS` | 11 | 15 |
| `fx.mod2=SPIN` | 11 | 14 |
| `fx.mod2=VIBE` | 45 | 55 |
| `fx.amp=HP24` | 8 | 13 |
| `fx.amp=JC` | 1 | 1 |
| `fx.amp=JC II` | 3 | 3 |
| `fx.amp=LP24` | 19 | 26 |
| `fx.amp=SMALL` | 12 | 16 |
| `fx.amp=TO ROTARY` | 2 | 2 |
| `fx.amp=TWIN` | 4 | 4 |
| `fx.amp=TWIN II` | 14 | 15 |
| `fx.amp=VERY SMALL` | 4 | 5 |
| `fx.amp=off` | 213 | 386 |
| `fx.delay.mode=0` | 41 | 67 |
| `fx.delay.mode=1` | 72 | 110 |
| `fx.delay.mode=2` | 32 | 48 |
| `fx.delay.mode=3` | 16 | 23 |
| `fx.delay_fb=CHOR` | 22 | 28 |
| `fx.delay_fb=ENS` | 5 | 10 |
| `fx.delay_fb=FLAM` | 10 | 13 |
| `fx.delay_fb=SPACE` | 21 | 27 |
| `fx.delay_fb=VIBE` | 11 | 17 |
| `fx.reverb=BOOTH` | 190 | 355 |
| `fx.reverb=ROOM` | 145 | 318 |
| `fx.reverb=SPRING` | 57 | 70 |
| `fx.reverb.type=0` | 40 | 46 |
| `fx.reverb.type=1` | 17 | 24 |
| `fx.reverb.type=2` | 37 | 55 |
| `fx.reverb.type=3` | 157 | 300 |
| `fx.reverb.type=4` | 131 | 263 |
| `fx.reverb.type=5` | 28 | 55 |
| `fx.amp` (enabled) | 254 | 471 |
| `fx.comp` (enabled) | 115 | 200 |
| `fx.delay` (enabled) | 151 | 248 |
| `fx.eq` (enabled) | 258 | 493 |
| `fx.mod1` (enabled) | 89 | 123 |
| `fx.mod2` (enabled) | 156 | 255 |
| `fx.reverb` (enabled) | 351 | 743 |

## 7. Verification status of the paths in use

Uses count a path once per layer or enabled FX unit that needs it; one layer or unit can appear in several rows (for example `fx.amp` and `fx.amp=LP24`), so the totals below are classified path uses, not distinct layers. `default` is the engine default, `firmware-all` is `--firmware-all` (routing aside).

| Path | Programs | Uses | Default | Firmware-all | Basis |
|---|---|---|---|---|---|
| `fx.amp` | 254 | 471 | verified | verified | effects-and-routing: firmware drive stage and cabinet are the default amp path (words VERIFIED, voicing ASSUMED) |
| `fx.amp=HP24` | 8 | 13 | assumed | assumed | effects-and-routing: topology ASSUMED |
| `fx.amp=LP24` | 19 | 26 | assumed | assumed | effects-and-routing: topology ASSUMED |
| `fx.amp=TO ROTARY` | 2 | 2 | unmodelled | unmodelled | effects-and-routing: To Rotary is a pass-through |
| `fx.comp` | 115 | 200 | assumed | verified | effects-and-routing: firmware compressor routine VERIFIED, parameters and bus position ASSUMED |
| `fx.delay` | 151 | 248 | assumed | verified | effects-and-routing: firmware time/mix words VERIFIED (firmware_fx.delay_laws), loop is the engine's |
| `fx.delay_fb` | 66 | 95 | assumed | assumed | effects-and-routing: feedback effects approximated |
| `fx.eq` | 258 | 493 | assumed | assumed | effects-and-routing: RBJ biquads; gain step and Q ASSUMED |
| `fx.mod1` | 89 | 123 | assumed | assumed | effects-and-routing: Mod 1 types are engine models, constants ASSUMED |
| `fx.mod2` | 156 | 255 | assumed | assumed | effects-and-routing: Mod 2 types are engine models, constants ASSUMED |
| `fx.mod2=ENS` | 3 | 5 | assumed | assumed | effects-and-routing: Ensemble approximated (DSP not in ARM image) |
| `fx.mod2=SPIN` | 11 | 14 | assumed | assumed | effects-and-routing: Spin approximated (DSP not in ARM image) |
| `fx.reverb` | 351 | 743 | assumed | assumed | effects-and-routing: reverb network is the engine FDN; wrap routine not wired; mix law verified under firmware-all |
| `organ.drawbars` | 37 | 52 | assumed | assumed | organ-engine: drawbar scaling per model, Legacy for non-Pipe |
| `organ.model=B3` | 26 | 35 | assumed | assumed | organ-model-paths: B3 tone source not located; Legacy wheels; firmware-all forces the Pipe bank (attribution disproved) |
| `organ.model=B3 Bass` | 3 | 3 | assumed | assumed | organ-model-paths: raw model 5 decodes to B3 plus the bass flag (was folded into Pipe2); bass processor 0x122296 not bit-exact (OQ18) |
| `organ.model=Farfisa` | 2 | 2 | assumed | assumed | organ-model-paths: Farfisa tone source not located; Legacy wheels |
| `organ.model=Pipe1` | 3 | 4 | verified | verified | organ-model-paths: 92-wheel bank VERIFIED, role for the pipes ASSUMED |
| `organ.model=Pipe2` | 4 | 5 | verified | verified | organ-model-paths: 92-wheel bank VERIFIED, role for the pipes ASSUMED |
| `organ.model=Vox` | 2 | 3 | assumed | assumed | organ-model-paths: Vox bank is VERIFIED but opt-in (`--tone-source vox`), not in firmware-all |
| `organ.percussion` | 6 | 8 | assumed | assumed | organ-engine: percussion ASSUMED |
| `organ.rotary` | 22 | 33 | assumed | assumed | organ-engine: Rotary 122 model ASSUMED; Leslie routine not wired into the program path |
| `organ.vibrato` | 18 | 22 | assumed | assumed | organ-engine: scanner vibrato/chorus ASSUMED (B3 tail opt-in) |
| `piano.extra_voice=not_played` | 244 | 270 | assumed | assumed | piano-sound-load: classes 1,2,3,5,6 start an extra voice the engine does not play |
| `piano.sample=loaded` | 264 | 303 | verified | verified | piano-engine: .npno stream decode and firmware level law (D23 default) |
| `routing.split` | 48 | 48 | assumed | verified | morph-and-zones: splits/zones follow the program only with --firmware-routing (zone codes partly verified) |
| `synth.analog=MISC` | 8 | 8 | unmodelled | unmodelled | synth-wave-models: not modelled |
| `synth.analog=MULTI` | 42 | 43 | unmodelled | unmodelled | synth-wave-models: not modelled |
| `synth.analog=PURE` | 61 | 90 | assumed | verified | synth-wave-models: PURE saw/sine/triangle/pulse stages verified on the oracle; only the saw is wired (strip chain, firmware-all) |
| `synth.analog=SHAPE` | 49 | 63 | assumed | assumed | synth-wave-models: Shape Saw/Triangle stages verified, shape word unknown; opt-in `--synth-shape` |
| `synth.analog=SHAPE SINE` | 30 | 32 | unmodelled | unmodelled | synth-wave-models: not modelled |
| `synth.analog=SUB OSC` | 1 | 1 | unmodelled | unmodelled | synth-wave-models: not modelled |
| `synth.analog=SUPER` | 47 | 57 | assumed | assumed | synth-engine: Super Saw EMPIRICAL voice spread, stage not modelled |
| `synth.analog=SYNC` | 16 | 18 | unmodelled | unmodelled | synth-wave-models: not modelled |
| `synth.arp` | 32 | 49 | assumed | assumed | synth-engine: arpeggiator ASSUMED (pattern table not located) |
| `synth.envelopes` | 287 | 579 | assumed | verified | synth-engine: strip chain records VERIFIED bit exact, panel mapping ASSUMED |
| `synth.filter=BandPass` | 18 | 24 | assumed | verified | synth-engine: firmware BP4 stage VERIFIED, type map ASSUMED |
| `synth.filter=HighPass` | 11 | 14 | assumed | verified | synth-engine: firmware HP4 stage VERIFIED, type map ASSUMED |
| `synth.filter=Ladder24` | 51 | 68 | assumed | verified | synth-engine: ZDF filter by default; firmware LP4 stage VERIFIED, type map ASSUMED |
| `synth.filter=Lp12` | 132 | 218 | assumed | verified | synth-engine: ZDF filter by default; firmware LP4 stage VERIFIED, type map ASSUMED |
| `synth.filter=Lp24` | 87 | 118 | assumed | verified | synth-engine: ZDF filter by default; firmware LP4 stage VERIFIED, type map ASSUMED |
| `synth.osc=Analog` | 191 | 312 | assumed | assumed | synth-engine: VA waveforms approximate the strip stages |
| `synth.osc=Fm` | 25 | 38 | assumed | assumed | synth-engine: FM ratio index decoded, wave audio ASSUMED |
| `synth.osc=Sample` | 119 | 215 | verified | verified | synth-engine: .nsmp4 PCM played through the verified 12-tap sinc kernels (kernel choice ASSUMED) |
| `synth.osc=Wavetable` | 12 | 14 | unmodelled | unmodelled | synth-engine: WAVE bank is a synthetic 64-table placeholder |

| Configuration | verified | assumed | unmodelled |
|---|---|---|---|
| default | 998 (18%) | 4321 (79%) | 118 (2%) |
| firmware-all | 2605 (48%) | 2714 (50%) | 118 (2%) |

Use-weighted: a path used by two layers counts twice. A `verified` row means the algorithm is firmware-verified, not that its parameters or the panel mapping are.

## 8. Render health

- `default`: 384 of 384 rendered without panic; non-finite 0, silent 0, any clipped sample 43, clipping >= 0.1%: 18.
- `firmware`: 384 of 384 rendered without panic; non-finite 0, silent 9, any clipped sample 55, clipping >= 0.1%: 18.

Level difference firmware minus default (RMS, dB) over 375 programs: min -16.33, p10 -3.29, median 0.34, p90 3.18, max 17.0, mean 0.24.

### Outliers

Counts: panic 0, non-finite 0, silent 9, clipping 36, level jump 29.

**Silent renders**

| entry | mode | peak |
|---|---|---|
| Program/Bank B/Burning Lead.ns4p | firmware | 0.0 |
| Program/Bank C/Farfisa II.ns4p | firmware | 0.0 |
| Program/Bank C/LoFi Piano.ns4p | firmware | 0.0 |
| Program/Bank C/Vox Like Me.ns4p | firmware | 0.0 |
| Program/Bank C/Vox Rising Sun.ns4p | firmware | 0.0 |
| Program/Bank D/70s CP Strings.ns4p | firmware | 0.0 |
| Program/Bank D/Vox Inna-Gadda.ns4p | firmware | 0.0 |
| Program/Bank E/Vox Im Down.ns4p | firmware | 0.0 |
| Program/Bank F/Vox DC5.ns4p | firmware | 0.0 |

**Clipping (top 30)**

| entry | mode | peak | clip_fraction |
|---|---|---|---|
| Program/Bank A/B3 Shade.ns4p | firmware | 1.0 | 0.340094 |
| Program/Bank B/Solo n Walk B3.ns4p | default | 1.0 | 0.301109 |
| Program/Bank A/12dB Sweep.ns4p | default | 1.0 | 0.29675 |
| Program/Bank A/B3 Shade.ns4p | default | 1.0 | 0.293339 |
| Program/Bank B/Solo n Walk B3.ns4p | firmware | 1.0 | 0.272891 |
| Program/Bank D/Windhoek B3 II.ns4p | firmware | 1.0 | 0.220917 |
| Program/Bank D/Windhoek B3 II.ns4p | default | 1.0 | 0.211375 |
| Program/Bank B/PWM Pan Pad.ns4p | default | 1.0 | 0.17838 |
| Program/Bank A/B3 Chords n Solo.ns4p | default | 1.0 | 0.169927 |
| Program/Bank B/B3 Bassline.ns4p | default | 1.0 | 0.136698 |
| Program/Bank D/16th Note Bass.ns4p | firmware | 1.0 | 0.128365 |
| Program/Bank B/PWM Pan Pad.ns4p | firmware | 1.0 | 0.107922 |
| Program/Bank A/B3 Chords n Solo.ns4p | firmware | 1.0 | 0.098526 |
| Program/Bank E/Ready ta Scream.ns4p | firmware | 1.0 | 0.088526 |
| Program/Bank E/Ready ta Scream.ns4p | default | 1.0 | 0.065036 |
| Program/Bank B/B3 Bassline.ns4p | firmware | 1.0 | 0.048891 |
| Program/Bank D/Rotor EP.ns4p | firmware | 1.0 | 0.0205 |
| Program/Bank D/Door Knob.ns4p | default | 1.0 | 0.017057 |
| Program/Bank D/Rotor EP.ns4p | default | 1.0 | 0.012219 |
| Program/Bank E/Hybrid Knock EP.ns4p | firmware | 1.0 | 0.009568 |
| Program/Bank D/Vox Inna-Gadda.ns4p | default | 1.0 | 0.009443 |
| Program/Bank B/Burning Lead.ns4p | default | 1.0 | 0.008812 |
| Program/Bank C/LoFi Piano.ns4p | default | 1.0 | 0.008266 |
| Program/Bank C/Bounce Clav.ns4p | firmware | 1.0 | 0.004615 |
| Program/Bank A/The Gospel II.ns4p | default | 1.0 | 0.004495 |
| Program/Bank B/Q and A.ns4p | firmware | 1.0 | 0.004484 |
| Program/Bank A/The Gospel II.ns4p | firmware | 1.0 | 0.003531 |
| Program/Bank A/12dB Sweep.ns4p | firmware | 1.0 | 0.003266 |
| Program/Bank F/Wooden Queen.ns4p | firmware | 1.0 | 0.00224 |
| Program/Bank D/PWM and Noise.ns4p | default | 1.0 | 0.002026 |

**Level jumps (top 30)**

| entry | rms_delta_db |
|---|---|
| Program/Bank C/Bounce Clav.ns4p | 17.0 |
| Program/Bank D/Danceflr Peak MW.ns4p | -16.33 |
| Program/Bank D/16th Note Bass.ns4p | 13.41 |
| Program/Bank F/Circus Box.ns4p | -12.76 |
| Program/Bank E/Short Chords.ns4p | -11.51 |
| Program/Bank D/Chip Bass.ns4p | -11.19 |
| Program/Bank F/Unclocked Pad.ns4p | -10.86 |
| Program/Bank B/Fan Club Piano.ns4p | 10.65 |
| Program/Bank E/Statement Syn Bs.ns4p | 10.6 |
| Program/Bank A/Dual Gates.ns4p | 10.57 |
| Program/Bank A/Smooth Lead.ns4p | 10.2 |
| Program/Bank F/Resonant Peaks.ns4p | 10.2 |
| Program/Bank D/Rose Synth Piano.ns4p | -10.15 |
| Program/Bank D/Arctic Choir Whl.ns4p | -9.3 |
| Program/Bank E/Wet Funk Syn Bs.ns4p | 9.11 |
| Program/Bank D/Phil Pad.ns4p | 8.72 |
| Program/Bank C/Rising Sun.ns4p | -8.3 |
| Program/Bank C/Syn Flute.ns4p | 8.18 |
| Program/Bank E/Snappy D6.ns4p | 8.06 |
| Program/Bank E/PineScape Synth.ns4p | -7.75 |
| Program/Bank F/Time Warp Saw.ns4p | 7.48 |
| Program/Bank F/Scream Lead AT.ns4p | -6.92 |
| Program/Bank D/HP Splash Whl.ns4p | -6.84 |
| Program/Bank E/Sine Rain Pad.ns4p | -6.38 |
| Program/Bank F/Atmos Lead Whl.ns4p | 6.31 |
| Program/Bank D/Trumpet Synth.ns4p | 6.29 |
| Program/Bank B/B3 Dual Samba.ns4p | -6.14 |
| Program/Bank A/4 Osc Funk Bass.ns4p | 6.05 |
| Program/Bank F/Bell Strings.ns4p | -6.02 |

## 9. Implications

- **Coverage by engine.** Of 384 programs, 168 use piano+synth, 96 synth only, 76 piano only; only 37 touch the organ. Piano and synth sample playback (real `.npno` and `.nsmp4` data) is therefore the bulk of the factory space, and every piano class in use (1-7, 14-16) is one whose extra-voice rule is documented (`piano-sound-load`).
- **Assumed paths dominate.** Use-weighted, 18% of classified path uses (layers and FX units) sit on a verified path with engine defaults (998 of 5437) and 48% with `--firmware-all` (2605 of 5437); 2% (118) are not modelled at all: SYNC, MULTI, SHAPE SINE, SUB OSC and MISC analog waves (102 layers), the synthetic WAVE bank (14 layers) and Amp To Rotary (2 units). Most of the remaining assumed weight is FX (reverb, mod, delay loop) and the analog oscillators.
- **Piano extra voice.** 244 programs use a piano class (1, 2, 3, 5, 6) whose firmware starts an extra voice per note that the engine does not play; the level of the main voice is verified, the layering is not.
- **Firmware routing is not safe to default yet.** 9 programs are silent in scene I under `--firmware-routing` (OQ24); the legacy fallback keeps them audible. 165 programs use a non-default key zone code on a sounding layer and 48 enable splits, so zone codes matter for the real space.
- **Health.** No panic, NaN or Inf in any of 768 renders (nor in 96 programs re-rendered with notes 21, 36, 96, 108 at velocity 127, run once before the B3 Bass fix). The master output is hard limited at 1.0: 43 programs (default) and 55 (firmware-all) touch full scale, 18 each clip on at least 0.1% of samples, mostly organ B3 or full-chord synth programs; this is a level calibration question, not a numeric fault.
- **Level jumps.** Firmware-all versus default RMS differs by a median of 0.34 dB with p10/p90 of -3.3/+3.2 dB; 29 programs differ by 6 dB or more (up to +17.0 and -16.3 dB), see the table. These are the programs where an opt-in path changes level most and the first candidates for a hardware A/B.

## 10. Follow-ups

- Decide scene I / II semantics and the empty-mask programs (OQ24) before `--firmware-routing` can join `--firmware-all`.
- Level calibration of the organ and full-chord synth programs that hit full scale: needs a hardware level reference, not a small fix.
- Wire the verified PURE sine/triangle/pulse and Shape Saw stages, and model SYNC, MULTI, SHAPE SINE, SUB OSC and MISC (102 layers), the largest unmodelled synth share.
- Play the piano extra voice for classes 1, 2, 3, 5, 6.
- Sweep limits: one note set (C3 G3 E4 C5, velocity 100, 2 s) for every program instead of one set per engine, engines are not soloed (a finding is per program), and scene II was not rendered. Path classifications are a reading of the specs, not measurements.
- Fixed during the sweep: raw organ model 5 (`B3 Bass`, 4 factory programs) was decoded as Pipe2; it now decodes to B3 plus a bass flag that reaches `OrganLayer::b3_bass_mode` (test `b3_bass_model_decodes_to_b3_with_the_bass_flag_and_round_trips`).

Data: [data/program-sweep.json](data/program-sweep.json). Reproduce: `cargo build --release -p nord-core --example program_sweep` then `uv run python tools/program_sweep.py --ns4b ".reference/<backup>.ns4b"`.
