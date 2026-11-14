"""Tests for tools/fx_models.py (routine registry and command line)."""

import fx_models
import pytest


def test_registry_lists_every_modelled_routine():
    rts = fx_models.routines()
    assert set(rts) == set(fx_models.MODULES)
    assert {
        "delay", "lfo", "allpass", "apan", "resonant", "resonant4", "chor4", "chor5",
        "vibe", "flang", "phas0", "phas1", "ens", "spin",
    } == set(rts)  # fmt: skip
    assert all(rt.pc < rt.end and rt.sets["M2"] == 2 for rt in rts.values())


def test_digest_is_stable_and_command_line_prints_it(capsys):
    rt = fx_models.routines()["delay"]
    assert fx_models.digest(rt) == fx_models.digest(rt)
    assert fx_models.main(["digest", "delay"]) == 0
    assert capsys.readouterr().out.strip() == fx_models.digest(rt)


def test_unknown_routine_and_missing_binary_are_rejected(tmp_path, monkeypatch):
    with pytest.raises(SystemExit):
        fx_models.main(["digest", "nonexistent"])
    assert fx_models.find_binary(str(tmp_path / "x")) == tmp_path / "x"
    monkeypatch.setattr(fx_models.Path, "exists", lambda self: False)
    with pytest.raises(SystemExit, match="sharc-fx not built"):
        fx_models.find_binary(None)
