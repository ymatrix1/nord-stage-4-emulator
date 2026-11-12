#!/usr/bin/env python3
"""Nord Stage 4 CBIN preset, .ns4b backup archive, and OS v1.66 firmware inspector.

Spec: docs/specs/cbin-and-presets.md
"""

from __future__ import annotations

import argparse
import json
import struct
import sys
import xml.etree.ElementTree as ET
import zipfile
from dataclasses import asdict, dataclass
from pathlib import Path

CBIN_MAGIC = b"CBIN"
CBIN_HEADER_SIZE = 48

EXPECTED_SIZES: dict[str, int] = {
    "ns4p": 868,
    "ns4l": 868,
    "ns4y": 541,
    "ns4n": 195,
    "ns4o": 183,
    "ns4t": 124,
}

EXPECTED_SCHEMAS: dict[str, int] = {
    "ns4p": 0x0139,
    "ns4l": 0x0139,
    "ns4y": 0x00D0,
    "ns4n": 0x00CB,
    "ns4o": 0x00CD,
    "ns4t": 0x006C,
}


@dataclass(frozen=True)
class CbinSummary:
    tag: str
    format_version: int
    bank_index: int
    location_index: int
    category_id: int
    schema_id: int
    checksum: int
    total_size: int
    payload_size: int


@dataclass(frozen=True)
class BackupSummary:
    product_id: int
    product_version: int
    product_build: int
    manager_version: int
    extension_counts: dict[str, int]


class BitReader:
    """Reads sequential bits from big-endian packed byte stream."""

    def __init__(self, data: bytes):
        self.data = data
        self.bit_pos = 0

    def set_bit_pos(self, bit_pos: int) -> None:
        self.bit_pos = bit_pos

    def read_bits(self, count: int) -> int:
        val = 0
        for _ in range(count):
            byte_idx = self.bit_pos // 8
            bit_idx = 7 - (self.bit_pos % 8)
            bit = (self.data[byte_idx] >> bit_idx) & 1
            val = (val << 1) | bit
            self.bit_pos += 1
        return val

    def read_bool(self) -> bool:
        return self.read_bits(1) != 0

    def read_signed(self, count: int) -> int:
        raw = self.read_bits(count)
        sign_bit = 1 << (count - 1)
        return raw - (1 << count) if (raw & sign_bit) else raw

    def read_morph(self, bit_pos: int, width: int, max_val: int) -> tuple[int, dict[str, float]]:
        self.bit_pos = bit_pos
        base = self.read_bits(width)
        deltas = {
            "wheel": round((self.read_bits(width + 1) - max_val) / max_val, 3),
            "aftertouch": round((self.read_bits(width + 1) - max_val) / max_val, 3),
            "ctrl_pedal": round((self.read_bits(width + 1) - max_val) / max_val, 3),
        }
        return base, deltas


SPLIT_NOTE_TABLE = [41, 48, 53, 60, 65, 72, 77, 84, 89, 96]
ORGAN_MODELS = ["B3", "Vox", "Farfisa", "Pipe1", "Pipe2"]
VIBRATO_TYPES = ["V1", "C1", "V2", "C2", "V3", "C3"]
PIANO_CATEGORIES = ["Grand", "Upright", "Electric", "Clav", "Digital", "Misc"]
PIANO_TOUCHES = ["Light", "Medium", "Heavy"]
PIANO_TIMBRES = ["Off", "Soft", "Mid", "Bright", "Dyno1", "Dyno2"]
MOD1_TYPES = [
    "AutoPanSoft",
    "AutoPanHard",
    "TremoloSoft",
    "TremoloHard",
    "RingMod",
    "RingModStereo",
    "AWahCry",
    "AWahEnv",
    "WahWahLfo",
    "WahWahPedal",
    "PumpLfo",
    "PumpPedal",
]
MOD2_TYPES = [
    "Phaser1",
    "Phaser2",
    "FlangerStereo",
    "FlangerMono",
    "ChorusAnalog",
    "VibeStereo",
    "VibeMono",
    "EnsembleStereo",
    "EnsembleMono",
    "SpinTreble",
    "SpinBass",
    "SpinBoth",
]
REVERB_TYPES = ["Spring", "Booth", "Room", "Stage", "Hall", "Cathedral"]


def decode_fx_payload(payload: bytes) -> dict:
    """Decodes hardware `0x36` 55-byte per-layer FX chain."""
    if len(payload) < 55:
        raise ValueError(f"fx payload too short: {len(payload)} < 55")
    r = BitReader(payload[:55])
    r.set_bit_pos(8)
    mod1_on = r.read_bool()
    mod1_sync = r.read_bool()
    mod1_rate, mod1_rate_m = r.read_morph(10, 7, 127)
    mod1_amt, mod1_amt_m = r.read_morph(41, 7, 127)
    r.set_bit_pos(72)
    mod1_type_idx = r.read_bits(4)

    mod2_on = r.read_bool()
    mod2_rate, mod2_rate_m = r.read_morph(77, 7, 127)
    mod2_amt, mod2_amt_m = r.read_morph(108, 7, 127)
    r.set_bit_pos(139)
    mod2_type_idx = r.read_bits(4)

    amp_on = r.read_bool()
    eq_treble = round(r.read_signed(7) * 0.25, 2)
    eq_mid = round(r.read_signed(7) * 0.25, 2)
    eq_bass = round(r.read_signed(7) * 0.25, 2)
    eq_mid_freq, _ = r.read_morph(165, 7, 127)
    amp_drive, _ = r.read_morph(196, 7, 127)

    r.set_bit_pos(231)
    comp_on = r.read_bool()
    comp_amt = round(r.read_bits(7) / 127.0, 3)
    comp_fast = r.read_bool()

    dly_on = r.read_bool()
    dly_sync = r.read_bool()
    dly_rate, _ = r.read_morph(242, 14, 16383)
    dly_mix, _ = r.read_morph(301, 7, 127)
    r.set_bit_pos(332)
    dly_mode = r.read_bits(2)
    dly_pp = r.read_bool()
    dly_flt = r.read_bool()
    dly_fb, _ = r.read_morph(336, 7, 127)
    r.set_bit_pos(367)
    dly_fb_fx = r.read_bits(4)

    rvb_on = r.read_bool()
    rvb_amt, rvb_amt_m = r.read_morph(372, 7, 127)
    r.set_bit_pos(403)
    rvb_color = r.read_bits(2)
    rvb_type_idx = r.read_bits(3)
    amp_type_new = r.read_bits(9)

    return {
        "mod1": {
            "enabled": mod1_on,
            "clock_sync": mod1_sync,
            "type": MOD1_TYPES[mod1_type_idx]
            if mod1_type_idx < len(MOD1_TYPES)
            else f"Unknown({mod1_type_idx})",
            "rate": round(mod1_rate / 127.0, 3),
            "rate_morph": mod1_rate_m,
            "amount": round(mod1_amt / 127.0, 3),
            "amount_morph": mod1_amt_m,
        },
        "mod2": {
            "enabled": mod2_on,
            "type": MOD2_TYPES[mod2_type_idx]
            if mod2_type_idx < len(MOD2_TYPES)
            else f"Unknown({mod2_type_idx})",
            "rate": round(mod2_rate / 127.0, 3),
            "rate_morph": mod2_rate_m,
            "amount": round(mod2_amt / 127.0, 3),
            "amount_morph": mod2_amt_m,
        },
        "amp_eq": {
            "enabled": amp_on,
            "amp_type_new": amp_type_new,
            "treble_db": eq_treble,
            "mid_db": eq_mid,
            "bass_db": eq_bass,
            "mid_freq": round(eq_mid_freq / 127.0, 3),
            "drive": round((amp_drive / 127.0) * 10.0, 2),
        },
        "comp": {"enabled": comp_on, "amount": comp_amt, "fast": comp_fast},
        "delay": {
            "enabled": dly_on,
            "clock_sync": dly_sync,
            "rate": round(dly_rate / 16383.0, 4),
            "mix": round(dly_mix / 127.0, 3),
            "feedback": round(dly_fb / 127.0, 3),
            "mode": dly_mode,
            "ping_pong": dly_pp,
            "filter": dly_flt,
            "fb_fx_type": dly_fb_fx,
        },
        "reverb": {
            "enabled": rvb_on,
            "type": REVERB_TYPES[rvb_type_idx]
            if rvb_type_idx < len(REVERB_TYPES)
            else f"Unknown({rvb_type_idx})",
            "color": rvb_color,
            "amount": round(rvb_amt / 127.0, 3),
            "amount_morph": rvb_amt_m,
        },
    }


def decode_global_payload(payload: bytes) -> dict:
    """Decodes hardware `0x2C` 45-byte Global program block."""
    if len(payload) < 45:
        raise ValueError(f"global payload too short: {len(payload)} < 45")
    r = BitReader(payload[:45])
    r.set_bit_pos(8)
    split_enabled = r.read_bool()
    zone_en = [r.read_bool() for _ in range(3)]
    split_pts = [SPLIT_NOTE_TABLE[min(r.read_bits(4), len(SPLIT_NOTE_TABLE) - 1)] for _ in range(3)]
    split_xfade = [r.read_bits(2) for _ in range(3)]
    r.set_bit_pos(35)
    tempo_bpm = 30 + r.read_bits(8)
    r.set_bit_pos(284)
    sec_en_i = r.read_bits(3)
    sec_en_ii = r.read_bits(3)
    return {
        "tempo_bpm": tempo_bpm,
        "split_enabled": split_enabled,
        "split_zone_enabled": zone_en,
        "split_points": split_pts,
        "split_crossfades": split_xfade,
        "section_enable_i": sec_en_i,
        "section_enable_ii": sec_en_ii,
    }


def _decode_organ_layer(r: BitReader, base: int, enabled: bool, level: float) -> dict:
    r.set_bit_pos(base + 12)
    octave = max(-2, min(2, r.read_signed(4)))
    r.set_bit_pos(base + 17)
    model_idx = r.read_bits(3)
    drawbars = []
    drawbar_morphs = []
    for i in range(9):
        db, db_m = r.read_morph(base + 40 + i * 19, 4, 8)
        drawbars.append(min(db, 8))
        drawbar_morphs.append(db_m)
    r.set_bit_pos(base + 232)
    vib_on = r.read_bool()
    perc_on = r.read_bool()
    perc_third = r.read_bool()
    perc_fast = r.read_bool()
    perc_soft = r.read_bool()
    return {
        "enabled": enabled,
        "level": round(level, 3),
        "octave": octave,
        "model": ORGAN_MODELS[model_idx]
        if model_idx < len(ORGAN_MODELS)
        else f"Unknown({model_idx})",
        "drawbars": drawbars,
        "drawbar_morphs": drawbar_morphs,
        "vibrato_on": vib_on,
        "percussion": {
            "enabled": perc_on,
            "third": perc_third,
            "fast": perc_fast,
            "soft": perc_soft,
        },
    }


def decode_organ_payload(payload: bytes) -> dict:
    """Decodes hardware `0x11` 135-byte Organ block (with `0x03` layers + `0x36` FX)."""
    if len(payload) < 135:
        raise ValueError(f"organ payload too short: {len(payload)} < 135")
    r = BitReader(payload[:135])
    r.set_bit_pos(9)
    enable_i = r.read_bits(2)
    r.set_bit_pos(12)
    enable_ii = r.read_bits(2)
    lvl_a, lvl_a_m = r.read_morph(14, 7, 127)
    lvl_b, lvl_b_m = r.read_morph(45, 7, 127)

    r.set_bit_pos(80)
    rot_enabled = r.read_bool()
    leslie_bits = r.read_bits(2)
    rot_speed = {0: "Stop", 1: "Slow", 2: "Stop", 3: "Fast"}[leslie_bits]
    r.set_bit_pos(92)
    rot_drive = round((r.read_bits(7) / 127.0) * 10.0, 2)
    r.set_bit_pos(130)
    vib_idx = min(r.read_bits(3), 5)

    la = _decode_organ_layer(r, 144, (enable_i & 0x01) != 0, lvl_a / 127.0)
    lb = _decode_organ_layer(r, 392, (enable_i & 0x02) != 0, lvl_b / 127.0)
    la["level_morph"] = lvl_a_m
    lb["level_morph"] = lvl_b_m

    return {
        "model": la["model"],
        "enable_ii_mask": enable_ii,
        "percussion": la["percussion"],
        "vibrato": {"enabled": la["vibrato_on"], "type": VIBRATO_TYPES[vib_idx]},
        "rotary": {"enabled": rot_enabled, "speed": rot_speed, "drive": rot_drive},
        "layer_a": la,
        "layer_b": lb,
        "fx": decode_fx_payload(payload[80:135]),
    }


def _decode_piano_layer(r: BitReader, base: int, enabled: bool, level: float) -> dict:
    r.set_bit_pos(base + 12)
    octave = max(-2, min(2, r.read_signed(4)))
    r.set_bit_pos(base + 18)
    cat_idx = r.read_bits(3)
    model_id = r.read_bits(5)
    r.set_bit_pos(base + 28)
    variation = r.read_bits(2)
    name_hash = r.read_bits(32)
    acoustic = r.read_bits(2)
    pedal_noise = r.read_bool()
    touch_idx = r.read_bits(2)
    unison = r.read_bits(2)
    dyn_comp = r.read_bits(2)
    timbre_idx = r.read_bits(3)
    return {
        "enabled": enabled,
        "level": round(level, 3),
        "octave": octave,
        "category": PIANO_CATEGORIES[cat_idx]
        if cat_idx < len(PIANO_CATEGORIES)
        else f"Unknown({cat_idx})",
        "model_id": model_id,
        "variation": variation,
        "name_hash": f"0x{name_hash:08x}",
        "acoustic_level": acoustic,
        "pedal_noise": pedal_noise,
        "touch": PIANO_TOUCHES[min(touch_idx, 2)],
        "unison": unison,
        "dyn_comp": dyn_comp,
        "timbre": PIANO_TIMBRES[min(timbre_idx, 5)],
    }


def decode_piano_payload(payload: bytes) -> dict:
    """Decodes hardware `0x0C` 147-byte Piano block (with `0x0B` layers + `0x36` FX)."""
    if len(payload) < 147:
        raise ValueError(f"piano payload too short: {len(payload)} < 147")
    r = BitReader(payload[:147])
    r.set_bit_pos(9)
    enable_i = r.read_bits(2)
    r.set_bit_pos(12)
    enable_ii = r.read_bits(2)
    lvl_a, lvl_a_m = r.read_morph(14, 7, 127)
    lvl_b, lvl_b_m = r.read_morph(45, 7, 127)
    la = _decode_piano_layer(r, 104, (enable_i & 0x01) != 0, lvl_a / 127.0)
    lb = _decode_piano_layer(r, 200, (enable_i & 0x02) != 0, lvl_b / 127.0)
    la["level_morph"] = lvl_a_m
    lb["level_morph"] = lvl_b_m
    return {
        "enable_ii_mask": enable_ii,
        "layer_a": la,
        "layer_b": lb,
        "fx_a": decode_fx_payload(payload[37:92]),
        "fx_b": decode_fx_payload(payload[92:147]),
    }


def _decode_synth_layer(
    r: BitReader, layer_base: int, sound_base: int, enabled: bool, level: float, pan: float
) -> dict:
    r.set_bit_pos(layer_base + 8)
    is_osc = r.read_bool()
    r.set_bit_pos(layer_base + 32)
    sample_id = r.read_bits(32)
    r.set_bit_pos(layer_base + 135)
    arp_run = r.read_bool()
    arp_rate, _ = r.read_morph(layer_base + 175, 7, 127)

    r.set_bit_pos(sound_base + 8)
    osc_bank = r.read_bits(3)
    osc_cat = r.read_bits(4)
    osc_wave = r.read_bits(10)
    osc_ctrl, _ = r.read_morph(sound_base + 25, 7, 127)
    lfo_rate, _ = r.read_morph(sound_base + 133, 7, 127)
    lfo_amt, _ = r.read_morph(sound_base + 164, 7, 127)

    r.set_bit_pos(sound_base + 218)
    flt_type = r.read_bits(3)
    flt_freq, flt_freq_m = r.read_morph(sound_base + 221, 7, 127)
    flt_res, _ = r.read_morph(sound_base + 252, 7, 127)
    cutoff_hz = round(80.0 * (200.0 ** (flt_freq / 127.0)), 1)
    r.set_bit_pos(sound_base + 318)
    fenv_adr = [r.read_bits(7) for _ in range(3)]

    return {
        "enabled": enabled,
        "level": round(level, 3),
        "pan": round(pan, 3),
        "is_osc_mode": is_osc,
        "sample_id": f"0x{sample_id:08x}",
        "osc_bank": osc_bank,
        "osc_cat": osc_cat,
        "osc_wave": osc_wave,
        "osc_ctrl": round(osc_ctrl / 127.0, 3),
        "lfo_rate": round(0.1 + (lfo_rate / 127.0) * 19.9, 2),
        "lfo_amount": round(lfo_amt / 127.0, 3),
        "filter_type": flt_type,
        "filter_cutoff_hz": cutoff_hz,
        "filter_cutoff_morph": flt_freq_m,
        "filter_resonance": round((flt_res / 127.0) * 0.95, 3),
        "filter_env_adr_idx": fenv_adr,
        "arp_enabled": arp_run,
        "arp_bpm": round(30.0 + (arp_rate / 127.0) * 270.0, 1),
    }


def decode_synth_payload(payload: bytes) -> dict:
    """Decodes hardware `0x1E` 493-byte Synth block (`0x32` + `0x2F` + `0x36`)."""
    if len(payload) < 493:
        raise ValueError(f"synth payload too short: {len(payload)} < 493")
    r = BitReader(payload[:493])
    r.set_bit_pos(10)
    enable_i = r.read_bits(3)
    r.set_bit_pos(15)
    enable_ii = r.read_bits(3)
    lvl_a, _ = r.read_morph(18, 7, 127)
    lvl_b, _ = r.read_morph(49, 7, 127)
    lvl_c, _ = r.read_morph(80, 7, 127)
    r.set_bit_pos(111)
    pan_a = r.read_bits(7) / 127.0
    r.set_bit_pos(142)
    pan_b = r.read_bits(7) / 127.0
    r.set_bit_pos(173)
    pan_c = r.read_bits(7) / 127.0

    return {
        "enable_ii_mask": enable_ii,
        "layer_a": _decode_synth_layer(r, 248, 1472, (enable_i & 0x01) != 0, lvl_a / 127.0, pan_a),
        "layer_b": _decode_synth_layer(r, 656, 1856, (enable_i & 0x02) != 0, lvl_b / 127.0, pan_b),
        "layer_c": _decode_synth_layer(r, 1064, 2240, (enable_i & 0x04) != 0, lvl_c / 127.0, pan_c),
        "fx_a": decode_fx_payload(payload[328:383]),
        "fx_b": decode_fx_payload(payload[383:438]),
        "fx_c": decode_fx_payload(payload[438:493]),
    }


def decode_cbin_payload(tag: str, payload: bytes) -> dict:
    if tag == "ns4o":
        return {"organ": decode_organ_payload(payload)}
    if tag == "ns4n" and len(payload) >= 147:
        return {"piano": decode_piano_payload(payload), "piano_bytes": len(payload)}
    if tag == "ns4y" and len(payload) >= 493:
        return {"synth": decode_synth_payload(payload), "synth_bytes": len(payload)}
    if tag == "ns4t" and len(payload) >= 55:
        return {"fx": decode_fx_payload(payload), "fx_bytes": len(payload)}
    if tag in ("ns4p", "ns4l") and len(payload) >= 820:
        return {
            "global": decode_global_payload(payload[0:45]),
            "organ": decode_organ_payload(payload[45:180]),
            "piano": decode_piano_payload(payload[180:327]),
            "synth": decode_synth_payload(payload[327:820]),
            "piano_bytes": len(payload[180:327]),
            "synth_bytes": len(payload[327:820]),
        }
    return {"raw_payload_len": len(payload)}


def parse_cbin_bytes(data: bytes) -> CbinSummary:
    """Parses and validates a 48-byte CBIN v1 preset header."""
    if len(data) < CBIN_HEADER_SIZE:
        raise ValueError(f"CBIN buffer too short: {len(data)} < {CBIN_HEADER_SIZE}")
    magic = data[:4]
    if magic != CBIN_MAGIC:
        raise ValueError(f"invalid CBIN magic: {magic!r}")
    (fmt_ver,) = struct.unpack("<I", data[4:8])
    if fmt_ver != 1:
        raise ValueError(f"unsupported CBIN version: {fmt_ver}")
    tag = data[8:12].decode("ascii", errors="replace")
    bank, loc, cat, schema_le, crc = struct.unpack("<HHIII", data[12:28])
    (schema_be,) = struct.unpack(">H", data[46:48])

    if tag in EXPECTED_SIZES and len(data) != EXPECTED_SIZES[tag]:
        raise ValueError(f"{tag} size mismatch: expected {EXPECTED_SIZES[tag]}, got {len(data)}")
    if tag in EXPECTED_SCHEMAS and schema_le != schema_be:
        raise ValueError(f"schema LE/BE mismatch: 0x{schema_le:04X} != 0x{schema_be:04X}")

    return CbinSummary(
        tag=tag,
        format_version=fmt_ver,
        bank_index=bank,
        location_index=loc,
        category_id=cat,
        schema_id=schema_be,
        checksum=crc,
        total_size=len(data),
        payload_size=len(data) - CBIN_HEADER_SIZE,
    )


def parse_meta_xml(xml_text: str) -> dict[str, int]:
    """Parses the root <backup .../> element from a .ns4b archive's meta.xml."""
    root = ET.fromstring(xml_text)
    if root.tag != "backup":
        raise ValueError(f"unexpected root tag: {root.tag}")
    return {k: int(v) for k, v in root.attrib.items()}


def calculate_checksum(data: bytes) -> int:
    """Calculates Clavia's 32-bit one's-complement byte checksum: 0xFFFFFFFF - sum(bytes)."""
    return (0xFFFFFFFF - sum(data)) & 0xFFFFFFFF


KNOWN_DSP_TAGS: list[tuple[bytes, str]] = [
    (b"DSPFxCore1", "SHARC Effects Pipeline & String Resonance DSP (DSPFxCore1)"),
    (b"DSPOrg", "SHARC Organ Physical Modeling & Rotary 122 DSP (DSPOrg)"),
    (b"DSPSyn", "SHARC 3-Layer Nord Wave 2 Synth DSP (DSPSyn)"),
    (b"Fx to Org", "SPORT Bus Routing: Effects to Organ"),
    (b"to Fx", "SPORT Bus Routing: Inter-core to Effects"),
    (b"Host", "AM1705 Host Processor Routing"),
    (b"ORG", "B3 Organ Physical Model Domain Tag"),
    (b"PNO", "Grand / Upright Piano Acoustic Model Domain Tag"),
    (b"SMP", "Sample Library Synth Domain Tag"),
    (b"SY1", "Wave 2 Synth Layer 1 Domain Tag"),
    (b"SYL3", "Wave 2 Synth Layer 3 Domain Tag"),
]


def extract_firmware_payload(raw: bytes) -> bytes:
    """Extracts Resource 101 payload from Windows PE executable, or returns raw bytes."""
    if raw.startswith(b"MZ") and len(raw) > 0x40:
        (e_lfanew,) = struct.unpack_from("<I", raw, 0x3C)
        if e_lfanew + 24 <= len(raw) and raw[e_lfanew : e_lfanew + 4] == b"PE\x00\x00":
            # Search for the 32-byte firmware container header signature:
            # 00 00 00 20 followed by image0_size and image1_offset
            for offset in range(0x1000, len(raw) - 0x20, 4):
                if raw[offset : offset + 4] == b"\x00\x00\x00\x20":
                    img0_size, img1_off = struct.unpack_from(">II", raw, offset + 4)
                    if 0x20 + img0_size == img1_off and img1_off < len(raw):
                        return raw[offset:]
    return raw


def parse_firmware_container(raw: bytes) -> dict:
    """Parses dual-bank Clavia OS firmware container (Resource 101)."""
    data = extract_firmware_payload(raw)
    if len(data) < 32:
        raise ValueError(f"firmware buffer too short: {len(data)} < 32")

    header_size, img0_size, img1_off, img1_size = struct.unpack_from(">IIII", data, 0)
    if header_size != 0x20:
        raise ValueError(f"invalid firmware header size: 0x{header_size:X}")
    if 0x20 + img0_size != img1_off:
        raise ValueError(f"image1 offset mismatch: 0x{img1_off:X} != 0x20 + 0x{img0_size:X}")

    def parse_bank_segments(base_off: int) -> list[dict]:
        if len(data) < base_off + 20:
            return []
        (seg_count,) = struct.unpack_from(">I", data, base_off + 0x10)
        cursor = base_off + 0x14
        segs = []
        for _ in range(min(seg_count, 8)):
            if cursor + 44 > len(data):
                break
            entry = data[cursor : cursor + 44]
            name = entry[:4].decode("latin1", errors="replace").rstrip("\x00")
            off, size, target, csum = struct.unpack_from(">IIII", entry, 4)
            seg_info: dict = {
                "name": name,
                "data_offset": off,
                "data_size": size,
                "target_address": f"0x{target:08X}",
                "checksum": f"0x{csum:08X}",
            }
            if name != "CODE" and base_off + off + size <= len(data):
                seg_bytes = data[base_off + off : base_off + off + size]
                calc_csum = calculate_checksum(seg_bytes)
                seg_info["checksum_valid"] = calc_csum == csum
            segs.append(seg_info)
            cursor += 44
        return segs

    partitions = []
    for tag, desc in KNOWN_DSP_TAGS:
        pos = data.find(tag)
        if pos != -1:
            partitions.append({"tag": tag.decode("latin1"), "offset": pos, "description": desc})

    return {
        "header_size": header_size,
        "image0_size": img0_size,
        "image1_offset": img1_off,
        "image1_size": img1_size,
        "image0_segments": parse_bank_segments(0x20),
        "image1_segments": parse_bank_segments(img1_off),
        "dsp_partitions": partitions,
    }


def inspect_ns4b_archive(path: Path) -> BackupSummary:
    """Inspects a .ns4b ZIP archive without extracting large sample banks to disk."""
    with zipfile.ZipFile(path, "r") as zf:
        meta_bytes = zf.read("meta.xml")
        attrs = parse_meta_xml(meta_bytes.decode("utf-8"))
        counts: dict[str, int] = {}
        for name in zf.namelist():
            ext = name.rsplit(".", 1)[-1] if "." in name else "NONE"
            counts[ext] = counts.get(ext, 0) + 1
    return BackupSummary(
        product_id=attrs["product_id"],
        product_version=attrs["product_version"],
        product_build=attrs["product_build"],
        manager_version=attrs["manager_version"],
        extension_counts=counts,
    )


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Inspect Nord Stage 4 CBIN presets, backups, or OS firmware."
    )
    parser.add_argument("path", type=Path, help="Path to preset, archive, or firmware file")
    parser.add_argument("--decode", action="store_true", help="Decode internal bitstream fields")
    parser.add_argument(
        "--firmware", action="store_true", help="Inspect as Clavia OS firmware container"
    )
    args = parser.parse_args(argv)

    if args.firmware or args.path.suffix.lower() in (".exe", ".bin"):
        raw = args.path.read_bytes()
        summary = parse_firmware_container(raw)
    elif args.path.suffix.lower() == ".ns4b":
        summary = asdict(inspect_ns4b_archive(args.path))
    else:
        raw = args.path.read_bytes()
        cbin = parse_cbin_bytes(raw)
        summary = asdict(cbin)
        if args.decode:
            summary["decoded"] = decode_cbin_payload(cbin.tag, raw[CBIN_HEADER_SIZE:])
    print(json.dumps(summary, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main())
