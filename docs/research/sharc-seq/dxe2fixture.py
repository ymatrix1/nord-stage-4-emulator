"""Turns the probe part of an ADSP-21489 .dxe into a text fixture the Rust oracle tests can run.

usage: python dxe2fixture.py <x.dxe> <out.txt> --code START END --data START END --entry ADDR --exit ADDR --out ADDR N [--fill WORD] [--sym NAME ADDR]...
Addresses are hex (short-word addresses for code, DM addresses for data). Only the bytes inside the
ranges are written, so the C runtime of the build is not part of the fixture.
"""
import struct
import sys


def segments(path):
    d = open(path, 'rb').read()
    phoff, = struct.unpack_from('<I', d, 0x1C)
    phentsize, phnum = struct.unpack_from('<HH', d, 0x2A)
    for i in range(phnum):
        typ, off, vaddr, paddr, filesz, memsz, flags, align = struct.unpack_from('<8I', d, phoff + i * phentsize)
        if typ == 1:
            yield vaddr, d[off:off + filesz]


def main(argv):
    dxe, out = argv[0], argv[1]
    args = argv[2:]
    opt = {}
    k = 0
    while k < len(args):
        name = args[k]
        n = {'--code': 2, '--data': 2, '--entry': 1, '--exit': 1, '--out': 2, '--fill': 1, '--sym': 2}[name]
        vals = args[k + 1:k + 1 + n]
        if name == '--sym':   # NAME HEXADDR, repeatable
            opt.setdefault('syms', []).append((vals[0], int(vals[1], 16)))
        else:
            opt[name] = [int(a, 16) if name != '--out' or j == 0 else int(a) for j, a in enumerate(vals)]
        k += 1 + n
    code = {}
    data = {}
    for vaddr, blob in segments(dxe):
        if vaddr >= 0x100000:   # short-word code window: 2 bytes per address
            for j in range(len(blob) // 2):
                a = vaddr + j
                if opt['--code'][0] <= a < opt['--code'][1]:
                    code[a] = '%04x' % (blob[2 * j] << 8 | blob[2 * j + 1])   # parcels are stored most significant byte first
        else:                   # data memory, 4 bytes per word
            for j in range(len(blob) // 4):
                a = vaddr + j
                if opt['--data'][0] <= a < opt['--data'][1]:
                    data[a] = struct.unpack_from('<I', blob, 4 * j)[0]
    lines = ['# fixture of %s' % dxe.split('/')[-1], 'entry %x' % opt['--entry'][0], 'exit %x' % opt['--exit'][0],
             'out %x %d' % tuple(opt['--out'])]
    if '--fill' in opt:
        lines.append('fill %x' % opt['--fill'][0])
    run, start = [], None
    for a in sorted(code):
        if start is not None and a == start + len(run) and len(run) < 24:
            run.append(code[a])
            continue
        if run:
            lines.append('code %x %s' % (start, ''.join(run)))
        start, run = a, [code[a]]
    if run:
        lines.append('code %x %s' % (start, ''.join(run)))
    lines += ['sym %s %x' % (n, a) for n, a in opt.get('syms', [])]
    lines += ['dm %x %x' % (a, data[a]) for a in sorted(data) if data[a]]
    open(out, 'w').write('\n'.join(lines) + '\n')
    print(len(code), 'parcels', len(data), 'words')


if __name__ == '__main__':
    main(sys.argv[1:])
