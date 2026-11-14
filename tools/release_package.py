#!/usr/bin/env python3
"""Package one platform's release archive from `cargo build` / `cargo xtask bundle` output.

Replaces the per-OS shell and PowerShell packaging in `.github/workflows/release.yml`
with one tested implementation: same layout on every OS, LF-only `.sha256` files,
forward-slash zip entries, and an allowlist so nothing outside the expected files
(no `.reference/`, firmware dumps or presets) can enter an archive.

Archive layout (one top-level folder, so extraction never litters the cwd):

    nord-stage-4-emulator-<tag>-<platform>/
        ns4[.exe]  nord-stage4[.exe]  nord-plugin-standalone[.exe]
        nord-plugin.clap  nord-plugin.vst3/
        README.txt  HOST_COMPATIBILITY.md  [LICENSE*]

Verify a built archive with `tools/release_verify.py`.
"""

from __future__ import annotations

import argparse
import gzip
import hashlib
import io
import re
import sys
import tarfile
import zipfile
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

PRODUCT = "nord-stage-4-emulator"
BINS = ("ns4", "nord-stage4", "nord-plugin-standalone")
PLUGIN_STEM = "nord-plugin"
README_SRC = Path("docs/RELEASE_README.txt")
EXTRA_DOCS = (("docs/HOST_COMPATIBILITY.md", "HOST_COMPATIBILITY.md"),)
TAG_RE = re.compile(r"^v(\d+\.\d+\.\d+(?:-[0-9A-Za-z.]+)?)$")


@dataclass(frozen=True)
class Platform:
    """One release target: Rust triple, archive flavour and name tag."""

    triple: str
    name: str  # used in the archive file name
    kind: str  # "zip" or "tar.gz"
    exe: str  # executable suffix

    @property
    def os(self) -> str:
        return self.name.split("-")[0]

    @property
    def vst3_binary(self) -> str:
        """Path of the VST3 module inside the bundle."""
        return {
            "windows": f"Contents/x86_64-win/{PLUGIN_STEM}.vst3",
            "linux": f"Contents/x86_64-linux/{PLUGIN_STEM}.so",
            "macos": f"Contents/MacOS/{PLUGIN_STEM}",
        }[self.os]


PLATFORMS = {
    "x86_64-unknown-linux-gnu": Platform("x86_64-unknown-linux-gnu", "linux-x86_64", "tar.gz", ""),
    "x86_64-pc-windows-msvc": Platform("x86_64-pc-windows-msvc", "windows-x86_64", "zip", ".exe"),
    "aarch64-apple-darwin": Platform("aarch64-apple-darwin", "macos-arm64", "tar.gz", ""),
}


def workspace_version(root: Path = ROOT) -> str:
    """Read `[workspace.package] version` from the root Cargo.toml."""
    text = (root / "Cargo.toml").read_text(encoding="utf-8")
    section = re.search(r"\[workspace\.package\](.*?)(?:\n\[|\Z)", text, re.S)
    match = re.search(r'^version\s*=\s*"([^"]+)"', section.group(1) if section else "", re.M)
    if not match:
        raise ValueError("no [workspace.package] version in Cargo.toml")
    return match.group(1)


def version_from_tag(tag: str, cargo_version: str) -> str:
    """Return the version for `tag` or raise if it is malformed or not the Cargo version."""
    match = TAG_RE.match(tag)
    if not match:
        raise ValueError(f"tag {tag!r} is not vMAJOR.MINOR.PATCH[-prerelease]")
    if match.group(1) != cargo_version:
        raise ValueError(
            f"tag {tag} does not match workspace version {cargo_version}; "
            "bump Cargo.toml first (docs/RELEASING.md)"
        )
    return match.group(1)


def archive_stem(label: str, platform: Platform) -> str:
    """Top-level folder and archive base name, e.g. `nord-stage-4-emulator-v0.1.0-linux-x86_64`."""
    return f"{PRODUCT}-{label}-{platform.name}"


def _walk_files(path: Path) -> list[Path]:
    """All regular files below `path` (or `path` itself), sorted for determinism."""
    if path.is_file():
        return [path]
    return sorted(p for p in path.rglob("*") if p.is_file())


def collect(
    platform: Platform, root: Path, target_dir: Path | None = None
) -> list[tuple[Path, str]]:
    """Return `(source, relative archive path)` for every file that ships.

    Only the allowlisted names below are ever read, so stray files in `target/`
    or the repo cannot leak into an archive.
    """
    target_dir = target_dir or root / "target"
    release_dir = target_dir / platform.triple / "release"
    bundled = target_dir / "bundled"
    items: list[tuple[Path, str]] = []

    def need(path: Path) -> Path:
        if not path.exists():
            raise FileNotFoundError(f"missing build output: {path}")
        return path

    for name in BINS:
        items.append((need(release_dir / f"{name}{platform.exe}"), f"{name}{platform.exe}"))
    for artifact in (f"{PLUGIN_STEM}.clap", f"{PLUGIN_STEM}.vst3"):
        base = need(bundled / artifact)
        for src in _walk_files(base):
            rel = src.relative_to(bundled).as_posix()
            items.append((src, rel))
    items.append((need(root / README_SRC), "README.txt"))
    for src, dest in EXTRA_DOCS:
        items.append((need(root / src), dest))
    for lic in sorted(root.glob("LICENSE*")):
        if lic.is_file():
            items.append((lic, lic.name))
    return items


def sha256_hex(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def write_sha256(archive: Path) -> Path:
    """Write `<archive>.sha256` as `<hex>  <name>\\n` (LF, two spaces: sha256sum format)."""
    out = archive.with_name(archive.name + ".sha256")
    with out.open("w", encoding="ascii", newline="\n") as handle:
        handle.write(f"{sha256_hex(archive)}  {archive.name}\n")
    return out


def _is_executable(rel: str) -> bool:
    """Programs and plugin modules get mode 0755 in tarballs."""
    return (
        rel in BINS
        or rel.endswith(".so")
        or "/MacOS/" in rel
        or rel == f"{PLUGIN_STEM}.clap"  # Linux CLAP is a single shared object
    )


def write_zip(archive: Path, stem: str, items: list[tuple[Path, str]]) -> None:
    with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as zf:
        for src, rel in items:
            info = zipfile.ZipInfo(f"{stem}/{rel}", date_time=(1980, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o644 << 16
            zf.writestr(info, src.read_bytes())


def write_tar_gz(archive: Path, stem: str, items: list[tuple[Path, str]]) -> None:
    """Write a gzip tarball with fixed mtime/owner and 0755 on executables."""
    raw = io.BytesIO()
    with tarfile.open(fileobj=raw, mode="w", format=tarfile.PAX_FORMAT) as tf:
        for src, rel in items:
            data = src.read_bytes()
            info = tarfile.TarInfo(f"{stem}/{rel}")
            info.size = len(data)
            info.mtime = 0
            info.uid = info.gid = 0
            info.uname = info.gname = ""
            info.mode = 0o755 if _is_executable(rel) else 0o644
            tf.addfile(info, io.BytesIO(data))
    with (
        archive.open("wb") as handle,
        gzip.GzipFile(fileobj=handle, mode="wb", mtime=0, filename="") as gz,
    ):
        gz.write(raw.getvalue())


def package(
    triple: str,
    label: str,
    out_dir: Path,
    root: Path = ROOT,
    target_dir: Path | None = None,
) -> Path:
    """Build `<out_dir>/<stem>.<ext>` plus its `.sha256` and return the archive path."""
    platform = PLATFORMS[triple]
    items = collect(platform, root, target_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    stem = archive_stem(label, platform)
    archive = out_dir / f"{stem}.{platform.kind}"
    if platform.kind == "zip":
        write_zip(archive, stem, items)
    else:
        write_tar_gz(archive, stem, items)
    write_sha256(archive)
    return archive


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--target", choices=sorted(PLATFORMS))
    parser.add_argument(
        "--check-tag", metavar="TAG", help="only check TAG against the Cargo.toml version"
    )
    parser.add_argument("--tag", help="release tag vX.Y.Z; must equal the Cargo.toml version")
    parser.add_argument("--out", type=Path, default=Path("dist"))
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--target-dir", type=Path, help="cargo target dir (default <root>/target)")
    args = parser.parse_args(argv)

    if not args.check_tag and not args.target:
        parser.error("--target is required unless --check-tag is given")
    cargo_version = workspace_version(args.root)
    try:
        if args.check_tag:
            version_from_tag(args.check_tag, cargo_version)
            print(f"release_package: {args.check_tag} matches workspace version")
            return 0
        if args.tag:
            version_from_tag(args.tag, cargo_version)
            label = args.tag
        else:
            label = f"v{cargo_version}-dev"  # dry run: never looks like a release
        archive = package(args.target, label, args.out, args.root, args.target_dir)
    except (ValueError, FileNotFoundError) as err:
        print(f"release_package: {err}", file=sys.stderr)
        return 1
    print(archive)
    return 0


if __name__ == "__main__":
    sys.exit(main())
