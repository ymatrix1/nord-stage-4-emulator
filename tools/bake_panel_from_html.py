#!/usr/bin/env python3
"""Offline HTML/CSS-to-egui panel baker for the Nord Stage 4 / nd matrix 1 UI.

Renders ``prototypes/panel-mock/`` in headless Chromium/Edge at 2x HiDPI and
exports:
  - ``panel_plate_2x.png``: static deck with moving caps/LEDs/OLEDs hidden
  - ``panel_sprites_2x.png`` + ``panel_sprites.json``: packed component atlas
  - ``panel_layout.json``: normalized ``(x, y, w, h)`` rects from DOM layout
  - ``panel_assets_manifest.json``: SHA-256 hashes & provenance (swap-ready for Blender)
"""

from __future__ import annotations

import argparse
import base64
import functools
import hashlib
import http.server
import io
import json
import os
import shutil
import socket
import struct
import subprocess
import tempfile
import threading
import time
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Any
from urllib.parse import urlparse

from PIL import Image

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_MOCK_DIR = REPO_ROOT / "prototypes" / "panel-mock"
DEFAULT_OUT_DIR = REPO_ROOT / "crates" / "nord-gui" / "assets" / "baked"

SPRITE_NAMES: tuple[str, ...] = (
    "btn_black_up",
    "btn_black_down",
    "btn_grey_up",
    "btn_grey_down",
    "btn_red_up",
    "btn_red_down",
    "knob_pot_cap",
    "knob_encoder_cap",
    "fader_cap",
    "drawbar_white",
    "drawbar_black",
    "pitch_stick",
    "mod_wheel_drum",
)


@dataclass(frozen=True)
class SpriteSlot:
    """Packed sprite placement in the 2x texture atlas."""

    name: str
    x: int
    y: int
    w: int
    h: int
    source: str = "html_css"


def sha256_bytes(data: bytes) -> str:
    """Return lowercase hex SHA-256 digest for *data*."""
    return hashlib.sha256(data).hexdigest()


def pack_sprites(
    sprites: dict[str, Image.Image],
    *,
    padding: int = 4,
    max_width: int = 1024,
) -> tuple[Image.Image, dict[str, SpriteSlot]]:
    """Shelf-pack RGBA sprites into a single transparent atlas."""
    if not sprites:
        raise ValueError("sprites mapping must not be empty")

    ordered = sorted(sprites.items(), key=lambda kv: (-kv[1].height, kv[0]))
    x_cur = padding
    y_cur = padding
    row_h = 0
    atlas_w = 0
    placements: list[tuple[str, Image.Image, int, int]] = []

    for name, img in ordered:
        w, h = img.size
        if x_cur + w + padding > max_width and x_cur > padding:
            x_cur = padding
            y_cur += row_h + padding
            row_h = 0
        placements.append((name, img, x_cur, y_cur))
        x_cur += w + padding
        row_h = max(row_h, h)
        atlas_w = max(atlas_w, x_cur)

    atlas_h = y_cur + row_h + padding
    atlas = Image.new("RGBA", (atlas_w, atlas_h), (0, 0, 0, 0))
    slots: dict[str, SpriteSlot] = {}
    for name, img, px, py in placements:
        rgba = img.convert("RGBA")
        atlas.paste(rgba, (px, py))
        slots[name] = SpriteSlot(
            name=name,
            x=px,
            y=py,
            w=rgba.width,
            h=rgba.height,
        )
    return atlas, slots


def build_sprites_manifest(
    slots: dict[str, SpriteSlot],
    atlas_size: tuple[int, int],
    *,
    scale: int = 2,
) -> dict[str, Any]:
    """Build JSON-serializable sprite atlas manifest (swap-ready for Blender sprites)."""
    aw, ah = atlas_size
    entries = {
        name: {
            "px": [s.x, s.y, s.w, s.h],
            "uv": [
                round(s.x / aw, 6),
                round(s.y / ah, 6),
                round((s.x + s.w) / aw, 6),
                round((s.y + s.h) / ah, 6),
            ],
            "source": s.source,
        }
        for name, s in sorted(slots.items())
    }
    return {"scale": scale, "atlas_px": [aw, ah], "sprites": entries}


def build_assets_manifest(files: dict[str, bytes], *, scale: int = 2) -> dict[str, Any]:
    """Build deterministic SHA-256 manifest for committed baked assets."""
    return {
        "scale": scale,
        "generator": "tools/bake_panel_from_html.py",
        "files": {
            name: {"bytes": len(raw), "sha256": sha256_bytes(raw)}
            for name, raw in sorted(files.items())
        },
    }


def find_browser_executable() -> Path | None:
    """Locate local Edge or Chrome executable for offline baking."""
    candidates = [
        shutil.which("chrome"),
        shutil.which("msedge"),
        os.path.expandvars(r"%ProgramFiles%\Google\Chrome\Application\chrome.exe"),
        os.path.expandvars(r"%ProgramFiles(x86)%\Google\Chrome\Application\chrome.exe"),
        os.path.expandvars(r"%ProgramFiles%\Microsoft\Edge\Application\msedge.exe"),
        os.path.expandvars(r"%ProgramFiles(x86)%\Microsoft\Edge\Application\msedge.exe"),
        "/usr/bin/google-chrome",
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
    ]
    for c in candidates:
        if c and Path(c).is_file():
            return Path(c)
    return None


class _QuietHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, format: str, *args: Any) -> None:  # noqa: A002
        return


class _LocalMockServer:
    def __init__(self, directory: Path) -> None:
        self.directory = directory
        self._server: http.server.ThreadingHTTPServer | None = None
        self._thread: threading.Thread | None = None
        self.port = 0

    def __enter__(self) -> _LocalMockServer:
        handler = functools.partial(_QuietHandler, directory=str(self.directory))
        self._server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
        self.port = self._server.server_address[1]
        self._thread = threading.Thread(target=self._server.serve_forever, daemon=True)
        self._thread.start()
        return self

    def __exit__(self, *_: Any) -> None:
        if self._server:
            self._server.shutdown()
            self._server.server_close()


class _CdpClient:
    """Minimal stdlib RFC-6455 WebSocket client for Chrome DevTools Protocol."""

    def __init__(self, ws_url: str) -> None:
        parsed = urlparse(ws_url)
        host = parsed.hostname or "127.0.0.1"
        port = parsed.port or 80
        path = parsed.path or "/"
        self._sock = socket.create_connection((host, port), timeout=30)
        key = base64.b64encode(os.urandom(16)).decode("ascii")
        req = (
            f"GET {path} HTTP/1.1\r\n"
            f"Host: {host}:{port}\r\n"
            "Upgrade: websocket\r\n"
            "Connection: Upgrade\r\n"
            f"Sec-WebSocket-Key: {key}\r\n"
            "Sec-WebSocket-Version: 13\r\n\r\n"
        )
        self._sock.sendall(req.encode("ascii"))
        buf = b""
        while b"\r\n\r\n" not in buf:
            buf += self._sock.recv(4096)
        self._msg_id = 0

    def close(self) -> None:
        self._sock.close()

    def _send_frame(self, text: str) -> None:
        payload = text.encode("utf-8")
        mask = os.urandom(4)
        header = bytearray([0x81])
        n = len(payload)
        if n < 126:
            header.append(0x80 | n)
        elif n < 65536:
            header.append(0x80 | 126)
            header.extend(struct.pack("!H", n))
        else:
            header.append(0x80 | 127)
            header.extend(struct.pack("!Q", n))
        header.extend(mask)
        masked = bytes(b ^ mask[i % 4] for i, b in enumerate(payload))
        self._sock.sendall(bytes(header) + masked)

    def _recv_exact(self, n: int) -> bytes:
        chunks: list[bytes] = []
        rem = n
        while rem > 0:
            chunk = self._sock.recv(rem)
            if not chunk:
                raise ConnectionError("CDP WebSocket closed unexpectedly")
            chunks.append(chunk)
            rem -= len(chunk)
        return b"".join(chunks)

    def _recv_frame(self) -> str:
        while True:
            b1, b2 = self._recv_exact(2)
            opcode = b1 & 0x0F
            length = b2 & 0x7F
            if length == 126:
                (length,) = struct.unpack("!H", self._recv_exact(2))
            elif length == 127:
                (length,) = struct.unpack("!Q", self._recv_exact(8))
            if b2 & 0x80:
                mask = self._recv_exact(4)
                raw = bytes(b ^ mask[i % 4] for i, b in enumerate(self._recv_exact(length)))
            else:
                raw = self._recv_exact(length)
            if opcode == 0x8:
                raise ConnectionError("CDP WebSocket closed by browser")
            if opcode == 0x9:
                continue
            if opcode in (0x1, 0x0):
                return raw.decode("utf-8")

    def call(self, method: str, params: dict[str, Any] | None = None) -> dict[str, Any]:
        self._msg_id += 1
        cid = self._msg_id
        self._send_frame(json.dumps({"id": cid, "method": method, "params": params or {}}))
        while True:
            msg = json.loads(self._recv_frame())
            if msg.get("id") == cid:
                if "error" in msg:
                    raise RuntimeError(f"CDP {method} failed: {msg['error']}")
                return msg.get("result", {})

    def eval_js(self, expr: str) -> Any:
        res = self.call("Runtime.evaluate", {"expression": expr, "returnByValue": True})
        if "exceptionDetails" in res:
            raise RuntimeError(f"JS eval error: {res['exceptionDetails']}")
        return res.get("result", {}).get("value")


def _wait_cdp_ws_url(port: int, timeout_s: float = 15.0) -> str:
    deadline = time.monotonic() + timeout_s
    url = f"http://127.0.0.1:{port}/json"
    while time.monotonic() < deadline:
        try:
            with urllib.request.urlopen(url, timeout=1.0) as resp:
                targets = json.loads(resp.read().decode("utf-8"))
                for t in targets:
                    if t.get("type") == "page" and "webSocketDebuggerUrl" in t:
                        return str(t["webSocketDebuggerUrl"])
        except OSError:
            pass
        time.sleep(0.15)
    raise TimeoutError(f"Timed out waiting for CDP target on port {port}")


def bake_from_html(
    mock_dir: Path,
    out_dir: Path,
    *,
    browser: Path | None = None,
    scale: int = 2,
) -> dict[str, Any]:
    """Render *mock_dir* in headless Chromium/Edge and write baked PNGs + JSONs to *out_dir*."""
    exe = browser or find_browser_executable()
    if exe is None:
        raise RuntimeError("No Chrome/Edge executable found; pass --browser or install Edge/Chrome")

    out_dir.mkdir(parents=True, exist_ok=True)

    # Reserve an ephemeral debugging port
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        cdp_port = s.getsockname()[1]

    with (
        _LocalMockServer(mock_dir) as srv,
        tempfile.TemporaryDirectory(prefix="ns4_baker_") as profile_dir,
    ):
        proc = subprocess.Popen(
            [
                str(exe),
                "--headless=new",
                "--disable-gpu",
                "--no-sandbox",
                f"--remote-debugging-port={cdp_port}",
                f"--user-data-dir={profile_dir}",
                "--window-size=2800,900",
                f"http://127.0.0.1:{srv.port}/index.html",
            ],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        try:
            ws_url = _wait_cdp_ws_url(cdp_port)
            cdp = _CdpClient(ws_url)
            try:
                cdp.call("Page.enable")
                cdp.call(
                    "Emulation.setDefaultBackgroundColorOverride",
                    {"color": {"r": 0, "g": 0, "b": 0, "a": 0}},
                )
                cdp.call(
                    "Emulation.setDeviceMetricsOverride",
                    {
                        "width": 3600,
                        "height": 1000,
                        "deviceScaleFactor": scale,
                        "mobile": False,
                    },
                )
                # Wait until panel.js finishes building bays & group-box SVG borders
                for _ in range(100):
                    ready = cdp.eval_js(
                        "document.readyState === 'complete' && "
                        "Boolean(window.__ns4ExtractLayout && document.getElementById('bay-fx'))"
                    )
                    if ready:
                        break
                    time.sleep(0.1)
                else:
                    raise TimeoutError(
                        "Timed out waiting for prototypes/panel-mock to finish rendering"
                    )
                time.sleep(0.25)

                # 1. Switch to static bake mode, size viewport to full chassis, and extract layout
                cdp.eval_js(
                    "document.body.classList.add('bake-static'); void document.body.offsetHeight;"
                )
                time.sleep(0.15)
                chassis_js = (
                    "(() => { const r = document.getElementById('ns4-chassis')"
                    ".getBoundingClientRect();"
                    " return { x: r.left, y: r.top, width: r.width, height: r.height }; })()"
                )
                chassis_box = cdp.eval_js(chassis_js)
                if chassis_box["width"] + 64 > 3600:
                    cdp.call(
                        "Emulation.setDeviceMetricsOverride",
                        {
                            "width": int(chassis_box["width"] + 64),
                            "height": int(chassis_box["height"] + 64),
                            "deviceScaleFactor": scale,
                            "mobile": False,
                        },
                    )
                    time.sleep(0.15)
                    chassis_box = cdp.eval_js(chassis_js)
                layout = cdp.eval_js("window.__ns4ExtractLayout()")

                plate_shot = cdp.call(
                    "Page.captureScreenshot",
                    {
                        "format": "png",
                        "omitBackground": True,
                        "clip": {
                            "x": chassis_box["x"],
                            "y": chassis_box["y"],
                            "width": chassis_box["width"],
                            "height": chassis_box["height"],
                            "scale": 1,
                        },
                    },
                )
                plate_bytes = base64.b64decode(plate_shot["data"])

                # 2. Capture each isolated component sprite
                sprites: dict[str, Image.Image] = {}
                for name in SPRITE_NAMES:
                    clip = cdp.eval_js(f"window.__ns4StageSprite({json.dumps(name)})")
                    time.sleep(0.05)
                    shot = cdp.call(
                        "Page.captureScreenshot",
                        {
                            "format": "png",
                            "omitBackground": True,
                            "clip": {**clip, "scale": 1},
                        },
                    )
                    sprites[name] = Image.open(io.BytesIO(base64.b64decode(shot["data"]))).convert(
                        "RGBA"
                    )
            finally:
                cdp.close()
        finally:
            proc.terminate()
            proc.wait(timeout=5)

    atlas_img, slots = pack_sprites(sprites)
    atlas_buf = io.BytesIO()
    atlas_img.save(atlas_buf, format="PNG", optimize=True)
    atlas_bytes = atlas_buf.getvalue()

    sprites_json = json.dumps(
        build_sprites_manifest(slots, atlas_img.size, scale=scale),
        indent=2,
    ).encode("utf-8")
    layout_json = json.dumps(layout, indent=2).encode("utf-8")

    outputs: dict[str, bytes] = {
        "panel_plate_2x.png": plate_bytes,
        "panel_sprites_2x.png": atlas_bytes,
        "panel_sprites.json": sprites_json,
        "panel_layout.json": layout_json,
    }
    manifest = build_assets_manifest(outputs, scale=scale)
    outputs["panel_assets_manifest.json"] = json.dumps(manifest, indent=2).encode("utf-8")

    for fname, raw in outputs.items():
        (out_dir / fname).write_bytes(raw)

    return manifest


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Bake 2x panel plate, sprites, and layout from HTML/CSS."
    )
    parser.add_argument("--mock-dir", type=Path, default=DEFAULT_MOCK_DIR)
    parser.add_argument("--out-dir", type=Path, default=DEFAULT_OUT_DIR)
    parser.add_argument("--browser", type=Path, default=None)
    parser.add_argument("--scale", type=int, default=2)
    args = parser.parse_args(argv)

    manifest = bake_from_html(args.mock_dir, args.out_dir, browser=args.browser, scale=args.scale)
    print(f"Baked {len(manifest['files'])} asset(s) into {args.out_dir}:")
    for name, info in manifest["files"].items():
        print(f"  {name}: {info['bytes']} bytes (sha256:{info['sha256'][:12]}...)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
