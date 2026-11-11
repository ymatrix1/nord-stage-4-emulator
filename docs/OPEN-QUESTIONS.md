# Open Questions

| # | Question | Notes |
|---|---|---|
| **OQ1** | Which open piano SFZ library should be the default auto-downloaded bundle? | Candidates: Salamander Grand Piano V3 (CC-BY 3.0), Ivy Audio Piano in 162 (CC-BY), or VSCO-2 CE Upright (CC0). |
| **OQ2** | Resolved in [D11](DECISIONS.md) (DAW Plugin CLAP/VST3 + Standalone via `nih-plug`). Which GUI sub-crate inside `nih-plug`: `nih_plug_vizia` or `nih_plug_iced`? | `nih_plug_vizia` has built-in rotary knobs, LED meters, and CSS styling ideal for the Nord red panel layout. |
| **OQ3** | Exact CRC-32 polynomial/seed used at `CBIN` offset `+0x18..+0x1C`? | Standard IEEE 802.3 (`0xEDB88320`) vs custom seed over payload bytes `+0x2E..EOF`; currently preserved verbatim on round-trip and being verified across all 384 `.ns4p` files. |
| **OQ4** | Bit offsets for Synth Wavetable index inside `.ns4y` (541 B)? | Comparing `1 Osc Saw Bass.ns4y` vs `12db Pad.ns4y` across all 384 synth presets (`tools/ns4_inspect.py`). |
| **OQ5** | Should WebAssembly + WebMIDI browser demo deploy automatically to Cloudflare Pages or GitHub Pages? | Once Phase 6 UI lands, can deploy static WASM bundle with COOP/COEP headers. |
| **OQ6** | Open-source license choice for repository release? | MIT / Apache-2.0 dual license vs GPL-3.0-or-later (note: `nih-plug` VST3 wrapper uses GPLv3 if VST3 is enabled; CLAP + Standalone can stay MIT/Apache-2.0 or MPL-2.0). |
