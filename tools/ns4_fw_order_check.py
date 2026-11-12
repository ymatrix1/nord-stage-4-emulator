#!/usr/bin/env python3
"""Check our Synth `0x32`/`0x2F` field order and widths against the OS v1.66 firmware.

The firmware (`bank0_CODE.bin`, loaded at ``0xC0000000``) stores one table of field-name strings per
program buffer: ``Buffer_kSynLay`` (the `0x32` Layer) and ``Buffer_kSynSnd`` (the `0x2F` Sound).
The names appear in bitstream order, so they verify two things independently of ns4decode:

1. **Order**: our fields, sorted by offset, appear in the same order as the firmware names.
2. **Contiguity**: where the firmware packs fields back to back, ``offset + width`` of each field
   equals the offset of the next one (a wrong width would break the chain).

Absolute positions are still anchored by the corpus (`ns4_bitscan.py`) and ns4decode; this check
does not read bit positions out of the firmware.

    python tools/ns4_fw_order_check.py target/fw_unpacked/bank0_CODE.bin
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path
from typing import NamedTuple

import ns4_synth_fields as sf


class Entry(NamedTuple):
    fw_name: str
    offset: int
    width: int
    chained: bool = True  # True: next entry starts at offset + width
    field: str | None = None  # name in ns4_synth_fields.FIELDS when we decode it


# `0x2F` Sound: from Osc Ctrl to Smp Timbre. Morph fields are `7 + 3 * 8 = 31` bits.
SOUND_CHAIN: tuple[Entry, ...] = (
    Entry("Edt_Syn_kOscCtrl", 25, 31),
    Entry("Edt_Syn_kOscTuneFine", 56, 7, field="pitch_fine"),
    Entry("Edt_Syn_kOscTuneCoarse", 63, 6, field="pitch_coarse"),
    Entry("Edt_Syn_kOscUnison_obsolete", 69, 2),
    Entry("Edt_Syn_kOscEnvAtk", 71, 7, field="osc_env_attack"),
    Entry("Edt_Syn_kOscEnvDcy", 78, 7, field="osc_env_decay"),
    Entry("Edt_Syn_kOscEnvRls", 85, 7, field="osc_env_release"),
    Entry("Edt_Syn_kOscEnvAmt", 92, 31, field="osc_env_amt"),
    Entry("Edt_Syn_kOscEnvDst", 123, 1, field="osc_env_to_pitch"),
    Entry("Edt_Syn_kOscEnvVel", 124, 1, field="osc_env_velocity"),
    Entry("Edt_Syn_kSmpDynamic", 125, 2),
    Entry("Edt_Syn_kLFODst", 127, 2, field="lfo_target"),
    Entry("Edt_Syn_kLFOWave", 129, 3, field="lfo_shape"),
    Entry("Edt_Syn_kLFOClkEnable", 132, 1, field="lfo_mst_clk"),
    Entry("Edt_Syn_kLFORateClkDiv", 133, 31),
    Entry("Edt_Syn_kLFOAmt", 164, 31),
    Entry("Edt_Syn_kAmpEnvAtk", 195, 7, field="amp_env_attack"),
    Entry("Edt_Syn_kAmpEnvDcy", 202, 7, field="amp_env_decay"),
    Entry("Edt_Syn_kAmpEnvRls", 209, 7, field="amp_env_release"),
    Entry("Edt_Syn_kAmpEnvVel", 216, 2, field="amp_env_velocity"),
    Entry("Edt_Syn_kFltType", 218, 3, field="filter_type"),
    Entry("Edt_Syn_kFltFreq", 221, 31),
    Entry("Edt_Syn_kFltRes", 252, 31),
    Entry("Edt_Syn_kFltKbdTrk", 283, 2),
    Entry("Edt_Syn_kFltDrive", 285, 2),
    Entry("Edt_Syn_kFltEnvAmt", 287, 31),
    Entry("Edt_Syn_kFltEnvAtk", 318, 7, field="filter_env_attack"),
    Entry("Edt_Syn_kFltEnvDcy", 325, 7, field="filter_env_decay"),
    Entry("Edt_Syn_kFltEnvRls", 332, 7, field="filter_env_release"),
    Entry("Edt_Syn_kFltEnvVel", 339, 1, field="filter_velocity"),
    Entry("Edt_Syn_kFltEnable", 340, 1, field="filter_on"),
    Entry("Edt_Syn_kVibRate", 341, 7, field="vibrato_rate"),
    Entry("Edt_Syn_kVibAmt", 348, 6, field="vibrato_amount"),
    Entry("Edt_Syn_kSmpTimbre", 354, 2, chained=False),
)

# `0x32` Layer: the packed run from Kb Zone to Glide Rate, then spaced fields (extern, arpeggiator).
LAYER_CHAIN: tuple[Entry, ...] = (
    Entry("Edt_Syn_kSmpId", 26, 32),
    Entry("Edt_Syn_kKbZone", 58, 4),
    Entry("Edt_Syn_kOctaveShift", 62, 4, field="octave"),
    Entry("Edt_Syn_kPStickEnable", 66, 1),
    Entry("Edt_Syn_kPitchStickRange", 67, 4),
    Entry("Edt_Syn_kSustainEnable", 71, 1),
    Entry("Edt_Syn_kVibMode", 72, 3, field="vibrato_mode"),
    Entry("Edt_Syn_kVibDly_obsolete", 75, 2),
    # VoiceMode is one 2-bit enum; ns4decode exposes it as legato (+77) and mono (+78).
    Entry("Edt_Syn_kVoiceMode", 77, 2, field="legato"),
    Entry("Edt_Syn_kVoicePrio", 79, 2, field="voice_priority"),
    Entry("Edt_Syn_kGlideRate", 81, 7, chained=False, field="glide"),
    Entry("Edt_Syn_kKbdHoldEnableLayer", 134, 1, chained=False),
    Entry("Edt_Syn_kArpRun", 135, 1, chained=False),
    Entry("Edt_Syn_kArpKbdSync", 139, 1, chained=False),
    Entry("Edt_Syn_kOscUnison", 306, 2, chained=False, field="unison"),
    Entry("Edt_Syn_kVibDly", 372, 5, field="vibrato_delay"),
)

BUFFERS = {
    "Buffer_kSynLay": ("layer", LAYER_CHAIN),
    "Buffer_kSynSnd": ("sound", SOUND_CHAIN),
}


def firmware_fields(fw: bytes, buffer_name: str) -> list[str]:
    """`Edt_*` field names listed after `buffer_name` up to the next `Buffer_k*` string."""
    marker = buffer_name.encode() + b"\0"
    at = fw.find(marker)
    if at < 0:
        raise ValueError(f"{buffer_name} not found in firmware")
    names: list[str] = []
    pos = at + len(marker)
    while pos < len(fw):
        end = fw.find(b"\0", pos)
        if end < 0:
            break
        chunk = fw[pos:end]
        pos = end + 1
        if not chunk:
            continue
        if not all(32 <= c < 127 for c in chunk):
            break
        text = chunk.decode("ascii")
        if text.startswith("Buffer_k"):
            break
        names.append(text)
    return names


def check_chain(fw_names: list[str], chain: tuple[Entry, ...]) -> list[str]:
    """Returns human-readable problems (empty list = chain is consistent with the firmware)."""
    problems: list[str] = []
    positions: list[int] = []
    for e in chain:
        if e.fw_name not in fw_names:
            problems.append(f"{e.fw_name} missing from firmware table")
            continue
        positions.append(fw_names.index(e.fw_name))
    if positions != sorted(positions):
        problems.append("firmware field order differs from our offset order")
    offsets = [e.offset for e in chain]
    if offsets != sorted(offsets) or len(set(offsets)) != len(offsets):
        problems.append("our offsets are not strictly increasing")
    for cur, nxt in zip(chain, chain[1:]):
        end = cur.offset + cur.width
        if cur.chained and end != nxt.offset:
            problems.append(
                f"{cur.fw_name}: ends at +{end} but {nxt.fw_name} starts at +{nxt.offset}"
            )
        if not cur.chained and end > nxt.offset:
            problems.append(f"{cur.fw_name} overlaps {nxt.fw_name}")
    for e in chain:
        if e.field is None or e.fw_name == "Edt_Syn_kVoiceMode":
            continue
        f = sf.BY_NAME[e.field]
        # Morph fields are 31 bits in the chain; the field table keeps only the 7-bit base value.
        if f.offset != e.offset or f.width > e.width:
            problems.append(
                f"{e.field}: ns4_synth_fields has +{f.offset}/w{f.width}, "
                f"chain has +{e.offset}/w{e.width}"
            )
    return problems


def check_firmware(fw: bytes) -> dict[str, list[str]]:
    return {
        name: check_chain(firmware_fields(fw, name), chain) for name, (_, chain) in BUFFERS.items()
    }


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    p.add_argument("firmware", type=Path, help="bank0_CODE.bin from `ns4 inspect --unpack`")
    args = p.parse_args(argv)
    results = check_firmware(args.firmware.read_bytes())
    bad = 0
    for name, problems in results.items():
        chain = BUFFERS[name][1]
        print(f"{name}: {len(chain)} fields checked, {len(problems)} problems")
        for line in problems:
            print("  " + line)
        bad += len(problems)
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
