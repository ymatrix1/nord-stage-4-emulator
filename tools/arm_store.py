#!/usr/bin/env python3
"""Program store of the host firmware: record format, a Store driven through the panel keys.

`arm_i2c.py` models the four 64 KB I2C EEPROMs the program memory lives on. A blank store only
holds the volume header, so the home screen shows `<empty>`. This module drives a real Store
(`Session` plus the Store key), flushes the firmware's write-back cache to the EEPROM model and
decodes what was written, so that an initial EEPROM image with a named program can be built and
booted (`arm_screens.py boot --eeprom`). The image is firmware-derived data: keep it under
`target/` (git-ignored), never commit it.

    uv run --with unicorn python tools/arm_store.py store target/fw_unpacked/bank0_CODE.bin \
        target/fw_unpacked/bank0_SRAM.bin --state target/oled/state.bin \
        --name "Studio Grand" --out target/oled/eeprom.bin

Evidence and layout: `docs/research/arm-oled-render.md` section 13.
"""

from __future__ import annotations

import argparse
import struct
import sys
import zlib
from pathlib import Path

import arm_i2c as ai

# VERIFIED (key sweep with hooks on the Store handler `0xC01504FC` and its disabled message
# `0xC0150660`, research note s13): the Store key is `(0x10, 0, 1)`; no other key reaches them.
STORE_KEY = (0x10, 0, 1)
# VERIFIED (disassembly `0xC0150500..0xC0150660`, the Store button handler): `0xC0150540` is the
# `cmp r0, #1` of the Memory Protect test; with r0 = 1 the handler shows `Store Disabled / Memory
# Protected / in System menu`. The harness forces r0 = 0 there (Memory Protect off) because a
# System-menu value cannot be edited yet (the encoders are not decoded to a menu action).
MEMORY_PROTECT_CMP = 0xC0150540
# VERIFIED (disassembly `0xC0240518`): the storage task step; `[r0 + 8]` is a service object whose
# `[+8]` is the device cache object, `[r0 + 0x24]` / `[r0 + 0x28]` are its page budgets. The task is
# not scheduled by the harness, so the step is called by hand (`flush`).
FLUSH_STEP = 0xC0240518
# EMPIRICAL: the cache object of the program volume in the harness heap (`[+0x3C] == 1`
# write-back, `[+0x44]` points to itself, so `dev + 0x3C` can stand in as the service object).
CACHE_DEVICE = 0x90048600

# VERIFIED (EEPROM 0 after a Store, test `test_unpack_record_matches_the_real_store`): the volume
# starts with `79 B8 F8 99 01 00 00 00`; records live in 128-byte clusters from offset 0x40, each
# cluster is a 4-byte header `01 00 <index> 00` plus 124 payload bytes; the payload stream is
# `size u32` (`0x35C`), `crc32 u32` over the last 824 bytes, a 36-byte header (name[24], `p4sn`,
# 4 zeros, `0x11`, 4 bytes) and the 824-byte program body.
VOLUME_MAGIC = 0x99F8B879
CLUSTER_BASE, CLUSTER_SIZE, CLUSTER_HEADER = 0x40, 0x80, 4
CLUSTER_PAYLOAD = CLUSTER_SIZE - CLUSTER_HEADER
NAME_OFFSET, NAME_BYTES = 8, 24
BODY_OFFSET, BODY_BYTES = 8 + 36, 824


def pack_clusters(stream: bytes) -> bytes:
    """Cluster pages for a payload `stream`: header `01 00 <index> 00` + 124 bytes each."""
    pages = []
    for index, start in enumerate(range(0, len(stream), CLUSTER_PAYLOAD)):
        chunk = stream[start : start + CLUSTER_PAYLOAD]
        pages.append(bytes([1, 0, index, 0]) + chunk.ljust(CLUSTER_PAYLOAD, b"\xff"))
    return b"".join(pages)


def unpack_clusters(volume: bytes, first: int = CLUSTER_BASE) -> bytes:
    """Payload stream of the cluster chain at `first`: stops at the first free cluster."""
    out = bytearray()
    at, index = first, 0
    while at + CLUSTER_SIZE <= len(volume) and volume[at : at + 4] == bytes([1, 0, index, 0]):
        out += volume[at + CLUSTER_HEADER : at + CLUSTER_SIZE]
        at += CLUSTER_SIZE
        index += 1
    return bytes(out)


def unpack_record(stream: bytes) -> dict[str, object]:
    """Decode a payload stream: `size`, `crc`, `name` and whether the CRC matches the body."""
    size, crc = struct.unpack_from("<II", stream)
    body = stream[BODY_OFFSET : 8 + size]
    name = stream[NAME_OFFSET : NAME_OFFSET + NAME_BYTES].split(b"\0")[0].decode("latin-1")
    return {"size": size, "crc": crc, "name": name, "crc_ok": zlib.crc32(body) == crc}


def set_name(volume: bytearray, name: str, first: int = CLUSTER_BASE) -> None:
    """Rename the record at `first` in place (the CRC covers the body only, not the name)."""
    raw = name.encode("latin-1")
    if len(raw) >= NAME_BYTES:
        raise ValueError(f"name longer than {NAME_BYTES - 1} bytes")
    at = first + CLUSTER_HEADER + NAME_OFFSET
    volume[at : at + NAME_BYTES] = raw.ljust(NAME_BYTES, b"\0")


def join_image(eeproms: list[bytes]) -> bytes:
    """The four devices as one 256 KB file."""
    return b"".join(eeproms)


def split_image(blob: bytes) -> list[bytes]:
    """Inverse of `join_image`."""
    n = ai.EEPROM_SIZE
    if len(blob) != n * ai.EEPROM_COUNT:
        raise ValueError(f"expected {n * ai.EEPROM_COUNT} bytes, got {len(blob)}")
    return [blob[i * n : (i + 1) * n] for i in range(ai.EEPROM_COUNT)]


def unprotect(session) -> None:  # noqa: ANN001 - arm_screens.Session, needs unicorn
    """Force Memory Protect off in the Store handler (see `MEMORY_PROTECT_CMP`)."""
    emu = session.emu
    emu.uc.hook_add(
        emu._u.UC_HOOK_CODE,
        lambda _uc, _a, _s, _u: emu.set_reg(0, 0),
        begin=MEMORY_PROTECT_CMP,
        end=MEMORY_PROTECT_CMP,
    )


def flush(session, rounds: int = 60, pages: int = 16) -> int:  # noqa: ANN001
    """Call the storage task step by hand until the cache is written; returns I2C transactions."""
    import arm_emu as ae

    emu, start = session.emu, len(session.bus.log)
    for _ in range(rounds):
        saved = [emu.reg(n) for n in range(16)]
        sp = emu.reg(13) - 0x1000
        service = sp - 0x100
        emu.write32(service + 8, CACHE_DEVICE + 0x3C)
        emu.write32(service + 0x24, pages)
        emu.write32(service + 0x28, pages)
        emu.set_reg(0, service)
        emu.set_reg(13, sp)
        emu.set_reg(14, ae.SENTINEL)
        emu.run(FLUSH_STEP, ae.SENTINEL, 5_000_000)
        for n in range(16):
            emu.set_reg(n, saved[n])
    return len(session.bus.log) - start


def store_current(session) -> list[bytes]:  # noqa: ANN001
    """Press Store twice (target screen, then confirm), flush, return the four EEPROM images."""
    unprotect(session)
    session.press([STORE_KEY], hold=100, after=60)
    session.press([STORE_KEY], hold=100, after=100)
    flush(session)
    return [bytes(e.data) for e in session.bus.eeproms]


def main(argv: list[str] | None = None) -> int:  # pragma: no cover - needs the local firmware
    import arm_screens as sc

    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    p.add_argument("cmd", choices=["store"])
    p.add_argument("image", type=Path)
    p.add_argument("sram", type=Path)
    p.add_argument("--state", type=Path, default=Path("target/oled/state.bin"))
    p.add_argument("--name", default="")
    p.add_argument("--out", type=Path, default=Path("target/oled/eeprom.bin"))
    ns = p.parse_args(argv)
    session = sc.Session(ns.image, ns.sram)
    session.load_state(ns.state.read_bytes())
    images = [bytearray(e) for e in store_current(session)]
    if ns.name:
        set_name(images[0], ns.name)
    rec = unpack_record(unpack_clusters(bytes(images[0])))
    print("record:", rec)
    ns.out.parent.mkdir(parents=True, exist_ok=True)
    ns.out.write_bytes(join_image([bytes(i) for i in images]))
    return 0


if __name__ == "__main__":
    sys.exit(main())
