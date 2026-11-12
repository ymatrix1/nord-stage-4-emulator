"""Turns the memory dumps of the second fixed-point probes into fixed2_measured.csv.

usage: python build_csv2.py <probe dir with <build>.json> <dump dir with fixed2_<build>.txt> <out.csv>
Columns: build,mode1,name,short,word,x,y,p0,p1,p2,q0,q1,q2,r0,mr0,mr1,mr2,b0,b1,b2,astat,stky
(p* = MRF preset, q* = MRB preset; hex).
"""
import json
import sys

OUTCOLS = ('r0', 'mr0', 'mr1', 'mr2', 'b0', 'b1', 'b2', 'astat', 'stky')


def main(jdir, ddir, out):
    rows = ['build,mode1,name,short,word,x,y,p0,p1,p2,q0,q1,q2,' + ','.join(OUTCOLS)]
    for b in ('or', 'fix0', 'fix1', 'mrb', 'srcu'):
        L = json.load(open('%s/%s.json' % (jdir, b)))
        w = [int(x, 16) for x in open('%s/fixed2_%s.txt' % (ddir, b))]
        assert w[-1] == 0xD0E5 and len(w) == L['n_words'] + 1, (b, len(w), L['n_words'])
        w, k = w[:-1], 0

        def emit(r, x, y, p, q):
            nonlocal k
            vals = dict(zip(r['outs'], w[k:k + len(r['outs'])]))
            k += len(r['outs'])
            f = lambda v: '' if v is None else '%x' % v
            rows.append(','.join([b, '%x' % L['mode1'], r['name'], str(r['short']), '%x' % r['word'], f(x), f(y)]
                                 + [f(v) for v in p] + [f(v) for v in q] + [f(vals.get(o)) for o in OUTCOLS]))
        for s in L['sections']:
            recs = s['recs']
            if s['kind'] == 'pairs':
                for x in s['A']:
                    for y in s['B']:
                        for r in recs:
                            emit(r, x, y, r.get('mrp') or [0, 0, 0], r.get('mrq') or [0, 0, 0])
            elif s['kind'] == 'ytable':
                for x in s['A']:
                    for r in recs:
                        ys = [v & 0xFFFFFFFF for v in L['sh']] if r['ysel'] == 'sh' else L['fy']
                        for y in ys:
                            emit(r, x, y, [0x0F0F0F0F, 0, 0], [0, 0, 0])
            else:
                for q in L['mrp']:
                    for r in recs:
                        emit(r, 0x7FFFFFF1, 0, [0, 0, 0], q)
        assert k == len(w), (b, k, len(w))
    open(out, 'w').write('\n'.join(rows) + '\n')
    print(len(rows))


if __name__ == '__main__':
    main(*sys.argv[1:])
