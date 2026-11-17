#!/usr/bin/env python3
"""Where do the ARM milliseconds go? Unicorn execution vs Python hooks vs the tick loop (D119).

Boots the full machine as `arm_org_stream.py` does (program in the Store record, board present),
settles it, then measures `--ticks` idle ticks of 200,000 ARM instructions in four ways:

1. `harness`  the normal loop (`arm_steady.run_ticking`), wall clock per tick;
2. `hooks`    the same with every Python hook wrapped in a timer: calls and seconds per hook, plus
              the timer interrupt injection (`inject_irq`) and the instruction chunking around it;
3. `nohooks`  the firmware code from the settled state with every hook removed (`hook_del`),
              raw Unicorn `emu_start(count=N)`: MIPS of Unicorn alone on the real code;
4. `ceiling`  a synthetic ARM loop (`add`, `subs`, `bne`) in a fresh Unicorn: the MIPS ceiling.

The split of one tick is `unicorn = nohooks time for 200k instructions`, `hooks = callback seconds`,
`rest = tick wall - unicorn - hooks` (loop, interrupt injection, wrapper overhead). The wrapper adds
about `2 * perf_counter` per call, so the callback seconds are an upper bound.

    uv run --with unicorn python tools/arm_profile.py target/fw_unpacked/bank0_CODE.bin \
        target/fw_unpacked/bank0_SRAM.bin --eeprom target/oled/eeprom.bin \
        "factory_programs/Program/Bank A/B3 Chords n Solo.ns4p"

Evidence and table: `docs/research/org-system-run.md` (ARM cost).
"""

from __future__ import annotations

import argparse
import statistics
import sys
import time
from pathlib import Path

TICK_INSNS = 200_000
# ARM (A32) loop: add r0, r0, #1; subs r1, r1, #1; bne -12 (back to the add): 3 instructions
# per iteration (little-endian words).
LOOP_CODE = bytes.fromhex("010080e2011051e2fcffff1a")
LOOP_BASE = 0x10000


def mips(instructions: int, seconds: float) -> float:
    """Million instructions per second."""
    return instructions / seconds / 1e6 if seconds > 0 else 0.0


def split_tick(tick_s: float, unicorn_s: float, hooks_s: float) -> dict[str, float]:
    """`{unicorn, hooks, rest}` seconds of one tick; `rest` is clamped at 0."""
    return {"unicorn": unicorn_s, "hooks": hooks_s, "rest": max(tick_s - unicorn_s - hooks_s, 0.0)}


def realtime_factor(tick_s: float, tick_budget_s: float = 1e-3) -> float:
    """Wall seconds per emulated second for a tick that must fit in `tick_budget_s`."""
    return tick_s / tick_budget_s


def summarise(samples: list[float]) -> dict[str, float]:
    """Mean, median and worst of a series."""
    return {
        "mean": statistics.fmean(samples),
        "median": statistics.median(samples),
        "worst": max(samples),
    }


def hook_table(stats: dict[str, list[float]], ticks: int) -> list[tuple[str, float, float]]:
    """`(name, calls per tick, ms per tick)` rows, most expensive first."""
    rows = [(n, c / ticks, s * 1e3 / ticks) for n, (c, s) in stats.items()]
    return sorted(rows, key=lambda r: -r[2])


class HookProfiler:  # pragma: no cover - needs unicorn
    """Wraps `unicorn.Uc.hook_add` so that every callback is timed and every handle remembered."""

    def __init__(self) -> None:
        import unicorn

        self.stats: dict[str, list[float]] = {}
        self.handles: list[tuple[object, int]] = []
        self.names: list[str] = []
        self.on = False  # timers only run while on: the plain harness is timed with it off
        self._orig = unicorn.Uc.hook_add
        profiler = self

        def hook_add(uc, htype, callback, user_data=None, begin=1, end=0, *rest):  # noqa: ANN001
            name = f"{htype:#x} {getattr(callback, '__qualname__', repr(callback))}"
            st = profiler.stats.setdefault(name, [0, 0.0])

            def wrapped(*args):  # noqa: ANN002
                if not profiler.on:
                    return callback(*args)
                t0 = time.perf_counter()
                result = callback(*args)
                st[1] += time.perf_counter() - t0
                st[0] += 1
                return result

            handle = profiler._orig(uc, htype, wrapped, user_data, begin, end, *rest)
            profiler.handles.append((uc, handle))
            profiler.names.append(name)
            return handle

        unicorn.Uc.hook_add = hook_add

    def reset(self) -> None:
        for st in self.stats.values():
            st[0], st[1] = 0, 0.0

    def remove_matching(self, text: str) -> int:
        """Removes the hooks whose callback name contains `text`; returns how many."""
        keep, gone = [], 0
        for (uc, handle), name in zip(self.handles, self.names):
            if text in name:
                uc.hook_del(handle)
                gone += 1
            else:
                keep.append(((uc, handle), name))
        self.handles = [h for h, _ in keep]
        self.names = [n for _, n in keep]
        return gone

    def remove_all(self) -> int:
        for uc, handle in self.handles:
            try:
                uc.hook_del(handle)
            except Exception:  # noqa: BLE001, S110 - already removed
                pass
        return len(self.handles)


def ceiling_mips(count: int = 30_000_000) -> float:  # pragma: no cover - needs unicorn
    """MIPS of a fresh Unicorn on the synthetic loop, no hooks."""
    from unicorn import UC_ARCH_ARM, UC_MODE_ARM, Uc
    from unicorn.arm_const import UC_ARM_REG_R1

    uc = Uc(UC_ARCH_ARM, UC_MODE_ARM)
    uc.mem_map(LOOP_BASE, 0x1000)
    uc.mem_write(LOOP_BASE, LOOP_CODE)
    uc.reg_write(UC_ARM_REG_R1, count)
    t0 = time.perf_counter()
    uc.emu_start(LOOP_BASE, LOOP_BASE + len(LOOP_CODE), count=count)
    return mips(count, time.perf_counter() - t0)


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - needs the local firmware
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    ap.add_argument("image", type=Path)
    ap.add_argument("sram", type=Path)
    ap.add_argument("--eeprom", type=Path, required=True)
    ap.add_argument("program", type=Path)
    ap.add_argument("--settle", type=int, default=800)
    ap.add_argument("--ticks", type=int, default=100)
    ap.add_argument("--raw-insns", type=int, default=20_000_000)
    ns = ap.parse_args(argv)
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    prof = HookProfiler()
    import arm_keydown as kd
    import arm_program_load as apl
    import arm_screens as sc
    import arm_steady as ast

    inject = {"calls": 0, "seconds": 0.0}
    orig_inject = ast.inject_irq

    def timed_inject(emu, irq=ast.TIMER_IRQ):  # noqa: ANN001
        t0 = time.perf_counter()
        r = orig_inject(emu, irq)
        inject["seconds"] += time.perf_counter() - t0
        inject["calls"] += 1
        return r

    ast.inject_irq = timed_inject
    session = sc.Session(ns.image, ns.sram)
    session.load_eeproms(apl.build_image(ns.eeprom.read_bytes(), ns.program.read_bytes(), "p"))
    kd.install_board_present(session.emu)
    session.boot()
    session.emu.install_dsp_ports()
    print(f"settling {ns.settle} ticks ...", flush=True)
    ast.run_ticking(session.emu, ns.settle * TICK_INSNS)
    # 1: plain harness (timers off), 2: the same ticks with every hook callback timed
    plain, profiled = [], []
    for _ in range(ns.ticks):
        t0 = time.perf_counter()
        ast.run_ticking(session.emu, TICK_INSNS, per_tick=TICK_INSNS)
        plain.append(time.perf_counter() - t0)
    prof.reset()
    inject.update(calls=0, seconds=0.0)
    prof.on = True
    for _ in range(ns.ticks):
        t0 = time.perf_counter()
        ast.run_ticking(session.emu, TICK_INSNS, per_tick=TICK_INSNS)
        profiled.append(time.perf_counter() - t0)
    prof.on = False
    hooks_s = sum(s for _c, s in prof.stats.values()) / ns.ticks
    print("plain tick wall (ms):", {k: round(v * 1e3, 2) for k, v in summarise(plain).items()})
    print("timed tick wall (ms):", {k: round(v * 1e3, 2) for k, v in summarise(profiled).items()})
    print(
        f"instructions per tick: {TICK_INSNS}, "
        f"plain harness MIPS {mips(TICK_INSNS, statistics.fmean(plain)):.1f}"
    )
    print(
        f"hook callbacks: {hooks_s * 1e3:.2f} ms per tick (timer overhead included); "
        f"inject_irq {inject['seconds'] * 1e3 / ns.ticks:.2f} ms per tick"
    )
    for name, calls, ms in hook_table(prof.stats, ns.ticks)[:10]:
        print(f"  {name}: {calls:.0f} calls/tick, {ms:.3f} ms/tick")
    # 2b: drop the no-op time base hook (`tick_state['step']` is 0 once the real timer runs)
    assert session.emu.tick_state["step"] == 0
    dropped = prof.remove_matching("install_tick")
    lean = []
    for _ in range(ns.ticks):
        t0 = time.perf_counter()
        ast.run_ticking(session.emu, TICK_INSNS, per_tick=TICK_INSNS)
        lean.append(time.perf_counter() - t0)
    print(
        f"without the no-op tick hook ({dropped} removed), tick wall (ms):",
        {k: round(v * 1e3, 2) for k, v in summarise(lean).items()},
        f"MIPS {mips(TICK_INSNS, statistics.fmean(lean)):.1f}",
    )
    # 3: Unicorn alone on the real code, every hook removed
    removed = prof.remove_all()
    uc, pc = session.emu.uc, session.emu.reg(15)
    t0 = time.perf_counter()
    done, err = 0, ""
    try:
        uc.emu_start(pc, 0xFFFFFFF0, count=ns.raw_insns)
        done = ns.raw_insns
    except Exception as exc:  # noqa: BLE001
        err = str(exc)
    raw_s = time.perf_counter() - t0
    print(
        f"nohooks ({removed} hooks removed): {done} insns in {raw_s:.3f} s = "
        f"{mips(done or ns.raw_insns, raw_s):.1f} MIPS {err}"
    )
    print(f"ceiling (synthetic loop, no hooks): {ceiling_mips():.1f} MIPS")
    return 0


if __name__ == "__main__":
    sys.exit(main())
