#include <asm_sprt.h>
.extern _out;
.section/dm seg_dmda;
.var _buf[16];
.section/pm seg_pmco;
.global _probe;
.global _probe_exit;
_probe:
    leaf_entry;
    i2 = _out;
    m2 = 1;
    i1 = _buf;
    m1 = 1;
    /* A: outer 2, inner 3, inner last instr = plain compute */
    r0 = 0; r1 = 0; r2 = 0;
    lcntr = 2, do oa until lce;
        r0 = r0 + 1;
        lcntr = 3, do ia until lce;
            r1 = r1 + 1;
ia:         r2 = r2 + r1;
oa:     nop;
    dm(i2, m2) = r0;
    dm(i2, m2) = r1;
    dm(i2, m2) = r2;
    /* B: inner last instr is a multifunction with memory access */
    r0 = 0; r1 = 0; r2 = 0; r3 = 1;
    i1 = _buf;
    lcntr = 2, do ob until lce;
        r0 = r0 + 1;
        lcntr = 3, do ib until lce;
            r1 = r1 + 1;
ib:         r2 = r2 + r3, dm(i1, m1) = r1;
ob:     nop;
    dm(i2, m2) = r0;
    dm(i2, m2) = r1;
    dm(i2, m2) = r2;
    r5 = i1;
    dm(i2, m2) = r5;
    /* C: outer 3, inner 1 */
    r0 = 0; r1 = 0;
    lcntr = 3, do oc until lce;
        r0 = r0 + 1;
        lcntr = 1, do ic until lce;
ic:         r1 = r1 + 1;
oc:     nop;
    dm(i2, m2) = r0;
    dm(i2, m2) = r1;
_probe_exit:
    leaf_exit;
._probe.end:
