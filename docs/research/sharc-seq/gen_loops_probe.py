"""Arithmetic-loop probe (DO ... UNTIL cond and counter loops) for the ADSP-21489 simulator.

usage: python gen_loops_probe.py <outdir>      -> <outdir>/loops.asm, loops.c, loops.json
Every case runs one loop whose body decrements R0 once per pass (at body index p, the other body
instructions are NOPs); the final R0 gives the number of passes the loop executed. One word of
output per case (the final R0), then the done flag 0xD0E5.
"""
import json
import os
import sys

CONDS = ['eq', 'ne', 'lt', 'ge', 'gt', 'le']
NS = range(1, 7)
KS = range(0, 9)
# `eq`, `ge` and `gt` are true only for a few counter values: with a small K the sampled state can
# already have passed them and the loop never ends, so they start at K = 4
SAFE_FROM = {'eq': 4, 'ge': 4, 'gt': 4}


def cases():
    out = []
    for cond in CONDS:
        for n in NS:
            for p in range(n):
                for k in KS:
                    if k < SAFE_FROM.get(cond, 0):
                        continue
                    out.append(dict(kind='until', cond=cond, n=n, p=p, k=k))
    for n in NS:
        for p in range(n):
            for k in range(1, 10):
                out.append(dict(kind='lce', cond='lce', n=n, p=p, k=k))
    return out


def cases_entry():
    """Conditions that already hold when the loop is entered: six stores after the loop show when
    the trailing iterations run."""
    out = []
    for cond in ('ne', 'ge'):
        for n in NS:
            for p in range(n):
                for k in (4, 7):
                    out.append(dict(kind='until', cond=cond, n=n, p=p, k=k))
    return out


def body(c, i):
    lab = 'lp_%d' % i
    lines = []
    for j in range(c['n']):
        ins = 'r0 = r0 - 1;' if j == c['p'] else 'nop;'
        lines.append(('%s: %s' % (lab, ins)) if j == c['n'] - 1 else ins)
    return lab, lines


def main(outdir, variant='loops'):
    cs = cases() if variant == 'loops' else cases_entry()
    stores = 2 if variant == 'loops' else 6
    asm = ['/* arithmetic-loop probe for ADSP-21489 (see ../sharc-sequencer.md) */', '#include <asm_sprt.h>',
           '.extern _out;', '.section/pm seg_pmco;', '.global _probe;', '.global _probe_exit;', '_probe:',
           '    leaf_entry;', '    i2 = r4;', '    m5 = 1;']
    for i, c in enumerate(cs):
        lab, lines = body(c, i)
        asm.append('    r0 = %d;' % c['k'])
        if c['kind'] == 'until':
            asm += ['    r15 = 1;', '    r15 = pass r15;', '    do %s until %s;' % (lab, c['cond'])]
        else:
            asm.append('    lcntr = %d, do %s until lce;' % (c['k'], lab))
        asm += ['    ' + l for l in lines]
        # two stores by absolute address: the first directly after the loop, the second one
        # instruction later, so a skipped instruction after the loop shows up as a sentinel
        asm += ['    dm(_out + %d) = r0;' % (stores * i + j) for j in range(stores)]
    asm += ['    r0 = 0xD0E5;', '    dm(_out + %d) = r0;' % (stores * len(cs)), '_probe_exit:', '    leaf_exit;', '._probe.end:']
    os.makedirs(outdir, exist_ok=True)
    open(os.path.join(outdir, variant + '.asm'), 'w').write('\n'.join(asm) + '\n')
    n = stores * len(cs) + 1
    open(os.path.join(outdir, variant + '.c'), 'w').write(
        '/* Runs on the CCES ADSP-21489 simulator; results are saved from the Memory Browser. */\n'
        '#define NOUT %d\nextern void probe(unsigned *out);\nunsigned out[NOUT];\nvolatile unsigned done;\n'
        'int main(void) { probe(out); done = 0xD0E5; return 0; }\n' % n)
    json.dump(dict(n_words=n, cases=cs), open(os.path.join(outdir, variant + '.json'), 'w'))
    with open(os.path.join(outdir, variant + '_cases.txt'), 'w') as f:
        f.write(''.join('%s %s %d %d %d\n' % (c['kind'], c['cond'], c['n'], c['p'], c['k']) for c in cs))
    print(n)


if __name__ == '__main__':
    main(*sys.argv[1:])
