//! Command palette — Ctrl+Shift+P for commands, Ctrl+P for quick file open.

use crate::app::{AppState, PaletteMode};
use crate::ui::theme::Theme;

use std::sync::{Arc, Mutex};

pub struct CommandPalette {
    query: String,
    selected: usize,
}

struct PaletteEntry {
    label: String,
    shortcut: &'static str,
    category: &'static str,
}

impl CommandPalette {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            selected: 0,
        }
    }

    pub fn render(&mut self, ctx: &egui::Context, state: &Arc<Mutex<AppState>>) {
        let theme = Theme::from_ctx(ctx);

        let show = { state.lock().unwrap().show_command_palette };
        if !show {
            self.query.clear();
            self.selected = 0;
            return;
        }

        let mode = { state.lock().unwrap().palette_mode.clone() };

        // Dimmed background overlay
        let screen = ctx.screen_rect();
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new("palette_overlay"),
        ));
        painter.rect_filled(screen, 0.0, egui::Color32::from_black_alpha(100));

        let palette_width = 520.0_f32.min(screen.width() - 40.0);
        let palette_x = screen.center().x - palette_width / 2.0;

        egui::Area::new(egui::Id::new("command_palette"))
            .fixed_pos(egui::pos2(palette_x, screen.top() + 80.0))
            .show(ctx, |ui| {
                let frame = egui::Frame::NONE
                    .fill(theme.bg_elevated)
                    .corner_radius(egui::CornerRadius::same((theme.corner_radius as u8) + 2))
                    .stroke(egui::Stroke::new(1.0, theme.border))
                    .shadow(egui::Shadow {
                        offset: [0, 4],
                        blur: 24,
                        spread: 0,
                        color: egui::Color32::from_black_alpha(60),
                    })
                    .inner_margin(egui::Margin::same(12));

                frame.show(ui, |ui| {
                    ui.set_width(palette_width);

                    // Search input
                    let hint = match mode {
                        PaletteMode::Commands => "Type a command...",
                        PaletteMode::Files => "Type a filename...",
                    };

                    let response = ui.add(
                        egui::TextEdit::singleline(&mut self.query)
                            .hint_text(hint)
                            .desired_width(palette_width - 24.0)
                            .font(egui::TextStyle::Body),
                    );
                    response.request_focus();

                    ui.add_space(4.0);
                    let sep = ui.available_rect_before_wrap();
                    ui.painter().rect_filled(
                        egui::Rect::from_min_size(sep.min, egui::vec2(sep.width(), 1.0)),
                        0.0,
                        theme.separator,
                    );
                    ui.add_space(4.0);

                    match mode {
                        PaletteMode::Commands => self.render_commands(ui, state, &theme),
                        PaletteMode::Files => self.render_files(ui, state, &theme),
                    }
                });
            });

        // Handle keyboard
        ctx.input(|i| {
            if i.key_pressed(egui::Key::Escape) {
                state.lock().unwrap().show_command_palette = false;
            }
            if i.key_pressed(egui::Key::ArrowDown) {
                self.selected = self.selected.saturating_add(1);
            }
            if i.key_pressed(egui::Key::ArrowUp) {
                self.selected = self.selected.saturating_sub(1);
            }
        });
    }

    fn render_commands(&mut self, ui: &mut egui::Ui, state: &Arc<Mutex<AppState>>, theme: &Theme) {
        let commands = vec![
            PaletteEntry { label: "Open APK".into(), shortcut: "Ctrl+O", category: "File" },
            PaletteEntry { label: "Save File".into(), shortcut: "Ctrl+S", category: "File" },
            PaletteEntry { label: "Decode APK".into(), shortcut: "", category: "APK" },
            PaletteEntry { label: "Decompile APK".into(), shortcut: "", category: "APK" },
            PaletteEntry { label: "Build APK".into(), shortcut: "", category: "APK" },
            PaletteEntry { label: "Sign APK".into(), shortcut: "", category: "APK" },
            PaletteEntry { label: "Install on Device".into(), shortcut: "", category: "Device" },
            PaletteEntry { label: "ADB Devices".into(), shortcut: "", category: "Device" },
            PaletteEntry { label: "Start Logcat".into(), shortcut: "", category: "Device" },
            PaletteEntry { label: "Toggle Theme".into(), shortcut: "", category: "Settings" },
            PaletteEntry { label: "Open Settings".into(), shortcut: "", category: "Settings" },
            PaletteEntry { label: "Toggle Help".into(), shortcut: "", category: "Settings" },
            PaletteEntry { label: "Refresh Toolchain".into(), shortcut: "", category: "Settings" },
            PaletteEntry { label: "Find in File".into(), shortcut: "Ctrl+F", category: "Search" },
            PaletteEntry { label: "Quick Open File".into(), shortcut: "Ctrl+P", category: "Navigate" },
            PaletteEntry { label: "Flutter SSL Bypass".into(), shortcut: "", category: "Patch" },
            PaletteEntry { label: "View Manifest".into(), shortcut: "", category: "Analysis" },
            PaletteEntry { label: "View Strings".into(), shortcut: "", category: "Analysis" },
            PaletteEntry { label: "View DEX Stats".into(), shortcut: "", category: "Analysis" },
            PaletteEntry { label: "Open App Studio".into(), shortcut: "", category: "Analysis" },
        ];

        let query_lower = self.query.to_lowercase();
        let filtered: Vec<&PaletteEntry> = commands
            .iter()
            .filter(|c| {
                query_lower.is_empty()
                    || c.label.to_lowercase().contains(&query_lower)
                    || c.category.to_lowercase().contains(&query_lower)
            })
            .collect();

        self.selected = self.selected.min(filtered.len().saturating_sub(1));

        egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
            for (i, entry) in filtered.iter().enumerate() {
                let is_selected = i == self.selected;
                let bg = if is_selected { theme.accent_subtle } else { egui::Color32::TRANSPARENT };

                let frame = egui::Frame::NONE
                    .fill(bg)
                    .corner_radius(egui::CornerRadius::same(4))
                    .inner_margin(egui::Margin::symmetric(8, 4));

                let resp = frame.show(ui, |ui| {
                    ui.horizontal(|ui| {
                        // Category badge
                        let badge_frame = egui::Frame::NONE
                            .fill(theme.accent_primary.linear_multiply(0.12))
                            .corner_radius(egui::CornerRadius::same(3))
                            .inner_margin(egui::Margin::symmetric(4, 1));
                        badge_frame.show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(entry.category)
                                    .size(theme.font_small)
                                    .color(theme.accent_primary),
                            );
                        });

                        ui.label(
                            egui::RichText::new(&entry.label)
                                .color(theme.text_primary),
                        );

                        if !entry.shortcut.is_empty() {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(
                                    egui::RichText::new(entry.shortcut)
                                        .size(theme.font_small)
                                        .color(theme.text_muted),
                                );
                            });
                        }
                    });
                }).response;

                if resp.clicked() || (is_selected && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                    self.execute_command(&entry.label, state);
                }
            }
        });
    }

    fn render_files(&mut self, ui: &mut egui::Ui, state: &Arc<Mutex<AppState>>, theme: &Theme) {
        let workspace_root = {
            let s = state.lock().unwrap();
            s.workspace.root_dir().map(|p| p.to_path_buf())
        };

        let Some(root) = workspace_root else {
            ui.label(
                egui::RichText::new("No workspace open")
                    .color(theme.text_muted)
                    .italics(),
            );
            return;
        };

        // Collect files (limited scan)
        let mut files: Vec<std::path::PathBuf> = Vec::new();
        let exts = ["java", "smali", "xml", "json", "so"];
        for entry in walkdir::WalkDir::new(&root).max_depth(10).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                if let Some(ext) = entry.path().extension().and_then(|e| e.to_str()) {
                    if exts.contains(&ext.to_lowercase().as_str()) {
                        files.push(entry.path().to_path_buf());
                    }
                }
            }
            if files.len() > 500 {
                break;
            }
        }

        let query_lower = self.query.to_lowercase();
        let filtered: Vec<&std::path::PathBuf> = files
            .iter()
            .filter(|f| {
                query_lower.is_empty()
                    || f.file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n.to_lowercase().contains(&query_lower))
                        .unwrap_or(false)
            })
            .take(50)
            .collect();

        self.selected = self.selected.min(filtered.len().saturating_sub(1));

        egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
            for (i, path) in filtered.iter().enumerate() {
                let is_selected = i == self.selected;
                let bg = if is_selected { theme.accent_subtle } else { egui::Color32::TRANSPARENT };

                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                let (icon, icon_color) = theme.file_icon(ext);

                let rel_path = path.strip_prefix(&root).unwrap_or(path);

                let frame = egui::Frame::NONE
                    .fill(bg)
                    .corner_radius(egui::CornerRadius::same(4))
                    .inner_margin(egui::Margin::symmetric(8, 3));

                let resp = frame.show(ui, |ui| {
                    ui.horizontal(|ui| {
                        // File badge
                        let badge = egui::Frame::NONE
                            .fill(icon_color.linear_multiply(0.15))
                            .corner_radius(egui::CornerRadius::same(2))
                            .inner_margin(egui::Margin::symmetric(3, 1));
                        badge.show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(icon)
                                    .size(9.0)
                                    .strong()
                                    .color(icon_color),
                            );
                        });

                        ui.label(
                            egui::RichText::new(name)
                                .color(theme.text_primary),
                        );
                        ui.label(
                            egui::RichText::new(rel_path.display().to_string())
                                .size(theme.font_small)
                                .color(theme.text_muted),
                        );
                    });
                }).response;

                if resp.clicked() || (is_selected && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                    state.lock().unwrap().open_file((*path).clone());
                    state.lock().unwrap().show_command_palette = false;
                }
            }
        });
    }

    fn execute_command(&self, label: &str, state: &Arc<Mutex<AppState>>) {
        state.lock().unwrap().show_command_palette = false;

        match label {
            "Open APK" => {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("APK / XAPK Files", &["apk", "xapk"])
                    .add_filter("All Files", &["*"])
                    .pick_file()
                {
                    let state_c = Arc::clone(state);
                    std::thread::spawn(move || {
                        if let Err(e) = crate::engine::apk::ApkProcessor::open_apk(&state_c, &path) {
                            let mut s = state_c.lock().unwrap();
                            s.push_log(crate::app::LogLevel::Error, &format!("APK open failed: {}", e));
                            s.status_message = "APK open failed".into();
                            s.busy = false;
                        }
                    });
                }
            }
            "Save File" => {
                let save_result = {
                    let mut s = state.lock().unwrap();
                    s.save_active_tab()
                };
                if let Err(e) = save_result {
                    state.lock().unwrap().push_log(crate::app::LogLevel::Error, &format!("Save failed: {}", e));
                }
            }
            "Decode APK" => Self::spawn_app_task(state, "Decode", crate::engine::apk::ApkProcessor::decode_apk),
            "Decompile APK" => Self::spawn_app_task(state, "Decompile", crate::engine::apk::ApkProcessor::decompile_apk),
            "Build APK" => Self::spawn_app_task(state, "Build", crate::engine::apk::ApkProcessor::build_apk),
            "Sign APK" => Self::spawn_app_task(state, "Sign", crate::engine::apk::ApkProcessor::sign_apk),
            "Install on Device" => Self::spawn_app_task(state, "Install", crate::runtime::adb::AdbManager::install_apk),
            "ADB Devices" => Self::spawn_app_task(state, "ADB devices", crate::runtime::adb::AdbManager::list_devices),
            "Start Logcat" => {
                crate::runtime::adb::AdbManager::stream_logcat(Arc::clone(state));
            }
            "Toggle Theme" => {
                let mut s = state.lock().unwrap();
                s.dark_mode = !s.dark_mode;
                s.settings.dark_mode = s.dark_mode;
                s.save_settings();
            }
            "Toggle Help" => {
                let mut s = state.lock().unwrap();
                s.show_help = !s.show_help;
            }
            "Open Settings" => {
                let mut s = state.lock().unwrap();
                s.sidebar_view = crate::app::SideBarView::Settings;
            }
            "Refresh Toolchain" => {
                let mut s = state.lock().unwrap();
                let results = s.toolchain.verify_all();
                for (tool, ok) in results {
                    if ok {
                        s.push_log(crate::app::LogLevel::Info, &format!("Tool found: {}", tool));
                    } else {
                        let required = s
                            .toolchain
                            .get(&tool)
                            .map(|info| info.required == crate::engine::toolchain::ToolRequirement::Required)
                            .unwrap_or(true);
                        if required {
                            s.push_log(crate::app::LogLevel::Warn, &format!("Tool missing: {}", tool));
                        } else {
                            s.push_log(crate::app::LogLevel::Info, &format!("Optional tool missing: {}", tool));
                        }
                    }
                    if let Some(tip) = crate::engine::toolchain::ToolchainManager::get_tool_tip(&tool) {
                        s.push_log(crate::app::LogLevel::Debug, tip);
                    }
                }
            }
            "Quick Open File" => {
                let mut s = state.lock().unwrap();
                s.palette_mode = PaletteMode::Files;
                s.show_command_palette = true;
            }
            "Find in File" => {
                let mut s = state.lock().unwrap();
                let active = s.active_tab;
                if let Some(idx) = active.and_then(|idx| s.open_tabs.get_mut(idx)) {
                    idx.search_visible = true;
                }
            }
            "Flutter SSL Bypass" => {
                let native_dir = { state.lock().unwrap().workspace.native_dir().map(|p| p.to_path_buf()) };
                if let Some(nd) = native_dir {
                    let state_c = Arc::clone(state);
                    std::thread::spawn(move || {
                        let flutter_libs = crate::native::flutter_patch::FlutterPatcher::detect_flutter(&nd);
                        if flutter_libs.is_empty() {
                            state_c.lock().unwrap().push_log(crate::app::LogLevel::Warn, "No libflutter.so found in native libs.");
                        } else {
                            for lib in &flutter_libs {
                                state_c.lock().unwrap().push_log(crate::app::LogLevel::Info, &format!("Patching: {}", lib.display()));
                                match crate::native::flutter_patch::FlutterPatcher::bypass_ssl_pinning(lib) {
                                    Ok(result) => {
                                        let mut s = state_c.lock().unwrap();
                                        if result.patched {
                                            s.push_log(crate::app::LogLevel::Info, &format!("SSL bypass applied: {} patches at {}", result.patches_applied.len(), result.target_file.display()));
                                        } else {
                                            s.push_log(crate::app::LogLevel::Warn, &result.message);
                                        }
                                    }
                                    Err(e) => {
                                        state_c.lock().unwrap().push_log(crate::app::LogLevel::Error, &format!("Flutter patch failed: {}", e));
                                    }
                                }
                            }
                        }
                    });
                } else {
                    let mut s = state.lock().unwrap();
                    s.push_log(crate::app::LogLevel::Warn, "No workspace open. Open an APK first.");
                }
            }
            "View Manifest" => {
                let mut s = state.lock().unwrap();
                s.sidebar_view = crate::app::SideBarView::NativeAnalysis;
            }
            "View Strings" => {
                let mut s = state.lock().unwrap();
                s.sidebar_view = crate::app::SideBarView::Strings;
            }
            "View DEX Stats" => {
                let mut s = state.lock().unwrap();
                s.sidebar_view = crate::app::SideBarView::NativeAnalysis;
            }
            "Open App Studio" => {
                let mut s = state.lock().unwrap();
                s.sidebar_view = crate::app::SideBarView::AppStudio;
            }
            _ => {
                let mut s = state.lock().unwrap();
                s.push_log(crate::app::LogLevel::Warn, &format!("Unknown command palette action: {}", label));
            }
        }
    }

    fn spawn_app_task(
        state: &Arc<Mutex<AppState>>,
        label: &'static str,
        task: fn(&Arc<Mutex<AppState>>) -> anyhow::Result<()>,
    ) {
        let state_c = Arc::clone(state);
        std::thread::spawn(move || {
            if let Err(e) = task(&state_c) {
                let mut s = state_c.lock().unwrap();
                s.push_log(crate::app::LogLevel::Error, &format!("{} failed: {}", label, e));
                s.status_message = format!("{} failed", label);
                s.busy = false;
            }
        });
    }
}
