#!/usr/bin/env python3
# ruff: noqa: E501
"""Factory program sweep: engine-path coverage and render health of every `.ns4p` program.

Drives `crates/nord-core/examples/program_sweep.rs` (release build; one JSON line per program:
decoded program, resolved sounds, routing facts, render statistics for the engine defaults and
for `--firmware-all --firmware-routing`), then derives

* which engine paths each program needs (organ model, piano class, synth wave, filter, FX types,
  splits and zones) and how each path is classified (`verified`, `assumed`, `unmodelled`) under
  the defaults and under the firmware-all switch set,
* render outliers (panic, NaN/Inf, silence, clipping, large default-vs-firmware level jumps).

Only derived numbers are written (`docs/research/data/program-sweep.json`,
`docs/research/program-coverage.md`): no audio, no firmware, no sample content.

Usage:
    cargo build --release -p nord-core --example program_sweep
    uv run python tools/program_sweep.py --ns4b ".reference/<backup>.ns4b"
    uv run python tools/program_sweep.py --jsonl sweep.jsonl      # reuse a previous run
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from collections import Counter, defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DOC_PATH = ROOT / "docs" / "research" / "program-coverage.md"
JSON_PATH = ROOT / "docs" / "research" / "data" / "program-sweep.json"
EXAMPLE = ROOT / "target" / "release" / "examples" / "program_sweep"

LAYERS = ["organ_a", "organ_b", "piano_a", "piano_b", "synth_a", "synth_b", "synth_c"]
ANALOG_CATEGORIES = ["PURE", "SUB OSC", "SYNC", "SHAPE", "SHAPE SINE", "MULTI", "SUPER", "MISC"]
WAVE_CATEGORIES = ["BELLS/TINES", "ACOUSTIC", "DIGITAL", "ORGAN", "KEYS"]
MOD1 = ["A-PAN", "TREM", "RM", "A-WAH", "WAH", "PUMP"]
MOD2 = ["PHAS", "FLANG", "CHOR", "VIBE", "ENS", "SPIN"]
AMP = ["off", "TWIN", "TWIN II", "JC", "JC II", "SMALL", "VERY SMALL", "TO ROTARY", "LP24", "HP24"]
REVERB = ["SPRING", "BOOTH", "ROOM", "STAGE", "HALL", "CATHEDRAL"]
DELAY_FB = ["none", "ENS", "VIBE", "CHOR", "FLAM", "SPACE"]
PIANO_EXTRA_VOICE = {1, 2, 3, 5, 6}
PIANO_MAIN_ONLY = {4, 7, 14, 15, 16}

# Output-health thresholds (tool conventions, not hardware facts).
CLIP_FRACTION = 1e-3
JUMP_DB = 6.0

# Path classification: key -> (default, firmware-all, basis). `verified` = a firmware-verified
# algorithm runs, `assumed` = engine approximation or legacy model, `unmodelled` = pass-through,
# placeholder or decoded-but-ignored. Basis cites the spec / research note the reading comes from.
V, A, N = "verified", "assumed", "unmodelled"
STATUS: dict[str, tuple[str, str, str]] = {
    "organ.model=B3": (
        A,
        A,
        "organ-model-paths: B3 tone source not located; Legacy wheels; firmware-all forces the Pipe bank (attribution disproved)",
    ),
    "organ.model=Vox": (
        A,
        A,
        "organ-model-paths: Vox bank is VERIFIED but opt-in (`--tone-source vox`), not in firmware-all",
    ),
    "organ.model=Farfisa": (
        A,
        A,
        "organ-model-paths: Farfisa tone source not located; Legacy wheels",
    ),
    "organ.model=Pipe1": (
        V,
        V,
        "organ-model-paths: 92-wheel bank VERIFIED, role for the pipes ASSUMED",
    ),
    "organ.model=Pipe2": (
        V,
        V,
        "organ-model-paths: 92-wheel bank VERIFIED, role for the pipes ASSUMED",
    ),
    "organ.model=B3 Bass": (
        A,
        A,
        "organ-model-paths: raw model 5 decodes to B3 plus the bass flag (was folded into Pipe2); bass processor 0x122296 not bit-exact (OQ18)",
    ),
    "organ.rotary": (
        A,
        A,
        "organ-engine: Rotary 122 model ASSUMED; Leslie routine not wired into the program path",
    ),
    "organ.vibrato": (A, A, "organ-engine: scanner vibrato/chorus ASSUMED (B3 tail opt-in)"),
    "organ.percussion": (A, A, "organ-engine: percussion ASSUMED"),
    "organ.drawbars": (A, A, "organ-engine: drawbar scaling per model, Legacy for non-Pipe"),
    "piano.sample=loaded": (
        V,
        V,
        "piano-engine: .npno stream decode and firmware level law (D23 default)",
    ),
    "piano.sample=missing": (N, N, "no .npno resolved or loadable: synthetic fallback voice"),
    "piano.extra_voice=not_played": (
        A,
        A,
        "piano-sound-load: classes 1,2,3,5,6 start an extra voice the engine does not play",
    ),
    "piano.class=unchecked": (
        A,
        A,
        "piano-sound-load: level law checked on classes 1-7, 14-16 only",
    ),
    "synth.osc=Analog": (A, A, "synth-engine: VA waveforms approximate the strip stages"),
    "synth.osc=Wavetable": (N, N, "synth-engine: WAVE bank is a synthetic 64-table placeholder"),
    "synth.osc=Fm": (A, A, "synth-engine: FM ratio index decoded, wave audio ASSUMED"),
    "synth.osc=Sample": (
        V,
        V,
        "synth-engine: .nsmp4 PCM played through the verified 12-tap sinc kernels (kernel choice ASSUMED)",
    ),
    "synth.sample=missing": (N, N, "no .nsmp4 resolved or loadable"),
    **{
        f"synth.sample.kernel={k}": (
            A,
            A,
            "synth-sample-kernel-rule: kernel chosen by key, VERIFIED run of the ARM chain and key"
            " field (D40, default); zone pitch key and tune of the sample map ASSUMED",
        )
        for k in range(6)
    },
    "synth.analog=PURE": (
        A,
        V,
        "synth-wave-models: PURE saw/sine/triangle/pulse stages verified on the oracle; only the saw is wired (strip chain, firmware-all)",
    ),
    "synth.analog=SHAPE": (
        A,
        A,
        "synth-wave-models: Shape Saw/Triangle stages verified, shape word unknown; opt-in `--synth-shape`",
    ),
    "synth.analog=SUB OSC": (N, N, "synth-wave-models: not modelled"),
    "synth.analog=SYNC": (N, N, "synth-wave-models: not modelled"),
    "synth.analog=SHAPE SINE": (N, N, "synth-wave-models: not modelled"),
    "synth.analog=MULTI": (N, N, "synth-wave-models: not modelled"),
    "synth.analog=SUPER": (
        A,
        A,
        "synth-engine: Super Saw EMPIRICAL voice spread, stage not modelled",
    ),
    "synth.analog=MISC": (N, N, "synth-wave-models: not modelled"),
    "synth.filter=Lp12": (
        A,
        V,
        "synth-engine: ZDF filter by default; firmware LP4 stage VERIFIED, type map ASSUMED",
    ),
    "synth.filter=Lp24": (
        A,
        V,
        "synth-engine: ZDF filter by default; firmware LP4 stage VERIFIED, type map ASSUMED",
    ),
    "synth.filter=Ladder24": (
        A,
        V,
        "synth-engine: ZDF filter by default; firmware LP4 stage VERIFIED, type map ASSUMED",
    ),
    "synth.filter=BandPass": (A, V, "synth-engine: firmware BP4 stage VERIFIED, type map ASSUMED"),
    "synth.filter=HighPass": (A, V, "synth-engine: firmware HP4 stage VERIFIED, type map ASSUMED"),
    "synth.envelopes": (
        A,
        V,
        "synth-engine: strip chain records VERIFIED bit exact, panel mapping ASSUMED",
    ),
    "synth.unison": (A, A, "synth-engine: ratios VERIFIED, voice count/level EMPIRICAL/ASSUMED"),
    "synth.arp": (A, A, "synth-engine: arpeggiator ASSUMED (pattern table not located)"),
    "fx.mod1": (A, A, "effects-and-routing: Mod 1 types are engine models, constants ASSUMED"),
    "fx.mod2": (A, A, "effects-and-routing: Mod 2 types are engine models, constants ASSUMED"),
    "fx.mod2=ENS": (A, A, "effects-and-routing: Ensemble approximated (DSP not in ARM image)"),
    "fx.mod2=SPIN": (A, A, "effects-and-routing: Spin approximated (DSP not in ARM image)"),
    "fx.amp": (
        V,
        V,
        "effects-and-routing: firmware drive stage and cabinet are the default amp path (words VERIFIED, voicing ASSUMED)",
    ),
    "fx.amp=TO ROTARY": (N, N, "effects-and-routing: To Rotary is a pass-through"),
    "fx.amp=LP24": (A, A, "effects-and-routing: topology ASSUMED"),
    "fx.amp=HP24": (A, A, "effects-and-routing: topology ASSUMED"),
    "fx.eq": (A, A, "effects-and-routing: RBJ biquads; gain step and Q ASSUMED"),
    "fx.comp": (
        A,
        V,
        "effects-and-routing: firmware compressor routine VERIFIED, parameters and bus position ASSUMED",
    ),
    "fx.delay": (
        A,
        V,
        "effects-and-routing: firmware time/mix words VERIFIED (firmware_fx.delay_laws), loop is the engine's",
    ),
    "fx.delay_fb": (A, A, "effects-and-routing: feedback effects approximated"),
    "fx.reverb": (
        A,
        A,
        "effects-and-routing: reverb network is the engine FDN; wrap routine not wired; mix law verified under firmware-all",
    ),
    "routing.split": (
        A,
        V,
        "morph-and-zones: splits/zones follow the program only with --firmware-routing (zone codes partly verified)",
    ),
}


def status(path: str) -> tuple[str, str, str]:
    """Classification of `path`; unknown paths are reported as assumed with a marker basis."""
    return STATUS.get(path, (A, A, "no classification recorded"))


def layer_masks(rec: dict) -> tuple[int, int]:
    """Scene I / II layer masks (bit i = layer i of [`LAYERS`]) as the program stores them."""
    m = rec["routing"]["exact_masks"]
    return int(m[0]), int(m[1])


def used_layers(rec: dict) -> list[int]:
    """Indices of layers that sound in either scene (section enable AND layer enable)."""
    m1, m2 = layer_masks(rec)
    return [i for i in range(7) if ((m1 | m2) >> i) & 1]


def _name(table: list[str], idx: int) -> str:
    return table[idx] if 0 <= idx < len(table) else f"raw{idx}"


def fx_paths(fx: dict) -> list[str]:
    """Engine paths a layer FX block needs (enabled units only)."""
    out: list[str] = []
    if fx.get("mod1_enabled"):
        out += ["fx.mod1", f"fx.mod1={_name(MOD1, fx['mod1_type_raw'] // 2)}"]
    if fx.get("mod2_enabled"):
        out += ["fx.mod2", f"fx.mod2={_name(MOD2, fx['mod2_type_raw'] // 2)}"]
    if fx.get("amp_enabled"):
        mode = int(fx["amp_type_new"]) & 0xF
        out += ["fx.amp", f"fx.amp={_name(AMP, mode)}"]
    if any(abs(fx.get(k, 0.0)) > 0.0 for k in ("eq_treble_db", "eq_mid_db", "eq_bass_db")):
        out.append("fx.eq")
    if fx.get("comp_enabled"):
        out.append("fx.comp")
    if fx.get("delay_enabled"):
        out += ["fx.delay", f"fx.delay.mode={fx['delay_mode']}"]
        if fx["delay_fb_fx_type"] >= 2:
            out += ["fx.delay_fb", f"fx.delay_fb={_name(DELAY_FB, fx['delay_fb_fx_type'] // 2)}"]
    if fx.get("reverb_enabled"):
        out += ["fx.reverb", f"fx.reverb={_name(REVERB, fx['reverb_type_raw'] // 2)}"]
        out.append(f"fx.reverb.type={fx['reverb_type_raw']}")
    return out


def _synth_paths(
    s: dict, loaded: bool, has_ref: bool, kernels: list[int] | None = None
) -> list[str]:
    out = [f"synth.osc={s['osc_mode']}"]
    if s["osc_mode"] == "Analog":
        out.append(f"synth.analog={_name(ANALOG_CATEGORIES, s['osc_category'])}")
        out.append(
            f"synth.analog.wave={_name(ANALOG_CATEGORIES, s['osc_category'])}:{s['osc_wave']}"
        )
    elif s["osc_mode"] == "Wavetable":
        out.append(f"synth.wave={_name(WAVE_CATEGORIES, s['osc_category'])}")
    elif s["osc_mode"] == "Sample":
        out.append("synth.sample=loaded" if loaded else "synth.sample=missing")
        out += [f"synth.sample.kernel={k}" for k in kernels or []]
    if s["filter_on"]:
        out.append(f"synth.filter={s['filter_model']}")
    out.append("synth.envelopes")
    if s["unison"]:
        out.append(f"synth.unison={s['unison']}")
    if s["arp_enabled"]:
        out.append("synth.arp")
    if s["mono"]:
        out.append("synth.mono")
    out.append(f"synth.lfo_dest={s['lfo_dest']}")
    return out


def _piano_paths(p: dict, snd: dict, cls: int | None, tag: str) -> list[str]:
    out = [f"piano.category={p['category']}"]
    if snd["loaded"]:
        out.append("piano.sample=loaded")
        if cls is not None:
            out.append(f"piano.class={cls}")
            if cls in PIANO_EXTRA_VOICE:
                out.append("piano.extra_voice=not_played")
            elif cls not in PIANO_MAIN_ONLY:
                out.append("piano.class=unchecked")
    else:
        out.append("piano.sample=missing")
    if p["string_res"]:
        out.append("piano.string_res")
    return out


def _organ_paths(prog: dict, layer: str) -> list[str]:
    o = prog["organ"]
    model = o["model"] if layer == "a" else o["model_b"]
    bass = prog["organ"].get("b3_bass_a" if layer == "a" else "b3_bass_b", False)
    out = ["organ.model=B3 Bass" if bass else f"organ.model={model}"]
    if o["rotary_enabled"]:
        out += ["organ.rotary", f"organ.rotary_speed={o['rotary_speed']}"]
    vib_on = o["vibrato_on"] if layer == "a" else o["vibrato_on_b"]
    if vib_on:
        out += ["organ.vibrato", f"organ.vibrato_type={o['vibrato_type']}"]
    if o["percussion_on"]:
        out.append("organ.percussion")
    out.append("organ.drawbars")
    return out


def program_paths(rec: dict) -> dict[str, list[str]]:
    """Paths needed per used layer, plus program level paths under the key `program`."""
    prog = rec["program"]
    snd = rec["sounds"]
    used = used_layers(rec)
    out: dict[str, list[str]] = {}
    for i in used:
        name = LAYERS[i]
        if name.startswith("organ"):
            out[name] = _organ_paths(prog, name[-1])
        elif name.startswith("piano"):
            sub = "layer_" + name[-1]
            out[name] = _piano_paths(prog["piano"][sub], snd[name], snd.get(name + "_class"), name)
        else:
            sub = "layer_" + name[-1]
            has_ref = snd[name]["ref"]
            out[name] = _synth_paths(
                prog["synth"][sub], snd[name]["loaded"], has_ref, snd.get(name + "_kernels")
            )
    fx_blocks = {
        "organ": (prog["organ"]["fx"], {0, 1}),
        "piano_a": (prog["piano"]["fx_a"], {2}),
        "piano_b": (prog["piano"]["fx_b"], {3}),
        "synth_a": (prog["synth"]["fx_a"], {4}),
        "synth_b": (prog["synth"]["fx_b"], {5}),
        "synth_c": (prog["synth"]["fx_c"], {6}),
    }
    fx: list[str] = []
    for fx_block, owners in fx_blocks.values():
        if owners & set(used):
            fx += fx_paths(fx_block)
    out["fx"] = fx
    g = prog["global"]
    prog_paths = []
    if g["split_enabled"]:
        points = [n for n, on in zip(("low", "mid", "high"), g["split_zone_enabled"]) if on]
        prog_paths += ["routing.split", f"routing.split_points={len(points)}"]
    m1, m2 = layer_masks(rec)
    prog_paths.append(f"routing.scene_ii={'used' if m2 else 'empty'}")
    zones = {rec["routing"]["kb_zone"][i] for i in used}
    for z in sorted(zones):
        prog_paths.append(f"routing.kb_zone={z}")
    if len(zones) > 1 or (zones and next(iter(zones)) != 5):
        prog_paths.append("routing.kb_zone_nondefault")
    prog_paths.append(f"routing.layers_used={len(used)}")
    engines = {n.split("_")[0] for n in (LAYERS[i] for i in used)}
    prog_paths.append("routing.engines=" + ("+".join(sorted(engines)) or "none"))
    out["program"] = prog_paths
    return out


def aggregate(records: list[dict]) -> dict[str, dict[str, int]]:
    """Per path: programs that use it and (layer or FX-unit) occurrences."""
    programs: dict[str, set[int]] = defaultdict(set)
    uses: Counter = Counter()
    for idx, rec in enumerate(records):
        for paths in program_paths(rec).values():
            for p in paths:
                programs[p].add(idx)
                uses[p] += 1
    return {p: {"programs": len(programs[p]), "uses": uses[p]} for p in sorted(programs)}


def render_row(rec: dict) -> dict:
    """Numbers of one program's two renders."""
    out: dict = {"entry": rec["entry"], "rms_delta_db": rec.get("rms_delta_db")}
    for mode in ("default", "firmware"):
        r = rec["renders"][mode]
        if "panic" in r:
            out[mode] = {"panic": r["panic"]}
        else:
            n = max(1, 2 * r["frames"])
            out[mode] = {
                "finite": r["nan"] == 0 and r["inf"] == 0,
                "nan": r["nan"],
                "inf": r["inf"],
                "peak": round(r["peak"], 4),
                "rms": round(r["rms"], 5),
                "clipped": r["clipped"],
                "clip_fraction": round(r["clipped"] / n, 6),
                "silent": r["silent"],
            }
    return out


def outliers(rows: list[dict]) -> dict[str, list[dict]]:
    """Programs that panic, go non-finite, fall silent, clip or jump in level."""
    found: dict[str, list[dict]] = {
        "panic": [],
        "non_finite": [],
        "silent": [],
        "clipping": [],
        "level_jump": [],
    }
    for r in rows:
        for mode in ("default", "firmware"):
            m = r[mode]
            if "panic" in m:
                found["panic"].append({"entry": r["entry"], "mode": mode, "message": m["panic"]})
                continue
            if not m["finite"]:
                found["non_finite"].append(
                    {"entry": r["entry"], "mode": mode, "nan": m["nan"], "inf": m["inf"]}
                )
            if m["silent"]:
                found["silent"].append({"entry": r["entry"], "mode": mode, "peak": m["peak"]})
            if m["clip_fraction"] >= CLIP_FRACTION:
                found["clipping"].append(
                    {
                        "entry": r["entry"],
                        "mode": mode,
                        "peak": m["peak"],
                        "clip_fraction": m["clip_fraction"],
                    }
                )
        d = r["rms_delta_db"]
        if d is not None and abs(d) >= JUMP_DB:
            found["level_jump"].append({"entry": r["entry"], "rms_delta_db": round(d, 2)})
    found["clipping"].sort(key=lambda x: -x["clip_fraction"])
    found["level_jump"].sort(key=lambda x: -abs(x["rms_delta_db"]))
    return found


def level_summary(rows: list[dict]) -> dict:
    """Distribution of the default-vs-firmware RMS difference (dB)."""
    ds = sorted(r["rms_delta_db"] for r in rows if r["rms_delta_db"] is not None)
    if not ds:
        return {}

    def q(p: float) -> float:
        return round(ds[min(len(ds) - 1, int(p * (len(ds) - 1) + 0.5))], 2)

    return {
        "n": len(ds),
        "min": round(ds[0], 2),
        "p10": q(0.1),
        "median": q(0.5),
        "p90": q(0.9),
        "max": round(ds[-1], 2),
        "mean": round(sum(ds) / len(ds), 2),
    }


def classification_table(agg: dict[str, dict[str, int]], n_programs: int) -> list[dict]:
    """Rows for every classified path that occurs, with its status in both configurations."""
    rows = []
    for path, counts in agg.items():
        if path not in STATUS:
            continue
        d, f, basis = STATUS[path]
        rows.append({"path": path, **counts, "default": d, "firmware_all": f, "basis": basis})
    return rows


def status_totals(rows: list[dict]) -> dict[str, dict[str, int]]:
    """Layer/FX-unit occurrences grouped by status for each configuration."""
    out = {"default": Counter(), "firmware_all": Counter()}
    for r in rows:
        out["default"][r["default"]] += r["uses"]
        out["firmware_all"][r["firmware_all"]] += r["uses"]
    return {k: dict(v) for k, v in out.items()}


def analyze(records: list[dict]) -> dict:
    """Everything the document and the data file report."""
    agg = aggregate(records)
    rows = [render_row(r) for r in records]
    table = classification_table(agg, len(records))
    return {
        "programs": len(records),
        "paths": agg,
        "classification": table,
        "status_totals": status_totals(table),
        "renders": rows,
        "outliers": outliers(rows),
        "level_delta_db": level_summary(rows),
    }


def _pct(n: int, total: int) -> str:
    return f"{100.0 * n / total:.0f}%" if total else "-"


def _group(agg: dict[str, dict[str, int]], prefix: str) -> list[tuple[str, dict[str, int]]]:
    return [(p, c) for p, c in agg.items() if p.startswith(prefix)]


def markdown(result: dict, meta: dict) -> str:
    """Renders `docs/research/program-coverage.md` from `result` (derived numbers only)."""
    n = result["programs"]
    agg = result["paths"]
    lines = [
        "# Research: factory program coverage and render health",
        "",
        f"- **Checked:** {meta['checked']}",
        f"- **Sources (local, never committed):** the factory restore backup `{meta['backup']}` "
        f"({n} `.ns4p` programs, its `.npno` and `.nsmp4` entries), `{meta['tool']}` driving "
        "`crates/nord-core/examples/program_sweep.rs` (release build), the classification of "
        "paths from `docs/specs/*.md` and `docs/research/*.md` cited per row.",
        f"- **Question:** how much of the factory program space does the emulator cover, which "
        f"engine paths do the {n} programs rely on, and which of them are firmware-verified, "
        "assumed or not modelled; does any program panic, go non-finite, fall silent or clip.",
        "",
        "This is a coverage and risk map, not an audio-quality claim. All numbers are derived "
        "from the decoded programs and from output statistics; no audio, firmware or sample "
        "content is stored.",
        "",
        "## 1. Method",
        "",
        "- Layer used = bit set in the scene I or scene II mask the program stores (section "
        "enable AND layer enable, `ProgramRouting::exact_masks`).",
        f"- Render: `{meta['frames']}` frames at 48 kHz, notes {meta['notes']} (C3, G3, E4, C5) "
        f"velocity {meta['velocity']}, released at frame {meta['hold']}, one engine per render, "
        "release build. The same note set drives every program, so a layer whose key zone "
        "excludes all four notes counts as silent here.",
        "- `default` = engine defaults. `firmware` = `--firmware-all --firmware-routing` "
        "(firmware tone source, filter, strip chain, FX ports, piano level, program routing).",
        f"- Output health: NaN/Inf counted, silent = peak < 1e-4, clipped = |x| >= 1.0; "
        f"clipping is reported at >= {CLIP_FRACTION:.1%} of samples, level jump at >= "
        f"{JUMP_DB:.0f} dB RMS difference.",
        "- Status per path: `verified` = a firmware-verified algorithm runs; `assumed` = engine "
        "approximation or legacy model; `unmodelled` = pass-through, placeholder or decoded and "
        "ignored. The classification is a reading of the cited specs, not a measurement.",
        "",
        "## 2. Engine usage",
        "",
    ]
    eng = _group(agg, "routing.engines=")
    lines += ["| Engine mix | Programs |", "|---|---|"]
    for p, c in sorted(eng, key=lambda x: -x[1]["programs"]):
        lines.append(f"| {p.split('=')[1]} | {c['programs']} ({_pct(c['programs'], n)}) |")
    lines.append("")
    layers = _group(agg, "routing.layers_used=")
    lines += ["| Layers sounding | Programs |", "|---|---|"]
    for p, c in sorted(layers, key=lambda x: int(x[0].split("=")[1])):
        lines.append(f"| {p.split('=')[1]} | {c['programs']} |")
    lines.append("")
    lines += [
        f"Scene II used by {agg.get('routing.scene_ii=used', {}).get('programs', 0)} programs, "
        f"split enabled in {agg.get('routing.split', {}).get('programs', 0)}, non-default key "
        f"zone code on a sounding layer in "
        f"{agg.get('routing.kb_zone_nondefault', {}).get('programs', 0)}.",
        "",
    ]
    lines += ["| Key zone code (sounding layers) | Programs |", "|---|---|"]
    for p, c in _group(agg, "routing.kb_zone="):
        lines.append(f"| {p.split('=')[1]} | {c['programs']} |")
    lines.append("")

    sections = [
        ("3. Organ", "organ.", ["organ.model=", "organ.rotary_speed=", "organ.vibrato_type="]),
        ("4. Piano", "piano.", ["piano.category=", "piano.sample=", "piano.class="]),
        (
            "5. Synth",
            "synth.",
            [
                "synth.osc=",
                "synth.analog=",
                "synth.wave=",
                "synth.sample=",
                "synth.filter=",
                "synth.unison=",
                "synth.lfo_dest=",
            ],
        ),
        (
            "6. Effects",
            "fx.",
            [
                "fx.mod1=",
                "fx.mod2=",
                "fx.amp=",
                "fx.delay.mode=",
                "fx.delay_fb=",
                "fx.reverb=",
                "fx.reverb.type=",
            ],
        ),
    ]
    for title, _prefix, groups in sections:
        lines += [f"## {title}", "", "| Path | Programs | Occurrences |", "|---|---|---|"]
        for g in groups:
            for p, c in _group(agg, g):
                lines.append(f"| `{p}` | {c['programs']} | {c['uses']} |")
        flags = [
            k
            for k in agg
            if k
            in (
                "organ.rotary",
                "organ.vibrato",
                "organ.percussion",
                "synth.arp",
                "synth.mono",
                "fx.mod1",
                "fx.mod2",
                "fx.amp",
                "fx.eq",
                "fx.comp",
                "fx.delay",
                "fx.reverb",
                "piano.string_res",
            )
            and k.startswith(_prefix)
        ]
        for k in flags:
            lines.append(f"| `{k}` (enabled) | {agg[k]['programs']} | {agg[k]['uses']} |")
        lines.append("")

    lines += [
        "## 7. Verification status of the paths in use",
        "",
        "Uses count a path once per layer or enabled FX unit that needs it; one layer or unit can "
        "appear in several rows (for example `fx.amp` and `fx.amp=LP24`), so the totals below are "
        "classified path uses, not distinct layers. `default` is the engine default, "
        "`firmware-all` is `--firmware-all` (routing aside).",
        "",
        "| Path | Programs | Uses | Default | Firmware-all | Basis |",
        "|---|---|---|---|---|---|",
    ]
    for r in result["classification"]:
        lines.append(
            f"| `{r['path']}` | {r['programs']} | {r['uses']} | {r['default']} | "
            f"{r['firmware_all']} | {r['basis']} |"
        )
    lines += ["", "| Configuration | verified | assumed | unmodelled |", "|---|---|---|---|"]
    for cfg, label in (("default", "default"), ("firmware_all", "firmware-all")):
        t = result["status_totals"][cfg]
        tot = sum(t.values())
        cells = " | ".join(
            f"{t.get(s, 0)} ({_pct(t.get(s, 0), tot)})"
            for s in ("verified", "assumed", "unmodelled")
        )
        lines.append(f"| {label} | {cells} |")
    lines += [
        "",
        "Use-weighted: a path used by two layers counts twice. A `verified` row means the algorithm is firmware-verified, not that "
        "its parameters or the panel mapping are.",
        "",
        "## 8. Render health",
        "",
    ]
    rows = result["renders"]
    for mode in ("default", "firmware"):
        ms = [r[mode] for r in rows if "panic" not in r[mode]]
        lines.append(
            f"- `{mode}`: {len(ms)} of {len(rows)} rendered without panic; non-finite "
            f"{sum(1 for m in ms if not m['finite'])}, silent {sum(1 for m in ms if m['silent'])}, "
            f"any clipped sample {sum(1 for m in ms if m['clipped'])}, clipping >= "
            f"{CLIP_FRACTION:.1%}: {sum(1 for m in ms if m['clip_fraction'] >= CLIP_FRACTION)}."
        )
    ld = result["level_delta_db"]
    if ld:
        lines += [
            "",
            f"Level difference firmware minus default (RMS, dB) over {ld['n']} programs: min "
            f"{ld['min']}, p10 {ld['p10']}, median {ld['median']}, p90 {ld['p90']}, max "
            f"{ld['max']}, mean {ld['mean']}.",
        ]
    out = result["outliers"]
    lines += ["", "### Outliers", ""]
    lines.append(
        f"Counts: panic {len(out['panic'])}, non-finite {len(out['non_finite'])}, silent "
        f"{len(out['silent'])}, clipping {len(out['clipping'])}, level jump "
        f"{len(out['level_jump'])}."
    )
    for key, title, cols in (
        ("panic", "Panics", ["entry", "mode", "message"]),
        ("non_finite", "Non-finite output", ["entry", "mode", "nan", "inf"]),
        ("silent", "Silent renders", ["entry", "mode", "peak"]),
        ("clipping", "Clipping (top 30)", ["entry", "mode", "peak", "clip_fraction"]),
        ("level_jump", "Level jumps (top 30)", ["entry", "rms_delta_db"]),
    ):
        items = out[key][:30]
        if not items:
            continue
        lines += ["", f"**{title}**", "", "| " + " | ".join(cols) + " |"]
        lines.append("|" + "---|" * len(cols))
        for it in items:
            lines.append("| " + " | ".join(str(it[c]) for c in cols) + " |")
    lines += ["", "## 9. Implications", ""]
    lines += meta.get("implications", ["(see the data file)"])
    lines += ["", "## 10. Follow-ups", ""]
    lines += meta.get("followups", ["-"])
    lines += [
        "",
        "Data: [data/program-sweep.json](data/program-sweep.json). Reproduce: "
        "`cargo build --release -p nord-core --example program_sweep` then "
        '`uv run python tools/program_sweep.py --ns4b ".reference/<backup>.ns4b"`.',
        "",
    ]
    return "\n".join(lines)


def load_jsonl(path: Path) -> list[dict]:
    """Reads the example's output, one JSON object per line."""
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line]


def run_example(ns4b: Path, extra: list[str]) -> list[dict]:
    """Runs the release example on `ns4b` and parses its output."""
    exe = EXAMPLE.with_suffix(".exe") if sys.platform == "win32" else EXAMPLE
    if not exe.exists():
        raise SystemExit(
            f"missing {exe}: cargo build --release -p nord-core --example program_sweep"
        )
    proc = subprocess.run(
        [str(exe), str(ns4b), *extra], capture_output=True, text=True, check=False
    )
    if proc.returncode != 0:
        raise SystemExit(f"program_sweep failed: {proc.stderr.strip()}")
    return [json.loads(line) for line in proc.stdout.splitlines() if line]


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--ns4b", type=Path, help="factory backup to render")
    ap.add_argument("--jsonl", type=Path, help="reuse the example output instead of rendering")
    ap.add_argument("--meta", type=Path, help="JSON with checked/backup/implications/followups")
    ap.add_argument("--no-write", action="store_true", help="print a summary only")
    args = ap.parse_args(argv)
    if args.jsonl:
        records = load_jsonl(args.jsonl)
    elif args.ns4b:
        records = run_example(args.ns4b, [])
    else:
        ap.error("give --ns4b or --jsonl")
    result = analyze(records)
    meta = {
        "checked": "unset",
        "backup": "Nord Stage 4 Factory Restore RevK v1.66.ns4b",
        "tool": "tools/program_sweep.py",
        "frames": 96000,
        "notes": "48, 55, 64, 72",
        "velocity": 100,
        "hold": 72000,
    }
    if args.meta:
        meta.update(json.loads(args.meta.read_text(encoding="utf-8")))
    if args.no_write:
        print(json.dumps({k: result[k] for k in ("programs", "status_totals", "level_delta_db")}))
        return 0
    JSON_PATH.parent.mkdir(parents=True, exist_ok=True)
    JSON_PATH.write_text(
        json.dumps(
            {
                "meta": {
                    k: meta[k] for k in ("checked", "backup", "frames", "notes", "velocity", "hold")
                },
                **result,
            },
            indent=1,
        )
        + "\n",
        encoding="utf-8",
    )
    DOC_PATH.write_text(markdown(result, meta), encoding="utf-8")
    print(f"wrote {DOC_PATH} and {JSON_PATH}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
