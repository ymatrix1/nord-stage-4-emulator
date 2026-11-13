"""Headless runner for the CCES ADSP-21489 simulator (no GUI, ~0.1 s per probe).

usage: python sim_run.py <probe.dxe> --count N [--out _out] [--exit ___lib_prog_term] [--output file.txt]

Loads the .dxe on the "ADSP-214xx Simulator" through CCES's own `TpsdkServer` Python API, runs it until
the exit breakpoint, and writes N DM words starting at --out (a symbol of the .dxe or an address) as one
`0x%08X` per line, the format of `*_measured.txt` (identical to a Memory Browser dump).

Requirements (the tool checks them and says what is missing):
  * CCES 2.12.1 installed with its licence active (`CCES_HOME`, default
    `C:\\Analog Devices\\CrossCore Embedded Studio 2.12.1`). The licence is never touched.
  * TpsdkServer is a 32-bit SWIG module built for Python 3.6, so this script re-runs itself under a
    32-bit Python 3.6: `NS4_SIM_PYTHON` (path), `py -3.6-32`, or pyenv-win's `3.6.8-win32`
    (`pyenv install 3.6.8-win32`). It unpacks the wheel shipped in `<CCES_HOME>/Eclipse/support` into
    `%LOCALAPPDATA%/ns4-sim` on first use.
Kept Python 3.6 compatible (no f-string `=`, no walrus, no dataclasses).
"""
import argparse
import glob
import os
import re
import struct
import subprocess
import sys
import time
import zipfile

CCES_DEFAULT = r'C:\Analog Devices\CrossCore Embedded Studio 2.12.1'
TARGET = ('ADSP-214xx Family Simulator', 'ADSP-214xx Simulator', 'ADSP-21489')
TARGET_PLUS = ('ChipFactory Simulator', 'ADSP-21571 Cycle-Accurate-Sim', 'ADSP-21571')   # SHARC+, ~140 s C-runtime start-up
DM_MEMORY_ID = 1  # "Data(DM) Memory", 32-bit words (checked against GetMemoryTypes)


def cces_home():
    return os.environ.get('CCES_HOME', CCES_DEFAULT)


def is_py36_32():
    return sys.version_info[:2] == (3, 6) and struct.calcsize('P') == 4


def find_python():
    """Path of a 32-bit Python 3.6 that can load TpsdkServer, or None."""
    cands = []
    if os.environ.get('NS4_SIM_PYTHON'):
        cands.append(os.environ['NS4_SIM_PYTHON'])
    home = os.path.expanduser('~')
    cands += sorted(glob.glob(os.path.join(home, '.pyenv', 'pyenv-win', 'versions', '3.6*-win32', 'python.exe')))
    for c in cands:
        if os.path.isfile(c):
            return c
    try:
        out = subprocess.check_output(['py', '-3.6-32', '-c', 'import sys;print(sys.executable)'],
                                      stderr=subprocess.DEVNULL, universal_newlines=True).strip()
        if os.path.isfile(out):
            return out
    except (OSError, subprocess.CalledProcessError):
        pass
    return None


def unpack_wheel():
    """Directory containing the TpsdkServer package (unpacked from the CCES wheel once)."""
    wheels = sorted(glob.glob(os.path.join(cces_home(), 'Eclipse', 'support', 'TpsdkServer-*-py3-none-any.whl')))
    if not wheels:
        sys.exit('no TpsdkServer wheel under %s\\Eclipse\\support (set CCES_HOME)' % cces_home())
    dest = os.path.join(os.environ.get('LOCALAPPDATA', os.path.expanduser('~')), 'ns4-sim',
                        os.path.basename(wheels[-1])[:-4])
    if not os.path.isfile(os.path.join(dest, 'TpsdkServer', '_TpsdkServer.pyd')):
        zipfile.ZipFile(wheels[-1]).extractall(dest)
    return dest


def elf_symbols(dxe):
    """name -> address from CCES `elfsyms` (lines like `_out = 0xB2060;`)."""
    exe = os.path.join(cces_home(), 'elfsyms.exe')
    text = subprocess.check_output([exe, dxe], universal_newlines=True)
    return {m.group(1): int(m.group(2), 16) for m in re.finditer(r'^(\S+) = 0x([0-9A-Fa-f]+);', text, re.M)}


def resolve(spec, syms):
    if spec in syms:
        return syms[spec]
    try:
        return int(spec, 0)
    except ValueError:
        sys.exit('unknown symbol or address: %s' % spec)


def run_probe(dxe, out_addr, exit_addr, count, timeout, plus=False):
    """Runs the program to the exit breakpoint and returns `count` DM words from `out_addr`."""
    sys.path.insert(0, unpack_wheel())
    os.environ['PATH'] = os.path.join(cces_home(), 'System') + os.pathsep + os.environ['PATH']
    os.environ.setdefault('CCES_HOME', cces_home())
    import TpsdkServer.TpsdkServer as T
    T.Connect(*(TARGET_PLUS if plus else TARGET))
    try:
        T.LoadProgram(0, dxe, False)
        T.SetBreakpointAtAddress(0, exit_addr)
        if plus:   # blocks until the breakpoint (the state reads HALTED before the run starts)
            T.Run(0, True)
        else:
            T.Run(0, False)
        t0 = time.time()
        while T.GetTargetState(0) == T.STATE_RUNNING:
            if time.time() - t0 > timeout:
                T.Halt(0, True)
                sys.exit('timeout after %ss (probe did not reach 0x%X)' % (timeout, exit_addr))
            time.sleep(0.01)
        buf = bytearray(4 * count)
        T.GetMemory(0, 5 if plus else DM_MEMORY_ID, out_addr, count, 1, buf)
    finally:
        T.Disconnect()
    return struct.unpack('>%dI' % count, bytes(buf))


def main():
    ap = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    ap.add_argument('dxe')
    ap.add_argument('--count', type=int, required=True, help='DM words to read')
    ap.add_argument('--out', default='_out', help='symbol or address of the output array')
    ap.add_argument('--exit', default='___lib_prog_term', help='symbol or address to stop at')
    ap.add_argument('--output', help='write here instead of stdout')
    ap.add_argument('--timeout', type=float, default=120.0)
    ap.add_argument('--target', choices=['21489', '21571'], default='21489')
    ap.add_argument('--no-reexec', action='store_true', help=argparse.SUPPRESS)
    a = ap.parse_args()
    if not is_py36_32() and not a.no_reexec:
        py = find_python()
        if not py:
            sys.exit('need 32-bit Python 3.6 (TpsdkServer is a 32-bit 3.6 module): '
                     'pyenv install 3.6.8-win32, or set NS4_SIM_PYTHON')
        sys.exit(subprocess.call([py, os.path.abspath(__file__), '--no-reexec'] + sys.argv[1:]))
    dxe = os.path.abspath(a.dxe)
    syms = elf_symbols(dxe)
    words = run_probe(dxe, resolve(a.out, syms), resolve(a.exit, syms), a.count, a.timeout, a.target == '21571')
    text = ''.join('0x%08X\n' % w for w in words)
    if a.output:
        open(a.output, 'w').write(text)
    else:
        sys.stdout.write(text)


if __name__ == '__main__':
    main()
