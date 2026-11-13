import os
import struct
import sys

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import synth_stage_oracle as o  # noqa: E402


def test_float_word_conversions_round_trip():
    assert o.f2u(1.0) == 0x3F800000
    assert o.u2f(0x3F000000) == 0.5
    assert o.u2f(o.f2u(-0.25)) == -0.25


def test_patch_controls_turns_three_stores_into_nops():
    image = bytes(range(1, 256)) * 0x2600  # about 2.4 MB of non-zero filler
    patched = o.patch_controls(image)
    assert len(patched) == len(image)
    for pc in o.CONTROL_STORES:
        words = struct.unpack_from("<3H", patched, 2 * pc)
        assert words == (o.NOP,) * 3
    assert patched[:16] == image[:16]


def test_message_script_places_samples_and_frame_words():
    script, passes = o.message_script(
        0x12566E, {-30: (0x3E99999A, 0x3E99999B)}, [1.0, 0.0, 0.0, 0.0, 0.5], None
    )
    assert passes == 2
    lines = script.splitlines()
    assert f"0 {o.B_ENTRY:#x} 0x12566e" in lines
    assert f"0 {o.A_ENTRY:#x} {o.STUB_A:#x}" in lines
    assert f"0 {o.FRAME - 30:#x} 0x3e99999a" in lines
    assert f"0 {o.FRAME - 29:#x} 0x3e99999b" in lines
    assert f"0 {o.IN0:#x} 0x3f800000" in lines
    assert f"1 {o.IN0:#x} 0x3f000000" in lines  # sample 4 starts pass 1
    # the second pass is padded with zeros up to the block of 4 samples
    assert f"1 {o.IN0 + 6:#x} 0x0" in lines


def test_default_paths_use_the_environment(monkeypatch):
    monkeypatch.setenv("NS4_SHARC_ORG", "x/org")
    monkeypatch.setenv("NS4_ORG_IMAGE", "x/img.bin")
    assert o.default_paths() == ("x/org", "x/img.bin")
