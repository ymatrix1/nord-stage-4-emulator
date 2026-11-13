# FX reverb mix, reverb type words, `kCmpResponse` and the Leslie status

- **Checked:** 2026-11-13
- **Sources (local, never committed):** the settled ARM snapshot and `unicorn` harness ([fx-delay-time](fx-delay-time.md) s2, `tools/fx_clients.py`), `capstone` listings of the `kCmpResponse` handler, `docs/research/data/fx-type-words.csv`. Derived data committed: [data/fx-reverb-amt.csv](data/fx-reverb-amt.csv) (128 captured word pairs), [data/fx-reverb-types.csv](data/fx-reverb-types.csv) (the per-type differing words).
- **Question:** what do `kRvbAmt` and the reverb type words write, why does `kCmpResponse` write no value-dependent word, and where is the Leslie?

Tags follow `.agents/rules/provenance-in-code.md`.

## 1. Short answer

| Item | Result | Tag |
|---|---|---|
| `kRvbAmt` (0..127) | two words per reverb module (`0xC02BB44C` words `-0x1F`, `-0x1E`; the type 0, 1 module `0xC02BB54C` words 18, 19): send `wet = x^2 (2 - x)`, `x = v / 128`, and dry `sqrt(1 - wet^2)`; at `v = 127` exactly `(1, 0)`. Equal power, max abs error 1.2e-7 over all 128 values | words VERIFIED, role EMPIRICAL |
| `kRvbType` words | types 2..11 write 64 words each; only 10 to 21 of them differ from type 2 (s3) | VERIFIED (capture) |
| `kCmpResponse` | the handler (jump table entry reached from `0xC010C990`, gate `0xC010D0AC`, body `0xC0012270`) **ignores the value**: it re-sends the four constants attack `0.102071`, `1.0`, release `4.607e-4`, `0.04` for 0 and 1, in every state tried (fresh, `kCmpEnable` 1, `kCmpAmt` 64, `Edt_Com_kEffectsCmpGlobal` 0 and 1). The 253-instruction trace is identical for both values | EMPIRICAL (4 states x 2 values; the jump-table path never reads the value) |
| Leslie | no code record in the 100 FX routines; the rotor runs from the ARM tick state machine and the ORG DSP image. Only `kEffectsLeslieDrv` (`v / 127`, words 3 and 7 of its row) is mapped | open (not attempted beyond [fx-contexts](fx-contexts.md) s6) |

## 2. Reverb amount

Reproduce (`tools/fx_clients.py` unit L3, one dispatch per value, about 5 s each):

```bash
uv run --with unicorn python tools/fx_clients.py CODE.bin SRAM.bin --cache target/settled.snap --units 3 --slots rvb.csv --slot Rvb
uv run pytest tools/tests/test_fx_reverb_amt.py        # closed form against fx-reverb-amt.csv
```

Reading of the series: `wet / x^2 = 2 - x` exactly at `v = 8, 16, 64, 96` (`x = v/128`: `31/16`, `15/8`, `3/2`, `5/4`), and `wet^2 + dry^2 = 1.00000` for every value. Rust: `fw_map::reverb_send` / `reverb_dry`, wired as `FirmwareFx::reverb_mix` (the dry and send of the engine reverb; the engine's own taps, decay and damping stay). Default off.

## 3. Reverb type words (types 2..11)

The block at DM `0x9500A..0x95033` (fixed point init data: `0x80000000`, `0x7FFFFFFE`, `-2.0 / 2.0` pairs) is the same for every type except the words below (types 0 and 1 write module `0xC02BB54C` instead); `0xB0F..` and `0xB1C26..` are the module state. Data: `fx-reverb-types.csv`.

| Type | `0x95013` / `0x95024` | `0xB0F0D` | `0xB0F1D` | `0xB0F20` | `0xB0F21` | `0xB0F59` / `0xB0F69` | `0xB0F5C` | `0xB1C26` |
|---|---|---|---|---|---|---|---|---|
| 2 | 262 | 0.21 | 0.1758 | 0.1258 | 0.7 | 98957 | 34.72 | 2.385 |
| 3 | 1324 | 0.21 | 0.1758 | 0.1258 | 0.7 | 98957 | 34.72 | 2.385 |
| 4 | 270 | 0.21 | 0.2629 | 0.1538 | 0.7 | 98957 | 34.72 | 1.95 |
| 5 | 1766 | 0.21 | 0.2629 | 0.1538 | 0.7 | 98957 | 34.72 | 1.95 |
| 7 | 278 | 0.28 | 0.5557 | 0.2982 | 0.6 | 197914 | 138.9 | 0.8049 |
| 8 | 920 | 0.35 | 0.7091 | 0.4211 | 0.5 | 98957 | 34.72 | 0.475 |
| 9 | 920 | 0.35 | 0.7091 | 0.4211 | 0.5 | 197914 | 138.9 | 0.475 |
| 10 | 1788 | 0.35 | 0.8688 | 0.4661 | 0.5 | 98957 | 34.72 | 0.3755 |
| 11 | 1788 | 0.35 | 0.8688 | 0.4661 | 0.5 | 197914 | 138.9 | 0.3755 |

Structure (EMPIRICAL, positions only): the types come in pairs of two (2 and 3, 4 and 5, 8 to 11 share the `0xB0F1D`, `0xB0F20`, `0xB1C26` voicing) and differ in a single length word (`0x95013`, `0x95024`: 262 against 1324, 270 against 1766, 920, 1788) and the LFO-like pair `0xB0F59` (98957 or 197914) / `0xB0F5C` (34.72 or 138.9, a 4:1 ratio). The 0.21, 0.28, 0.35 words rise with the size of the room, `0xB0F21` falls 0.7, 0.6, 0.5. Type 6 sends nothing (the default). The roles (decay, damping, lengths) are **not verified**: that needs an impulse response of the multi-tap routines `0x12314A`, `0x12329B`, `0x123ABE`, `0x12417E`, `0x12435C` in the oracle.

## 4. `kCmpResponse`

Dispatch trace of `Edt_Effects_kCmpResponse` (id `0x1AB`): `0xC00CC4F0` dispatch, `0xC010C990` jump table (`id - 0x18A` entries), `0xC010D0AC` (`cmp r3, #0; beq` to `0xC010D78C`), `0xC0012270` (the compressor setter: it loads the effect slot, then sends four words through the message queue in the block `0xC0012588`) with `r3 = 0` for both values. The second path (`0xC01F5CC0`) that would use a non-zero gate was never entered. So `kCmpResponse` has no DSP effect in OS v1.66 in the settled state: the engine's `comp_fast` keeps its legacy meaning, and the compressor words stay `CMP_ATTACK` / `CMP_RELEASE`.

## 5. Not done (stop point)

| Item | State |
|---|---|
| Multi-tap reverb routines | not modelled: five routines (`0x123ABE` / `0x12314A` / `0x12329B` / `0x12417E` / `0x12435C`, 155 to 207 instructions, `fx_pe.py` lacks `RSQRTS`); the type words above are the inputs to read |
| Oracle impulse responses of the reverb | not run, so no reverb network is ported and `emu_vs_hw.py` has no reverb segment |
| Leslie / rotary | locating the routine needs the organ unit with the ARM tick running the rotor state machine and the ORG image; not started |

## Implications

- The reverb dry / send law is exact; it is the only part of the reverb the Rust engine can follow the firmware on now.
- `kCmpResponse` is closed: no word depends on it.

## Follow-ups

1. Model `0x12417E` / `0x12435C` (types 0, 1, 39 words) first: smaller than the multi-tap pair and its words are all in `fx-type-words.csv`.
2. Oracle impulse responses for the types of s3, then read the lengths and gains.
3. Leslie: capture the rotor words over many ARM ticks per `kEffectsLeslieSpeed` value.
