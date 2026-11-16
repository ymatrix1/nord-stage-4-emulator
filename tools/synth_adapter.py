#!/usr/bin/env python3
"""PEFT adapters for Transkun 2.0 (LoKr / LoRA), kept apart from the base weights.

The base ``2.0.pt`` is never modified: ``attach`` wraps the layers in place in memory and
``save_adapter`` writes only the adapter (and the fully trained heads) to a folder.
Layer names are VERIFIED with ``named_modules`` (``docs/research/transkun-synth-finetune-plan.md``
section 4): FFN Linear ``module.0/3``, attention ``out_proj``, ``inputConv``, ``downConv.1/5/9/13``
(the other ``downConv`` entries are padding or activation layers), ``scorer.map.0``. The raw
q/k/v ``nn.Parameter`` tensors need ``target_parameters``, which LoRA supports (PEFT 0.21.3)
and LoKr does not (VERIFIED: ``LoKrConfig`` has no such argument), so the LoKr arm leaves them
frozen. ``ConvTranspose1d`` is not supported by either and stays frozen.

LoKr size (VERIFIED, ``lokr_count.py``): the adapter weights barely depend on ``r`` (37,296 at
r=8, 40,456 at r=16 or 32 with the default factor) but on ``decompose_factor``: 103,984 at
factor 8, 183,028 at 4, 364,054 at 2 (r=16). Hence the arm uses r=16, factor 4.

Torch, peft and transkun are imported inside the functions, so the module imports without them.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

HEADS = ["velocityPredictor", "refinedOFPredictor"]
TARGET_MODULES = (
    r"backbone\.(encoderLayers\.\d+\.(mhaBlock|fnnBlock)[FT]\.module(\.out_proj|\.[03])"
    r"|inputConv|downConv\.(1|5|9|13))|scorer\.map\.0"
)
QKV_PARAMETERS = [
    f"backbone.encoderLayers.{i}.mhaBlock{a}.module.{k}_proj_weight"
    for i in range(6)
    for a in "FT"
    for k in "qkv"
]
ARMS = ("lokr", "lora")


def arm_settings(arm: str, r: int | None, factor: int | None) -> dict[str, Any]:
    """Keyword arguments of the PEFT config for an arm (defaults from the plan)."""
    if arm == "lokr":
        r = r or 16
        return {"r": r, "alpha": 2 * r, "decompose_factor": factor or 4, "rank_dropout": 0.0}
    if arm == "lora":
        r = r or 16
        # VERIFIED: PEFT 0.21.3 raises "ParamWrapper does not work with lora_dropout != 0"
        # as soon as ``target_parameters`` (the raw q/k/v tensors) is set, so dropout is 0.
        return {"r": r, "lora_alpha": 2 * r, "lora_dropout": 0.0, "bias": "none"}
    raise ValueError(f"unknown arm {arm!r}, expected one of {ARMS}")


def build_config(arm: str, r: int | None = None, factor: int | None = None) -> Any:
    from peft import LoKrConfig, LoraConfig

    kw = arm_settings(arm, r, factor)
    if arm == "lokr":
        return LoKrConfig(target_modules=TARGET_MODULES, modules_to_save=HEADS, **kw)
    return LoraConfig(
        target_modules=TARGET_MODULES,
        target_parameters=QKV_PARAMETERS,
        modules_to_save=HEADS,
        **kw,
    )


def load_base(device: str = "cpu") -> Any:
    """Pretrained Transkun 2.0 as shipped (needs torch + transkun)."""
    import moduleconf
    import torch
    import transkun

    root = Path(transkun.__file__).parent / "pretrained"
    mgr = moduleconf.parseFromFile(str(root / "2.0.conf"))
    model = mgr["Model"].module.TransKun(conf=mgr["Model"].config)
    ck = torch.load(root / "2.0.pt", map_location="cpu")
    model.load_state_dict(ck.get("best_state_dict", ck.get("state_dict")), strict=False)
    return model.to(device)


def attach(model: Any, arm: str, r: int | None = None, factor: int | None = None) -> Any:
    """Wraps ``model`` in place and returns the ``PeftModel``; ``model`` itself keeps working."""
    from peft import get_peft_model

    return get_peft_model(model, build_config(arm, r, factor))


def count_trainable(model: Any) -> int:
    return sum(p.numel() for p in model.parameters() if p.requires_grad)


def save_adapter(peft_model: Any, folder: Path) -> None:
    Path(folder).mkdir(parents=True, exist_ok=True)
    peft_model.save_pretrained(str(folder))


def restore_adapter(peft_model: Any, folder: Path) -> None:
    """Loads adapter weights from ``folder`` into an already attached ``peft_model`` (resume)."""
    from peft import set_peft_model_state_dict
    from safetensors.torch import load_file

    weights = load_file(str(Path(folder) / "adapter_model.safetensors"))
    result = set_peft_model_state_dict(peft_model, weights)
    unexpected = getattr(result, "unexpected_keys", None)
    if unexpected:
        raise ValueError(f"adapter in {folder} does not match this arm: {unexpected[:3]}")


def scale_adapter(model: Any, s: float) -> None:
    """Scales the adapter's effect to ``s`` times the trained one (``s=0`` is the base model).

    ``model`` is what ``load_adapter`` returns. LoRA / LoKr layers get ``scaling * s`` (their
    output is added linearly), and the fully trained heads are interpolated between the base head
    and the trained one: ``base + s * (trained - base)``. Calling it again rescales from the
    trained values, not from the previous call.
    """
    import torch

    for m in model.modules():
        scaling = getattr(m, "scaling", None)
        if isinstance(scaling, dict):
            if not hasattr(m, "_ns4_scaling0"):
                m._ns4_scaling0 = dict(scaling)
            for name, value in m._ns4_scaling0.items():
                scaling[name] = value * s
        if hasattr(m, "original_module") and hasattr(m, "modules_to_save"):
            base = dict(m.original_module.named_parameters())
            for name, trained in m.modules_to_save.items():
                if not hasattr(m, "_ns4_trained0"):
                    m._ns4_trained0 = {}
                saved = m._ns4_trained0.setdefault(
                    name, {n: p.detach().clone() for n, p in trained.named_parameters()}
                )
                with torch.no_grad():
                    for n, p in trained.named_parameters():
                        p.copy_(base[n] + s * (saved[n] - base[n]))


def load_adapter(folder: Path, device: str = "cpu") -> Any:
    """Base model plus the adapter from ``folder``; returns the model to call ``transcribe`` on."""
    from peft import PeftModel

    peft_model = PeftModel.from_pretrained(load_base(device), str(folder))
    return peft_model.base_model.model.eval()
