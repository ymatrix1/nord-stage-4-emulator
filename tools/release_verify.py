#!/usr/bin/env python3
"""Verify a release archive built by `tools/release_package.py`.

Checks, with no third-party dependencies and no need for `dumpbin`/`nm`:

1. Layout: one top-level folder holding exactly the expected files, including the
   VST3 module at its platform path (Windows `Contents/x86_64-win/nord-plugin.vst3`).
2. Nothing forbidden: no `.reference/`, `target/`, firmware, preset, backup or
   sample files, and no manufacturer name in the shipped plugin/GUI binaries.
3. Plugin entry points: `clap_entry` in the CLAP module and `GetPluginFactory`
   in the VST3 module. PE files (Windows) get a real export-table parse; ELF and
   Mach-O modules are checked by symbol-name search in the file image.
4. `<archive>.sha256` matches the archive and uses LF line endings.

Usage: python tools/release_verify.py <archive> [--target <rust triple>]
Exit status 0 when clean, 1 with one problem per line on stderr otherwise.
"""

from __future__ import annotations

import argparse
import struct
import sys
import tarfile
import zipfile
from pathlib import Path

from release_package import BINS, PLATFORMS, PLUGIN_STEM, Platform, sha256_hex

FORBIDDEN_PARTS = frozenset(
    {
        ".reference",
        "references",
        "factory_programs",
        "target",
        ".git",
        ".beans",
        ".claude",
        ".agents",
    }
)
FORBIDDEN_SUFFIXES = (
    ".ns4b", ".ns4p", ".ns4y", ".ns4o", ".ns4n", ".ns4l", ".ns4t",  # Clavia Binary files
    ".nsmp4", ".npno", ".pdf", ".bin", ".fw", ".ldr", ".dxe", ".hex", ".asm", ".pyc", ".pdb",
)  # fmt: skip
# Manufacturer name must not be compiled into anything a user sees.
FORBIDDEN_BYTES = (b"Clavia", b"CLAVIA")
CLAP_SYMBOL = "clap_entry"
VST3_SYMBOL = "GetPluginFactory"


def detect_platform(archive: Path, triple: str | None = None) -> Platform:
    if triple:
        return PLATFORMS[triple]
    for platform in PLATFORMS.values():
        if f"-{platform.name}." in archive.name:
            return platform
    raise ValueError(f"cannot infer platform from {archive.name}; pass --target")


def read_archive(archive: Path) -> dict[str, bytes]:
    """Return `{path: bytes}` for every file; paths keep the top-level folder."""
    files: dict[str, bytes] = {}
    if archive.name.endswith(".zip"):
        with zipfile.ZipFile(archive) as zf:
            for info in zf.infolist():
                if not info.is_dir():
                    files[info.filename] = zf.read(info)
    else:
        with tarfile.open(archive, "r:gz") as tf:
            for member in tf.getmembers():
                if member.isfile():
                    handle = tf.extractfile(member)
                    files[member.name] = handle.read() if handle else b""
    return files


def expected_files(platform: Platform) -> list[str]:
    """Paths (relative to the top folder) that must exist."""
    exe = platform.exe
    base = [f"{name}{exe}" for name in BINS] + ["README.txt", "HOST_COMPATIBILITY.md"]
    vst3 = f"{PLUGIN_STEM}.vst3/{platform.vst3_binary}"
    if platform.os == "macos":
        return base + [
            f"{PLUGIN_STEM}.clap/Contents/MacOS/{PLUGIN_STEM}",
            f"{PLUGIN_STEM}.clap/Contents/Info.plist",
            f"{PLUGIN_STEM}.vst3/Contents/Info.plist",
            vst3,
        ]
    return base + [f"{PLUGIN_STEM}.clap", vst3]


def check_layout(paths: list[str], platform: Platform) -> list[str]:
    """Problems with names: single top folder, required files, forbidden content."""
    problems: list[str] = []
    tops = {p.split("/", 1)[0] for p in paths}
    if len(tops) != 1:
        return [f"expected one top-level folder, found {sorted(tops)}"]
    top = next(iter(tops))
    rel = {p.split("/", 1)[1] for p in paths if "/" in p}
    problems += [f"missing {need}" for need in expected_files(platform) if need not in rel]
    for path in sorted(rel):
        lower = path.lower()
        if "\\" in path:
            problems.append(f"backslash in archive path {path!r}")
        if set(path.split("/")) & FORBIDDEN_PARTS:
            problems.append(f"forbidden path {top}/{path}")
        if lower.endswith(FORBIDDEN_SUFFIXES):
            problems.append(f"forbidden file type {top}/{path}")
    return problems


def pe_exports(data: bytes) -> set[str]:
    """Names in the export table of a PE (DLL) image; empty set if absent."""
    if data[:2] != b"MZ":
        raise ValueError("not a PE file")
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    if data[pe : pe + 4] != b"PE\0\0":
        raise ValueError("bad PE signature")
    nsections = struct.unpack_from("<H", data, pe + 6)[0]
    opt_size = struct.unpack_from("<H", data, pe + 20)[0]
    opt = pe + 24
    magic = struct.unpack_from("<H", data, opt)[0]
    dir_off = opt + (112 if magic == 0x20B else 96)
    export_rva = struct.unpack_from("<I", data, dir_off)[0]
    if export_rva == 0:
        return set()
    sections = [
        struct.unpack_from("<IIII", data, opt + opt_size + 40 * i + 8) for i in range(nsections)
    ]  # (virtual size, virtual address, raw size, raw pointer)

    def to_offset(rva: int) -> int:
        for vsize, vaddr, rsize, raw in sections:
            if vaddr <= rva < vaddr + max(vsize, rsize):
                return rva - vaddr + raw
        raise ValueError(f"RVA {rva:#x} not in any section")

    exp = to_offset(export_rva)
    count = struct.unpack_from("<I", data, exp + 24)[0]
    names_rva = struct.unpack_from("<I", data, exp + 32)[0]
    names = to_offset(names_rva)
    out: set[str] = set()
    for i in range(count):
        start = to_offset(struct.unpack_from("<I", data, names + 4 * i)[0])
        out.add(data[start : data.index(b"\0", start)].decode("ascii"))
    return out


def has_symbol(data: bytes, symbol: str) -> bool:
    """True when `symbol` is exported (PE table) or present as a symbol name (ELF/Mach-O)."""
    if data[:2] == b"MZ":
        try:
            return symbol in pe_exports(data)
        except (struct.error, ValueError, UnicodeDecodeError) as err:
            raise ValueError(f"truncated or malformed PE image ({err})") from err
    needle = symbol.encode()
    return any(
        data[i - 1 : i] in (b"\0", b"_") and data[i + len(needle) : i + len(needle) + 1] == b"\0"
        for i in _find_all(data, needle)
    )


def _find_all(data: bytes, needle: bytes):
    pos = data.find(needle)
    while pos != -1:
        yield pos
        pos = data.find(needle, pos + 1)


def check_content(files: dict[str, bytes], platform: Platform) -> list[str]:
    """Entry points, manufacturer-name scan and README disclaimer."""
    problems: list[str] = []
    rel = {p.split("/", 1)[1]: data for p, data in files.items() if "/" in p}
    clap = rel.get(
        f"{PLUGIN_STEM}.clap"
        if platform.os != "macos"
        else f"{PLUGIN_STEM}.clap/Contents/MacOS/{PLUGIN_STEM}"
    )
    vst3 = rel.get(f"{PLUGIN_STEM}.vst3/{platform.vst3_binary}")
    for label, data, symbol in (("CLAP", clap, CLAP_SYMBOL), ("VST3", vst3, VST3_SYMBOL)):
        if data is None:
            continue  # reported by check_layout
        try:
            if not has_symbol(data, symbol):
                problems.append(f"{label} module does not export {symbol}")
        except ValueError as err:
            problems.append(f"{label} module unreadable: {err}")
    for name, data in rel.items():
        if name.split("/")[0] in {f"{b}{platform.exe}" for b in BINS} or name.startswith(
            PLUGIN_STEM
        ):
            problems += [
                f"manufacturer name {needle.decode()!r} found in {name}"
                for needle in FORBIDDEN_BYTES
                if needle in data
            ]
    readme = rel.get("README.txt", b"").decode("utf-8", "replace").lower()
    if "not been verified in any daw" not in readme:
        problems.append("README.txt must state the plugin is not DAW-verified")
    return problems


def check_sha256(archive: Path) -> list[str]:
    sidecar = archive.with_name(archive.name + ".sha256")
    if not sidecar.exists():
        return [f"missing {sidecar.name}"]
    raw = sidecar.read_bytes()
    if b"\r" in raw:
        return [f"{sidecar.name} has CR line endings (breaks `sha256sum -c`)"]
    want = f"{sha256_hex(archive)}  {archive.name}\n".encode()
    return [] if raw == want else [f"{sidecar.name} does not match {archive.name}"]


def verify(archive: Path, triple: str | None = None) -> list[str]:
    """Return every problem found (empty list means the archive is good)."""
    platform = detect_platform(archive, triple)
    files = read_archive(archive)
    problems = check_layout(list(files), platform)
    problems += check_content(files, platform)
    problems += check_sha256(archive)
    return problems


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("archive", type=Path)
    parser.add_argument("--target", choices=sorted(PLATFORMS))
    args = parser.parse_args(argv)
    try:
        problems = verify(args.archive, args.target)
    except (ValueError, OSError, zipfile.BadZipFile, tarfile.TarError) as err:
        problems = [str(err)]
    for problem in problems:
        print(f"release_verify: {problem}", file=sys.stderr)
    if not problems:
        print(f"release_verify: {args.archive.name} OK")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
