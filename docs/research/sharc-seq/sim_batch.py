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
    scr, cases = None, []
    for line in open(path):
        t = line.split()
        if not t:
            continue
        if t[0] == 'scr':
            scr = [int(x, 16) for x in t[1:]]
        else:
            cases.append((t[0], [int(x, 16) for x in t[1:]]))
    return scr, cases


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('dxe')
    ap.add_argument('cases')
    ap.add_argument('--output', required=True)
    ap.add_argument('--n-state', type=int, default=107)
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
    scr, cases = read_cases(a.cases)
    sys.path.insert(0, sim_run.unpack_wheel())
    os.environ['PATH'] = os.path.join(sim_run.cces_home(), 'System') + os.pathsep + os.environ['PATH']
    os.environ.setdefault('CCES_HOME', sim_run.cces_home())
    import TpsdkServer.TpsdkServer as T
    T.Connect(*sim_run.TARGET)
    out = open(a.output, 'w')
    t0 = time.time()
    try:
        buf_res = bytearray(4 * a.n_state)
        buf_scr = bytearray(4 * SCR_WORDS)
        flag = bytearray(4)
        for k, (parcels, state) in enumerate(cases):
            T.LoadProgram(0, dxe, False)   # fresh memory and registers: Reset is slow and leaves no program
            T.SetBreakpointAtAddress(0, exit_a)
            ps = [int(parcels[i:i + 4], 16) for i in range(0, len(parcels), 4)]
            ps += [0x0001] * (SLOT_PARCELS - len(ps))
            T.SetMemory(0, sim_run.DM_MEMORY_ID, st, len(state), 1, bytearray(words_bytes(state)))
            T.SetMemory(0, sim_run.DM_MEMORY_ID, scr_a, SCR_WORDS, 1, bytearray(words_bytes(scr)))
            T.SetMemory(0, PM_SHORT_WORD_ID, slot, SLOT_PARCELS, 1, bytearray(struct.pack('>%dH' % SLOT_PARCELS, *ps)))
            T.Run(0, False)
            t1 = time.time()
            while True:   # the state reads HALTED until the run starts: wait for the done flag instead
                try:
                    T.GetMemory(0, sim_run.DM_MEMORY_ID, done, 1, 1, flag)
                    if struct.unpack('>I', bytes(flag))[0] == 0xD0E5:
                        break
                except RuntimeError:
                    pass
                if time.time() - t1 > 3:
                    break
            try:
                T.GetMemory(0, sim_run.DM_MEMORY_ID, res, a.n_state, 1, buf_res)
                T.GetMemory(0, sim_run.DM_MEMORY_ID, scr_a, SCR_WORDS, 1, buf_scr)
            except RuntimeError as e:   # the simulator stopped on the instruction (bad access, ...)
                out.write('HALT %s\n' % str(e).split(':')[-1].strip().replace(' ', '_'))
                continue
            ws = struct.unpack('>%dI' % a.n_state, bytes(buf_res)) + struct.unpack('>%dI' % SCR_WORDS, bytes(buf_scr))
            out.write(' '.join('%08X' % w for w in ws) + '\n')
    finally:
        T.Disconnect()
        out.close()
    print('%d cases in %.1fs' % (len(cases), time.time() - t0))


if __name__ == '__main__':
    main()
