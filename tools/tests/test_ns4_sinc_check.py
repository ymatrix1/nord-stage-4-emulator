import os
import struct
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import ns4_sinc_check as s  # noqa: E402
import ns4_synth_kernels as k  # noqa: E402


def test_bessel_i0_known_values():
    assert s.bessel_i0(0.0) == 1.0
    assert abs(s.bessel_i0(1.0) - 1.2660658777520082) < 1e-14  # Abramowitz and Stegun 9.8
    assert abs(s.bessel_i0(8.5) - 1.0 / s.kaiser(6.0)) < 1e-9


def test_kaiser_shape():
    assert s.kaiser(0.0) == 1.0
    assert s.kaiser(1.5) == s.kaiser(-1.5)
    assert abs(s.kaiser(6.0) - 1.0 / s.bessel_i0(8.5)) < 1e-15  # the edge is kept, not zeroed
    assert s.kaiser(6.01) == 0.0
    assert s.kaiser(3.0) < s.kaiser(1.0) < 1.0


def test_kernel_value_layout():
    assert abs(s.kernel_value(1.0, 5, 0) - 1.0) < 1e-15  # sinc peak at tap 5, phase 0
    assert abs(s.kernel_value(1.0, 6, 0)) < 1e-15  # integer offsets are zero crossings at fc = 1
    assert s.kernel_value(1.1, 11, 0) > 0  # d = +6 keeps the Kaiser edge value
    assert len(s.model_block(1.0)) == k.BLOCK
    assert len(s.model_block(1.0, phase_step=8)) == 12 * 32


def test_model_is_symmetric_and_gain_is_one_over_fc():
    for fc in s.CUTOFFS:
        block = s.model_block(fc)
        assert s.mirror_error(block) < 1e-15
        lo, hi = s.dc_gain(block)
        assert abs(lo - 1 / fc) < 2e-3 and abs(hi - 1 / fc) < 2e-3


def test_errors_and_subsample():
    block = s.model_block(1.05)
    assert s.errors(block, 1.05) == (0.0, 0.0)
    assert s.errors(block, 1.05, phase_step=8)[1] == 0.0
    assert s.errors(block, 1.0)[0] > 1e-3  # a wrong cutoff is visible
    assert len(s.subsample(block, 8)) == 12 * 32


def test_fit_beta_recovers_a_planted_beta():
    blocks = [s.model_block(fc, beta=7.0) for fc in s.CUTOFFS]
    assert abs(s.fit_beta(blocks) - 7.0) < 1e-3


def test_float32_rounding_stays_within_one_ulp():
    block = [s.float32(v) for v in s.model_block(0.96)]
    rms, worst = s.errors(block, 0.96)
    assert worst < 6e-8 and rms < 3e-8


def _image() -> bytes:
    filler = struct.pack("<200f", *[0.5 + 0.001 * i for i in range(200)]) + struct.pack("<f", 99.0)
    body = b"".join(struct.pack(f"<{k.BLOCK}f", *s.model_block(fc)) for fc in s.CUTOFFS)
    return filler + body + struct.pack("<f", 99.0)


def test_report_and_cli(tmp_path, capsys):
    code, lines = s.report(_image())
    assert code == 0 and len(lines) == 8
    assert "beta = 8.5" in lines[1] and "kernel 5" in lines[-1]
    f = tmp_path / "img.bin"
    f.write_bytes(_image())
    assert s.main([str(f)]) == 0
    assert "6 kernels" in capsys.readouterr().out
    empty = tmp_path / "e.bin"
    empty.write_bytes(bytes(256))
    assert s.main([str(empty)]) == 1
