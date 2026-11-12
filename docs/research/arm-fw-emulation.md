# ARM firmware emulation: capturing real ARM to DSP messages

- **Checked:** 2026-11-12
- **Sources (local, never committed):** `target/fw_unpacked/bank0_CODE.bin` and `bank0_SRAM.bin` (OS v1.66; ARM mode, code base `0xC0000000`, RAM init block base `0x80000000`), the ORG image cut with `tools/ns4_dsp_images.py` and `tools/ns4_dsp_ldr.py --sram`, `crates/nord-sharc` (`sharc-org`), `capstone` 5 (`uv run --with capstone python tools/arm_xref.py dis ...`), `unicorn` 2.1 (pinned in the `dev` group of `pyproject.toml`). Prior notes: [arm-dsp-protocol](arm-dsp-protocol.md), [dsp-rx-protocol](dsp-rx-protocol.md), [org-stage-map](org-stage-map.md); the sender census of PR #70 (`arm-param-messages.md`, `tools/arm_params.py`) is not required to reproduce anything here. Derived data (addresses and small integers only, no firmware bytes): [arm-strip-setter.csv](arm-strip-setter.csv), [data/arm-capture-strips-q2.jsonl](data/arm-capture-strips-q2.jsonl), [data/arm-capture-controllers.jsonl](data/arm-capture-controllers.jsonl), [data/arm-replay-strips-org2.msg](data/arm-replay-strips-org2.msg), [data/arm-replay-controllers-org2.msg](data/arm-replay-controllers-org2.msg).
- **Question:** Can the host firmware itself be run (in pieces) so that every ARM to DSP message is captured as `(target, address, value)` from real code instead of from a static reading, and can the captured messages be replayed into the DSP images?

Tags follow `.agents/rules/provenance-in-code.md`: **VERIFIED** = reproduced by the command shown, **EMPIRICAL** = measured but not proven, **ASSUMED** = structural reading.

## 1. Short answer

| Question | Result | Status |
|---|---|---|
| Harness | `tools/arm_emu.py`: ARM926EJ-S on `unicorn`, code at `0xC0000000`, `bank0_SRAM.bin` at `0x80000000`, auto-mapped zero pages, stubs, fake DSP comm objects, port-level message capture. 36 unit tests with hand-assembled ARM words (`tools/tests/test_arm_emu.py`, `test_arm_capture.py`), none needs the firmware. | VERIFIED (tests) |
| Static initialiser | `0xC00002A4` runs **unmodified** (calls included, no stub) in 0.1 s and touches no unmapped page: it is self-contained. | VERIFIED |
| Real messages captured | Strip setter `0xC0017088`: 56 of the 66 ids send messages from a zeroed object, 3975 messages over 9 sample inputs (ORG core 1+2, PNO core 2 per call). Controller-word senders `0xC01CE034/0xC01CE128`: 96 calls, 96 messages. | VERIFIED |
| Replay | `tools/arm_capture.py script` writes `sharc-org --msg-script` files. Replayed into ORG core 2 the strip burst installs a new stage module id and ~140 field writes, the controller burst fills the packed words `DM(0xC1DCC..)`; the strict pass stays at "skipped ops 0" (section 7). | VERIFIED |
| Organ drawbar / vibrato / percussion / rotary / key-on messages | **Still not captured.** Instead recovered: the edit-parameter ids (`Edt_Org_Preset_kDb1..9` = `0x16D..0x175`), the organ layer class that subscribes to them, and the UI manager that builds the 48 controller objects (section 6). The full boot (RTOS, timers, edit framework) is not emulated; section 5 says exactly where it stops. | open |
| Real `.ns4o` preset through the preset-load function | not done (needs the edit framework and its object graph, section 5) | open |

## 2. Harness (`tools/arm_emu.py`, `tools/arm_capture.py`)

Memory map used (the firmware's MMU is **not** enabled, see section 5):

| Region | Content | Source |
|---|---|---|
| `0xC0000000..0xC1000000` | `bank0_CODE.bin` at offset 0, zero beyond it (BSS, heap statics up to about `0xC05C0000`) | VERIFIED (addresses used by the senders) |
| `0x80000000..0x80100000` | `bank0_SRAM.bin` (`0xFC80` bytes, the `SRAM` segment) then zeros; holds `.data` such as the critical-section counter `0x80001DB8` and the tick `0x80001DC0` (roles ASSUMED from use) | VERIFIED (segment table, `ns4-firmware-teardown.md` s2) |
| `0x700F0000` | stack; `0x70100000` return sentinel | harness choice |
| `0xA0000000..0xA0020000` | **fake DSP comm objects and ports** (harness only) | harness choice |
| everything else | zero page mapped on first access (logged in `auto_mapped`) | harness choice |

**Capturing messages.** The senders reach a DSP through objects that the boot code builds; the harness replaces them:

- `[0xC055B150]` (port object) points to a fake block with the port pointer of comm `c` at `+0x40 + 4c` and the pad byte at `+0x90 + c`;
- the comm table `0xC053F8A4` (entry `c + 1`) points to fake path 1 objects with the port at `+0x10` and flow control state at `+0x20`;
- flow control `0xC006CA48` is stubbed to return 1.

Every store to a fake port is logged and grouped: path 2 ports take `(address, [0 if pad], value)`, path 1 ports `(word1, word2)`. Both the writer wrappers (`0xC0062D28/60`, `0xC0062E10/78`) and the **inlined** senders (the controller senders and most large senders store to the port themselves) end in the same stores, which is why capture sits at the port, not at the wrappers. The first version of the harness hooked the wrappers only and saw nothing from `0xC01CE034` for that reason. The pad byte is `1` for comm 0..2 and `0` for comm 3, 4 (ASSUMED from the DSP side, [arm-param-messages](arm-param-messages.md) s2); the comm to chip order `FX, PNO, ORG, SY, SMP` is ASSUMED as before.

Helpers for code that needs more of the system: `add_stub`, `skip_instruction`, `guard_null_calls` (call through a null function pointer or vtable returns 0), `enable_poll_breaker` (a device word read more than 40 times by one instruction is rotated through `0xFFFFFFFF, 0, 3, 1, 0x3F, ...` until the wait loop exits), `bump_new` (allocator stub). Commands:

```bash
FW=target/fw_unpacked
uv run --with unicorn python tools/arm_capture.py strips      $FW/bank0_CODE.bin $FW/bank0_SRAM.bin --jsonl strips.jsonl --csv strips.csv
uv run --with unicorn python tools/arm_capture.py controllers $FW/bank0_CODE.bin $FW/bank0_SRAM.bin --jsonl ctrl.jsonl
uv run --with unicorn python tools/arm_capture.py probe       $FW/bank0_CODE.bin $FW/bank0_SRAM.bin 0xC01CE034 0x80010000   # any function, r0..r3
python tools/arm_capture.py script ctrl.jsonl ORG 2 --scenario controller --sender 0xc01ce034 > ctrl_org2.msg
```

JSONL record: `{"scenario", "id" | "sender"+"index", "input_q16" | "value7", "messages": [{"chip", "comm", "path", "core", "w1", "w2", "dm", "pc"}]}`; `w1` is the system byte address (path 2) or DM word (path 1), `dm` the DM word in the receiving DSP (`(w1 - 0x28000000) / 4` core 1, `(w1 - 0x28800000) / 4` core 2, [arm-param-messages](arm-param-messages.md) s2), `pc` the storing instruction.

## 3. Strip setter `0xC0017088(this, id, &value)` (VERIFIED, [arm-strip-setter.csv](arm-strip-setter.csv))

`0xC0017088` is a method with a jump table at `0xC00170A4` (ids `0..0x41`, 66 handlers); 110 `bl` call sites, among them the default-value routine around `0xC01EE590` that calls ids `0x30, 0x2E, 0x2F, 0x32, 0x34, 0x35, 0x36, 0x1E, 0x1F, 0x20, 0x21, 0x23, 0x22, 0x0B, ...` with Q16.16 values on every strip object (`0xC05A0390[k]`, object `+0x14`), and the large layer handler at `0xC011E3xx`. `this + 0x68` holds the strip index `k` (sweep: only that word changes the target, `k = 3` moves every address by `3 * 0xF8`). Each call that reaches a sender writes the **same word three times**, one record per receiving core: PNO core 2 (comm 1, `0x28B8....`), ORG core 1 (comm 2, `0x2838....`), ORG core 2 (comm 2, `0x28B8....`), exactly the `(3, 4, 5)` record triple of [arm-param-messages](arm-param-messages.md) s3. VERIFIED by the captures (every record of `arm-capture-strips-q2.jsonl` has this shape for strip fields).

The value is read as **Q16.16 and rounded to an integer** (`(v + 0x8000) >> 16`, visible in the handler at `0xC0017A44`: `add r2, r2, #0x8000; asr r2, r2, #0x10`): `0.5` and `1.0` give the same output in the table. With integer `n` the encodings seen are:

| Encoding (measured on inputs `n = 0, 1, 2, 4, 8, 16, -1, -16`) | Ids (strip word, `abs` = absolute DM word) | Status |
|---|---|---|
| stage module id: the constant `0x00120CD0` written to **word 0** whatever the value | `0x00, 0x05..0x0A, 0x10, 0x11, 0x38, 0x39` (several also write per-slot records `0xC1B3C..0xC1B46`) | VERIFIED capture, role ASSUMED. `0x120CD0` is not among the module ids of [org-stage-map](org-stage-map.md) s3/s4 (`0x12082F`, `0x120C68`, ...), and the DSP default is `0x80120BF7` |
| float `-2^(2n+1)` (`n = 0` gives `-2.0`, `1: -8`, `2: -32`, `4: -512`, `-1: -0.5`) | `0x0B, 0x0C, 0x0E` words 32, 33, 34; `0x12, 0x13, 0x15, 0x18, 0x19, 0x1B` words 26..31; `0x32` word 55 | VERIFIED capture, meaning ASSUMED (gain-like, 12 dB per step) |
| float `-0.5 + n / 127` (7-bit value to a bipolar `[-0.5, 0.5]`, `n = 4` gives `0xBEEFDFC0`) | `0x03` `abs 0xE02EE`, `0x0D` `abs 0xE02F6`, `0x14` `abs 0xE02F2`, `0x1A` `abs 0xE02F3`, `0x28` `abs 0xE02EF` | VERIFIED capture (fits 5 of 5 ids to the printed digits) |
| float `-60.5 + n` and `-59.5 + n` (dB-like) | `0x3A` word 48, `0x3B` word 50 | VERIFIED capture |
| float `1 / (n + 1)` and `-1 / (n + 1)` | `0x3C` word 49, `0x3D` word 51 (the default strip's `1/12` and `-1/6` are `n = 11` and `n = 5`) | VERIFIED capture |
| integer `n << 24` and `n * 167772` (`0.01` in Q8.24) | `0x01`, `0x02` `abs 0xE02E8` | VERIFIED capture |
| integer or flag | `0x31` word 36 (`n`), `0x17` word 39 (`n == 1`), `0x26/0x29` word 1 (`0x001253BD`, constant) | VERIFIED capture |
| float curve `0xC1390..0xC1393` (cos-like, `1.0 -> 0.9887 -> ...`) | `0x40`, `0x41` | VERIFIED capture, shape not fitted |
| burst into per-slot records `0xC1088..0xC10A7` (32 words) | `0x30` | VERIFIED capture |
| writes to PNO core 1 only | `0x1F, 0x22, 0x24` | VERIFIED capture |

The remaining 10 ids send nothing from a zeroed object (guards on `this + 0x1B0` and similar sub-objects; several only store into a smoother and send later from a periodic update). Strip words 26..34, 48..55 of this table are exactly the words the ORG setup code reads ([org-stage-map](org-stage-map.md) s3: 26, 28, 30, 32, 34, 37, 39, 40, 48..51, 53, 55, 57), which raises the earlier "9 of 16" overlap from weak to confirmed for 26..34 and 48..51. **None of these ids is an organ drawbar, vibrato, percussion or rotary parameter**: the strip setter is the channel-strip (mixer / modulation slot) interface, consistent with [org-stage-map](org-stage-map.md) s5.

## 4. Controller words (VERIFIED)

`uv run --with unicorn python tools/arm_capture.py controllers ...`: object `{[0] = index, [8] = state}` with `state = (value7 << 8) | 0x20`, 48 slots, both senders. Result for `0xC01CE034` (the other sender differs only in the state word it rewrites):

| Index | Message | Chip / core |
|---|---|---|
| `0..15` | `(0x28B07730 + 4 i, 0, state)` | PNO core 2 (comm 1), `DM(0xC1DCC + i)` |
| `16..31` | `(0x28307730 + 4 (i-16), 0, state)` | ORG core 1 (comm 2), `DM(0xC1DCC + i - 16)` |
| `32..47` | `(0x28B07730 + 4 (i-32), 0, state)` | ORG core 2 (comm 2), `DM(0xC1DCC + i - 32)` |
| `48..` | nothing (`cmp r2, #0x2F; pophi`) | |

One message per call, not three: the controller senders pick one record of table `0xC05495C8`. State after the call: `0xC01CE034` gives `(state & ~0x7000000 & ~0xFF) | 0x4000000` (value `0x55`: `0x04005500`), `0xC01CE128` gives `state & ~0x7000000` (`0x00005420`). So the packed word of [org-stage-map](org-stage-map.md) s3 is `(value7 << 8) | (kind << 24) | ...` with the type bits 5..7 cleared by `0xC01CE034`. This closes the "format" half of Follow-up 3 of [arm-param-messages](arm-param-messages.md); the mapping from physical control to slot index is **not** closed (section 6).

## 5. Boot attempt and where it stops (VERIFIED observations)

Goal: build the real object graph (edit framework, organ layer class, UI manager) by running the application constructor `0xC0247B64` (called through the thunk at `0xC025D524`), then call the real preset-apply path. Steps taken, each needed for the previous one to get further:

| # | Obstacle | Fix in the harness |
|---|---|---|
| 1 | `UC_ERR_EXCEPTION` on the first data stores after `0xC0024624` | the code at `0xC00244xx` writes the cp15 control register (`mcr p15,0,r0,c1,c0,0`, MMU and cache enable, ASSUMED) without the harness having built the firmware's page tables; the instruction is skipped (`skip_instruction`). The CPU model is set to ARM926 (the default model was not evaluated further). |
| 2 | `operator new` `0xC025D56C` (calls `0xC026BA80`) | `bump_new` |
| 3 | PSC power-domain waits (`0xC0069C80`, `0xC0069CBC`, `0xC0069CCC`) | `enable_poll_breaker` |
| 4 | DSP image download `0xC0241C58` (SPI waits) | stubbed to return 0 (success) |
| 5 | `b .` at `0xC0247C14` after two queries of `0xC024BF70` (board configuration object: needs byte `[+0x12]`, `[+0x13]` set and the two values equal and not 2) | stub that returns 1 and writes 0 |
| 6 | virtual calls on objects that were not built (`0xC0247494`, `0xC0247D18`, `0xC006A260`, 104 in total) | `guard_null_calls` |

After these the init runs through the CPU/clock/DSP bring-up and spends the rest of any budget (300 M instructions) in countdown loops and a loop that sends something through `0xC0066E64 -> 0xC006CA48` repeatedly while waiting for a DSP answer. **Next stubs that would continue** (not done): (a) a timer model: delay loops read the 64-bit tick at `0x80001DC0` (`ldrd` at `0xC0080684`), it must advance per read; (b) the DSP handshake (`0xC006CA48` consumers) must report "acknowledged"; (c) the interrupt vector table in `bank0_ARM0.bin` (IRQ handler at `0x6C`, which dispatches through `[0x...+0xD04]`) with a periodic timer IRQ for the RTOS tick, and the task creation calls; (d) the edit framework's registration function (`0xC01A44C8`, thousands of `0xC00F0FFC` calls) must have run before any edit id can be set. Because (c) and (d) mean implementing an RTOS scheduler and its message queues, the preset-apply path was **not** reached; the harness instead calls the high-level senders directly (sections 3 and 4).

## 6. Where the organ messages must come from (disassembly, VERIFIED unless noted)

New facts found while looking for the drawbar sender, with the exact addresses so the next pass can start from them:

- **Edit parameter names and ids.** The registration function `0xC01A44C8` (no direct caller; 122 calls of `0xC00F0FFC(ctx, id, name, ...)`, 246 of `0xC00F1030`, 21 of the buffer variant `0xC00F10C0`, all in `0xC01A4568..0xC01ABB3C`) registers the parameters; the organ block is `Edt_Org_kModel`, `Edt_Org_kPresetEnable`, the buffer `Buffer_kOrgDrawbar` (registered with id `0x0C` via `0xC00F10C0`), then `Edt_Org_Preset_kDb1..kDb9` with ids `0x16D..0x175` (name pointers at `0xC02D694C..0xC02D69EC`, id literals at `0xC01A8664..0xC01A869C`), then `Buffer_kOrgVibPerc` and `Edt_Org_Preset_kVibratoEnable`, `kB3PercEnable`, `kB3PercHarmonic`, `kB3PercSpeed`, `kB3PercLevel` (strings `0xC02D6A00..0xC02D6A90`). The two live-buffer names `OrgDrawbarLive` and `OrgVibPercLive` (`0xC02D3544/54`) are registered at `0xC01A1818/0xC01A1850` through `0xC00F37C8`. The descriptor of each drawbar holds `8` (ASSUMED to be its maximum, i.e. the 0..8 drawbar scale of the `.ns4p` field, not a DSP unit).
- **Organ layer class.** Constructor `0xC0019BAC` (one `bl` site) subscribes to the edit ids with `0xC00D57F0(this, id)`: the nine drawbar ids `0x16D..0x175` appear as nine consecutive subscriptions at `0xC0019D88..0xC0019DE8`, followed by `0x16C, 0x16B, 0x177, ...`. Its vtable is `0xC027AD8C` (slot 1 `0xC011CB60` is the change handler: it compares the edit id against `0x8E, 0x35, 0x8D, 0x166, 0x16C` and forwards everything else through a per-layer sub-object with a virtual call at `+0x20`). The method that calls the strip setter is the big switch at `0xC011E3xx` (same address region as the handler; that it belongs to the same class is ASSUMED); it reads the edit value with `0xC00D04D0` and calls `0xC0017088` through `[this + 0x9C] + 0x14`.
- **Controller objects.** The UI manager init (`0xC01EC9D8`, one `bl` at `0xC02499A8`, which lies after the application constructor `0xC0247B64` by the nearest-prologue rule, so inside it or a sibling) builds 48 controller objects in the loop at `0xC01EE3B0..0xC01EE424` (size `0x48`, `[0] = i`, `[0x30] = i`, `[0x34] = i`), registered in `0xC0565CD0` and `0xC0566D18` (pointer to `obj + 0x30`, the object the controller senders receive). The scan function around `0xC01E3C78..0xC01E3DA4` walks a list of control ids ("changed controls" is ASSUMED from the sender use), reads the type bits (`state >> 5 & 7`) and calls the senders. So controller index `i` is the **order of creation** (0..47), and the mapping drawbar to index is decided by whoever fills the changed-control list from the front panel scan: not found.
- **Organ weights.** [organ-tonegen](organ-tonegen.md) places the tonewheel bank in PNO core 1; channel table "A" (`0xC0548FD8`) rows 10..13 are target 2 (PNO core 1 by the comm map; bases `0x2838C618`, `0x2838D5F8`, `0x2838DD28`, `0x2838E2F0`, sizes `0x201 ...`, tags `0x08110401 ...`); the senders of that table are `0xC002AC5C`, `0xC02040DC`, `0xC0208B5C` (signature `(this, a, b, c)`, `this[1] < 14` is the channel). Not driven here. ASSUMED that the drawbar mix reaches the DSP through such a block write (the name `Buffer_kOrgDrawbar` says "buffer").

Why the harness cannot answer this today: every link of the chain edit id to message is a virtual call on an object that only the full boot builds (section 5). With a working boot the next experiment is one call: `set edit value (id 0x16D, 5)` and read `emu.messages`.

## 7. Replay into the ORG image (VERIFIED, `sharc-org` strict mode)

```bash
python tools/ns4_dsp_images.py target/fw_unpacked/bank0_CODE.bin --out DSP
python tools/ns4_dsp_ldr.py DSP/dsp_image_2_ORG.bin --sram SRAM
cargo build -p nord-sharc --release --bin sharc-org
O="SRAM/core2_sram.bin --dev 0x282c0380=1 --dev 0x3108d008=5"
python tools/arm_capture.py script docs/research/data/arm-capture-strips-q2.jsonl ORG 2 --scenario strip > strip_org2.msg   # or the committed .msg
target/release/sharc-org $O --passes 6 --msg-base 0 --msg-script strip_org2.msg --dump 0xE0000:64
```

`--msg-base 0` because the script's word 1 is the absolute DM word (`dm` field). Results, ORG core 2, 6 passes:

| Run | Pass 0 steps / words changed | Pass 5 steps / words changed | `DM(0xE0000..)` (strip 0) |
|---|---|---|---|
| default image | 4264 / 356 | 4265 / 87 | `80120BF7 80125B79 0 0 ...` (null stages) |
| strip burst (140 messages, n = 2) | 4258 / 428 | 4259 / 262 | `00120CD0 001253BD 000025EC FE7EBAD6 ... C2000000 x9 ... 0 2 80000000 AAAAAAAA ... C26A0000 3EAAAAAB C2660000 BEAAAAAB 3D70A3D 3E809BCC ...` |
| controller burst (16 messages) | 4264 / 382 | pass 2: 4265 / 96 (default 91) | `DM(0xC1DCC..)` goes from all zero to `4000300 4000B00 4001300 4001B00 ... 4007B00` (`0x04000000 | slot-value << 8`) |

The strip burst replaces both null stage pointers of strip 0 (`0x80120BF7`, `0x80125B79`) by the module ids the ARM sends (`0x120CD0`, `0x1253BD`), fills the pow4 gain words `26..34` with `-32.0` (`0xC2000000`) and the words `48..51` with `-58.5, 0.333, -57.5, -0.333`, i.e. the strip array looks like a configured channel, and the pass still runs strictly (`skipped ops 0`). In the controller run a handful of words differ at pass 2 against the default run (7 by a rough parse of the two `--diff` listings, for example `DM(0xC0BCC)` `0x3F800000 -> 0x3E800000` and `DM(0xC0BE8)` `0 -> 0xB069B`), so the packed words do steer the ORG code. Which of those words is a drawbar or tonewheel level is not decided, because the packed words were written with arbitrary test values; the stage type switch of [org-stage-map](org-stage-map.md) s3 needs bits 5..7 = 2, which `0xC01CE034` clears (bits 0..7 of the state); `0xC01CE128` keeps them, so call it with a state that has bit 6 set (`value7 << 8 | 0x40`) to select the `0x1208D6` stage.

## 8. Limits

- Messages come from isolated calls on synthetic objects (zeroed, only `this+0x68`, the controller index and state set). Fields guarded by sub-objects (10 of 66 strip ids) stay silent.
- Pad bytes and comm to chip order are ASSUMED (section 2); a different pad assignment would change message grouping for comm 0 (FX) and 3/4 (SY, SMP) only.
- The ARM runs with the MMU off, no interrupts, no RTOS, no timer; any firmware path that depends on them (everything above `0xC0247B64`) is untested.
- `capture` hooks stores to the fake ports, so senders that talk to the real EMIFA window `0x62000010` directly (not through `[0xC055B150]`) would be missed; none was found among the senders driven here.

## Implications

- `crates/nord-sharc` / a future DSP-side harness can be driven by real ARM bursts today: `arm_capture.py script` plus `sharc-org --msg-base 0 --msg-script`. A faithful organ patch still lacks the drawbar/percussion bursts (section 6).
- The strip words and their encodings (section 3) are now measured, which lets an engine model set the same fields: pow4 gains, 7-bit bipolar `n/127 - 0.5`, dB `-60.5 + n`, `1/(n+1)` ramps.
- The controller sender format is closed; the control-to-slot mapping is the open piece for drawbars.

## Follow-ups

1. Build the timer/handshake/RTOS stubs of section 5 (a) to (c) far enough for the application constructor to return, then run the edit registration `0xC01A44C8` and call the edit-set method with id `0x16D`; capture the burst for all nine drawbars, then `kVibratoEnable`, `kB3Perc*`, Leslie.
2. Trace the changed-control list filled by the front panel scan to map physical drawbars to controller slots `0..47`.
3. Drive the table "A" senders (`0xC002AC5C`, `0xC02040DC`, `0xC0208B5C`) to see the PNO core 1 block writes and compare with the tonewheel bank agent's DM addresses.
4. Apply the controller burst with `kind` bits 5..7 = 2 and diff `0xB0050..` (the `FPACK` output of stage `0x1208D6`) as listed in [org-stage-map](org-stage-map.md) Follow-up 2.
