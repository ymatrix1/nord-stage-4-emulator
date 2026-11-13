# B3 vibrato state, the processor tail and the organ level stage (ARM capture and oracle)

**Checked:** 2026-11-13
**Sources (local, never committed):** `tools/arm_edit_capture.py` (re-run, filter `kLayerLevelOrg|kVibratoType|kVibratoEnable|kLayerLevel`, booted ARM v1.66), [arm-edit-capture-detail.csv](data/arm-edit-capture-detail.csv), `sharc-pass` with the PNO image, [organ-b3-tail](organ-b3-tail.md), [b3-vox-video-gap](b3-vox-video-gap.md), [arm-organ-messages](arm-organ-messages.md). Only addresses, counts and derived gains are recorded.
**Question:** does the vibrato state (Edt_Org_Preset_kVibratoEnable `0x16C`, Edt_OrgCom_kVibratoType* `0x168..0x16A`) reprogram the PNO core 1 chain so that the tail response `G(f)` of [organ-b3-tail](organ-b3-tail.md) differs from the video's type-5 program, and does the ARM write a layer-level word ahead of the rotary?

## 1. Answer in short

| Item | Result | Tag |
|---|---|---|
| What the vibrato ids write | **type** `0x168` (B3, values 0..5), `0x169` (Vox 0..2), `0x16A` (Farfisa 0..3): two words per call, `DM 0xB44AE` and `DM 0xB44B0` (the second layer's pair `0xB452E`, `0xB4530` for the other store), all with value `0x00000000` in the harness; **enable** `0x16C`: value 1 writes `0xB44AE`; value 0 additionally clears 160 words, `DM 0x95331..0x9537F` and `0x95381..0x953CF` (two blocks of 79) plus `0xB4549`, `0xB454B`. The engine constructor and the model install leave `0xB44B0 = 1.0`, `0xB4530 = 1.0`, `0xB44AE = 0` | VERIFIED (re-run: 40 messages from 4 ids) |
| Why the values are zero | the handlers form the words from tables the harness does not load (SPI downloads are stubbed); the harness therefore shows which words are written, not the values of a type-5 state | VERIFIED (the same 0 for all types), cause ASSUMED |
| Effect of the flag words on the oracle chain | `DM 0xB44AE = 1.0`: **bit-identical** output; `DM 0xB44B0` from 1.0 to 0: -0.9 to -1.8 dB on the lines of 36..88 (`+0.9` at 440 Hz); `0xB44AE = 0xB44B0 = 0.5`: -0.5 to -1.3 dB. The response keeps its shape (0 dB at 65 Hz down to -24 dB at 0.9 kHz) | VERIFIED (run, 8 wheels, `organ_b3_proc.py measure`) |
| Effect of the 160 words `0x95331..0x953CF` | poking either block (or both) with the copy of the B3 key weights `0x951F1..`: **bit-identical** output (peak 0.00969, all eight line gains equal to the baseline to 0.1 dB). These weight arrays do not feed the `0x121E2B` chain of slot 4 in the replay | VERIFIED (run, `vb1`, `vb2`) |
| Conclusion for the tail | the vibrato enable / type words do not select another response of the `0x121E2B` chain: the table `b3-tail-gain.csv` holds for every state the harness can produce (flag effects at most 1.8 dB), so **no per-state tail table** is built and the Rust path is unchanged. The hypothesis of [b3-vox-video-gap](b3-vox-video-gap.md) (type-5 tables reprogram the tail) is **not supported** for the words that exist | VERIFIED (negative) |
| Organ level ahead of the rotary | `Edt_OrgCom_kLayerLevelOrgA/B` (`0xB1`, `0xB2`) produce **no message** in the capture (the float branch `0xC011AC38` is reached with no sender; re-run with the same filter: none of the 12 ids named level writes a word); the post stage `0x1229B3` and the mix stage have no level word the ARM sets (the `L` of OQ17 and `DM 0x95B42` are ASSUMED 1.0). The organ-to-rotary level stays an engine assumption | VERIFIED (negative) |
| Video B3 segment | the Rust configuration of [b3-vox-video-gap](b3-vox-video-gap.md) already runs vibrato type 5 (scanner `C2`) with the tail on; with no new state to select, the numbers are those of that note (mid -11.9 dB, sub +16.8 dB, crest 23.1 dB) | not re-run (no code change) |
| Default | **B3Wheels stays opt-in**: the oracle chord criterion holds on the strong lines of a single chord only (10 / 13, 8 / 13, 6 / 11) and the video mid band is 12 dB off | decision |

## 2. Method

```bash
uv run --with unicorn --with capstone --with numpy python tools/arm_edit_capture.py CODE SRAM --cache settled.snap \
    --filter "kLayerLevelOrg|kVibratoType|kVibratoEnable|kLayerLevel" --csv lvl.csv     # 40 messages, 4 ids
# oracle: arm_t0.script (constructor, install, key-down of keys 48, 65, 96), W zeroed except the
# eight clean notes, DM(0xB44C0) = 1.0, 6,000 passes, plus one extra poke script per state:
#   s1: 0 0xb44ae 0x3f800000      s2: + 0 0xb44b0 0       s3: both 0x3f000000
#   vb1 / vb2: 0x95331.. / 0x95381.. (79 words) = the W words of 0x951F1..
python tools/pno_full_pass.py run SRAM/core1_sram.bin --script arm_t0.script --script x_all.override.script \
    --script dry_gain.script --script s2.script --passes 6000 --out vs2
python tools/organ_b3_proc.py measure arm_t0.script x_all.override.json vs2.f32.npy
```

Gain per wheel line (dB, the table of [organ-b3-tail](organ-b3-tail.md) is the baseline column):

| Wheel note | baseline | `0xB44AE = 1` | `0xB44B0 = 0` (and `0xB44AE = 1`) | both 0.5 | `0x95331..` or `0x95381..` copied |
|---|---|---|---|---|---|
| 36 | +0.4 | +0.4 | -0.9 | -0.5 | +0.4 |
| 42 | -1.8 | -1.8 | -3.1 | -2.8 | -1.8 |
| 47 | -5.5 | -5.5 | -6.4 | -6.2 | -5.5 |
| 48 | -4.5 | -4.5 | -6.3 | -5.9 | -4.5 |
| 55 | -8.2 | -8.2 | -9.4 | -9.1 | -8.2 |
| 69 | -19.8 | -19.8 | -18.9 | -19.2 | -19.8 |
| 81 | -24.4 | -24.4 | -24.8 | -25.0 | -24.4 |
| 88 | -24.2 | -24.2 | -25.7 | -25.3 | -24.2 |

## 3. What this leaves open

- **The video's mid band.** The oracle chain with every state the harness can produce is dark above 0.5 kHz; the recording is not. Candidates not tested: the second `0x121E2B` / `0x122B85` slot pair (slots 5 to 7 of the B3 install list, the Farfisa processor shares `0x122B85`; they carry the second layer and possibly the scanner), the rotary and vibrato path of the **hardware** being a separate DSP (the effects DSP, not PNO core 1), and the hardware level stage after the PNO output. The B3 organ DSP output plus the effects DSP (rotary) is not a model of the audio at the codec until the PNO to FX routing and the layer level word are captured.
- **The values of the vibrato words.** They need the SPI-downloaded tables in the harness (`dsp-image-hunt.md`), then a re-run of this note's flag experiment with the real type-5 words.
- **Layer level ids `0xB1/0xB2`.** The handler's sender is not reached; a client for the organ layer level (probably a different dispatcher, `Edt_OrgCom_*` are `_private` names) is still to be found.

## Implications

- The tail table is state independent in the oracle: do not add a vibrato selector to `B3Wheels`; the 12 dB mid gap against the video has another cause.
- The organ-to-rotary level cannot be made firmware-based yet; the Rust rotary gain stays an assumption ([b3-vox-video-gap](b3-vox-video-gap.md) section 4).

## Follow-ups

1. Load the SPI tables in the ARM harness and capture the real vibrato type words (then repeat the flag experiment).
2. Replay the second B3 slot pair (layer B) in the oracle and compare with the video mid band; capture the PNO output routing into the effects chain.
3. Find the client that handles `Edt_OrgCom_kLayerLevelOrgA/B`.
