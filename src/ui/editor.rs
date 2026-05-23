//! Code editor panel with tabs, syntax highlighting, line numbers, and search.

use crate::app::{AppState, FileLanguage, TabContent};
use crate::ui::theme::Theme;

use std::sync::{Arc, Mutex};

/// Actions triggered from xref context menu.
enum XrefAction {
    FindCallers(String),
    FindUsages(String),
    ShowHierarchy(String),
}

pub struct EditorPanel;

impl EditorPanel {
    pub fn new() -> Self {
        Self
    }

    pub fn render(&mut self, ui: &mut egui::Ui, state: &Arc<Mutex<AppState>>) {
        let theme = Theme::current(ui);
        let full_rect = ui.available_rect_before_wrap();
        ui.painter().rect_filled(full_rect, 0.0, theme.editor_bg);

        let mut s = state.lock().unwrap();

        if s.open_tabs.is_empty() {
            self.render_welcome(ui, &theme);
            return;
        }

        let mut tab_to_close = None;
        let mut clicked_tab = None;

        // ── Tab bar ──
        let tab_bar_frame = egui::Frame::NONE
            .fill(theme.tab_inactive_bg)
            .inner_margin(egui::Margin { left: 0, right: 0, top: 0, bottom: 0 });

        tab_bar_frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;

                for (i, tab) in s.open_tabs.iter().enumerate() {
                    let is_active = s.active_tab == Some(i);

                    let bg = if is_active {
                        theme.tab_active_bg
                    } else {
                        theme.tab_inactive_bg
                    };

                    let text_color = if is_active {
                        theme.tab_active_text
                    } else {
                        theme.tab_inactive_text
                    };

                    // File type badge color
                    let ext = tab.path.extension().and_then(|e| e.to_str()).unwrap_or("");
                    let (badge_char, badge_color) = theme.file_icon(ext);

                    let frame = egui::Frame::NONE
                        .fill(bg)
                        .inner_margin(egui::Margin::symmetric(12, 6));

                    frame.show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;

                            // File type badge
                            let badge_frame = egui::Frame::NONE
                                .fill(badge_color.linear_multiply(0.15))
                                .corner_radius(egui::CornerRadius::same(2))
                                .inner_margin(egui::Margin::symmetric(3, 1));
                            badge_frame.show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new(badge_char)
                                        .size(9.0)
                                        .strong()
                                        .color(badge_color),
                                );
                            });

                            // Tab label
                            let tab_response = ui.label(
                                egui::RichText::new(&tab.title)
                                    .size(12.0)
                                    .color(text_color),
                            );
                            if tab_response.clicked() {
                                clicked_tab = Some(i);
                            }

                            // Modified dot
                            if tab.modified {
                                ui.label(
                                    egui::RichText::new("\u{2022}")
                                        .size(14.0)
                                        .color(theme.warning),
                                );
                            }

                            // Close button
                            let close = ui.add(
                                egui::Button::new(
                                    egui::RichText::new("\u{00d7}")
                                        .size(12.0)
                                        .color(theme.text_muted),
                                )
                                .frame(false),
                            );
                            if close.clicked() {
                                tab_to_close = Some(i);
                            }
                        });
                    });

                    // Active tab top accent border
                    if is_active {
                        let last_rect = ui.min_rect();
                        let accent_rect = egui::Rect::from_min_size(
                            last_rect.left_top(),
                            egui::vec2(last_rect.width(), 2.0),
                        );
                        ui.painter().rect_filled(accent_rect, 0.0, theme.tab_accent);
                    }
                }
            });
        });

        // Tab bar bottom border
        let tab_bottom = ui.cursor().min;
        let border_rect = egui::Rect::from_min_size(
            egui::pos2(ui.min_rect().left(), tab_bottom.y),
            egui::vec2(ui.available_width(), 1.0),
        );
        ui.painter().rect_filled(border_rect, 0.0, theme.tab_border);
        ui.add_space(1.0);

        if let Some(idx) = clicked_tab {
            s.active_tab = Some(idx);
        }
        if let Some(idx) = tab_to_close {
            s.close_tab(idx);
            return;
        }

        Self::render_editor_action_bar(ui, &mut s, &theme);

        let mut save_requested = false;
        let mut jump_request: Option<(std::path::PathBuf, usize)> = None;
        let mut reverse_jump_request: Option<(std::path::PathBuf, usize)> = None;
        let mut xref_request: Option<XrefAction> = None;
        let mut native_patch_path: Option<std::path::PathBuf> = None;
        let mut hex_patch_req: Option<(std::path::PathBuf, String, String)> = None;

        if !matches!(s.active_tab, Some(idx) if idx < s.open_tabs.len()) {
            s.active_tab = Some(0);
        }
        if matches!(s.active_tab_right, Some(idx) if idx >= s.open_tabs.len()) {
            s.active_tab_right = None;
        }

        let left_idx = s.active_tab;
        s.active_tab_right = None;
        let settings = s.settings.clone();
        let (completion_classes, completion_methods) = if let Some(db) = &s.xref_db {
            let classes = db.classes.keys().cloned().collect::<Vec<_>>();
            let mut methods = db
                .classes
                .values()
                .flat_map(|class| class.methods.iter().map(|method| method.name.clone()))
                .collect::<Vec<_>>();
            methods.sort();
            methods.dedup();
            (classes, methods)
        } else {
            (Vec::new(), Vec::new())
        };

        if let Some(left) = left_idx {
            if let Some(tab) = s.open_tabs.get_mut(left) {
                Self::render_tab(ui, tab, &settings, &completion_classes, &completion_methods, &mut save_requested, &mut jump_request, &mut reverse_jump_request, &mut xref_request, &mut native_patch_path, &mut hex_patch_req);
            }
        }

        drop(s);

        if save_requested {
            let mut s = state.lock().unwrap();
            let _ = s.save_active_tab();
        }

        if let Some((path, line_num)) = jump_request {
            let mut s = state.lock().unwrap();
            s.jump_to_smali(&path, line_num);
        }

        // Ctrl+K — Reverse nav: Smali -> Java
        if let Some((path, line_num)) = reverse_jump_request {
            let mut s = state.lock().unwrap();
            s.jump_to_java(&path, line_num);
        }

        // Flutter SSL patch
        if let Some(lib_path) = native_patch_path {
            let state_c = Arc::clone(state);
            std::thread::spawn(move || {
                {
                    let mut s = state_c.lock().unwrap();
                    s.push_log(crate::app::LogLevel::Info, &format!("[Flutter] Patching SSL pinning in {}...", lib_path.display()));
                    s.status_message = "Patching Flutter SSL...".into();
                }
                match crate::native::flutter_patch::FlutterPatcher::bypass_ssl_pinning(&lib_path) {
                    Ok(result) => {
                        let mut s = state_c.lock().unwrap();
                        let level = if result.patched { crate::app::LogLevel::Info } else { crate::app::LogLevel::Warn };
                        s.push_log(level, &result.message);
                        s.push_log(crate::app::LogLevel::Debug, &format!("[Flutter] Detected architecture: {}", result.arch));
                        for patch in &result.patches_applied {
                            let original = patch.original.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ");
                            let patched = patch.patched.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ");
                            s.push_log(
                                crate::app::LogLevel::Debug,
                                &format!(
                                    "  Patch @ {:#x}: {} [{} -> {}]",
                                    patch.offset, patch.description, original, patched
                                ),
                            );
                        }
                        s.status_message = if result.patched { "Flutter SSL patched".into() } else { "SSL patch: no patterns found".into() };
                    }
                    Err(e) => {
                        let mut s = state_c.lock().unwrap();
                        s.push_log(crate::app::LogLevel::Error, &format!("[Flutter] Patch failed: {}", e));
                        s.status_message = "Flutter patch failed".into();
                    }
                }
            });
        }

        if let Some((path, find_hex, replace_hex)) = hex_patch_req {
            let state_c = Arc::clone(state);
            std::thread::spawn(move || {
                {
                    let mut s = state_c.lock().unwrap();
                    s.push_log(crate::app::LogLevel::Info, &format!("Patching hex in {}...", path.display()));
                    s.status_message = "Patching...".into();
                }
                let result: Result<usize, &'static str> = match (Self::decode_hex_input(&find_hex), Self::decode_hex_input(&replace_hex)) {
                    (Some(find_bytes), Some(replace_bytes)) => {
                        if find_bytes.is_empty() {
                            Err("Find hex cannot be empty")
                        } else if let Ok(data) = std::fs::read(&path) {
                            let mut patch_count = 0;
                            let mut new_data = Vec::with_capacity(data.len());
                            let mut i = 0;
                            while i < data.len() {
                                if i <= data.len().saturating_sub(find_bytes.len()) && &data[i..i+find_bytes.len()] == find_bytes.as_slice() {
                                    new_data.extend_from_slice(&replace_bytes);
                                    patch_count += 1;
                                    i += find_bytes.len();
                                } else {
                                    new_data.push(data[i]);
                                    i += 1;
                                }
                            }
                            if patch_count > 0 {
                                if std::fs::write(&path, new_data).is_ok() { Ok(patch_count) } else { Err("Write failed") }
                            } else { Err("No hex matches found") }
                        } else { Err("Cannot read file") }
                    }
                    _ => Err("Invalid hex string (ensure even # of chars)"),
                };
                let mut s = state_c.lock().unwrap();
                match result {
                    Ok(count) => {
                        s.push_log(crate::app::LogLevel::Info, &format!("Applied {} hex patches in {}.", count, path.display()));
                        s.status_message = format!("Patched {} locations", count);
                    }
                    Err(e) => {
                        s.push_log(crate::app::LogLevel::Error, &format!("Hex patch failed: {}", e));
                        s.status_message = "Hex patch failed".into();
                    }
                }
            });
        }

        // Handle xref actions
        if let Some(action) = xref_request {
            let mut s = state.lock().unwrap();
            match action {
                XrefAction::FindCallers(method_sig) => {
                    if let Some(ref db) = s.xref_db {
                        let mut results: Vec<crate::engine::xref::CodeSite> = Vec::new();
                        for (method, _) in db.method_callers.iter().filter(|(method, _)| {
                            method.full_signature().contains(&method_sig)
                                || method.short_name().contains(&method_sig)
                                || method.name == method_sig
                        }) {
                            results.extend(db.find_callers(method).into_iter().cloned());
                        }

                        if let Some((class, rest)) = method_sig.split_once("->") {
                            let name = rest.split('(').next().unwrap_or(rest);
                            for (_, sites) in db.find_callers_by_name(class.trim(), name.trim()) {
                                results.extend_from_slice(sites);
                            }
                        }

                        let count = results.len();
                        s.xref_results = results;
                        s.sidebar_view = crate::app::SideBarView::Search;
                        s.push_log(crate::app::LogLevel::Info, &format!("Found {} callers of '{}'", count, method_sig));
                    } else {
                        s.push_log(crate::app::LogLevel::Warn, "Xref database not built. Decode APK first.");
                    }
                }
                XrefAction::FindUsages(token) => {
                    // Search type refs + string refs + method refs
                    if let Some(ref db) = s.xref_db {
                        let mut results: Vec<crate::engine::xref::CodeSite> = Vec::new();

                        let normalized_type = if token.starts_with('L') {
                            token.clone()
                        } else if token.contains('.') {
                            format!("L{};", token.replace('.', "/"))
                        } else {
                            String::new()
                        };
                        if !normalized_type.is_empty() {
                            results.extend(db.find_type_usages(&normalized_type).into_iter().cloned());
                        }
                        results.extend(db.find_string_usages(&token).into_iter().cloned());

                        for (type_name, sites) in &db.type_refs {
                            if type_name.contains(&token) {
                                results.extend(sites.clone());
                            }
                        }
                        for (val, sites) in &db.string_refs {
                            if val.contains(&token) {
                                results.extend(sites.clone());
                            }
                        }
                        for (method, sites) in &db.method_callers {
                            if method.name == token
                                || method.full_signature().contains(&token)
                                || method.short_name().contains(&token)
                            {
                                results.extend(sites.clone());
                            }
                        }
                        for (field, sites) in db.field_reads.iter().chain(db.field_writes.iter()) {
                            if field.name == token || field.full_signature().contains(&token) {
                                results.extend(sites.clone());
                            }
                        }

                        let count = results.len();
                        s.xref_results = results;
                        s.sidebar_view = crate::app::SideBarView::Search;
                        s.push_log(crate::app::LogLevel::Info, &format!("Found {} usages of '{}'", count, token));
                    } else {
                        s.push_log(crate::app::LogLevel::Warn, "Xref database not built. Decode APK first.");
                    }
                }
                XrefAction::ShowHierarchy(class_name) => {
                    if let Some(ref db) = s.xref_db {
                        let chain = db.get_class_hierarchy(&class_name);
                        let subs: Vec<String> = db.find_subclasses(&class_name)
                            .iter().map(|c| c.name.clone()).collect();
                        let impls: Vec<String> = db.find_implementors(&class_name)
                            .iter().map(|c| c.name.clone()).collect();

                        let mut msg = format!("Class hierarchy for {}:\n", class_name);
                        if let Some(info) = db.classes.get(&class_name) {
                            let flags = [
                                (info.is_interface, "interface"),
                                (info.is_abstract, "abstract"),
                            ]
                            .into_iter()
                            .filter_map(|(enabled, label)| enabled.then_some(label))
                            .collect::<Vec<_>>()
                            .join(", ");
                            msg.push_str(&format!(
                                "  Defined in: {}\n  Methods: {} | Fields: {}{}\n",
                                info.file.display(),
                                info.methods.len(),
                                info.fields.len(),
                                if flags.is_empty() { String::new() } else { format!(" | {}", flags) }
                            ));
                        }
                        if !chain.is_empty() {
                            msg.push_str(&format!("  Supers: {}\n", chain.join(" -> ")));
                        }
                        if !subs.is_empty() {
                            msg.push_str(&format!("  Subclasses: {}\n", subs.join(", ")));
                        }
                        if !impls.is_empty() {
                            msg.push_str(&format!("  Implementors: {}\n", impls.join(", ")));
                        }
                        s.push_log(crate::app::LogLevel::Info, &msg);
                    } else {
                        s.push_log(crate::app::LogLevel::Warn, "Xref database not built. Decode APK first.");
                    }
                }
            }
        }
    }

    fn render_editor_action_bar(ui: &mut egui::Ui, state: &mut AppState, theme: &Theme) {
        let Some(active_idx) = state.active_tab.filter(|idx| *idx < state.open_tabs.len()) else {
            return;
        };

        let (title, path, language, modified, can_split) = {
            let tab = &state.open_tabs[active_idx];
            (
                tab.title.clone(),
                tab.path.display().to_string(),
                tab.language.label().to_string(),
                tab.modified,
                state.open_tabs.len() > 1,
            )
        };

        let width = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 30.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 0.0, theme.bg_secondary);
        ui.painter().line_segment(
            [rect.left_bottom(), rect.right_bottom()],
            egui::Stroke::new(1.0, theme.tab_border),
        );

        let content_rect = rect.shrink2(egui::vec2(10.0, 4.0));
        let mut strip_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(content_rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );

        strip_ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;

                    ui.label(
                        egui::RichText::new("EDITOR")
                            .size(theme.font_small - 1.0)
                            .strong()
                            .color(theme.text_muted),
                    );
                    ui.label(
                        egui::RichText::new(title)
                            .size(theme.font_small)
                            .strong()
                            .color(theme.text_primary),
                    );
                    if modified {
                        ui.label(
                            egui::RichText::new("modified")
                                .size(theme.font_small - 1.0)
                                .color(theme.warning),
                        );
                    }
                    ui.label(
                        egui::RichText::new(language)
                            .size(theme.font_small - 1.0)
                            .color(theme.text_muted),
                    );
                    ui.label(
                        egui::RichText::new(Self::truncate_start_chars(&path, 72))
                            .size(theme.font_small - 1.0)
                            .color(theme.text_disabled),
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.menu_button(
                            egui::RichText::new("More")
                                .size(theme.font_ui)
                                .color(theme.text_secondary),
                            |ui| {
                                if ui.button("Close Other Editors").clicked() {
                                    ui.close_menu();
                                    let keep = state.open_tabs[active_idx].clone();
                                    state.open_tabs = vec![keep];
                                    state.active_tab = Some(0);
                                    state.active_tab_right = None;
                                }
                                if ui.button("Close Saved Editors").clicked() {
                                    ui.close_menu();
                                    let active_path = state.open_tabs[active_idx].path.clone();
                                    state.open_tabs.retain(|tab| tab.modified || tab.path == active_path);
                                    state.active_tab = state
                                        .open_tabs
                                        .iter()
                                        .position(|tab| tab.path == active_path)
                                        .or(Some(0));
                                    state.active_tab_right = None;
                                }
                            },
                        );

                        if Self::action_button(ui, theme, "Split", "Open the next editor beside this one").clicked() {
                            state.active_tab_right = None;
                            if can_split {
                                state.status_message = "Split view is temporarily disabled while fixing the layout glitch.".into();
                            } else {
                                state.status_message = "Open another tab to view it side by side later.".into();
                            }
                        }
                        if Self::action_button(ui, theme, "Find", "Find in active file").clicked() {
                            if let Some(tab) = state.open_tabs.get_mut(active_idx) {
                                tab.search_visible = true;
                            }
                        }
                        if Self::action_button(ui, theme, "Save", "Save active file").clicked() {
                            let _ = state.save_active_tab();
                        }
                    });
        });
    }

    fn action_button(ui: &mut egui::Ui, theme: &Theme, label: &str, tooltip: &str) -> egui::Response {
        ui.add(
            egui::Button::new(
                egui::RichText::new(label)
                    .size(theme.font_small)
                    .color(theme.text_secondary),
            )
            .frame(false)
            .min_size(egui::vec2(44.0, 20.0)),
        )
        .on_hover_text(tooltip)
    }

    fn truncate_start_chars(text: &str, max_chars: usize) -> String {
        let len = text.chars().count();
        if len <= max_chars {
            text.to_string()
        } else {
            format!("...{}", text.chars().skip(len - max_chars).collect::<String>())
        }
    }

    fn render_breadcrumb(ui: &mut egui::Ui, path: &std::path::Path, theme: &Theme) {
        let breadcrumb_frame = egui::Frame::NONE
            .fill(theme.bg_secondary)
            .inner_margin(egui::Margin { left: 12, right: 12, top: 6, bottom: 6 })
            .stroke(egui::Stroke::new(1.0, theme.tab_border));

        breadcrumb_frame.show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;

                let path_str = path.to_string_lossy();
                let components: Vec<&str> = path_str.split('/').collect();

                for (i, component) in components.iter().enumerate() {
                    if component.is_empty() {
                        continue;
                    }

                    if i > 0 {
                        ui.label(
                            egui::RichText::new("/")
                                .size(theme.font_small)
                                .color(theme.text_muted),
                        );
                    }

                    // Style: different color for filename vs directories
                    let is_file = i == components.len() - 1;
                    let text_color = if is_file {
                        theme.text_primary
                    } else {
                        theme.text_muted
                    };

                    let button = ui.add(
                        egui::Button::new(
                            egui::RichText::new(*component)
                                .color(text_color)
                                .size(theme.font_small - 1.0),
                        )
                        .frame(false)
                        .sense(egui::Sense::hover()),
                    );

                    // Show full path on hover
                    if button.hovered() {
                        button.on_hover_text(path_str.as_ref());
                    }
                }
            });
        });
    }

    fn render_tab(
        ui: &mut egui::Ui,
        tab: &mut crate::app::EditorTab,
        settings: &crate::app::AppSettings,
        completion_classes: &[String],
        completion_methods: &[String],
        save_requested: &mut bool,
        jump_request: &mut Option<(std::path::PathBuf, usize)>,
        reverse_jump_request: &mut Option<(std::path::PathBuf, usize)>,
        xref_request: &mut Option<XrefAction>,
        native_patch_path: &mut Option<std::path::PathBuf>,
        hex_patch_req: &mut Option<(std::path::PathBuf, String, String)>,
    ) {
        let theme = Theme::current(ui);
        let language = tab.language.clone();

        // Floating search bar (VS Code-style)
        if tab.search_visible {
            let avail = ui.available_rect_before_wrap();
            let search_width = 420.0_f32.min(avail.width() - 20.0);

            let frame = egui::Frame::NONE
                .fill(theme.bg_elevated)
                .corner_radius(egui::CornerRadius::same(theme.corner_radius as u8))
                .stroke(egui::Stroke::new(1.0, theme.border))
                .shadow(egui::Shadow {
                    offset: [0, 2],
                    blur: 8,
                    spread: 0,
                    color: egui::Color32::from_black_alpha(40),
                })
                .inner_margin(egui::Margin::symmetric(10, 6));

            let search_rect = egui::Rect::from_min_size(
                egui::pos2(avail.right() - search_width - 16.0, avail.top() + 4.0),
                egui::vec2(search_width, if tab.replace_visible { 62.0 } else { 32.0 }),
            );

            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(search_rect), |ui| {
                frame.show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        ui.add(
                            egui::TextEdit::singleline(&mut tab.search_query)
                                .hint_text("Find...")
                                .desired_width(search_width - 180.0)
                                .font(egui::TextStyle::Small),
                        );

                        // Match count
                        if !tab.search_query.is_empty() {
                            if let TabContent::Code(text) = &tab.content {
                                let count = text.matches(&tab.search_query).count();
                                ui.label(
                                    egui::RichText::new(format!("{} matches", count))
                                        .size(theme.font_small)
                                        .color(theme.text_muted),
                                );
                            }
                        }

                        if ui.add(
                            egui::Button::new(egui::RichText::new("R").size(10.0).color(
                                if tab.replace_visible { theme.accent_primary } else { theme.text_muted }
                            )).frame(false)
                        ).on_hover_text("Toggle Replace").clicked() {
                            tab.replace_visible = !tab.replace_visible;
                        }
                        if ui.add(
                            egui::Button::new(egui::RichText::new("\u{00d7}").size(12.0).color(theme.text_muted)).frame(false)
                        ).clicked() {
                            tab.search_visible = false;
                        }
                    });

                    if tab.replace_visible {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            ui.add(
                                egui::TextEdit::singleline(&mut tab.replace_query)
                                    .hint_text("Replace...")
                                    .desired_width(search_width - 180.0)
                                    .font(egui::TextStyle::Small),
                            );
                            if ui.add(
                                egui::Button::new(egui::RichText::new("Replace All").size(theme.font_small).color(theme.text_primary))
                                    .corner_radius(3.0)
                            ).clicked() {
                                let find = tab.search_query.clone();
                                let replace = tab.replace_query.clone();
                                if let TabContent::Code(text) = &mut tab.content {
                                    *text = text.replace(&find, &replace);
                                    tab.modified = true;
                                }
                            }
                        });
                    }
                });
            });
        }

        // Keyboard shortcuts
        if ui.input(|i| i.key_pressed(egui::Key::F) && i.modifiers.ctrl) {
            tab.search_visible = !tab.search_visible;
        }
        if ui.input(|i| i.key_pressed(egui::Key::S) && i.modifiers.ctrl) {
            *save_requested = true;
        }
        if language == FileLanguage::Java
            && ui.input(|i| i.key_pressed(egui::Key::J) && i.modifiers.ctrl)
        {
            let content = if let TabContent::Code(text) = &tab.content {
                text.as_str()
            } else {
                ""
            };
            let cursor_pos = Self::clamp_to_char_boundary(content, tab.cursor_pos);
            let line_idx = content[..cursor_pos].lines().count();
            *jump_request = Some((tab.path.clone(), line_idx + 1));
        }

        // Ctrl+K — Reverse nav: Smali -> Java
        if language == FileLanguage::Smali
            && ui.input(|i| i.key_pressed(egui::Key::K) && i.modifiers.ctrl)
        {
            let content = if let TabContent::Code(text) = &tab.content {
                text.as_str()
            } else {
                ""
            };
            let cursor_pos = Self::clamp_to_char_boundary(content, tab.cursor_pos);
            let line_idx = content[..cursor_pos].lines().count();
            *reverse_jump_request = Some((tab.path.clone(), line_idx + 1));
        }

        // Shift+F12 — Find Usages
        if (language == FileLanguage::Smali || language == FileLanguage::Java)
            && ui.input(|i| i.key_pressed(egui::Key::F12) && i.modifiers.shift)
        {
            let content = if let TabContent::Code(text) = &tab.content {
                text.as_str()
            } else {
                ""
            };
            let word = Self::extract_word_at(content, Self::clamp_to_char_boundary(content, tab.cursor_pos));
            if !word.is_empty() {
                *xref_request = Some(XrefAction::FindUsages(word));
            }
        }

        // Smali validation markers
        if language == FileLanguage::Smali {
            let source = if let TabContent::Code(text) = &tab.content {
                text.as_str()
            } else {
                ""
            };
            let errors = crate::engine::smali_validator::SmaliValidator::validate(source);
            if !errors.is_empty() {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("!")
                            .color(theme.warning)
                            .strong(),
                    );
                    ui.label(
                        egui::RichText::new(format!("{} syntax issues", errors.len()))
                            .size(theme.font_small)
                            .color(theme.text_muted),
                    );
                });
            }
        }

        // Breadcrumb path display
        Self::render_breadcrumb(ui, &tab.path, &theme);

        let syntect_name = language.syntect_name().to_owned();

        // Handle scroll-to-line request
        let mut preset_scroll = None;
        if let Some(line) = tab.target_line.take() {
            let lh = 15.6_f32;
            let offset = (line as f32) * lh - (ui.available_height() / 2.0);
            preset_scroll = Some(offset.max(0.0));
        }

        let mut text_changed = false;
        match &mut tab.content {
            TabContent::Native(info, insns) => {
                let mut action = crate::ui::native_view::NativeAction::None;
                egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
                    action = crate::ui::native_view::NativeView::render(ui, info, insns, &mut tab.search_query, &mut tab.replace_query);
                });
                match action {
                    crate::ui::native_view::NativeAction::AutoPatchSsl => *native_patch_path = Some(tab.path.clone()),
                    crate::ui::native_view::NativeAction::FindReplaceBytes(f, r) => *hex_patch_req = Some((tab.path.clone(), f, r)),
                    crate::ui::native_view::NativeAction::None => {},
                }
            }
            TabContent::Hex(bytes) => {
                Self::render_hex(ui, bytes, &theme);
            }
            TabContent::Code(text) => {
                let old_text = text.clone();
                let t_theme = egui_extras::syntax_highlighting::CodeTheme::from_memory(ui.ctx(), ui.style());
                let syntect_name_ref = syntect_name.clone();
                let mut layouter = |ui: &egui::Ui, string: &str, wrap_width: f32| {
                    let mut layout_job = egui_extras::syntax_highlighting::highlight(ui.ctx(), ui.style(), &t_theme, string, &syntect_name_ref);
                    layout_job.wrap.max_width = if settings.word_wrap { wrap_width } else { f32::INFINITY };
                    ui.fonts(|f| f.layout_job(layout_job))
                };

                let line_count = text.lines().count().max(1);
                let gutter_chars = format!("{}", line_count).len();
                let code_font = egui::FontId::monospace(theme.font_code);
                let char_width = ui.fonts(|f| f.glyph_width(&code_font, ' '));
                let gutter_width = (gutter_chars as f32 * char_width) + 28.0;
                let line_height = (theme.font_code * settings.line_height).round();

                // Pre-compute minimap line data before the TextEdit mutable borrow
                //let minimap_lines: Vec<String> = text.lines().map(|l| l.to_string()).collect();

                let avail_size = ui.available_size();
                let editor_w = avail_size.x.max(100.0);

                ui.horizontal_top(|ui| {
                    // ── Editor pane (scrollable) ──
                    ui.allocate_ui_with_layout(
                        egui::vec2(editor_w, avail_size.y),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            let mut scroll = egui::ScrollArea::both()
                                .id_salt(&tab.path)
                                .auto_shrink([false, false]);
                            if let Some(y) = preset_scroll {
                                scroll = scroll.vertical_scroll_offset(y);
                            }
                            let scroll_out = scroll.show(ui, |ui| {
                                ui.horizontal_top(|ui| {
                                    let gutter_height = (line_count as f32 * line_height).max(ui.available_height());
                                    let (gutter_rect, _) = ui.allocate_exact_size(
                                        egui::vec2(gutter_width, gutter_height),
                                        egui::Sense::hover(),
                                    );
                                    let text_left = gutter_rect.right();

                                    ui.painter().rect_filled(
                                        ui.max_rect(),
                                        0.0,
                                        theme.editor_bg,
                                    );

                                    // Paint gutter background
                                    ui.painter().rect_filled(gutter_rect, 0.0, theme.editor_gutter_bg);

                                    // Paint line numbers
                                    let font_id = code_font.clone();
                                    for i in 0..line_count {
                                        let y_pos = gutter_rect.top() + (i as f32 * line_height);
                                        if y_pos > gutter_rect.bottom() + 100.0 { break; }
                                        ui.painter().text(
                                            egui::pos2(gutter_rect.right() - 10.0, y_pos + 1.0),
                                            egui::Align2::RIGHT_TOP,
                                            format!("{}", i + 1),
                                            font_id.clone(),
                                            theme.editor_gutter_text,
                                        );
                                    }

                                    // Gutter separator
                                    ui.painter().line_segment(
                                        [
                                            egui::pos2(gutter_rect.right(), gutter_rect.top()),
                                            egui::pos2(gutter_rect.right(), gutter_rect.bottom()),
                                        ],
                                        egui::Stroke::new(1.0, theme.separator),
                                    );

                                    // ── Current line highlight (drawn before TextEdit as a background) ──
                                    {
                                        let prev_cursor = Self::clamp_to_char_boundary(text, tab.cursor_pos);
                                        let hl_line = text[..prev_cursor].chars().filter(|&c| c == '\n').count();
                                        let hl_y = gutter_rect.top() + hl_line as f32 * line_height;
                                        // Wide enough to cover gutter + editor
                                        let hl_rect = egui::Rect::from_min_size(
                                            egui::pos2(gutter_rect.left(), hl_y),
                                            egui::vec2(gutter_rect.width() + 4000.0, line_height),
                                        );
                                        ui.painter().rect_filled(hl_rect, 0.0, theme.editor_line_highlight);

                                        // Active line number (brighter) drawn over the gutter highlight
                                        if hl_line < line_count {
                                            ui.painter().text(
                                                egui::pos2(gutter_rect.right() - 8.0, hl_y),
                                                egui::Align2::RIGHT_TOP,
                                                format!("{}", hl_line + 1),
                                                code_font.clone(),
                                                theme.text_primary,
                                            );
                                        }
                                    }

                                    // ── Highlight Search Matches ──
                                    if tab.search_visible && !tab.search_query.is_empty() {
                                        let query_chars = tab.search_query.chars().count();
                                        let mut match_count = 0;
                                        for (start_byte, _) in text.match_indices(&tab.search_query) {
                                            match_count += 1;
                                            if match_count > 1000 { break; } // limit to prevent lag
                                            
                                            let safe = start_byte.min(text.len());
                                            let lines_before = text[..safe].chars().filter(|&c| c == '\n').count();
                                            let col_start = text[..safe].rfind('\n').map(|n| n + 1).unwrap_or(0);
                                            let chars_before = text[col_start..safe].chars().count();
                                            
                                            let bx = text_left + chars_before as f32 * char_width;
                                            let by_ = gutter_rect.top() + lines_before as f32 * line_height;
                                            
                                            // Only render if visible in the scroll area vertically
                                            if by_ + line_height > ui.clip_rect().min.y && by_ < ui.clip_rect().max.y {
                                                let br = egui::Rect::from_min_size(
                                                    egui::pos2(bx, by_),
                                                    egui::vec2(query_chars as f32 * char_width, line_height)
                                                );
                                                ui.painter().rect_filled(br, 2.0, theme.editor_find_match);
                                            }
                                        }
                                    }

                                    // ── TextEdit ──
                                    let output = egui::TextEdit::multiline(text)
                                        .font(egui::TextStyle::Monospace)
                                        .code_editor()
                                        .frame(false)
                                        .desired_width(f32::INFINITY)
                                        .lock_focus(true)
                                        .layouter(&mut layouter)
                                        .show(ui);

                                    if let Some(cursor) = output.cursor_range {
                                        // Update cursor position for next frame's line highlight
                                        let cursor_char_idx = cursor.primary.ccursor.index;
                                        let cursor_idx = Self::char_to_byte_index(text, cursor_char_idx);
                                        tab.cursor_pos = cursor_idx;

                                        // ── Bracket matching ──
                                        tab.bracket_match = Self::find_bracket_match(text, cursor_char_idx);
                                        if let Some((pos_a, pos_b)) = tab.bracket_match {
                                            for bpos in [pos_a, pos_b] {
                                                let safe = bpos.min(text.len().saturating_sub(1));
                                                let bl = text[..safe].chars().filter(|&c| c == '\n').count();
                                                let bc = Self::line_char_column(text, safe);
                                                let bx = text_left + bc as f32 * char_width;
                                                let by_ = gutter_rect.top() + bl as f32 * line_height;
                                                let br = egui::Rect::from_min_size(egui::pos2(bx, by_), egui::vec2(char_width, line_height));
                                                ui.painter().rect_filled(br, 0.0, theme.editor_bracket_match_bg);
                                            }
                                        }

                                        let word_under_cursor = Self::extract_word_at(text, cursor_idx);
                                        // Pre-compute nav info for context menu (before closures move things)
                                        let ctx_line = text[..cursor_idx].chars().filter(|&c| c == '\n').count() + 1;
                                        let ctx_path = tab.path.clone();

                                        // Right-click context menu
                                        output.response.context_menu(|ui| {
                                            if !word_under_cursor.is_empty() {
                                                ui.label(egui::RichText::new(format!("\"{}\"", &word_under_cursor))
                                                    .size(11.0).color(theme.text_muted).italics());
                                                ui.separator();
                                            }
                                            if ui.button(egui::RichText::new("Find Callers")
                                                .size(12.0).color(theme.text_primary)).clicked() {
                                                *xref_request = Some(XrefAction::FindCallers(word_under_cursor.clone()));
                                                ui.close_menu();
                                            }
                                            if ui.button(egui::RichText::new("Find Usages (Shift+F12)")
                                                .size(12.0).color(theme.text_primary)).clicked() {
                                                *xref_request = Some(XrefAction::FindUsages(word_under_cursor.clone()));
                                                ui.close_menu();
                                            }
                                            if ui.button(egui::RichText::new("Show Hierarchy")
                                                .size(12.0).color(theme.text_primary)).clicked() {
                                                *xref_request = Some(XrefAction::ShowHierarchy(word_under_cursor.clone()));
                                                ui.close_menu();
                                            }
                                            // ── Navigation ──────────────────────────────────
                                            if language == FileLanguage::Java || language == FileLanguage::Smali {
                                                ui.separator();
                                                if language == FileLanguage::Java {
                                                    if ui.button(egui::RichText::new("Open in Smali (Ctrl+J)")
                                                        .size(12.0).color(theme.accent_primary)).clicked()
                                                    {
                                                        *jump_request = Some((ctx_path.clone(), ctx_line));
                                                        ui.close_menu();
                                                    }
                                                } else {
                                                    if ui.button(egui::RichText::new("Open in Java (Ctrl+K)")
                                                        .size(12.0).color(theme.accent_primary)).clicked()
                                                    {
                                                        *reverse_jump_request = Some((ctx_path.clone(), ctx_line));
                                                        ui.close_menu();
                                                    }
                                                }
                                            }
                                        });

                                        // Opcode hover tooltip for Smali files
                                        if language == FileLanguage::Smali && !word_under_cursor.is_empty() {
                                            if let Some(desc) = crate::engine::smali_opcodes::SmaliOpcodes::describe(&word_under_cursor) {
                                                output.response.on_hover_text(desc);
                                            }
                                        }

                                        // ── Smali Autocomplete ──
                                        if language == FileLanguage::Smali {
                                            let (query, _) = crate::engine::smali_complete::SmaliCompleter::current_word(text, cursor_char_idx);
                                            if query.len() >= 2 && !query.chars().all(|c| c.is_ascii_digit()) {
                                                if tab.autocomplete_query != query {
                                                    tab.autocomplete_query = query.clone();
                                                    tab.autocomplete_suggestions = crate::engine::smali_complete::SmaliCompleter::suggest(
                                                        &query,
                                                        completion_classes,
                                                        completion_methods,
                                                        8,
                                                    );
                                                    tab.autocomplete_sel = 0;
                                                }
                                                tab.autocomplete_visible = !tab.autocomplete_suggestions.is_empty();
                                            } else {
                                                tab.autocomplete_visible = false;
                                            }
                                        }
                                    }

                                    // ── Autocomplete popup ──
                                    if tab.autocomplete_visible && !tab.autocomplete_suggestions.is_empty() {
                                        let cidx = Self::clamp_to_char_boundary(text, tab.cursor_pos);
                                        let cline = text[..cidx].chars().filter(|&c| c == '\n').count();
                                        let ccol = Self::line_char_column(text, cidx);
                                        let px = text_left + ccol as f32 * char_width;
                                        let py = gutter_rect.top() + (cline as f32 + 1.0) * line_height;
                                        let ph = tab.autocomplete_suggestions.len().min(8) as f32 * 24.0 + 8.0;
                                        let popup_rect = egui::Rect::from_min_size(egui::pos2(px, py), egui::vec2(360.0, ph));
                                        let mut insert_completion: Option<String> = None;
                                        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(popup_rect), |ui| {
                                            egui::Frame::NONE
                                                .fill(theme.bg_elevated)
                                                .stroke(egui::Stroke::new(1.0, theme.border))
                                                .corner_radius(egui::CornerRadius::same(4))
                                                .inner_margin(egui::Margin::same(4))
                                                .show(ui, |ui| {
                                                    for (idx, sug) in tab.autocomplete_suggestions.clone().iter().enumerate() {
                                                        let is_sel = idx == tab.autocomplete_sel;
                                                        let bg = if is_sel { theme.accent_primary.linear_multiply(0.15) } else { egui::Color32::TRANSPARENT };
                                                        let tc = if is_sel { theme.accent_primary } else { theme.text_primary };
                                                        let f = egui::Frame::NONE.fill(bg).show(ui, |ui| {
                                                            ui.horizontal(|ui| {
                                                                ui.label(
                                                                    egui::RichText::new(sug.kind.icon())
                                                                        .size(theme.font_small)
                                                                        .strong()
                                                                        .color(theme.text_muted),
                                                                );
                                                                ui.label(
                                                                    egui::RichText::new(&sug.text)
                                                                        .size(theme.font_small)
                                                                        .monospace()
                                                                        .color(tc),
                                                                );
                                                                ui.label(
                                                                    egui::RichText::new(&sug.detail)
                                                                        .size(theme.font_small - 1.0)
                                                                        .color(theme.text_muted),
                                                                );
                                                            }).response.on_hover_text(&sug.detail)
                                                        });
                                                        if f.inner.clicked() {
                                                            insert_completion = Some(sug.text.clone());
                                                        }
                                                    }
                                                });
                                        });
                                        if ui.input(|i| i.key_pressed(egui::Key::Escape)) { tab.autocomplete_visible = false; }
                                        if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
                                            tab.autocomplete_sel = (tab.autocomplete_sel + 1).min(tab.autocomplete_suggestions.len().saturating_sub(1));
                                        }
                                        if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
                                            tab.autocomplete_sel = tab.autocomplete_sel.saturating_sub(1);
                                        }
                                        if ui.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Tab)) {
                                            insert_completion = tab.autocomplete_suggestions.get(tab.autocomplete_sel).map(|s| s.text.clone());
                                        }
                                        if let Some(insert) = insert_completion {
                                            let cursor_char_idx = text[..cidx].chars().count();
                                            let (query, start) = crate::engine::smali_complete::SmaliCompleter::current_word(text, cursor_char_idx);
                                            let end = (start + query.len()).min(text.len());
                                            if start <= end && text.is_char_boundary(start) && text.is_char_boundary(end) {
                                                text.replace_range(start..end, &insert);
                                                tab.cursor_pos = start + insert.len();
                                                tab.modified = true;
                                            }
                                            tab.autocomplete_visible = false;
                                        }
                                    }
                                }); // horizontal_top
                            }); // scroll.show
                            tab.scroll_offset_y = scroll_out.state.offset.y;
                        },
                    ); // allocate_ui_with_layout (editor)

                    // ── Minimap pane ──
//                    ui.allocate_ui_with_layout(
//                        egui::vec2(minimap_w, avail_size.y),
//                        egui::Layout::top_down(egui::Align::LEFT),
//                        |ui| {
//                            let avail = ui.available_rect_before_wrap();
//                            ui.painter().rect_filled(avail, 0.0, theme.bg_secondary);
//                            let lc = minimap_lines.len().max(1);
//                            let mini_lh = (avail.height() / lc as f32).max(0.5).min(3.0);
//
//                            for (i, line) in minimap_lines.iter().enumerate() {
//                                let my = avail.top() + i as f32 * mini_lh;
//                                if my > avail.bottom() { break; }
//                                let trimmed = line.trim();
//                                let color = if trimmed.starts_with("//") || trimmed.starts_with(';') || trimmed.starts_with('#') {
//                                    // Comments — Catppuccin green
//                                    egui::Color32::from_rgb(166, 227, 161)
//                                } else if trimmed.contains('"') || trimmed.contains('\'') {
//                                    // Strings — Catppuccin peach
//                                    egui::Color32::from_rgb(250, 179, 135)
//                                } else if trimmed.starts_with('.') || trimmed.starts_with("invoke") || trimmed.starts_with("const") || trimmed.starts_with("return") {
//                                    // Keywords/directives — Catppuccin mauve
//                                    egui::Color32::from_rgb(203, 166, 247)
//                                } else if trimmed.starts_with("class ") || trimmed.starts_with("import ") || trimmed.starts_with("package ") {
//                                    // Type declarations — Catppuccin sky
//                                    egui::Color32::from_rgb(137, 220, 235)
//                                } else if !trimmed.is_empty() {
//                                    // Regular code — dimmed text
//                                    egui::Color32::from_rgb(100, 110, 140)
//                                } else {
//                                    egui::Color32::TRANSPARENT
//                                };
//                                if color != egui::Color32::TRANSPARENT {
//                                    let indent = line.chars().take_while(|&c| c == ' ' || c == '\t').count() as f32 * 0.6;
//                                    let bar_w = (line.trim_end().len() as f32 * 0.7).min(minimap_w - 4.0 - indent).max(0.0);
//                                    ui.painter().rect_filled(
//                                        egui::Rect::from_min_size(egui::pos2(avail.left() + 2.0 + indent, my), egui::vec2(bar_w, mini_lh.max(1.0))),
//                                        0.0, color,
//                                    );
//                                }
//                            }
//
//                            // Viewport indicator
//                            let total_h = lc as f32 * line_height;
//                            let scroll_frac = (tab.scroll_offset_y / total_h.max(1.0)).min(1.0);
//                            let viewport_frac = (avail_size.y / total_h.max(1.0)).min(1.0);
//                            let vp_top = avail.top() + scroll_frac * avail.height();
//                            let vp_h = (viewport_frac * avail.height()).max(4.0);
//                            ui.painter().rect_filled(
//                                egui::Rect::from_min_size(egui::pos2(avail.left(), vp_top), egui::vec2(minimap_w, vp_h)),
//                                0.0, theme.minimap_viewport,
//                            );
//
//                            // Click to jump
//                            let mini_resp = ui.allocate_rect(avail, egui::Sense::click());
//                            if mini_resp.clicked() {
//                                if let Some(pos) = mini_resp.interact_pointer_pos() {
//                                    let frac = ((pos.y - avail.top()) / avail.height()).clamp(0.0, 1.0);
//                                    tab.target_line = Some((frac * lc as f32) as usize);
//                                }
//                            }
//                        },
//                    ); // allocate_ui_with_layout (minimap)
                }); // horizontal_top

                text_changed = *text != old_text;
            }
        }

        if text_changed {
            if settings.auto_save {
                if let crate::app::TabContent::Code(content) = &tab.content {
                    if std::fs::write(&tab.path, content).is_ok() {
                        tab.modified = false;
                    } else {
                        tab.modified = true;
                    }
                }
            } else {
                tab.modified = true;
            }
        }
    }

    /// Extract the word (identifier) at the given byte offset in text.
    fn extract_word_at(text: &str, pos: usize) -> String {
        if pos >= text.len() {
            return String::new();
        }
        let bytes = text.as_bytes();
        let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || b == b'/' || b == b';' || b == b'-' || b == b'>';
        let mut start = pos;
        while start > 0 && is_ident(bytes[start - 1]) {
            start -= 1;
        }
        let mut end = pos;
        while end < bytes.len() && is_ident(bytes[end]) {
            end += 1;
        }
        if start == end {
            return String::new();
        }
        text[start..end].to_string()
    }

    fn char_to_byte_index(text: &str, char_idx: usize) -> usize {
        text.char_indices()
            .nth(char_idx)
            .map(|(byte_idx, _)| byte_idx)
            .unwrap_or(text.len())
    }

    fn clamp_to_char_boundary(text: &str, byte_idx: usize) -> usize {
        let mut idx = byte_idx.min(text.len());
        while idx > 0 && !text.is_char_boundary(idx) {
            idx -= 1;
        }
        idx
    }

    fn line_char_column(text: &str, byte_idx: usize) -> usize {
        let idx = Self::clamp_to_char_boundary(text, byte_idx);
        let line_start = text[..idx].rfind('\n').map(|n| n + 1).unwrap_or(0);
        text[line_start..idx].chars().count()
    }

    fn decode_hex_input(input: &str) -> Option<Vec<u8>> {
        let clean: Vec<char> = input.chars().filter(|c| !c.is_whitespace()).collect();
        if clean.len() % 2 != 0 {
            return None;
        }

        let mut out = Vec::with_capacity(clean.len() / 2);
        for pair in clean.chunks(2) {
            let hi = pair[0].to_digit(16)?;
            let lo = pair[1].to_digit(16)?;
            out.push(((hi << 4) | lo) as u8);
        }
        Some(out)
    }

    /// Render a hex dump view for binary file content.
    fn render_hex(ui: &mut egui::Ui, bytes: &[u8], theme: &Theme) {
        const COLS: usize = 16;
        let row_count = bytes.len().div_ceil(COLS);
        let font_id = egui::FontId::monospace(theme.font_code);

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for row in 0..row_count {
                let start = row * COLS;
                let end = (start + COLS).min(bytes.len());
                let chunk = &bytes[start..end];

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;

                    // Offset column
                    ui.label(egui::RichText::new(format!("{:08X}  ", start))
                        .font(font_id.clone())
                        .color(theme.hex_address_color));

                    // Hex bytes
                    let mut hex_str = String::with_capacity(COLS * 3 + 2);
                    for (i, b) in chunk.iter().enumerate() {
                        hex_str.push_str(&format!("{:02X}", b));
                        if i == 7 { hex_str.push_str("  "); }
                        else { hex_str.push(' '); }
                    }
                    // Pad if short row
                    let remaining = COLS - chunk.len();
                    for i in 0..remaining {
                        hex_str.push_str("   ");
                        if chunk.len() + i == 7 { hex_str.push(' '); }
                    }
                    ui.label(egui::RichText::new(&hex_str).font(font_id.clone()).color(theme.text_primary));

                    ui.label(egui::RichText::new(" |").font(font_id.clone()).color(theme.text_muted));

                    // ASCII column
                    let ascii: String = chunk.iter().map(|&b| {
                        if b.is_ascii_graphic() || b == b' ' { b as char } else { '.' }
                    }).collect();
                    ui.label(egui::RichText::new(format!("{}|", ascii)).font(font_id.clone()).color(theme.text_secondary));
                });
            }
        });
    }

    /// Find the matching bracket (character-offset pair) for the bracket at or before the cursor.
    fn find_bracket_match(text: &str, cursor: usize) -> Option<(usize, usize)> {
        let chars: Vec<char> = text.chars().collect();
        let len = chars.len();
        if len == 0 { return None; }

        // Check char at cursor and char before cursor
        let candidates = [
            if cursor < len { Some(cursor) } else { None },
            if cursor > 0 { Some(cursor - 1) } else { None },
        ];

        for maybe_pos in candidates.iter().flatten() {
            let pos = *maybe_pos;
            match chars[pos] {
                '{' | '(' | '[' => {
                    let open = chars[pos];
                    let close = match open { '{' => '}', '(' => ')', _ => ']' };
                    let mut depth = 0i32;
                    for i in pos..len {
                        if chars[i] == open { depth += 1; }
                        else if chars[i] == close {
                            depth -= 1;
                            if depth == 0 {
                                // Convert char indices to byte offsets
                                let a = text.char_indices().nth(pos).map(|(b, _)| b)?;
                                let b = text.char_indices().nth(i).map(|(b, _)| b)?;
                                return Some((a, b));
                            }
                        }
                    }
                }
                '}' | ')' | ']' => {
                    let close = chars[pos];
                    let open = match close { '}' => '{', ')' => '(', _ => '[' };
                    let mut depth = 0i32;
                    let ipos = pos as i64;
                    let mut i = ipos;
                    while i >= 0 {
                        let idx = i as usize;
                        if chars[idx] == close { depth += 1; }
                        else if chars[idx] == open {
                            depth -= 1;
                            if depth == 0 {
                                let a = text.char_indices().nth(idx).map(|(b, _)| b)?;
                                let b2 = text.char_indices().nth(pos).map(|(b, _)| b)?;
                                return Some((a, b2));
                            }
                        }
                        i -= 1;
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn render_welcome(&self, ui: &mut egui::Ui, theme: &Theme) {
        let rect = ui.available_rect_before_wrap();
        ui.painter().rect_filled(rect, 0.0, theme.editor_bg);
        let content_rect = rect.shrink2(egui::vec2(48.0, 32.0));
        let mut content_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(content_rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );

        content_ui.vertical_centered(|ui| {
            ui.add_space(24.0);
            ui.label(
                egui::RichText::new("RevEng IDE")
                    .size(32.0)
                    .strong()
                    .color(theme.text_primary),
            );
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Professional Android Reverse Engineering Suite")
                    .size(theme.font_ui)
                    .color(theme.accent_primary),
            );
            ui.add_space(16.0);

            let desc = egui::RichText::new(
                "Stop juggling APKTool, JADX, hex editors, and terminal tabs.\n\
                 RevEng IDE brings decoding, decompiling, smali browsing,\n\
                 cross-referencing, string extraction, secret hunting, patching,\n\
                 and rebuilding into a single unified workspace. One tool,\n\
                 one workflow, zero tab switching."
            )
            .size(theme.font_small)
            .color(theme.text_muted);
            ui.label(desc);
            ui.add_space(16.0);

            ui.separator();
            ui.add_space(12.0);

            ui.label(
                egui::RichText::new("Developed by")
                    .size(theme.font_small)
                    .color(theme.text_muted),
            );
            ui.label(
                egui::RichText::new("Levython Technologies")
                    .size(theme.font_ui + 2.0)
                    .strong()
                    .color(theme.info),
            );
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new("www.levython.in")
                    .size(theme.font_small)
                    .color(theme.accent_primary)
                    .underline(),
            );
            ui.add_space(12.0);

            ui.label(
                egui::RichText::new("Open Source - Contribute on GitHub")
                    .size(theme.font_small)
                    .color(theme.success),
            );

            ui.add_space(24.0);
            ui.separator();
            ui.add_space(12.0);

            ui.label(
                egui::RichText::new("Quick Start")
                    .size(theme.font_heading)
                    .strong()
                    .color(theme.text_primary),
            );
            ui.add_space(8.0);
            egui::Grid::new("welcome_actions")
                .spacing([24.0, 6.0])
                .show(ui, |ui| {
                    let action = |s: &str| {
                        egui::RichText::new(s)
                            .size(theme.font_small)
                            .color(theme.text_secondary)
                    };
                    let key = |s: &str| {
                        egui::RichText::new(s)
                            .size(theme.font_small)
                            .color(theme.text_muted)
                    };
                    ui.label(action("Open APK")); ui.label(key("Ctrl+O")); ui.end_row();
                    ui.label(action("Command Palette")); ui.label(key("Ctrl+Shift+P")); ui.end_row();
                    ui.label(action("Quick Open")); ui.label(key("Ctrl+P")); ui.end_row();
                    ui.label(action("Decode APK")); ui.label(key("Toolbar")); ui.end_row();
                    ui.label(action("Decompile to Java")); ui.label(key("Toolbar")); ui.end_row();
                });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::EditorPanel;

    #[test]
    fn char_cursor_to_byte_index_handles_multibyte_text() {
        let text = "aé中;";
        assert_eq!(EditorPanel::char_to_byte_index(text, 0), 0);
        assert_eq!(EditorPanel::char_to_byte_index(text, 1), 1);
        assert_eq!(EditorPanel::char_to_byte_index(text, 2), 3);
        assert_eq!(EditorPanel::char_to_byte_index(text, 3), 6);
        assert_eq!(EditorPanel::char_to_byte_index(text, 99), text.len());
    }

    #[test]
    fn clamps_byte_offsets_to_utf8_boundaries() {
        let text = "aé中;";
        assert_eq!(EditorPanel::clamp_to_char_boundary(text, 2), 1);
        assert_eq!(EditorPanel::clamp_to_char_boundary(text, 5), 3);
        assert_eq!(EditorPanel::clamp_to_char_boundary(text, 99), text.len());
    }

    #[test]
    fn line_char_column_counts_characters_not_bytes() {
        let text = "one\nαb中;";
        let col_after_alpha = "one\nα".len();
        let col_after_zhong = "one\nαb中".len();

        assert_eq!(EditorPanel::line_char_column(text, col_after_alpha), 1);
        assert_eq!(EditorPanel::line_char_column(text, col_after_zhong), 3);
        assert_eq!(EditorPanel::line_char_column(text, col_after_zhong - 1), 2);
    }

    #[test]
    fn decodes_hex_without_byte_slicing_input() {
        assert_eq!(EditorPanel::decode_hex_input("CA FE 00"), Some(vec![0xca, 0xfe, 0x00]));
        assert_eq!(EditorPanel::decode_hex_input("a"), None);
        assert_eq!(EditorPanel::decode_hex_input("é0"), None);
    }
}
