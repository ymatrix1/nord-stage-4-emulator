//! Shadcn/ui-inspired separate-window Settings & Diagnostics dialog for `nord-stage4`.
//!
//! Opens in its own native OS viewport (`show_viewport_immediate`) so it is never clipped by the
//! compact Nord Stage 4 hardware window, while sharing the exact same borderless dark hardware
//! aesthetic and vector Minimize / Close icon buttons.

use crate::audio::AudioHandle;
use crate::firmware_panel::FirmwarePanelState;
use crate::library::BackupLibraryState;
use crate::os_update_panel::OsUpdatePanel;
use crate::panel::Stage4PanelState;
use egui::{
    Color32, Context, CornerRadius, Frame, Margin, RichText, Stroke, Ui, ViewportBuilder,
    ViewportClass, ViewportCommand, ViewportId,
};

const ZINC_950_GLASS: Color32 = Color32::from_rgba_premultiplied(10, 11, 15, 210);
const ZINC_900: Color32 = Color32::from_rgba_premultiplied(20, 21, 26, 215);
const ZINC_800: Color32 = Color32::from_rgb(42, 42, 48);
const ZINC_700: Color32 = Color32::from_rgb(63, 63, 70);
const ZINC_400: Color32 = Color32::from_rgb(161, 161, 170);
const ZINC_200: Color32 = Color32::from_rgb(228, 228, 231);
const ZINC_50: Color32 = Color32::from_rgb(250, 250, 250);
const ACCENT_RED: Color32 = Color32::from_rgb(184, 37, 42);

/// Apply a cohesive dark-zinc shadcn/ui theme to the Studio Settings viewport so all buttons,
/// ComboBox popups, checkboxes, inputs, and highlights are crisp, opaque, and consistent.
fn apply_shadcn_theme(ctx: &Context) {
    ctx.style_mut(|s| {
        s.spacing.interact_size.y = 26.0;
        s.spacing.button_padding = egui::vec2(10.0, 5.0);
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.combo_height = 240.0;
        s.spacing.scroll = egui::style::ScrollStyle {
            floating: false,
            bar_width: 6.0,
            handle_min_length: 24.0,
            bar_inner_margin: 4.0,
            bar_outer_margin: 2.0,
            ..egui::style::ScrollStyle::solid()
        };

        let v = &mut s.visuals;
        v.dark_mode = true;
        v.interact_cursor = Some(egui::CursorIcon::PointingHand);
        v.panel_fill = Color32::TRANSPARENT;
        // Opaque dark zinc for ComboBox dropdowns, popups, and tooltips (no text bleed-through).
        v.window_fill = Color32::from_rgb(18, 18, 22);
        v.window_stroke = Stroke::new(1.0, ZINC_700);
        v.window_corner_radius = CornerRadius::same(8);
        v.menu_corner_radius = CornerRadius::same(8);
        v.popup_shadow = egui::epaint::Shadow {
            offset: [0, 8],
            blur: 20,
            spread: 0,
            color: Color32::from_black_alpha(180),
        };
        v.extreme_bg_color = Color32::from_rgb(9, 9, 11);
        v.faint_bg_color = Color32::from_rgb(26, 26, 31);
        v.selection.bg_fill = ACCENT_RED;
        v.selection.stroke = Stroke::new(1.0, ZINC_50);

        v.widgets.noninteractive.bg_fill = ZINC_900;
        v.widgets.noninteractive.weak_bg_fill = ZINC_900;
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, ZINC_800);
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, ZINC_200);
        v.widgets.noninteractive.corner_radius = CornerRadius::same(6);

        v.widgets.inactive.bg_fill = Color32::from_rgb(28, 28, 33);
        v.widgets.inactive.weak_bg_fill = Color32::from_rgb(28, 28, 33);
        v.widgets.inactive.bg_stroke = Stroke::new(1.0, ZINC_800);
        v.widgets.inactive.fg_stroke = Stroke::new(1.0, ZINC_200);
        v.widgets.inactive.corner_radius = CornerRadius::same(6);

        v.widgets.hovered.bg_fill = Color32::from_rgb(39, 39, 46);
        v.widgets.hovered.weak_bg_fill = Color32::from_rgb(39, 39, 46);
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, ZINC_700);
        v.widgets.hovered.fg_stroke = Stroke::new(1.0, ZINC_50);
        v.widgets.hovered.corner_radius = CornerRadius::same(6);

        v.widgets.active.bg_fill = Color32::from_rgb(52, 52, 60);
        v.widgets.active.weak_bg_fill = Color32::from_rgb(52, 52, 60);
        v.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT_RED);
        v.widgets.active.fg_stroke = Stroke::new(1.0, ZINC_50);
        v.widgets.active.corner_radius = CornerRadius::same(6);

        v.widgets.open.bg_fill = Color32::from_rgb(39, 39, 46);
        v.widgets.open.weak_bg_fill = Color32::from_rgb(39, 39, 46);
        v.widgets.open.bg_stroke = Stroke::new(1.0, ZINC_700);
        v.widgets.open.fg_stroke = Stroke::new(1.0, ZINC_50);
        v.widgets.open.corner_radius = CornerRadius::same(6);
    });
}
const FACTORY_BANK_URL: &str = "https://assets.nordkeyboards.com/media/documents/UTQHpgLs/Nord%20Stage%204%20Factory%20Restore%20RevK%20v1.66.ns4b";
const OS_UPDATE_URL: &str =
    "https://www.nordkeyboards.com/wt/documents/1042/Update%20Nord%20Stage%204%20OS%20v1.66.zip";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DebugTab {
    #[default]
    Firmware,
    AudioMidi,
    Library,
    OsUpdate,
    Guide,
}

impl DebugTab {
    pub const ALL: [Self; 5] = [
        Self::Firmware,
        Self::AudioMidi,
        Self::Library,
        Self::OsUpdate,
        Self::Guide,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Firmware => "Firmware & DSP",
            Self::AudioMidi => "Audio & MIDI",
            Self::Library => "Library & Backup",
            Self::OsUpdate => "OS & Wave Tables",
            Self::Guide => "Setup Guide",
        }
    }
}

/// Persistent UI state for the separate-window shadcn-styled Diagnostics & Settings dialog.
#[derive(Default)]
pub struct DebugDialogState {
    pub open: bool,
    pub tab: DebugTab,
    /// When set, the separate OS viewport requests focus on the next frame so clicking
    /// `OPEN .NS4B` / `DEBUG` / `? HELP` brings an already-open Settings window to the front.
    pub focus_requested: bool,
}

impl DebugDialogState {
    /// Open (or focus if already open) the Studio Settings window on `tab`.
    pub fn open_tab(&mut self, tab: DebugTab) {
        self.tab = tab;
        self.open = true;
        self.focus_requested = true;
    }
}

fn shadcn_card(ui: &mut Ui, title: &str, subtitle: &str, body: impl FnOnce(&mut Ui)) {
    Frame::NONE
        .fill(ZINC_900)
        .stroke(Stroke::new(1.0, ZINC_800))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(14))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(RichText::new(title).size(13.0).strong().color(ZINC_50));
            ui.label(RichText::new(subtitle).size(11.0).color(ZINC_400));
            ui.add_space(8.0);
            body(ui);
        });
}

fn paint_window_controls(ctx: &Context, ui: &mut Ui, dlg: &mut DebugDialogState) {
    // Vector Close button (rightmost in right_to_left layout)
    let (cls_rect, cls_resp) = ui.allocate_exact_size(egui::vec2(24.0, 18.0), egui::Sense::click());
    let cls_resp = cls_resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    let cls_bg = if cls_resp.hovered() {
        ACCENT_RED
    } else {
        ZINC_900
    };
    ui.painter().rect_filled(cls_rect, 4.0, cls_bg);
    ui.painter().rect_stroke(
        cls_rect,
        4.0,
        Stroke::new(0.8, ZINC_800),
        egui::StrokeKind::Inside,
    );
    let c = cls_rect.center();
    let d = 4.0;
    let stroke = Stroke::new(1.5, ZINC_50);
    ui.painter()
        .line_segment([c + egui::vec2(-d, -d), c + egui::vec2(d, d)], stroke);
    ui.painter()
        .line_segment([c + egui::vec2(d, -d), c + egui::vec2(-d, d)], stroke);
    if cls_resp.clicked() {
        dlg.open = false;
    }

    // Vector Minimize button
    let (min_rect, min_resp) = ui.allocate_exact_size(egui::vec2(24.0, 18.0), egui::Sense::click());
    let min_resp = min_resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    let min_bg = if min_resp.hovered() {
        Color32::from_rgb(46, 50, 58)
    } else {
        ZINC_900
    };
    ui.painter().rect_filled(min_rect, 4.0, min_bg);
    ui.painter().rect_stroke(
        min_rect,
        4.0,
        Stroke::new(0.8, ZINC_800),
        egui::StrokeKind::Inside,
    );
    ui.painter().line_segment(
        [
            egui::pos2(min_rect.center().x - 4.5, min_rect.center().y + 1.5),
            egui::pos2(min_rect.center().x + 4.5, min_rect.center().y + 1.5),
        ],
        Stroke::new(1.5, ZINC_50),
    );
    if min_resp.clicked() {
        ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
    }
}

fn render_dialog_contents(
    ui: &mut Ui,
    dlg: &mut DebugDialogState,
    state: &mut Stage4PanelState,
    library: &mut BackupLibraryState,
    firmware: &mut FirmwarePanelState,
    os_update: &mut OsUpdatePanel,
    audio_handle: &AudioHandle,
) {
    let ctx = ui.ctx().clone();
    // Header row: entire left + middle area (including title & subtitle) drags the window
    let header_resp = ui.horizontal(|ui| {
        let title_area = ui
            .vertical(|ui| {
                ui.label(
                    RichText::new("Studio Settings & Diagnostics")
                        .size(16.0)
                        .strong()
                        .color(ZINC_50),
                );
                ui.label(
                    RichText::new(
                        "Firmware DSP options, MIDI & audio metering, and .ns4b backup tools.",
                    )
                    .size(11.5)
                    .color(ZINC_400),
                );
            })
            .response;
        let drag_w = (ui.available_width() - 62.0).max(16.0);
        let (_, spacer_resp) =
            ui.allocate_exact_size(egui::vec2(drag_w, 34.0), egui::Sense::click_and_drag());
        let drag_rect = title_area.rect.union(spacer_resp.rect);
        let header_drag = ui.interact(
            drag_rect,
            egui::Id::new("ns4_debug_header_drag"),
            egui::Sense::click_and_drag(),
        );
        if header_drag.hovered() {
            ctx.set_cursor_icon(egui::CursorIcon::Grab);
        }
        if header_drag.is_pointer_button_down_on() && header_drag.dragged() {
            ctx.send_viewport_cmd(ViewportCommand::StartDrag);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
            ui.spacing_mut().item_spacing.x = 5.0;
            paint_window_controls(&ctx, ui, dlg);
        });
    });
    let _ = header_resp;

    ui.add_space(12.0);

    // Segmented pill tab bar (shadcn TabsList)
    Frame::NONE
        .fill(ZINC_900)
        .stroke(Stroke::new(1.0, ZINC_800))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for tab in DebugTab::ALL {
                    let active = dlg.tab == tab;
                    let fill = if active {
                        ACCENT_RED
                    } else {
                        Color32::TRANSPARENT
                    };
                    let text_col = if active { ZINC_50 } else { ZINC_400 };
                    let btn = ui
                        .add(
                            egui::Button::new(
                                RichText::new(tab.label())
                                    .size(11.5)
                                    .strong()
                                    .color(text_col),
                            )
                            .fill(fill)
                            .stroke(Stroke::NONE)
                            .corner_radius(CornerRadius::same(6)),
                        )
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    if btn.clicked() {
                        dlg.tab = tab;
                    }
                }
            });
        });

    ui.add_space(12.0);

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| match dlg.tab {
            DebugTab::Firmware => {
                firmware.show_organized(ui, audio_handle);
            }
            DebugTab::AudioMidi => {
                shadcn_card(
                    ui,
                    "Computer Keyboard & Velocity",
                    "Play the Nord Stage 4 directly from your computer keyboard.",
                    |ui| {
                        crate::panel::computer_kb_bar::show_bar(ui, &mut state.computer_kb);
                    },
                );
                ui.add_space(10.0);
                shadcn_card(
                    ui,
                    "MIDI Input & Audio Engine Meter",
                    "Live MIDI port selection and real-time audio callback health.",
                    |ui| {
                        if let Some(ports) = audio_handle.midi_ports() {
                            crate::audio::midi_ports::show(ui, ports);
                            ui.add_space(6.0);
                        }
                        crate::audio::meter_ui::show(ui, audio_handle.meter());
                    },
                );
            }
            DebugTab::Library => {
                // Accept .ns4b drag-and-drop directly onto the Settings window
                if let Some(path) = ctx.input(|i| {
                    i.raw.dropped_files.iter().find_map(|f| {
                        f.path.as_ref().filter(|p| {
                            p.extension()
                                .and_then(|e| e.to_str())
                                .is_some_and(|e| e.eq_ignore_ascii_case("ns4b"))
                        })
                    })
                    .cloned()
                }) {
                    if let Err(err) = library.open_backup_path(&path) {
                        library.error_message = Some(err);
                    }
                }
                shadcn_card(
                    ui,
                    "Nord Stage 4 Backup & Sample Browser (.ns4b)",
                    "Open a factory or user .ns4b backup and browse Programs, Pianos, and Samples directly here.",
                    |ui| {
                        library.render_browser_inline(ui, state, audio_handle);
                    },
                );
            }
            DebugTab::OsUpdate => {
                shadcn_card(
                    ui,
                    "Nord OS Update & Wave Tables",
                    "Drop or paste your Nord Stage 4 OS update (.zip / .exe / .dmg) below to install the 46 SHARC wave tables.",
                    |ui| {
                        os_update.show_inline(&ctx, ui);
                    },
                );
            }
            DebugTab::Guide => {
                shadcn_card(
                    ui,
                    "1. Factory Sound Bank (.ns4b)",
                    "Download the official Nord Stage 4 Factory Restore backup (~3.1 GB) to load factory programs, pianos, and samples.",
                    |ui| {
                        ui.horizontal_wrapped(|ui| {
                            let dl_btn = ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("Download Factory Bank v1.66 (.ns4b)")
                                            .size(11.5)
                                            .strong()
                                            .color(ZINC_50),
                                    )
                                    .fill(ACCENT_RED)
                                    .stroke(Stroke::NONE)
                                    .corner_radius(CornerRadius::same(6)),
                                )
                                .on_hover_cursor(egui::CursorIcon::PointingHand);
                            if dl_btn.clicked() {
                                ctx.open_url(egui::OpenUrl::new_tab(FACTORY_BANK_URL));
                            }
                            let open_btn = ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("Open .ns4b Library")
                                            .size(11.5)
                                            .color(ZINC_50),
                                    )
                                    .fill(ZINC_900)
                                    .stroke(Stroke::new(1.0, ZINC_800))
                                    .corner_radius(CornerRadius::same(6)),
                                )
                                .on_hover_cursor(egui::CursorIcon::PointingHand);
                            if open_btn.clicked() {
                                dlg.tab = DebugTab::Library;
                            }
                        });
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(
                                "After downloading, drag & drop the .ns4b file onto the keyboard or load it from the Library & Backup tab.",
                            )
                            .size(11.0)
                            .color(ZINC_400),
                        );
                    },
                );
                ui.add_space(10.0);
                shadcn_card(
                    ui,
                    "2. Nord Stage 4 OS Update (Wave Tables)",
                    "Download the official Nord Stage 4 OS v1.66 update (.zip) to enable the 46 hardware SHARC wave tables.",
                    |ui| {
                        ui.horizontal_wrapped(|ui| {
                            let dl_btn = ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("Download Nord Stage 4 OS v1.66 (.zip)")
                                            .size(11.5)
                                            .strong()
                                            .color(ZINC_50),
                                    )
                                    .fill(ACCENT_RED)
                                    .stroke(Stroke::NONE)
                                    .corner_radius(CornerRadius::same(6)),
                                )
                                .on_hover_cursor(egui::CursorIcon::PointingHand);
                            if dl_btn.clicked() {
                                ctx.open_url(egui::OpenUrl::new_tab(OS_UPDATE_URL));
                            }
                            let go_btn = ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("Open OS & Wave Tables Tab")
                                            .size(11.5)
                                            .color(ZINC_50),
                                    )
                                    .fill(ZINC_900)
                                    .stroke(Stroke::new(1.0, ZINC_800))
                                    .corner_radius(CornerRadius::same(6)),
                                )
                                .on_hover_cursor(egui::CursorIcon::PointingHand);
                            if go_btn.clicked() {
                                dlg.tab = DebugTab::OsUpdate;
                            }
                        });
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(
                                "Extract the .zip and select the Nord OS update file inside the OS & Wave Tables tab (saved automatically to your config).",
                            )
                            .size(11.0)
                            .color(ZINC_400),
                        );
                    },
                );
                ui.add_space(10.0);
                shadcn_card(
                    ui,
                    "3. Quick Controls & Shortcuts",
                    "Mouse, keyboard, and window tips.",
                    |ui| {
                        for tip in [
                            "• Drag & drop .ns4b, .ns4p, .npno, or .nsmp4 files directly onto the instrument.",
                            "• Scroll mouse wheel over knobs, faders, drawbars, or the Mod Wheel for fine adjustments.",
                            "• Computer keyboard: A–K white keys, W/E/T/Y/U black keys, Z/X octave, C/V velocity, Space sustain.",
                            "• Drag the empty transparent top strip to move the window; drag edges/bottom-right corner to resize.",
                        ] {
                            ui.label(RichText::new(tip).size(11.0).color(ZINC_400));
                        }
                    },
                );
            }
        });
}

/// Render the separate-window shadcn-style Settings & Diagnostics dialog when `dlg.open` is true.
pub fn show_debug_dialog(
    ctx: &Context,
    dlg: &mut DebugDialogState,
    state: &mut Stage4PanelState,
    library: &mut BackupLibraryState,
    firmware: &mut FirmwarePanelState,
    os_update: &mut OsUpdatePanel,
    audio_handle: &AudioHandle,
) {
    if !dlg.open {
        return;
    }

    let viewport_id = ViewportId::from_hash_of("ns4_shadcn_debug_viewport");
    let builder = ViewportBuilder::default()
        .with_title("Nord Stage 4 — Studio Settings & Diagnostics")
        .with_decorations(false)
        .with_transparent(true)
        .with_inner_size([880.0, 520.0])
        .with_min_inner_size([600.0, 340.0]);

    ctx.show_viewport_immediate(viewport_id, builder, |vp_ctx, class| {
        if vp_ctx.input(|i| i.viewport().close_requested()) {
            dlg.open = false;
            return;
        }
        if dlg.focus_requested {
            dlg.focus_requested = false;
            vp_ctx.send_viewport_cmd(ViewportCommand::Focus);
        }
        let rounded_frame = Frame::NONE
            .fill(ZINC_950_GLASS)
            .stroke(Stroke::new(1.0, ZINC_800))
            .corner_radius(CornerRadius::same(12))
            .inner_margin(Margin::same(16));

        if class == ViewportClass::Embedded {
            let mut open = dlg.open;
            egui::Window::new("Studio Settings & Diagnostics")
                .id(egui::Id::new("ns4_shadcn_debug_embedded"))
                .open(&mut open)
                .title_bar(false)
                .frame(rounded_frame)
                .show(vp_ctx, |ui| {
                    render_dialog_contents(
                        ui,
                        dlg,
                        state,
                        library,
                        firmware,
                        os_update,
                        audio_handle,
                    );
                });
            if !open {
                dlg.open = false;
            }
        } else {
            apply_shadcn_theme(vp_ctx);
            let flush_glass = Frame::NONE
                .fill(ZINC_950_GLASS)
                .stroke(Stroke::new(1.0, ZINC_800))
                .corner_radius(CornerRadius::ZERO)
                .inner_margin(Margin::same(16));
            egui::CentralPanel::default()
                .frame(flush_glass)
                .show(vp_ctx, |ui| {
                    render_dialog_contents(
                        ui,
                        dlg,
                        state,
                        library,
                        firmware,
                        os_update,
                        audio_handle,
                    );
                });

            // Bottom-right corner resize handle for the borderless settings window
            let screen = vp_ctx.screen_rect();
            let corner = egui::Rect::from_min_max(
                egui::pos2(screen.right() - 16.0, screen.bottom() - 16.0),
                screen.right_bottom(),
            );
            let (hovered, pressed) = vp_ctx.input(|i| {
                let h = i.pointer.hover_pos().is_some_and(|p| corner.contains(p));
                (h, h && i.pointer.primary_pressed())
            });
            if hovered {
                vp_ctx.set_cursor_icon(egui::CursorIcon::ResizeNwSe);
            }
            if pressed {
                vp_ctx.send_viewport_cmd(ViewportCommand::BeginResize(
                    egui::ResizeDirection::SouthEast,
                ));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_dialog_renders_all_tabs_without_panics() {
        let ctx = Context::default();
        let (audio, _rx) = AudioHandle::new();
        let mut state = Stage4PanelState::default();
        let mut library = BackupLibraryState::new();
        let mut firmware = FirmwarePanelState::new(Default::default());
        let mut os_update = OsUpdatePanel::new(None, None);
        let mut dlg = DebugDialogState::default();
        dlg.open_tab(DebugTab::Firmware);
        assert!(dlg.focus_requested);
        for tab in DebugTab::ALL {
            dlg.tab = tab;
            let _ = ctx.run(Default::default(), |ctx| {
                show_debug_dialog(
                    ctx,
                    &mut dlg,
                    &mut state,
                    &mut library,
                    &mut firmware,
                    &mut os_update,
                    &audio,
                );
            });
        }
    }
}
