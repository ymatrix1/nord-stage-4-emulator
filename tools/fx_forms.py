#!/usr/bin/env python3
"""Closed forms of the context words the ARM writes for the effect ids (fx-contexts.md).

Every function maps the UI value `v` of an edit id (range as in `arm-edit-params.csv`) to the
word the firmware sends. They were read from the captured series in
`docs/research/data/fx-param-fits.csv` and are checked against its samples by
`tools/tests/test_fx_forms.py` (float32 words, relative error below 1e-5 of the series range).

Status of every form: the **word values** are `VERIFIED` (reproduced from the capture, command in
the note). The **meaning** of the context word (which model field it is) follows the field tables
of `docs/research/fx-routine-models.md` and is `EMPIRICAL` where the position matches a field of
the model (for example `c[4]` is the dry gain target of the delay) and `ASSUMED` otherwise.
"""

from __future__ import annotations

from dataclasses import dataclass


def delay_wet(v: float) -> float:
    """Delay send level `c[-0x1B]` (`kDlyMix`, 0..127): `min(1, (v / 64)^2)`."""
    return min(1.0, (v / 64.0) ** 2)


def delay_dry(v: float) -> float:
    """Delay dry gain target `c[4]` (`kDlyMix`): `min(1, (2 - v / 64)^2)`."""
    return min(1.0, (2.0 - v / 64.0) ** 2)


def delay_feedback(v: float) -> float:
    """Delay feedback matrix targets `c[-0x1A]`, `c[-0x19]` (`kDlyFbkAmt`, 0..127): `v / 127`."""
    return v / 127.0


def compressor_threshold(v: float) -> float:
    """Compressor word `c[-0x1B]` (`kCmpAmt`, 0..127): `-36 v / 127` (log2 level units)."""
    return -36.0 * v / 127.0


def compressor_makeup(v: float) -> float:
    """Compressor makeup target `c[-0x1C]` (`kCmpAmt`): `1 + 5.72 (v / 127)^2` (1.0 .. 6.72)."""
    return 1.0 + 5.72 * (v / 127.0) ** 2


def amp_drive(v: float) -> float:
    """Table-stage phase target `c[-0x20]` of the Amp drive module (`kAmpDrive`): `v / 127`."""
    return v / 127.0


def phase_increment(v: float) -> int:
    """All-pass / vibe LFO phase increment `c[-0x20]` (`kFx1RateClkDiv`, Fx1 types 0..3):
    `12800 + 16384 v` counts per call."""
    return int(12800 + 16384 * v)


def crossfade_wet(v: float, full_scale: float = 127.0) -> float:
    """Wet word of an Fx1 / Fx2 amount control: `v / full_scale` (127 for Fx1 types 4, 5 and the
    delay-style units, 254 for Fx1 types 2, 3 where the word peaks at 0.5)."""
    return v / full_scale


def crossfade_dry(v: float, full_scale: float = 127.0) -> float:
    """Dry word that goes with `crossfade_wet`: `1 - v / full_scale`."""
    return 1.0 - v / full_scale


@dataclass(frozen=True)
class Check:
    """Where a closed form was verified: the series key in `fx-param-fits.csv`."""

    slot: str
    type: str
    ident: str
    list: str
    entry: str
    ctx: str


def parse_samples(text: str) -> dict[int, int]:
    """`"0:0x3f800000;64:0x..."` to `{0: 0x3f800000, 64: ...}`."""
    out = {}
    for item in text.split(";"):
        v, _, w = item.partition(":")
        out[int(v)] = int(w, 16)
    return out
