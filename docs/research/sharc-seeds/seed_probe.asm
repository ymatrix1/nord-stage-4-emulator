/* RECIPS / RSQRTS seed probe for ADSP-21489 (see ../sharc-float.md).
   probe(out_rec, out_rsq, out_in): for each biased exponent (with sign bit 8) in EXPS and each
   of 128 fraction indices k, two inputs per index: fraction = k<<16 (bin low edge) and
   (k<<16)|0xFFFF (just below the next edge); writes RECIPS / RSQRTS results and the input. */
#include <asm_sprt.h>
.section/dm seg_dmda;
.var _exps[22] = 127, 128, 129, 130, 126, 125, 124, 187, 67, 227, 27, 252, 253, 254, 1,
                 383, 384, 382, 385, 2, 3, 253;
.var _specials[12] = 0x00000000, 0x80000000, 0x7F800000, 0xFF800000, 0x7E800000, 0x7F000000,
                     0x7FC00000, 0x00400000, 0x00800000, 0x3F800000, 0x40000000, 0xBF800000;
.var _flags_rec[12];
.var _flags_rsq[12];
.global _exps, _specials, _flags_rec, _flags_rsq;

.section/pm seg_pmco;
.global _probe;
_probe:
    leaf_entry;
    /* args: r4 = out_rec, r8 = out_rsq, r12 = out_in (cc21k: first args in r4, r8, r12) */
    i0 = r4;
    i1 = r8;
    i2 = r12;
    i4 = _exps;
    m5 = 1;
    m6 = 0;
    r5 = 0xFFFF;
    r9 = 0x10000;
    lcntr = 22, do exp_loop until lce;
        r2 = dm(i4, m5);
        r2 = lshift r2 by 23;
        r3 = 0;
        lcntr = 128, do k_loop until lce;
            r1 = r2 or r3;
            dm(i2, m5) = r1;
            f0 = recips f1;
            f6 = rsqrts f1;
            dm(i0, m5) = r0;
            dm(i1, m5) = r6;
            r1 = r1 or r5;
            dm(i2, m5) = r1;
            f0 = recips f1;
            f6 = rsqrts f1;
            dm(i0, m5) = r0;
            dm(i1, m5) = r6;
            r3 = r3 + r9;
k_loop:     nop;
exp_loop:   nop;
    /* specials with ASTAT */
    i4 = _specials;
    i5 = _flags_rec;
    i3 = _flags_rsq;
    r1 = 0;
    lcntr = 12, do sp_loop until lce;
        r1 = dm(i4, m5);
        f0 = recips f1;
        r7 = astat;
        dm(i5, m5) = r7;
        f6 = rsqrts f1;
        r7 = astat;
        dm(i3, m5) = r7;
        dm(i0, m5) = r0;
        dm(i1, m5) = r6;
        dm(i2, m5) = r1;
sp_loop: nop;
    leaf_exit;
._probe.end:
