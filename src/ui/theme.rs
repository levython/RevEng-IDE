//! Centralized theme system for the entire IDE.
//!
//! Stores every color and dimension in one place. Access the current theme
//! from any UI component via `Theme::current(ui)`.

use std::sync::Arc;

/// Centralized theme definition for the entire IDE.
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct Theme {
    // --- Surface colors ---
    pub bg_primary: egui::Color32,
    pub bg_secondary: egui::Color32,
    pub bg_tertiary: egui::Color32,
    pub bg_elevated: egui::Color32,
    pub bg_input: egui::Color32,

    // --- Border & separator ---
    pub border: egui::Color32,
    pub border_subtle: egui::Color32,
    pub separator: egui::Color32,

    // --- Text colors ---
    pub text_primary: egui::Color32,
    pub text_secondary: egui::Color32,
    pub text_muted: egui::Color32,
    pub text_disabled: egui::Color32,
    pub text_accent: egui::Color32,

    // --- Accent colors ---
    pub accent_primary: egui::Color32,
    pub accent_hover: egui::Color32,
    pub accent_active: egui::Color32,
    pub accent_subtle: egui::Color32,

    // --- Semantic colors ---
    pub success: egui::Color32,
    pub warning: egui::Color32,
    pub error: egui::Color32,
    pub info: egui::Color32,

    // --- Status bar ---
    pub status_bar_bg: egui::Color32,
    pub status_bar_text: egui::Color32,
    pub status_bar_busy: egui::Color32,
    pub status_bar_error: egui::Color32,

    // --- Editor ---
    pub editor_bg: egui::Color32,
    pub editor_gutter_bg: egui::Color32,
    pub editor_gutter_text: egui::Color32,
    pub editor_line_highlight: egui::Color32,
    pub editor_selection: egui::Color32,
    pub editor_find_match: egui::Color32,
    /// Vertical indent guide lines inside the code area.
    pub editor_indent_guide: egui::Color32,
    /// Background highlight for matching bracket pairs.
    pub editor_bracket_match_bg: egui::Color32,
    /// Translucent viewport indicator on the minimap.
    pub minimap_viewport: egui::Color32,
    /// Hex viewer address column color.
    pub hex_address_color: egui::Color32,

    // --- Tab bar ---
    pub tab_active_bg: egui::Color32,
    pub tab_inactive_bg: egui::Color32,
    pub tab_active_text: egui::Color32,
    pub tab_inactive_text: egui::Color32,
    pub tab_hover_bg: egui::Color32,
    pub tab_border: egui::Color32,
    pub tab_accent: egui::Color32,

    // --- File tree ---
    pub tree_indent_guide: egui::Color32,
    pub tree_hover_bg: egui::Color32,
    pub tree_selected_bg: egui::Color32,

    // --- Activity bar ---
    pub activity_bar_bg: egui::Color32,
    pub activity_bar_icon: egui::Color32,
    pub activity_bar_active_icon: egui::Color32,
    pub activity_bar_active_border: egui::Color32,

    // --- Console ---
    pub console_tag: egui::Color32,
    pub console_text: egui::Color32,
    pub console_timestamp: egui::Color32,
    pub console_alt_row: egui::Color32,

    // --- Terminal ---
    pub terminal_bg: egui::Color32,

    // --- Syntax highlighting ---
    pub syn_keyword: egui::Color32,
    pub syn_string: egui::Color32,
    pub syn_comment: egui::Color32,
    pub syn_function: egui::Color32,
    pub syn_type: egui::Color32,
    pub syn_number: egui::Color32,
    pub syn_operator: egui::Color32,
    pub syn_label: egui::Color32,
    pub syn_instruction: egui::Color32,
    pub syn_tag: egui::Color32,

    // --- Disassembly ---
    pub disasm_address: egui::Color32,
    pub disasm_bytes: egui::Color32,
    pub disasm_mnemonic: egui::Color32,
    pub disasm_operand: egui::Color32,

    // --- Dimensions ---
    pub corner_radius: f32,
    pub border_width: f32,

    // --- Font sizes ---
    pub font_code: f32,
    pub font_ui: f32,
    pub font_small: f32,
    pub font_heading: f32,

    // --- Is dark mode ---
    pub is_dark: bool,
}

const THEME_ID: &str = "reveng_theme";

impl Theme {
    /// VS Code-inspired dark theme.
    pub fn dark() -> Self {
        Self {
            bg_primary: egui::Color32::from_rgb(30, 30, 30),
            bg_secondary: egui::Color32::from_rgb(37, 37, 38),
            bg_tertiary: egui::Color32::from_rgb(30, 30, 30),
            bg_elevated: egui::Color32::from_rgb(45, 45, 45),
            bg_input: egui::Color32::from_rgb(36, 36, 36),

            border: egui::Color32::from_rgb(62, 62, 66),
            border_subtle: egui::Color32::from_rgb(51, 51, 51),
            separator: egui::Color32::from_rgb(48, 48, 48),

            text_primary: egui::Color32::from_rgb(204, 204, 204),
            text_secondary: egui::Color32::from_rgb(170, 170, 170),
            text_muted: egui::Color32::from_rgb(133, 133, 133),
            text_disabled: egui::Color32::from_rgb(90, 90, 90),
            text_accent: egui::Color32::from_rgb(78, 201, 176),

            accent_primary: egui::Color32::from_rgb(0, 122, 204),
            accent_hover: egui::Color32::from_rgb(17, 139, 220),
            accent_active: egui::Color32::from_rgb(9, 71, 113),
            accent_subtle: egui::Color32::from_rgb(37, 60, 80),

            // Semantic
            success: egui::Color32::from_rgb(166, 227, 161),       // #a6e3a1
            warning: egui::Color32::from_rgb(249, 226, 175),       // #f9e2af
            error: egui::Color32::from_rgb(243, 139, 168),         // #f38ba8
            info: egui::Color32::from_rgb(137, 180, 250),          // #89b4fa

            // Status bar
            status_bar_bg: egui::Color32::from_rgb(0, 122, 204),
            status_bar_text: egui::Color32::from_rgb(255, 255, 255),
            status_bar_busy: egui::Color32::from_rgb(223, 142, 29),// #df8e1d - orange
            status_bar_error: egui::Color32::from_rgb(210, 60, 80),

            // Editor
            editor_bg: egui::Color32::from_rgb(30, 30, 30),
            editor_gutter_bg: egui::Color32::from_rgb(30, 30, 30),
            editor_gutter_text: egui::Color32::from_rgb(120, 120, 120),
            editor_line_highlight: egui::Color32::from_rgb(42, 45, 46),
            editor_selection: egui::Color32::from_rgb(38, 79, 120),
            editor_find_match: egui::Color32::from_rgba_premultiplied(250, 200, 80, 50),
            editor_indent_guide: egui::Color32::TRANSPARENT,
            editor_bracket_match_bg: egui::Color32::from_rgba_premultiplied(137, 180, 250, 55),
            minimap_viewport: egui::Color32::from_rgba_premultiplied(137, 180, 250, 40),
            hex_address_color: egui::Color32::from_rgb(108, 112, 134),

            // Tabs - Enhanced
            tab_active_bg: egui::Color32::from_rgb(30, 30, 30),
            tab_inactive_bg: egui::Color32::from_rgb(45, 45, 45),
            tab_active_text: egui::Color32::from_rgb(255, 255, 255),
            tab_inactive_text: egui::Color32::from_rgb(150, 150, 150),
            tab_hover_bg: egui::Color32::from_rgb(68, 68, 70),
            tab_border: egui::Color32::from_rgb(37, 37, 38),
            tab_accent: egui::Color32::from_rgb(0, 122, 204),

            // File tree - Better hover states
            tree_indent_guide: egui::Color32::from_rgb(64, 64, 64),
            tree_hover_bg: egui::Color32::from_rgb(62, 62, 64),
            tree_selected_bg: egui::Color32::from_rgb(55, 55, 55),

            // Activity bar - Enhanced
            activity_bar_bg: egui::Color32::from_rgb(51, 51, 51),
            activity_bar_icon: egui::Color32::from_rgb(133, 133, 133),
            activity_bar_active_icon: egui::Color32::from_rgb(255, 255, 255),
            activity_bar_active_border: egui::Color32::from_rgb(0, 122, 204),

            // Console
            console_tag: egui::Color32::from_rgb(180, 190, 254),   // #b4befe
            console_text: egui::Color32::from_rgb(186, 194, 222),  // #bac2de
            console_timestamp: egui::Color32::from_rgb(88, 91, 112),
            console_alt_row: egui::Color32::from_rgb(26, 26, 40),

            // Terminal
            terminal_bg: egui::Color32::from_rgb(26, 26, 26),

            // Syntax
            syn_keyword: egui::Color32::from_rgb(203, 166, 247),   // #cba6f7 mauve
            syn_string: egui::Color32::from_rgb(166, 227, 161),    // #a6e3a1 green
            syn_comment: egui::Color32::from_rgb(108, 112, 134),   // #6c7086 overlay0
            syn_function: egui::Color32::from_rgb(137, 180, 250),  // #89b4fa blue
            syn_type: egui::Color32::from_rgb(249, 226, 175),      // #f9e2af yellow
            syn_number: egui::Color32::from_rgb(250, 179, 135),    // #fab387 peach
            syn_operator: egui::Color32::from_rgb(148, 226, 213),  // #94e2d5 teal
            syn_label: egui::Color32::from_rgb(249, 226, 175),     // #f9e2af yellow
            syn_instruction: egui::Color32::from_rgb(245, 194, 231),// #f5c2e7 pink
            syn_tag: egui::Color32::from_rgb(116, 199, 236),       // #74c7ec sapphire

            // Disassembly
            disasm_address: egui::Color32::from_rgb(108, 112, 134),
            disasm_bytes: egui::Color32::from_rgb(88, 91, 112),
            disasm_mnemonic: egui::Color32::from_rgb(203, 166, 247),
            disasm_operand: egui::Color32::from_rgb(205, 214, 244),

            // Dimensions — VS Code proportions
            corner_radius: 2.0,
            border_width: 1.0,

            // Font sizes — VS Code-like hierarchy
            font_code: 14.0,
            font_ui: 13.0,
            font_small: 11.5,
            font_heading: 14.0,

            is_dark: true,
        }
    }

    /// Warm professional light theme (Catppuccin Latte-inspired).
    pub fn light() -> Self {
        Self {
            // Surfaces
            bg_primary: egui::Color32::from_rgb(239, 241, 245),    // #eff1f5
            bg_secondary: egui::Color32::from_rgb(230, 233, 239),  // #e6e9ef
            bg_tertiary: egui::Color32::from_rgb(220, 224, 232),   // #dce0e8
            bg_elevated: egui::Color32::from_rgb(255, 255, 255),   // white
            bg_input: egui::Color32::from_rgb(248, 249, 252),      // brighter text inputs

            // Borders
            border: egui::Color32::from_rgb(188, 192, 204),        // #bcc0cc
            border_subtle: egui::Color32::from_rgb(204, 208, 218), // #ccd0da
            separator: egui::Color32::from_rgb(204, 208, 218),     // #ccd0da

            // Text
            text_primary: egui::Color32::from_rgb(76, 79, 105),    // #4c4f69
            text_secondary: egui::Color32::from_rgb(108, 111, 133),// #6c6f85
            text_muted: egui::Color32::from_rgb(140, 143, 161),    // #8c8fa1
            text_disabled: egui::Color32::from_rgb(172, 176, 190), // #acb0be
            text_accent: egui::Color32::from_rgb(30, 102, 245),    // #1e66f5

            // Accent
            accent_primary: egui::Color32::from_rgb(30, 102, 245), // #1e66f5
            accent_hover: egui::Color32::from_rgb(18, 88, 225),
            accent_active: egui::Color32::from_rgb(16, 72, 192),
            accent_subtle: egui::Color32::from_rgb(217, 229, 252),

            // Semantic
            success: egui::Color32::from_rgb(64, 160, 43),         // #40a02b
            warning: egui::Color32::from_rgb(223, 142, 29),        // #df8e1d
            error: egui::Color32::from_rgb(210, 15, 57),           // #d20f39
            info: egui::Color32::from_rgb(30, 102, 245),           // #1e66f5

            // Status bar
            status_bar_bg: egui::Color32::from_rgb(30, 102, 245),
            status_bar_text: egui::Color32::from_rgb(255, 255, 255),
            status_bar_busy: egui::Color32::from_rgb(223, 142, 29),
            status_bar_error: egui::Color32::from_rgb(210, 15, 57),

            // Editor
            editor_bg: egui::Color32::from_rgb(239, 241, 245),
            editor_gutter_bg: egui::Color32::from_rgb(230, 233, 239),
            editor_gutter_text: egui::Color32::from_rgb(140, 143, 161),
            editor_line_highlight: egui::Color32::from_rgba_premultiplied(0, 0, 0, 8),
            editor_selection: egui::Color32::from_rgb(188, 192, 204),
            editor_find_match: egui::Color32::from_rgba_premultiplied(250, 200, 80, 60),
            editor_indent_guide: egui::Color32::from_rgba_premultiplied(120, 130, 160, 70),
            editor_bracket_match_bg: egui::Color32::from_rgba_premultiplied(30, 102, 245, 45),
            minimap_viewport: egui::Color32::from_rgba_premultiplied(0, 0, 0, 20),
            hex_address_color: egui::Color32::from_rgb(140, 143, 161),

            // Tabs
            tab_active_bg: egui::Color32::from_rgb(239, 241, 245),
            tab_inactive_bg: egui::Color32::from_rgb(220, 224, 232),
            tab_active_text: egui::Color32::from_rgb(76, 79, 105),
            tab_inactive_text: egui::Color32::from_rgb(140, 143, 161),
            tab_hover_bg: egui::Color32::from_rgb(230, 233, 239),
            tab_border: egui::Color32::from_rgb(204, 208, 218),
            tab_accent: egui::Color32::from_rgb(30, 102, 245),

            // File tree
            tree_indent_guide: egui::Color32::from_rgb(204, 208, 218),
            tree_hover_bg: egui::Color32::from_rgb(221, 228, 241),
            tree_selected_bg: egui::Color32::from_rgb(201, 216, 247),

            // Activity bar
            activity_bar_bg: egui::Color32::from_rgb(220, 224, 232),
            activity_bar_icon: egui::Color32::from_rgb(118, 123, 146),
            activity_bar_active_icon: egui::Color32::from_rgb(76, 79, 105),
            activity_bar_active_border: egui::Color32::from_rgb(30, 102, 245),

            // Console
            console_tag: egui::Color32::from_rgb(114, 135, 253),   // #7287fd
            console_text: egui::Color32::from_rgb(76, 79, 105),
            console_timestamp: egui::Color32::from_rgb(140, 143, 161),
            console_alt_row: egui::Color32::from_rgb(230, 233, 239),

            // Terminal
            terminal_bg: egui::Color32::from_rgb(248, 248, 248),

            // Syntax
            syn_keyword: egui::Color32::from_rgb(136, 57, 239),    // #8839ef
            syn_string: egui::Color32::from_rgb(64, 160, 43),      // #40a02b
            syn_comment: egui::Color32::from_rgb(140, 143, 161),   // #8c8fa1
            syn_function: egui::Color32::from_rgb(30, 102, 245),   // #1e66f5
            syn_type: egui::Color32::from_rgb(223, 142, 29),       // #df8e1d
            syn_number: egui::Color32::from_rgb(254, 100, 11),     // #fe640b
            syn_operator: egui::Color32::from_rgb(23, 146, 153),   // #179299
            syn_label: egui::Color32::from_rgb(223, 142, 29),      // #df8e1d
            syn_instruction: egui::Color32::from_rgb(234, 118, 203),// #ea76cb
            syn_tag: egui::Color32::from_rgb(4, 165, 229),         // #04a5e5

            // Disassembly
            disasm_address: egui::Color32::from_rgb(140, 143, 161),
            disasm_bytes: egui::Color32::from_rgb(172, 176, 190),
            disasm_mnemonic: egui::Color32::from_rgb(136, 57, 239),
            disasm_operand: egui::Color32::from_rgb(76, 79, 105),

            // Dimensions — tighter VS Code proportions
            corner_radius: 4.0,
            border_width: 1.0,

            // Font sizes — VS Code-like hierarchy
            font_code: 13.0,
            font_ui: 13.0,
            font_small: 11.0,
            font_heading: 14.0,

            is_dark: false,
        }
    }

    /// Apply this theme to the egui context's visuals and store it for access.
    pub fn apply(&self, ctx: &egui::Context) {
        let mut style = (*ctx.style()).clone();
        let cr = self.corner_radius as u8;

        if self.is_dark {
            style.visuals = egui::Visuals::dark();
        } else {
            style.visuals = egui::Visuals::light();
        }

        style.visuals.window_fill = self.bg_primary;
        style.visuals.panel_fill = self.bg_secondary;
        style.visuals.extreme_bg_color = self.bg_input;
        style.visuals.faint_bg_color = self.bg_tertiary;
        style.visuals.selection.bg_fill = self.editor_selection;
        style.visuals.selection.stroke = egui::Stroke::new(0.0, egui::Color32::TRANSPARENT);
        style.text_styles.insert(
            egui::TextStyle::Monospace,
            egui::FontId::new(self.font_code, egui::FontFamily::Monospace),
        );
        style.text_styles.insert(
            egui::TextStyle::Body,
            egui::FontId::new(self.font_ui, egui::FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Button,
            egui::FontId::new(self.font_ui, egui::FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Small,
            egui::FontId::new(self.font_small, egui::FontFamily::Proportional),
        );
        style.text_styles.insert(
            egui::TextStyle::Heading,
            egui::FontId::new(self.font_heading, egui::FontFamily::Proportional),
        );

        // Widget styling
        style.visuals.widgets.noninteractive.bg_fill = self.bg_secondary;
        style.visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, self.text_primary);
        style.visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(self.border_width, self.border_subtle);
        style.visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(cr);

        style.visuals.widgets.inactive.bg_fill = self.bg_elevated;
        style.visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, self.text_secondary);
        style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(self.border_width, self.border_subtle);
        style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(cr);

        style.visuals.widgets.hovered.bg_fill = self.tree_hover_bg;
        style.visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, self.text_primary);
        style.visuals.widgets.hovered.bg_stroke = egui::Stroke::new(self.border_width, self.border);
        style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(cr);

        style.visuals.widgets.active.bg_fill = self.tree_selected_bg;
        style.visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, self.text_primary);
        style.visuals.widgets.active.bg_stroke = egui::Stroke::new(self.border_width, self.border);
        style.visuals.widgets.active.corner_radius = egui::CornerRadius::same(cr);

        style.visuals.widgets.open.bg_fill = self.tree_selected_bg;
        style.visuals.widgets.open.fg_stroke = egui::Stroke::new(1.0, self.text_primary);
        style.visuals.widgets.open.bg_stroke = egui::Stroke::new(self.border_width, self.accent_primary);
        style.visuals.widgets.open.corner_radius = egui::CornerRadius::same(cr);

        // Separator
        style.visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, self.separator);

        // Window shadow
        style.visuals.window_shadow = if self.is_dark {
            egui::Shadow { offset: [0, 4], blur: 16, spread: 0, color: egui::Color32::from_black_alpha(80) }
        } else {
            egui::Shadow { offset: [0, 2], blur: 12, spread: 0, color: egui::Color32::from_black_alpha(30) }
        };

        // Spacing — VS Code compact density
        style.spacing.item_spacing = egui::vec2(6.0, 3.0);
        style.spacing.window_margin = egui::Margin::same(6);
        style.spacing.button_padding = egui::vec2(8.0, 4.0);
        style.spacing.interact_size = egui::vec2(28.0, 18.0);

        ctx.set_style(style);

        // Store theme in context memory for access from any component
        ctx.data_mut(|d| d.insert_temp(egui::Id::new(THEME_ID), Arc::new(self.clone())));
    }

    /// Retrieve the current theme from egui's context memory.
    pub fn current(ui: &egui::Ui) -> Arc<Theme> {
        ui.data(|d| d.get_temp::<Arc<Theme>>(egui::Id::new(THEME_ID)))
            .unwrap_or_else(|| Arc::new(Theme::dark()))
    }

    /// Retrieve theme from a Context directly.
    pub fn from_ctx(ctx: &egui::Context) -> Arc<Theme> {
        ctx.data(|d| d.get_temp::<Arc<Theme>>(egui::Id::new(THEME_ID)))
            .unwrap_or_else(|| Arc::new(Theme::dark()))
    }

    /// Get log level color using the theme's semantic colors.
    pub fn log_level_color(&self, level: &crate::app::LogLevel) -> egui::Color32 {
        match level {
            crate::app::LogLevel::Info => self.success,
            crate::app::LogLevel::Warn => self.warning,
            crate::app::LogLevel::Error => self.error,
            crate::app::LogLevel::Debug => self.info,
        }
    }

    /// Get a compact file badge label and color for file tree display.
    pub fn file_icon(&self, ext: &str) -> (&'static str, egui::Color32) {
        match ext.to_lowercase().as_str() {
            "java" => ("JV", egui::Color32::from_rgb(232, 150, 72)),
            "kt" | "kts" => ("KT", egui::Color32::from_rgb(139, 183, 255)),
            "smali" => ("SM", egui::Color32::from_rgb(120, 200, 120)),
            "xml" | "html" | "svg" => ("</>", egui::Color32::from_rgb(100, 160, 255)),
            "json" => ("{}", egui::Color32::from_rgb(255, 200, 80)),
            "yaml" | "yml" => ("YM", egui::Color32::from_rgb(137, 220, 235)),
            "properties" | "cfg" | "conf" | "ini" | "toml" => ("CFG", egui::Color32::from_rgb(210, 188, 142)),
            "md" | "txt" | "log" => ("TXT", egui::Color32::from_rgb(166, 173, 200)),
            "js" => ("JS", egui::Color32::from_rgb(255, 220, 90)),
            "ts" => ("TS", egui::Color32::from_rgb(90, 170, 255)),
            "css" => ("CSS", egui::Color32::from_rgb(120, 170, 255)),
            "sh" | "bash" | "zsh" => ("SH", egui::Color32::from_rgb(166, 227, 161)),
            "ps1" => ("PS", egui::Color32::from_rgb(100, 170, 240)),
            "so" => ("SO", self.syn_keyword),
            "dex" | "odex" | "oat" | "vdex" => ("DEX", egui::Color32::from_rgb(100, 180, 220)),
            "arsc" => ("RES", egui::Color32::from_rgb(180, 180, 130)),
            "apk" | "xapk" => ("APK", egui::Color32::from_rgb(130, 220, 160)),
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" => ("IMG", egui::Color32::from_rgb(180, 150, 200)),
            _ => ("..", self.text_muted),
        }
    }
}
