//! Top toolbar — VS Code-style compact command bar with icon-label buttons.

use crate::app::{AppState, LogLevel};
use crate::engine::apk::ApkProcessor;
use crate::ui::theme::Theme;

use std::sync::{Arc, Mutex};

pub struct Toolbar;

impl Toolbar {
    pub fn new() -> Self {
        Self
    }

    pub fn render(
        &mut self,
        ui: &mut egui::Ui,
        state: &Arc<Mutex<AppState>>,
        rt: &tokio::runtime::Runtime,
    ) {
        let theme = Theme::current(ui);

        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;

            // ── File Operations ──
            let can_open_apk = { !state.lock().unwrap().busy };
            Self::toolbar_button(ui, &theme, "Open", can_open_apk,
                "Open an APK or XAPK file for analysis (Ctrl+O)")
                .map(|_| {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("APK / XAPK Files", &["apk", "xapk"])
                        .add_filter("All Files", &["*"])
                        .pick_file()
                    {
                        let state_c = Arc::clone(state);
                        let path_c = path.clone();
                        rt.spawn(async move {
                            let state_blocking = Arc::clone(&state_c);
                            let result = tokio::task::spawn_blocking(move || {
                                ApkProcessor::open_apk(&state_blocking, &path_c)
                            })
                            .await;
                            match result {
                                Ok(Ok(())) => {}
                                Ok(Err(e)) => {
                                    let mut s = state_c.lock().unwrap();
                                    s.push_log(LogLevel::Error, &format!("APK open failed: {}", e));
                                    s.status_message = "APK open failed".into();
                                    s.busy = false;
                                }
                                Err(e) => {
                                    log::error!("Task join error: {}", e);
                                }
                            }
                        });
                    }
                });

            Self::toolbar_button(ui, &theme, "Project", true,
                "Open a saved project file")
                .map(|_| {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("RevEng Project", &["json"])
                        .add_filter("All Files", &["*"])
                        .pick_file()
                    {
                        let mut s = state.lock().unwrap();
                        match s.load_project_from_path(&path) {
                            Ok(()) => {
                                s.current_project_path = Some(path.clone());
                                s.status_message = format!("Project loaded: {}", path.display());
                                s.push_log(LogLevel::Info, &format!("Loaded project: {}", path.display()));
                            }
                            Err(e) => {
                                s.push_log(LogLevel::Error, &format!("Project load failed: {}", e));
                                s.status_message = "Project load failed".into();
                            }
                        }
                    }
                });

            let recent_apks = { state.lock().unwrap().recent_apks.clone() };
            if !recent_apks.is_empty() {
                ui.menu_button(
                    egui::RichText::new("Recent")
                        .size(theme.font_ui)
                        .color(theme.text_secondary),
                    |ui| {
                        ui.set_min_width(320.0);
                        for path in recent_apks {
                            let label = path
                                .file_name()
                                .and_then(|name| name.to_str())
                                .unwrap_or("APK");
                            let response = ui.button(label).on_hover_text(path.display().to_string());
                            if response.clicked() {
                                ui.close_menu();
                                if state.lock().unwrap().busy {
                                    state.lock().unwrap().push_log(
                                        LogLevel::Warn,
                                        "Ignored recent APK open because another operation is running.",
                                    );
                                } else {
                                    let state_c = Arc::clone(state);
                                    rt.spawn(async move {
                                        let state_blocking = Arc::clone(&state_c);
                                        let result = tokio::task::spawn_blocking(move || {
                                            ApkProcessor::open_apk(&state_blocking, &path)
                                        })
                                        .await;
                                        match result {
                                            Ok(Ok(())) => {}
                                            Ok(Err(e)) => {
                                                let mut s = state_c.lock().unwrap();
                                                s.push_log(LogLevel::Error, &format!("APK open failed: {}", e));
                                                s.status_message = "APK open failed".into();
                                                s.busy = false;
                                            }
                                            Err(e) => {
                                                log::error!("Task join error: {}", e);
                                            }
                                        }
                                    });
                                }
                            }
                        }
                    },
                );
            }

            Self::render_separator(ui, &theme);

            // ── APK Pipeline ──
            let can_decode = {
                let s = state.lock().unwrap();
                s.workspace.has_apk() && !s.busy
            };
            Self::toolbar_button(ui, &theme, "Decode", can_decode,
                "Decode APK with APKTool (Smali + resources)")
                .map(|_| {
                    let state_c = Arc::clone(state);
                    rt.spawn(async move {
                        let state_blocking = Arc::clone(&state_c);
                        let result = tokio::task::spawn_blocking(move || {
                            ApkProcessor::decode_apk(&state_blocking)
                        })
                        .await;
                        if let Ok(Err(e)) = result {
                            let mut s = state_c.lock().unwrap();
                            s.push_log(LogLevel::Error, &format!("Decode failed: {}", e));
                            s.busy = false;
                        }
                    });
                });

            let can_decompile = {
                let s = state.lock().unwrap();
                s.workspace.has_apk() && !s.busy
            };
            Self::toolbar_button(ui, &theme, "Decompile", can_decompile,
                "Decompile APK to Java source with JADX")
                .map(|_| {
                    let state_c = Arc::clone(state);
                    rt.spawn(async move {
                        let state_blocking = Arc::clone(&state_c);
                        let result = tokio::task::spawn_blocking(move || {
                            ApkProcessor::decompile_apk(&state_blocking)
                        })
                        .await;
                        if let Ok(Err(e)) = result {
                            let mut s = state_c.lock().unwrap();
                            s.push_log(LogLevel::Error, &format!("Decompile failed: {}", e));
                            s.busy = false;
                        }
                    });
                });

            Self::render_separator(ui, &theme);

            // ── Build & Sign ──
            let can_build = {
                let s = state.lock().unwrap();
                s.workspace.decoded_dir().is_some() && !s.busy
            };
            Self::toolbar_button(ui, &theme, "Build", can_build,
                "Rebuild APK from decoded sources")
                .map(|_| {
                    let state_c = Arc::clone(state);
                    rt.spawn(async move {
                        let state_blocking = Arc::clone(&state_c);
                        let result = tokio::task::spawn_blocking(move || {
                            ApkProcessor::build_apk(&state_blocking)
                        })
                        .await;
                        if let Ok(Err(e)) = result {
                            let mut s = state_c.lock().unwrap();
                            s.push_log(LogLevel::Error, &format!("Build failed: {}", e));
                            s.busy = false;
                        }
                    });
                });

            let can_sign = {
                let s = state.lock().unwrap();
                s.workspace.build_dir().is_some() && !s.busy
            };
            Self::toolbar_button(ui, &theme, "Sign", can_sign,
                "Sign the built APK")
                .map(|_| {
                    let state_c = Arc::clone(state);
                    rt.spawn(async move {
                        let state_blocking = Arc::clone(&state_c);
                        let result = tokio::task::spawn_blocking(move || {
                            ApkProcessor::sign_apk(&state_blocking)
                        })
                        .await;
                        if let Ok(Err(e)) = result {
                            let mut s = state_c.lock().unwrap();
                            s.push_log(LogLevel::Error, &format!("Sign failed: {}", e));
                            s.busy = false;
                        }
                    });
                });

            Self::render_separator(ui, &theme);

            // ── Device ──
            let can_install = { !state.lock().unwrap().busy };
            Self::toolbar_button(ui, &theme, "Install", can_install,
                "Install APK on device via ADB")
                .map(|_| {
                    let state_c = Arc::clone(state);
                    rt.spawn(async move {
                        let state_blocking = Arc::clone(&state_c);
                        let result = tokio::task::spawn_blocking(move || {
                            crate::runtime::adb::AdbManager::install_apk(&state_blocking)
                        })
                        .await;
                        if let Ok(Err(e)) = result {
                            let mut s = state_c.lock().unwrap();
                            s.push_log(LogLevel::Error, &format!("Install failed: {}", e));
                            s.busy = false;
                        }
                    });
                });

            Self::toolbar_button(ui, &theme, "Logcat", true,
                "Start real-time Logcat streaming")
                .map(|_| {
                    crate::runtime::adb::AdbManager::stream_logcat(Arc::clone(state));
                });

            // ── Settings group (right-aligned) ──
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;

                // Help
                Self::toolbar_icon_button(ui, &theme, "?", "Help & Shortcuts")
                    .map(|_| {
                        let mut s = state.lock().unwrap();
                        s.show_help = !s.show_help;
                    });

                // Tools
                Self::toolbar_icon_button(ui, &theme, "TOOLS", "Refresh Toolchain")
                    .map(|_| {
                        let mut s = state.lock().unwrap();
                        let results = s.toolchain.verify_all();
                        for (tool, ok) in results {
                            if ok {
                                s.push_log(LogLevel::Info, &format!("Tool found: {}", tool));
                            } else {
                                let required = s.toolchain.get(&tool)
                                    .map(|info| info.required == crate::engine::toolchain::ToolRequirement::Required)
                                    .unwrap_or(true);
                                if required {
                                    s.push_log(LogLevel::Warn, &format!("Tool missing: {}", tool));
                                } else {
                                    s.push_log(LogLevel::Info, &format!("Optional tool missing: {}", tool));
                                }
                            }
                            if let Some(tip) = crate::engine::toolchain::ToolchainManager::get_tool_tip(&tool) {
                                s.push_log(LogLevel::Debug, tip);
                            }
                        }
                    });

                Self::toolbar_icon_button(ui, &theme, "SAVE", "Save current project")
                    .map(|_| {
                        let path = { state.lock().unwrap().current_project_path.clone() };
                        let mut dialog = rfd::FileDialog::new()
                            .add_filter("RevEng Project", &["json"])
                            .add_filter("All Files", &["*"]);
                        if let Some(existing) = path.as_ref() {
                            if let Some(parent) = existing.parent() {
                                dialog = dialog.set_directory(parent);
                            }
                            if let Some(name) = existing.file_name().and_then(|n| n.to_str()) {
                                dialog = dialog.set_file_name(name);
                            }
                        }
                        if let Some(save_path) = dialog.save_file() {
                            let mut s = state.lock().unwrap();
                            match s.save_project_to_path(&save_path) {
                                Ok(()) => {
                                    s.current_project_path = Some(save_path.clone());
                                    s.status_message = format!("Project saved: {}", save_path.display());
                                    s.push_log(LogLevel::Info, &format!("Saved project: {}", save_path.display()));
                                }
                                Err(e) => {
                                    s.push_log(LogLevel::Error, &format!("Project save failed: {}", e));
                                    s.status_message = "Project save failed".into();
                                }
                            }
                        }
                    });

                // Theme toggle
                {
                    let dark_mode = state.lock().unwrap().dark_mode;
                    let icon = if dark_mode { "LIGHT" } else { "DARK" };
                    Self::toolbar_icon_button(ui, &theme, icon, "Toggle Theme")
                        .map(|_| {
                            let mut s = state.lock().unwrap();
                            s.dark_mode = !s.dark_mode;
                            s.settings.dark_mode = s.dark_mode;
                            s.save_settings();
                        });
                }

                Self::render_separator(ui, &theme);

                // Frida
                Self::toolbar_icon_button(ui, &theme, "FRIDA", "Frida Runtime")
                    .map(|_| {
                        state.lock().unwrap().sidebar_view = crate::app::SideBarView::Runtime;
                    });
            });
        });
    }

    fn toolbar_button(
        ui: &mut egui::Ui,
        theme: &Theme,
        label: &str,
        enabled: bool,
        tooltip: &str,
    ) -> Option<()> {
        let text_color = if enabled { theme.text_primary } else { theme.text_disabled };
        let desired = egui::vec2(48.0, 28.0);
        let (rect, resp) = ui.allocate_exact_size(desired, egui::Sense::click());

        if enabled && resp.hovered() {
            ui.painter().rect_filled(rect, 2.0, theme.tree_hover_bg);
        }

        ui.put(
            rect,
            egui::Label::new(
                egui::RichText::new(label)
                    .size(theme.font_ui)
                    .color(text_color),
            )
            .selectable(false),
        );

        let resp = resp.on_hover_text(tooltip);

        if enabled && resp.clicked() { Some(()) } else { None }
    }

    fn toolbar_icon_button(
        ui: &mut egui::Ui,
        theme: &Theme,
        icon: &str,
        tooltip: &str,
    ) -> Option<()> {
        let desired = egui::vec2(42.0, 28.0);
        let (rect, resp) = ui.allocate_exact_size(desired, egui::Sense::click());

        if resp.hovered() {
            ui.painter().rect_filled(rect, 4.0, theme.tree_hover_bg);
        }

        ui.put(
            rect,
            egui::Label::new(
                egui::RichText::new(icon)
                    .size(theme.font_small)
                    .strong()
                    .color(theme.text_secondary),
            )
            .selectable(false),
        );

        let resp = resp.on_hover_text(tooltip);

        if resp.clicked() { Some(()) } else { None }
    }

    fn render_separator(ui: &mut egui::Ui, theme: &Theme) {
        ui.add_space(2.0);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(1.0, 18.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 0.0, theme.separator);
        ui.add_space(2.0);
    }
}
