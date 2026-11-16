# Transkun adapter for synth clips: feasibility and plan (LoRA vs LoKr vs full)

- **Checked:** 2026-11-15
- **Sources (local, never committed):** installed Transkun 2.0.1 in `C:\tmp\ns4-transcribe\venv` (`transkun/ModelTransformer.py`, `LayersTransformer.py`, `Data.py`, `train.py`, `pretrained/2.0.conf`, `pretrained/2.0.pt`, `transkun-2.0.1.dist-info/LICENSE`, `METADATA`), upstream [github.com/Yujia-Yan/Skipping-The-Frame-Level](https://github.com/Yujia-Yan/Skipping-The-Frame-Level) (named in the package metadata; **not cloned**, the installed wheel is the source), ISMIR 2024 paper arXiv 2404.09466; `target/release/ns4.exe` (`render-patch`), `.reference/` factory backup `Nord Stage 4 Factory Restore RevK v1.66.ns4b`; earlier work [piano-transcription](piano-transcription.md), [synth-transcription-muscriptor](synth-transcription-muscriptor.md), [emu_vs_hw_segments.json](data/emu_vs_hw_segments.json); probe scripts in `C:\tmp\ns4-ft-probe` (`inspect_model.py`, `bench_step.py`, `lora_count.py`, `peft_check.py`, listed in section 9).
- **Question:** Can a small adapter on Transkun, trained on deterministic emulator renders of synth programs, raise note recall and give per-note velocity on hardware synth clips (Horizon Whl, Leaky Stabs II, Grand Pad), fits the RTX 4070 SUPER (12 GB), and which of LoRA, LoKr and full fine-tuning should be tried first?

Tags: **VERIFIED** = run in this session (command and number given); **ASSUMED** = not checked, with what would confirm it. Phase 0 trains nothing.

## 1. Result in five lines

1. Transkun 2.0 is a 13.6 M parameter model (VERIFIED), small enough that **even a full fine-tune fits** the GPU (peak 6.7 GiB at 4 x 16 s, VERIFIED). The adapter choice is about forgetting and file separation, not memory.
2. Its attention uses raw `nn.Parameter` q/k/v matrices, not `nn.Linear` (VERIFIED, 36 tensors, 2.36 M params). Stock PEFT `target_modules` reaches only the other 50 Linear and 5 Conv2d layers (6.9 M weights); q/k/v needs `target_parameters` (VERIFIED with PEFT 0.21.3) or a 20-line wrapper.
3. The velocity output is a 128-way categorical per detected note, trained by log-likelihood (VERIFIED in `log_prob`). That is the head to adapt; velocity is only scored on notes the CRF finds, so recall and velocity are two separate problems.
4. Rendering is not the bottleneck: the heaviest tested program (Horizon Whl) renders at 4.5x real time on one core and about 84x aggregate with 32 workers (VERIFIED). Training eats about 60 s of audio per second (bf16, VERIFIED on dummy audio).
5. **Order (user decision):** **LoKr first** and stop if it meets the targets in section 7.2; **LoRA** only if LoKr misses; full fine-tune only if both plateau. Same data, seed and step budget. PEFT supports both on every layer except `ConvTranspose1d`, and raw q/k/v through `target_parameters` (VERIFIED, section 4). The Phase 1 dataset (20.2 h) is rendered and the base-model baseline on its held-out programs is measured (section 6.1): onset F1 0.546, velocity MAE 26.1, no better than predicting the median (26.8).

## 2. Model facts (VERIFIED, `inspect_model.py`, `lora_count.py`)

| Item | Value |
|---|---|
| Parameters | 13,613,711 (backbone 12,626,688; scorer 131,841; velocityPredictor 459,392; refinedOFPredictor 395,780) |
| Input | 44.1 kHz audio, mono mix, per-segment gain normalisation (`processFramesBatch` subtracts mean, divides by std), 229-band log mel (`f_min` 30, `f_max` 8000), window 4096, hop 1024, 5 extra windows |
| Segment | 16 s with 8 s hop in `2.0.conf` (the README says the shipped weight was trained at 10 / 20 s; inference code uses the conf values) |
| Backbone | 5 Conv2d (`inputConv`, `downConv.1/5/9/13`) + 1 ConvTranspose1d (`upConv1dSkip`) + 6 encoder layers with factorised attention over frequency (F) and time (T), 8 heads, `baseSize` 64, FFN factor 4, RMSNorm and GroupNorm |
| Leaf modules | `Linear` 50 (9.17 M weights), `Conv2d` 5 (1.55 M), `ConvTranspose1d` 1 (0.52 M), `GroupNorm` 4, `RMSNorm` 24 (no params) |
| Raw attention params | `backbone.encoderLayers.N.mhaBlock{F,T}.module.{q,k,v}_proj_weight` (`MultiHeadAttentionKernel`, `LayersTransformer.py:146-148`), 36 tensors, 2,359,296 weights; the output projection `out_proj` is a Linear |
| Output | pitches `[-64, -67] + 21..108` (88 keys plus two pedal symbols: -64 sustain (CC 64) and -67 soft pedal (CC 67), `Data.py` `supportedCC`). Per pitch a **neural semi-CRF over intervals** (`ScaledInnerProductIntervalScorer`, 131,841 params) scores (onset, offset) frame pairs; there are no separate framewise onset/offset heads |
| Per-note heads | `velocityPredictor` (Linear 768 to 512, GELU, dropout, Linear 512 to 128; input is `[ctx_on, ctx_off, ctx_on * ctx_off]`) and `refinedOFPredictor` (768 to 512 to 4: sub-frame onset/offset position as ContinuousBernoulli plus "has onset" / "has offset" Bernoulli logits) |
| Velocity read-out | expectation of the 128-way softmax (`computeStats`); trained with `log_softmax` gather on the integer velocity |
| Loss | negative of `log P(intervals) + log P(velocity) + log P(refined offsets) + log P(presence)` per segment, `loss = -logp.sum(-1).mean()` (`train.py:187`); CRF normaliser `computeLogZ` |
| Dropout | scorer 0.1, velocity 0.1, refined 0.1, context 0.0 |

The decoded transcription (`transcribeFrames`, `transcribe.py`) keeps the CRF path and then asks the two heads for velocity and sub-frame times of that path. A synthetic-trained adapter therefore changes (a) which intervals the CRF finds (needs backbone and scorer adaptation) and (b) the velocity read-out (heads, plus the context features).

### 2.1 Training code and data format (VERIFIED by reading, not run)

| Item | Value |
|---|---|
| Script | `python -m transkun.train <ckpt> --nProcess N --datasetPath <dir> --datasetMetaFile_train train.pickle --datasetMetaFile_val val.pickle --modelConf <conf> [--augment --noiseFolder --irFolder]`; AdaBelief (`torch_optimizer`) with OneCycleLR (5 % warm-up), `max_lr` 2e-4, weight decay 1e-4, `nIter` 180000, adaptive gradient clipping at the 0.8 quantile, `loss/50` before backward (gradient accumulation by 50 sub-batches per step in the loop) |
| Dataset | `DatasetMaestro`: a pickle of dicts (`duration`, `audio_filename`, `notes` list of `Note(start, end, pitch, velocity, hasOnset, hasOffset)` with pedal as negative pitches); audio read from `.wav` by `readAudioSlice` (scipy `mmap`, integer PCM scaled by the dtype max, so write int16 WAV) |
| Slicing | `DatasetMaestroIterator`: chunks of `segmentSizeInSecond` hopping `segmentHopSizeInSecond` with random dither; `collate_fn_batching` stacks audio |
| Note rules | per pitch, notes must not overlap (`validateNotes` asserts `start >= previous end`); notes quantised to 1024-sample frames collide are merged and warn |
| Gotcha | `createIndexEvents` imports `ncls` (no Windows wheel, not installed). The dataset index path needs it; **inference does not**. Plan: write our own small iterator producing `{"notes", "audioSlices"}` batches and call `model.log_prob` directly (this is what `bench_step.py` does) |
| Not installed | `peft`, `lycoris-lora`, `pip` inside the venv (it was built with uv) |

## 3. Licence (VERIFIED for the code, ASSUMED for the weights)

- `LICENSE` in the wheel: **MIT**, copyright Yujia Yan 2021. Fine-tuning, modifying and redistributing the code is permitted with the notice.
- The pretrained `2.0.pt` has no separate licence file in the wheel. The README says only that it is the shipped checkpoint. **ASSUMED:** MIT applies to the weights as part of the package. The checkpoint was trained on MAESTRO (CC BY-NC-SA 4.0 audio and MIDI) per the training instructions; whether that restricts the weights is unsettled (ASSUMED: non-commercial research use is safe).
- **Decision for this repo:** adapters and any derived weights stay local (`C:\tmp`), never committed, consistent with the repo rule; only scripts, configs, numbers and docs go in git. Non-commercial research use only. Confirm with the upstream author before any redistribution.

## 4. PEFT / LyCORIS support (VERIFIED, `peft_check.py`)

Environment: new venv `C:/tmp/ns4-ft-venv` (Python 3.12, `uv`), reusing torch 2.6.0+cu124 and transkun from the transcription venv through a `.pth`; installed `peft` 0.21.3, `lycoris-lora`, `accelerate`, `transformers`, `toml`, `einops`. The existing venvs are untouched. Gotcha: `uv pip install peft` pulls a second torch (2.15.0) into the new venv; it was uninstalled so the `.pth` torch wins. `lycoris-lora` 4.1.3 fails to import on torch 2.6 (custom op schema); 3.2.0.post2 imports. LyCORIS itself is not needed: PEFT ships LoKr.

One forward, backward and optimizer-free step on a dummy batch (8 s audio, 15 notes), base weights loaded from `2.0.pt`:

| Config | Result | Trainable | Tensors with grad |
|---|---|---|---|
| `LoraConfig(r=16, alpha=32)` on Linear + Conv2d (regex below) + `modules_to_save` heads | works | 1,574,900 | 50 |
| `LoKrConfig(r=8, alpha=16)` same targets and heads | works | 892,468 (adapters about 37 k, heads 855 k) | 50 |
| `target_parameters` on the 36 raw q/k/v tensors (+ `inputConv` LoRA) | **works** in PEFT 0.21.3 | 296,800 (q/k/v LoRA r=16 = 294,912, matches the arithmetic below) | 37 |
| LoRA on `ConvTranspose1d` (`upConv1dSkip`) | **not supported** (`ValueError`) | n/a | n/a |
| regex that also matches `downConv.0/2/3/...` | fails: `ConstantPad2d` / `ReLU`-type modules are not adaptable; list only `downConv.(1|5|9|13)` | n/a | n/a |

- Raw q/k/v need **no wrapper**: `target_parameters` works, so the 20-line wrapper is only a fallback. Their LoKr support is unchecked.
- With the default reentrant activation checkpoint, a forward whose inputs do not require grad gives "does not require grad". An adapter on `backbone.inputConv` (or `use_reentrant=False`) avoids it; the real configs include `inputConv`, so it does not bite. Keep in mind for a q/k/v-only arm.
- `ConvTranspose1d` (0.52 M) stays frozen or fully trained.
- LoRA, LoKr and LoHa for Conv2d and Linear are in PEFT itself (`LoKrConfig`), LyCORIS is optional.

**Config used in the check (regex VERIFIED):**

```python
LoraConfig(r=16, lora_alpha=32, lora_dropout=0.05, bias="none",
           target_modules=r"backbone\.(encoderLayers\.\d+\.(mhaBlock|fnnBlock)[FT]\.module(\.(q|k|v)|\.out_proj|\.[03])|inputConv|downConv\.\d+)|scorer\.map\.0",
           modules_to_save=["velocityPredictor", "refinedOFPredictor"])
LoKrConfig(r=8, alpha=16, decompose_both=False, decompose_factor=-1, rank_dropout=0.0,
           module_dropout=0.0, target_modules=<same regex>,
           modules_to_save=["velocityPredictor", "refinedOFPredictor"])
```

Trainable weights by rank (VERIFIED arithmetic, `lora_count.py`, LoRA `r * (in + out)` per layer, Conv2d `r * (in*k*k + out)`):

| r | Backbone + scorer Linear (46) | Heads Linear (4) | Conv2d (5) | q/k/v (36) | Total |
|---|---|---|---|---|---|
| 8 | 351,792 | 29,728 | 58,800 | 147,456 | 587,776 (4.3 % of 13.6 M) |
| 16 | 703,584 | 59,456 | 117,600 | 294,912 | 1,175,552 (8.6 %) |
| 32 | 1,407,168 | 118,912 | 235,200 | 589,824 | 2,351,104 (17.3 %) |

`modules_to_save` copies the two heads in full (855,172 weights) on top; they are small enough to train fully, which is how `velocityPredictor` can learn a new loudness mapping.

## 5. GPU feasibility (VERIFIED, `bench_step.py`)

Setup: RTX 4070 SUPER 12,282 MiB (1.9 GiB already used by the desktop), torch 2.6.0+cu124, **full fine-tune of all 13.6 M parameters with AdamW** (the upstream script uses AdaBelief, same state size: two moments) (the worst case for memory), dummy Gaussian audio and random note lists (loss values are meaningless). `model.train()` turns on activation checkpointing automatically (`Backbone.forward`: `useGradientCheckpoint or self.training`), so "gradient checkpointing on" is the default. Step time is the mean of steps 2 to 4.

| Segment | Batch | Precision | Peak allocated | Step time | Audio per second of training |
|---|---|---|---|---|---|
| 16 s | 1 | fp32 | 1.75 GiB | 0.52 s | 31 s |
| 16 s | 2 | fp32 | 3.50 GiB | 0.72 s | 44 s |
| 16 s | 4 | fp32 | 6.69 GiB | 1.42 s | 45 s |
| 16 s | 2 | bf16 autocast | 2.57 GiB | 0.75 s | 43 s |
| 16 s | 4 | bf16 autocast | 5.51 GiB | 1.06 s | 60 s |
| 8 s | 4 | bf16 autocast | 2.15 GiB | 0.55 s | 58 s |
| 8 s | 8 | bf16 autocast | 3.78 GiB | 0.85 s | 75 s |

- Everything fits in 12 GiB with large margin; a LoRA / LoKr run stores far fewer gradients and optimizer states than this full run, so these numbers are an **upper bound** for the adapters (activations dominate).
- bf16 saves about 20 % memory and 25 % time at batch 4. Its effect on the semi-CRF log-partition (`computeLogZ`) is **not checked**: ASSUMED stable, check by comparing the loss of one real batch in fp32 and bf16 (should agree within about 0.5 %); otherwise train in fp32 (the cost is small).
- Throughput: one pass over 50 h of audio at 16 s segments with 8 s hop (each second seen twice) costs about 100 h / 60 per second = **about 1.7 h** at batch 4 bf16 (ASSUMED linear scaling; real data adds dataloader time). 50 h dataset, 10 epochs is a night of training.
- No GPU unlock needed (the card is not the limit).

## 6. Data rate (VERIFIED, `ns4 render-patch`)

Command: `ns4 render-patch <program.ns4p> --events ev60.json --velocity 100 --wav out.wav` on a random 350-note, 60 s phrase (stereo float32, **48 kHz**: the 23,035,052-byte file is 2,879,381 frames; Transkun needs 44.1 kHz, so the dataset tool resamples), 32 logical cores. `target/release/ns4.exe` of the main checkout built before this session; binary freshness vs `main` not checked.

| Program (engine) | Wall for 60 s audio | Real-time factor | Note |
|---|---|---|---|
| Smooth Lead (backup load included) | 2.6 s | 27x | light synth |
| Leaky Stabs II (backup load included) | 14.9 s | 4.0x | |
| Grand Pad (backup load included) | 15.4 s | 3.9x | piano + synth |
| Horizon Whl, from `--backup` | 22.7 s | 2.6x | includes about 9 s to read the 3 GB `.ns4b` |
| Horizon Whl, extracted `.ns4p` (`ns4 backup <b> --extract`) | 13.3 s | 4.5x | heaviest tested |
| Horizon Whl, 8 s phrase | 1.5 s | 5.3x | start-up included |

| Parallel workers (Horizon, extracted, 60 s each) | Wall | Aggregate real-time factor |
|---|---|---|
| 8 | 15 s | 32x |
| 16 | 17 s | 56x |
| 32 | 23 s | 84x |

Conclusions:
- Extract each program once (`ns4 backup <backup> --extract <entry> <file>`) and render from the file: the backup load costs about 9 s per process.
- **About 80 h of audio per hour of rendering (measured end to end by the Phase 1 tool: 0.5 h in 30 s, section 6.1)** for the heaviest program, much more for light ones; a 50 h dataset costs under 40 minutes of CPU. Disk: stereo float32 is 0.35 MB/s; store 16-bit mono 44.1 kHz WAV (0.088 MB/s, 16 GB for 50 h) because Transkun reads integer PCM.
- **Dataset target:** start with **20 h** (about 1,100 programs x 65 s phrases is 20 h; many more phrase variants per program are cheaper than more programs), scale to 50 h only if held-out F1 is still rising. Programs matter more than hours: the factory bank has the synth, organ and piano programs; hold out whole programs for validation.

### 6.1 Phase 1 dataset (VERIFIED, PR #425, `tools/synth_dataset.py`, `tools/synth_ds_check.py`)

`extract` took 4 s for 384 programs. `render --hours 20 --workers 24 --seed 1` took **2,332 s (38.9 min) wall** with augmentation (about 31x real time aggregate; without augmentation 0.5 h took 30 s, about 60x), 6.0 GB of 16-bit mono 44.1 kHz WAV (about 300 MB per hour), stored in `C:/tmp/ns4-ft-data` (never committed; raw float renders are deleted per item).

| Item | Value |
|---|---|
| Items | 1,200 phrases of 60 s (about 60.4 s with tail): 20.18 h, **train 17.92 h, val 2.25 h** |
| Programs | 384 factory programs round-robin, 3 or 4 items each; **val = 43 whole programs, train = 341, overlap 0** (split by hash of the program name); includes the organ and piano programs (forgetting control; no engine tag, the mix is whatever the factory set holds) |
| Notes | 186,758 (2.57 per second), velocity median 59; histogram by 16 from 0: 15,449 / 27,015 / 29,352 / 29,572 / 28,399 / 25,477 / 20,579 / 10,915 |
| Pitch | octaves from MIDI 21: 268 / 18,442 / 37,367 / 43,931 / 43,448 / 34,926 / 8,376 / 0 (main range 36 to 96, 25 % of items 28 to 100) |
| Phrase kinds | mixed 237, repeats 234, arpeggio 262, chords 218, melody 249 |
| Velocity modes | skewed 634, loud 175, ramp 206, uniform 185 (ASSUMED mix) |
| Pedal | 24.2 % of items carry sustain CC 64 presses (ASSUMED 25 %); note offsets stay at key-up |
| Augmentation | tilt 60 %, codec round trip (AAC / MP3 / Opus via ffmpeg) 49.9 %, pink noise 49.9 %, final peak -24 to -1 dBFS; all ASSUMED ranges |
| Silent renders | 0 (raw peak under -60 dBFS counts as silent) |
| Alignment check (120 items) | for isolated notes the fundamental energy rises 6 dB at the listed onset in a median share of 1.0 of notes per item, median lag 3.4 ms; worst items are slow-attack organ and pad programs (zero or few isolated notes pass), not misalignments |
| Transkun note rules | `prepareIntervals` printed no merge message on the 30-item smoke set |

**Baseline of base Transkun 2.0 on the held-out split** (`tools/synth_eval.py`, 134 non-silent val items, 21,445 notes, 769 s on the GPU, match: exact pitch and onset within 50 ms, greedy one-to-one):

| Subset | Precision | Recall | F1 | Velocity MAE | Velocity r |
|---|---|---|---|---|---|
| all | 0.524 | 0.569 | **0.546** | **26.06** | 0.285 |
| arpeggio / chords / melody / mixed / repeats (F1) | | | 0.657 / 0.434 / 0.487 / 0.562 / 0.487 | 24.6 to 27.6 | 0.17 to 0.36 |
| vel mode skewed / loud / ramp / uniform (F1) | | | 0.512 / 0.459 / 0.685 / 0.592 | 24.3 / 29.0 / 27.4 / 29.1 | |

Predicting the val median (57) for every note gives a velocity MAE of 26.8 and uniform 100 gives 44.1, so the base velocity head carries almost no information on synth renders (r 0.28).

**Hardware-clip baselines** (`tools/emu_vs_hw.py`, `ns4` built from `origin/main` of 2026-11-15, default variant only; score = `gap_score`, lower is better; 'uniform 100' replays the video-detected notes at velocity 100, 'base Transkun' replays the committed `piano_events__zksT69H30c.json` stream):

| Clip | Uniform 100 | Base Transkun | Base Transkun recall at +-150 ms (video-confirmed notes) |
|---|---|---|---|
| Horizon Whl | 1.89 | 3.53 | 0.20 |
| Leaky Stabs II | 1.95 | 3.97 | 0.14 |
| Grand Pad | 2.19 | 1.43 | 0.25 |
| mean | 2.01 | 2.98 | 0.20 |

## 7. Plan, phases and commands

**Phase 1: data (done, PR #425).**

```bash
python tools/synth_dataset.py extract --backup ".reference/Nord Stage 4 Factory Restore RevK v1.66.ns4b" --out C:/tmp/ns4-ft-data
python tools/synth_dataset.py render --out C:/tmp/ns4-ft-data --hours 20 --workers 24 --seed 1
python tools/synth_ds_check.py C:/tmp/ns4-ft-data --sample 120 --out C:/tmp/ns4-ft-data/stats.json
C:/tmp/ns4-ft-venv/Scripts/python.exe tools/synth_eval.py C:/tmp/ns4-ft-data --split val --out C:/tmp/ns4-ft-data/base_val.json
```

**Phase 2: training (GPU), not started; waits for user approval of section 7.2.** Arms, same data, same seed, same step budget (about 20k steps at batch 4, 16 s segments, `max_lr` 2e-4 for adapters, 2e-5 for any full fine-tune, early stopping on held-out programs):

| Order | Arm | Trainable | Notes |
|---|---|---|---|
| 1 | LoKr r=8 | adapters (about 37 k measured on the Linear and Conv2d layers) + heads (0.86 M) | user's first choice; the adapter part is small, so if it underfits raise to r=16 or `decompose_both=True` inside the same arm before moving on (counts as the same arm) |
| 2 | LoRA r=16 | adapters (1.18 M with q/k/v) + heads (0.86 M) | only if LoKr misses the targets |
| 3 | full fine-tune | 13.6 M | only if both plateau |

Layer names in the regex are VERIFIED (`named_modules`). Mix per batch: 70 % synth renders, 30 % piano and organ renders (forgetting control; the dataset has no engine tag, so take the 30 % from the programs whose name contains Piano, Grand, Upright, Organ, B3 or Vox, ASSUMED). The piano path keeps using the base weights: the adapter is a separate `adapter_model.safetensors` loaded only for synth clips, so the base `2.0.pt` stays bit-identical (check by SHA-256 before and after).

**Phase 3: evaluation protocol.** Scripts: `tools/synth_eval.py` (held-out, base model today, `--adapter` in Phase 2), `tools/transcribe_agree.py` (hardware recall), `tools/emu_vs_hw.py --events-from <stream>` (replay score). Listening WAVs for the user go in the layout of `C:\tmp\listen-2026-11-15`.

### 7.2 Targets before training (PROPOSED, for user approval; derived from the baselines in section 6.1)

All six must hold for an arm to count as good enough. Numbers marked 'base' are measured; the targets are judgement calls (ASSUMED) and the user may change them before Phase 2 starts.

| # | Check | Base (measured) | Target (proposed) |
|---|---|---|---|
| T1 | Held-out programs (43 programs, 134 items): onset F1 at +-50 ms | 0.546 | **>= 0.75** |
| T2 | Held-out recall at +-50 ms (precision must not fall below base) | recall 0.569, precision 0.524 | **recall >= 0.75, precision >= 0.70** |
| T3 | Held-out velocity MAE (matched notes) and Pearson r | MAE 26.06, r 0.285 (median predictor: 26.8) | **MAE <= 13, r >= 0.75** |
| T4 | Piano non-regression: with the adapter **disabled** the transcription of every committed piano stream is bit-identical (SHA-256 of `2.0.pt` and of re-run `piano_events_*` streams); with the adapter **enabled** on emulator piano-class val items, F1 and velocity MAE are reported (informational, the adapter is not used for piano) | n/a | **bit-identical when disabled: pass or fail** |
| T5 | Hardware recall at +-150 ms vs video-confirmed notes (`transcribe_agree.py`) | 0.20 / 0.14 / 0.25 (Horizon / Leaky / Grand Pad) | **>= 0.40 on at least two of the three clips and no clip below base** |
| T6 | Replay score `emu_vs_hw.py --events-from <adapter stream>` (lower is better) | uniform 100: 1.89 / 1.95 / 2.19 (mean 2.01); base Transkun: 3.53 / 3.97 / 1.43 (mean 2.98) | **each clip <= its uniform-100 score and mean <= 1.80 (10 % better than uniform 100)**; also reported against base Transkun |

Rationale: T1, T2 ask for a clear step up on programs the adapter never saw (about +0.2 F1) without being impossible given reverb tails (base F1 by phrase kind runs 0.43 to 0.66). T3 asks to halve the base velocity error; ASSUMED because the dataset velocity is the emulator's input law, not a hardware truth. T5 doubles base recall on the two hardest clips. T6 is the project metric: replaying the adapter's notes and velocities must beat the uniform-100 replay that is used today.

### 7.3 Decision and stop rule

1. Train **LoKr** (arm 1, including its one allowed rank bump). Evaluate T1 to T6.
2. **All of T1 to T6 pass: stop.** No LoRA, no full fine-tune. Record the decision (next free D) with the scores.
3. T1 to T4 pass but T5 or T6 fail: **stop anyway** and report. Held-out emulator scores are good while the hardware ones are not, so the cause is the emulator-to-hardware gap (ASSUMED main risk), not adapter capacity; the next step is more augmentation or a better emulator, not another arm.
4. T1 to T3 miss while the held-out loss still improves (more than 1 % over the last 20 % of steps): continue LoKr for the same step budget once; if still missing, run **LoRA** (arm 2) with identical data, seed and budget.
5. Both arms miss T1 to T3 and the held-out loss has plateaued (under 1 % improvement over the last 20 % of steps, training loss still falling): run **full fine-tune** (arm 3). Forgetting is acceptable only because the base file stays separate (T4).
6. Budget cap: at most 2 runs per arm (one per rank setting). After that, stop and report the numbers.
7. Between arms, pick the winner by held-out F1, then velocity MAE, then hardware recall, then the smaller adapter; a tie within 1 F1 point and 1 velocity unit goes to LoKr (the user's first choice).

## 8. Risks

| Risk | Mitigation |
|---|---|
| Emulator-to-hardware domain gap (unverified synth voicing, filters, FX); the model learns the emulator, not the keyboard | augmentation; hold out programs; **check 3 on real clips** is the gate |
| Same synthetic "velocity" for hardware: the emulator velocity law is itself an estimate; a velocity head trained on it predicts emulator-velocity | velocity MAE only means something against the emulator; on hardware use the replay level match (check 4), not an absolute velocity claim |
| Reverb tails and overlapping notes cap recall | include reverb programs and long releases; report recall by program class |
| Pedal symbols rarely matter on synth clips | 24 % of items carry pedal presses (ground-truth note offsets stay at key-up); the pedal output is ignored in evaluation |
| `ncls` missing, `Data.py` indexing | own iterator (section 2.1) |
| Raw q/k/v with LoKr untested; reentrant checkpoint needs grad-requiring input | LoRA via `target_parameters`; keep `inputConv` adapted |
| bf16 and the CRF normaliser | compare fp32 vs bf16 loss on one batch before using bf16 |
| Weight licence uncertainty (MAESTRO lineage) | local only, non-commercial, no redistribution (section 3) |
| Scope creep: more tools than the project needs | the stop rule in section 7 |

## 9. Reproduce (probe scripts, local only)

`C:\tmp\ns4-ft-probe\inspect_model.py` (params, layer census), `lora_count.py` (adapter parameter arithmetic), `peft_check.py` (section 4, run with `C:/tmp/ns4-ft-venv\Scripts\python.exe -I`), `bench_step.py` (dummy-batch step: builds `Note` lists with no same-pitch overlap, calls `model.log_prob`, backward, `AdamW.step`, reports `torch.cuda.max_memory_allocated`). Run with `C:\tmp\ns4-transcribe\venv\Scripts\python.exe -I <script>`. Render timing: `ns4 render-patch` as in section 6, workers via `xargs -P`. These are throw-away measurement scripts and are not committed; the numbers above are the record.

## 10. Phase 2 results (VERIFIED runs; stop rule triggered)

Checked 2026-11-15. Runs: LoKr r16 (1,038,200 trainable) and LoRA r16 alpha 32, dropout 0 (1,869,812 trainable; PEFT rejects dropout with `target_parameters`, VERIFIED error), AdamW lr 2e-4, batch 4 x 16 s, seed 1, fp32, 3000 steps each (about 1.7 to 1.9 s/step). Tools: `synth_train.py` (`--resume`, `--schedule wsd`), `synth_eval.py`, `synth_vel_check.py`, `synth_hw_eval.py`, `scale_adapter` in `synth_adapter.py`. Local outputs (not committed): `C:/tmp/ns4-ft-runs/{lokr1,lora1,eval}`.

### 10.1 Held-out emulator split (134 items, 43 programs, step 3000, onset +-50 ms)

| Model | F1 | Precision | Recall | Velocity MAE | Velocity r |
|---|---|---|---|---|---|
| base | 0.546 | 0.524 | 0.569 | 26.06 | 0.285 |
| LoKr | 0.629 | 0.803 | 0.517 | 25.11 | 0.298 |
| LoRA | 0.748 | 0.841 | 0.673 | 22.84 | 0.463 |

`synth_vel_check.py` splits programs by Spearman rho between note velocity and onset loudness (weak = rho < 0.3: 128 of 379 programs; 39 of 134 val items, 6098 of 21,445 notes). LoRA responsive items: F1 0.813, velocity MAE 20.97, r 0.549; weak items: F1 0.549, MAE 30.2, r 0.100. Velocity MAE <= 13 is out of reach on the weak third because the render carries no velocity information there.

Targets: T1 missed (0.748 < 0.75), T2 missed (recall 0.673 < 0.75, precision passes), T3 missed (MAE 22.84, r 0.463).

### 10.2 Hardware clips (T5; recall at +-150 ms vs video-confirmed notes)

| Model | Horizon Whl | Leaky Stabs II | Grand Pad | Notes output (Horizon) |
|---|---|---|---|---|
| base | 0.203 | 0.146 | 0.233 | 257 |
| LoRA step 3000 | 0.006 | 0.042 | 0.082 | 6 |
| LoKr step 3000 | 0.012 | 0.035 | 0.062 | 19 |

T5 failed on all three clips and every clip is below base. The collapse is already there at LoRA step 500 (Horizon 0.012), so it is not progressive over-fitting.

Probes that did not explain it:

| Probe | Result |
|---|---|
| Input gain -12 to +18 dB | no change |
| Adapter strength 0.25 / 0.5 / 0.75 (LoRA, Horizon / Leaky / Grand) | 0.110 / 0.073 / 0.226; 0.052 / 0.062 / 0.212; 0.023 / 0.062 / 0.144; monotonic, never above base (s = 0 reproduces base exactly) |
| LoKr strength 0.25 / 0.5 / 0.75 | 0.157 / 0.111 / 0.233; 0.041 / 0.076 / 0.219; 0.017 / 0.059 / 0.144 |
| Val items sped up 2x (denser) | LoRA F1 0.748 to 0.620, base flat at 0.566; real but does not explain the collapse |
| Emulator replay of the clips' notes (`emu_default.wav`) | adapters also output only 3 to 6 notes |

T4 (piano bit-identical with the adapter disabled) was not run: the adapter lives in a separate folder and the base `2.0.pt` is untouched, but no SHA-256 check was recorded. T6 was not run: with 6 to 54 notes per clip the replay score is meaningless.

Stop rule 7.3 step 3 applies: held-out emulator scores up, hardware scores down. Decision: D115.

### 10.3 Dataset defects found after the runs

An independent read (Antigravity `agy`, plan mode, read only) listed defects. Status after my own check:

| # | Defect | Status |
|---|---|---|
| 1 | `synth_dataset.py` renders without `--backup`. Grand Pad: no backup = placeholder piano (peak 0.2339, RMS 0.0476, no sound refs printed); with `--backup "Program/Bank A/Grand Pad.ns4p"` = Astoria Grand + White Grand (peak 0.5072, RMS 0.0801). Agy counts 150 programs with piano layers and 113 with synth-sample layers affected, val included. **Effect measured (`C:/tmp/ns4-ft-probe/backup_probe.py`, 40 val items, no augmentation, +-50 ms): LoRA F1 0.762 without backup, 0.748 with; base 0.571 both. The placeholder sound is a small effect, not the cause of the hardware collapse.** | **CONFIRMED** defect, effect small (program counts SUSPECTED) |
| 2 | Labels are the input phrase; 124 of 384 programs shift octave or pitch, 48 use splits, 39 arp / pattern / hold, 30 are mono, so the sounded notes differ from the labels. LoRA F1 on the Horizon program's own training items is 0.00 to 0.06 (agy). | SUSPECTED |
| 3 | Phrase generator is single-stream (no overlapping two-handed polyphony); hardware clips run 4.9 to 8.2 confirmed notes/s against 2.6 mean (max 4.4) in the data. | density VERIFIED (section 6.1 stats), polyphony SUSPECTED |
| 4 | Hardware event streams (`vr/*.json`) carry chromatic clusters from low-frequency FFT resolution; the replay renders heuristic peaks too. The hardware "truth" is partly noisy, base numbers included. | SUSPECTED |
| 5 | Sustain pedal labels are trained though the default `sustped` leaves CC64 without effect on 234 synth / organ programs. | SUSPECTED |
| 6 | `SegmentSampler` does not sample boundary-padded segments (upstream begins in [-16, dur]); augmentation is weaker than upstream (no reverb, pitch, EQ). | SUSPECTED |

Checked clean by agy and not by me: note clipping at segment edges, `has_onset` / `has_offset` flags, program-hash split without overlap, `scale_adapter`, resume and optimizer state.

### 10.4 Next step (needs user approval)

The `--backup` fix alone will not close the hardware gap (section 10.3 item 1). A corrected dataset before any new arm: render with `--backup`, labels from the sounded notes (apply octave / split / arp or drop those programs), overlapping polyphony and 5 to 8 notes/s phrases, pedal labels only where the program honours CC64. Then rerun LoRA with the same budget. Not started.

## Implications

- Adapter training is practical on the existing GPU in hours, with rendering a small share of the budget.
- The data exist (20.2 h, 43 held-out programs) and the base baseline is measured; the next engineering step is an own dataset iterator plus the training script, then the approved targets decide when to stop.
- Success is decided on hardware clips, not on held-out emulator renders.

## Follow-ups

- [x] Confirm PEFT LoRA / LoKr on Linear, Conv2d and raw q/k/v (section 4).
- [x] Phase 1: `tools/synth_dataset.py` (extract, phrase generator, render, write dataset) with tests; 20 h.
- [ ] fp32 vs bf16 loss comparison on one real batch (not needed until a new run).
- [x] Phase 2 (LoKr, LoRA) and 3: both arms fail T5; recorded as D115 (section 10).
- [ ] Measure which of section 10.3 items 2 to 4 drives the hardware gap (label mismatch, density, noisy reference), then a corrected dataset (items 1 to 5) and one LoRA rerun; waits for user approval.
- [ ] T4 SHA-256 check of `2.0.pt` and re-run piano streams with the adapter disabled.
- [ ] Confirm the checkpoint licence with the upstream author if weights are ever to leave the machine.
