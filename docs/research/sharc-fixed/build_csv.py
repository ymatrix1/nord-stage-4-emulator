"""Turns the memory dumps of the fixed-point probes into fixed_measured.csv (see README.md).

usage: python build_csv.py <probe dir with <build>.json> <dump dir with fixed_<build>.txt> <out.csv>
Columns: build,mode1,name,word,short,x,y,p0,p1,p2,o0..o5 (hex; pre-state p*, outputs o*).
"""
import json
import sys


def main(jdir, ddir, out):
    rows = ['build,mode1,name,short,word,x,y,p0,p1,p2,r0,mr0,mr1,mr2,astat,stky']
    for b in ('alu0', 'alu1', 'mul0', 'mul1', 'mul2', 'shift'):
        L = json.load(open('%s/%s.json' % (jdir, b)))
        w = [int(x, 16) for x in open('%s/fixed_%s.txt' % (ddir, b))]
        assert w[-1] == 0xD0E5 and len(w) == L['n_words'] + 1
        w, k = w[:-1], 0

        def emit(r, x, y, p):
            nonlocal k
            vals = dict(zip(r['outs'], w[k:k + len(r['outs'])]))
            k += len(r['outs'])
            f = lambda v: '' if v is None else '%x' % v
            rows.append(','.join([b, '%x' % L['mode1'], r['name'], str(r['short']), '%x' % r['word'], f(x), f(y)]
                                 + [f(q) for q in p] + [f(vals.get(o)) for o in ('r0', 'mr0', 'mr1', 'mr2', 'astat', 'stky')]))
        if L['loops'] == 'pairs':
            for x in L['a']:
                for y in L['b']:
                    for r in L['recs']:
                        p = [r['ci'] if r.get('ci') is not None else 0, 0, 0]
                        if r.get('mrp'):
                            p = list(r['mrp'])
                        emit(r, x, y, p)
        else:
            sh = [v & 0xFFFFFFFF for v in L['sh']]
            shift_recs = [r for r in L['recs'] if r['ysel']]
            mr_recs = [r for r in L['recs'] if not r['ysel']]
            for x in L['a']:
                for r in shift_recs:
                    ys = sh if r['ysel'] == 'sh' else L['fy']
                    for y in ys:
                        emit(r, x, y, [0, 0, 0])
            for p in L['mrp']:
                for r in mr_recs:
                    emit(r, 0x7FFFFFF1, 0, p)
        assert k == len(w), (b, k, len(w))
    open(out, 'w').write('\n'.join(rows) + '\n')
    print(len(rows))


if __name__ == '__main__':
    main(*sys.argv[1:])
