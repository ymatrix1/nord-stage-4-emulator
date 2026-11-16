"""Tests for tools/synth_adapter.py (config pieces; PEFT and Transkun parts when installed)."""

from __future__ import annotations

import re
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import synth_adapter as sa  # noqa: E402


def test_target_regex_matches_adaptable_layers_only():
    ok = [
        "backbone.encoderLayers.3.mhaBlockF.module.out_proj",
        "backbone.encoderLayers.0.fnnBlockT.module.0",
        "backbone.encoderLayers.5.fnnBlockF.module.3",
        "backbone.inputConv",
        "backbone.downConv.1",
        "backbone.downConv.13",
        "scorer.map.0",
    ]
    no = [
        "backbone.downConv.0",  # padding
        "backbone.downConv.2",  # activation
        "backbone.downConv.14",
        "backbone.upConv1dSkip",  # ConvTranspose1d, unsupported
        "velocityPredictor.0",  # heads are trained in full instead
        "backbone.encoderLayers.0.fnnBlockF.module.1",  # GELU
    ]
    assert all(re.fullmatch(sa.TARGET_MODULES, n) for n in ok)
    assert not any(re.fullmatch(sa.TARGET_MODULES, n) for n in no)


def test_qkv_parameter_names():
    assert len(sa.QKV_PARAMETERS) == 36 and len(set(sa.QKV_PARAMETERS)) == 36
    assert "backbone.encoderLayers.5.mhaBlockT.module.v_proj_weight" in sa.QKV_PARAMETERS


def test_arm_settings_defaults_and_errors():
    assert sa.arm_settings("lokr", None, None) == {
        "r": 16,
        "alpha": 32,
        "decompose_factor": 4,
        "rank_dropout": 0.0,
    }
    assert sa.arm_settings("lokr", 8, 2)["decompose_factor"] == 2
    lora = sa.arm_settings("lora", 32, None)
    assert lora["r"] == 32 and lora["lora_alpha"] == 64
    assert lora["lora_dropout"] == 0.0  # PEFT rejects dropout together with target_parameters
    with pytest.raises(ValueError):
        sa.arm_settings("loha", None, None)


def test_build_config_lokr_has_no_parameter_targets_lora_has():
    pytest.importorskip("peft")
    lokr, lora = sa.build_config("lokr"), sa.build_config("lora")
    assert lokr.modules_to_save == sa.HEADS and not hasattr(lokr, "target_parameters")
    assert lora.target_parameters == sa.QKV_PARAMETERS
    assert lora.target_modules == sa.TARGET_MODULES


def test_attach_save_load_round_trip_leaves_base_unchanged(tmp_path):
    torch = pytest.importorskip("torch")
    pytest.importorskip("peft")
    pytest.importorskip("transkun")
    base = sa.load_base("cpu")
    ref = {k: v.clone() for k, v in base.state_dict().items()}
    pm = sa.attach(base, "lokr")
    assert 100_000 < sa.count_trainable(pm) < 2_000_000
    sa.save_adapter(pm, tmp_path / "ad")
    assert (tmp_path / "ad" / "adapter_model.safetensors").exists()
    again = sa.load_adapter(tmp_path / "ad")
    # base weights of the wrapped layers are the shipped ones (adapter init is a no-op or small)
    name = "backbone.inputConv.base_layer.weight"
    got = dict(again.named_parameters())
    assert name in got and torch.equal(got[name].detach(), ref["backbone.inputConv.weight"])


def test_restore_adapter_reproduces_saved_weights(tmp_path):
    torch = pytest.importorskip("torch")
    pytest.importorskip("peft")
    pytest.importorskip("transkun")
    first = sa.attach(sa.load_base("cpu"), "lokr")
    with torch.no_grad():
        for p in first.parameters():
            if p.requires_grad:
                p.add_(0.01)
    sa.save_adapter(first, tmp_path / "ad")
    second = sa.attach(sa.load_base("cpu"), "lokr")
    sa.restore_adapter(second, tmp_path / "ad")
    a, b = dict(first.named_parameters()), dict(second.named_parameters())
    trained = [k for k, p in a.items() if p.requires_grad]
    assert trained and all(torch.equal(a[k], b[k]) for k in trained)


def test_scale_adapter_interpolates_heads_and_scales_layers():
    torch = pytest.importorskip("torch")

    class Layer(torch.nn.Module):  # stands in for a PEFT LoRA layer
        def __init__(self):
            super().__init__()
            self.scaling = {"default": 2.0}

    class Head(torch.nn.Module):  # stands in for PEFT's ModulesToSaveWrapper
        def __init__(self):
            super().__init__()
            self.original_module = torch.nn.Linear(2, 2)
            self.modules_to_save = torch.nn.ModuleDict({"default": torch.nn.Linear(2, 2)})

    net = torch.nn.Sequential(Layer(), Head())
    head = net[1]
    base = {n: p.clone() for n, p in head.original_module.named_parameters()}
    trained = {n: p.detach().clone() for n, p in head.modules_to_save["default"].named_parameters()}
    for s in (0.0, 0.5, 1.0, 0.25):  # rescaling starts from the trained values each time
        sa.scale_adapter(net, s)
        assert net[0].scaling["default"] == pytest.approx(2.0 * s)
        for n, p in head.modules_to_save["default"].named_parameters():
            assert torch.allclose(p, base[n] + s * (trained[n] - base[n]), atol=1e-6)
