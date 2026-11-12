/* FIX/TRUNC rounding and 40-bit extension probe for ADSP-21489 (see ../sharc-float.md). */
#include <asm_sprt.h>
.section/dm seg_dmda;
.var _fixin[24] = 0x00000000, 0x3F000000, 0xBF000000, 0x3FC00000, 0xBFC00000, 0x40200000, 0xC0200000, 0x40600000, 0x3EFFF2E5, 0x3F00068E, 0xBEFFF2E5, 0xBF00068E, 0x3F7FFFFF, 0x3F800000, 0xBF800000, 0x449A5000, 0xC49A5000, 0x4E6E6B28, 0xCE6E6B28, 0x4EFFFFFF, 0x4F000000, 0xCF000000, 0x4F32D05E, 0xCF32D05E;
.var _scales[3] = 0, 4, -4;
.var _modes[8] = 0x00000000, 0x00002000, 0x00010000, 0x00012000, 0x00008000, 0x0000A000, 0x00018000, 0x0001A000;
.var _badd[26] = 0x2F000000, 0x2F800000, 0x2FC00000, 0x30000000, 0x30200000, 0x30400000, 0x30800000, 0x33000000, 0x33800000, 0x34000000, 0x34002000, 0x34004000, 0x34008000, 0xAF000000, 0xAF800000, 0xAFC00000, 0xB0000000, 0xB0200000, 0xB0400000, 0xB0800000, 0xB3000000, 0xB3800000, 0xB4000000, 0xB4002000, 0xB4004000, 0xB4008000;
.var _mula[8] = 0x3F800080, 0x3F800800, 0x3F800001, 0x3F800080, 0x3F800100, 0x3F802000, 0x3F800001, 0x3F7FFFFF;
.var _mulb[8] = 0x3F800080, 0x3F800008, 0x3F808000, 0x3F800040, 0x3F800080, 0x3F800004, 0x3F800001, 0x3F7FFFFF;
.var _ints[8] = 0x40000001, 0x40000080, 0x400000FF, 0x7FFFFFFF, 0x40000100, 0x40000101, 0x00000001, 0x7FFFFF80;
.var _tmp[1];
.global _fixin, _scales, _modes, _badd, _mula, _mulb, _ints;
.section/pm seg_pmco;
.global _probe;
_probe:
    leaf_entry;
    i0 = r4;               /* out */
    m5 = 1;
    m6 = 0;
    r12 = mode1;
    r13 = 0x1A000;
    r13 = not r13;
    r12 = r12 and r13;  /* base mode: TRUNC, RND32, ALUSAT clear */
    i2 = _modes;
    lcntr = 8, do mode_loop until lce;
        r15 = dm(i2, m5);
        r14 = r12 or r15;
        mode1 = r14;
        nop; nop;
        /* FIX / TRUNC */
        i1 = _fixin;
        lcntr = 24, do fx_loop until lce;
            r1 = dm(i1, m5);
            i3 = _scales;
            lcntr = 3, do sc_loop until lce;
                r2 = dm(i3, m5);
                r0 = fix f1 by r2;
                r7 = astat;
                dm(i0, m5) = r0;
                dm(i0, m5) = r7;
                r0 = trunc f1 by r2;
                r7 = astat;
                dm(i0, m5) = r0;
                dm(i0, m5) = r7;
sc_loop:        nop;
fx_loop:    nop;
        /* extension: add */
        r1 = 0x3F800000;
        i1 = _badd;
        lcntr = 26, do ad_loop until lce;
            r2 = dm(i1, m5);
            f0 = f1 + f2;
            f6 = f0 - f1;
            dm(i0, m5) = r0;
            dm(i0, m5) = r6;
            f7 = f0;                 /* register move */
            f8 = f7 - f1;
            dm(i0, m5) = r8;
            f9 = pass f0;
            f10 = f9 - f1;
            dm(i0, m5) = r10;
            dm(_tmp) = r0;           /* through memory */
            r11 = dm(_tmp);
            f3 = f11 - f1;
            dm(i0, m5) = r3;
            f9 = rnd f0;
            f10 = f9 - f1;
            dm(i0, m5) = r10;
            mode1 = r12;             /* variant b: created in base mode, consumed in mode */
            nop; nop;
            f0 = f1 + f2;
            mode1 = r14;
            nop; nop;
            f3 = f0 - f1;
            dm(i0, m5) = r3;
ad_loop:    nop;
        /* extension: multiply */
        r4 = 0x3F800000;
        i1 = _mula;
        i4 = _mulb;
        lcntr = 8, do mu_loop until lce;
            r2 = dm(i1, m5);
            r3 = dm(i4, m5);
            f0 = f2 * f3;
            f6 = f0 - f4;
            dm(i0, m5) = r0;
            dm(i0, m5) = r6;
mu_loop:    nop;
        /* extension: FLOAT of 32-bit integers */
        r4 = 0x4E800000;
        i1 = _ints;
        lcntr = 8, do fl_loop until lce;
            r2 = dm(i1, m5);
            f0 = float r2;
            f6 = f0 - f4;
            dm(i0, m5) = r0;
            dm(i0, m5) = r6;
fl_loop:    nop;
mode_loop: nop;
    mode1 = r12;
    nop; nop;
    leaf_exit;
._probe.end:
