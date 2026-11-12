#!/usr/bin/env python3
"""Official Nord Stage 4 OS updater download links (nordkeyboards.com).

The numeric document id in ``/wt/documents/<id>/<file>`` is only valid together with the exact
file name, so every known id is recorded here. Ids rise with upload date. The server answers
``302`` (to a short-lived signed link) when id and name match and ``404`` otherwise, so probes
need no download.

    python tools/nord_os_urls.py            # list known links
    python tools/nord_os_urls.py 1.60       # print one URL

Saved updaters live in ``.reference/`` (git-ignored) as ``Update Nord Stage 4 OS v<ver>.zip``; each
zip holds one ``.exe``. Firmware resource: see ``docs/research/dsp-image-hunt.md`` section 5.

Provenance: every id below is VERIFIED by a ``HEAD`` request that returned ``302`` on 2026-11-12
(v1.44 and v1.46 came from the user, v1.66 from the vendor downloads page).
Ids marked ``None`` are not found yet (probed ranges are in ``PROBED``).
"""

from __future__ import annotations

import sys
from urllib.parse import quote

BASE = "https://www.nordkeyboards.com/wt/documents"

# version -> (document id, extension)
DOCS: dict[str, tuple[int | None, str]] = {
    "1.42": (None, "zip"),
    "1.44": (767, "zip"),
    "1.46": (783, "zip"),
    "1.52": (899, "zip"),
    "1.54": (906, "zip"),
    "1.60": (945, "zip"),
    "1.62": (947, "zip"),
    "1.64": (949, "zip"),
    "1.66": (1042, "zip"),
}

# Vendor downloads page lists the current OS as consecutive ids: 1041 = .dmg, 1042 = .zip.
EXTRA = {("1.66", "dmg"): 1041}

# (version, first id, last id) ranges that returned 404 for that file name (zip).
PROBED: list[tuple[str, int, int]] = [
    ("1.52", 784, 898),
    ("1.52", 900, 1040),
    ("1.54", 1000, 1040),
    ("1.60", 900, 937),
    ("1.60", 1000, 1040),
    ("1.62", 900, 937),
    ("1.64", 900, 937),
    ("1.44", 700, 766),
    ("1.42", 640, 782),
]


def file_name(version: str, ext: str = "zip") -> str:
    """Vendor file name for an OS version."""
    return f"Update Nord Stage 4 OS v{version}.{ext}"


def url(version: str, ext: str = "zip") -> str | None:
    """Download URL for a version, or None when the document id is unknown."""
    doc = EXTRA.get((version, ext))
    if doc is None and version in DOCS and DOCS[version][1] == ext:
        doc = DOCS[version][0]
    if doc is None:
        return None
    return f"{BASE}/{doc}/{quote(file_name(version, ext))}"


def main(argv: list[str] | None = None) -> int:
    args = sys.argv[1:] if argv is None else argv
    versions = args or list(DOCS)
    for v in versions:
        print(f"{v}: {url(v) or 'unknown id'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
