//! Console / log panel — VS Code-style output panel with filter tabs and proper styling.

use crate::app::{AppState, TabContent};
use crate::engine::smali_validator::ValidationSeverity;
use crate::ui::theme::Theme;

use std::sync::{Arc, Mutex};

/// Which sub-tab is visible in the bottom panel.
#[derive(PartialEq, Clone, Copy, Debug)]
pub enum BottomTab {
    Terminal,
    Output,
    Problems,
}

pub struct ConsolePanel {
    auto_scroll: bool,
    filter: LogFilter,
    search_query: String,
    active_tab: BottomTab,
    pub panel_visible: bool,
}

#[derive(PartialEq, Clone, Copy)]
enum LogFilter {
    All,
    Info,
    Warn,
    Error,
}

impl ConsolePanel {
    pub fn new() -> Self {
        Self {
            auto_scroll: true,
            filter: LogFilter::All,
            search_query: String::new(),
            active_tab: BottomTab::Output,
            panel_visible: true,
        }
    }
    
    pub fn is_visible(&self) -> bool {
        self.panel_visible
    }
    
    pub fn toggle_visibility(&mut self) {
        self.panel_visible = !self.panel_visible;
    }
    
    pub fn switch_to_terminal(&mut self) {
        self.active_tab = BottomTab::Terminal;
        self.panel_visible = true;
    }
    
    pub fn switch_to_output(&mut self) {
        self.active_tab = BottomTab::Output;
        self.panel_visible = true;
    }
    
    pub fn switch_to_problems(&mut self) {
        self.active_tab = BottomTab::Problems;
        self.panel_visible = true;
    }

    pub fn render(&mut self, ui: &mut egui::Ui, state: &Arc<Mutex<AppState>>) {
        let theme = Theme::current(ui);

        // Fill the entire panel to prevent black voids regardless of tab content height
        ui.painter().rect_filled(ui.available_rect_before_wrap(), 0.0, theme.bg_tertiary);

        // ── Tab bar header ──
        egui::Frame::NONE
            .fill(theme.bg_secondary)
            .inner_margin(egui::Margin::symmetric(0, 0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;

                    // Tab buttons
                    self.render_tab_button(ui, &theme, "TERMINAL", BottomTab::Terminal);
                    self.render_tab_button(ui, &theme, "OUTPUT", BottomTab::Output);
                    self.render_tab_button(ui, &theme, "PROBLEMS", BottomTab::Problems);

                    ui.add_space(12.0);

                    // Filter pills and search (only for Output tab)
                    if self.active_tab == BottomTab::Output {
                        ui.spacing_mut().item_spacing.x = 1.0;
                        self.render_filter_pill(ui, &theme, "All", LogFilter::All);
                        self.render_filter_pill(ui, &theme, "Info", LogFilter::Info);
                        self.render_filter_pill(ui, &theme, "Warn", LogFilter::Warn);
                        self.render_filter_pill(ui, &theme, "Error", LogFilter::Error);

                        ui.add_space(6.0);
                        // Search box
                        ui.add(
                            egui::TextEdit::singleline(&mut self.search_query)
                                .hint_text("Filter...")
                                .desired_width(120.0)
                                .font(egui::TextStyle::Small)
                                .frame(false),
                        );
                    }

                    // Right side controls
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;

                        // Close button
                        if ui.add(
                            egui::Button::new(
                                egui::RichText::new("x").size(12.0).color(theme.text_muted)
                            ).frame(false)
                        ).on_hover_text("Close Panel (Ctrl+J)").clicked() {
                            self.panel_visible = false;
                            let mut s = state.lock().unwrap();
                            s.settings.show_bottom_panel = false;
                            s.save_settings();
                        }

                        // Chevron down (minimize)
                        if ui.add(
                            egui::Button::new(
                                egui::RichText::new("-").size(13.0).color(theme.text_muted)
                            ).frame(false)
                        ).on_hover_text("Minimize Panel").clicked() {
                            self.panel_visible = false;
                            let mut s = state.lock().unwrap();
                            s.settings.show_bottom_panel = false;
                            s.save_settings();
                        }

                        ui.separator();

                        // Tab-specific controls
                        match self.active_tab {
                            BottomTab::Output => {
                                if ui.add(
                                    egui::Button::new(
                                        egui::RichText::new("Clear").size(theme.font_small).color(theme.text_muted)
                                    ).frame(false)
                                ).on_hover_text("Clear Output").clicked() {
                                    state.lock().unwrap().console_log.clear();
                                }

                                // Auto-scroll toggle
                                let scroll_icon = if self.auto_scroll { "Follow" } else { "Manual" };
                                let scroll_color = if self.auto_scroll { theme.accent_primary } else { theme.text_muted };
                                if ui.add(
                                    egui::Button::new(
                                        egui::RichText::new(scroll_icon).size(theme.font_small).color(scroll_color)
                                    ).frame(false)
                                ).on_hover_text(if self.auto_scroll { "Auto-scroll ON" } else { "Auto-scroll OFF" }).clicked() {
                                    self.auto_scroll = !self.auto_scroll;
                                }
                            }
                            BottomTab::Terminal => {
                                if ui.add(
                                    egui::Button::new(
                                        egui::RichText::new("Clear").size(theme.font_small).color(theme.text_muted)
                                    ).frame(false)
                                ).on_hover_text("Clear Terminal").clicked() {
                                    state.lock().unwrap().terminal_output.clear();
                                }
                            }
                            BottomTab::Problems => {
                                // Problems controls
                            }
                        }
                    });
                });
            });

        // ── Thin separator ──
        let sep_rect = ui.available_rect_before_wrap();
        let sep_rect = egui::Rect::from_min_size(sep_rect.min, egui::vec2(sep_rect.width(), 1.0));
        ui.painter().rect_filled(sep_rect, 0.0, theme.separator);
        ui.add_space(1.0);

        match self.active_tab {
            BottomTab::Terminal => self.render_terminal(ui, state, &theme),
            BottomTab::Output => self.render_output(ui, state, &theme),
            BottomTab::Problems => self.render_problems(ui, state, &theme),
        }
    }
    
    fn render_terminal(&mut self, ui: &mut egui::Ui, state: &Arc<Mutex<AppState>>, theme: &Theme) {
        let font = egui::FontId::monospace(theme.font_small);
        let input_row_h = 24.0;

        let (workspace_root, workspace_name, terminal_output_snapshot) = {
            let s = state.lock().unwrap();
            (
                s.workspace.root_dir().map(|p| p.display().to_string()),
                s.workspace.root_dir()
                    .and_then(|r| r.file_name())
                    .and_then(|n| n.to_str())
                    .map(|n| n.to_string()),
                s.terminal_output.iter().cloned().collect::<Vec<_>>(),
            )
        };

        let avail = ui.available_rect_before_wrap();
        let scroll_h = (avail.height() - input_row_h - 2.0).max(0.0);

        ui.allocate_ui_with_layout(
            egui::vec2(avail.width(), scroll_h),
            egui::Layout::top_down(egui::Align::LEFT),
            |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("term_scroll")
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;

                        if terminal_output_snapshot.is_empty() {
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new("RevEng Terminal")
                                    .font(font.clone())
                                    .size(theme.font_ui + 1.0)
                                    .color(theme.text_primary),
                            );
                            ui.label(
                                egui::RichText::new(workspace_root.as_deref().unwrap_or("~"))
                                    .font(font.clone())
                                    .color(theme.text_muted),
                            );
                            ui.label(
                                egui::RichText::new("Type a command and press Enter to execute.")
                                    .font(font.clone())
                                    .color(theme.text_disabled),
                            );
                        }

                        for line in &terminal_output_snapshot {
                            let color = match line.kind {
                                crate::app::TermOutputKind::Stdout => theme.console_text,
                                crate::app::TermOutputKind::Stderr => theme.error,
                                crate::app::TermOutputKind::System => theme.text_muted,
                            };
                            ui.label(
                                egui::RichText::new(&line.text)
                                    .font(font.clone())
                                    .color(color),
                            );
                        }
                    });
            },
        );

        ui.allocate_ui_with_layout(
            egui::vec2(avail.width(), input_row_h),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let mut command_to_run = None;

                let mut s = state.lock().unwrap();

                if let Some(ref name) = workspace_name {
                    ui.label(
                        egui::RichText::new(name)
                            .font(font.clone())
                            .color(theme.info)
                            .strong(),
                    );
                }
                ui.label(
                    egui::RichText::new("$")
                        .font(font.clone())
                        .color(theme.success)
                        .strong(),
                );

                let input_response = ui.add_sized(
                    [ui.available_width(), input_row_h],
                    egui::TextEdit::singleline(&mut s.terminal_input)
                        .frame(false)
                        .hint_text("type a command...")
                        .font(font.clone())
                        .text_color(theme.console_text)
                        .desired_width(f32::INFINITY),
                );

                if input_response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    let cmd = s.terminal_input.trim().to_string();
                    if !cmd.is_empty() {
                        s.terminal_history.push(cmd.clone());
                        s.terminal_history_index = s.terminal_history.len();
                        s.terminal_input.clear();
                        command_to_run = Some(cmd);
                    }
                    input_response.request_focus();
                }

                if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) && s.terminal_history_index > 0 {
                    s.terminal_history_index -= 1;
                    if let Some(cmd) = s.terminal_history.get(s.terminal_history_index) {
                        s.terminal_input = cmd.clone();
                    }
                }
                if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) && s.terminal_history_index + 1 < s.terminal_history.len() {
                    s.terminal_history_index += 1;
                    if let Some(cmd) = s.terminal_history.get(s.terminal_history_index) {
                        s.terminal_input = cmd.clone();
                    }
                }

                let cwd = s.workspace.root_dir().map(|p| p.to_path_buf());
                drop(s);

                if let Some(cmd) = command_to_run {
                    crate::app::AppState::run_terminal_command_async(Arc::clone(state), cmd, cwd);
                }
            },
        );
    }

    fn render_tab_button(&mut self, ui: &mut egui::Ui, theme: &Theme, label: &str, tab: BottomTab) {
        let is_active = self.active_tab == tab;
        let text_color = if is_active { theme.text_primary } else { theme.text_muted };

        let frame = egui::Frame::NONE
            .inner_margin(egui::Margin::symmetric(12, 6));

        frame.show(ui, |ui| {
            let resp = ui.add(
                egui::Button::new(
                    egui::RichText::new(label)
                        .size(theme.font_small)
                        .strong()
                        .color(text_color)
                ).frame(false)
            );

            // Smooth underline animation
            let underline_t = ui.ctx().animate_bool_with_time(
                egui::Id::new(format!("tab_underline_{}", label)),
                is_active,
                0.15
            );

            if underline_t > 0.0 {
                let rect = resp.rect;
                let underline_width = rect.width() * underline_t;
                let x_offset = (rect.width() - underline_width) / 2.0;
                let accent = egui::Rect::from_min_size(
                    egui::pos2(rect.left() + x_offset, rect.bottom() - 2.0),
                    egui::vec2(underline_width, 2.0),
                );
                ui.painter().rect_filled(accent, 1.0, theme.accent_primary);
            }

            // Hover effect
            if resp.hovered() && !is_active {
                let rect = resp.rect;
                let hover_line = egui::Rect::from_min_size(
                    egui::pos2(rect.left(), rect.bottom() - 2.0),
                    egui::vec2(rect.width(), 2.0),
                );
                ui.painter().rect_filled(hover_line, 1.0, theme.text_muted.linear_multiply(0.3));
            }

            if resp.clicked() {
                self.active_tab = tab;
            }
        });
    }

    fn render_filter_pill(&mut self, ui: &mut egui::Ui, theme: &Theme, label: &str, filter: LogFilter) {
        let is_active = self.filter == filter;
        let color = if is_active { theme.accent_primary } else { theme.text_muted };
        let bg = if is_active { 
            theme.accent_primary.linear_multiply(0.18) 
        } else { 
            egui::Color32::TRANSPARENT 
        };

        let frame = egui::Frame::NONE
            .fill(bg)
            .corner_radius(egui::CornerRadius::same(12))
            .inner_margin(egui::Margin::symmetric(8, 3));

        let resp = frame.show(ui, |ui| {
            ui.add(
                egui::Button::new(
                    egui::RichText::new(label)
                        .size(theme.font_small - 0.5)
                        .strong()
                        .color(color)
                ).frame(false)
            )
        }).inner;

        if resp.hovered() && !is_active {
            ui.painter().rect_filled(
                resp.rect.expand(1.0),
                12.0,
                theme.text_muted.linear_multiply(0.08)
            );
        }

        if resp.clicked() {
            self.filter = filter;
        }
    }

    fn render_output(&self, ui: &mut egui::Ui, state: &Arc<Mutex<AppState>>, theme: &Theme) {
        let s = state.lock().unwrap();
        let font = egui::FontId::monospace(theme.font_small);

        let scroll = egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(self.auto_scroll);

        scroll.show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;

            for (row_idx, entry) in s.console_log.iter().enumerate() {
                // Apply filter
                let show = match self.filter {
                    LogFilter::All => true,
                    LogFilter::Info => entry.level == crate::app::LogLevel::Info,
                    LogFilter::Warn => entry.level == crate::app::LogLevel::Warn,
                    LogFilter::Error => entry.level == crate::app::LogLevel::Error,
                };
                if !show { continue; }

                if !self.search_query.is_empty()
                    && !entry.message.to_lowercase().contains(&self.search_query.to_lowercase())
                {
                    continue;
                }

                let level_color = theme.log_level_color(&entry.level);

                let frame = egui::Frame::NONE
                    .fill(if row_idx % 2 == 0 { egui::Color32::TRANSPARENT } else { theme.console_alt_row })
                    .inner_margin(egui::Margin::symmetric(6, 1));

                frame.show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;

                        // Left colored indicator bar
                        let (bar_rect, _) = ui.allocate_exact_size(egui::vec2(2.0, 13.0), egui::Sense::hover());
                        ui.painter().rect_filled(bar_rect, 1.0, level_color);

                        // Timestamp (dimmed)
                        ui.label(
                            egui::RichText::new(&entry.timestamp)
                                .font(font.clone())
                                .color(theme.console_timestamp),
                        );

                        // Level badge
                        let badge_frame = egui::Frame::NONE
                            .fill(level_color.linear_multiply(0.12))
                            .corner_radius(egui::CornerRadius::same(2))
                            .inner_margin(egui::Margin::symmetric(4, 0));
                        badge_frame.show(ui, |ui| {
                            ui.label(
                                egui::RichText::new(entry.level.label())
                                    .font(egui::FontId::monospace(9.0))
                                    .strong()
                                    .color(level_color),
                            );
                        });

                        // Message with optional tag highlighting
                        let msg = &entry.message;
                        if msg.starts_with('[') && msg.contains(']') {
                            let Some(end_split) = msg.find(']').map(|idx| idx + 1) else {
                                ui.label(
                                    egui::RichText::new(msg)
                                        .font(font.clone())
                                        .color(theme.console_text),
                                );
                                return;
                            };
                            let tag = &msg[..end_split];
                            let content = &msg[end_split..];
                            ui.label(egui::RichText::new(tag).font(font.clone()).color(theme.console_tag).strong());
                            ui.label(egui::RichText::new(content).font(font.clone()).color(theme.console_text));
                        } else {
                            ui.label(
                                egui::RichText::new(msg)
                                    .font(font.clone())
                                    .color(theme.console_text),
                            );
                        }
                    });
                });
            }
        });
    }

    fn render_problems(&self, ui: &mut egui::Ui, state: &Arc<Mutex<AppState>>, theme: &Theme) {
        let problems = {
            let s = state.lock().unwrap();
            s.open_tabs
                .iter()
                .filter_map(|tab| {
                    if tab.language != crate::app::FileLanguage::Smali {
                        return None;
                    }
                    let TabContent::Code(source) = &tab.content else {
                        return None;
                    };
                    let issues = crate::engine::smali_validator::SmaliValidator::validate(source);
                    if issues.is_empty() {
                        None
                    } else {
                        Some((tab.title.clone(), tab.path.clone(), issues))
                    }
                })
                .collect::<Vec<_>>()
        };

        if problems.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(24.0);
                ui.label(
                    egui::RichText::new("No problems detected")
                        .size(theme.font_ui)
                        .color(theme.text_muted),
                );
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("Smali validation issues appear here when editing.")
                        .size(theme.font_small)
                        .color(theme.text_disabled),
                );
            });
            return;
        }

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (title, path, issues) in problems {
                    ui.label(
                        egui::RichText::new(title)
                            .size(theme.font_ui)
                            .strong()
                            .color(theme.text_primary),
                    );
                    ui.label(
                        egui::RichText::new(path.display().to_string())
                            .size(theme.font_small)
                            .color(theme.text_disabled),
                    );
                    ui.add_space(4.0);

                    for issue in issues {
                        let (label, color) = match issue.severity {
                            ValidationSeverity::Error => ("ERR", theme.error),
                            ValidationSeverity::Warning => ("WARN", theme.warning),
                        };

                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                egui::RichText::new(label)
                                    .font(egui::FontId::monospace(9.0))
                                    .strong()
                                    .color(color),
                            );
                            ui.label(
                                egui::RichText::new(format!("line {}", issue.line))
                                    .font(egui::FontId::monospace(theme.font_small))
                                    .color(theme.text_muted),
                            );
                            ui.label(
                                egui::RichText::new(issue.message)
                                    .size(theme.font_small)
                                    .color(theme.console_text),
                            );
                        });
                    }
                    ui.add_space(10.0);
                }
            });
    }
}
