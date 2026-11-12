"""Declarative bit map of the Synth `0x32` Layer and `0x2F` Sound blocks.

Single source of truth for Python tooling (`ns4_inspect.py`, `ns4_crosscheck.py`); the Rust
decoder in `crates/nord-core/src/cbin/synth_preset.rs` implements the same offsets.

Offsets are block-relative bit positions (MSB first). They come from the third-party ns4decode
program map (MIT, `.reference/ns4decode_pkg/ns4maps.py`), converting its byte-bit ids with
``bit = (byte - 1) * 8 + (bit - 1) - 3000 - block_base``. Status: EMPIRICAL, not firmware-verified;
cross-checked against its regression program and the factory presets, see
``docs/research/synth-0x2f-ns4decode-crosscheck.md``.
"""

from __future__ import annotations

from typing import NamedTuple

LAYER_BASES = (248, 656, 1064)  # 0x32, stride 408 bits
SOUND_BASES = (1472, 1856, 2240)  # 0x2F, stride 384 bits
SYNTH_PAYLOAD_LEN = 493
# The synth block starts at byte 375 of an `.ns4p` file: 48 B header + 45 B global + 135 B organ
# + 147 B piano.
NS4P_SYNTH_OFFSET = 48 + 45 + 135 + 147


class Field(NamedTuple):
    name: str
    block: str  # "layer" | "sound"
    offset: int
    width: int
    signed: bool = False


FIELDS: tuple[Field, ...] = (
    # 0x32 Layer
    Field("is_osc_mode", "layer", 8, 1),
    Field("octave", "layer", 62, 4, True),
    Field("vibrato_mode", "layer", 72, 3),
    Field("legato", "layer", 77, 1),
    Field("mono", "layer", 78, 1),
    Field("voice_priority", "layer", 79, 2),
    Field("glide", "layer", 81, 7),
    Field("unison", "layer", 306, 2),
    Field("vibrato_delay", "layer", 372, 5),
    # 0x2F Sound
    Field("osc_type", "sound", 10, 2),
    Field("osc_category", "sound", 15, 3),
    Field("osc_wave", "sound", 19, 6),
    Field("pitch_fine", "sound", 56, 7, True),
    Field("pitch_coarse", "sound", 63, 6, True),
    Field("osc_env_attack", "sound", 71, 7),
    Field("osc_env_decay", "sound", 78, 7),
    Field("osc_env_release", "sound", 85, 7),
    Field("osc_env_amt", "sound", 92, 7, True),
    Field("osc_env_to_pitch", "sound", 123, 1),
    Field("osc_env_velocity", "sound", 124, 1),
    Field("lfo_target", "sound", 127, 2),
    Field("lfo_shape", "sound", 129, 3),
    Field("lfo_mst_clk", "sound", 132, 1),
    Field("amp_env_attack", "sound", 195, 7),
    Field("amp_env_decay", "sound", 202, 7),
    Field("amp_env_release", "sound", 209, 7),
    Field("amp_env_velocity", "sound", 216, 2),
    Field("filter_type", "sound", 218, 3),
    Field("filter_env_attack", "sound", 318, 7),
    Field("filter_env_decay", "sound", 325, 7),
    Field("filter_env_release", "sound", 332, 7),
    Field("filter_velocity", "sound", 339, 1),
    Field("filter_on", "sound", 340, 1),
    Field("vibrato_rate", "sound", 341, 7),
    Field("vibrato_amount", "sound", 348, 6),
)

BY_NAME = {f.name: f for f in FIELDS}


def to_bits(data: bytes) -> str:
    return "".join(f"{b:08b}" for b in data)


def read_field(bits: str, base: int, field: Field) -> int:
    """Reads `field` from a payload bit string for the layer block starting at `base`."""
    start = base + field.offset
    raw = int(bits[start : start + field.width], 2)
    if field.signed and raw & (1 << (field.width - 1)):
        raw -= 1 << field.width
    return raw


def read_layer_fields(payload: bytes, layer: int) -> dict[str, int]:
    """All `FIELDS` of synth layer `layer` (0 = A, 1 = B, 2 = C) from a 493-byte synth payload."""
    if len(payload) < SYNTH_PAYLOAD_LEN:
        raise ValueError(f"synth payload too short: {len(payload)} < {SYNTH_PAYLOAD_LEN}")
    bits = to_bits(payload[:SYNTH_PAYLOAD_LEN])
    out: dict[str, int] = {}
    for f in FIELDS:
        base = (LAYER_BASES if f.block == "layer" else SOUND_BASES)[layer]
        out[f.name] = read_field(bits, base, f)
    return out


def synth_payload_from_ns4p(raw: bytes) -> bytes:
    """Slices the 493-byte synth block out of a complete `.ns4p` file."""
    end = NS4P_SYNTH_OFFSET + SYNTH_PAYLOAD_LEN
    if len(raw) < end:
        raise ValueError(f".ns4p too short: {len(raw)} < {end}")
    return raw[NS4P_SYNTH_OFFSET:end]
