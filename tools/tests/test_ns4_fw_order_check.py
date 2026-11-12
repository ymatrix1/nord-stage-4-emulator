from __future__ import annotations

from pathlib import Path

import ns4_fw_order_check as fc
import pytest


def fake_firmware(
    layer: list[str] | None = None, sound: list[str] | None = None, tail: bytes = b""
) -> bytes:
    layer = layer if layer is not None else [e.fw_name for e in fc.LAYER_CHAIN]
    sound = sound if sound is not None else [e.fw_name for e in fc.SOUND_CHAIN]
    parts = [b"junk\0", b"Buffer_kSynLay\0"]
    parts += [n.encode() + b"\0" for n in layer]
    parts.append(b"Buffer_kSynSnd\0")
    parts += [n.encode() + b"\0" for n in sound]
    parts.append(b"Buffer_kFx\0Edt_Fx_kOther\0")
    return b"".join(parts) + tail


def test_chains_are_internally_consistent_with_the_firmware_order():
    result = fc.check_firmware(fake_firmware())
    assert result == {"Buffer_kSynLay": [], "Buffer_kSynSnd": []}


def test_firmware_fields_stops_at_the_next_buffer_and_rejects_missing_buffer():
    names = fc.firmware_fields(fake_firmware(), "Buffer_kSynSnd")
    assert names[0] == "Edt_Syn_kOscCtrl"
    assert names[-1] == "Edt_Syn_kSmpTimbre"
    assert "Edt_Fx_kOther" not in names
    with pytest.raises(ValueError):
        fc.firmware_fields(b"nothing here", "Buffer_kSynLay")


def test_firmware_fields_stops_on_binary_garbage():
    fw = b"Buffer_kSynLay\0Edt_Syn_kMode\0\xff\xfe\0Edt_Syn_kAfter\0"
    assert fc.firmware_fields(fw, "Buffer_kSynLay") == ["Edt_Syn_kMode"]


def test_reordered_or_missing_firmware_fields_are_reported():
    sound = [e.fw_name for e in fc.SOUND_CHAIN]
    swapped = sound.copy()
    i, j = swapped.index("Edt_Syn_kLFODst"), swapped.index("Edt_Syn_kLFOWave")
    swapped[i], swapped[j] = swapped[j], swapped[i]
    problems = fc.check_firmware(fake_firmware(sound=swapped))["Buffer_kSynSnd"]
    assert any("order" in p for p in problems)

    missing = [n for n in sound if n != "Edt_Syn_kFltEnable"]
    problems = fc.check_firmware(fake_firmware(sound=missing))["Buffer_kSynSnd"]
    assert any("Edt_Syn_kFltEnable missing" in p for p in problems)


def test_wrong_width_breaks_the_contiguity_check():
    broken = tuple(
        e._replace(width=e.width + 1) if e.fw_name == "Edt_Syn_kVibRate" else e
        for e in fc.SOUND_CHAIN
    )
    names = [e.fw_name for e in broken]
    problems = fc.check_chain(names, broken)
    assert any("Edt_Syn_kVibRate: ends at" in p for p in problems)


def test_field_table_mismatch_and_overlap_are_reported():
    moved = tuple(
        e._replace(offset=e.offset + 1) if e.fw_name == "Edt_Syn_kAmpEnvVel" else e
        for e in fc.SOUND_CHAIN
    )
    problems = fc.check_chain([e.fw_name for e in moved], moved)
    assert any("amp_env_velocity" in p for p in problems)

    overlap = (fc.Entry("A", 10, 8, chained=False), fc.Entry("B", 12, 2))
    assert any("overlaps" in p for p in fc.check_chain(["A", "B"], overlap))
    unsorted = (fc.Entry("A", 20, 2), fc.Entry("B", 10, 2))
    assert any("increasing" in p for p in fc.check_chain(["A", "B"], unsorted))


def test_main_exit_codes(tmp_path: Path, capsys):
    good = tmp_path / "good.bin"
    good.write_bytes(fake_firmware())
    assert fc.main([str(good)]) == 0
    assert "0 problems" in capsys.readouterr().out

    bad = tmp_path / "bad.bin"
    sound = [e.fw_name for e in fc.SOUND_CHAIN if e.fw_name != "Edt_Syn_kVibAmt"]
    bad.write_bytes(fake_firmware(sound=sound))
    assert fc.main([str(bad)]) == 1
