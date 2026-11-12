# Open Questions

| # | Question | Notes |
|---|---|---|
| **OQ1** | Which open piano SFZ library should be the default auto-downloaded bundle? | Candidates: Salamander Grand Piano V3 (CC-BY 3.0), Ivy Audio Piano in 162 (CC-BY), or VSCO-2 CE Upright (CC0). |
| **OQ2** | Resolved in [D11](DECISIONS.md) (DAW Plugin CLAP/VST3 + Standalone via `nih-plug`). Which GUI sub-crate inside `nih-plug`: `nih_plug_vizia` or `nih_plug_iced`? | `nih_plug_vizia` has built-in rotary knobs, LED meters, and CSS styling ideal for the Nord red panel layout. |
| **OQ3** | Exact CRC-32 polynomial/seed used at `CBIN` offset `+0x18..+0x1C`? | Standard IEEE 802.3 (`0xEDB88320`) vs custom seed over payload bytes `+0x2E..EOF`; currently preserved verbatim on round-trip and being verified across all 384 `.ns4p` files. |
| **OQ5** | Should WebAssembly + WebMIDI browser demo deploy automatically to Cloudflare Pages or GitHub Pages? | Once Phase 6 UI lands, can deploy static WASM bundle with COOP/COEP headers. |
| **OQ6** | Open-source license choice for repository release? | MIT / Apache-2.0 dual license vs GPL-3.0-or-later (note: `nih-plug` VST3 wrapper uses GPLv3 if VST3 is enabled; CLAP + Standalone can stay MIT/Apache-2.0 or MPL-2.0). |
| **OQ7** | Are ns4decode's Synth `0x32`/`0x2F` offsets and value scales confirmed by firmware/hardware (glide time, vibrato rate/amount, osc env amount, unison cents, LFO mod amount for pitch, WAVE bank waveforms)? | Disassemble the `ARM0` preset parser or record hardware sweeps; compare with `tools/ns4_crosscheck.py` output. Wave names/order already confirmed in firmware (`docs/research/synth-0x2f-ns4decode-crosscheck.md`). OQ4 (wavetable index offsets) is resolved by [D13](DECISIONS.md). |
