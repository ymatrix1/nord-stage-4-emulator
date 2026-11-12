#!/usr/bin/env python3
"""Nord Stage 4 Hardware Video & Audio Reference Workbench.

Reusable toolkit for fetching/locating YouTube hardware demo videos (such as
``https://www.youtube.com/watch?v=_zksT69H30c``), auto-detecting the Nord Red
top panel, extracting high-resolution section/OLED crops + 48 kHz WAV slices,
rendering timestamped preset storyboards, and comparing spectral/stereo audio
telemetry between hardware recordings and emulator output.
"""

from __future__ import annotations

import argparse
import dataclasses
import json
import math
import re
import shutil
import struct
import subprocess
import sys
from pathlib import Path
from typing import Any, Sequence
from urllib.parse import parse_qs, urlparse

from PIL import Image, ImageDraw

DEFAULT_REFERENCE_DIR = Path(".reference/videos")
DEFAULT_SOURCES_INDEX = DEFAULT_REFERENCE_DIR / "sources.json"
VIDEO_EXTENSIONS = (".webm", ".mp4", ".mkv", ".mov")

BUILTIN_SOURCES: dict[str, dict[str, Any]] = {
    "_zksT69H30c": {
        "id": "_zksT69H30c",
        "alias": "part_a",
        "url": "https://www.youtube.com/watch?v=_zksT69H30c",
        "title": "NEW NORD STAGE 4 - PROGRAMAS PRESETS DEMO (PART A)",
        "creator": "Filipe Martins",
        "bookmarks": {
            "04:45": "A:15 Hybrid Str Whl",
            "07:25": "A:21 White Grand",
        },
    },
}

# Panel-relative ROI boxes (x0, y0, x1, y1) inside detected Nord Red top panel [0..1, 0..1].
# Note: `keybed` extends below y=1.0 to capture the player's hands on the 88-key bed.
PANEL_ROI_BOXES: dict[str, tuple[float, float, float, float]] = {
    "panel": (0.00, 0.00, 1.00, 1.00),
    "master_organ": (0.01, 0.02, 0.32, 0.98),
    "organ": (0.12, 0.04, 0.32, 0.96),
    "piano": (0.31, 0.04, 0.415, 0.96),
    "program_oled": (0.41, 0.04, 0.53, 0.96),
    "oled_zoom": (0.435, 0.42, 0.51, 0.67),
    "synth": (0.52, 0.04, 0.77, 0.96),
    "fx": (0.765, 0.04, 0.965, 0.96),
    "reverb": (0.875, 0.56, 0.96, 0.95),
    "keybed": (0.01, 1.00, 0.99, 1.92),
}

DEFAULT_EXTRACT_ROIS = ("panel", "oled_zoom", "piano", "fx", "reverb")

SPECTRAL_BANDS: tuple[tuple[str, float, float], ...] = (
    ("sub_20_120", 20.0, 120.0),
    ("low_120_400", 120.0, 400.0),
    ("mid_400_1500", 400.0, 1500.0),
    ("presence_1500_3000", 1500.0, 3000.0),
    ("brilliance_3000_8000", 3000.0, 8000.0),
    ("air_8000_20000", 8000.0, 20000.0),
)

_YT_ID_RE = re.compile(r"^[A-Za-z0-9_-]{11}$")
_BRACKET_ID_RE = re.compile(r"\[([A-Za-z0-9_-]{11})\]")


@dataclasses.dataclass(frozen=True)
class AudioTelemetry:
    """Acoustic, stereo, and 6-band spectral telemetry of an audio clip."""

    path: str
    sample_rate: int
    channels: int
    frames: int
    duration_s: float
    peak_dbfs: float
    rms_dbfs: float
    crest_db: float
    stereo_width_ratio: float
    lr_balance_db: float
    spectral_centroid_hz: float
    bands_dbfs: dict[str, float]
    bands_rel_db: dict[str, float]

    def to_dict(self) -> dict[str, Any]:
        return dataclasses.asdict(self)


def extract_video_id(source: str) -> str | None:
    """Extracts an 11-char YouTube ID from a URL, alias, bracketed filename, or bare ID."""
    text = source.strip()
    if not text:
        return None
    for vid, meta in BUILTIN_SOURCES.items():
        if text.lower() == str(meta.get("alias", "")).lower():
            return vid
    if _YT_ID_RE.match(text):
        return text
    if text.startswith(("http://", "https://")):
        parsed = urlparse(text)
        host = parsed.netloc.lower()
        if "youtu.be" in host:
            cand = parsed.path.strip("/").split("/")[0]
            if _YT_ID_RE.match(cand):
                return cand
        qs = parse_qs(parsed.query)
        if "v" in qs and qs["v"] and _YT_ID_RE.match(qs["v"][0]):
            return qs["v"][0]
        parts = [p for p in parsed.path.split("/") if p]
        for marker in ("shorts", "embed", "live"):
            if marker in parts:
                idx = parts.index(marker)
                if idx + 1 < len(parts) and _YT_ID_RE.match(parts[idx + 1]):
                    return parts[idx + 1]
    match = _BRACKET_ID_RE.search(text)
    if match:
        return match.group(1)
    return None


def parse_timestamp(value: str | float | int) -> float:
    """Parses seconds, ``MM:SS``, or ``HH:MM:SS(.ms)`` into non-negative float seconds."""
    if isinstance(value, (int, float)):
        secs = float(value)
    else:
        raw = value.strip()
        if not raw:
            raise ValueError("Empty timestamp")
        if ":" not in raw:
            secs = float(raw)
        else:
            parts = raw.split(":")
            if len(parts) == 2:
                m, s = float(parts[0]), float(parts[1])
                if m < 0 or s < 0:
                    raise ValueError(f"Negative timestamp component in '{value}'")
                secs = m * 60.0 + s
            elif len(parts) == 3:
                h, m, s = float(parts[0]), float(parts[1]), float(parts[2])
                if h < 0 or m < 0 or s < 0:
                    raise ValueError(f"Negative timestamp component in '{value}'")
                secs = h * 3600.0 + m * 60.0 + s
            else:
                raise ValueError(f"Invalid timestamp format: '{value}'")
    if not math.isfinite(secs) or secs < 0.0:
        raise ValueError(f"Timestamp must be finite and >= 0: '{value}'")
    return secs


def format_timestamp(seconds: float) -> str:
    """Formats seconds into filename-safe ``HHhMMmSSs`` or ``MMmSSs`` / ``MM:SS`` display."""
    total = max(0, int(round(seconds)))
    h, rem = divmod(total, 3600)
    m, s = divmod(rem, 60)
    if h > 0:
        return f"{h:02d}:{m:02d}:{s:02d}"
    return f"{m:02d}:{s:02d}"


def timestamp_slug(seconds: float) -> str:
    """Returns filesystem-safe timestamp identifier such as ``07m25s``."""
    total = max(0, int(round(seconds)))
    h, rem = divmod(total, 3600)
    m, s = divmod(rem, 60)
    if h > 0:
        return f"{h:02d}h{m:02d}m{s:02d}s"
    return f"{m:02d}m{s:02d}s"


def default_search_dirs(extra_dirs: Sequence[Path] = ()) -> list[Path]:
    """Returns deduplicated local search directories for cached reference videos."""
    home = Path.home()
    candidates = [
        *extra_dirs,
        DEFAULT_REFERENCE_DIR,
        home / "Downloads" / "Best available",
        home / "Downloads",
    ]
    seen: set[Path] = set()
    out: list[Path] = []
    for cand in candidates:
        resolved = cand.expanduser()
        if resolved not in seen:
            seen.add(resolved)
            out.append(resolved)
    return out


def find_local_video(video_id: str, search_dirs: Sequence[Path] = ()) -> Path | None:
    """Searches configured directories for a video matching ``[<video_id>]``."""
    dirs = default_search_dirs(search_dirs)
    token = f"[{video_id}]"
    for directory in dirs:
        if not directory.is_dir():
            continue
        matches = sorted(
            p
            for p in directory.iterdir()
            if p.is_file()
            and p.suffix.lower() in VIDEO_EXTENSIONS
            and (token in p.name or p.stem == video_id)
        )
        if matches:
            return matches[0]
    return None


def load_sources_registry(registry_path: Path = DEFAULT_SOURCES_INDEX) -> dict[str, dict[str, Any]]:
    """Loads built-in sources merged with any saved local ``sources.json`` metadata."""
    merged = {k: dict(v) for k, v in BUILTIN_SOURCES.items()}
    if registry_path.is_file():
        saved = json.loads(registry_path.read_text(encoding="utf-8"))
        if isinstance(saved, dict):
            for k, val in saved.items():
                if isinstance(val, dict):
                    merged[k] = {**merged.get(k, {}), **val}
    return merged


def save_source_entry(
    entry: dict[str, Any],
    registry_path: Path = DEFAULT_SOURCES_INDEX,
) -> Path:
    """Persists or updates a video reference entry in ``.reference/videos/sources.json``."""
    vid = entry["id"]
    current: dict[str, Any] = {}
    if registry_path.is_file():
        loaded = json.loads(registry_path.read_text(encoding="utf-8"))
        if isinstance(loaded, dict):
            current = loaded
    existing = current.get(vid, {})
    current[vid] = {**existing, **entry}
    registry_path.parent.mkdir(parents=True, exist_ok=True)
    registry_path.write_text(json.dumps(current, indent=2, sort_keys=True), encoding="utf-8")
    return registry_path


def _require_binary(name: str) -> str:
    resolved = shutil.which(name)
    if not resolved:
        raise FileNotFoundError(
            f"Required executable '{name}' not found in PATH. "
            f"Install {name} to use video fetching/extraction features."
        )
    return resolved


def fetch_video(
    source: str,
    output_dir: Path = DEFAULT_REFERENCE_DIR,
    search_dirs: Sequence[Path] = (),
    force_download: bool = False,
    registry_path: Path = DEFAULT_SOURCES_INDEX,
) -> Path:
    """Locates an existing local video for ``source`` or downloads it via ``yt-dlp``."""
    direct = Path(source)
    if direct.is_file():
        return direct

    vid = extract_video_id(source)
    if vid is None:
        raise FileNotFoundError(f"Video path not found and no YouTube ID parsed from '{source}'")

    if not force_download:
        existing = find_local_video(vid, search_dirs=search_dirs)
        if existing is not None:
            return existing

    yt_dlp = _require_binary("yt-dlp")
    url = (
        source
        if source.startswith(("http://", "https://"))
        else f"https://www.youtube.com/watch?v={vid}"
    )
    output_dir.mkdir(parents=True, exist_ok=True)
    template = str(output_dir / "%(title)s [%(id)s].%(ext)s")
    cmd = [
        yt_dlp,
        "--no-playlist",
        "--write-info-json",
        "-f",
        "bestvideo+bestaudio/best",
        "-o",
        template,
        url,
    ]
    subprocess.run(cmd, check=True)
    downloaded = find_local_video(vid, search_dirs=(output_dir,))
    if downloaded is None:
        raise FileNotFoundError(f"yt-dlp completed but no video matching [{vid}] found")
    save_source_entry(
        {"id": vid, "url": url, "local_path": str(downloaded)},
        registry_path=registry_path,
    )
    return downloaded


def resolve_video(source: str, search_dirs: Sequence[Path] = ()) -> tuple[Path, str]:
    """Resolves ``source`` (file path, alias, URL, or ID) to an existing local video file."""
    cand = Path(source)
    if cand.is_file():
        vid = extract_video_id(cand.name) or cand.stem
        return (cand, vid)
    vid = extract_video_id(source)
    if vid is None:
        raise FileNotFoundError(f"Could not find local video or parse YouTube ID from '{source}'")
    found = find_local_video(vid, search_dirs=search_dirs)
    if found is None:
        raise FileNotFoundError(
            f"Video [{vid}] not found locally. Run `video_ref.py fetch {source}` first."
        )
    return (found, vid)


def probe_video(video_path: Path) -> dict[str, Any]:
    """Runs ``ffprobe`` to inspect streams, format metadata, tags, and embedded chapters."""
    ffprobe = _require_binary("ffprobe")
    cmd = [
        ffprobe,
        "-v",
        "quiet",
        "-print_format",
        "json",
        "-show_format",
        "-show_streams",
        "-show_chapters",
        str(video_path),
    ]
    proc = subprocess.run(cmd, check=True, capture_output=True, text=True)
    return json.loads(proc.stdout)


def detect_red_panel_box(
    im: Image.Image,
    min_red_excess: int = 85,
    min_red_value: int = 115,
    min_red_ratio: float = 2.4,
    row_density: float = 0.05,
    col_density: float = 0.06,
) -> tuple[int, int, int, int]:
    """Auto-detects the Nord Red top panel bounding box ``(x0, y0, x1, y1)``.

    Uses chromatic red excess ``R - max(G, B)`` and red/non-red ratio ``R / max(G, B)``
    on a downsampled thumbnail so warm skin tones, salmon shirts, white keys, and dark
    backgrounds are ignored while the Nord Stage 4 crimson aluminum chassis is isolated.
    """
    w, h = im.size
    small_w = min(320, max(32, w))
    small_h = max(24, int(round(small_w * h / max(1, w))))
    raw = im.convert("RGB").resize((small_w, small_h), Image.Resampling.BILINEAR).tobytes()

    row_counts = [0] * small_h
    col_counts = [0] * small_w
    for y in range(small_h):
        row_base = y * small_w * 3
        for x in range(small_w):
            off = row_base + x * 3
            r, g, b = raw[off], raw[off + 1], raw[off + 2]
            hi_gb = max(g, b)
            if (
                r >= min_red_value
                and (r - hi_gb) >= min_red_excess
                and r >= min_red_ratio * max(1, hi_gb)
            ):
                row_counts[y] += 1
                col_counts[x] += 1

    # Require horizontal red coverage (end-cheeks + divider borders + rails)
    row_thresh = max(3, int(small_w * row_density))
    valid_rows = [y for y, count in enumerate(row_counts) if count >= row_thresh]
    if not valid_rows:
        return (
            int(round(0.06 * w)),
            int(round(0.54 * h)),
            int(round(0.94 * w)),
            int(round(0.79 * h)),
        )

    # Pick the highest-scoring contiguous vertical run (gap <= 3 rows)
    best_span: tuple[int, int] = (valid_rows[0], valid_rows[0])
    best_score = -1
    run_start = valid_rows[0]
    prev_y = valid_rows[0]
    run_score = row_counts[prev_y]
    for y in valid_rows[1:]:
        if y <= prev_y + 3:
            run_score += row_counts[y]
            prev_y = y
        else:
            if run_score > best_score:
                best_score = run_score
                best_span = (run_start, prev_y)
            run_start = y
            prev_y = y
            run_score = row_counts[y]
    if run_score > best_score:
        best_span = (run_start, prev_y)

    # Recompute horizontal density specifically within the winning vertical band
    band_col_counts = [0] * small_w
    band_y0, band_y1 = best_span
    for y in range(band_y0, band_y1 + 1):
        row_base = y * small_w * 3
        for x in range(small_w):
            off = row_base + x * 3
            r, g, b = raw[off], raw[off + 1], raw[off + 2]
            hi_gb = max(g, b)
            if (
                r >= min_red_value
                and (r - hi_gb) >= min_red_excess
                and r >= min_red_ratio * max(1, hi_gb)
            ):
                band_col_counts[x] += 1

    band_h = max(1, band_y1 - band_y0 + 1)
    col_thresh = max(1, int(max(band_h * 0.15, small_h * col_density * 0.15)))
    valid_cols = [x for x, count in enumerate(band_col_counts) if count >= col_thresh]
    if not valid_cols:
        valid_cols = [x for x, count in enumerate(col_counts) if count >= 1]
    if not valid_cols:
        return (
            int(round(0.06 * w)),
            int(round(0.54 * h)),
            int(round(0.94 * w)),
            int(round(0.79 * h)),
        )

    sx = w / small_w
    sy = h / small_h
    x0 = max(0, int(math.floor(valid_cols[0] * sx)))
    x1 = min(w, int(math.ceil((valid_cols[-1] + 1) * sx)))
    y0 = max(0, int(math.floor(band_y0 * sy)))
    y1 = min(h, int(math.ceil((band_y1 + 1) * sy)))
    return (x0, y0, max(x0 + 1, x1), max(y0 + 1, y1))


def compute_roi_pixel_box(
    img_size: tuple[int, int],
    panel_box: tuple[int, int, int, int],
    roi_name: str,
) -> tuple[int, int, int, int]:
    """Converts a panel-relative named ROI into clamped integer image pixel coordinates."""
    w, h = img_size
    if roi_name == "full":
        return (0, 0, w, h)
    if roi_name not in PANEL_ROI_BOXES:
        valid = ", ".join(["full", *PANEL_ROI_BOXES.keys()])
        raise ValueError(f"Unknown ROI '{roi_name}'. Valid choices: {valid}")
    px0, py0, px1, py1 = panel_box
    pw = max(1, px1 - px0)
    ph = max(1, py1 - py0)
    rx0, ry0, rx1, ry1 = PANEL_ROI_BOXES[roi_name]
    x0 = max(0, min(w - 1, int(round(px0 + rx0 * pw))))
    y0 = max(0, min(h - 1, int(round(py0 + ry0 * ph))))
    x1 = max(x0 + 1, min(w, int(round(px0 + rx1 * pw))))
    y1 = max(y0 + 1, min(h, int(round(py0 + ry1 * ph))))
    return (x0, y0, x1, y1)


def extract_roi_crops(
    image_path: Path,
    outdir: Path,
    prefix: str,
    rois: Sequence[str] = DEFAULT_EXTRACT_ROIS,
    panel_box_norm: tuple[float, float, float, float] | None = None,
) -> dict[str, Path]:
    """Crops requested named panel ROIs from an extracted video frame PNG."""
    im = Image.open(image_path)
    w, h = im.size
    if panel_box_norm is not None:
        nx0, ny0, nx1, ny1 = panel_box_norm
        panel_box = (
            max(0, int(round(nx0 * w))),
            max(0, int(round(ny0 * h))),
            min(w, int(round(nx1 * w))),
            min(h, int(round(ny1 * h))),
        )
    else:
        panel_box = detect_red_panel_box(im)

    resolved_rois = list(PANEL_ROI_BOXES.keys()) if "all" in rois else list(rois)
    outdir.mkdir(parents=True, exist_ok=True)
    saved: dict[str, Path] = {}
    for roi in resolved_rois:
        box = compute_roi_pixel_box((w, h), panel_box, roi)
        out_path = outdir / f"{prefix}_{roi}.png"
        im.crop(box).save(out_path)
        saved[roi] = out_path
    return saved


def extract_segment(
    video_path: Path,
    timestamp_s: float,
    outdir: Path,
    rois: Sequence[str] = DEFAULT_EXTRACT_ROIS,
    duration_s: float = 15.0,
    pcm_format: str = "s16",
    panel_box_norm: tuple[float, float, float, float] | None = None,
) -> dict[str, Any]:
    """Extracts a frame, panel ROI crops, and optional 48 kHz WAV clip at ``timestamp_s``."""
    ffmpeg = _require_binary("ffmpeg")
    outdir.mkdir(parents=True, exist_ok=True)
    slug = timestamp_slug(timestamp_s)
    full_png = outdir / f"{slug}_full.png"

    subprocess.run(
        [
            ffmpeg,
            "-y",
            "-ss",
            f"{timestamp_s:.3f}",
            "-i",
            str(video_path),
            "-frames:v",
            "1",
            str(full_png),
        ],
        check=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )

    crops = extract_roi_crops(
        full_png,
        outdir=outdir,
        prefix=slug,
        rois=rois,
        panel_box_norm=panel_box_norm,
    )

    wav_path: Path | None = None
    if duration_s > 0.0:
        codec = "pcm_f32le" if pcm_format == "f32" else "pcm_s16le"
        wav_path = outdir / f"{slug}_{int(round(duration_s))}s.wav"
        subprocess.run(
            [
                ffmpeg,
                "-y",
                "-ss",
                f"{timestamp_s:.3f}",
                "-t",
                f"{duration_s:.3f}",
                "-i",
                str(video_path),
                "-vn",
                "-ac",
                "2",
                "-ar",
                "48000",
                "-c:a",
                codec,
                str(wav_path),
            ],
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )

    return {
        "timestamp_s": timestamp_s,
        "timestamp": format_timestamp(timestamp_s),
        "full_frame": str(full_png),
        "crops": {k: str(v) for k, v in crops.items()},
        "wav": str(wav_path) if wav_path is not None else None,
    }


def compose_storyboard_grid(
    tile_paths: Sequence[tuple[float, Path]],
    output_path: Path,
    cols: int = 6,
    tile_width: int = 360,
) -> Path:
    """Composes timestamp-labeled PNG tiles into a single contact-sheet grid image."""
    if not tile_paths:
        raise ValueError("Cannot build storyboard from empty tile list")
    cols = max(1, cols)
    opened: list[tuple[float, Image.Image]] = []
    for ts, path in tile_paths:
        im = Image.open(path).convert("RGB")
        aspect = im.height / max(1, im.width)
        tile_h = max(1, int(round(tile_width * aspect)))
        opened.append((ts, im.resize((tile_width, tile_h), Image.Resampling.LANCZOS)))

    cell_h = max(im.height for _, im in opened)
    rows = math.ceil(len(opened) / cols)
    pad = 6
    canvas = Image.new(
        "RGB",
        (cols * (tile_width + pad) + pad, rows * (cell_h + pad) + pad),
        color=(18, 20, 24),
    )
    draw = ImageDraw.Draw(canvas)
    for idx, (ts, tile) in enumerate(opened):
        r, c = divmod(idx, cols)
        x0 = pad + c * (tile_width + pad)
        y0 = pad + r * (cell_h + pad)
        canvas.paste(tile, (x0, y0))
        label = format_timestamp(ts)
        draw.rectangle((x0 + 4, y0 + 4, x0 + 10 + len(label) * 7, y0 + 20), fill=(0, 0, 0))
        draw.text((x0 + 7, y0 + 6), label, fill=(255, 220, 90))

    output_path.parent.mkdir(parents=True, exist_ok=True)
    canvas.save(output_path)
    return output_path


def build_storyboard(
    video_path: Path,
    output_path: Path,
    start_s: float = 0.0,
    end_s: float | None = None,
    step_s: float = 30.0,
    roi: str = "oled_zoom",
    cols: int = 6,
    panel_box_norm: tuple[float, float, float, float] | None = None,
) -> Path:
    """Samples frames from ``video_path`` every ``step_s`` and renders a labeled ROI grid."""
    if step_s <= 0.0:
        raise ValueError("Storyboard step_s must be > 0")
    if end_s is None:
        meta = probe_video(video_path)
        dur = float(meta.get("format", {}).get("duration", start_s + 300.0))
        end_s = max(start_s, dur - 1.0)
    if end_s < start_s:
        raise ValueError("Storyboard end_s must be >= start_s")

    timestamps: list[float] = []
    cur = start_s
    while cur <= end_s + 1e-6:
        timestamps.append(cur)
        cur += step_s

    work_dir = output_path.parent / f".storyboard_tmp_{output_path.stem}"
    tile_paths: list[tuple[float, Path]] = []
    try:
        for ts in timestamps:
            res = extract_segment(
                video_path,
                timestamp_s=ts,
                outdir=work_dir,
                rois=(roi,),
                duration_s=0.0,
                panel_box_norm=panel_box_norm,
            )
            tile_paths.append((ts, Path(res["crops"][roi])))
        return compose_storyboard_grid(tile_paths, output_path=output_path, cols=cols)
    finally:
        if work_dir.exists():
            shutil.rmtree(work_dir, ignore_errors=True)


def read_wav_stereo(path: Path) -> tuple[int, list[float], list[float]]:
    """Reads PCM (8/16/24/32-bit) or IEEE float (32/64-bit) mono/stereo WAV into ``[-1, 1]``."""
    raw = path.read_bytes()
    if len(raw) < 12 or raw[:4] != b"RIFF" or raw[8:12] != b"WAVE":
        raise ValueError(f"Not a valid RIFF/WAVE file: {path}")

    pos = 12
    fmt_tag: int | None = None
    channels = 0
    sample_rate = 0
    bits_per_sample = 0
    data_bytes = b""

    while pos + 8 <= len(raw):
        chunk_id = raw[pos : pos + 4]
        chunk_size = struct.unpack_from("<I", raw, pos + 4)[0]
        payload_start = pos + 8
        payload_end = min(len(raw), payload_start + chunk_size)
        if chunk_id == b"fmt " and chunk_size >= 16:
            fmt_tag, channels, sample_rate, _, _, bits_per_sample = struct.unpack_from(
                "<HHIIHH", raw, payload_start
            )
            if fmt_tag == 0xFFFE and chunk_size >= 40:
                fmt_tag = struct.unpack_from("<H", raw, payload_start + 24)[0]
        elif chunk_id == b"data":
            data_bytes = raw[payload_start:payload_end]
        pos = payload_end + (chunk_size & 1)

    if fmt_tag is None or channels not in (1, 2) or sample_rate <= 0:
        raise ValueError(f"Unsupported WAV format or channel count ({channels}): {path}")

    frame_bytes = (bits_per_sample // 8) * channels
    if frame_bytes <= 0:
        raise ValueError(f"Invalid WAV bit depth ({bits_per_sample}): {path}")
    nframes = len(data_bytes) // frame_bytes
    total_samples = nframes * channels

    interleaved: list[float]
    if fmt_tag == 1 and bits_per_sample == 16:
        vals = struct.unpack_from(f"<{total_samples}h", data_bytes, 0)
        interleaved = [v / 32768.0 for v in vals]
    elif fmt_tag == 1 and bits_per_sample == 8:
        interleaved = [(b - 128) / 128.0 for b in data_bytes[:total_samples]]
    elif fmt_tag == 1 and bits_per_sample == 24:
        interleaved = []
        for idx in range(0, total_samples * 3, 3):
            b0, b1, b2 = data_bytes[idx], data_bytes[idx + 1], data_bytes[idx + 2]
            signed = b0 | (b1 << 8) | (b2 << 16)
            if signed & 0x800000:
                signed -= 0x1000000
            interleaved.append(signed / 8388608.0)
    elif fmt_tag == 1 and bits_per_sample == 32:
        vals = struct.unpack_from(f"<{total_samples}i", data_bytes, 0)
        interleaved = [v / 2147483648.0 for v in vals]
    elif fmt_tag == 3 and bits_per_sample == 32:
        interleaved = list(struct.unpack_from(f"<{total_samples}f", data_bytes, 0))
    elif fmt_tag == 3 and bits_per_sample == 64:
        interleaved = list(struct.unpack_from(f"<{total_samples}d", data_bytes, 0))
    else:
        raise ValueError(
            f"Unsupported WAV encoding (fmt={fmt_tag}, bits={bits_per_sample}): {path}"
        )

    if channels == 1:
        return (sample_rate, interleaved, list(interleaved))
    left = interleaved[0::2]
    right = interleaved[1::2]
    return (sample_rate, left, right)


def _fft_radix2(samples: Sequence[float]) -> list[complex]:
    """Iterative Cooley-Tukey radix-2 FFT for power-of-two real/complex frames."""
    n = len(samples)
    if n == 0 or (n & (n - 1)) != 0:
        raise ValueError(f"FFT length must be a power of 2, got {n}")
    buf = [complex(x) for x in samples]
    j = 0
    for i in range(1, n):
        bit = n >> 1
        while j & bit:
            j ^= bit
            bit >>= 1
        j ^= bit
        if i < j:
            buf[i], buf[j] = buf[j], buf[i]

    size = 2
    while size <= n:
        half = size // 2
        angle = -2.0 * math.pi / size
        w_step = complex(math.cos(angle), math.sin(angle))
        for start in range(0, n, size):
            w = 1.0 + 0.0j
            for k in range(half):
                u = buf[start + k]
                t = w * buf[start + k + half]
                buf[start + k] = u + t
                buf[start + k + half] = u - t
                w *= w_step
        size <<= 1
    return buf


def _rfft_power_bins(frame: Sequence[float]) -> list[float]:
    """Computes single-sided magnitude-squared power spectrum using numpy if available."""
    try:
        import numpy as np  # type: ignore[import-not-found]

        arr = np.asarray(frame, dtype=np.float64)
        spec = np.fft.rfft(arr)
        return (spec.real**2 + spec.imag**2).tolist()
    except ImportError:
        full = _fft_radix2(frame)
        half = len(frame) // 2 + 1
        return [c.real * c.real + c.imag * c.imag for c in full[:half]]


def _to_dbfs(power: float, floor_db: float = -120.0) -> float:
    if power <= 1e-12:
        return floor_db
    return max(floor_db, 10.0 * math.log10(power))


def analyze_wav(path: Path, fft_size: int = 2048, max_frames: int = 48) -> AudioTelemetry:
    """Computes peak/RMS dBFS, stereo width, L/R balance, centroid, and 6-band spectrum."""
    sr, left, right = read_wav_stereo(path)
    nframes = min(len(left), len(right))
    if nframes == 0:
        raise ValueError(f"Empty WAV audio stream: {path}")

    peak = 0.0
    l_sq = 0.0
    r_sq = 0.0
    mid_sq = 0.0
    side_sq = 0.0
    mono: list[float] = [0.0] * nframes

    for i in range(nframes):
        l_val = left[i]
        r_val = right[i]
        peak = max(peak, abs(l_val), abs(r_val))
        l_sq += l_val * l_val
        r_sq += r_val * r_val
        m = 0.5 * (l_val + r_val)
        s = 0.5 * (l_val - r_val)
        mono[i] = m
        mid_sq += m * m
        side_sq += s * s

    mean_sq = 0.5 * (l_sq + r_sq) / nframes
    rms_dbfs = _to_dbfs(mean_sq)
    peak_dbfs = _to_dbfs(peak * peak)
    crest_db = peak_dbfs - rms_dbfs
    stereo_width_ratio = math.sqrt(side_sq / mid_sq) if mid_sq > 1e-18 else 0.0
    lr_balance_db = 0.5 * (_to_dbfs(l_sq / nframes) - _to_dbfs(r_sq / nframes))

    # Evenly sample Hann-windowed frames across the recording
    nfft = min(fft_size, 1 << max(6, int(math.log2(max(64, nframes)))))
    if nfft > nframes:
        nfft = 1 << int(math.log2(max(64, nframes))) if nframes >= 64 else 64
    hann = [0.5 - 0.5 * math.cos(2.0 * math.pi * k / max(1, nfft - 1)) for k in range(nfft)]
    hann_norm = sum(w * w for w in hann)

    if nframes <= nfft:
        offsets = [0]
    else:
        span = nframes - nfft
        count = min(max_frames, max(1, nframes // max(1, nfft // 2)))
        offsets = [int(round(i * span / max(1, count - 1))) for i in range(count)]

    accum_bins = [0.0] * (nfft // 2 + 1)
    for off in offsets:
        seg = mono[off : off + nfft]
        if len(seg) < nfft:
            seg = [*seg, *([0.0] * (nfft - len(seg)))]
        windowed = [seg[k] * hann[k] for k in range(nfft)]
        pw = _rfft_power_bins(windowed)
        # Scale one-sided PSD into RMS-consistent bin power (doubling interior bins)
        scale = 2.0 / (max(1e-12, nfft * hann_norm))
        for b_idx, pval in enumerate(pw):
            factor = 0.5 * scale if b_idx in (0, len(pw) - 1) else scale
            accum_bins[b_idx] += pval * factor

    avg_bins = [p / len(offsets) for p in accum_bins]
    bin_hz = sr / nfft

    bands_dbfs: dict[str, float] = {}
    bands_rel_db: dict[str, float] = {}
    total_spec_energy = 0.0
    weighted_freq_sum = 0.0

    for b_idx, pval in enumerate(avg_bins):
        if b_idx == 0:
            continue
        freq = b_idx * bin_hz
        total_spec_energy += pval
        weighted_freq_sum += freq * pval

    centroid_hz = weighted_freq_sum / total_spec_energy if total_spec_energy > 1e-18 else 0.0

    for name, f_lo, f_hi in SPECTRAL_BANDS:
        lo_bin = max(1, int(math.floor(f_lo / bin_hz)))
        hi_bin = min(len(avg_bins) - 1, int(math.ceil(f_hi / bin_hz)))
        band_power = sum(avg_bins[lo_bin : hi_bin + 1]) if hi_bin >= lo_bin else 0.0
        db_val = _to_dbfs(band_power)
        bands_dbfs[name] = round(db_val, 2)
        bands_rel_db[name] = round(db_val - rms_dbfs, 2)

    return AudioTelemetry(
        path=str(path),
        sample_rate=sr,
        channels=2,
        frames=nframes,
        duration_s=round(nframes / sr, 3),
        peak_dbfs=round(peak_dbfs, 2),
        rms_dbfs=round(rms_dbfs, 2),
        crest_db=round(crest_db, 2),
        stereo_width_ratio=round(stereo_width_ratio, 4),
        lr_balance_db=round(lr_balance_db, 2),
        spectral_centroid_hz=round(centroid_hz, 1),
        bands_dbfs=bands_dbfs,
        bands_rel_db=bands_rel_db,
    )


def compare_wavs(hw_path: Path, emu_path: Path) -> dict[str, Any]:
    """Computes side-by-side loudness, stereo, centroid, and 6-band spectral deltas."""
    hw = analyze_wav(hw_path)
    emu = analyze_wav(emu_path)
    band_rel_delta = {
        k: round(emu.bands_rel_db[k] - hw.bands_rel_db[k], 2) for k in hw.bands_rel_db
    }
    return {
        "hardware": hw.to_dict(),
        "emulator": emu.to_dict(),
        "delta_emu_minus_hw": {
            "peak_dbfs": round(emu.peak_dbfs - hw.peak_dbfs, 2),
            "rms_dbfs": round(emu.rms_dbfs - hw.rms_dbfs, 2),
            "crest_db": round(emu.crest_db - hw.crest_db, 2),
            "stereo_width_ratio": round(emu.stereo_width_ratio - hw.stereo_width_ratio, 4),
            "spectral_centroid_hz": round(emu.spectral_centroid_hz - hw.spectral_centroid_hz, 1),
            "bands_rel_db": band_rel_delta,
        },
    }


def _parse_panel_box(raw: str | None) -> tuple[float, float, float, float] | None:
    if not raw:
        return None
    parts = [float(p.strip()) for p in raw.split(",")]
    if len(parts) != 4:
        raise ValueError(f"--panel-box expects 4 comma-separated floats, got '{raw}'")
    x0, y0, x1, y1 = parts
    if not (0.0 <= x0 < x1 <= 1.0 and 0.0 <= y0 < y1 <= 1.0):
        raise ValueError(f"--panel-box coordinates must satisfy 0 <= x0 < x1 <= 1: '{raw}'")
    return (x0, y0, x1, y1)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="video_ref.py",
        description="Nord Stage 4 YouTube/local video & audio reference toolkit.",
    )
    parser.add_argument("--json", action="store_true", help="Emit machine-readable JSON output.")
    sub = parser.add_subparsers(dest="command", required=True)

    p_src = sub.add_parser("sources", help="List known Nord Stage 4 reference videos & bookmarks.")
    p_src.add_argument("--add", help="Register a new YouTube URL or video ID locally.")
    p_src.add_argument("--alias", help="Optional short alias for --add.")
    p_src.add_argument("--title", help="Optional title for --add.")

    p_fetch = sub.add_parser("fetch", help="Locate locally or download video via yt-dlp.")
    p_fetch.add_argument("source", help="YouTube URL, video ID, alias (e.g. part_a), or file path.")
    p_fetch.add_argument("--outdir", default=str(DEFAULT_REFERENCE_DIR))
    p_fetch.add_argument("--force", action="store_true", help="Force download via yt-dlp.")

    p_probe = sub.add_parser("probe", help="Inspect streams, duration, tags, and chapters.")
    p_probe.add_argument("source", help="YouTube URL, video ID, alias, or local video path.")

    p_ext = sub.add_parser(
        "extract", help="Extract PNG frames, Nord panel ROIs, and 48 kHz stereo WAV slice."
    )
    p_ext.add_argument("source", help="YouTube URL, video ID, alias, or local video path.")
    p_ext.add_argument("--at", nargs="+", required=True, help="Timestamps (e.g. 07:25 445).")
    p_ext.add_argument("--duration", type=float, default=15.0, help="Audio clip seconds (0=none).")
    p_ext.add_argument("--rois", nargs="+", default=list(DEFAULT_EXTRACT_ROIS))
    p_ext.add_argument("--panel-box", help="Normalized x0,y0,x1,y1 override.")
    p_ext.add_argument("--pcm-format", choices=("s16", "f32"), default="s16")
    p_ext.add_argument("--outdir", help="Destination folder (default .reference/videos/extracted).")

    p_sb = sub.add_parser("storyboard", help="Generate a timestamped preset grid PNG.")
    p_sb.add_argument("source", help="YouTube URL, video ID, alias, or local video path.")
    p_sb.add_argument("--start", default="0", help="Start timestamp (default: 0).")
    p_sb.add_argument("--end", help="End timestamp (default: full video).")
    p_sb.add_argument("--step", type=float, default=30.0, help="Step seconds (default: 30).")
    p_sb.add_argument("--roi", default="oled_zoom", help="ROI name to tile (default: oled_zoom).")
    p_sb.add_argument("--cols", type=int, default=6)
    p_sb.add_argument("--panel-box", help="Normalized x0,y0,x1,y1 override.")
    p_sb.add_argument("--output", help="Output PNG path.")

    p_ana = sub.add_parser("analyze", help="Analyze loudness, stereo width, and 6-band spectrum.")
    p_ana.add_argument("wav", help="Path to WAV audio file.")

    p_cmp = sub.add_parser("compare", help="Compare hardware reference WAV against emulator WAV.")
    p_cmp.add_argument("hw_wav", help="Hardware reference WAV file.")
    p_cmp.add_argument("emu_wav", help="Emulator rendered WAV file.")

    return parser


def _print_comparison(report: dict[str, Any]) -> None:
    hw = report["hardware"]
    emu = report["emulator"]
    delta = report["delta_emu_minus_hw"]
    print(f"Hardware: {hw['path']} ({hw['duration_s']}s, {hw['sample_rate']}Hz)")
    print(f"Emulator: {emu['path']} ({emu['duration_s']}s, {emu['sample_rate']}Hz)")
    print(
        f"RMS: HW {hw['rms_dbfs']} dBFS | Emu {emu['rms_dbfs']} dBFS "
        f"(delta {delta['rms_dbfs']:+.2f} dB)"
    )
    print(
        f"Stereo Width: HW {hw['stereo_width_ratio']:.3f} | "
        f"Emu {emu['stereo_width_ratio']:.3f} (delta {delta['stereo_width_ratio']:+.4f})"
    )
    print(
        f"Centroid: HW {hw['spectral_centroid_hz']} Hz | "
        f"Emu {emu['spectral_centroid_hz']} Hz (delta {delta['spectral_centroid_hz']:+.1f} Hz)"
    )
    print("\nRelative Spectral Profile (dB vs RMS):")
    for band in hw["bands_rel_db"]:
        h_val = hw["bands_rel_db"][band]
        e_val = emu["bands_rel_db"][band]
        d_val = delta["bands_rel_db"][band]
        print(f"  {band:22s}  HW {h_val:+6.2f} dB  |  Emu {e_val:+6.2f} dB  |  {d_val:+6.2f} dB")


def main(argv: Sequence[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)

    if args.command == "sources":
        if args.add:
            vid = extract_video_id(args.add)
            if vid is None:
                raise SystemExit(f"Could not extract YouTube ID from '{args.add}'")
            save_source_entry(
                {
                    "id": vid,
                    "url": args.add,
                    "alias": args.alias or vid,
                    "title": args.title or f"Reference Video {vid}",
                }
            )
        reg = load_sources_registry()
        for vid, entry in reg.items():
            entry["local_cached"] = str(find_local_video(vid) or "")
        if args.json:
            print(json.dumps(reg, indent=2))
        else:
            for vid, entry in reg.items():
                print(f"[{vid}] {entry.get('title', vid)} ({entry.get('url', '')})")
                if entry.get("local_cached"):
                    print(f"  cached: {entry['local_cached']}")
                for ts, label in entry.get("bookmarks", {}).items():
                    print(f"  {ts} -> {label}")
        return 0

    if args.command == "fetch":
        path = fetch_video(args.source, output_dir=Path(args.outdir), force_download=args.force)
        print(json.dumps({"path": str(path)}) if args.json else str(path))
        return 0

    if args.command == "probe":
        video_path, vid = resolve_video(args.source)
        info = probe_video(video_path)
        if args.json:
            print(json.dumps(info, indent=2))
        else:
            fmt = info.get("format", {})
            print(f"Video [{vid}]: {video_path}")
            print(f"Title: {fmt.get('tags', {}).get('title', video_path.stem)}")
            print(f"Duration: {float(fmt.get('duration', 0.0)):.2f}s | Size: {fmt.get('size')} B")
            for st in info.get("streams", []):
                print(
                    f"  Stream #{st.get('index')} {st.get('codec_type')}: "
                    f"{st.get('codec_name')} {st.get('width', '')}x{st.get('height', '')} "
                    f"{st.get('sample_rate', '')}"
                )
        return 0

    if args.command == "extract":
        video_path, vid = resolve_video(args.source)
        outdir = Path(args.outdir) if args.outdir else DEFAULT_REFERENCE_DIR / "extracted" / vid
        panel_box = _parse_panel_box(args.panel_box)
        results = [
            extract_segment(
                video_path,
                timestamp_s=parse_timestamp(ts),
                outdir=outdir,
                rois=args.rois,
                duration_s=args.duration,
                pcm_format=args.pcm_format,
                panel_box_norm=panel_box,
            )
            for ts in args.at
        ]
        if args.json:
            print(json.dumps({"video": str(video_path), "segments": results}, indent=2))
        else:
            for item in results:
                print(f"[{item['timestamp']}] frame={item['full_frame']} wav={item['wav']}")
                for roi, rpath in item["crops"].items():
                    print(f"  roi:{roi} -> {rpath}")
        return 0

    if args.command == "storyboard":
        video_path, vid = resolve_video(args.source)
        out = (
            Path(args.output)
            if args.output
            else DEFAULT_REFERENCE_DIR / "extracted" / vid / f"storyboard_{args.roi}.png"
        )
        sb = build_storyboard(
            video_path,
            output_path=out,
            start_s=parse_timestamp(args.start),
            end_s=parse_timestamp(args.end) if args.end else None,
            step_s=args.step,
            roi=args.roi,
            cols=args.cols,
            panel_box_norm=_parse_panel_box(args.panel_box),
        )
        print(json.dumps({"storyboard": str(sb)}) if args.json else str(sb))
        return 0

    if args.command == "analyze":
        telemetry = analyze_wav(Path(args.wav))
        if args.json:
            print(json.dumps(telemetry.to_dict(), indent=2))
        else:
            d = telemetry.to_dict()
            print(f"{d['path']} | {d['duration_s']}s | Peak {d['peak_dbfs']} dBFS")
            print(
                f"RMS {d['rms_dbfs']} dBFS | Crest {d['crest_db']} dB | "
                f"Width {d['stereo_width_ratio']} | Centroid {d['spectral_centroid_hz']} Hz"
            )
            for band, rel_db in d["bands_rel_db"].items():
                print(f"  {band:22s}: {rel_db:+6.2f} dB (rel)")
        return 0

    if args.command == "compare":
        report = compare_wavs(Path(args.hw_wav), Path(args.emu_wav))
        if args.json:
            print(json.dumps(report, indent=2))
        else:
            _print_comparison(report)
        return 0

    return 1


if __name__ == "__main__":
    sys.exit(main())
