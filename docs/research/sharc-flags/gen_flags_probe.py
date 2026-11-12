import struct, random, json  # usage: python gen_flags_probe.py <mode index 0..3> (writes flags_probe_m<i>.asm/.c/layout)
random.seed(214)
def h(v): return '0x%08X'%(v&0xFFFFFFFF)
vals=[0x00000000,0x80000000,0x3F800000,0xBF800000,0x40400000,0x00800000,0x00400000,0x80400000,0x00000001,0x7F7FFFFF,0xFF7FFFFF,0x7F800000,0xFF800000,0x7FC00000,0x3F800001,0x7F000000]
nvals=len(vals)
scal=[3,-3,200,-200]
NR=256
rp=[]
for _ in range(NR):
    def r():
        e=random.randint(118,136) if random.random()<0.85 else random.randint(1,254)
        return (random.getrandbits(1)<<31)|(e<<23)|random.getrandbits(23)
    rp.append((r(),r()))
import sys
ALLM=[0,1<<15,1<<16,1<<13]
IDX=int(sys.argv[1])
modes=[ALLM[IDX]]
BIN=[('add','f0 = f1 + f2;'),('sub','f0 = f1 - f2;'),('mul','f0 = f1 * f2;'),('avg','f0 = (f1 + f2)/2;'),
     ('min','f0 = min(f1, f2);'),('max','f0 = max(f1, f2);'),('copysign','f0 = f1 copysign f2;'),('clip','f0 = clip f1 by f2;'),('comp','comp(f1, f2);')]
UN=[('neg','f0 = -f1;'),('abs','f0 = abs f1;'),('pass','f0 = pass f1;'),('mant','r0 = mant f1;'),('logb','r0 = logb f1;'),('rnd','f0 = rnd f1;')]
def rec(op):
    return f'''            r0 = 0;
            r15 = 0;
            stky = r15;
            {op}
            r7 = astat;
            r8 = stky;
            dm(i0, m5) = r0;
            dm(i0, m5) = r7;
            dm(i0, m5) = r8;
'''
def rec_ext(op):
    return f'''            r0 = 0;
            r15 = 0;
            stky = r15;
            {op}
            r7 = astat;
            r8 = stky;
            f9 = rnd f0;
            f10 = f0 - f9;
            dm(i0, m5) = r0;
            dm(i0, m5) = r10;
            dm(i0, m5) = r7;
            dm(i0, m5) = r8;
'''
a=f'''/* Combined float probe for ADSP-21489 (see ../sharc-float.md): special values and flags of the
   float ALU/multiplier, random operands, extended-precision residuals, S-register swaps. */
#include <asm_sprt.h>
.section/dm seg_dmda;
.var _vals[{nvals}] = {', '.join(h(v) for v in vals)};
.var _scal[{len(scal)}] = {', '.join(str(x) for x in scal)};
.var _modes[{len(modes)}] = {', '.join(h(m) for m in modes)};
.var _ra[{NR}] = {', '.join(h(x) for x,_ in rp)};
.var _rb[{NR}] = {', '.join(h(y) for _,y in rp)};
.global _vals, _scal, _modes, _ra, _rb;
.section/pm seg_pmco;
.global _probe;
_probe:
    leaf_entry;
    i0 = r4;
    m5 = 1;
    r12 = mode1;
    r13 = 0x1A000;
    r13 = not r13;
    r12 = r12 and r13;
    i2 = _modes;
    lcntr = {len(modes)}, do mode_loop until lce;
        r14 = dm(i2, m5);
        r14 = r12 or r14;
        mode1 = r14;
        nop; nop;
        /* binary ops on all pairs of special values */
        i1 = _vals;
        lcntr = {nvals}, do a_loop until lce;
            r1 = dm(i1, m5);
            i3 = _vals;
            lcntr = {nvals}, do b_loop until lce;
                r2 = dm(i3, m5);
'''+''.join(rec(o).replace('            ','                ',1) if False else rec(o) for _,o in BIN)+f'''b_loop:         nop;
a_loop:     nop;
        /* unary ops on the special values; scalb with four exponents */
        i1 = _vals;
        lcntr = {nvals}, do u_loop until lce;
            r1 = dm(i1, m5);
'''+''.join(rec(o) for _,o in UN)+f'''            i3 = _scal;
            lcntr = {len(scal)}, do s_loop until lce;
                r2 = dm(i3, m5);
'''+rec('f0 = scalb f1 by r2;')+f'''s_loop:         nop;
u_loop:     nop;
        /* random operands: add, sub, mul, avg with the extension residual */
        i1 = _ra;
        i3 = _rb;
        lcntr = {NR}, do r_loop until lce;
            r1 = dm(i1, m5);
            r2 = dm(i3, m5);
'''+''.join(rec_ext(o) for n,o in BIN[:4])+f'''r_loop:     nop;
        /* S-register swaps and moves keep the extension? */
        r1 = 0x3F800000;
        r2 = 0x30800000;
        f5 = f1 + f2;
        r6 = 0; r9 = 0;
        r5 <-> s2;
        r5 <-> s2;
        f3 = f5 - f1;
        dm(i0, m5) = r3;
        s3 = r5;
        r6 <-> s3;
        f3 = f6 - f1;
        dm(i0, m5) = r3;
        s3 = r5;
        r9 = s3;
        f3 = f9 - f1;
        dm(i0, m5) = r3;
mode_loop: nop;
    mode1 = r12;
    nop; nop;
    leaf_exit;
._probe.end:
'''
per_mode_special=nvals*nvals*len(BIN)*3
per_mode_un=nvals*(len(UN)+len(scal))*3
per_mode_r=NR*4*4
per_mode=per_mode_special+per_mode_un+per_mode_r+3
open('./flags_probe_m%d.asm'%IDX,'w').write(a)
open('./flags_probe_m%d.c'%IDX,'w').write(f'''/* Runs on the CCES ADSP-21489 simulator; results stay in `out` and are saved from the Memory Browser. */
#define NOUT {per_mode*len(modes)}
extern void probe(unsigned *out);
unsigned out[NOUT];
volatile unsigned done;
int main(void) {{ probe(out); done = 0xD0E5; return 0; }}
''')
json.dump(dict(vals=vals,scal=scal,modes=modes,rp=rp,bin=[n for n,_ in BIN],un=[n for n,_ in UN],per_mode=per_mode,nr=NR),open('./layout_m%d.json'%IDX,'w'))
print(per_mode,per_mode*len(modes))
