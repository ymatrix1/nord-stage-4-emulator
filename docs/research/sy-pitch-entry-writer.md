# The SY pitch table entry `e`: who fills `DM(0x9354A + index)` (negative result, 2026-11-15; refill modelled, section 7)

- **Checked:** 2026-11-15
- **Sources (local, never committed):** OS v1.66 `bank0_CODE.bin` / `bank0_SRAM.bin` in the ARM unicorn harness (`target/keydown.snap`), the five DSP images cut from `bank0_CODE@0xC044667C..0xC05263A8` (SY1SY2 listing `target/oracle/sy2.lst`, SMP image `dsp_image_4_SMP.bin` through `ns4_dsp_raw.py` and `adi_dis.py`), the boot log `target/bulk/boot_log.jsonl` ([arm-bulk-tables](arm-bulk-tables.md)), the sample set `Carina Accordion 1_PS 4.1.nsmp4` from the local backup, new `tools/arm_sy_slot_probe.py` (`tools/tests/test_arm_sy_slot_probe.py`). Follows [synth-sample-kernel-rule](synth-sample-kernel-rule.md) sections 12 to 14 (D61, D65) and [synth-p-writer](synth-p-writer.md) (the method: find the table driven writer, then run it).
- **Question:** Which ARM routine (or other agent) writes the 16-bit packed float entry `e` that the SY audio ISR step 3 reads at `DM(0x9354A + index)`, so that `--sample-pitch fw` can be rebuilt on the written values and flipped to the default (D22)?

Tags follow `.agents/rules/provenance-in-code.md`.

## 1. Short answer

| Question | Result | Tag |
|---|---|---|
| Is there an ARM writer of `e` in the harness? | **No.** Over sample load (`0xC01D710C`), bind, two ticks and a SAMPLES note-on (voice update `0xC01E00AC` with the queue pump and the DSP status bit, `play_note`, 19 SY messages) no ARM message addresses `0x93500..0x935A0`; the boot log (290 SY, 1,540 SMP messages) has none either | VERIFIED (run, `tools/arm_sy_slot_probe.py`) |
| Is the entry in the DSP image? | **No.** The SY image block stream sets `DM(0x93580) = 0x80008000` (block at `bank0_CODE@0xC0501670`) and zero fill for `0x93581..0x9358D`; `0x9354A..0x9357F` are zero (flat image `target/oracle/sy.bin`, 54 words read back) | VERIFIED (image dump) |
| Does any ARM code or table name the window? | **No.** The 32-bit literals `0x93540..0x93591` occur in `bank0_CODE` only inside the DSP image streams (`0xC0501670..0xC05017B8` SY, `0xC051E28C..0xC051E35C` SMP); no instruction or rodata pointer outside the DSP images carries one. The port tables `0xC0547F78`, `0xC05482F8` and `0xC0548D78` entry 13 (`0x92196`) are the only SY targets (`arm-chan-tables.csv`, target 6: 97 rows) | VERIFIED (byte scan, table dump) |
| Does the SMP DSP write it? | No evidence. The SMP image has its own records at `0x93542`, `0x9354A`, `0x93578`, `0x93580` (pointer pairs `0x9404E, 0xE3A00`, a different table), and its program listing (`adi_dis.py` over short words `0x1C0300 .. 0x1C2800`, 3,618 instructions, incl. the ISRs at `0x1C0EE0`, `0x1C03FD`, `0x1C0A71`, `0x1C058C`) holds no operand in `0x93500..0x935A0` | VERIFIED (listing scan); SMP to SY link not checked (no SMP ISR run) |
| Is a table of semitone entries stored in the ARM image? | **No.** A scan of `bank0_CODE` for runs of 16-bit values whose unpacked ratio (`unpack_ratio`) steps by `2^(1/12)` (halfword stride 1 and 2, up and down, 5 or more steps) finds only two 32-bit integer tables with a geometric ratio of 1.06 (84 and 78 entries at `0xC02B9EC8` and `0xC0441134`, values 521 .. 63348 and 900 .. 64355, purpose not identified, not packed floats) and short accidental runs; none is a 16-bit packed float table | VERIFIED (scan) |
| What is new about the index? | The index is **not** the DSP reset value `0x36`: the ARM writes pair block word 0 itself (section 2). It is a per-layer slot that the ARM copies from a two-word slot object; in the harness the object holds `0, -1` and nothing writes it during load, bind or note-on | VERIFIED (run, listing) |
| Verdict | **Writer not found.** The default of `--sample-pitch` stays off; no engine change, no re-baseline (D89) | open |

## 2. The index chain (new, listing and run)

| Step | Where | What | Tag |
|---|---|---|---|
| DSP reads | SY ISR step 3, `0x1C0692 .. 0x1C06AF` | `r13 = DM(i7 - 0x20)` (word 0 of the pair block, `i7 = 0x921BE` for pair 0); `r4 = fext r13 by 0x400` is the index, `r1 = fext r13 by 0x410` is the field spec; `m5 = r4` and, **in the other SIMD lane**, `m6 = s4`; then `r12 = DM(m5, i5)`, `r12 = fext r12 by r1`, `funpack`. So the pair block word 0 of the even address (`0x9219E`) feeds lane x and the odd one (`0x9219F`) feeds lane y; the earlier "28 pairs" reading of section 13 is the same loop, two voices per pass | VERIFIED (listing) |
| DSP default | voice reset `0x1C0794` | `DM(rec + 0x16 - 0x20) = 0x36 \| 0x4000000`: index 54, spec `0x0400` (low half); `DM(0x93580)` holds `0x80008000`, the entry unpacks to a ratio the step clip rejects. This is a parking value, not a played one | VERIFIED (listing, run) |
| ARM overwrite | `0xC01E2B08` (voice object types dispatched at `0xC01E2AE8`: `[obj + 0x24] \| 0x4000000`), `0xC01E2C6C` / `0xC01E2D08` (`[obj + 0x28] \| 0x4100000`), store at `0xC01E2CC8` after `bl 0xC006CA48` (FIFO lock) | the ARM sends word 0 of the pair block (address `[table + 4]` of `0xC05482F8`, `0xC01E2C90`) as slot index OR spec; in the harness the first send is `0x9219E <- 0xFFFFFFFF` (the slot fields are still `-1`) | VERIFIED (run, pc `0xC01E2CC8`) |
| Slot source | voice update `0xC01E09D8`, `0xC01E09F0`: `[obj + 0x24] = [sp + 0x7C]`, `[obj + 0x28] = [sp + 0x78]`, set at `0xC01E22E8 / 0xC01E22EC` from the object `0x9062F0F0` (`[+0]`, `[+4]`) of the handle table entry `[sp + 0x154]` | after the voice update `[obj + 0x24] = 0`, `[obj + 0x28] = -1`: slot 0 for lane x, none for lane y, i.e. `word0 = 0x04000000`, entry `DM(0x9354A)` low half | VERIFIED (run: memory watches at `0x90670254`, `0x90670258`, `0x9062F0F0`) |
| Slot allocator | none seen | the slot object holds `0, -1` from the settled snapshot and no store hits it during sample load, bind, two ticks or the note-on (only the copy into the voice object at `0xC01D66C0`, the voice reset) | VERIFIED (run, watch `0x9062F0F0..F7`) |

Consequence for section 14 of [synth-sample-kernel-rule](synth-sample-kernel-rule.md): "the voice index stays `0x36` in all 28 pairs" holds for the DSP-side reset only. On the hardware the ARM replaces it by the slot of the layer; for the first SAMPLES voice it is **0**, so the DSP reads `DM(0x9354A)` (lane x low half), which is zero in the image: the ratio is 0 and the step is 0 until something fills it.

## 3. What was ruled out

| Candidate | Evidence | Result |
|---|---|---|
| DSP store (SY) | no store into `0x9354A..0x9358F` in the listing (section 14 of the kernel-rule note); the new check of the stage A states and the ISR loop shows only stores through the pair block pointer (`dm(0xffffffe4, i3)` etc.) | none |
| ARM opcode 0 message | none of the 19 note-on messages and none of the 290 boot messages | none |
| ARM opcode 1 to 15 message (handler call, SPI DMA opcode 9 `0x1C03AA`) | the ARM never sends `w1 >> 28 != 0` in the boot log or the note-on; the SPI master routine `0xC0066E64` (62 callers, including `0xC01E26F8`, `0xC01E296C`, `0xC01E30F4` in the type 4 event) is a request / answer transfer: the callers pass a DM address, a count and a stack buffer for the **answer** (DSP to ARM) | none seen; opcode 9 not driven by the ARM in any run |
| Pitch sender | the three copies `0xC01E8CCC`, `0xC01E8FCC`, `0xC01E92D4` write only the pitch word (`[table + 4] + 0x20`, `0x921A6`) and read the voice fields; word 8 is the key independent zone word (section 13) | not the entry |
| Table in rodata | no semitone table of packed floats (section 1) | none |

## 4. What stays open

| Item | Why it stays open |
|---|---|
| The writer of `e` | not an ARM message, not a DSP store, not in the SMP code. What the harness cannot run: (1) the ARM state after the DSP answers the prepare message (the status bit is set by hand, [synth-sample-kernel-rule](synth-sample-kernel-rule.md) s12), (2) the slot allocator that would give the slot object a real index, probably tied to the SAMPLES bank activation of the whole synth section (the object keeps its snapshot values `0, -1` in every run here), (3) an SPI DMA from the ARM or the SMP into the SY (opcode 9), (4) a DSP ISR other than the audio ISR (the vector sweep of `sharc-ref` stops at vector 0) |
| What `e` holds | still `word_law(key) / 0.05` only as a model (D65); the index chain says the table is per slot, not per key, so the value may also be a per-zone or per-note entry written together with the slot |

Reproduce:

```bash
uv run --with unicorn python tools/arm_sy_slot_probe.py target/fw_unpacked/bank0_CODE.bin \
    target/fw_unpacked/bank0_SRAM.bin --cache target/keydown.snap \
    --ns4b ".reference/Nord Stage 4 Factory Restore RevK v1.66.ns4b" \
    --member "Samp Lib/Accordion_Harm/Carina Accordion 1_PS 4.1.nsmp4"
uv run pytest tools/tests/test_arm_sy_slot_probe.py
```

The byte scans are plain: the 32-bit literals `0x93540..0x93591` in `bank0_CODE`, and 16-bit runs through `unpack_ratio` of `docs/research/sy-smp-audio.md` section 4 / `sinc::step_join`.

## 5. Follow-up checks (2026-11-15, review suggestions, each verified here)

| Check | Result | Tag |
|---|---|---|
| (1) xrefs of `0x9062F0F0` and what allocates it | `0x9062F0F0` is a heap address; no literal `0x9062F0F0` / `0x9062F0F4` in `bank0_CODE`. It is object 0 of a **pool of 56 slot objects** (`0x1C` bytes each, `0x9062F0F0 .. 0x9062F7D0`, list at `0xC0563664`) built at boot by `0xC01ED7FC..0xC01ED838`: `[+0] = 0`, `[+4] = -1`, the other five words 0 (stores at `0xC01ED81C`, `0xC01ED820`). 56 = the SY voice count. Users of the list literal `0xC0563664`: `0xC01E0FB4` (voice update), `0xC01E23CC` (`0xC01E23D0`, a per-event walk that calls `0xC01D69A4` / `0xC01D662C`, not an allocator), `0xC01E41B8`, `0xC01ECD74` (not read yet) | VERIFIED (run, byte scan, listing) |
| (1) trigger of `go` (`0x1C084C`) | the only ARM literals of the four stage A states are the ones listed in [sy-smp-audio](sy-smp-audio.md) s3; no ARM store of `0x1C084C` in any run here (the message list of the note-on ends with `0x92432 <- 0xBF800000`) | VERIFIED (run); trigger open |
| (2) bulk records spanning the window | the SY stream (`dsp_image_3_SY1SY2.bin`, 569 blocks) has **zero fill `0x93045 .. 0x93580` (1,339 words, tag 1)**, data `0x93580` (1 word), zero fill `0x93581 .. 0x9358E`, data `0x9358E .. 0x935D2` (68 words). So the table `0x9354A..0x9357F` is explicitly zeroed by the loader and must be filled at run time. Literals in `0x934A0..0x935A0` outside the DSP streams: none; `movw` / `movt` pairs with `#0x9` high half and a low half in `0x3400..0x35A0`: none | VERIFIED (block parse, byte scan) |
| (3) cold boot, write watch | `arm_bulk_tables.capture_boot` from the cold image (not `keydown.snap`), 96,111 port messages: **0** SY or SMP messages in `0x93500..0x935A0`; write hooks on all 56 pool objects over boot and 12 settle windows: **112 writes = the 56 constructor stores `0` and `-1`**, nothing else. So the allocator does not run in boot or in the settled main loop; it needs a user action not driven here (a SAMPLES layer activation beyond `bind_layer`, a program recall, or the real ring consumer) | VERIFIED (run) |
| (4) SY ISRs, opcode handlers 1 to 15 under `sharc-ref --watch` | **not run** (cost); the note-on message list has no opcode 1 to 15 word, so no handler is reached from the ARM side; an ISR other than the audio ISR writing the table is still possible | open |
| (5) other `.nsmp4` members with `[zone + 0x6C] == 1` | **not run**; the kernel-rule note section 12 captured one member only | open |

### Two observations of `0xC01E24F0`

[synth-sample-kernel-rule](synth-sample-kernel-rule.md) section 14 says the type 4 event `0xC01E24F0` sends no SY message **during the sample bind** (`load_sample`, `EDIT_SMP_ID`, `EDIT_MODE`, `bind_layer`, two ticks): at that point no note is on and the event is not in the queue. Section 2 above (and follow-up 2 of that note) describe the **note-on**: the key event queues the type 4 event first and the voice update (slot 1) second, `pump` delivers them in that order, so the event sends the voice reset (`0x92414 <- 0x1C0794`) and word 0 (`0x9219E <- 0xFFFFFFFF`) while the slot fields of the voice object are still `-1`; only then does the voice update copy slot `0` and `-1` from the pool object into `[obj + 0x24]`, `[obj + 0x28]` (`0xC01E09D8`, `0xC01E09F0`). Same code, different harness state (no note, then a queued note); the earlier statement is about bind, this one about note-on. Whether the hardware sends word 0 a second time after the voice update (value `0x04000000`) is not shown: the pump delivers each event once.

## 6. Second hunt (2026-11-15, follow-up task)

Question: who writes the SY pitch entry `e` at `DM(0x9354A + index)`, with ARM and SHARC only. Checked against the section 5 list and the outside-review ideas.

| Check | Result | Tag |
|---|---|---|
| Who gives a pool object its index | The note-on function `0xC01E31C8..0xC01E3DC0` writes the pool object: `[+0] = voice number k`, `[+4] = -1`, `[+8] = key`, `[+0xC] = layer type 4`, `[+0x10] = 0xFF800000`, `[+0x14] = 0x480000`, `[+0x18]` counter (stores at pc `0xC01E3AB8..0xC01E3AE0`, found with a write watch on the pool and the pc, not `lr`). The boot constructor writes only `0, -1`. So the table index is the **voice number** (0..55 for the pool of 56, stride `0x20`, not `0x1C`), allocated at note-on; the first SAMPLES voice reads `DM(0x9354A + k)` | VERIFIED (run, `arm_sy_slot_probe`-style watch) |
| Broad MIDI flow | note on / off, pitch bend, mod wheel, sustain on the sample rig: no SY or SMP message into `0x93500..0x935A0`; the ARM sends opcode 0 only | VERIFIED (run) |
| ARM packer | no ARM routine or literal that builds the 16-bit `E:5 / m:11` entry (capstone scan of `bank0_CODE` for the window literals and for the pack shifts) | VERIFIED (scan); a computed address is not excluded |
| SY / SMP DSP stores | SY listing: the only literal into the window is `i5 = 0x9354A` at `0x379` (the reader); the neighbours `DM(0x9358E)` / `DM(0x93590)` (stores at `0x29D..0x2B6`) lie past the table end `0x9358D`. No `FPACK` in the SY or SMP listings. The SMP image keeps its own records at `0x93542` / `0x9354A` (pointer pairs, a different table) | VERIFIED (listing grep) |
| SY ISR sweep under `sharc-ref --watch 0x93500:0xA0` | **blocked**: vector 12 stops at `0x1C0B33` (wait) and vector 17 inside the ring refill unpacker `0x1C1276..0x1C12DD` (2,000,000 steps, `unfinished`), because the external-port refill DMA and the unpacker pointer table `DM(0x95C7C)` are not modelled ([sy-smp-audio](sy-smp-audio.md) s5). No write into the window was seen before the stop | open |
| Other `.nsmp4` members, ring consumer `0xC01CE7D8`, `0xC01E41B8` / `0xC01ECD74` | not run | open |

Reading: the table is zero-filled by the image loader, the ARM only sends the index, and nothing the harness drives writes the value. Most likely the entry arrives through the unmodelled path (external-port DMA of a packed stream, or the ARM initialisation script `0xC0503600 ..` replay, [sy-smp-audio](sy-smp-audio.md) s5) which this harness does not run. That stays a hypothesis (ASSUMED) until the script is decoded and replayed.

Decision: D105. `--sample-pitch fw` stays opt-in.

### Follow-ups (replace section 5 follow-ups 1 to 3)

1. Decode the ARM initialisation script `0xC0503600 ..` (type 4 bulk records) and replay it with the unpacker table and the ring refill modelled; then repeat the ISR sweep with the watch.
2. Read `0xC01E41B8` and `0xC01ECD74` (remaining pool-list users) and run the ring consumer `0xC01CE7D8`.
3. Try the other `.nsmp4` members with `[zone + 0x6C] == 1`.

## 7. Third hunt: initialisation script replayed, ring refill modelled (2026-11-15, D111)

Question: with the firmware's own SY initialisation script in the DM image and the ring refill modelled, does any SY ISR or opcode handler write `DM(0x9354A..0x9358D)`? Tools: new `tools/ns4_arm_init_script.py` (`tools/tests/test_ns4_arm_init_script.py`), `sharc-ref` commands `refill`, `dump`, `regs` (`crates/nord-sharc/src/bin/sharc_ref/refill.rs`), scenarios `tools/oracle_scenarios/sy_isr_refill.scn` and `sy_isr_handlers.scn`, `tools/arm_sy_slot_probe.py --dump-messages --notes --idle-ticks`.

| Step | Result | Tag |
|---|---|---|
| The init script format | The records at `0xC0503600 ..` use the DSP image block format. Type 4 `[4][n][address][n values]` is a DM block write: the SY part is `0xC0503740 .. 0xC05038B0` (7 records: `0x93E0C`, `0x93E12`, `0x93E18`, `0x93E1E` (`-1` words, `1`), scan pointer `DM(0x95C69) = 0x92426, 0, 0x92426`, unpacker table `DM(0x95C7B) = 0x95C6D, 0x1C12DD x4, 0x1C14BD, 0x1C14B6, ...` (35 words), `DM(0x95CBC)` pattern words). **No record covers `0x93500..0x935A0`** (the records that do, from `0xC051E1E0`, are the SMP image stream). The type 1 records after it (`0xC05038B0 ..`) are not DM writes: their addresses are `0x36014`, `0x3601A`, ... (stride 6, an IOP block) with values `0xB64CB ..` (stride 1) and `0x93FF7`, `0x94007`, `0x94017`, ... (stride `0x10`): the transfer control blocks of the external-port DMA, source (SDRAM word) and destination (DM staging buffer) per voice | VERIFIED (parse, `tools/ns4_arm_init_script.py`: 53 SY words); TCB reading ASSUMED |
| Pointer table index | `DM(0x95C7C + 2 * count + k)`, `count = header bits 22..19 + 1` (15 bit packing at index 30, `0x1C12E7` / `0x1C12E0`); indices 0 to 3 and 32 to 35 are the trap `0x1C12DD`. A zero staging buffer reads header 0, `count = 1`, index 2: the trap, so replaying the table alone does not remove the hang | VERIFIED (run: vector 17 stops at `0x1C12DD` with the table replayed and `rec+0x10` not poked) |
| The unpacker stream | `B1 = DM(rec + 0x2F)` (`0x93FF7` after the prepare state), `L1 = 0x80` words, read pointer `DM(rec + 0x1C)`; records `rec + 0x13 = 7`, `rec + 0xA..0xF` come from the ARM note-on messages (`0x9240A <- 0xC`, `0x9240B <- 0x2C60`, `0x92413 <- 7`, ...) | VERIFIED (run, record dump) |
| Refill model | `refill VOICE` writes a block header word `(14 << 19) \| 0x40` into all 128 staging words before each interrupt, so a header is found wherever the reader is and the 15 bit handler runs. 400 audio ISRs after the `go` state run with no `rec+0x10` poke: the scheduler `0x1C11AC`, the queue unpacker `0x1C1228`, the header decode `0x1C1245..0x1C1270` and the handler `0x1C12E7` are executed (PC trace), no trap. The decoded audio is meaningless (synthetic page, ASSUMED); only the control flow is modelled. The real packed format (block length unit, selector bits 17..14) is not reverse engineered, the sweep does not need it | VERIFIED (run, `NS4_IRQ_TRACE=full`); model ASSUMED |
| SY ISR sweep, `--watch 0x93500:0xA0` | Vectors 0 to 31, each raised after the note-on sequence: no write into the window. Vectors 13, 16, 17 run to the idle loop; vector 20 runs `0x1C058C..0x1C05A0` and writes `DM(0x93590)` (past the table end `0x9358D`); vectors 12 and 18 to 31 never leave the idle loop; vector 1 enters the start-up code `0x1C0528` and waits at `0x1C0546` | VERIFIED (run) |
| Opcode handlers 1 to 15 | Each body run with `R6 = 0x9354A`, `R7 = 0x1234` (`sy_isr_handlers.scn`): all return, none writes the window (opcode 4 writes `DM(0x1820..0x1825)`, `DM(0x180B)`; opcode 6 stores to `0x4000000 + address`, a different space). **Oracle caveat:** `msg op=N` does not call the handler: the dispatch `if ne call (m8,i8)` at `0x1C064B` is the last instruction of the `do` loop `0x1C0635..0x1C064B` and the core does not take a call there (`I8 = 0x1C0304` is loaded, `break=0x1C0304` is never hit). The hardware behaviour of that instruction is not measured. The ARM never sends opcode 1 to 15 in any run | VERIFIED (run); core behaviour at loop end open |
| ARM side, long run | `tools/arm_sy_slot_probe.py --notes 36 48 60 72 96 60 --idle-ticks 3000` (Carina Accordion): 545 SY messages (33 of them in the idle ticks), **0** into `0x93500..0x935A0`; the slot object `0x9062F0F0` is still written only by the boot constructor and the note-on function | VERIFIED (run, 81 s) |
| `agy` second opinion (hypotheses only, not evidence) | Five hypotheses: loop-end call gap, DMA TCB targeting the table, unpacker handlers other than the 15 bit one, a main-loop routine, a two-hop ARM copy. Checked here: the init script holds 71 distinct DM-range words in the TCB region and none lies in `0x93500..0x935A0` (`0x935D2`, `0x935D3` are the nearest); the SY listing has one literal into the window (the reader, section 6); the 545 ARM messages address no copy source for it. Second `agy` pass (accept-edits mode, listing excerpt `0x1C12E0..0x1C23F0` only, report read back and grep-checked here): the 15 unpacker handlers store only through `I2` (the ring), the tail through `I3` (record), `I4` (`DM(rec+0x30)`, `0x93E36` in the run) and `DM(0x95C68)`; no literal in `0x93500..0x935A0`; no call or jump ends a do-loop in that span. So the other 14 handlers cannot write the table unless the ARM sends a ring or `rec+0x30` pointer into it (no such message). The loop-end call gap of the core stays open | checked (TCB, literals, handler stores); loop-end call open |

Reading: with the ring refill and the pointer table in place, the SY audio ISR, the other SY vectors that have handlers and the opcode handler bodies write nothing into the table, which is read as zero (`rec+0x2D`, the step word, stays 0 after 400 frames in `sy_isr_refill.scn`). The writer is not in the SY code, not in any ARM message this harness drives (545), and not in the image stream. Still not driven: (1) the real SDRAM stream through the external-port DMA TCBs (the entry could ride in the sample page; the TCB destinations seen are the staging buffers `0x93FF7 + 0x10 k`, not the table); (2) the ARM ring consumer `0xC01CE7D8` and the pool list users `0xC01E41B8`, `0xC01ECD74`; (3) other `.nsmp4` members (three more probed by an `agy` run with permission checks skipped at the user's request, `--notes 36 60 96 --idle-ticks 500`: Supreme Modern Fp 470 messages, Men+Women Oo Soft 470, Spitfire StringQuintet Stac 480, **0** into the window each; the members' `[zone + 0x6C]` values were not checked); (4) the missing loop-end call of the core (irrelevant while the ARM sends only opcode 0).

### 7.1 Follow-ups run by `agy` (permission checks skipped at the user's request; results re-run here)

| Item | Result | Tag |
|---|---|---|
| Ring consumer `0xC01CE7D8` (new `tools/arm_sy_ring_consumer.py`, `tools/tests/test_arm_sy_ring_consumer.py`) | One caller (`BL` at `0xC0019410`), `r0` = the synth strip object (`0x90674418` in the Carina Accordion run); `agy` calls it the strip apply routine. Called after load, bind and note-on it returns `1` and sends 393 port messages, **0** to SY or SMP, **0** into `0x93500..0x935A0` | VERIFIED (run twice, here and by `agy`) |
| Pool list users `0xC01E41B8`, `0xC01ECD74` | `0xC01E41B8` is the literal of the note-on function `0xC01E31C8` (already covered by the note-on run); `0xC01ECD74` lies in the synth constructor `0xC01EC9D8` that builds the 56 pool objects and sends no port message | VERIFIED (listing read by `agy`; note-on run here); constructor not re-checked |
| Loop-end call of the oracle core | Cause read from the source: `exec_flow` takes the call, then `finish_step` (`branch.rs`) sees `pc == loop end` and, while `remaining > 1`, replaces the target by the loop start and leaves the return address `0x1C064E` on the call stack. `agy` cites the programming reference (p. 4-55: no jump or call in the last three instructions of a loop) so the firmware pattern is outside the documented rules, and the handlers end in `rts (lr)` (loop re-entry), which the core does not model. Core left unchanged; behaviour on the hardware stays unmeasured. The ARM sends only opcode 0, so the pattern is never exercised | page citation not re-checked here; code reading VERIFIED by `agy`, not by a test |
| Opcode sweep with `msg op=1..15 0x93550 0x1234` after `sy_isr_refill.scn` | no write into the window (same as the handler bodies) | VERIFIED (run by `agy`) |

Decision: D111. `--sample-pitch fw` stays opt-in.

Reproduce:

```bash
python tools/ns4_arm_init_script.py target/fw_unpacked/bank0_CODE.bin   # the poke block of the scenarios
cargo build --release -p nord-sharc --bin sharc-ref
target/release/sharc-ref sy.bin tools/oracle_scenarios/sy_isr_refill.scn --out target/oracle/refill --watch 0x93500:0xA0
target/release/sharc-ref sy.bin tools/oracle_scenarios/sy_isr_handlers.scn --out target/oracle/hand --watch 0x93500:0xA0
uv run --with unicorn python tools/arm_sy_slot_probe.py target/fw_unpacked/bank0_CODE.bin target/fw_unpacked/bank0_SRAM.bin \
    --cache target/keydown.snap --ns4b ".reference/Nord Stage 4 Factory Restore RevK v1.66.ns4b" \
    --member "Samp Lib/Accordion_Harm/Carina Accordion 1_PS 4.1.nsmp4" --notes 36 48 60 72 96 60 --idle-ticks 3000
```

The scenarios embed only addresses, pcs and message words (D45); the sample data and the SDRAM stream stay local.

## Implications

- `--sample-pitch fw` stays opt-in (D89); `step_join::table_entry_for_key` remains a model. No default, golden or `render_hash` moves.
- Any future writer must be searched where the harness is blind: the slot allocator (who gives `0x9062F0F0 + 0` a non-zero slot and fills the table), the "go" state after the prepare answer, an SPI DMA into the SY.
- A slot index chain exists, so the entry is indexed per slot, not per key: if a writer is found, check whether `e` carries the zone rate ratio and the key law separately.

## Follow-ups

0. (D111) Drive the external-port DMA with a real packed stream (the TCBs of the init script, `0xC05038B0 ..`) and the ARM ring consumer `0xC01CE7D8`; check whether the table entry rides in the sample page.
1. Read the two unread users of the pool list `0xC0563664` (`0xC01E41B8`, `0xC01ECD74`) and the activation that gives a pool object a slot; the cold-boot watch (section 5) shows boot and the settled loop never write it.
2. Drive an SMP / SY ISR set in `sharc-ref` (vector sweep beyond vector 0) with the full message list and a table fill poked at `0x9354A`, to see whether the zero entry really mutes the voice.
3. Run opcode 9 (`0x1C03AA`) with a harness SPI model if the ARM ever sends it.
