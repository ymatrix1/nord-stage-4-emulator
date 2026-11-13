"""Runs many single-instruction cases on the CCES ADSP-21489 simulator (headless, see sim_run.py).

usage: python sim_batch.py cens.dxe cases.txt --output results.txt

cases.txt (hex words): a line `scr W0 .. W511` (the scratch table every case starts with), then one line
per case `PARCELS S0 .. S106`: PARCELS = the instruction's 16-bit parcels (1 to 3, concatenated) poked
into `_slot`, S0.. = the state table written to `_st` (layout in gen_census_probe.py). results.txt has,
per case, the dumped state (`_res`, N words) followed by the scratch table (512 words) after the slot ran.
"""
import argparse
import os
import struct
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import sim_run  # noqa: E402

SLOT_PARCELS = 10
SCR_WORDS = 512
PM_SHORT_WORD_ID = 9   # "Short Word Memory", 16-bit locations (checked: reads back the slot's 0x0001 nops)


def words_bytes(ws):
    return struct.pack('>%dI' % len(ws), *ws)


def read_cases(path):
    """Returns (scratch table, global memory setup, cases). Header lines besides `scr`:
    `code ADDR PARCELS` (16-bit parcels, hex, poked into program memory), `dm ADDR WORDS..` and
    `watch ADDR COUNT` (DM words appended to every result). A case line may end with
    ` | ADDR WORDS..` chunks of DM to write before it runs (routine inputs)."""
    scr, cases, setup = None, [], {'code': [], 'dm': [], 'watch': []}
    for line in open(path):
        parts = line.split('|')
        t = parts[0].split()
        if not t:
            continue
        if t[0] == 'scr':
            scr = [int(x, 16) for x in t[1:]]
        elif t[0] == 'code':
            setup['code'].append((int(t[1], 16), t[2]))
        elif t[0] == 'dm':
            setup['dm'].append((int(t[1], 16), [int(x, 16) for x in t[2:]]))
        elif t[0] == 'watch':
            setup['watch'].append((int(t[1], 16), int(t[2])))
        else:
            chunks = [(int(c.split()[0], 16), [int(x, 16) for x in c.split()[1:]]) for c in parts[1:] if c.split()]
            cases.append((t[0], [int(x, 16) for x in t[1:]], chunks))
    return scr, setup, cases


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('dxe')
    ap.add_argument('cases')
    ap.add_argument('--output', required=True)
    ap.add_argument('--n-state', type=int, default=107)
    ap.add_argument('--target', choices=['21489', '21571'], default='21489',
                    help='simulator: ADSP-21489 (functional) or ADSP-21571 (SHARC+, cycle-accurate)')
    ap.add_argument('--no-reexec', action='store_true')
    a = ap.parse_args()
    if not sim_run.is_py36_32() and not a.no_reexec:
        py = sim_run.find_python()
        if not py:
            sys.exit('need 32-bit Python 3.6 (see sim_run.py)')
        sys.exit(__import__('subprocess').call([py, os.path.abspath(__file__), '--no-reexec'] + sys.argv[1:]))
    dxe = os.path.abspath(a.dxe)
    syms = sim_run.elf_symbols(dxe)
    slot, st, res, scr_a, exit_a, done = (syms[n] for n in ('_slot', '_st', '_res', '_scr', '_probe_exit', '_done'))
    scr, setup, cases = read_cases(a.cases)
    sys.path.insert(0, sim_run.unpack_wheel())
    os.environ['PATH'] = os.path.join(sim_run.cces_home(), 'System') + os.pathsep + os.environ['PATH']
    os.environ.setdefault('CCES_HOME', sim_run.cces_home())
    import TpsdkServer.TpsdkServer as T
    target = sim_run.TARGET if a.target == '21489' else ('ChipFactory Simulator', 'ADSP-21571 Cycle-Accurate-Sim', 'ADSP-21571')
    dm_id = sim_run.DM_MEMORY_ID if a.target == '21489' else 5          # 21571: "L1 32-bit (NW) Block 0" (any block reaches the whole space)
    pm_id = PM_SHORT_WORD_ID if a.target == '21489' else 17             # 21571: "L1 16-bit (SW) Block 0"
    T.Connect(*target)
    out = open(a.output, 'w')
    t0 = time.time()
    try:
        buf_res = bytearray(4 * a.n_state)
        buf_scr = bytearray(4 * SCR_WORDS)
        flag = bytearray(4)
        if a.target == '21571':
            # The cycle-accurate SHARC+ simulator needs about 140 s to get through the C runtime start-up,
            # so the program is loaded and run to the probe exit once; every case then restarts the
            # probe (SetRegister(PC) is ignored by the SHARC+ simulator, so the 21571 probe ends in `jump _restart`:
            # resuming from the exit breakpoint runs the next case).
            T.LoadProgram(0, dxe, False)
            T.RemoveAllBreakpoints(0)
            T.SetBreakpointAtAddress(0, exit_a)
            T.Run(0, True)
        for k, (parcels, state, chunks) in enumerate(cases):
            if a.target != '21571':
                T.LoadProgram(0, dxe, False)   # fresh memory and registers: Reset is slow and leaves no program
                T.SetBreakpointAtAddress(0, exit_a)
            ps = [int(parcels[i:i + 4], 16) for i in range(0, len(parcels), 4)]
            ps += [0x0001] * (SLOT_PARCELS - len(ps))
            T.SetMemory(0, dm_id, st, len(state), 1, bytearray(words_bytes(state)))
            T.SetMemory(0, dm_id, scr_a, SCR_WORDS, 1, bytearray(words_bytes(scr)))
            T.SetMemory(0, pm_id, slot, SLOT_PARCELS, 1, bytearray(struct.pack('>%dH' % SLOT_PARCELS, *ps)))
            for addr, hexp in setup['code']:
                n = len(hexp) // 4
                T.SetMemory(0, pm_id, addr, n, 1, bytearray(bytes.fromhex(hexp)))
            for addr, ws in setup['dm'] + chunks:
                T.SetMemory(0, dm_id, addr, len(ws), 1, bytearray(words_bytes(ws)))
            if a.target == '21571':
                T.SetMemory(0, dm_id, done, 1, 1, bytearray(4))
                T.Run(0, True)
            else:
                T.Run(0, False)
            t1 = time.time()
            while True:   # the state reads HALTED until the run starts: wait for the done flag instead
                try:
                    T.GetMemory(0, dm_id, done, 1, 1, flag)
                    if struct.unpack('>I', bytes(flag))[0] == 0xD0E5:
                        break
                except RuntimeError:
                    pass
                if time.time() - t1 > 3:
                    break
            try:
                T.GetMemory(0, dm_id, res, a.n_state, 1, buf_res)
                T.GetMemory(0, dm_id, scr_a, SCR_WORDS, 1, buf_scr)
            except RuntimeError as e:   # the simulator stopped on the instruction (bad access, ...)
                out.write('HALT %s\n' % str(e).split(':')[-1].strip().replace(' ', '_'))
                continue
            ws = struct.unpack('>%dI' % a.n_state, bytes(buf_res)) + struct.unpack('>%dI' % SCR_WORDS, bytes(buf_scr))
            for addr, n in setup['watch']:
                wb = bytearray(4 * n)
                T.GetMemory(0, dm_id, addr, n, 1, wb)
                ws += struct.unpack('>%dI' % n, bytes(wb))
            out.write(' '.join('%08X' % w for w in ws) + '\n')
    finally:
        T.Disconnect()
        out.close()
    print('%d cases in %.1fs' % (len(cases), time.time() - t0))


if __name__ == '__main__':
    main()
