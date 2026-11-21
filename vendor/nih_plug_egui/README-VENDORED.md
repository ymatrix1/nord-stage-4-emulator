# Vendored nih_plug_egui

Copy of `nih_plug_egui` from https://github.com/robbert-vdh/nih-plug (rev de42101, ISC, see
`LICENSE`), used through `[patch]` in the workspace `Cargo.toml`. Local changes:

1. `editor.rs`: `queue.resize` took the LOGICAL size as a physical size, so after a plugin-driven
   resize egui rendered into a surface `ppp` times too small (content bottom-left, black rest) on
   HiDPI hosts. It now multiplies by the native pixels-per-point.
2. `EguiState::set_requested_size` is `pub` (the editor locks the panel aspect on resize).
