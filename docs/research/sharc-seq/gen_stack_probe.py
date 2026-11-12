"""Stack probe (PC stack, loop stack, status stack, LADDR, CJUMP/RFRAME) for the ADSP-21489 simulator.

usage: python gen_stack_probe.py <outdir> [frame]   (the CJUMP/RFRAME scenario is off by default: its first version jumped into IOP space on the simulator)      -> <outdir>/stack.asm, stack.c, stack.json
Every scenario stores register snapshots by absolute address (`dm(_out + n) = reg`); the C wrapper
fills `out` with 0xA5A5A5A5 first, so a store that did not run stays visible. A snapshot is
PCSTKP, PCSTK, LADDR, CURLCNTR and STKY.
"""
import json
import os
import sys

SNAP = ['pcstkp', 'pcstk', 'laddr', 'curlcntr', 'stky']


class Gen:
    def __init__(self):
        self.lines = []
        self.n = 0
        self.names = []

    def emit(self, *ls):
        self.lines += ['    ' + l for l in ls]

    def store(self, reg, name):
        self.emit('dm(_out + %d) = %s;' % (self.n, reg))
        self.names.append(name)
        self.n += 1

    def snap(self, tag):
        for r in SNAP:
            self.emit('r0 = %s;' % r)
            self.store('r0', '%s.%s' % (tag, r))


def build(frame=False):
    g = Gen()
    g.emit('i2 = _stk;', 'm5 = 1;')
    # S1: call chain, PC stack depth 1..30 (the simulator halts with "PC stack has overflowed" on a 31st entry, and on a 16th status stack entry)
    depth = 30
    g.emit('call chain0;', 'jump chain_done;')
    for i in range(depth):
        g.lines.append('chain%d:' % i)
        g.snap('chain%d' % i)
        if i + 1 < depth:
            g.emit('call chain%d;' % (i + 1), 'rts;')
        else:
            g.emit('jump chain_pop;')
    g.lines.append('chain_pop:')
    g.emit(*['pop pcstk;'] * depth)
    g.lines.append('chain_done:')
    g.snap('after_chain')
    # S2: nested counter loops, depth 1..6, one pass each
    for d in range(1, 7):
        g.emit('lcntr = 1, do nest%d_%d until lce;' % (d, 1))
        for k in range(2, d + 1):
            g.emit('lcntr = 1, do nest%d_%d until lce;' % (d, k))
        g.snap('nest%d' % d)
        g.emit('nop;')
        for k in range(d, 0, -1):
            g.lines.append('nest%d_%d: nop;' % (d, k))
    g.snap('after_nest')
    # S3: status stack
    for i in range(15):
        g.emit('push sts;')
        g.emit('r0 = stky;')
        g.store('r0', 'sts_push%d.stky' % (i + 1))
    for i in range(15):
        g.emit('pop sts;')
        g.emit('r0 = stky;')
        g.store('r0', 'sts_pop%d.stky' % (i + 1))
    # S4: push/pop of the loop and PC stack (outside a loop: the assembler warns against using them inside one and the loop stack is left corrupted)
    for tag, ops in (('seq1', ['push loop', 'push pcstk', 'pop pcstk', 'pop loop', 'push pcstk', 'pop pcstk']),
                     ('seq2', ['push pcstk', 'push loop', 'pop loop', 'pop pcstk']),
                     ('seq3', ['push loop', 'pop loop', 'push loop', 'pop loop'])):
        g.snap(tag + '.start')
        for j, op in enumerate(ops):
            g.emit(op + ';')
            g.snap('%s.%d_%s' % (tag, j, op.replace(' ', '_')))
    # S5: LADDR / CURLCNTR inside counter and arithmetic loops, bodies of 1..5 instructions
    for n in range(1, 6):
        for reg, nm in (('curlcntr', 'cnt'), ('laddr', 'lad')):
            g.emit('lcntr = 3, do ladc%s%d until lce;' % (nm, n))
            for j in range(n):
                ins = 'dm(i2, m5) = %s;' % reg if j == 0 else 'nop;'
                g.lines.append(('ladc%s%d: ' % (nm, n) if j == n - 1 else '    ') + ins)
        # stores through i2 go to the _stk area, copied to out afterwards
    g.emit('i2 = _stk;')
    for k in range(30):
        g.emit('r0 = dm(i2, m5);')
        g.store('r0', 'stk%d' % k)
    if frame:
        # S6: CJUMP / RFRAME with the C calling convention
        g.emit('i7 = _frame + 16;', 'i6 = _frame + 16;', 'm7 = -1;', 'm6 = 1;', 'm14 = 0;', 'r2 = 0x1234;')
        g.emit('cjump fsub (db);', 'dm(i7, m7) = r2;', 'dm(i7, m7) = cjret;')
        g.lines.append('cjret:')
        g.emit('r0 = i6;')
        g.store('r0', 'cjump.i6')
        g.emit('r0 = i7;')
        g.store('r0', 'cjump.i7')
        g.emit('r0 = i12;')
        g.store('r0', 'cjump.i12')
        for k in range(6):
            g.emit('r0 = dm(_frame + %d);' % (10 + k))
            g.store('r0', 'cjump.frame%d' % (10 + k))
        g.emit('i6 = _frame + 8;', 'i7 = _frame + 12;', 'r0 = 0xAAAA;', 'dm(_frame + 8) = r0;', 'r0 = 0xBBBB;',
               'dm(_frame + 7) = r0;', 'dm(_frame + 9) = r0;', 'rframe;')
        g.emit('r0 = i6;')
        g.store('r0', 'rframe.i6')
        g.emit('r0 = i7;')
        g.store('r0', 'rframe.i7')
        g.emit('jump fdone;')
        g.lines.append('fsub:')
        g.emit('i12 = dm(m7, i6);', 'jump (m14, i12) (db);', 'rframe;', 'nop;')
        g.lines.append('fdone:')
    return g


def main(outdir):
    g = build(frame='frame' in sys.argv[2:])
    asm = ['/* stack probe for ADSP-21489 (see ../sharc-stacks.md) */', '#include <asm_sprt.h>', '.extern _out;',
           '.section/dm seg_dmda;', '.var _stk[64];', '.var _frame[32];', '.section/pm seg_pmco;',
           '.global _probe;', '.global _probe_exit;', '_probe:', '    leaf_entry;']
    asm += g.lines
    asm += ['    r0 = 0xD0E5;', '    dm(_out + %d) = r0;' % g.n, '_probe_exit:', '    leaf_exit;', '._probe.end:']
    os.makedirs(outdir, exist_ok=True)
    open(os.path.join(outdir, 'stack.asm'), 'w').write('\n'.join(asm) + '\n')
    n = g.n + 1
    open(os.path.join(outdir, 'stack.c'), 'w').write(
        '/* Runs on the CCES ADSP-21489 simulator; results are saved from the Memory Browser. */\n'
        '#define NOUT %d\nextern void probe(unsigned *out);\nunsigned out[NOUT];\nvolatile unsigned done;\n'
        'int main(void) { int i; for (i = 0; i < NOUT; i++) out[i] = 0xA5A5A5A5u; probe(out); done = 0xD0E5; '
        'return 0; }\n' % n)
    json.dump(dict(n_words=n, names=g.names), open(os.path.join(outdir, 'stack.json'), 'w'))
    print(n)


if __name__ == '__main__':
    main(sys.argv[1])
