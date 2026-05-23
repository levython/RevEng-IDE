//! File tree panel — displays the workspace directory hierarchy.

use crate::app::{AppState, LogLevel};
use crate::ui::theme::Theme;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub struct FileTreePanel {
    expanded: std::collections::HashSet<PathBuf>,
}

enum TreeIcon<'a> {
    Folder { open: bool },
    File { ext: &'a str },
}

impl FileTreePanel {
    pub fn new() -> Self {
        Self {
            expanded: std::collections::HashSet::new(),
        }
    }

    pub fn render(&mut self, ui: &mut egui::Ui, state: &Arc<Mutex<AppState>>) {
        let theme = Theme::current(ui);

        let workspace_root = {
            let s = state.lock().unwrap();
            s.workspace.root_dir().map(|p| p.to_path_buf())
        };
        let active_path = {
            let s = state.lock().unwrap();
            s.active_tab
                .and_then(|idx| s.open_tabs.get(idx).map(|t| t.path.clone()))
        };

        ui.vertical(|ui| {
            let subtitle = workspace_root
                .as_ref()
                .map(|root| root.display().to_string())
                .unwrap_or_else(|| "Open an APK to browse decoded and decompiled sources.".into());
            Self::render_header(ui, &theme, "Explorer", &subtitle);
            ui.add_space(6.0);

            let mut submitted_search: Option<(PathBuf, String)> = None;
            if workspace_root.is_some() {
                {
                    let mut s = state.lock().unwrap();
                    let response = Self::render_search_box(
                        ui,
                        &theme,
                        "Search",
                        &mut s.global_search_query,
                        "Search workspace, classes, strings...",
                    );

                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        if let Some(root) = s.workspace.root_dir().map(|p| p.to_path_buf()) {
                            let query = s.global_search_query.trim().to_string();
                            if !query.is_empty() {
                                submitted_search = Some((root, query));
                            }
                        }
                    }
                }
                ui.add_space(8.0);
            }

            if let Some((root, query)) = submitted_search {
                let state_c = Arc::clone(state);
                std::thread::spawn(move || {
                    let results = crate::engine::patch::PatchEngine::search_in_dir(
                        &root,
                        &query,
                        &["java", "smali", "xml"],
                    )
                    .unwrap_or_default();
                    let mut s = state_c.lock().unwrap();
                    s.search_results = results;
                    let count = s.search_results.len();
                    s.push_log(
                        crate::app::LogLevel::Info,
                        &format!("Search completed: {} files matching '{}'", count, query),
                    );
                });
            }

            if let Some(root) = workspace_root {
                egui::ScrollArea::both()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let results = { state.lock().unwrap().search_results.clone() };
                        if !results.is_empty() {
                            Self::render_search_hits(ui, &theme, state, &root, &results);
                            ui.add_space(10.0);
                        }

                        self.render_directory(ui, &root, state, 0, active_path.as_ref());
                    });
            } else {
                Self::render_empty_state(
                    ui,
                    &theme,
                    "No workspace open",
                    "Open an APK to populate decoded and decompiled files here.",
                );
            }
        });
    }

    fn render_directory(
        &mut self,
        ui: &mut egui::Ui,
        dir: &Path,
        state: &Arc<Mutex<AppState>>,
        depth: usize,
        active_path: Option<&PathBuf>,
    ) {
        let theme = Theme::current(ui);

        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        let mut dirs: Vec<PathBuf> = Vec::new();
        let mut files: Vec<PathBuf> = Vec::new();

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                dirs.push(path);
            } else {
                files.push(path);
            }
        }

        dirs.sort_by(|a, b| Self::entry_sort_key(a).cmp(&Self::entry_sort_key(b)));
        files.sort_by(|a, b| {
            a.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_lowercase()
                .cmp(
                    &b.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("")
                        .to_lowercase(),
                )
        });

        for dir_path in dirs {
            let name = dir_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?");
            let is_expanded = self.expanded.contains(&dir_path);
            let folder_color = Self::folder_color(&theme, name);

            let response = Self::interactive_row(
                ui,
                &theme,
                false,
                depth,
                egui::Color32::TRANSPARENT,
                Self::row_content_width(ui, name),
                |ui| {
                    Self::draw_disclosure(ui, &theme, is_expanded);
                    Self::draw_tree_icon(ui, &theme, TreeIcon::Folder { open: is_expanded }, folder_color);

                    ui.label(
                        egui::RichText::new(name)
                            .size(theme.font_ui)
                            .color(theme.text_primary),
                    );
                },
            );

            let response = response.on_hover_text(dir_path.display().to_string());

            if response.clicked() {
                if is_expanded {
                    self.expanded.remove(&dir_path);
                } else {
                    self.expanded.insert(dir_path.clone());
                }
            }

            if is_expanded {
                self.render_directory(ui, &dir_path, state, depth + 1, active_path);
            }
        }

        for file_path in files {
            let name = file_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?");
            let ext = file_path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");
            let (_, icon_color) = theme.file_icon(ext);
            let is_active = active_path.map(|p| p == &file_path).unwrap_or(false);

            let response = Self::interactive_row(
                ui,
                &theme,
                is_active,
                depth,
                egui::Color32::TRANSPARENT,
                Self::row_content_width(ui, name),
                |ui| {
                    ui.add_space(16.0);
                    Self::draw_tree_icon(ui, &theme, TreeIcon::File { ext }, icon_color);
                    ui.label(
                        egui::RichText::new(name)
                            .size(theme.font_ui)
                            .color(if is_active {
                                theme.text_accent
                            } else {
                                theme.text_primary
                            }),
                    );
                },
            );

            if response.clicked() {
                let mut s = state.lock().unwrap();
                if ext == "so" {
                    s.push_log(
                        LogLevel::Info,
                        &format!("Native library: {}", file_path.display()),
                    );
                }
                s.open_file(file_path.clone());
            }

            response.context_menu(|ui| {
                let t = Theme::current(ui);
                if ui
                    .button(egui::RichText::new("Open").color(t.text_primary))
                    .clicked()
                {
                    state.lock().unwrap().open_file(file_path.clone());
                    ui.close_menu();
                }
                if ui
                    .button(egui::RichText::new("Open to Side").color(t.text_primary))
                    .clicked()
                {
                    state.lock().unwrap().open_file_right(file_path.clone(), None);
                    ui.close_menu();
                }
                ui.separator();
                if ui
                    .button(egui::RichText::new("Copy Path").color(t.text_primary))
                    .clicked()
                {
                    ui.ctx().copy_text(file_path.display().to_string());
                    ui.close_menu();
                }
                if ui
                    .button(
                        egui::RichText::new("Reveal in File Manager").color(t.text_primary),
                    )
                    .clicked()
                {
                    if let Some(parent) = file_path.parent() {
                        let _ = open::that(parent);
                    }
                    ui.close_menu();
                }
            });

            let _ = response.on_hover_text(file_path.display().to_string());
        }
    }

    fn render_header(ui: &mut egui::Ui, theme: &Theme, title: &str, subtitle: &str) {
        egui::Frame::NONE
            .inner_margin(egui::Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(title.to_uppercase())
                        .size(11.0)
                        .strong()
                        .color(theme.text_primary),
                );
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(subtitle)
                            .size(11.0)
                            .color(theme.text_muted)
                    ).wrap()
                );
            });
    }

    fn render_search_box(
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
                                .color(theme.text_muted),
                        );
                        let response = ui.add(
                            egui::TextEdit::singleline(query)
                                .frame(false)
                                .hint_text(hint)
                                .desired_width(f32::INFINITY)
                                .font(egui::TextStyle::Small),
                        );

                        if !query.is_empty()
                            && ui
                                .add(
                                    egui::Button::new(egui::RichText::new("x").size(10.0).color(theme.text_muted))
                                        .frame(false)
                                )
                                .clicked()
                        {
                            query.clear();
                        }
                        response
                    })
                    .inner
                })
                .inner
        })
        .inner
    }

    fn render_search_hits(
        ui: &mut egui::Ui,
        theme: &Theme,
        state: &Arc<Mutex<AppState>>,
        workspace_root: &Path,
        results: &[crate::engine::patch::SearchResult],
    ) {
        let total_matches: usize = results.iter().map(|res| res.matches.len()).sum();

        egui::Frame::NONE
            .fill(theme.bg_elevated)
            .stroke(egui::Stroke::new(1.0, theme.border_subtle))
            .corner_radius(egui::CornerRadius::same((theme.corner_radius as u8) + 1))
            .inner_margin(egui::Margin::symmetric(8, 8))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Search Hits")
                            .size(theme.font_ui)
                            .strong()
                            .color(theme.warning),
                    );
                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            let badge = egui::Frame::NONE
                                .fill(theme.warning.linear_multiply(0.12))
                                .corner_radius(egui::CornerRadius::same(4))
                                .inner_margin(egui::Margin::symmetric(6, 2));
                            badge.show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{} files / {} matches",
                                        results.len(),
                                        total_matches
                                    ))
                                    .size(theme.font_small)
                                    .color(theme.warning),
                                );
                            });
                        },
                    );
                });

                ui.add_space(6.0);

                for res in results.iter().take(12) {
                    let ext = res.path.extension().and_then(|e| e.to_str()).unwrap_or("");
                    let (_, icon_color) = theme.file_icon(ext);
                    let rel_path = res.path.strip_prefix(workspace_root).unwrap_or(&res.path);
                    let first_match = res.matches.first();

                    let response = Self::interactive_row(
                        ui,
                        theme,
                        false,
                        0,
                        theme.bg_secondary,
                        Self::row_content_width(ui, rel_path.file_name().and_then(|n| n.to_str()).unwrap_or("?")),
                        |ui| {
                            Self::draw_tree_icon(ui, theme, TreeIcon::File { ext }, icon_color);

                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 0.0;
                                ui.label(
                                    egui::RichText::new(
                                        rel_path.file_name().and_then(|n| n.to_str()).unwrap_or("?"),
                                    )
                                    .size(11.8)
                                    .color(theme.text_primary),
                                );
                                if let Some(matched) = first_match {
                                    let snippet = matched.line_content.trim();
                                    let snippet = if snippet.chars().count() > 64 {
                                        format!("{}...", Self::truncate_end_chars(snippet, 61))
                                    } else {
                                        snippet.to_string()
                                    };
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "L{}  {}",
                                            matched.line_number, snippet
                                        ))
                                        .size(theme.font_small)
                                        .color(theme.text_muted)
                                        .monospace(),
                                    );
                                }
                            });

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        egui::RichText::new(format!("{}", res.matches.len()))
                                            .size(theme.font_small)
                                            .color(theme.text_muted),
                                    );
                                },
                            );
                        },
                    );

                    if response.clicked() {
                        if let Some(matched) = first_match {
                            state
                                .lock()
                                .unwrap()
                                .open_file_at_line(res.path.clone(), matched.line_number, None);
                        } else {
                            state.lock().unwrap().open_file(res.path.clone());
                        }
                    }
                }
            });
    }

    fn render_empty_state(
        ui: &mut egui::Ui,
        theme: &Theme,
        title: &str,
        subtitle: &str,
    ) {
        egui::Frame::NONE
            .inner_margin(egui::Margin::symmetric(12, 18))
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
                            .color(theme.text_muted),
                    )
                    .wrap(),
                );
            });
    }

    fn interactive_row(
        ui: &mut egui::Ui,
        theme: &Theme,
        is_active: bool,
        depth: usize,
        base_fill: egui::Color32,
        content_width: f32,
        add_contents: impl FnOnce(&mut egui::Ui),
    ) -> egui::Response {
        let width = ui.available_width().max(content_width);
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(width, 24.0), egui::Sense::click());

        let fill = if is_active {
            theme.tree_selected_bg
        } else if response.hovered() {
            theme.tree_hover_bg
        } else {
            base_fill
        };

        if fill != egui::Color32::TRANSPARENT {
            ui.painter().rect_filled(rect, theme.corner_radius, fill);
        }

        if is_active {
            let accent_rect = egui::Rect::from_min_size(
                egui::pos2(rect.left() + 1.0, rect.top() + 4.0),
                egui::vec2(3.0, rect.height() - 8.0),
            );
            ui.painter()
                .rect_filled(accent_rect, 2.0, theme.accent_primary);
        }

        let content_rect = rect.shrink2(egui::vec2(8.0, 2.0));
        let mut row_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(content_rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        row_ui.add_space((depth as f32) * 14.0);
        row_ui.spacing_mut().item_spacing.x = 5.0;
        add_contents(&mut row_ui);

        response
    }

    fn row_content_width(ui: &egui::Ui, text: &str) -> f32 {
        let font_id = egui::FontId::proportional(11.8);
        let char_width = ui.fonts(|f| f.glyph_width(&font_id, 'W'));
        let text_width = (text.chars().count() as f32 * char_width) + 72.0;
        text_width.max(220.0)
    }

    fn draw_disclosure(ui: &mut egui::Ui, theme: &Theme, open: bool) {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 16.0), egui::Sense::hover());
        let c = rect.center();
        let stroke = egui::Stroke::new(1.2, theme.text_muted);
        if open {
            ui.painter().line_segment(
                [egui::pos2(c.x - 3.0, c.y - 1.0), egui::pos2(c.x, c.y + 2.5)],
                stroke,
            );
            ui.painter().line_segment(
                [egui::pos2(c.x, c.y + 2.5), egui::pos2(c.x + 3.0, c.y - 1.0)],
                stroke,
            );
        } else {
            ui.painter().line_segment(
                [egui::pos2(c.x - 1.0, c.y - 3.0), egui::pos2(c.x + 2.5, c.y)],
                stroke,
            );
            ui.painter().line_segment(
                [egui::pos2(c.x + 2.5, c.y), egui::pos2(c.x - 1.0, c.y + 3.0)],
                stroke,
            );
        }
    }

    fn draw_tree_icon(ui: &mut egui::Ui, theme: &Theme, icon: TreeIcon<'_>, color: egui::Color32) {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
        let stroke = egui::Stroke::new(1.25, color);
        match icon {
            TreeIcon::Folder { open } => {
                let body = egui::Rect::from_min_max(
                    egui::pos2(rect.left() + 1.5, rect.top() + 5.0),
                    egui::pos2(rect.right() - 1.5, rect.bottom() - 2.0),
                );
                let tab = egui::Rect::from_min_max(
                    egui::pos2(rect.left() + 2.5, rect.top() + 3.0),
                    egui::pos2(rect.left() + 8.0, rect.top() + 6.0),
                );
                ui.painter().rect_filled(body, 2.0, color.linear_multiply(if open { 0.18 } else { 0.10 }));
                ui.painter().rect_filled(tab, 1.5, color.linear_multiply(if open { 0.26 } else { 0.16 }));
                ui.painter().line_segment([body.left_top(), body.right_top()], stroke);
                ui.painter().line_segment([body.left_top(), body.left_bottom()], stroke);
                ui.painter().line_segment([body.right_top(), body.right_bottom()], stroke);
                ui.painter().line_segment([body.left_bottom(), body.right_bottom()], stroke);
                ui.painter().line_segment([tab.left_top(), tab.right_top()], stroke);
                ui.painter().line_segment([tab.left_top(), tab.left_bottom()], stroke);
            }
            TreeIcon::File { ext } => {
                let file = egui::Rect::from_min_max(
                    egui::pos2(rect.left() + 3.0, rect.top() + 2.0),
                    egui::pos2(rect.right() - 3.0, rect.bottom() - 2.0),
                );
                ui.painter().rect_filled(file, 1.5, color.linear_multiply(0.10));
                ui.painter().line_segment([file.left_top(), egui::pos2(file.right() - 3.5, file.top())], stroke);
                ui.painter().line_segment([egui::pos2(file.right() - 3.5, file.top()), file.right_top() + egui::vec2(0.0, 3.5)], stroke);
                ui.painter().line_segment([file.right_top() + egui::vec2(0.0, 3.5), file.right_bottom()], stroke);
                ui.painter().line_segment([file.right_bottom(), file.left_bottom()], stroke);
                ui.painter().line_segment([file.left_bottom(), file.left_top()], stroke);

                let accent_y = file.bottom() - 3.0;
                let accent_w = match ext {
                    "smali" => 7.0,
                    "xml" => 5.0,
                    "so" | "dex" | "apk" => 8.0,
                    _ => 6.0,
                };
                ui.painter().line_segment(
                    [egui::pos2(file.left() + 2.0, accent_y), egui::pos2(file.left() + 2.0 + accent_w, accent_y)],
                    egui::Stroke::new(1.4, color),
                );
            }
        }
        let _ = theme;
    }

    fn entry_sort_key(path: &Path) -> (usize, String) {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();
        let rank = match name.as_str() {
            "decoded" => 0,
            "decompiled" => 1,
            "build" => 2,
            "res" => 3,
            _ => 4,
        };
        (rank, name)
    }

    fn folder_color(theme: &Theme, name: &str) -> egui::Color32 {
        match name.to_lowercase().as_str() {
            "decoded" => theme.info,
            "decompiled" => theme.syn_keyword,
            "build" => theme.warning,
            "res" => theme.syn_tag,
            "assets" => theme.syn_function,
            "smali" => theme.success,
            other if other.starts_with("smali_classes") => theme.success,
            _ => theme.text_secondary,
        }
    }

    fn truncate_end_chars(text: &str, max_chars: usize) -> String {
        text.chars().take(max_chars).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::FileTreePanel;

    #[test]
    fn truncates_search_snippets_on_character_boundaries() {
        assert_eq!(FileTreePanel::truncate_end_chars("rooté中tail", 6), "rooté中");
    }
}
