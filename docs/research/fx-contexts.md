# FX clients and contexts (work in progress)

- **Checked:** 2026-11-13
- **Sources (local, never committed):** `target/fw_unpacked/bank0_CODE.bin`, `bank0_SRAM.bin` (OS v1.66), the `unicorn` harness (`tools/arm_steady.py`, `tools/arm_edit_capture.py`), `tools/fx_clients.py`, `tools/fx_contexts.py`.
- **Question:** where are the effect clients built, and which context words of the FX routines do the effect ids write?

## 1. Short answer (first push)

The effect clients are **not missing in the steady state**: all 6 FX parts and their 12 FX unit objects are built by the boot (hook on `0xC01177A8` / `0xC01155EC` at 433 M instructions). They are simply not in the dispatcher's client list, so `arm_edit_capture.find_stores` never saw them. `tools/fx_clients.py` finds them (`UNIT_VTABLE 0xC02BAD68`) and drives the dispatch `0xC00CC4F0(unit, 0, id, &value)` through them; the full write-up follows in this file.
