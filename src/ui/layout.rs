//! Main IDE layout that assembles the toolbar, explorer, editor, and console.

use crate::app::{AppState, LogLevel, PaletteMode, SideBarView};
use crate::engine::apk::ApkProcessor;
use crate::ui::activity_bar::ActivityBar;
use crate::ui::command_palette::CommandPalette;
use crate::ui::editor::EditorPanel;
use crate::ui::file_tree::FileTreePanel;
use crate::ui::panels::ConsolePanel;
use crate::ui::theme::Theme;
use crate::ui::toolbar::Toolbar;

use std::sync::{Arc, Mutex};

use egui::{CornerRadius, Margin, RichText, Sense, Stroke};

/// Orchestrates the full IDE window layout.
pub struct IdeLayout {
    toolbar: Toolbar,
    file_tree: FileTreePanel,
    editor: EditorPanel,
    console: ConsolePanel,
    command_palette: CommandPalette,
}

impl IdeLayout {
    pub fn new() -> Self {
        Self {
            toolbar: Toolbar::new(),
            file_tree: FileTreePanel::new(),
            editor: EditorPanel::new(),
            console: ConsolePanel::new(),
            command_palette: CommandPalette::new(),
        }
    }

    fn run_decoded_report_async<F>(
        state: &Arc<Mutex<AppState>>,
        title: &'static str,
        start_status: &'static str,
        start_log: &'static str,
        work: F,
    ) where
        F: FnOnce(std::path::PathBuf) -> anyhow::Result<(Vec<String>, String)> + Send + 'static,
    {
        let decoded = {
            let s = state.lock().unwrap();
            s.workspace.decoded_dir()
        };

        let Some(decoded) = decoded else {
            let mut s = state.lock().unwrap();
            s.push_log(
                LogLevel::Error,
                "[AppStudio] Action failed: No decoded workspace found. Decode APK first.",
            );
            s.status_message = "App Studio: No decoded workspace found. Decode APK first.".into();
            return;
        };

        {
            let mut s = state.lock().unwrap();
            if s.busy {
                s.status_message = "App Studio: another action is already running".into();
                s.push_log(
                    LogLevel::Warn,
                    "[AppStudio] Ignored action because another operation is already running.",
                );
                return;
            }
            s.busy = true;
            s.status_message = start_status.into();
            s.push_log(LogLevel::Info, start_log);
        }

        let state_c = Arc::clone(state);
        std::thread::spawn(move || {
            let result = work(decoded);
            let mut s = state_c.lock().unwrap();
            match result {
                Ok((lines, completion_log)) => {
                    s.app_studio_set_report(title, &lines);
                    s.status_message = "App Studio action completed - check report below".into();
                    s.push_log(LogLevel::Info, &completion_log);
                    s.busy = false;
                }
                Err(err) => {
                    s.status_message = format!("App Studio: {err}");
                    s.push_log(LogLevel::Error, &format!("[AppStudio] Action failed: {err}"));
                    s.busy = false;
                }
            }
        });
    }

    fn begin_app_studio_action(
        state: &Arc<Mutex<AppState>>,
        status: impl Into<String>,
        log: impl AsRef<str>,
    ) -> bool {
        let mut s = state.lock().unwrap();
        if s.busy {
            s.status_message = "App Studio: another action is already running".into();
            s.push_log(
                LogLevel::Warn,
                "[AppStudio] Ignored action because another operation is already running.",
            );
            return false;
        }
        s.busy = true;
        s.status_message = status.into();
        s.push_log(LogLevel::Info, log.as_ref());
        true
    }

    pub fn render(
        &mut self,
        ctx: &egui::Context,
        state: &Arc<Mutex<AppState>>,
        rt: &tokio::runtime::Runtime,
    ) {
        let settings = { state.lock().unwrap().settings.clone() };
        let mut theme = if settings.dark_mode { Theme::dark() } else { Theme::light() };
        theme.font_code = settings.editor_font_size;
        theme.font_ui = settings.ui_font_size;
        theme.font_small = (settings.ui_font_size - 1.5).max(9.0);
        theme.font_heading = (settings.ui_font_size + 1.0).max(12.0);
        theme.apply(ctx);
        if self.console.is_visible() != settings.show_bottom_panel {
            self.console.panel_visible = settings.show_bottom_panel;
        }

        // Global keyboard shortcuts
        ctx.input(|i| {
            if i.key_pressed(egui::Key::P) && i.modifiers.ctrl && i.modifiers.shift {
                let mut s = state.lock().unwrap();
                s.show_command_palette = !s.show_command_palette;
                s.palette_mode = PaletteMode::Commands;
            }
            if i.key_pressed(egui::Key::P) && i.modifiers.ctrl && !i.modifiers.shift {
                let mut s = state.lock().unwrap();
                s.show_command_palette = !s.show_command_palette;
                s.palette_mode = PaletteMode::Files;
            }
            // Ctrl+J to toggle bottom panel (VS Code style)
            if i.key_pressed(egui::Key::J) && i.modifiers.ctrl {
                self.console.toggle_visibility();
                let mut s = state.lock().unwrap();
                s.settings.show_bottom_panel = self.console.is_visible();
                s.save_settings();
            }
        });

        // Premium VSCode-like shortcuts for bottom panel
        ctx.input(|i| {
            // Ctrl+` to toggle terminal tab
            if i.key_pressed(egui::Key::Backtick) && i.modifiers.ctrl {
                self.console.switch_to_terminal();
                let mut s = state.lock().unwrap();
                s.settings.show_bottom_panel = true;
                s.save_settings();
            }
            // Ctrl+Shift+U to toggle output tab
            if i.key_pressed(egui::Key::U) && i.modifiers.ctrl && i.modifiers.shift {
                self.console.switch_to_output();
                let mut s = state.lock().unwrap();
                s.settings.show_bottom_panel = true;
                s.save_settings();
            }
            // Ctrl+Shift+M to toggle problems tab
            if i.key_pressed(egui::Key::M) && i.modifiers.ctrl && i.modifiers.shift {
                self.console.switch_to_problems();
                let mut s = state.lock().unwrap();
                s.settings.show_bottom_panel = true;
                s.save_settings();
            }
        });

        let (busy, status_message, active_tab_details) = {
            let s = state.lock().unwrap();
            let active_tab = s.active_tab.and_then(|idx| {
                s.open_tabs
                    .get(idx)
                    .map(|tab| (tab.language.label().to_string(), tab.path.display().to_string()))
            });
            (s.busy, s.status_message.clone(), active_tab)
        };

        if busy {
            ctx.request_repaint();
        }

        // ── Top navigation / command center ──
        egui::TopBottomPanel::top("workbench_nav_panel")
            .exact_height(30.0)
            .frame(
                egui::Frame::NONE
                    .fill(theme.bg_secondary)
                    .stroke(egui::Stroke::new(1.0, theme.separator))
                    .inner_margin(egui::Margin::symmetric(8, 0)),
            )
            .show(ctx, |ui| {
                self.render_navbar(ui, state, rt, &theme);
            });

        // ── Toolbar ──
        egui::TopBottomPanel::top("toolbar_panel")
            .exact_height(36.0)
            .frame(egui::Frame::NONE.fill(theme.bg_tertiary)
                .stroke(egui::Stroke::new(1.0, theme.separator))
                .inner_margin(egui::Margin::symmetric(10, 0)))
            .show(ctx, |ui| {
                self.toolbar.render(ui, state, rt);
            });

        // ── Busy bar ──
        if busy {
            egui::TopBottomPanel::top("busy_bar")
                .exact_height(3.0)
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| {
                    Self::render_busy_bar(ui, &theme);
                });
        }

        // ── Status bar ──
        egui::TopBottomPanel::bottom("status_bar")
            .exact_height(22.0)
            .frame(egui::Frame::NONE
                .fill(if busy { theme.status_bar_busy } else { theme.status_bar_bg })
                .inner_margin(egui::Margin::symmetric(8, 0)))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = 12.0;

                    // Left side
                    if busy {
                        ui.add(egui::Spinner::new().size(12.0));
                    }
                    ui.label(
                        egui::RichText::new(&status_message)
                            .size(theme.font_small)
                            .color(theme.status_bar_text),
                    );

                    // Right side
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some((language, path)) = &active_tab_details {
                            let sep = |ui: &mut egui::Ui| {
                                ui.label(
                                    egui::RichText::new("|")
                                        .size(theme.font_small)
                                        .color(egui::Color32::from_white_alpha(80)),
                                );
                            };
                            ui.label(egui::RichText::new(language).size(theme.font_small).color(theme.status_bar_text));
                            sep(ui);
                            ui.label(egui::RichText::new("UTF-8").size(theme.font_small).color(theme.status_bar_text));
                            sep(ui);
                            let short_path = if path.len() > 60 {
                                format!("...{}", Self::truncate_start_chars(&path, 57))
                            } else {
                                path.clone()
                            };
                            ui.label(egui::RichText::new(&short_path).size(theme.font_small).color(egui::Color32::from_white_alpha(180)));
                        }
                    });
                });
            });

        // ── Console/Terminal/Problems panel (unified bottom panel) ──
        if self.console.is_visible() {
            egui::TopBottomPanel::bottom("console_panel")
                .resizable(true)
                .default_height(250.0)
                .min_height(150.0)
                .frame(egui::Frame::NONE
                    .fill(theme.bg_tertiary)
                    .inner_margin(egui::Margin::ZERO))
                .show(ctx, |ui| {
                    self.console.render(ui, state);
                });
        }

        // ── Activity bar (far left) ──
        egui::SidePanel::left("activity_bar_panel")
            .exact_width(48.0)
            .resizable(false)
            .frame(egui::Frame::NONE
                .fill(theme.activity_bar_bg)
                .inner_margin(egui::Margin::ZERO))
            .show(ctx, |ui| {
                ui.set_min_width(ui.available_width());
                ActivityBar::render(ui, state);
            });

        // ── Left side panel (content depends on sidebar_view) ──
        let sidebar_view = { state.lock().unwrap().sidebar_view.clone() };

        egui::SidePanel::left("file_tree_panel")
            .resizable(true)
            .show_separator_line(false)
            .default_width(260.0)
            .min_width(160.0)
            .max_width(360.0)
                .frame(egui::Frame::NONE
                    .fill(theme.bg_secondary)
                    .inner_margin(egui::Margin::ZERO))
            .show(ctx, |ui| {
                ui.set_min_width(ui.available_width());
                match sidebar_view {
                    SideBarView::Explorer => self.file_tree.render(ui, state),
                    SideBarView::Search => Self::render_search_panel(ui, state, &theme),
                    SideBarView::NativeAnalysis => Self::render_native_panel_with_state(ui, state, &theme),
                    SideBarView::Runtime => Self::render_runtime_panel(ui, state, &theme),
                    SideBarView::Strings => Self::render_strings_panel(ui, state, &theme),
                    SideBarView::AppStudio => Self::render_app_studio_panel(ui, state, &theme),
                    SideBarView::Settings => Self::render_settings_panel(ui, state, &theme),
                }
            });

        let has_outline = {
            let s = state.lock().unwrap();
            s.active_tab
                .and_then(|idx| s.open_tabs.get(idx))
                .is_some_and(|tab| matches!(tab.content, crate::app::TabContent::Code(_)))
        };
        if has_outline {
            egui::SidePanel::right("outline_panel")
                .resizable(true)
                .show_separator_line(false)
                .default_width(180.0)
                .min_width(120.0)
                .max_width(360.0)
                .frame(egui::Frame::NONE
                    .fill(theme.bg_secondary)
                    .inner_margin(egui::Margin::symmetric(8, 6)))
                .show(ctx, |ui| {
                    ui.set_min_width(ui.available_width());
                    Self::render_outline(ui, state);
                });
        }

        // ── Editor (central) ──
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE
                .fill(theme.editor_bg)
                .inner_margin(egui::Margin::ZERO))
            .show(ctx, |ui| {
                self.editor.render(ui, state);
            });

        // ── Help modal ──
        let mut show_help = { state.lock().unwrap().show_help };
        if show_help {
            egui::Window::new("RevEng-IDE Tips")
                .open(&mut show_help)
                .resizable(false)
                .collapsible(false)
                .frame(egui::Frame::window(&ctx.style())
                    .fill(theme.bg_elevated)
                    .corner_radius(theme.corner_radius as u8)
                    .inner_margin(16.0))
                .show(ctx, |ui| {
                    let t = Theme::current(ui);
                    ui.set_max_width(420.0);
                    ui.heading(egui::RichText::new("Quick Start").color(t.text_accent).size(t.font_heading));
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("1. Open an APK using the toolbar.").color(t.text_primary));
                    ui.label(egui::RichText::new("2. Click Decode to extract Smali + resources.").color(t.text_primary));
                    ui.label(egui::RichText::new("3. Click Decompile to get Java source.").color(t.text_primary));
                    ui.add_space(12.0);
                    ui.heading(egui::RichText::new("Shortcuts").color(t.text_accent).size(t.font_heading));
                    ui.add_space(4.0);
                    egui::Grid::new("shortcuts_grid").spacing([20.0, 6.0]).show(ui, |ui| {
                        let key = |s: &str| egui::RichText::new(s).color(t.accent_primary).strong().size(t.font_small);
                        let desc = |s: &str| egui::RichText::new(s).color(t.text_secondary).size(t.font_small);
                        ui.label(key("Ctrl+O")); ui.label(desc("Open APK")); ui.end_row();
                        ui.label(key("Ctrl+S")); ui.label(desc("Save current file")); ui.end_row();
                        ui.label(key("Ctrl+J")); ui.label(desc("Jump Java -> Smali")); ui.end_row();
                        ui.label(key("Ctrl+K")); ui.label(desc("Jump Smali -> Java")); ui.end_row();
                        ui.label(key("Ctrl+F")); ui.label(desc("Search in file")); ui.end_row();
                        ui.label(key("Shift+F12")); ui.label(desc("Find Usages (Smali)")); ui.end_row();
                        ui.label(key("Ctrl+P")); ui.label(desc("Quick Open File")); ui.end_row();
                        ui.label(key("Ctrl+Shift+P")); ui.label(desc("Command Palette")); ui.end_row();
                    });
                });
            state.lock().unwrap().show_help = show_help;
        }

        Self::render_tool_update_prompt(ctx, state, &theme);

        // ── Command Palette (rendered last, on top of everything) ──
        self.command_palette.render(ctx, state);
    }

    fn render_navbar(
        &mut self,
        ui: &mut egui::Ui,
        state: &Arc<Mutex<AppState>>,
        rt: &tokio::runtime::Runtime,
        theme: &Theme,
    ) {
        let (workspace_name, tab_count, busy) = {
            let s = state.lock().unwrap();
            let workspace_name = s
                .workspace
                .root_dir()
                .and_then(|root| root.file_name())
                .and_then(|name| name.to_str())
                .map(|name| name.to_string())
                .unwrap_or_else(|| "No workspace".to_string());
            (workspace_name, s.open_tabs.len(), s.busy)
        };

        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;

            ui.label(
                egui::RichText::new("RevEng IDE")
                    .size(theme.font_ui)
                    .strong()
                    .color(theme.text_primary),
            );

            Self::nav_separator(ui, theme);

            ui.menu_button(
                egui::RichText::new("File").size(theme.font_small).color(theme.text_secondary),
                |ui| {
                    if ui
                        .add_enabled(!busy, egui::Button::new("Open APK..."))
                        .on_hover_text("Open APK or XAPK (Ctrl+O)")
                        .clicked()
                    {
                        ui.close_menu();
                        Self::open_apk_from_dialog(state, rt);
                    }
                    if ui
                        .add_enabled(tab_count > 0, egui::Button::new("Save Active File"))
                        .on_hover_text("Save active editor tab (Ctrl+S)")
                        .clicked()
                    {
                        ui.close_menu();
                        let _ = state.lock().unwrap().save_active_tab();
                    }
                    if ui
                        .add_enabled(!busy, egui::Button::new("Open Project..."))
                        .on_hover_text("Open a saved RevEng IDE project")
                        .clicked()
                    {
                        ui.close_menu();
                        Self::open_project_from_dialog(state);
                    }
                    if ui
                        .add_enabled(tab_count > 0, egui::Button::new("Save Project As..."))
                        .on_hover_text("Save workspace state as a project file")
                        .clicked()
                    {
                        ui.close_menu();
                        Self::save_project_from_dialog(state);
                    }
                    if ui
                        .add_enabled(tab_count > 0, egui::Button::new("Close Active Tab"))
                        .clicked()
                    {
                        ui.close_menu();
                        let mut s = state.lock().unwrap();
                        if let Some(idx) = s.active_tab {
                            s.close_tab(idx);
                        }
                    }
                },
            );

            ui.menu_button(
                egui::RichText::new("View").size(theme.font_small).color(theme.text_secondary),
                |ui| {
                    Self::sidebar_menu_item(ui, state, "Explorer", SideBarView::Explorer);
                    Self::sidebar_menu_item(ui, state, "Search", SideBarView::Search);
                    Self::sidebar_menu_item(ui, state, "Native Analysis", SideBarView::NativeAnalysis);
                    Self::sidebar_menu_item(ui, state, "Strings", SideBarView::Strings);
                    Self::sidebar_menu_item(ui, state, "Runtime / ADB", SideBarView::Runtime);
                    Self::sidebar_menu_item(ui, state, "Settings", SideBarView::Settings);
                    ui.separator();
                    if ui.button("Toggle Bottom Panel").on_hover_text("Ctrl+J").clicked() {
                        ui.close_menu();
                        self.console.toggle_visibility();
                    }
                    if ui.button("Terminal").on_hover_text("Ctrl+`").clicked() {
                        ui.close_menu();
                        self.console.switch_to_terminal();
                        let mut s = state.lock().unwrap();
                        s.settings.show_bottom_panel = true;
                        s.save_settings();
                    }
                    if ui.button("Output").on_hover_text("Ctrl+Shift+U").clicked() {
                        ui.close_menu();
                        self.console.switch_to_output();
                        let mut s = state.lock().unwrap();
                        s.settings.show_bottom_panel = true;
                        s.save_settings();
                    }
                    if ui.button("Problems").on_hover_text("Ctrl+Shift+M").clicked() {
                        ui.close_menu();
                        self.console.switch_to_problems();
                        let mut s = state.lock().unwrap();
                        s.settings.show_bottom_panel = true;
                        s.save_settings();
                    }
                },
            );

            ui.menu_button(
                egui::RichText::new("Go").size(theme.font_small).color(theme.text_secondary),
                |ui| {
                    if ui.button("Quick Open File").on_hover_text("Ctrl+P").clicked() {
                        ui.close_menu();
                        let mut s = state.lock().unwrap();
                        s.show_command_palette = true;
                        s.palette_mode = PaletteMode::Files;
                    }
                    if ui.button("Command Palette").on_hover_text("Ctrl+Shift+P").clicked() {
                        ui.close_menu();
                        let mut s = state.lock().unwrap();
                        s.show_command_palette = true;
                        s.palette_mode = PaletteMode::Commands;
                    }
                },
            );

            ui.menu_button(
                egui::RichText::new("Tools").size(theme.font_small).color(theme.text_secondary),
                |ui| {
                    if ui.add_enabled(!busy, egui::Button::new("Decode APK")).clicked() {
                        ui.close_menu();
                        Self::run_apk_task(state, rt, ApkProcessor::decode_apk, "Decode failed");
                    }
                    if ui.add_enabled(!busy, egui::Button::new("Decompile APK")).clicked() {
                        ui.close_menu();
                        Self::run_apk_task(state, rt, ApkProcessor::decompile_apk, "Decompile failed");
                    }
                    if ui.add_enabled(!busy, egui::Button::new("Build APK")).clicked() {
                        ui.close_menu();
                        Self::run_apk_task(state, rt, ApkProcessor::build_apk, "Build failed");
                    }
                    if ui.add_enabled(!busy, egui::Button::new("Sign APK")).clicked() {
                        ui.close_menu();
                        Self::run_apk_task(state, rt, ApkProcessor::sign_apk, "Sign failed");
                    }
                    ui.separator();
                    if ui.button("Verify Toolchain").clicked() {
                        ui.close_menu();
                        Self::verify_toolchain(state);
                    }
                },
            );

            ui.menu_button(
                egui::RichText::new("Help").size(theme.font_small).color(theme.text_secondary),
                |ui| {
                    if ui.button("Quick Start & Shortcuts").clicked() {
                        ui.close_menu();
                        state.lock().unwrap().show_help = true;
                    }
                },
            );

            ui.add_space(8.0);

            let command_center = egui::Button::new(
                egui::RichText::new("Command Center")
                    .size(theme.font_small)
                    .color(theme.text_muted),
            )
            .frame(false)
            .min_size(egui::vec2(220.0, 24.0));
            let response = ui.add(command_center).on_hover_text("Open Command Palette");
            if response.hovered() {
                ui.painter().rect_stroke(
                    response.rect.expand2(egui::vec2(4.0, 1.0)),
                    6.0,
                    egui::Stroke::new(1.0, theme.border),
                    egui::StrokeKind::Inside,
                );
            }
            if response.clicked() {
                let mut s = state.lock().unwrap();
                s.show_command_palette = true;
                s.palette_mode = PaletteMode::Commands;
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format!("{} tabs", tab_count))
                        .size(theme.font_small)
                        .color(theme.text_muted),
                );
                ui.label(
                    egui::RichText::new(workspace_name)
                        .size(theme.font_small)
                        .color(theme.text_secondary),
                );
            });
        });
    }

    fn nav_separator(ui: &mut egui::Ui, theme: &Theme) {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(1.0, 16.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 0.0, theme.separator);
    }

    fn sidebar_menu_item(
        ui: &mut egui::Ui,
        state: &Arc<Mutex<AppState>>,
        label: &str,
        view: SideBarView,
    ) {
        let active = { state.lock().unwrap().sidebar_view == view };
        if ui.selectable_label(active, label).clicked() {
            ui.close_menu();
            state.lock().unwrap().sidebar_view = view;
        }
    }

    fn open_apk_from_dialog(state: &Arc<Mutex<AppState>>, rt: &tokio::runtime::Runtime) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("APK / XAPK Files", &["apk", "xapk"])
            .add_filter("All Files", &["*"])
            .pick_file()
        {
            let state_c = Arc::clone(state);
            rt.spawn(async move {
                let state_blocking = Arc::clone(&state_c);
                let result =
                    tokio::task::spawn_blocking(move || ApkProcessor::open_apk(&state_blocking, &path))
                        .await;
                match result {
                    Ok(Ok(())) => {}
                    Ok(Err(err)) => {
                        let mut s = state_c.lock().unwrap();
                        s.push_log(LogLevel::Error, &format!("APK open failed: {}", err));
                        s.status_message = "APK open failed".into();
                        s.busy = false;
                    }
                    Err(err) => {
                        log::error!("Task join error: {}", err);
                    }
                }
            });
        }
    }

    fn open_project_from_dialog(state: &Arc<Mutex<AppState>>) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("RevEng Project", &["json"])
            .add_filter("All Files", &["*"])
            .pick_file()
        {
            match state.lock().unwrap().load_project_from_path(&path) {
                Ok(()) => {
                    let mut s = state.lock().unwrap();
                    s.current_project_path = Some(path.clone());
                    s.status_message = format!("Project loaded: {}", path.display());
                    s.push_log(LogLevel::Info, &format!("Loaded project: {}", path.display()));
                }
                Err(e) => {
                    let mut s = state.lock().unwrap();
                    s.push_log(LogLevel::Error, &format!("Project load failed: {}", e));
                    s.status_message = "Project load failed".into();
                }
            }
        }
    }

    fn save_project_from_dialog(state: &Arc<Mutex<AppState>>) {
        let initial = { state.lock().unwrap().current_project_path.clone() };
        let mut dialog = rfd::FileDialog::new()
            .add_filter("RevEng Project", &["json"])
            .add_filter("All Files", &["*"]);
        if let Some(path) = initial.as_ref() {
            if let Some(parent) = path.parent() {
                dialog = dialog.set_directory(parent);
            }
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                dialog = dialog.set_file_name(name);
            }
        }
        if let Some(path) = dialog.save_file() {
            match state.lock().unwrap().save_project_to_path(&path) {
                Ok(()) => {
                    let mut s = state.lock().unwrap();
                    s.current_project_path = Some(path.clone());
                    s.status_message = format!("Project saved: {}", path.display());
                    s.push_log(LogLevel::Info, &format!("Saved project: {}", path.display()));
                }
                Err(e) => {
                    let mut s = state.lock().unwrap();
                    s.push_log(LogLevel::Error, &format!("Project save failed: {}", e));
                    s.status_message = "Project save failed".into();
                }
            }
        }
    }

    fn run_apk_task(
        state: &Arc<Mutex<AppState>>,
        rt: &tokio::runtime::Runtime,
        task: fn(&Arc<Mutex<AppState>>) -> anyhow::Result<()>,
        failure: &'static str,
    ) {
        let state_c = Arc::clone(state);
        rt.spawn(async move {
            let state_blocking = Arc::clone(&state_c);
            let result = tokio::task::spawn_blocking(move || task(&state_blocking)).await;
            if let Ok(Err(err)) = result {
                let mut s = state_c.lock().unwrap();
                s.push_log(LogLevel::Error, &format!("{}: {}", failure, err));
                s.busy = false;
            }
        });
    }

    fn verify_toolchain(state: &Arc<Mutex<AppState>>) {
        let mut s = state.lock().unwrap();
        let results = s.toolchain.verify_all();
        for (tool, ok) in results {
            if ok {
                s.push_log(LogLevel::Info, &format!("Tool found: {}", tool));
            } else {
                let required = s
                    .toolchain
                    .get(&tool)
                    .map(|info| info.required == crate::engine::toolchain::ToolRequirement::Required)
                    .unwrap_or(true);
                let level = if required { LogLevel::Warn } else { LogLevel::Info };
                let label = if required { "Tool missing" } else { "Optional tool missing" };
                s.push_log(level, &format!("{}: {}", label, tool));
            }
            if let Some(tip) = crate::engine::toolchain::ToolchainManager::get_tool_tip(&tool) {
                s.push_log(LogLevel::Debug, tip);
            }
        }
    }

    fn render_outline(ui: &mut egui::Ui, state: &Arc<Mutex<AppState>>) {
        let t = Theme::current(ui);
        ui.heading(
            egui::RichText::new("Outline")
                .size(t.font_heading)
                .color(t.text_accent),
        );
        ui.separator();

        let s = state.lock().unwrap();
        if let Some(idx) = s.active_tab {
            if let Some(tab) = s.open_tabs.get(idx) {
                if let crate::app::TabContent::Code(content) = &tab.content {
                    let symbols = crate::engine::symbols::SymbolParser::parse(content, &tab.language);
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for sym in symbols {
                            ui.horizontal(|ui| {
                                let icon_color = match sym.kind {
                                    crate::engine::symbols::SymbolKind::Class => t.syn_type,
                                    crate::engine::symbols::SymbolKind::Method => t.syn_function,
                                    crate::engine::symbols::SymbolKind::Field => t.syn_string,
                                    crate::engine::symbols::SymbolKind::Annotation => t.syn_comment,
                                };
                                ui.label(egui::RichText::new(sym.kind.icon()).color(icon_color));
                                ui.label(egui::RichText::new(&sym.name).color(t.text_primary))
                                    .on_hover_text(format!("Line {}", sym.line));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(
                                        egui::RichText::new(format!("{}", sym.line))
                                            .size(t.font_small - 1.0)
                                            .color(t.text_muted),
                                    );
                                });
                            });
                        }
                    });
                }
            }
        }
    }

    fn render_sidebar_header(
        ui: &mut egui::Ui,
        theme: &Theme,
        title: &str,
        subtitle: &str,
        stat: Option<(&str, egui::Color32)>,
    ) {
        egui::Frame::NONE
            .inner_margin(egui::Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(title.to_uppercase())
                            .size(11.0)
                            .strong()
                            .color(theme.text_primary),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some((value, color)) = stat {
                            let badge = egui::Frame::NONE
                                .fill(color.linear_multiply(0.12))
                                .corner_radius(egui::CornerRadius::same(6))
                                .inner_margin(egui::Margin::symmetric(8, 2));
                            badge.show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new(value)
                                        .size(10.0)
                                        .strong()
                                        .color(color),
                                );
                            });
                        }
                    });
                });
                if !subtitle.is_empty() {
                    ui.add_space(3.0);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(subtitle)
                                .size(11.0)
                                .color(theme.text_muted),
                        )
                        .wrap(),
                    );
                }
            });
    }

    fn render_sidebar_search_box(
        ui: &mut egui::Ui,
        theme: &Theme,
        icon: &str,
        query: &mut String,
        hint: &str,
    ) -> egui::Response {
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            ui.set_width((ui.available_width() - 12.0).max(80.0));
            egui::Frame::NONE
                .fill(theme.bg_input)
                .stroke(egui::Stroke::new(1.0, if query.is_empty() { theme.border_subtle } else { theme.accent_primary.linear_multiply(0.5) }))
                .corner_radius(egui::CornerRadius::same(4))
                .inner_margin(egui::Margin::symmetric(9, 5))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 7.0;
                        ui.label(
                            egui::RichText::new(icon)
                                .size(11.0)
                                .color(if query.is_empty() { theme.text_muted } else { theme.accent_primary }),
                        );
                        let response = ui.add(
                            egui::TextEdit::singleline(query)
                                .frame(false)
                                .hint_text(hint)
                                .desired_width(f32::INFINITY)
                                .font(egui::TextStyle::Small),
                        );

                        if !query.is_empty() {
                            let clear_btn = egui::Button::new(
                                egui::RichText::new("x").size(theme.font_small).color(theme.text_muted)
                            )
                            .frame(false)
                            .min_size(egui::vec2(16.0, 16.0));
                            
                            if ui.add(clear_btn).on_hover_text("Clear").clicked() {
                                query.clear();
                            }
                        }
                        response
                    })
                    .inner
                })
                .inner
        })
        .inner
    }

    fn render_sidebar_empty_state(
        ui: &mut egui::Ui,
        theme: &Theme,
        title: &str,
        subtitle: &str,
    ) {
        ui.add_space(18.0);
        egui::Frame::NONE
            .inner_margin(egui::Margin::symmetric(12, 0))
            .show(ui, |ui| {
            ui.label(
                egui::RichText::new(title)
                    .size(12.0)
                    .color(theme.text_primary),
            );
            ui.add_space(4.0);
            
            ui.add(
                egui::Label::new(
                    egui::RichText::new(subtitle)
                        .size(theme.font_small)
                        .color(theme.text_muted)
                )
                .wrap()
            );
            });
        ui.add_space(18.0);
    }

    fn truncate_start_chars(text: &str, max_chars: usize) -> String {
        let len = text.chars().count();
        text.chars().skip(len.saturating_sub(max_chars)).collect()
    }

    fn truncate_end_chars(text: &str, max_chars: usize) -> String {
        text.chars().take(max_chars).collect()
    }

    fn sidebar_row(
        ui: &mut egui::Ui,
        theme: &Theme,
        is_active: bool,
        base_fill: egui::Color32,
        height: f32,
        add_contents: impl FnOnce(&mut egui::Ui),
    ) -> egui::Response {
        let width = ui.available_width();
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());

        let fill = if is_active {
            theme.tree_selected_bg
        } else if response.hovered() {
            theme.tree_hover_bg
        } else {
            base_fill
        };

        // Smooth rounded corners for hover/active states
        let corner_radius = if is_active || response.hovered() { 4.0 } else { 0.0 };
        
        if fill != egui::Color32::TRANSPARENT {
            let inner_rect = rect.shrink2(egui::vec2(4.0, 1.0));
            ui.painter().rect_filled(inner_rect, corner_radius, fill);
        }

        // Left accent bar for active items
        if is_active {
            let accent_rect = egui::Rect::from_min_size(
                egui::pos2(rect.left(), rect.top() + 2.0),
                egui::vec2(3.0, rect.height() - 4.0),
            );
            ui.painter()
                .rect_filled(accent_rect, 1.5, theme.accent_primary);
        }

        let content_rect = rect.shrink2(egui::vec2(12.0, 4.0));
        let mut row_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(content_rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        add_contents(&mut row_ui);

        response
    }

    fn render_search_panel(ui: &mut egui::Ui, state: &Arc<Mutex<AppState>>, theme: &Theme) {
        let (result_count, xref_count) = {
            let s = state.lock().unwrap();
            (s.search_results.len(), s.xref_results.len())
        };
        let stat_label = if xref_count > 0 {
            format!("{} files / {} xrefs", result_count, xref_count)
        } else if result_count > 0 {
            format!("{} files", result_count)
        } else {
            "Workspace".to_string()
        };

        Self::render_sidebar_header(
            ui,
            theme,
            "Search",
            "Find text across Java, smali, and XML, then jump straight to the hit.",
            Some((stat_label.as_str(), theme.info)),
        );
        ui.add_space(8.0);

        let has_workspace = { state.lock().unwrap().workspace.root_dir().is_some() };
        if !has_workspace {
            Self::render_sidebar_empty_state(
                ui,
                theme,
                "No workspace open",
                "Open an APK before searching decoded Java, smali, and XML files.",
            );
            return;
        }

        let mut query_buf = { state.lock().unwrap().global_search_query.clone() };
        let submitted = Self::render_sidebar_search_box(
            ui,
            theme,
            "Search",
            &mut query_buf,
            "Search workspace...",
        )
        .lost_focus()
            && ui.input(|i| i.key_pressed(egui::Key::Enter));

        {
            let mut s = state.lock().unwrap();
            s.global_search_query = query_buf.clone();
        }

        if submitted && !query_buf.is_empty() {
            let root = { state.lock().unwrap().workspace.root_dir().map(|p| p.to_path_buf()) };
            if let Some(r) = root {
                let query = query_buf.clone();
                let state_c = Arc::clone(state);
                std::thread::spawn(move || {
                    let results = crate::engine::patch::PatchEngine::search_in_dir(&r, &query, &["java", "smali", "xml"]).unwrap_or_default();
                    let mut s_mut = state_c.lock().unwrap();
                    s_mut.search_results = results;
                    let count = s_mut.search_results.len();
                    s_mut.push_log(LogLevel::Info, &format!("Search: {} files matching '{}'", count, query));
                });
            }
        }

        ui.add_space(8.0);

        let (results, xref_results, workspace_root) = {
            let s = state.lock().unwrap();
            (
                s.search_results.clone(),
                s.xref_results.clone(),
                s.workspace.root_dir().map(|p| p.to_path_buf()),
            )
        };

        if query_buf.trim().is_empty() && results.is_empty() && xref_results.is_empty() {
            Self::render_sidebar_empty_state(
                ui,
                theme,
                "Search the workspace",
                "Type a query and press Enter to search Java, smali, and XML files.",
            );
            return;
        }

        if !results.is_empty() {
            egui::ScrollArea::vertical().show(ui, |ui| {
                for res in &results {
                    let ext = res.path.extension().and_then(|e| e.to_str()).unwrap_or("");
                    let (icon, icon_color) = theme.file_icon(ext);
                    let rel_path = workspace_root
                        .as_ref()
                        .and_then(|root| res.path.strip_prefix(root).ok())
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| res.path.display().to_string());

                    egui::Frame::NONE
                        .fill(theme.bg_elevated)
                        .stroke(egui::Stroke::new(1.0, theme.border_subtle))
                        .corner_radius(egui::CornerRadius::same((theme.corner_radius as u8) + 1))
                        .inner_margin(egui::Margin::symmetric(8, 8))
                        .show(ui, |ui| {
                            let open_file = Self::sidebar_row(
                                ui,
                                theme,
                                false,
                                egui::Color32::TRANSPARENT,
                                32.0,
                                |ui| {
                                    let badge = egui::Frame::NONE
                                        .fill(icon_color.linear_multiply(0.15))
                                        .corner_radius(egui::CornerRadius::same(4))
                                        .inner_margin(egui::Margin::symmetric(6, 2));
                                    badge.show(ui, |ui| {
                                        ui.label(
                                            egui::RichText::new(icon)
                                                .size(8.8)
                                                .strong()
                                                .color(icon_color),
                                        );
                                    });

                                    ui.vertical(|ui| {
                                        ui.spacing_mut().item_spacing.y = 0.0;
                                        ui.label(
                                            egui::RichText::new(
                                                res.path
                                                    .file_name()
                                                    .and_then(|n| n.to_str())
                                                    .unwrap_or("?"),
                                            )
                                            .size(11.8)
                                            .color(theme.text_primary),
                                        );
                                        ui.label(
                                            egui::RichText::new(&rel_path)
                                                .size(theme.font_small)
                                                .color(theme.text_muted),
                                        );
                                    });

                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            ui.label(
                                                egui::RichText::new(format!(
                                                    "{} matches",
                                                    res.matches.len()
                                                ))
                                                .size(theme.font_small)
                                                .color(theme.text_muted),
                                            );
                                        },
                                    );
                                },
                            );

                            if open_file.clicked() {
                                if let Some(first_match) = res.matches.first() {
                                    state
                                        .lock()
                                        .unwrap()
                                        .open_file_at_line(res.path.clone(), first_match.line_number, None);
                                } else {
                                    state.lock().unwrap().open_file(res.path.clone());
                                }
                            }

                            ui.add_space(4.0);

                            for matched in res.matches.iter().take(4) {
                                let snippet = matched.line_content.trim();
                                let snippet = if snippet.chars().count() > 110 {
                                    format!("{}...", Self::truncate_end_chars(snippet, 107))
                                } else {
                                    snippet.to_string()
                                };

                                let hit = Self::sidebar_row(
                                    ui,
                                    theme,
                                    false,
                                    theme.bg_secondary,
                                    28.0,
                                    |ui| {
                                        let line_badge = egui::Frame::NONE
                                            .fill(theme.accent_primary.linear_multiply(0.12))
                                            .corner_radius(egui::CornerRadius::same(4))
                                            .inner_margin(egui::Margin::symmetric(6, 2));
                                        line_badge.show(ui, |ui| {
                                            ui.label(
                                                egui::RichText::new(format!("L{}", matched.line_number))
                                                    .size(theme.font_small)
                                                    .color(theme.accent_primary)
                                                    .monospace(),
                                            );
                                        });
                                        ui.label(
                                            egui::RichText::new(snippet)
                                                .size(theme.font_small)
                                                .color(theme.text_secondary)
                                                .monospace(),
                                        );
                                    },
                                )
                                .on_hover_text(matched.line_content.clone());

                                if hit.clicked() {
                                    state
                                        .lock()
                                        .unwrap()
                                        .open_file_at_line(res.path.clone(), matched.line_number, None);
                                }
                            }
                        });

                    ui.add_space(6.0);
                }
            });
        } else if !query_buf.trim().is_empty() {
            Self::render_sidebar_empty_state(
                ui,
                theme,
                "No text matches",
                "That query did not match any Java, smali, or XML files in the current workspace.",
            );
        }

        if !xref_results.is_empty() {
            ui.add_space(8.0);
            let xref_stat = format!("{}", xref_results.len());
            Self::render_sidebar_header(
                ui,
                theme,
                "Cross-References",
                "Caller and usage results from the smali xref database.",
                Some((xref_stat.as_str(), theme.accent_primary)),
            );
            ui.add_space(6.0);

            egui::ScrollArea::vertical()
                .id_salt("xref_scroll")
                .show(ui, |ui| {
                for site in &xref_results {
                    let short_class = site.in_class.rsplit('/').next().unwrap_or(&site.in_class)
                        .trim_end_matches(';');
                    let label = format!("{}.{} (line {})", short_class, site.in_method, site.line);

                    let resp = Self::sidebar_row(
                        ui,
                        theme,
                        false,
                        theme.bg_elevated,
                        30.0,
                        |ui| {
                            let badge = egui::Frame::NONE
                                .fill(theme.accent_primary.linear_multiply(0.12))
                                .corner_radius(egui::CornerRadius::same(4))
                                .inner_margin(egui::Margin::symmetric(6, 2));
                            badge.show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new("XREF")
                                        .size(theme.font_small)
                                        .color(theme.accent_primary),
                                );
                            });
                            ui.label(
                                egui::RichText::new(&label)
                                    .size(theme.font_small)
                                    .color(theme.text_primary),
                            );
                        },
                    )
                    .on_hover_text(site.instruction.clone());

                    if resp.clicked() {
                        state
                            .lock()
                            .unwrap()
                            .open_file_at_line(site.file.clone(), site.line, None);
                    }
                }
            });
        }
    }

    fn render_native_panel_with_state(ui: &mut egui::Ui, state: &Arc<Mutex<AppState>>, theme: &Theme) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            Self::render_sidebar_header(
                ui,
                theme,
                "Analysis",
                "Manifest, DEX, native libraries, and other workspace intelligence.",
                Some(("Inspector", theme.info)),
            );
            ui.add_space(8.0);

            let manifest = { state.lock().unwrap().manifest_info.clone() };
            let dex = { state.lock().unwrap().dex_stats.clone() };
            let apkid = { state.lock().unwrap().apkid_results.clone() };
            let (is_flutter, flutter_ver, flutter_libs, libapp, native_libs) = {
                let s = state.lock().unwrap();
                (s.is_flutter_app, s.flutter_version.clone(), s.flutter_lib_paths.clone(), s.libapp_path.clone(), s.native_lib_paths.clone())
            };

            // Manifest
            if let Some(info) = manifest {
                egui::Frame::NONE
                    .fill(theme.bg_elevated)
                    .corner_radius(egui::CornerRadius::same(5))
                    .inner_margin(egui::Margin::symmetric(10, 8))
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new("Manifest").size(theme.font_small).strong().color(theme.text_accent));
                        ui.add_space(4.0);
                        egui::Grid::new("manifest_grid").spacing([10.0, 2.0]).show(ui, |ui| {
                            let kv = |ui: &mut egui::Ui, k: &str, v: &str, theme: &Theme| {
                                ui.label(egui::RichText::new(k).size(10.0).color(theme.text_muted));
                                ui.label(egui::RichText::new(v).size(10.0).color(theme.text_primary));
                                ui.end_row();
                            };
                            kv(ui, "Package", &info.package, theme);
                            kv(ui, "Version", &format!("{} ({})", info.version_name, info.version_code), theme);
                            kv(ui, "Min SDK", &info.min_sdk, theme);
                            kv(ui, "Target SDK", &info.target_sdk, theme);
                            kv(ui, "Debuggable", if info.debuggable { "YES" } else { "no" }, theme);
                            kv(ui, "Backup", if info.allow_backup { "YES" } else { "no" }, theme);
                            kv(ui, "Cleartext", if info.uses_cleartext { "YES" } else { "no" }, theme);
                        });

                        if !info.warnings.is_empty() {
                            ui.add_space(6.0);
                            ui.label(egui::RichText::new(format!("{} Warnings", info.warnings.len()))
                                .size(10.0).strong().color(theme.warning));
                            for w in &info.warnings {
                                ui.horizontal(|ui| {
                                    let sev_color = w.severity.color();
                                    ui.label(egui::RichText::new(match &w.severity {
                                        crate::engine::manifest::WarningSeverity::High => "HIGH",
                                        crate::engine::manifest::WarningSeverity::Medium => "MED",
                                        crate::engine::manifest::WarningSeverity::Low => "LOW",
                                    }).size(7.0).strong().color(sev_color));
                                    ui.label(egui::RichText::new(&w.message).size(theme.font_small).color(theme.text_primary));
                                });
                            }
                        }

                        if !info.permissions.is_empty() {
                            ui.add_space(6.0);
                            ui.label(egui::RichText::new(format!("{} Permissions", info.permissions.len()))
                                .size(10.0).strong().color(theme.text_accent));
                            for p in &info.permissions.iter().take(15).collect::<Vec<_>>() {
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new(p.risk.label()).size(7.0).color(p.risk.color()));
                                    ui.label(egui::RichText::new(&p.short_name).size(theme.font_small).color(theme.text_primary))
                                        .on_hover_text(&p.name);
                                });
                            }
                        }

                        if !info.deeplinks.is_empty() {
                            ui.add_space(6.0);
                            ui.label(egui::RichText::new(format!("{} Deeplinks", info.deeplinks.len()))
                                .size(10.0).strong().color(theme.text_accent));
                            for dl in &info.deeplinks {
                                ui.label(egui::RichText::new(dl).size(theme.font_small).color(egui::Color32::from_rgb(130, 170, 255)));
                            }
                        }

                        let exported = info.components.iter().filter(|c| c.exported).count();
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new(format!("{} Components ({} exported)", info.components.len(), exported))
                            .size(10.0).strong().color(theme.text_accent));
                        for component in info.components.iter().take(8) {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(component.name.rsplit('.').next().unwrap_or(&component.name))
                                    .size(theme.font_small).color(theme.text_primary))
                                    .on_hover_text(&component.name);
                                if component.exported {
                                    ui.label(egui::RichText::new("exported").size(9.0).color(egui::Color32::from_rgb(255, 180, 60)));
                                }
                            });
                        }
                    });
                ui.add_space(8.0);
            }

            // DEX Stats
            if let Some(stats) = dex {
                egui::Frame::NONE
                    .fill(theme.bg_elevated)
                    .corner_radius(egui::CornerRadius::same(5))
                    .inner_margin(egui::Margin::symmetric(10, 8))
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new("DEX Statistics").size(theme.font_small).strong().color(theme.text_accent));
                        ui.add_space(4.0);
                        egui::Grid::new("dex_grid").spacing([10.0, 2.0]).show(ui, |ui| {
                            let kv = |ui: &mut egui::Ui, k: &str, v: &str, theme: &Theme| {
                                ui.label(egui::RichText::new(k).size(10.0).color(theme.text_muted));
                                ui.label(egui::RichText::new(v).size(10.0).color(theme.text_primary));
                                ui.end_row();
                            };
                            kv(ui, "Classes", &format!("{}", stats.total_classes), theme);
                            kv(ui, "Methods", &format!("{}", stats.total_methods), theme);
                            kv(ui, "Fields", &format!("{}", stats.total_fields), theme);
                            kv(ui, "Obfuscation", &format!("{:.0}%", stats.obfuscation_score), theme);
                        });

                        let pct = stats.max_method_pct();
                        let gauge_color = if pct > 90.0 { theme.error }
                            else if pct > 70.0 { theme.warning }
                            else { theme.success };
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new(format!("64K limit: {:.1}%", pct)).size(10.0).color(gauge_color));
                        let (bar_rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width().min(200.0), 5.0), egui::Sense::hover());
                        ui.painter().rect_filled(bar_rect, 2.5, theme.bg_tertiary);
                        let w = bar_rect.width() * (pct / 100.0).min(1.0);
                        ui.painter().rect_filled(egui::Rect::from_min_size(bar_rect.min, egui::vec2(w, 5.0)), 2.5, gauge_color);
                    });
                ui.add_space(8.0);
            }

            // APKiD
            egui::Frame::NONE
                .fill(theme.bg_elevated)
                .corner_radius(egui::CornerRadius::same(5))
                .inner_margin(egui::Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("APKiD").size(theme.font_small).strong().color(theme.text_accent));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("Run").on_hover_text("Identify packers, obfuscators").clicked() {
                                if let Some(apk_path) = { state.lock().unwrap().workspace.apk_path().map(|p| p.to_path_buf()) } {
                                    let sc = Arc::clone(state);
                                    std::thread::spawn(move || {
                                        let _ = crate::engine::apkid::ApkIdAnalyzer::analyze(&apk_path, &sc);
                                    });
                                }
                            }
                        });
                    });
                    if let Some(results) = apkid {
                        ui.add_space(4.0);
                        for r in &results {
                            if !r.detections.is_empty() {
                                ui.label(egui::RichText::new(&r.file).size(10.0).color(theme.text_muted).italics());
                                for entry in &r.detections {
                                    ui.horizontal(|ui| {
                                        ui.label(egui::RichText::new(&entry.category).size(10.0).color(theme.text_muted));
                                        ui.label(egui::RichText::new(&entry.description).size(10.0).color(theme.text_primary));
                                    });
                                }
                            }
                        }
                    } else {
                        ui.label(egui::RichText::new("Click Run to identify protection techniques").size(10.0).color(theme.text_muted).italics());
                    }
                });
            ui.add_space(8.0);

            // Native Libs
            egui::Frame::NONE
                .fill(theme.bg_elevated)
                .corner_radius(egui::CornerRadius::same(5))
                .inner_margin(egui::Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("Native Libraries").size(theme.font_small).strong().color(theme.text_accent));

                    if is_flutter {
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Flutter").strong().color(egui::Color32::from_rgb(116, 199, 236)).size(10.0));
                            if let Some(ver) = &flutter_ver {
                                ui.label(egui::RichText::new(format!("v{}", ver)).color(theme.text_muted).size(10.0));
                            }
                        });
                        for lib in &flutter_libs {
                            let p = lib.clone();
                            if ui.small_button(lib.file_name().unwrap_or_default().to_string_lossy().as_ref()).clicked() {
                                state.lock().unwrap().open_file(p);
                            }
                        }
                        if let Some(app_so) = &libapp {
                            let p = app_so.clone();
                            if ui.small_button("libapp.so (Dart AOT)").clicked() {
                                state.lock().unwrap().open_file(p);
                            }
                        }
                    }

                    if !native_libs.is_empty() {
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new(format!("{} libraries", native_libs.len())).size(10.0).color(theme.text_muted));
                        egui::ScrollArea::vertical().id_salt("native_libs_list").max_height(140.0).show(ui, |ui| {
                            for lib_path in &native_libs {
                                let name = lib_path.file_name().unwrap_or_default().to_string_lossy();
                                let is_key = name == "libflutter.so" || name == "libapp.so";
                                if ui.selectable_label(false,
                                    egui::RichText::new(name.as_ref()).size(theme.font_small).color(if is_key { theme.info } else { theme.text_secondary })
                                ).clicked() {
                                    state.lock().unwrap().open_file(lib_path.clone());
                                }
                            }
                        });
                    } else {
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new("No native libraries extracted").size(10.0).color(theme.text_muted).italics());
                    }
                });
        });
    }

    fn render_runtime_panel(ui: &mut egui::Ui, state: &Arc<Mutex<AppState>>, theme: &Theme) {
        Self::render_sidebar_header(
            ui,
            theme,
            "Runtime",
            "ADB, packages, shell commands, and Frida controls for live analysis.",
            Some(("Device", theme.warning)),
        );
        ui.add_space(8.0);

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            // ADB section
            egui::Frame::NONE
                .fill(theme.bg_elevated)
                .corner_radius(egui::CornerRadius::same(5))
                .inner_margin(egui::Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("ADB").size(theme.font_small).strong().color(theme.text_accent));
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        if ui.small_button("Refresh").clicked() {
                            let sc = Arc::clone(state);
                            std::thread::spawn(move || { let _ = crate::runtime::adb::AdbManager::list_devices(&sc); });
                        }
                        if ui.small_button("Logcat").clicked() {
                            let sc = Arc::clone(state);
                            std::thread::spawn(move || { let _ = crate::runtime::adb::AdbManager::logcat_snapshot(&sc); });
                        }
                        if ui.small_button("Screenshot").on_hover_text("Capture device screen").clicked() {
                            let sc = Arc::clone(state);
                            std::thread::spawn(move || { let _ = crate::runtime::adb::AdbManager::capture_screenshot(&sc); });
                        }
                    });

                    ui.add_space(6.0);
                    ui.label(egui::RichText::new("Shell").size(10.0).color(theme.text_muted));
                    let mut shell_input = { state.lock().unwrap().adb_shell_input.clone() };
                    let submitted = ui.add(
                        egui::TextEdit::singleline(&mut shell_input)
                            .hint_text("adb shell command...")
                            .font(egui::TextStyle::Small)
                            .desired_width(f32::INFINITY),
                    ).lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    { state.lock().unwrap().adb_shell_input = shell_input.clone(); }
                    if submitted && !shell_input.is_empty() {
                        let sc = Arc::clone(state);
                        let cmd = shell_input;
                        std::thread::spawn(move || { let _ = crate::runtime::adb::AdbManager::shell_command(&sc, &cmd); });
                        state.lock().unwrap().adb_shell_input.clear();
                    }
                });
            ui.add_space(8.0);

            // Packages section
            egui::Frame::NONE
                .fill(theme.bg_elevated)
                .corner_radius(egui::CornerRadius::same(5))
                .inner_margin(egui::Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("Packages").size(theme.font_small).strong().color(theme.text_accent));
                    ui.add_space(4.0);
                    {
                        let mut s = state.lock().unwrap();
                        if s.adb_package_input.is_empty() {
                            if let Some(manifest) = &s.manifest_info {
                                s.adb_package_input = manifest.package.clone();
                            }
                        }
                    }
                    let mut package_input = { state.lock().unwrap().adb_package_input.clone() };
                    ui.add(
                        egui::TextEdit::singleline(&mut package_input)
                            .hint_text("com.example.app")
                            .font(egui::TextStyle::Small)
                            .desired_width(f32::INFINITY),
                    );
                    { state.lock().unwrap().adb_package_input = package_input.clone(); }
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        if ui.small_button("List 3rd-party").clicked() {
                            let sc = Arc::clone(state);
                            std::thread::spawn(move || { let _ = crate::runtime::adb::AdbManager::list_packages(&sc, Some("-3")); });
                        }
                        if ui.small_button("Uninstall").on_hover_text("adb uninstall").clicked() {
                            let pkg = package_input.trim().to_string();
                            if pkg.is_empty() {
                                state.lock().unwrap().push_log(LogLevel::Warn, "Enter a package name.");
                            } else {
                                let sc = Arc::clone(state);
                                std::thread::spawn(move || { let _ = crate::runtime::adb::AdbManager::uninstall_package(&sc, &pkg); });
                            }
                        }
                    });
                });
            ui.add_space(8.0);

            // File Transfer
            egui::Frame::NONE
                .fill(theme.bg_elevated)
                .corner_radius(egui::CornerRadius::same(5))
                .inner_margin(egui::Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    ui.label(egui::RichText::new("File Transfer").size(theme.font_small).strong().color(theme.text_accent));
                    let (mut pull_remote, mut pull_local, mut push_local, mut push_remote) = {
                        let s = state.lock().unwrap();
                        (s.adb_pull_remote.clone(), s.adb_pull_local.clone(), s.adb_push_local.clone(), s.adb_push_remote.clone())
                    };
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("Pull").size(10.0).color(theme.text_muted));
                    ui.add(egui::TextEdit::singleline(&mut pull_remote).hint_text("remote path").font(egui::TextStyle::Small).desired_width(f32::INFINITY));
                    ui.add(egui::TextEdit::singleline(&mut pull_local).hint_text("local path").font(egui::TextStyle::Small).desired_width(f32::INFINITY));
                    if ui.small_button("Pull file").clicked() {
                        let r = pull_remote.trim().to_string();
                        let l = pull_local.trim().to_string();
                        if !r.is_empty() && !l.is_empty() {
                            let sc = Arc::clone(state);
                            std::thread::spawn(move || { let _ = crate::runtime::adb::AdbManager::pull_file(&sc, &r, &l); });
                        }
                    }
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new("Push").size(10.0).color(theme.text_muted));
                    ui.add(egui::TextEdit::singleline(&mut push_local).hint_text("local path").font(egui::TextStyle::Small).desired_width(f32::INFINITY));
                    ui.add(egui::TextEdit::singleline(&mut push_remote).hint_text("remote path").font(egui::TextStyle::Small).desired_width(f32::INFINITY));
                    if ui.small_button("Push file").clicked() {
                        let l = push_local.trim().to_string();
                        let r = push_remote.trim().to_string();
                        if !l.is_empty() && !r.is_empty() {
                            let sc = Arc::clone(state);
                            std::thread::spawn(move || { let _ = crate::runtime::adb::AdbManager::push_file(&sc, &l, &r); });
                        }
                    }
                    {
                        let mut s = state.lock().unwrap();
                        s.adb_pull_remote = pull_remote;
                        s.adb_pull_local = pull_local;
                        s.adb_push_local = push_local;
                        s.adb_push_remote = push_remote;
                    }
                });
            ui.add_space(8.0);

            // Frida
            egui::Frame::NONE
                .fill(theme.bg_elevated)
                .corner_radius(egui::CornerRadius::same(5))
                .inner_margin(egui::Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Frida").size(theme.font_small).strong().color(theme.text_accent));
                        let avail = crate::runtime::frida::FridaManager::is_available(state);
                        ui.label(egui::RichText::new(if avail { "ready" } else { "missing" })
                            .size(10.0).color(if avail { theme.success } else { theme.error }));
                    });

                    ui.add_space(4.0);
                    let mut arch = { state.lock().unwrap().frida_server_arch.clone() };
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("ABI:").size(10.0).color(theme.text_muted));
                        egui::ComboBox::from_id_salt("frida_arch_cb").width(64.0)
                            .selected_text(egui::RichText::new(&arch).size(10.0))
                            .show_ui(ui, |ui| {
                                for a in &["arm64", "arm", "x86_64", "x86"] {
                                    ui.selectable_value(&mut arch, a.to_string(), egui::RichText::new(*a).size(10.0));
                                }
                            });
                        { state.lock().unwrap().frida_server_arch = arch.clone(); }
                        if ui.small_button("Get").on_hover_text("Download frida-server").clicked() {
                            let sc = Arc::clone(state);
                            let ac = arch.clone();
                            std::thread::spawn(move || { let _ = crate::runtime::frida::FridaManager::run_setup_script(&sc, &ac); });
                        }
                    });

                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        if ui.small_button("Push").on_hover_text("Push server to device").clicked() {
                            let sc = Arc::clone(state);
                            std::thread::spawn(move || { let _ = crate::runtime::frida::FridaManager::push_server(&sc); });
                        }
                        if ui.small_button("Start").on_hover_text("Start server on device").clicked() {
                            let sc = Arc::clone(state);
                            std::thread::spawn(move || { let _ = crate::runtime::frida::FridaManager::start_server(&sc); });
                        }
                        if ui.small_button("Kill").on_hover_text("Stop server").clicked() {
                            let sc = Arc::clone(state);
                            std::thread::spawn(move || { let _ = crate::runtime::frida::FridaManager::kill_server(&sc); });
                        }
                    });

                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Processes").size(10.0).color(theme.text_muted));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("Refresh").clicked() {
                                let sc = Arc::clone(state);
                                std::thread::spawn(move || { let _ = crate::runtime::frida::FridaManager::list_processes(&sc); });
                            }
                        });
                    });

                    let processes = { state.lock().unwrap().frida_processes.clone() };
                    let current_target = { state.lock().unwrap().frida_attach_target.clone() };
                    if !processes.is_empty() {
                        egui::ScrollArea::vertical().id_salt("frida_procs").max_height(100.0).show(ui, |ui| {
                            for proc in &processes {
                                let label = format!("{:>5}  {}", proc.pid, proc.name);
                                let selected = current_target == proc.name || current_target == proc.identifier;
                                if ui.selectable_label(selected,
                                    egui::RichText::new(&label).size(theme.font_small)
                                        .color(if selected { theme.accent_primary } else { theme.text_secondary })
                                ).clicked() {
                                    state.lock().unwrap().frida_attach_target = if proc.identifier.is_empty() {
                                        proc.name.clone()
                                    } else { proc.identifier.clone() };
                                }
                            }
                        });
                    }

                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("Target").size(10.0).color(theme.text_muted));
                    let mut target_buf = { state.lock().unwrap().frida_attach_target.clone() };
                    ui.add(egui::TextEdit::singleline(&mut target_buf).hint_text("com.example.app").font(egui::TextStyle::Small).desired_width(f32::INFINITY));
                    { state.lock().unwrap().frida_attach_target = target_buf; }

                    ui.add_space(4.0);
                    let mut spawn_mode = { state.lock().unwrap().frida_spawn_mode };
                    ui.horizontal(|ui| {
                        if ui.selectable_label(!spawn_mode, egui::RichText::new("Attach").size(10.0)).clicked() { spawn_mode = false; }
                        if ui.selectable_label(spawn_mode, egui::RichText::new("Spawn").size(10.0)).clicked() { spawn_mode = true; }
                    });
                    { state.lock().unwrap().frida_spawn_mode = spawn_mode; }

                    ui.add_space(4.0);
                    let templates = crate::runtime::frida_templates::get_templates();
                    let mut sel_idx = { state.lock().unwrap().frida_selected_template };
                    let cur = templates.get(sel_idx).map(|t| t.name).unwrap_or("-");
                    egui::ComboBox::from_id_salt("frida_tpl").width(ui.available_width()).selected_text(egui::RichText::new(cur).size(10.0)).show_ui(ui, |ui| {
                        for (i, tpl) in templates.iter().enumerate() {
                            if ui.selectable_label(sel_idx == i, egui::RichText::new(tpl.name).size(10.0)).on_hover_text(tpl.description).clicked() {
                                sel_idx = i;
                            }
                        }
                    });
                    {
                        let mut s = state.lock().unwrap();
                        if s.frida_selected_template != sel_idx {
                            s.frida_selected_template = sel_idx;
                            if let Some(tpl) = templates.get(sel_idx) {
                                s.frida_script = tpl.code.to_string();
                            }
                        }
                    }

                    ui.add_space(4.0);
                    let mut script_buf = { state.lock().unwrap().frida_script.clone() };
                    ui.add(egui::TextEdit::multiline(&mut script_buf)
                        .font(egui::FontId::monospace(10.0))
                        .desired_rows(6)
                        .desired_width(f32::INFINITY)
                        .code_editor()
                        .hint_text("// Frida script..."));
                    { state.lock().unwrap().frida_script = script_buf; }

                    ui.add_space(6.0);
                    let (attached, child_pid) = {
                        let s = state.lock().unwrap();
                        (s.frida_attached, s.frida_child_pid)
                    };
                    let target_str = { state.lock().unwrap().frida_attach_target.clone() };

                    if attached {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Running").size(10.0)
                                .color(theme.success).strong());
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("Detach").clicked() {
                                    if let Some(pid) = child_pid {
                                        let sc = Arc::clone(state);
                                        std::thread::spawn(move || { crate::runtime::frida::FridaManager::detach(pid, &sc); });
                                    }
                                }
                            });
                        });
                    } else {
                        let run_enabled = !target_str.is_empty();
                        let run_btn = ui.add_enabled(run_enabled,
                            egui::Button::new(egui::RichText::new("Run Frida").size(theme.font_small)
                                .color(if run_enabled { theme.success } else { theme.text_muted }))
                        );
                        if run_btn.on_hover_text("Inject script into target process").clicked() {
                            let sc = Arc::clone(state);
                            let log_tx = { state.lock().unwrap().log_tx.clone() };
                            let script = { state.lock().unwrap().frida_script.clone() };
                            let spawn = { state.lock().unwrap().frida_spawn_mode };
                            let target = target_str.clone();
                            std::thread::spawn(move || {
                                match crate::runtime::frida::FridaManager::attach_and_run(&sc, target, script, spawn, log_tx) {
                                    Ok(pid) => { sc.lock().unwrap().frida_attached = true; sc.lock().unwrap().frida_child_pid = Some(pid); }
                                    Err(e) => { sc.lock().unwrap().push_log(LogLevel::Error, &format!("[frida] {}", e)); }
                                }
                            });
                        }
                        if !run_enabled {
                            ui.label(egui::RichText::new("Set a target above").size(10.0).color(theme.text_muted).italics());
                        }
                    }
                });
        });
    }

    fn render_settings_panel(ui: &mut egui::Ui, state: &Arc<Mutex<AppState>>, theme: &Theme) {
        Self::render_sidebar_header(
            ui,
            theme,
            "Settings",
            "Configure editor behavior, appearance, and workspace defaults.",
            Some(("User", theme.info)),
        );
        ui.add_space(8.0);

        let mut settings = { state.lock().unwrap().settings.clone() };
        let mut dark_mode = { state.lock().unwrap().dark_mode };
        let original = settings.clone();
        let mut reset = false;

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            Self::settings_section(ui, theme, "Appearance", |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Theme").size(theme.font_small).color(theme.text_muted));
                    ui.selectable_value(&mut dark_mode, true, "Dark");
                    ui.selectable_value(&mut dark_mode, false, "Light");
                });

                ui.add_space(8.0);
                Self::settings_slider(
                    ui,
                    theme,
                    "UI font size",
                    &mut settings.ui_font_size,
                    10.0..=16.0,
                    "Controls menus, sidebars, toolbar, and panels.",
                );
            });

            ui.add_space(8.0);

            Self::settings_section(ui, theme, "Editor", |ui| {
                Self::settings_slider(
                    ui,
                    theme,
                    "Text font size",
                    &mut settings.editor_font_size,
                    10.0..=22.0,
                    "Controls code editor and hex viewer text.",
                );
                Self::settings_slider(
                    ui,
                    theme,
                    "Line height",
                    &mut settings.line_height,
                    1.10..=1.80,
                    "Controls vertical spacing between code lines.",
                );
                ui.checkbox(&mut settings.word_wrap, "Word wrap");
                ui.checkbox(&mut settings.auto_save, "Auto save files after edits");
            });

            ui.add_space(8.0);

            Self::settings_section(ui, theme, "Workbench", |ui| {
                ui.checkbox(&mut settings.show_bottom_panel, "Show bottom panel");
                ui.checkbox(
                    &mut settings.check_tool_updates_on_startup,
                    "Check toolbase updates on startup",
                );
                ui.label(
                    egui::RichText::new("Tool updates cover APKTool, JADX, APK signer, and Android platform-tools.")
                        .size(theme.font_small)
                        .color(theme.text_muted),
                );
                ui.add_space(6.0);
                if ui.button("Check Toolbase Updates").clicked() {
                    crate::app::RevEngApp::start_tool_update_check(state);
                }
                let status = {
                    let s = state.lock().unwrap();
                    if s.tool_update_checking {
                        Some("Checking for updates...".to_string())
                    } else if s.tool_update_installing {
                        Some("Updating toolbase...".to_string())
                    } else if !s.tool_updates_available.is_empty() {
                        Some(format!("{} update(s) waiting for your choice.", s.tool_updates_available.len()))
                    } else {
                        s.tool_update_error
                            .as_ref()
                            .map(|err| format!("Last check failed: {}", err))
                    }
                };
                if let Some(status) = status {
                    ui.label(
                        egui::RichText::new(status)
                            .size(theme.font_small)
                            .color(theme.text_muted),
                    );
                }
            });

            ui.add_space(12.0);
            egui::Frame::NONE
                .inner_margin(egui::Margin::symmetric(12, 0))
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Reset Settings").clicked() {
                            reset = true;
                        }
                        if let Some(path) = crate::app::AppSettings::path() {
                            if ui.button("Open Settings File").clicked() {
                                let _ = open::that(path);
                            }
                        }
                    });
                });
        });

        if reset {
            settings = crate::app::AppSettings::default();
            dark_mode = settings.dark_mode;
        }

        if reset || settings != original || dark_mode != original.dark_mode {
            let mut s = state.lock().unwrap();
            settings.dark_mode = dark_mode;
            s.dark_mode = dark_mode;
            s.settings = settings;
            s.save_settings();
        }
    }

    fn render_tool_update_prompt(ctx: &egui::Context, state: &Arc<Mutex<AppState>>, theme: &Theme) {
        let (updates, installing) = {
            let s = state.lock().unwrap();
            (s.tool_updates_available.clone(), s.tool_update_installing)
        };

        if updates.is_empty() || installing {
            return;
        }

        egui::Window::new("Toolbase Updates")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(theme.bg_elevated)
                    .corner_radius(theme.corner_radius as u8)
                    .inner_margin(16.0),
            )
            .show(ctx, |ui| {
                ui.set_width(480.0);
                ui.label(
                    egui::RichText::new("Updates are available for the bundled toolbase.")
                        .size(theme.font_heading)
                        .color(theme.text_primary),
                );
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new("Update now to replace the local tools used by decode, decompile, signing, install, and device actions.")
                        .size(theme.font_small)
                        .color(theme.text_secondary),
                );
                ui.add_space(12.0);

                for update in &updates {
                    egui::Frame::NONE
                        .fill(theme.bg_secondary)
                        .stroke(egui::Stroke::new(1.0, theme.border_subtle))
                        .corner_radius(egui::CornerRadius::same(theme.corner_radius as u8))
                        .inner_margin(egui::Margin::symmetric(10, 8))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(&update.display_name)
                                        .size(theme.font_small)
                                        .strong()
                                        .color(theme.text_primary),
                                );
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(
                                        egui::RichText::new(format!("latest {}", update.latest_version))
                                            .size(theme.font_small)
                                            .color(theme.success),
                                    );
                                });
                            });
                            let current = update
                                .current_version
                                .as_deref()
                                .unwrap_or("not installed");
                            ui.label(
                                egui::RichText::new(format!("Current: {}", current))
                                    .size(theme.font_small)
                                    .color(theme.text_muted),
                            );
                            ui.label(
                                egui::RichText::new(&update.detail)
                                    .size(theme.font_small)
                                    .color(theme.text_secondary),
                            );
                        });
                    ui.add_space(6.0);
                }

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Update Toolbase").clicked() {
                        crate::app::RevEngApp::start_tool_update_install(state);
                    }
                    if ui.button("Skip").clicked() {
                        let mut s = state.lock().unwrap();
                        s.tool_updates_available.clear();
                        s.status_message = "Toolbase update skipped".into();
                        s.push_log(LogLevel::Info, "Toolbase update skipped.");
                    }
                });
            });
    }

    fn settings_section(ui: &mut egui::Ui, theme: &Theme, title: &str, add_contents: impl FnOnce(&mut egui::Ui)) {
        egui::Frame::NONE
            .inner_margin(egui::Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(title)
                        .size(11.0)
                        .strong()
                        .color(theme.text_primary),
                );
                ui.add_space(6.0);
                add_contents(ui);
            });
    }

    fn settings_slider(
        ui: &mut egui::Ui,
        theme: &Theme,
        label: &str,
        value: &mut f32,
        range: std::ops::RangeInclusive<f32>,
        help: &str,
    ) {
        ui.label(egui::RichText::new(label).size(theme.font_small).color(theme.text_muted));
        let response = ui.add(
            egui::Slider::new(value, range)
                .step_by(0.5)
                .show_value(true),
        );
        response.on_hover_text(help);
    }

    fn render_app_studio_panel(ui: &mut egui::Ui, state: &Arc<Mutex<AppState>>, theme: &Theme) {
        Self::render_sidebar_header(
            ui,
            theme,
            "App Studio",
            "Identity editing + RE automation: graphing, detectors, recipes, rules, notes, plugins.",
            Some(("Arsenal", theme.accent_primary)),
        );
        ui.add_space(8.0);

        let mut pick_icon = false;
        let mut pick_rules = false;
        let mut pick_diff_dir = false;
        let mut action: Option<&'static str> = None;

        {
            let mut s = state.lock().unwrap();
            if s.app_studio_package_name.is_empty() {
                if let Some(pkg) = s.current_manifest_package() {
                    s.app_studio_package_name = pkg;
                }
            }

            egui::Frame::NONE
                .fill(theme.bg_elevated)
                .stroke(egui::Stroke::new(1.0, theme.border_subtle))
                .corner_radius(egui::CornerRadius::same((theme.corner_radius as u8) + 1))
                .inner_margin(egui::Margin::symmetric(10, 10))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("Identity")
                            .size(theme.font_small)
                            .color(theme.text_accent)
                            .strong(),
                    );
                    ui.add_space(4.0);

                    ui.add(
                        egui::TextEdit::singleline(&mut s.app_studio_package_name)
                            .hint_text("com.example.app")
                            .desired_width(f32::INFINITY)
                            .font(egui::TextStyle::Small),
                    );

                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if ui.button("Apply Package Rename").clicked() {
                            action = Some("rename_package");
                        }

                        ui.add(
                            egui::TextEdit::singleline(&mut s.app_studio_icon_path)
                                .hint_text("Icon file path (.png/.webp/.jpg)")
                                .desired_width((ui.available_width() - 94.0).max(100.0))
                                .font(egui::TextStyle::Small),
                        );
                        if ui.button("Browse").clicked() {
                            pick_icon = true;
                        }
                    });

                    ui.add_space(4.0);
                    if ui.button("Apply Icon Replace").clicked() {
                        action = Some("apply_icon");
                    }
                });

            ui.add_space(8.0);

            egui::Frame::NONE
                .fill(theme.bg_elevated)
                .stroke(egui::Stroke::new(1.0, theme.border_subtle))
                .corner_radius(egui::CornerRadius::same((theme.corner_radius as u8) + 1))
                .inner_margin(egui::Margin::symmetric(10, 10))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("Core Indexes")
                            .size(theme.font_small)
                            .color(theme.text_accent)
                            .strong(),
                    );
                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Rebuild Nav").clicked() { action = Some("rebuild_nav"); }
                        if ui.button("Rebuild Xref").clicked() { action = Some("rebuild_xref"); }
                        if ui.button("Rebuild Strings").clicked() { action = Some("rebuild_strings"); }
                        if ui.button("Build Smali Graph").clicked() { action = Some("smali_graph"); }
                    });
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut s.app_studio_diff_dir)
                                .hint_text("Other decoded APK directory")
                                .desired_width((ui.available_width() - 156.0).max(100.0))
                                .font(egui::TextStyle::Small),
                        );
                        if ui.button("Browse").clicked() {
                            pick_diff_dir = true;
                        }
                        if ui.button("Diff").clicked() {
                            action = Some("diff_decoded");
                        }
                    });
                });

            ui.add_space(8.0);

            egui::Frame::NONE
                .fill(theme.bg_elevated)
                .stroke(egui::Stroke::new(1.0, theme.border_subtle))
                .corner_radius(egui::CornerRadius::same((theme.corner_radius as u8) + 1))
                .inner_margin(egui::Margin::symmetric(10, 10))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("Security + Intelligence")
                            .size(theme.font_small)
                            .color(theme.text_accent)
                            .strong(),
                    );
                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("API Abuse Scan").clicked() { action = Some("api_abuse"); }
                        if ui.button("Obfuscation Assistant").clicked() { action = Some("deobf"); }
                        if ui.button("Anti-Tamper Scan").clicked() { action = Some("anti_tamper"); }
                        if ui.button("Endpoint Intel").clicked() { action = Some("endpoint"); }
                        if ui.button("JNI Bridge Map").clicked() { action = Some("jni_map"); }
                        if ui.button("Signing Forensics").clicked() { action = Some("signing"); }
                    });
                });

            ui.add_space(8.0);

            egui::Frame::NONE
                .fill(theme.bg_elevated)
                .stroke(egui::Stroke::new(1.0, theme.border_subtle))
                .corner_radius(egui::CornerRadius::same((theme.corner_radius as u8) + 1))
                .inner_margin(egui::Margin::symmetric(10, 10))
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("Automation")
                            .size(theme.font_small)
                            .color(theme.text_accent)
                            .strong(),
                    );
                    ui.add_space(6.0);

                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Patch Recipe: Root").clicked() { action = Some("patch_root"); }
                        if ui.button("Patch Recipe: SSL").clicked() { action = Some("patch_ssl"); }
                        if ui.button("Patch Recipe: Debuggable").clicked() { action = Some("patch_debug"); }
                    });

                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut s.app_studio_class_rename)
                                .hint_text("old.package.Class -> new.package.Class")
                                .desired_width((ui.available_width() - 104.0).max(100.0))
                                .font(egui::TextStyle::Small),
                        );
                        if ui.button("Rename Class").clicked() {
                            action = Some("rename_class");
                        }
                    });

                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut s.app_studio_symbol_input)
                                .hint_text("Frida target class/symbol")
                                .desired_width((ui.available_width() - 130.0).max(80.0))
                                .font(egui::TextStyle::Small),
                        );
                        if ui.button("Gen Frida Script").clicked() {
                            action = Some("frida_gen");
                        }
                    });

                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut s.app_studio_rule_file)
                                .hint_text("Rule file JSON path")
                                .desired_width((ui.available_width() - 140.0).max(80.0))
                                .font(egui::TextStyle::Small),
                        );
                        if ui.button("Browse Rules").clicked() {
                            pick_rules = true;
                        }
                        if ui.button("Run Rules").clicked() {
                            action = Some("run_rules");
                        }
                    });

                    ui.add_space(6.0);
                    ui.add(
                        egui::TextEdit::multiline(&mut s.app_studio_note_input)
                            .hint_text("Session note to append to analysis/session_notes.md")
                            .desired_rows(3)
                            .font(egui::TextStyle::Small),
                    );
                    if ui.button("Append Session Note").clicked() {
                        action = Some("append_note");
                    }

                    ui.add_space(6.0);
                    if ui.button("Run Plugins").clicked() {
                        action = Some("run_plugins");
                    }
                });

            ui.add_space(12.0);
            
            // Latest Report section with better styling
            egui::Frame::NONE
                .fill(theme.bg_elevated)
                .stroke(egui::Stroke::new(1.0, theme.border_subtle))
                .corner_radius(egui::CornerRadius::same((theme.corner_radius as u8) + 1))
                .inner_margin(egui::Margin::symmetric(10, 10))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Latest Report")
                                .size(theme.font_small)
                                .color(theme.text_accent)
                                .strong(),
                        );
                        
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let report = &s.app_studio_report;
                            if !report.trim().is_empty() {
                                if ui.small_button("Copy").on_hover_text("Copy report to clipboard").clicked() {
                                    ui.ctx().copy_text(report.clone());
                                    s.push_log(LogLevel::Info, "[AppStudio] Report copied to clipboard");
                                }
                                if ui.small_button("Clear").on_hover_text("Clear report").clicked() {
                                    s.app_studio_report.clear();
                                }
                            }
                        });
                    });
                    ui.add_space(6.0);
                    
                    let report = s.app_studio_report.clone();
                    
                    if report.trim().is_empty() {
                        ui.vertical_centered(|ui| {
                            ui.add_space(20.0);
                            ui.label(
                                egui::RichText::new("No report generated yet")
                                    .size(theme.font_small)
                                    .color(theme.text_muted)
                                    .italics(),
                            );
                            ui.add_space(4.0);
                            ui.label(
                                egui::RichText::new("Run a scan or analysis to see results here")
                                    .size(theme.font_small - 1.0)
                                    .color(theme.text_disabled),
                            );
                            ui.add_space(20.0);
                        });
                    } else {
                        egui::ScrollArea::vertical()
                            .max_height(300.0)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.add(
                                    egui::TextEdit::multiline(&mut report.as_str())
                                        .desired_width(f32::INFINITY)
                                        .font(egui::FontId::monospace(theme.font_small))
                                        .text_color(theme.console_text)
                                        .interactive(false),
                                );
                            });
                    }
                });
        }

        if pick_icon {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Image", &["png", "webp", "jpg", "jpeg"])
                .pick_file()
            {
                let mut s = state.lock().unwrap();
                s.app_studio_icon_path = path.to_string_lossy().to_string();
            }
        }

        if pick_rules {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("JSON", &["json"])
                .pick_file()
            {
                let mut s = state.lock().unwrap();
                s.app_studio_rule_file = path.to_string_lossy().to_string();
            }
        }

        if pick_diff_dir {
            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                let mut s = state.lock().unwrap();
                s.app_studio_diff_dir = path.to_string_lossy().to_string();
            }
        }

        if let Some(a) = action {
            if a == "run_plugins" {
                let root = {
                    let s = state.lock().unwrap();
                    s.workspace.root_dir().map(|p| p.to_path_buf())
                };

                match root {
                    Some(root) => {
                        if !Self::begin_app_studio_action(
                            state,
                            "App Studio: running plugins...",
                            "[AppStudio] Plugin execution started.",
                        ) {
                            return;
                        }
                        let state_c = Arc::clone(state);
                        std::thread::spawn(move || {
                            let lines = crate::engine::arsenal::Arsenal::run_plugins(&root);
                            let mut s = state_c.lock().unwrap();
                            s.app_studio_set_report("Plugin System", &lines);
                            s.status_message = "App Studio action completed - check report below".into();
                            s.push_log(LogLevel::Info, "[AppStudio] Plugin execution completed.");
                            s.busy = false;
                        });
                    }
                    None => {
                        let mut s = state.lock().unwrap();
                        s.push_log(LogLevel::Error, "[AppStudio] Action failed: No workspace open. Open APK first.");
                        s.status_message = "App Studio: No workspace open. Open APK first.".into();
                    }
                }
                return;
            }

            if a == "diff_decoded" {
                let input = {
                    let s = state.lock().unwrap();
                    s.workspace
                        .decoded_dir()
                        .map(|current| (current, s.app_studio_diff_dir.trim().to_string()))
                };

                match input {
                    Some((current, other_raw)) if !other_raw.is_empty() => {
                        let other = std::path::PathBuf::from(other_raw);
                        if !other.is_dir() {
                            let mut s = state.lock().unwrap();
                            s.push_log(
                                LogLevel::Error,
                                &format!(
                                    "[AppStudio] Action failed: Diff target is not a directory: {}",
                                    other.display()
                                ),
                            );
                            s.status_message =
                                format!("App Studio: Diff target is not a directory: {}", other.display());
                            return;
                        }

                        if !Self::begin_app_studio_action(
                            state,
                            "App Studio: diffing decoded workspaces...",
                            "[AppStudio] Decoded APK diff started.",
                        ) {
                            return;
                        }
                        let state_c = Arc::clone(state);
                        std::thread::spawn(move || {
                            let result = crate::engine::diff::ApkDiffer::diff(&current, &other);
                            let mut s = state_c.lock().unwrap();
                            match result {
                                Ok(diff) => {
                                    let mut lines = vec![
                                        format!("Current decoded: {}", diff.dir_a.display()),
                                        format!("Compare target: {}", diff.dir_b.display()),
                                        format!("Files in current: {}", diff.total_files_a),
                                        format!("Files in target: {}", diff.total_files_b),
                                        format!(
                                            "Modified: {} | Added: {} | Removed: {}",
                                            diff.modified_count(),
                                            diff.added_count(),
                                            diff.removed_count()
                                        ),
                                    ];

                                    for item in diff.diffs.iter().take(200) {
                                        lines.push(format!(
                                            "{} {} ({} -> {} bytes)",
                                            item.diff_type.label(),
                                            item.path,
                                            item.size_a,
                                            item.size_b
                                        ));
                                    }
                                    if diff.diffs.len() > 200 {
                                        lines.push(format!(
                                            "... {} additional changes omitted",
                                            diff.diffs.len() - 200
                                        ));
                                    }

                                    s.app_studio_set_report("Decoded APK Diff", &lines);
                                    s.status_message =
                                        "App Studio action completed - check report below".into();
                                    s.push_log(
                                        LogLevel::Info,
                                        &format!(
                                            "[AppStudio] Diff completed: {} changed files",
                                            diff.diffs.len()
                                        ),
                                    );
                                    s.busy = false;
                                }
                                Err(err) => {
                                    s.status_message = format!("App Studio: {err}");
                                    s.push_log(
                                        LogLevel::Error,
                                        &format!("[AppStudio] Action failed: {err}"),
                                    );
                                    s.busy = false;
                                }
                            }
                        });
                    }
                    Some((_current, _other_raw)) => {
                        let mut s = state.lock().unwrap();
                        s.push_log(
                            LogLevel::Error,
                            "[AppStudio] Action failed: Choose another decoded APK directory first.",
                        );
                        s.status_message =
                            "App Studio: Choose another decoded APK directory first.".into();
                    }
                    None => {
                        let mut s = state.lock().unwrap();
                        s.push_log(
                            LogLevel::Error,
                            "[AppStudio] Action failed: No decoded workspace found. Decode APK first.",
                        );
                        s.status_message =
                            "App Studio: No decoded workspace found. Decode APK first.".into();
                    }
                }
                return;
            }

            match a {
                "smali_graph" => {
                    Self::run_decoded_report_async(
                        state,
                        "Smali Graph",
                        "App Studio: building smali call graph...",
                        "[AppStudio] Smali graph build started.",
                        |decoded| {
                            let (path, nodes) =
                                crate::engine::arsenal::Arsenal::build_smali_call_graph(&decoded)?;
                            Ok((
                                vec![
                                    format!("Graph nodes: {}", nodes),
                                    format!("Output: {}", path.display()),
                                ],
                                format!(
                                    "[AppStudio] Smali graph built ({} nodes): {}",
                                    nodes,
                                    path.display()
                                ),
                            ))
                        },
                    );
                    return;
                }
                "api_abuse" => {
                    Self::run_decoded_report_async(
                        state,
                        "API Abuse Detector",
                        "App Studio: scanning API abuse patterns...",
                        "[AppStudio] API abuse scan started.",
                        |decoded| {
                            let findings = crate::engine::arsenal::Arsenal::detect_api_abuse(&decoded);
                            let count = findings.len();
                            Ok((
                                findings,
                                format!("[AppStudio] API abuse scan completed: {} findings", count),
                            ))
                        },
                    );
                    return;
                }
                "deobf" => {
                    Self::run_decoded_report_async(
                        state,
                        "Deobfuscation Assistant",
                        "App Studio: analyzing obfuscation signals...",
                        "[AppStudio] Deobfuscation analysis started.",
                        |decoded| {
                            let findings =
                                crate::engine::arsenal::Arsenal::suggest_deobfuscation(&decoded);
                            let count = findings.len();
                            Ok((
                                findings,
                                format!("[AppStudio] Deobfuscation suggestions: {}", count),
                            ))
                        },
                    );
                    return;
                }
                "anti_tamper" => {
                    Self::run_decoded_report_async(
                        state,
                        "Anti-Tamper Scanner",
                        "App Studio: scanning anti-tamper signals...",
                        "[AppStudio] Anti-tamper scan started.",
                        |decoded| {
                            let findings =
                                crate::engine::arsenal::Arsenal::scan_anti_tamper(&decoded);
                            let count = findings.len();
                            Ok((
                                findings,
                                format!(
                                    "[AppStudio] Anti-tamper scan completed: {} findings",
                                    count
                                ),
                            ))
                        },
                    );
                    return;
                }
                "endpoint" => {
                    Self::run_decoded_report_async(
                        state,
                        "Endpoint Intelligence",
                        "App Studio: extracting endpoint intelligence...",
                        "[AppStudio] Endpoint intel started.",
                        |decoded| {
                            let findings = crate::engine::arsenal::Arsenal::endpoint_intel(&decoded);
                            let count = findings.len();
                            Ok((
                                findings,
                                format!("[AppStudio] Endpoint intel completed: {} entries", count),
                            ))
                        },
                    );
                    return;
                }
                "jni_map" => {
                    let input = {
                        let s = state.lock().unwrap();
                        match (s.workspace.decoded_dir(), s.workspace.native_dir()) {
                            (Some(decoded), Some(native)) => Ok((decoded, native)),
                            (None, _) => Err("No decoded workspace found. Decode APK first."),
                            (_, None) => Err("No native workspace found. Open APK first."),
                        }
                    };

                    match input {
                        Ok((decoded, native)) => {
                            if !Self::begin_app_studio_action(
                                state,
                                "App Studio: mapping JNI bridges...",
                                "[AppStudio] JNI mapping analysis started.",
                            ) {
                                return;
                            }
                            let state_c = Arc::clone(state);
                            std::thread::spawn(move || {
                                let findings =
                                    crate::engine::arsenal::Arsenal::native_jni_bridge(&decoded, &native);
                                let mut s = state_c.lock().unwrap();
                                s.app_studio_set_report("Native JNI Bridge Mapper", &findings);
                                s.status_message =
                                    "App Studio action completed - check report below".into();
                                s.push_log(
                                    LogLevel::Info,
                                    "[AppStudio] JNI mapping analysis completed.",
                                );
                                s.busy = false;
                            });
                        }
                        Err(message) => {
                            let mut s = state.lock().unwrap();
                            s.push_log(
                                LogLevel::Error,
                                &format!("[AppStudio] Action failed: {message}"),
                            );
                            s.status_message = format!("App Studio: {message}");
                        }
                    }
                    return;
                }
                "signing" => {
                    let apk = {
                        let s = state.lock().unwrap();
                        s.workspace.apk_path().map(|p| p.to_path_buf())
                    };

                    match apk {
                        Some(apk) => {
                            if !Self::begin_app_studio_action(
                                state,
                                "App Studio: analyzing APK signing artifacts...",
                                "[AppStudio] Signing forensics started.",
                            ) {
                                return;
                            }
                            let state_c = Arc::clone(state);
                            std::thread::spawn(move || {
                                let result =
                                    crate::engine::arsenal::Arsenal::signing_forensics(&apk);
                                let mut s = state_c.lock().unwrap();
                                match result {
                                    Ok(findings) => {
                                        s.app_studio_set_report("Signing Forensics", &findings);
                                        s.status_message =
                                            "App Studio action completed - check report below".into();
                                        s.push_log(
                                            LogLevel::Info,
                                            "[AppStudio] Signing forensics completed.",
                                        );
                                        s.busy = false;
                                    }
                                    Err(err) => {
                                        s.status_message = format!("App Studio: {err}");
                                        s.push_log(
                                            LogLevel::Error,
                                            &format!("[AppStudio] Action failed: {err}"),
                                        );
                                        s.busy = false;
                                    }
                                }
                            });
                        }
                        None => {
                            let mut s = state.lock().unwrap();
                            s.push_log(
                                LogLevel::Error,
                                "[AppStudio] Action failed: No APK loaded. Open an APK first.",
                            );
                            s.status_message = "App Studio: No APK loaded. Open an APK first.".into();
                        }
                    }
                    return;
                }
                "run_rules" => {
                    let input = {
                        let s = state.lock().unwrap();
                        s.workspace
                            .decoded_dir()
                            .map(|decoded| (decoded, s.app_studio_rule_file.trim().to_string()))
                    };

                    match input {
                        Some((decoded, rule_file)) if !rule_file.is_empty() => {
                            if !Self::begin_app_studio_action(
                                state,
                                "App Studio: running rule engine...",
                                "[AppStudio] Rule engine started.",
                            ) {
                                return;
                            }
                            let state_c = Arc::clone(state);
                            std::thread::spawn(move || {
                                let result = crate::engine::arsenal::Arsenal::run_rule_engine(
                                    &decoded,
                                    std::path::Path::new(&rule_file),
                                );
                                let mut s = state_c.lock().unwrap();
                                match result {
                                    Ok(findings) => {
                                        let count = findings.len();
                                        s.app_studio_set_report("Rule Engine", &findings);
                                        s.status_message =
                                            "App Studio action completed - check report below".into();
                                        s.push_log(
                                            LogLevel::Info,
                                            &format!("[AppStudio] Rule engine completed: {} hits", count),
                                        );
                                        s.busy = false;
                                    }
                                    Err(err) => {
                                        s.status_message = format!("App Studio: {err}");
                                        s.push_log(
                                            LogLevel::Error,
                                            &format!("[AppStudio] Action failed: {err}"),
                                        );
                                        s.busy = false;
                                    }
                                }
                            });
                        }
                        Some((_decoded, _rule_file)) => {
                            let mut s = state.lock().unwrap();
                            s.push_log(
                                LogLevel::Error,
                                "[AppStudio] Action failed: Set a rules JSON path first.",
                            );
                            s.status_message = "App Studio: Set a rules JSON path first.".into();
                        }
                        None => {
                            let mut s = state.lock().unwrap();
                            s.push_log(
                                LogLevel::Error,
                                "[AppStudio] Action failed: No decoded workspace found. Decode APK first.",
                            );
                            s.status_message =
                                "App Studio: No decoded workspace found. Decode APK first.".into();
                        }
                    }
                    return;
                }
                "patch_root" | "patch_ssl" | "patch_debug" => {
                    let decoded = {
                        let s = state.lock().unwrap();
                        s.workspace.decoded_dir()
                    };

                    let Some(decoded) = decoded else {
                        let mut s = state.lock().unwrap();
                        s.push_log(
                            LogLevel::Error,
                            "[AppStudio] Action failed: No decoded workspace found. Decode APK first.",
                        );
                        s.status_message =
                            "App Studio: No decoded workspace found. Decode APK first.".into();
                        return;
                    };

                    let (recipe_key, recipe) = match a {
                        "patch_root" => (
                            "root",
                            crate::engine::arsenal::PatchRecipe::DisableRootChecks,
                        ),
                        "patch_ssl" => (
                            "ssl",
                            crate::engine::arsenal::PatchRecipe::BypassSslPinning,
                        ),
                        _ => (
                            "debug",
                            crate::engine::arsenal::PatchRecipe::ForceDebuggable,
                        ),
                    };

                    if !Self::begin_app_studio_action(
                        state,
                        format!("App Studio: applying '{}' patch recipe...", recipe_key),
                        format!("[AppStudio] Patch recipe '{}' started.", recipe_key),
                    ) {
                        return;
                    }

                    let state_c = Arc::clone(state);
                    std::thread::spawn(move || {
                        let result =
                            crate::engine::arsenal::Arsenal::apply_patch_recipe(&decoded, recipe);
                        let mut s = state_c.lock().unwrap();
                        match result {
                            Ok(changed) => {
                                let lines =
                                    vec![format!("Recipe '{}' changed {} files.", recipe_key, changed)];
                                s.app_studio_set_report("Smali Patch Recipes", &lines);
                                s.status_message =
                                    "App Studio action completed - check report below".into();
                                s.push_log(
                                    LogLevel::Info,
                                    &format!(
                                        "[AppStudio] Patch recipe '{}' applied ({} files)",
                                        recipe_key, changed
                                    ),
                                );
                                s.busy = false;
                            }
                            Err(err) => {
                                s.status_message = format!("App Studio: {err}");
                                s.push_log(
                                    LogLevel::Error,
                                    &format!("[AppStudio] Action failed: {err}"),
                                );
                                s.busy = false;
                            }
                        }
                    });
                    return;
                }
                "rename_class" => {
                    let input = {
                        let s = state.lock().unwrap();
                        s.workspace
                            .decoded_dir()
                            .map(|root| (root, s.app_studio_class_rename.trim().to_string()))
                    };

                    let Some((root, expression)) = input else {
                        let mut s = state.lock().unwrap();
                        s.push_log(
                            LogLevel::Error,
                            "[AppStudio] Action failed: No decoded workspace found. Decode APK first.",
                        );
                        s.status_message =
                            "App Studio: No decoded workspace found. Decode APK first.".into();
                        return;
                    };

                    let Some((old_class, new_class)) = expression.split_once("->") else {
                        let mut s = state.lock().unwrap();
                        s.push_log(
                            LogLevel::Error,
                            "[AppStudio] Action failed: Use class rename format: old.package.Class -> new.package.Class",
                        );
                        s.status_message =
                            "App Studio: Use class rename format: old.package.Class -> new.package.Class"
                                .into();
                        return;
                    };
                    let old_class = old_class.trim().to_string();
                    let new_class = new_class.trim().to_string();

                    if !Self::begin_app_studio_action(
                        state,
                        "App Studio: renaming class references...",
                        format!("[AppStudio] Class refactor started: {} -> {}", old_class, new_class),
                    ) {
                        return;
                    }

                    let state_c = Arc::clone(state);
                    std::thread::spawn(move || {
                        let result = crate::engine::refactor::RefactoringEngine::rename_class(
                            &root,
                            &old_class,
                            &new_class,
                        );
                        let mut s = state_c.lock().unwrap();
                        match result {
                            Ok(changed) => {
                                s.nav_index = crate::engine::navigation::NavIndex::default();
                                s.xref_db = None;
                                s.app_studio_set_report(
                                    "Class Refactor",
                                    &vec![
                                        format!("{} -> {}", old_class, new_class),
                                        format!("Touched {} files/paths.", changed),
                                        "Navigation and xref indexes were invalidated. Rebuild indexes after reviewing changes.".to_string(),
                                    ],
                                );
                                s.status_message =
                                    "App Studio action completed - check report below".into();
                                s.push_log(
                                    LogLevel::Info,
                                    &format!(
                                        "[AppStudio] Class refactor applied: {} -> {} ({} changes)",
                                        old_class, new_class, changed
                                    ),
                                );
                                s.busy = false;
                            }
                            Err(err) => {
                                s.status_message = format!("App Studio: {err}");
                                s.push_log(
                                    LogLevel::Error,
                                    &format!("[AppStudio] Action failed: {err}"),
                                );
                                s.busy = false;
                            }
                        }
                    });
                    return;
                }
                "apply_icon" => {
                    let input = {
                        let s = state.lock().unwrap();
                        s.workspace
                            .decoded_dir()
                            .map(|decoded| (decoded, s.app_studio_icon_path.trim().to_string()))
                    };

                    let Some((decoded, icon_path)) = input else {
                        let mut s = state.lock().unwrap();
                        s.push_log(
                            LogLevel::Error,
                            "[AppStudio] Action failed: No decoded workspace found. Decode APK first.",
                        );
                        s.status_message =
                            "App Studio: No decoded workspace found. Decode APK first.".into();
                        return;
                    };

                    if icon_path.is_empty() {
                        let mut s = state.lock().unwrap();
                        s.push_log(
                            LogLevel::Error,
                            "[AppStudio] Action failed: Choose an icon file first.",
                        );
                        s.status_message = "App Studio: Choose an icon file first.".into();
                        return;
                    }

                    let icon = std::path::PathBuf::from(icon_path);
                    if !Self::begin_app_studio_action(
                        state,
                        "App Studio: replacing launcher icons...",
                        format!("[AppStudio] Icon replacement started: {}", icon.display()),
                    ) {
                        return;
                    }

                    let state_c = Arc::clone(state);
                    std::thread::spawn(move || {
                        let result = crate::engine::arsenal::Arsenal::replace_app_icon(&decoded, &icon);
                        let mut s = state_c.lock().unwrap();
                        match result {
                            Ok(replaced) => {
                                s.app_studio_icon_path = icon.to_string_lossy().to_string();
                                s.app_studio_set_report(
                                    "Launcher Icon Replacement",
                                    &vec![format!(
                                        "Updated launcher icon assets in {} files.",
                                        replaced
                                    )],
                                );
                                s.status_message = format!("Icon replaced in {} files", replaced);
                                s.push_log(
                                    LogLevel::Info,
                                    &format!(
                                        "[AppStudio] App icon updated from {} ({} files)",
                                        icon.display(),
                                        replaced
                                    ),
                                );
                                s.busy = false;
                            }
                            Err(err) => {
                                s.status_message = format!("App Studio: {err}");
                                s.push_log(
                                    LogLevel::Error,
                                    &format!("[AppStudio] Action failed: {err}"),
                                );
                                s.busy = false;
                            }
                        }
                    });
                    return;
                }
                "rename_package" => {
                    let input = {
                        let s = state.lock().unwrap();
                        s.workspace.decoded_dir().map(|decoded| {
                            (
                                decoded,
                                s.workspace.decompiled_dir(),
                                s.app_studio_package_name.trim().to_string(),
                            )
                        })
                    };

                    let Some((decoded, decompiled, new_package)) = input else {
                        let mut s = state.lock().unwrap();
                        s.push_log(
                            LogLevel::Error,
                            "[AppStudio] Action failed: No decoded workspace found. Decode APK first.",
                        );
                        s.status_message =
                            "App Studio: No decoded workspace found. Decode APK first.".into();
                        return;
                    };

                    if !Self::begin_app_studio_action(
                        state,
                        format!("App Studio: renaming package to {}...", new_package),
                        format!("[AppStudio] Package rename started: {}", new_package),
                    ) {
                        return;
                    }

                    let state_c = Arc::clone(state);
                    std::thread::spawn(move || {
                        let result = crate::engine::refactor::RefactoringEngine::rename_package(
                            &decoded,
                            decompiled.as_deref(),
                            &new_package,
                        );
                        let mut s = state_c.lock().unwrap();
                        match result {
                            Ok((old_package, touched)) => {
                                s.app_studio_package_name = new_package.clone();
                                s.nav_index = crate::engine::navigation::NavIndex::default();
                                s.xref_db = None;
                                s.app_studio_set_report(
                                    "Package Refactor",
                                    &vec![
                                        format!("{} -> {}", old_package, new_package),
                                        format!("Touched {} files/paths.", touched),
                                        "Navigation and xref indexes were invalidated. Rebuild indexes after reviewing changes.".to_string(),
                                    ],
                                );
                                if touched == 0 {
                                    s.status_message = "Package name already set".into();
                                    s.push_log(LogLevel::Info, "Package name unchanged.");
                                } else {
                                    s.status_message = format!("Package renamed to {}", new_package);
                                    s.push_log(
                                        LogLevel::Info,
                                        &format!(
                                            "[AppStudio] Package renamed: {} -> {} ({} file updates)",
                                            old_package, new_package, touched
                                        ),
                                    );
                                }
                                s.busy = false;
                            }
                            Err(err) => {
                                s.status_message = format!("App Studio: {err}");
                                s.push_log(
                                    LogLevel::Error,
                                    &format!("[AppStudio] Action failed: {err}"),
                                );
                                s.busy = false;
                            }
                        }
                    });
                    return;
                }
                "rebuild_nav" => {
                    let state_c = Arc::clone(state);
                    if !Self::begin_app_studio_action(
                        state,
                        "App Studio: rebuilding navigation index...",
                        "[AppStudio] Navigation index rebuild started.",
                    ) {
                        return;
                    }
                    std::thread::spawn(move || {
                        let result = crate::engine::apk::ApkProcessor::build_nav_index(&state_c);
                        let mut s = state_c.lock().unwrap();
                        match result {
                            Ok(()) => {
                                s.status_message = "Navigation index rebuilt".into();
                                s.push_log(
                                    LogLevel::Info,
                                    "[AppStudio] Navigation index rebuild completed.",
                                );
                                s.busy = false;
                            }
                            Err(err) => {
                                s.status_message = format!("App Studio: {err}");
                                s.push_log(
                                    LogLevel::Error,
                                    &format!("[AppStudio] Action failed: {err}"),
                                );
                                s.busy = false;
                            }
                        }
                    });
                    return;
                }
                "rebuild_xref" => {
                    let state_c = Arc::clone(state);
                    if !Self::begin_app_studio_action(
                        state,
                        "App Studio: rebuilding xref index...",
                        "[AppStudio] Xref rebuild started.",
                    ) {
                        return;
                    }
                    std::thread::spawn(move || {
                        let result = crate::engine::apk::ApkProcessor::build_xref_db(&state_c);
                        let mut s = state_c.lock().unwrap();
                        match result {
                            Ok(()) => {
                                s.status_message = "Xref index rebuilt".into();
                                s.push_log(LogLevel::Info, "[AppStudio] Xref rebuild completed.");
                                s.busy = false;
                            }
                            Err(err) => {
                                s.status_message = format!("App Studio: {err}");
                                s.push_log(
                                    LogLevel::Error,
                                    &format!("[AppStudio] Action failed: {err}"),
                                );
                                s.busy = false;
                            }
                        }
                    });
                    return;
                }
                "rebuild_strings" => {
                    let state_c = Arc::clone(state);
                    if !Self::begin_app_studio_action(
                        state,
                        "App Studio: extracting strings...",
                        "[AppStudio] String extraction started.",
                    ) {
                        return;
                    }
                    std::thread::spawn(move || {
                        crate::engine::apk::ApkProcessor::extract_strings(&state_c);
                        let mut s = state_c.lock().unwrap();
                        s.status_message = "Strings extracted".into();
                        s.push_log(LogLevel::Info, "[AppStudio] String extraction completed.");
                        s.busy = false;
                    });
                    return;
                }
                _ => {}
            }

            if a == "append_note" {
                    let input = {
                        let s = state.lock().unwrap();
                        s.workspace
                            .root_dir()
                            .map(|root| (root.to_path_buf(), s.app_studio_note_input.trim().to_string()))
                    };

                    match input {
                        Some((root, note)) if !note.is_empty() => {
                            if !Self::begin_app_studio_action(
                                state,
                                "App Studio: saving session note...",
                                "[AppStudio] Session note save started.",
                            ) {
                                return;
                            }
                            let state_c = Arc::clone(state);
                            std::thread::spawn(move || {
                                let result =
                                    crate::engine::arsenal::Arsenal::append_session_note(&root, &note);
                                let mut s = state_c.lock().unwrap();
                                match result {
                                    Ok(path) => {
                                        s.app_studio_set_report(
                                            "Session Notebook",
                                            &vec![format!("Note appended to {}", path.display())],
                                        );
                                        s.app_studio_note_input.clear();
                                        s.status_message =
                                            "App Studio action completed - check report below".into();
                                        s.push_log(
                                            LogLevel::Info,
                                            &format!(
                                                "[AppStudio] Session note saved: {}",
                                                path.display()
                                            ),
                                        );
                                        s.busy = false;
                                    }
                                    Err(err) => {
                                        s.status_message = format!("App Studio: {err}");
                                        s.push_log(
                                            LogLevel::Error,
                                            &format!("[AppStudio] Action failed: {err}"),
                                        );
                                        s.busy = false;
                                    }
                                }
                            });
                            return;
                        }
                        Some((_root, _note)) => {
                            let mut s = state.lock().unwrap();
                            s.push_log(LogLevel::Error, "[AppStudio] Action failed: Note is empty.");
                            s.status_message = "App Studio: Note is empty.".into();
                            return;
                        }
                        None => {
                            let mut s = state.lock().unwrap();
                            s.push_log(
                                LogLevel::Error,
                                "[AppStudio] Action failed: No workspace open. Open APK first.",
                            );
                            s.status_message = "App Studio: No workspace open. Open APK first.".into();
                            return;
                        }
                    }
            }

            if a == "frida_gen" {
                state.lock().unwrap().app_studio_generate_frida_template();
                let mut s = state.lock().unwrap();
                s.status_message = "App Studio action completed - check report below".into();
            }
        }
    }

    fn render_strings_panel(ui: &mut egui::Ui, state: &Arc<Mutex<AppState>>, theme: &Theme) {
        let (strings_snapshot, decoded_exists, strings_revision) = {
            let s = state.lock().unwrap();
            (
                s.strings_view_cache
                    .clone()
                    .unwrap_or_else(|| Arc::new(s.extracted_strings.clone())),
                s.workspace
                    .decoded_dir()
                    .map(|dir| dir.exists())
                    .unwrap_or(false),
                s.strings_revision,
            )
        };

        #[derive(Default)]
        struct StringsPanelCache {
            revision: u64,
            search: String,
            filter: Option<crate::engine::strings::StringCategory>,
            indices: Vec<usize>,
        }

        thread_local! {
            static STRINGS_CACHE: std::cell::RefCell<StringsPanelCache> = std::cell::RefCell::new(StringsPanelCache::default());
        }
        let interesting_count = strings_snapshot
            .iter()
            .filter(|entry| entry.category != crate::engine::strings::StringCategory::Other)
            .count();
        let header_stat = if strings_snapshot.is_empty() {
            if decoded_exists { "Ready to rebuild".to_string() } else { "Waiting for decode".to_string() }
        } else {
            format!("{} total / {} interesting", strings_snapshot.len(), interesting_count)
        };

        Self::render_sidebar_header(
            ui, theme, "Strings",
            "URLs, secrets, tokens, resource text, and smali const-strings.",
            Some((header_stat.as_str(), theme.success)),
        );
        ui.add_space(8.0);

        let mut search_query = { state.lock().unwrap().string_search_query.clone() };
        if decoded_exists || !strings_snapshot.is_empty() {
            Self::render_sidebar_search_box(ui, theme, "Filter", &mut search_query,
                "Filter strings, categories, classes, or paths...",
            );
        }
        {
            state.lock().unwrap().string_search_query = search_query.clone();
        }

        let current_filter = { state.lock().unwrap().string_filter.clone() };
        let search_query2 = { state.lock().unwrap().string_search_query.clone() };
        let search_lower = search_query2.to_lowercase();

        let filtered_indices = STRINGS_CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            let needs_refresh = cache.revision != strings_revision
                || cache.search != search_lower
                || cache.filter != current_filter
                || cache.indices.len() > strings_snapshot.len();
            if needs_refresh {
                cache.revision = strings_revision;
                cache.search = search_lower.clone();
                cache.filter = current_filter.clone();
                cache.indices.clear();
                cache.indices.extend(strings_snapshot.iter().enumerate().filter_map(|(idx, s)| {
                    let cat_match = current_filter.as_ref().map(|f| &s.category == f).unwrap_or(true);
                    let search_match = search_lower.is_empty() || s.searchable_text().contains(&search_lower);
                    if cat_match && search_match { Some(idx) } else { None }
                }));
            }
            cache.indices.clone()
        });

        if strings_snapshot.is_empty() {
            ui.add_space(8.0);
            if decoded_exists {
                Self::render_sidebar_empty_state(ui, theme, "No strings loaded yet",
                    "Decoded files exist, but the strings index is empty. Click Refresh to rebuild it.");
            } else {
                Self::render_sidebar_empty_state(ui, theme, "No strings extracted yet",
                    "Decode an APK first so the IDE can scan smali and resources for strings.");
            }
            return;
        }

        // Refresh button
        if decoded_exists {
            if ui.small_button("Refresh strings").on_hover_text("Rebuild strings index").clicked() {
                let sc = Arc::clone(state);
                std::thread::spawn(move || { crate::engine::apk::ApkProcessor::extract_strings(&sc); });
            }
        }

        ui.add_space(6.0);

        // Chips
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
            let all_active = current_filter.is_none();
            let chip = |ui: &mut egui::Ui, label: &str, active: bool, color: egui::Color32| {
                let frame = egui::Frame::NONE
                    .fill(if active { color.linear_multiply(0.15) } else { theme.bg_elevated })
                    .stroke(Stroke::new(1.0, if active { color } else { theme.border_subtle }))
                    .corner_radius(CornerRadius::same(4))
                    .inner_margin(Margin::symmetric(6, 2));
                let resp = frame.show(ui, |ui| {
                    ui.label(RichText::new(label).size(9.0).color(if active { color } else { theme.text_muted }));
                }).response.interact(Sense::click());
                resp
            };
            if chip(ui, "All", all_active, theme.accent_primary).clicked() {
                state.lock().unwrap().string_filter = None;
            }
            for cat in crate::engine::strings::StringCategory::all() {
                let active = current_filter.as_ref() == Some(cat);
                if chip(ui, cat.label(), active, cat.color()).clicked() {
                    state.lock().unwrap().string_filter = if active { None } else { Some(cat.clone()) };
                }
            }
        });

        if filtered_indices.is_empty() {
            ui.add_space(12.0);
            Self::render_sidebar_empty_state(ui, theme, "No matching strings",
                "Try a broader filter or clear the active category chip.");
            return;
        }

        ui.add_space(6.0);
        ui.label(RichText::new(format!("{} of {} strings", filtered_indices.len(), strings_snapshot.len()))
            .size(theme.font_small).color(theme.text_muted));
        ui.add_space(4.0);

        let row_height = 42.0;
        egui::Frame::NONE
            .fill(theme.bg_elevated)
            .corner_radius(CornerRadius::same(5))
            .inner_margin(Margin::symmetric(4, 2))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show_rows(ui, row_height, filtered_indices.len(), |ui, range| {
                        for row in range {
                            let es = &strings_snapshot[filtered_indices[row]];
                            let bg = if row % 2 == 0 { theme.bg_elevated } else { theme.bg_secondary };
                            let display = {
                                let cc = es.value.chars().count();
                                if cc > 96 { format!("{}...", es.value.chars().take(93).collect::<String>()) }
                                else { es.value.clone() }
                            };
                            let source_name = es.source_file.file_name().and_then(|n| n.to_str()).unwrap_or("?");

                            let resp = Self::sidebar_row(ui, theme, false, bg, row_height, |ui| {
                                let cat_badge = egui::Frame::NONE
                                    .fill(es.category.color().linear_multiply(0.15))
                                    .corner_radius(CornerRadius::same(4))
                                    .inner_margin(Margin::symmetric(5, 2));
                                cat_badge.show(ui, |ui| {
                                    ui.label(RichText::new(es.category.label()).size(8.8).strong().color(es.category.color()));
                                });
                                let src_badge = egui::Frame::NONE
                                    .fill(theme.accent_primary.linear_multiply(0.1))
                                    .corner_radius(CornerRadius::same(4))
                                    .inner_margin(Margin::symmetric(5, 2));
                                src_badge.show(ui, |ui| {
                                    ui.label(RichText::new(es.context.source_kind()).size(8.8).color(theme.accent_primary));
                                });
                                ui.vertical(|ui| {
                                    ui.spacing_mut().item_spacing.y = 0.0;
                                    ui.label(RichText::new(&display).size(theme.font_small).color(theme.text_primary).monospace());
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new(source_name).size(theme.font_small - 0.5).color(theme.text_muted));
                                        ui.label(RichText::new(format!("line {}", es.line)).size(theme.font_small - 0.5).color(theme.text_muted));
                                    });
                                });
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(RichText::new("Open").size(theme.font_small - 0.5).color(theme.text_muted));
                                });
                            }).on_hover_text(format!("{}\n\n{}", es.value, es.location_label()));

                            if resp.clicked() {
                                state.lock().unwrap().open_file_at_line(es.source_file.clone(), es.line, Some(es.value.clone()));
                            }
                            resp.context_menu(|ui| {
                                let t = Theme::current(ui);
                                if ui.button(RichText::new("Open Source").color(t.text_primary)).clicked() {
                                    state.lock().unwrap().open_file_at_line(es.source_file.clone(), es.line, Some(es.value.clone()));
                                    ui.close_menu();
                                }
                                if ui.button(RichText::new("Copy String").color(t.text_primary)).clicked() {
                                    ui.ctx().copy_text(es.value.clone());
                                    ui.close_menu();
                                }
                                if ui.button(RichText::new("Copy Path").color(t.text_primary)).clicked() {
                                    ui.ctx().copy_text(es.source_file.display().to_string());
                                    ui.close_menu();
                                }
                            });
                        }
                    });
            });
    }

    fn render_busy_bar(ui: &mut egui::Ui, theme: &Theme) {
        let width = ui.available_width().max(80.0);
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(width, 3.0), egui::Sense::hover());

        let time = ui.ctx().input(|i| i.time) as f32;
        let segment_width = (rect.width() * 0.22).max(36.0);
        let travel = (rect.width() + segment_width).max(1.0);
        let phase = (time * 140.0) % travel;
        let left = rect.left() - segment_width + phase;
        let moving = egui::Rect::from_min_max(
            egui::pos2(left, rect.top()),
            egui::pos2((left + segment_width).min(rect.right()), rect.bottom()),
        )
        .intersect(rect);

        ui.painter().rect_filled(moving, 0.0, theme.accent_primary);
    }
}

#[cfg(test)]
mod tests {
    use super::IdeLayout;

    #[test]
    fn truncates_utf8_text_on_character_boundaries() {
        assert_eq!(IdeLayout::truncate_end_chars("abé中cd", 4), "abé中");
        assert_eq!(IdeLayout::truncate_start_chars("abé中cd", 4), "é中cd");
    }
}
