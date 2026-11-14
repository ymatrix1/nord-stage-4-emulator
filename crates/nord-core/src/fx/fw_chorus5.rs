//! Instruction-level port of the FX routine `0x122F77` (`kFx2Type` 5, `CHOR` variation, module `0xC0443694`).
//!
//! Generated from the `tools/adi_dis.py` listing of the OS v1.66 FX image (core 2, `0x122F77` ..
//! `0x1230AB`), one `Pe::apply` per executed instruction (`tools/fx_model_chorus.py` is the Python
//! twin). VERIFIED: the model reproduces the routine word for word against the `sharc-fx` oracle
//! (500/500 scenarios, two seeds, and the per-instruction register trace), the golden vectors of
//! `tests_chorus_golden.rs` pin this port to the model (`docs/research/fx-mod2-types.md`).

use super::fw_chorus::ChorusMem;
use super::fw_pe::{
    fabs, fadd, ffloat, fmul, fsub, ftrunc, iadd, imm, ineg, Db, Di, Dl, Dm, Pe, St, Wx, Wy, W,
};

/// Runs the routine once on the machine: the loop count is the block word of the context.
pub(super) fn run(m: &mut Pe<ChorusMem<'_>>) {
    m.apply(&[Di(3, m.i[1])]);
    m.apply(&[Di(2, m.rd(m.a(1, -0x16)))]);
    m.apply(&[W(2, m.ldb(m.a(1, -0x18)))]);
    m.apply(&[W(0, m.ldb(m.a(1, -0x1d)))]);
    m.apply(&[W(7, m.ldb(m.a(1, 0x6)))]);
    m.apply(&[W(3, iadd(m.r(2), m.r(0)))]);
    m.apply(&[W(11, m.ld(m.a(3, -0x9)))]);
    m.apply(&[St(m.a(3, -0x18), m.r(3))]);
    m.apply(&[W(4, m.ld(m.dmod(2, 0)))]);
    m.apply(&[W(14, imm(0xffffffe1))]);
    m.apply(&[W(1, ffloat(m.r(4), m.r(14))), W(12, m.ld(m.a(3, -0x8)))]);
    m.apply(&[Dl(6, 0x200)]);
    m.apply(&[W(15, fabs(m.r(1)))]);
    m.apply(&[W(12, fsub(m.r(12), m.r(11)))]);
    m.apply(&[W(12, fmul(m.r(12), m.r(7))), W(13, m.ldb(m.a(1, -0x6)))]);
    m.apply(&[W(14, m.ld(m.a(3, -0x5)))]);
    m.apply(&[W(11, fadd(m.r(11), m.r(12)))]);
    m.apply(&[W(14, fsub(m.r(14), m.r(13)))]);
    m.apply(&[St(m.a(3, -0x8), m.r(11))]);
    m.apply(&[W(14, fmul(m.r(14), m.r(7)))]);
    m.apply(&[W(11, m.ld(m.a(3, -0x1c)))]);
    m.apply(&[W(13, fadd(m.r(13), m.r(14))), W(12, m.ld(m.a(3, -0x1a)))]);
    m.apply(&[W(12, fsub(m.r(12), m.r(11)))]);
    m.apply(&[St(m.a(3, -0x5), m.r(13))]);
    m.apply(&[W(12, fmul(m.r(12), m.r(7)))]);
    m.apply(&[W(11, fadd(m.r(11), m.r(12)))]);
    m.apply(&[St(m.a(3, -0x1a), m.r(11))]);
    m.apply(&[W(1, m.r(15))]);
    m.apply(&[W(2, m.ld(m.a(3, -0x20)))]);
    m.apply(&[W(3, m.ldb(m.a(1, -0x1e)))]);
    m.apply(&[W(2, fsub(m.r(2), m.r(1)))]);
    m.apply(&[W(2, fmul(m.r(2), m.r(3)))]);
    m.apply(&[W(1, fadd(m.r(1), m.r(2)))]);
    m.apply(&[St(m.a(3, -0x20), m.r(1))]);
    m.apply(&[W(3, m.ldb(m.a(1, -0x1a)))]);
    m.apply(&[W(8, m.ldb(m.a(1, 0x3)))]);
    m.apply(&[W(4, m.ldb(m.a(1, -0x19)))]);
    m.apply(&[Dm(5, 0x200)]);
    m.apply(&[W(1, fmul(m.r(1), m.r(4)))]);
    m.apply(&[Dm(6, 0x202)]);
    m.apply(&[W(1, fadd(m.r(1), m.r(3)))]);
    m.apply(&[W(0, ftrunc(m.r(1)))]);
    m.apply(&[W(4, ineg(m.r(0)))]);
    m.apply(&[W(0, ffloat(m.r(0), imm(0))), St(m.a(3, -0x15), m.r(4))]);
    m.apply(&[W(1, fsub(m.r(1), m.r(0)))]);
    m.apply(&[W(2, fsub(m.r(8), m.r(1))), W(13, m.ld(m.a(3, -0xf)))]);
    m.apply(&[W(15, m.ldb(m.a(1, 0x2)))]);
    m.apply(&[Wx(2, m.y[1]), Wy(1, m.x[2])]);
    m.apply(&[St(m.a(3, -0x13), m.r(1))]);
    m.apply(&[St(m.a(3, -0x11), m.r(2))]);
    m.apply(&[Db(6, m.x[13])]);
    // 0x122FD5
    for _ in 0..m.x[15] {
        m.apply(&[W(0, m.ld(m.dmod(0, 0)))]);
        m.apply(&[W(8, m.ld(m.dmod(0, 2)))]);
        m.apply(&[Di(4, m.rd(m.a(3, -0xf)))]);
        m.apply(&[Di(6, m.rd(m.a(3, -0xd)))]);
        m.apply(&[W(6, m.r(0))]);
        m.apply(&[W(14, m.r(8))]);
        m.apply(&[St(0xc285d, m.r(6))]);
        m.apply(&[St(0xc285f, m.r(14))]);
        m.apply(&[St(m.dmod(6, 6), m.r(0))]);
        m.apply(&[Wx(0, m.y[0]), Wy(0, m.x[0])]);
        m.apply(&[St(m.a(6, 0), m.r(0)), Di(6, m.dmod(6, 1))]);
        m.apply(&[St(m.dmod(6, 6), m.r(8))]);
        m.apply(&[Wx(8, m.y[8]), Wy(8, m.x[8])]);
        m.apply(&[St(m.a(6, 0), m.r(8)), Di(6, m.dmod(6, 1))]);
        m.apply(&[W(1, imm(m.i[6]))]);
        m.apply(&[St(m.a(3, -0xd), m.r(1))]);
        m.apply(&[W(0, m.ld(m.a(4, 0))), Di(4, m.dmod(4, 5))]);
        m.apply(&[St(m.a(4, 0), m.r(0)), Di(4, m.dmod(4, 2))]);
        m.apply(&[W(0, m.ld(m.a(4, 0))), Di(4, m.dmod(4, 5))]);
        m.apply(&[St(m.dmod(4, 0), m.r(0))]);
        m.apply(&[W(1, m.ld(m.a(3, -0x15)))]);
        m.apply(&[Dm(3, m.x[1])]);
        m.apply(&[Dm(4, m.y[1])]);
        m.apply(&[Di(4, m.dmod(6, 3))]);
        m.apply(&[Di(4, m.dmod(4, 6))]);
        m.apply(&[Di(5, m.dmod(6, 4))]);
        m.apply(&[W(3, m.ld(m.a(3, -0x13)))]);
        m.apply(&[W(4, m.ld(m.a(3, -0x11)))]);
        m.apply(&[W(0, m.ld(m.a(4, 0))), Di(4, m.dmod(4, 1))]);
        m.apply(&[W(8, m.ld(m.dmod(4, 0)))]);
        m.apply(&[
            W(0, fmul(m.r(3), m.r(0))),
            W(1, m.ld(m.a(5, 0))),
            Di(5, m.dmod(5, 1)),
        ]);
        m.apply(&[W(8, fmul(m.r(3), m.r(8))), W(9, m.ld(m.dmod(5, 0)))]);
        m.apply(&[W(1, fmul(m.r(4), m.r(1)))]);
        m.apply(&[W(9, fmul(m.r(4), m.r(9)))]);
        m.apply(&[Wx(1, m.y[0]), Wy(0, m.x[1])]);
        m.apply(&[Wx(9, m.y[8]), Wy(8, m.x[9])]);
        m.apply(&[W(7, fadd(m.r(0), m.r(1))), W(4, m.ldb(m.a(1, -0x8)))]);
        m.apply(&[W(15, fadd(m.r(8), m.r(9))), W(2, m.ld(m.a(3, 0x3)))]);
        m.apply(&[W(0, fmul(m.r(7), m.r(4)))]);
        m.apply(&[W(1, fmul(m.r(15), m.r(4)))]);
        m.apply(&[W(0, fmul(m.r(0), m.r(2))), W(7, m.ldb(m.a(1, 0x7)))]);
        m.apply(&[W(1, fmul(m.r(1), m.r(2))), W(15, m.ldb(m.a(1, 0x13)))]);
        m.apply(&[W(9, m.ld(0xc285d))]);
        m.apply(&[W(10, m.ld(0xc285f))]);
        m.apply(&[W(4, fmul(m.r(7), m.r(0))), W(2, m.ld(m.a(3, 0xb)))]);
        m.apply(&[W(11, fmul(m.r(9), m.r(15))), W(12, m.ld(m.a(3, 0x15)))]);
        m.apply(&[W(0, fadd(m.r(2), m.r(4))), W(3, m.ldb(m.a(1, 0x9)))]);
        m.apply(&[W(6, fadd(m.r(11), m.r(12))), W(13, m.ldb(m.a(1, 0x14)))]);
        m.apply(&[W(5, fmul(m.r(0), m.r(3)))]);
        m.apply(&[W(14, fmul(m.r(6), m.r(13)))]);
        m.apply(&[W(2, fsub(m.r(5), m.r(4)))]);
        m.apply(&[W(12, fsub(m.r(11), m.r(14)))]);
        m.apply(&[W(4, fmul(m.r(7), m.r(1)))]);
        m.apply(&[W(11, fmul(m.r(10), m.r(15)))]);
        m.apply(&[W(1, fadd(m.r(2), m.r(4))), W(3, m.ldb(m.a(1, 0x9)))]);
        m.apply(&[W(8, fadd(m.r(11), m.r(12))), W(13, m.ldb(m.a(1, 0x14)))]);
        m.apply(&[W(5, fmul(m.r(1), m.r(3)))]);
        m.apply(&[W(14, fmul(m.r(8), m.r(13)))]);
        m.apply(&[W(5, fsub(m.r(5), m.r(4))), W(7, m.ldb(m.a(1, 0x8)))]);
        m.apply(&[W(12, fsub(m.r(11), m.r(14))), W(15, m.ldb(m.a(1, 0xf)))]);
        m.apply(&[St(m.a(3, 0xb), m.r(5))]);
        m.apply(&[W(4, fmul(m.r(7), m.r(0))), St(m.a(3, 0x15), m.r(12))]);
        m.apply(&[W(11, fmul(m.r(9), m.r(15))), W(2, m.ld(m.a(3, 0xd)))]);
        m.apply(&[W(0, fadd(m.r(2), m.r(4))), W(12, m.ld(m.a(3, 0x11)))]);
        m.apply(&[W(9, fadd(m.r(11), m.r(12))), W(3, m.ldb(m.a(1, 0xa)))]);
        m.apply(&[W(5, fmul(m.r(0), m.r(3))), W(13, m.ldb(m.a(1, 0x10)))]);
        m.apply(&[W(14, fmul(m.r(9), m.r(13)))]);
        m.apply(&[W(2, fsub(m.r(4), m.r(5)))]);
        m.apply(&[W(12, fsub(m.r(14), m.r(11)))]);
        m.apply(&[W(4, fmul(m.r(7), m.r(1)))]);
        m.apply(&[W(11, fmul(m.r(10), m.r(15)))]);
        m.apply(&[W(1, fadd(m.r(2), m.r(4))), W(3, m.ldb(m.a(1, 0xa)))]);
        m.apply(&[W(10, fadd(m.r(11), m.r(12))), W(13, m.ldb(m.a(1, 0x10)))]);
        m.apply(&[W(5, fmul(m.r(1), m.r(3)))]);
        m.apply(&[W(14, fmul(m.r(10), m.r(13)))]);
        m.apply(&[W(5, fsub(m.r(4), m.r(5)))]);
        m.apply(&[W(12, fsub(m.r(14), m.r(11)))]);
        m.apply(&[W(9, fadd(m.r(9), m.r(6))), St(m.a(3, 0xd), m.r(5))]);
        m.apply(&[W(10, fadd(m.r(10), m.r(8))), St(m.a(3, 0x11), m.r(12))]);
        m.apply(&[W(0, fadd(m.r(0), m.r(9)))]);
        m.apply(&[W(1, fadd(m.r(1), m.r(10))), W(3, m.ldb(m.a(1, -0x5)))]);
        m.apply(&[W(8, m.ld(0xc285d))]);
        m.apply(&[W(9, m.ld(0xc285f))]);
        m.apply(&[W(8, fmul(m.r(8), m.r(3)))]);
        m.apply(&[W(9, fmul(m.r(9), m.r(3)))]);
        m.apply(&[W(0, fadd(m.r(0), m.r(8)))]);
        m.apply(&[W(1, fadd(m.r(1), m.r(9)))]);
        m.apply(&[St(m.a(0, 0), m.r(0)), Di(0, m.dmod(0, 2))]);
        m.apply(&[St(m.a(0, 0), m.r(1)), Di(0, m.dmod(0, 2))]);
    }
    m.apply(&[Dl(6, 0x0)]);
}

#[cfg(test)]
mod tests {
    use super::super::fw_chorus::{ChorusKind, FwChorus, CTX_N, RING_N};

    #[test]
    fn zero_blocks_leave_the_frames_and_the_ring_alone() {
        let kind = if 5 == 4 {
            ChorusKind::Soft
        } else {
            ChorusKind::Variation
        };
        let mut c = FwChorus::new(kind, [0; CTX_N], [0; RING_N]);
        let mut frames = [0.5f32; 3];
        c.process(&mut frames);
        assert_eq!(frames, [0.5; 3], "a partial block is not processed");
        assert!(c.ring().iter().all(|&w| w == 0));
    }
}
