//! Vertical activity bar — VS Code-style icon strip on the far left.
//! Uses pixel-perfect Codicon-inspired vector icons for a polished look.

use crate::app::{AppState, SideBarView};
use crate::ui::theme::Theme;

use std::sync::{Arc, Mutex};

pub struct ActivityBar;

#[derive(Clone, Copy)]
pub enum IconType {
    Explorer,
    Search,
    Settings,
    Strings,
    Runtime,
    Preferences,
}

impl ActivityBar {
    pub fn render(ui: &mut egui::Ui, state: &Arc<Mutex<AppState>>) {
        let theme = Theme::current(ui);
        let active_view = { state.lock().unwrap().sidebar_view.clone() };

        ui.vertical_centered(|ui| {
            ui.add_space(8.0);
            ui.spacing_mut().item_spacing.y = 2.0;

            Self::icon_button(ui, &theme, IconType::Explorer, "Explorer", SideBarView::Explorer, &active_view, state);
            Self::icon_button(ui, &theme, IconType::Search, "Search", SideBarView::Search, &active_view, state);
            Self::icon_button(ui, &theme, IconType::Settings, "Native Analysis", SideBarView::NativeAnalysis, &active_view, state);
            Self::icon_button(ui, &theme, IconType::Strings, "Strings", SideBarView::Strings, &active_view, state);
            Self::icon_button(ui, &theme, IconType::Runtime, "Runtime / ADB", SideBarView::Runtime, &active_view, state);
            Self::icon_button(ui, &theme, IconType::Preferences, "Settings", SideBarView::Settings, &active_view, state);
        });
    }

    fn draw_icon(painter: &egui::Painter, rect: egui::Rect, icon: IconType, color: egui::Color32) {
        let cx = rect.center().x;
        let cy = rect.center().y;
        let stroke = egui::Stroke::new(1.4, color);

        match icon {
            IconType::Explorer => {
                // VS Code "files" icon — two overlapping document shapes
                // Back document
                let bx = cx - 1.0;
                let by = cy - 1.0;
                painter.line_segment([egui::pos2(bx - 4.0, by - 6.0), egui::pos2(bx + 3.0, by - 6.0)], stroke);
                painter.line_segment([egui::pos2(bx + 3.0, by - 6.0), egui::pos2(bx + 6.0, by - 3.0)], stroke);
                painter.line_segment([egui::pos2(bx + 6.0, by - 3.0), egui::pos2(bx + 6.0, by + 6.0)], stroke);
                painter.line_segment([egui::pos2(bx + 6.0, by + 6.0), egui::pos2(bx - 4.0, by + 6.0)], stroke);
                painter.line_segment([egui::pos2(bx - 4.0, by + 6.0), egui::pos2(bx - 4.0, by - 6.0)], stroke);
                // Front document (offset)
                let fx = cx + 1.0;
                let fy = cy + 1.0;
                painter.rect_filled(
                    egui::Rect::from_min_max(egui::pos2(fx - 6.0, fy - 4.0), egui::pos2(fx + 4.0, fy + 7.0)),
                    1.0,
                    egui::Color32::from_rgba_premultiplied(
                        color.r().saturating_sub(60),
                        color.g().saturating_sub(60),
                        color.b().saturating_sub(60),
                        30,
                    ),
                );
                painter.line_segment([egui::pos2(fx - 6.0, fy - 4.0), egui::pos2(fx + 1.0, fy - 4.0)], stroke);
                painter.line_segment([egui::pos2(fx + 1.0, fy - 4.0), egui::pos2(fx + 4.0, fy - 1.0)], stroke);
                painter.line_segment([egui::pos2(fx + 4.0, fy - 1.0), egui::pos2(fx + 4.0, fy + 7.0)], stroke);
                painter.line_segment([egui::pos2(fx + 4.0, fy + 7.0), egui::pos2(fx - 6.0, fy + 7.0)], stroke);
                painter.line_segment([egui::pos2(fx - 6.0, fy + 7.0), egui::pos2(fx - 6.0, fy - 4.0)], stroke);
                // Fold corner
                painter.line_segment([egui::pos2(fx + 1.0, fy - 4.0), egui::pos2(fx + 1.0, fy - 1.0)], stroke);
                painter.line_segment([egui::pos2(fx + 1.0, fy - 1.0), egui::pos2(fx + 4.0, fy - 1.0)], stroke);
            },
            IconType::Search => {
                // Magnifying glass — clean VS Code style
                let circle_center = egui::pos2(cx - 1.5, cy - 1.5);
                let r = 5.5;
                painter.circle_stroke(circle_center, r, egui::Stroke::new(1.6, color));
                // Handle (diagonal)
                let handle_start = egui::pos2(circle_center.x + r * 0.7, circle_center.y + r * 0.7);
                let handle_end = egui::pos2(cx + 7.0, cy + 7.0);
                painter.line_segment([handle_start, handle_end], egui::Stroke::new(2.0, color));
            },
            IconType::Settings => {
                // CPU/chip icon — represents analysis
                let s = 6.5;
                let it = 1.8_f32;
                // Center square
                painter.rect_stroke(
                    egui::Rect::from_center_size(egui::pos2(cx, cy), egui::vec2(s * 1.5, s * 1.5)),
                    1.5,
                    egui::Stroke::new(1.4, color),
                    egui::StrokeKind::Inside,
                );
                // Inner square (core)
                painter.rect_stroke(
                    egui::Rect::from_center_size(egui::pos2(cx, cy), egui::vec2(s * 0.7, s * 0.7)),
                    1.0,
                    egui::Stroke::new(1.2, color),
                    egui::StrokeKind::Inside,
                );
                // Pins — top
                for dx in [-2.5_f32, 0.0, 2.5] {
                    painter.line_segment([egui::pos2(cx + dx, cy - s * 0.75), egui::pos2(cx + dx, cy - s * 0.75 - it)], stroke);
                }
                // Pins — bottom
                for dx in [-2.5_f32, 0.0, 2.5] {
                    painter.line_segment([egui::pos2(cx + dx, cy + s * 0.75), egui::pos2(cx + dx, cy + s * 0.75 + it)], stroke);
                }
                // Pins — left
                for dy in [-2.5_f32, 0.0, 2.5] {
                    painter.line_segment([egui::pos2(cx - s * 0.75, cy + dy), egui::pos2(cx - s * 0.75 - it, cy + dy)], stroke);
                }
                // Pins — right
                for dy in [-2.5_f32, 0.0, 2.5] {
                    painter.line_segment([egui::pos2(cx + s * 0.75, cy + dy), egui::pos2(cx + s * 0.75 + it, cy + dy)], stroke);
                }
            },
            IconType::Strings => {
                // Text/A icon — single large letter with cursor
                let bold_stroke = egui::Stroke::new(1.6, color);
                // Letter A shape
                painter.line_segment([egui::pos2(cx - 5.0, cy + 6.0), egui::pos2(cx, cy - 6.0)], bold_stroke);
                painter.line_segment([egui::pos2(cx, cy - 6.0), egui::pos2(cx + 5.0, cy + 6.0)], bold_stroke);
                // Crossbar
                painter.line_segment([egui::pos2(cx - 3.0, cy + 1.0), egui::pos2(cx + 3.0, cy + 1.0)], bold_stroke);
                // Cursor blink line
                painter.line_segment([egui::pos2(cx + 7.0, cy - 4.0), egui::pos2(cx + 7.0, cy + 4.0)], egui::Stroke::new(1.2, color));
            },
            IconType::Runtime => {
                // Play button — filled triangle (VS Code debug style)
                let s = 7.0;
                let points = vec![
                    egui::pos2(cx - s * 0.5, cy - s),
                    egui::pos2(cx + s * 0.8, cy),
                    egui::pos2(cx - s * 0.5, cy + s),
                ];
                // Filled play triangle
                let fill_color = egui::Color32::from_rgba_premultiplied(color.r(), color.g(), color.b(), 40);
                painter.add(egui::Shape::convex_polygon(
                    points.clone(),
                    fill_color,
                    egui::Stroke::new(1.5, color),
                ));
            },
            IconType::Preferences => {
                for (dy, knob_x) in [(-5.0_f32, cx - 3.0), (0.0, cx + 4.0), (5.0, cx - 1.0)] {
                    let y = cy + dy;
                    painter.line_segment([egui::pos2(cx - 7.0, y), egui::pos2(cx + 7.0, y)], stroke);
                    painter.circle_filled(egui::pos2(knob_x, y), 2.0, color);
                }
            },
        }
    }

    fn icon_button(
        ui: &mut egui::Ui,
        theme: &Theme,
        icon: IconType,
        tooltip: &str,
        view: SideBarView,
        active: &SideBarView,
        state: &Arc<Mutex<AppState>>,
    ) {
        let is_active = *active == view;
        let size = 48.0;

        let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click());

        // Smooth hover animation
        let hover_t = ui.ctx().animate_bool_with_time(
            egui::Id::new(format!("activity_hover_{:?}", view)),
            response.hovered(),
            0.15
        );

        let icon_color = if is_active {
            theme.activity_bar_active_icon
        } else if hover_t > 0.01 {
            theme.text_primary
        } else {
            theme.activity_bar_icon
        };

        ui.painter().rect_filled(rect, 0.0, theme.activity_bar_bg);

        if is_active {
            ui.painter().rect_filled(rect, 0.0, theme.tree_selected_bg);
        }

        // Hover highlight with smooth fade
        if hover_t > 0.0 && !is_active {
            let hover = if theme.is_dark {
                theme.tree_hover_bg
            } else {
                egui::Color32::from_rgb(232, 236, 244)
            };
            ui.painter().rect_filled(rect, 0.0, hover.linear_multiply(hover_t));
        }

        // Active indicator — left border accent bar with glow
        if is_active {
            let indicator = egui::Rect::from_min_size(
                egui::pos2(rect.left(), rect.top() + 8.0),
                egui::vec2(3.0, rect.height() - 16.0),
            );
            ui.painter().rect_filled(indicator, 1.5, theme.activity_bar_active_border);
        }

        // Draw icon with slight scale on hover
        let icon_rect = if hover_t > 0.0 && !is_active {
            let scale = 1.0 + (hover_t * 0.04);
            let center = rect.center();
            egui::Rect::from_center_size(center, egui::vec2(size * scale, size * scale))
        } else {
            rect
        };
        
        Self::draw_icon(ui.painter(), icon_rect, icon, icon_color);

        if response.clicked() {
            state.lock().unwrap().sidebar_view = view;
        }

        response.on_hover_text_at_pointer(tooltip);
    }
}
