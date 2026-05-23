//! Root application state and eframe integration.

use crate::engine::toolchain::{ToolUpdateCandidate, ToolchainManager};
use crate::engine::workspace::WorkspaceManager;
use crate::ui::layout::IdeLayout;

use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub enum TabContent {
    Code(String),
    Native(crate::native::elf::ElfInfo, Vec<crate::native::disasm::DisasmInstruction>),
    /// Raw bytes displayed in the hex viewer.
    Hex(Vec<u8>),
}

/// Which view the left sidebar shows.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SideBarView {
    Explorer,
    Search,
    NativeAnalysis,
    Runtime,
    Strings,
    AppStudio,
    Settings,
}

/// Command palette mode.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum PaletteMode {
    Commands,
    Files,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedEditorTab {
    pub path: String,
    pub target_line: Option<usize>,
    pub search_query: String,
    pub replace_query: String,
    pub search_visible: bool,
    pub replace_visible: bool,
    pub right_split: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedProject {
    pub version: u32,
    pub apk_path: Option<String>,
    pub workspace_root: Option<String>,
    pub active_tab: Option<usize>,
    pub active_tab_right: Option<usize>,
    pub global_search_query: String,
    pub sidebar_view: SideBarView,
    pub tabs: Vec<SavedEditorTab>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub dark_mode: bool,
    pub editor_font_size: f32,
    pub ui_font_size: f32,
    pub line_height: f32,
    pub auto_save: bool,
    pub word_wrap: bool,
    pub show_bottom_panel: bool,
    pub check_tool_updates_on_startup: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            dark_mode: true,
            editor_font_size: 14.0,
            ui_font_size: 13.0,
            line_height: 1.42,
            auto_save: false,
            word_wrap: false,
            show_bottom_panel: true,
            check_tool_updates_on_startup: true,
        }
    }
}

impl AppSettings {
    pub fn path() -> Option<std::path::PathBuf> {
        dirs::config_dir().map(|dir| dir.join("reveng-ide").join("settings.json"))
    }

    pub fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };
        let Ok(text) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        serde_json::from_str(&text).unwrap_or_default()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let Some(path) = Self::path() else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}

/// Shared application state accessible from any UI panel or engine subsystem.
pub struct AppState {
    /// Manages bundled external tools (JADX, APKTool, ADB, etc.)
    pub toolchain: ToolchainManager,
    /// Manages the current APK workspace.
    pub workspace: WorkspaceManager,
    /// Log lines displayed in the console panel.
    pub console_log: Vec<LogEntry>,
    /// Currently open editor tabs.
    pub open_tabs: Vec<EditorTab>,
    /// Index of the active tab (if any).
    pub active_tab: Option<usize>,
    /// Index of the active right tab for split view.
    pub active_tab_right: Option<usize>,
    /// Status bar message.
    pub status_message: String,
    /// Whether a background task is running.
    pub busy: bool,
    /// Navigation index for Java-to-Smali jumps.
    pub nav_index: crate::engine::navigation::NavIndex,
    /// User-configurable preferences.
    pub settings: AppSettings,
    /// Startup tool update check is running.
    pub tool_update_checking: bool,
    /// Tool update installation is running.
    pub tool_update_installing: bool,
    /// Available tool updates awaiting user action.
    pub tool_updates_available: Vec<ToolUpdateCandidate>,
    /// Last tool update error, displayed in settings/output.
    pub tool_update_error: Option<String>,
    /// Theme preference.
    pub dark_mode: bool,
    /// Global search results.
    pub search_results: Vec<crate::engine::patch::SearchResult>,
    /// Search query for global search.
    pub global_search_query: String,
    /// Recently opened APKs.
    pub recent_apks: Vec<std::path::PathBuf>,
    /// Current project file path, if one has been opened or saved.
    pub current_project_path: Option<std::path::PathBuf>,
    /// Whether to show the help modal.
    pub show_help: bool,
    /// Background task receiver for log messages.
    pub log_rx: Option<std::sync::mpsc::Receiver<LogEntry>>,
    /// Log sender (cloneable, handed to background tasks).
    pub log_tx: std::sync::mpsc::Sender<LogEntry>,
    /// Active sidebar view.
    pub sidebar_view: SideBarView,
    /// Command palette visibility.
    pub show_command_palette: bool,
    /// Command palette mode.
    pub palette_mode: PaletteMode,
    /// Cross-reference database built from smali files.
    pub xref_db: Option<crate::engine::xref::SmaliXrefDb>,
    /// Extracted strings from smali + resources.
    pub extracted_strings: Vec<crate::engine::strings::ExtractedString>,
    /// Current xref results (callers, usages, etc.) shown in search panel.
    pub xref_results: Vec<crate::engine::xref::CodeSite>,
    /// String filter category for the strings panel.
    pub string_filter: Option<crate::engine::strings::StringCategory>,
    /// String search query for filtering extracted strings.
    pub string_search_query: String,
    /// Increments whenever the strings dataset is rebuilt.
    pub strings_revision: u64,
    /// Shared cached snapshot of extracted strings for the sidebar UI.
    pub strings_view_cache: Option<Arc<Vec<crate::engine::strings::ExtractedString>>>,
    /// Editable package name value for the App Studio side panel.
    pub app_studio_package_name: String,
    /// Icon file path selected in the App Studio side panel.
    pub app_studio_icon_path: String,
    /// Symbol or class input for generated Frida script templates.
    pub app_studio_symbol_input: String,
    /// Custom rule file path for rule-engine scanning.
    pub app_studio_rule_file: String,
    /// Decoded APK directory to compare against the active decoded workspace.
    pub app_studio_diff_dir: String,
    /// Class rename expression in the form old.Class -> new.Class.
    pub app_studio_class_rename: String,
    /// Session note text input.
    pub app_studio_note_input: String,
    /// Latest App Studio report (displayed in panel).
    pub app_studio_report: String,
    /// Parsed manifest info.
    pub manifest_info: Option<crate::engine::manifest::ManifestInfo>,
    /// DEX statistics.
    pub dex_stats: Option<crate::engine::dex_stats::DexStats>,
    /// ADB shell input buffer.
    pub adb_shell_input: String,
    /// Package name used by Runtime ADB package actions.
    pub adb_package_input: String,
    /// Remote device path used by ADB pull.
    pub adb_pull_remote: String,
    /// Local destination path used by ADB pull.
    pub adb_pull_local: String,
    /// Local source path used by ADB push.
    pub adb_push_local: String,
    /// Remote device destination path used by ADB push.
    pub adb_push_remote: String,

    // ── Frida ──────────────────────────────────────────────────────────────
    /// Processes visible in the Frida process list.
    pub frida_processes: Vec<crate::runtime::frida::FridaProcess>,
    /// Package name or process name to attach/spawn.
    pub frida_attach_target: String,
    /// Current Frida JS script in the editor.
    pub frida_script: String,
    /// true = spawn a fresh app instance; false = attach to running process.
    pub frida_spawn_mode: bool,
    /// Index of the selected script template.
    pub frida_selected_template: usize,
    /// Whether a Frida session is currently running.
    pub frida_attached: bool,
    /// OS PID of the running `frida` child process, used to kill it.
    pub frida_child_pid: Option<u32>,
    /// Target Android ABI for frida-server download (arm64 / arm / x86_64).
    pub frida_server_arch: String,
    /// Results from the last APKiD analysis run.
    pub apkid_results: Option<Vec<crate::engine::apkid::ApkIdResult>>,
    /// Whether the loaded APK is a Flutter app (libflutter.so detected).
    pub is_flutter_app: bool,
    /// Paths to all libflutter.so files found in extracted native libs.
    pub flutter_lib_paths: Vec<std::path::PathBuf>,
    /// Path to libapp.so (Dart AOT snapshot), if present.
    pub libapp_path: Option<std::path::PathBuf>,
    /// Flutter engine version string extracted from libflutter.so.
    pub flutter_version: Option<String>,
    /// All extracted .so paths (for the native lib browser).
    pub native_lib_paths: Vec<std::path::PathBuf>,
    // ── Terminal State ──────────────────────────────────────────────────────
    /// Terminal output lines
    pub terminal_output: std::collections::VecDeque<TermOutputLine>,
    /// Terminal input buffer
    pub terminal_input: String,
    /// Terminal command history
    pub terminal_history: Vec<String>,
    /// Current position in terminal history
    pub terminal_history_index: usize,
}

#[derive(Clone, Debug)]
pub struct TermOutputLine {
    pub text: String,
    pub kind: TermOutputKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TermOutputKind {
    Stdout,
    Stderr,
    System,
}

#[derive(Clone, Debug)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: LogLevel,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
    Debug,
}

impl LogLevel {
    pub fn label(&self) -> &str {
        match self {
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERR ",
            LogLevel::Debug => "DBG ",
        }
    }
}

#[derive(Clone, Debug)]
pub struct EditorTab {
    pub title: String,
    pub path: std::path::PathBuf,
    pub content: TabContent,
    pub language: FileLanguage,
    pub modified: bool,
    pub cursor_pos: usize,
    pub search_query: String,
    pub replace_query: String,
    pub search_visible: bool,
    pub replace_visible: bool,
    pub target_line: Option<usize>,
    // ── Visual enhancements ──
    /// Matched bracket pair (char indices). Updated every frame.
    pub bracket_match: Option<(usize, usize)>,
    /// Last recorded vertical scroll offset (for minimap viewport sync).
    pub scroll_offset_y: f32,
    // ── Autocomplete ──
    pub autocomplete_visible: bool,
    pub autocomplete_query: String,
    pub autocomplete_suggestions: Vec<crate::engine::smali_complete::Suggestion>,
    pub autocomplete_sel: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FileLanguage {
    Java,
    Smali,
    Xml,
    Json,
    Unknown,
}

impl FileLanguage {
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "java" => Self::Java,
            "smali" => Self::Smali,
            "xml" => Self::Xml,
            "json" => Self::Json,
            _ => Self::Unknown,
        }
    }

    pub fn label(&self) -> &str {
        match self {
            Self::Java => "Java",
            Self::Smali => "Smali",
            Self::Xml => "XML",
            Self::Json => "JSON",
            Self::Unknown => "Text",
        }
    }

    pub fn syntect_name(&self) -> &str {
        match self {
            Self::Java => "java",
            Self::Smali => "bash", // No smali in syntect by default, fallback to bash-like syntax
            Self::Xml => "xml",
            Self::Json => "json",
            Self::Unknown => "txt",
        }
    }
}

impl AppState {
    const MAX_HEX_VIEW_BYTES: u64 = 128 * 1024 * 1024;
    const MAX_TEXT_EDITOR_BYTES: u64 = 32 * 1024 * 1024;
    const MAX_NATIVE_VIEW_BYTES: u64 = 512 * 1024 * 1024;
    const MAX_CONSOLE_LOG_ENTRIES: usize = 5_000;
    const MAX_CONSOLE_LOG_MESSAGE_CHARS: usize = 8_000;

    fn tab_title_for_path(path: &std::path::Path) -> String {
        path.file_name()
            .or_else(|| path.components().next_back().map(|c| c.as_os_str()))
            .map(|name| name.to_string_lossy().into_owned())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| path.display().to_string())
    }

    pub fn new() -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        let settings = AppSettings::load();
        let mut state = Self {
            toolchain: ToolchainManager::new(),
            workspace: WorkspaceManager::new(),
            console_log: Vec::new(),
            open_tabs: Vec::new(),
            active_tab: None,
            active_tab_right: None,
            status_message: "Ready".into(),
            busy: false,
            nav_index: crate::engine::navigation::NavIndex::default(),
            settings: settings.clone(),
            tool_update_checking: false,
            tool_update_installing: false,
            tool_updates_available: Vec::new(),
            tool_update_error: None,
            dark_mode: settings.dark_mode,
            search_results: Vec::new(),
            global_search_query: String::new(),
            recent_apks: Vec::new(),
            current_project_path: None,
            show_help: false,
            log_rx: Some(rx),
            log_tx: tx,
            sidebar_view: SideBarView::Explorer,
            show_command_palette: false,
            palette_mode: PaletteMode::Commands,
            xref_db: None,
            extracted_strings: Vec::new(),
            xref_results: Vec::new(),
            string_filter: None,
            string_search_query: String::new(),
            strings_revision: 0,
            strings_view_cache: None,
            app_studio_package_name: String::new(),
            app_studio_icon_path: String::new(),
            app_studio_symbol_input: String::new(),
            app_studio_rule_file: String::new(),
            app_studio_diff_dir: String::new(),
            app_studio_class_rename: String::new(),
            app_studio_note_input: String::new(),
            app_studio_report: String::new(),
            manifest_info: None,
            dex_stats: None,
            adb_shell_input: String::new(),
            adb_package_input: String::new(),
            adb_pull_remote: String::new(),
            adb_pull_local: String::new(),
            adb_push_local: String::new(),
            adb_push_remote: String::new(),
            frida_processes: Vec::new(),
            frida_attach_target: String::new(),
            frida_script: crate::runtime::frida_templates::get_templates()[0].code.to_string(),
            frida_spawn_mode: true,
            frida_selected_template: 0,
            frida_attached: false,
            frida_child_pid: None,
            frida_server_arch: "arm64".to_string(),
            apkid_results: None,
            is_flutter_app: false,
            flutter_lib_paths: Vec::new(),
            libapp_path: None,
            flutter_version: None,
            native_lib_paths: Vec::new(),
            terminal_output: std::collections::VecDeque::with_capacity(1000),
            terminal_input: String::new(),
            terminal_history: Vec::new(),
            terminal_history_index: 0,
        };
        state.push_log(LogLevel::Info, "RevEng-IDE initialized.");
        state
    }

    pub fn save_settings(&mut self) {
        self.settings.dark_mode = self.dark_mode;
        if let Err(e) = self.settings.save() {
            self.push_log(LogLevel::Warn, &format!("Settings save failed: {}", e));
        }
    }

    pub fn reset_workspace_state(&mut self) {
        self.open_tabs.clear();
        self.active_tab = None;
        self.active_tab_right = None;
        self.search_results.clear();
        self.nav_index = crate::engine::navigation::NavIndex::default();
        self.xref_db = None;
        self.extracted_strings.clear();
        self.xref_results.clear();
        self.string_filter = None;
        self.string_search_query.clear();
        self.strings_revision = 0;
        self.strings_view_cache = None;
        self.app_studio_package_name.clear();
        self.app_studio_icon_path.clear();
        self.app_studio_symbol_input.clear();
        self.app_studio_rule_file.clear();
        self.app_studio_diff_dir.clear();
        self.app_studio_class_rename.clear();
        self.app_studio_note_input.clear();
        self.app_studio_report.clear();
        self.manifest_info = None;
        self.dex_stats = None;
        self.adb_package_input.clear();
        self.apkid_results = None;
        self.is_flutter_app = false;
        self.flutter_lib_paths.clear();
        self.libapp_path = None;
        self.flutter_version = None;
        self.native_lib_paths.clear();
    }

    pub fn save_project_to_path(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let project = SavedProject {
            version: 1,
            apk_path: self.workspace.apk_path().map(|p| p.display().to_string()),
            workspace_root: self.workspace.root_dir().map(|p| p.display().to_string()),
            active_tab: self.active_tab,
            active_tab_right: self.active_tab_right,
            global_search_query: self.global_search_query.clone(),
            sidebar_view: self.sidebar_view.clone(),
            tabs: self
                .open_tabs
                .iter()
                .enumerate()
                .map(|(idx, tab)| SavedEditorTab {
                    path: tab.path.display().to_string(),
                    target_line: tab.target_line,
                    search_query: tab.search_query.clone(),
                    replace_query: tab.replace_query.clone(),
                    search_visible: tab.search_visible,
                    replace_visible: tab.replace_visible,
                    right_split: self.active_tab_right == Some(idx),
                })
                .collect(),
        };

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(&project)?)?;
        Ok(())
    }

    pub fn load_project_from_path(&mut self, path: &std::path::Path) -> anyhow::Result<()> {
        let text = std::fs::read_to_string(path)?;
        let project: SavedProject = serde_json::from_str(&text)?;

        self.reset_workspace_state();
        self.global_search_query = project.global_search_query;
        self.sidebar_view = project.sidebar_view;

        let apk_path = project.apk_path.map(std::path::PathBuf::from);
        let workspace_root = project.workspace_root.map(std::path::PathBuf::from);
        self.workspace.restore(apk_path.clone(), workspace_root.clone());

        for saved in project.tabs {
            let path = std::path::PathBuf::from(saved.path);
            if !path.exists() {
                continue;
            }
            self.open_file(path.clone());
            if let Some(tab_idx) = self.open_tabs.iter().position(|t| t.path == path) {
                if let Some(tab) = self.open_tabs.get_mut(tab_idx) {
                    tab.target_line = saved.target_line;
                    tab.search_query = saved.search_query;
                    tab.replace_query = saved.replace_query;
                    tab.search_visible = saved.search_visible;
                    tab.replace_visible = saved.replace_visible;
                }
                if saved.right_split {
                    self.active_tab_right = Some(tab_idx);
                }
            }
        }

        self.active_tab = project.active_tab.filter(|idx| *idx < self.open_tabs.len());
        self.active_tab_right = project.active_tab_right.filter(|idx| *idx < self.open_tabs.len());
        if self.active_tab.is_none() && !self.open_tabs.is_empty() {
            self.active_tab = Some(0);
        }
        Ok(())
    }

    pub fn push_log(&mut self, level: LogLevel, message: &str) {
        let entry = LogEntry {
            timestamp: chrono::Local::now().format("%H:%M:%S%.3f").to_string(),
            level,
            message: Self::truncate_log_message(message),
        };
        self.push_log_entry(entry);
    }

    fn push_log_entry(&mut self, mut entry: LogEntry) {
        entry.message = Self::truncate_log_message(&entry.message);
        self.console_log.push(entry);
        let overflow = self.console_log.len().saturating_sub(Self::MAX_CONSOLE_LOG_ENTRIES);
        if overflow > 0 {
            self.console_log.drain(0..overflow);
        }
    }

    fn truncate_log_message(message: &str) -> String {
        if message.chars().count() <= Self::MAX_CONSOLE_LOG_MESSAGE_CHARS {
            return message.to_string();
        }

        let mut out: String = message
            .chars()
            .take(Self::MAX_CONSOLE_LOG_MESSAGE_CHARS)
            .collect();
        out.push_str("... [truncated]");
        out
    }

    pub(crate) fn decoded_smali_dir_count(&self) -> usize {
        let Some(decoded_root) = self.workspace.decoded_dir() else {
            return 0;
        };

        std::fs::read_dir(decoded_root)
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry.path().is_dir()
                    && entry
                        .file_name()
                        .to_str()
                        .map(|name| name.starts_with("smali"))
                        .unwrap_or(false)
            })
            .count()
    }

    pub fn ensure_nav_index(&mut self) -> bool {
        if !self.nav_index.mappings.is_empty()
            || !self.nav_index.reverse_mappings.is_empty()
            || !self.nav_index.class_mappings.is_empty()
        {
            return true;
        }

        let smali_dir_count = self.decoded_smali_dir_count();
        if smali_dir_count == 0 {
            return false;
        }

        let Some(decoded_root) = self.workspace.decoded_dir() else {
            return false;
        };

        self.push_log(
            LogLevel::Info,
            &format!(
                "Nav index missing - rebuilding from {} decoded smali directories...",
                smali_dir_count
            ),
        );

        let index = crate::engine::navigation::NavIndexer::index_workspace(&decoded_root);
        let mapping_count = index.mappings.len();
        let reverse_count = index.reverse_mappings.len();
        let class_count = index.class_mappings.len();
        self.nav_index = index;

        if mapping_count > 0 || reverse_count > 0 || class_count > 0 {
            self.push_log(
                LogLevel::Info,
                &format!(
                    "Indexed {} Java-to-Smali mappings ({} class fallbacks).",
                    mapping_count, class_count
                ),
            );
            true
        } else {
            self.push_log(
                LogLevel::Warn,
                "Decoded smali was found, but no navigation mappings could be built.",
            );
            false
        }
    }

    /// Tries to jump from current Java line to corresponding Smali code.
    pub fn jump_to_smali(&mut self, java_path: &std::path::Path, line_num: usize) {
        self.ensure_nav_index();

        let decompiled_root = match self.workspace.decompiled_dir() {
            Some(d) => d,
            None => return,
        };

        if let Ok(rel) = java_path.strip_prefix(&decompiled_root) {
            // jadx output is typically inside `sources/`.
            let mut rel_path = rel.to_path_buf();
            if let Ok(stripped) = rel.strip_prefix("sources") {
                rel_path = stripped.to_path_buf();
            }
            
            let base_class_name = rel_path.with_extension("")
                .to_string_lossy()
                .replace("\\", ".")
                .replace("/", ".");

            
            let mut target_path = None;
            for (ctx, loc) in &self.nav_index.mappings {
                // To support inner classes inside the same Java file, we match the prefix.
                // e.g. base_class_name = "com.app.MainActivity", ctx.class_name = "com.app.MainActivity$Inner"
                if (ctx.class_name == base_class_name || ctx.class_name.starts_with(&format!("{}$", base_class_name))) 
                    && ctx.line_number == line_num
                {
                    target_path = Some((loc.path.clone(), loc.line_number));
                    break;
                }
            }

            // Fallback: nearest line in matching class.
            if target_path.is_none() {
                let mut best: Option<(std::path::PathBuf, usize)> = None;
                let mut best_dist = usize::MAX;
                for (ctx, loc) in &self.nav_index.mappings {
                    if ctx.line_number == 0 {
                        continue;
                    }
                    if ctx.class_name == base_class_name
                        || ctx.class_name.starts_with(&format!("{}$", base_class_name))
                    {
                        let dist = ctx.line_number.abs_diff(line_num);
                        if dist < best_dist {
                            best_dist = dist;
                            best = Some((loc.path.clone(), loc.line_number));
                        }
                    }
                }
                target_path = best;
            }

            // Fallback: class-level mapping for stripped debug info.
            if target_path.is_none() {
                if let Some(loc) = self
                    .nav_index
                    .class_mappings
                    .get(&base_class_name)
                    .or_else(|| {
                        self.nav_index
                            .class_mappings
                            .iter()
                            .find(|(class_name, _)| class_name.starts_with(&format!("{}$", base_class_name)))
                            .map(|(_, loc)| loc)
                    })
                {
                    target_path = Some((loc.path.clone(), loc.line_number));
                    self.push_log(
                        LogLevel::Info,
                        "Line-debug mapping unavailable; used class-level Smali fallback.",
                    );
                }
            }

            if let Some((path, ln)) = target_path {
                if let Some(tab_idx) = self.open_tabs.iter().position(|t| t.path == *path) {
                    if let Some(tab) = self.open_tabs.get_mut(tab_idx) {
                        tab.target_line = Some(ln);
                    }
                    self.active_tab_right = Some(tab_idx);
                } else {
                    self.open_file_right(path.clone(), Some(ln));
                }
                
                self.push_log(LogLevel::Info, &format!("Jumped to Smali: {}:{}", path.display(), ln));
                self.status_message = format!("Smali {}:{}", path.file_name().unwrap_or_default().to_string_lossy(), ln);
                return;
            }
        }
        if self.nav_index.mappings.is_empty() && self.nav_index.class_mappings.is_empty() {
            if self.decoded_smali_dir_count() > 0 {
                self.push_log(
                    LogLevel::Warn,
                    "Nav index is still empty even though decoded smali exists.",
                );
                self.status_message = "Jump failed: navigation index could not be rebuilt".into();
            } else {
                self.push_log(LogLevel::Warn, "Nav index empty - decode APK with apktool first.");
                self.status_message = "Jump failed: nav index not built".into();
            }
        } else {
            self.push_log(LogLevel::Warn, "No Smali mapping found for this line.");
            self.status_message = "No Smali mapping for this line".into();
        }
    }

    /// Reverse navigation: jump from Smali to Java source.
    pub fn jump_to_java(&mut self, smali_path: &std::path::Path, line_num: usize) {
        self.ensure_nav_index();

        let smali_key = smali_path.to_string_lossy().to_string();

        // Find the closest .line directive at or before the current line
        let mut best: Option<(String, usize)> = None;
        let mut best_dist = usize::MAX;
        for ((path_str, smali_line), (class_name, java_line)) in &self.nav_index.reverse_mappings {
            if *path_str == smali_key && *smali_line <= line_num && (line_num - smali_line) < best_dist {
                best_dist = line_num - smali_line;
                best = Some((class_name.clone(), *java_line));
            }
        }

        if let Some((class_name, java_line)) = best {
            // Convert class name to Java source path
            let decompiled_root = match self.workspace.decompiled_dir() {
                Some(d) => d,
                None => {
                    self.push_log(LogLevel::Warn, "No decompiled directory found.");
                    return;
                }
            };

            // Try: decompiled/sources/com/example/MyClass.java
            let relative = class_name.replace('.', "/");
            // Strip inner class part for file lookup
            let base_class = relative.split('$').next().unwrap_or(&relative);
            let java_path = decompiled_root.join("sources").join(format!("{}.java", base_class));

            if java_path.exists() {
                if let Some(tab_idx) = self.open_tabs.iter().position(|t| t.path == java_path) {
                    if let Some(tab) = self.open_tabs.get_mut(tab_idx) {
                        tab.target_line = Some(java_line);
                    }
                    self.active_tab = Some(tab_idx);
                } else {
                    self.open_file(java_path.clone());
                    if let Some(tab) = self.open_tabs.last_mut() {
                        tab.target_line = Some(java_line);
                    }
                }
                self.push_log(LogLevel::Info, &format!("Jumped to Java: {}:{}", java_path.display(), java_line));
                self.status_message = format!("Java {}:{}", java_path.file_name().unwrap_or_default().to_string_lossy(), java_line);
            } else {
                self.push_log(LogLevel::Warn, &format!("Java source not found: {}", java_path.display()));
                self.status_message = "Java source file not found".into();
            }
        } else if self.nav_index.reverse_mappings.is_empty() {
            if self.decoded_smali_dir_count() > 0 {
                // Fallback: infer class from `.class` line and jump to corresponding Java file.
                let inferred_class = std::fs::read_to_string(smali_path)
                    .ok()
                    .and_then(|content| {
                        let re = regex::Regex::new(r"(?m)^\s*\.class\s+.*L([^;]+);").ok()?;
                        let cap = re.captures(&content)?;
                        Some(cap[1].replace('/', "."))
                    });

                if let Some(class_name) = inferred_class {
                    let decompiled_root = match self.workspace.decompiled_dir() {
                        Some(d) => d,
                        None => {
                            self.push_log(LogLevel::Warn, "No decompiled directory found.");
                            return;
                        }
                    };

                    let relative = class_name.replace('.', "/");
                    let base_class = relative.split('$').next().unwrap_or(&relative);
                    let java_path = decompiled_root.join("sources").join(format!("{}.java", base_class));

                    if java_path.exists() {
                        if let Some(tab_idx) = self.open_tabs.iter().position(|t| t.path == java_path) {
                            if let Some(tab) = self.open_tabs.get_mut(tab_idx) {
                                tab.target_line = Some(1);
                            }
                            self.active_tab = Some(tab_idx);
                        } else {
                            self.open_file(java_path.clone());
                            if let Some(tab) = self.open_tabs.last_mut() {
                                tab.target_line = Some(1);
                            }
                        }
                        self.push_log(
                            LogLevel::Info,
                            "Reverse line mapping unavailable; used class-level Java fallback.",
                        );
                        self.status_message = format!(
                            "Java {}:{}",
                            java_path.file_name().unwrap_or_default().to_string_lossy(),
                            1
                        );
                        return;
                    }
                }

                self.push_log(
                    LogLevel::Warn,
                    "Reverse nav index is still empty even though decoded smali exists.",
                );
                self.status_message = "Jump failed: reverse navigation index could not be rebuilt".into();
            } else {
                self.push_log(LogLevel::Warn, "Nav index empty - decode APK with apktool first.");
                self.status_message = "Jump failed: nav index not built".into();
            }
        } else {
            self.push_log(LogLevel::Warn, "No Java mapping found for this Smali line.");
            self.status_message = "No Java mapping for this Smali line".into();
        }
    }

    pub fn current_manifest_package(&self) -> Option<String> {
        let decoded_root = self.workspace.decoded_dir()?;
        crate::engine::refactor::RefactoringEngine::manifest_package(&decoded_root)
    }

    pub fn app_studio_set_report(&mut self, title: &str, lines: &[String]) {
        let mut report = String::new();
        report.push_str(title);
        report.push('\n');
        report.push_str("=".repeat(title.len()).as_str());
        report.push('\n');

        if lines.is_empty() {
            report.push_str("No findings.\n");
        } else {
            for line in lines.iter().take(400) {
                report.push_str("- ");
                report.push_str(line);
                report.push('\n');
            }
        }

        self.app_studio_report = report;
    }

    pub fn app_studio_generate_frida_template(&mut self) {
        let symbol = self.app_studio_symbol_input.trim().to_string();
        let script = crate::engine::arsenal::Arsenal::generate_frida_template(&symbol);
        self.frida_script = script.clone();
        self.app_studio_set_report(
            "Frida Script Generator",
            &vec![
                format!("Template generated for: {}", if symbol.is_empty() { "default target" } else { &symbol }),
                "Script copied into Runtime Frida editor buffer.".to_string(),
            ],
        );
        self.push_log(LogLevel::Info, "[AppStudio] Frida template generated and loaded.");
    }

    pub fn drain_logs(&mut self) {
        if let Some(rx) = self.log_rx.take() {
            while let Ok(entry) = rx.try_recv() {
                self.push_log_entry(entry);
            }
            self.log_rx = Some(rx);
        }
    }

    pub fn open_file(&mut self, path: std::path::PathBuf) {
        for (i, tab) in self.open_tabs.iter().enumerate() {
            if tab.path == path {
                self.active_tab = Some(i);
                return;
            }
        }

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        if ext == "so" {
            if self.reject_large_file(&path, Self::MAX_NATIVE_VIEW_BYTES, "native viewer") {
                return;
            }
            match self.open_native_library(&path) {
                Ok((info, insns)) => {
                    self.open_tabs.push(EditorTab {
                        title: Self::tab_title_for_path(&path),
                        path: path.clone(),
                        content: TabContent::Native(info, insns),
                        language: FileLanguage::Unknown,
                        modified: false,
                        cursor_pos: 0,
                        search_query: String::new(),
                        replace_query: String::new(),
                        search_visible: false,
                        replace_visible: false,
                        target_line: None,
                        bracket_match: None,
                        scroll_offset_y: 0.0,
                        autocomplete_visible: false,
                        autocomplete_query: String::new(),
                        autocomplete_suggestions: Vec::new(),
                        autocomplete_sel: 0,
                    });
                    self.active_tab = Some(self.open_tabs.len() - 1);
                    self.push_log(LogLevel::Info, &format!("Opened native library: {}", path.display()));
                    return;
                }
                Err(e) => {
                    self.push_log(LogLevel::Error, &format!("Failed to parse native library: {}", e));
                }
            }
        }

        // Open binary formats that benefit from a hex view
        let hex_exts = ["dex", "odex", "oat", "arsc", "vdex"];
        if hex_exts.contains(&ext.as_str()) {
            match std::fs::metadata(&path) {
                Ok(meta) if meta.len() > Self::MAX_HEX_VIEW_BYTES => {
                    self.push_log(
                        LogLevel::Warn,
                        &format!(
                            "Refusing to open huge binary in hex view: {} ({:.1} MB > {:.1} MB)",
                            path.display(),
                            meta.len() as f64 / 1_048_576.0,
                            Self::MAX_HEX_VIEW_BYTES as f64 / 1_048_576.0
                        ),
                    );
                    self.status_message = "Binary too large for in-memory hex view".into();
                    return;
                }
                Ok(_) => {}
                Err(e) => {
                    self.push_log(LogLevel::Error, &format!("Failed to stat {}: {}", path.display(), e));
                    return;
                }
            }
            match std::fs::read(&path) {
                Ok(bytes) => {
                    let byte_count = bytes.len();
                    self.open_tabs.push(EditorTab {
                        title: Self::tab_title_for_path(&path),
                        path: path.clone(),
                        content: TabContent::Hex(bytes),
                        language: FileLanguage::Unknown,
                        modified: false,
                        cursor_pos: 0,
                        search_query: String::new(),
                        replace_query: String::new(),
                        search_visible: false,
                        replace_visible: false,
                        target_line: None,
                        bracket_match: None,
                        scroll_offset_y: 0.0,
                        autocomplete_visible: false,
                        autocomplete_query: String::new(),
                        autocomplete_suggestions: Vec::new(),
                        autocomplete_sel: 0,
                    });
                    self.active_tab = Some(self.open_tabs.len() - 1);
                    self.push_log(LogLevel::Info, &format!("Opened in hex view: {} ({} bytes)", path.display(), byte_count));
                    return;
                }
                Err(e) => {
                    self.push_log(LogLevel::Error, &format!("Failed to read {}: {}", path.display(), e));
                    return;
                }
            }
        }

        let binary_exts = ["apk", "xapk", "png", "jpg", "jpeg", "gif", "ttf", "otf", "mp3", "ogg", "wav", "zip"];
        if binary_exts.contains(&ext.as_str()) {
            self.push_log(LogLevel::Warn, &format!("Cannot open binary file in text editor: {}", path.display()));
            return;
        }

        if self.reject_large_file(&path, Self::MAX_TEXT_EDITOR_BYTES, "text editor") {
            return;
        }
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                self.push_log(LogLevel::Error, &format!("Failed to read {}: {} (might be a binary file)", path.display(), e));
                return;
            }
        };

        let language = FileLanguage::from_extension(&ext);
        self.open_tabs.push(EditorTab {
            title: Self::tab_title_for_path(&path),
            path: path.clone(),
            content: TabContent::Code(content),
            language,
            modified: false,
            cursor_pos: 0,
            search_query: String::new(),
            replace_query: String::new(),
            search_visible: false,
            replace_visible: false,
            target_line: None,
            bracket_match: None,
            scroll_offset_y: 0.0,
            autocomplete_visible: false,
            autocomplete_query: String::new(),
            autocomplete_suggestions: Vec::new(),
            autocomplete_sel: 0,
        });
        self.active_tab = Some(self.open_tabs.len() - 1);
        self.push_log(LogLevel::Info, &format!("Opened: {}", path.display()));
    }

    pub fn open_file_at_line(&mut self, path: std::path::PathBuf, target_line: usize, search_term: Option<String>) {
        if let Some(tab_idx) = self.open_tabs.iter().position(|tab| tab.path == path) {
            if let Some(tab) = self.open_tabs.get_mut(tab_idx) {
                tab.target_line = Some(target_line);
                if let Some(q) = &search_term {
                    tab.search_query = q.clone();
                    tab.search_visible = true;
                }
            }
            self.active_tab = Some(tab_idx);
            return;
        }

        self.open_file(path.clone());

        if let Some(tab_idx) = self.open_tabs.iter().position(|tab| tab.path == path) {
            if let Some(tab) = self.open_tabs.get_mut(tab_idx) {
                tab.target_line = Some(target_line);
                if let Some(q) = &search_term {
                    tab.search_query = q.clone();
                    tab.search_visible = true;
                }
            }
            self.active_tab = Some(tab_idx);
        }
    }

    pub fn open_file_right(&mut self, path: std::path::PathBuf, target_line: Option<usize>) {
        for (i, tab) in self.open_tabs.iter().enumerate() {
            if tab.path == path {
                self.active_tab_right = Some(i);
                if let Some(tab_mut) = self.open_tabs.get_mut(i) {
                    tab_mut.target_line = target_line;
                }
                return;
            }
        }

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        let rich_view_exts = ["so", "dex", "odex", "oat", "arsc", "vdex"];
        if rich_view_exts.contains(&ext.as_str()) {
            let previous_active = self.active_tab;
            self.open_file(path.clone());
            if let Some(tab_idx) = self.open_tabs.iter().position(|tab| tab.path == path) {
                self.active_tab_right = Some(tab_idx);
                self.active_tab = previous_active.or(Some(tab_idx));
            }
            return;
        }

        let binary_exts = ["apk", "xapk", "png", "jpg", "jpeg", "gif", "ttf", "otf", "mp3", "ogg", "wav", "zip"];
        if binary_exts.contains(&ext.as_str()) {
            self.push_log(LogLevel::Warn, &format!("Cannot open binary file in side editor: {}", path.display()));
            return;
        }

        if self.reject_large_file(&path, Self::MAX_TEXT_EDITOR_BYTES, "side text editor") {
            return;
        }
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                self.push_log(LogLevel::Error, &format!("Failed to read {}: {}", path.display(), e));
                return;
            }
        };

        let language = FileLanguage::from_extension(&ext);
        self.open_tabs.push(EditorTab {
            title: Self::tab_title_for_path(&path),
            path: path.clone(),
            content: TabContent::Code(content),
            language,
            modified: false,
            cursor_pos: 0,
            search_query: String::new(),
            replace_query: String::new(),
            search_visible: false,
            replace_visible: false,
            target_line,
            bracket_match: None,
            scroll_offset_y: 0.0,
            autocomplete_visible: false,
            autocomplete_query: String::new(),
            autocomplete_suggestions: Vec::new(),
            autocomplete_sel: 0,
        });
        self.active_tab_right = Some(self.open_tabs.len() - 1);
        self.push_log(LogLevel::Info, &format!("Opened in split view: {}", path.display()));
    }

    fn open_native_library(&self, path: &std::path::Path) -> anyhow::Result<(crate::native::elf::ElfInfo, Vec<crate::native::disasm::DisasmInstruction>)> {
        use crate::native::elf::ElfParser;
        use crate::native::disasm::Disassembler;

        let mut info = ElfParser::parse(path)?;
        if !info.is_dart_snapshot {
            info.is_dart_snapshot = crate::native::flutter_patch::FlutterPatcher::is_dart_snapshot(path);
        }
        let data = std::fs::read(path)?;
        let arch = Disassembler::detect_arch(&info.machine);
        let mut all_insns = Vec::new();

        // Priority: .text → Dart isolate snapshot instructions → .rodata
        // libapp.so (Dart AOT) has no .text — code lives in .rodata or named Dart sections
        let section_priority: &[&str] = &[".text", ".rodata"];
        'sec: for &target in section_priority {
            for sec in &info.sections {
                if sec.name == target && sec.size > 0 {
                    if let Ok(insns) = Disassembler::disassemble_section(&data, sec.offset, sec.size, sec.addr, arch) {
                        if !insns.is_empty() {
                            all_insns = insns;
                            break 'sec;
                        }
                    }
                }
            }
        }

        // Fallback: for libapp.so — disassemble from _kDartIsolateSnapshotInstructions export symbol
        if all_insns.is_empty() {
            let dart_sym = info.exports.iter()
                .find(|e| e.name.contains("kDartIsolateSnapshotInstructions"));
            if let Some(sym) = dart_sym {
                // Locate which section contains this symbol's address
                for sec in &info.sections {
                    if sec.addr > 0 && sym.addr >= sec.addr && sym.addr < sec.addr + sec.size {
                        let rel_offset = sym.addr - sec.addr;
                        let file_off = sec.offset + rel_offset;
                        let max_bytes = (sec.size - rel_offset).min(512 * 1024);
                        if let Ok(insns) = Disassembler::disassemble_section(&data, file_off, max_bytes, sym.addr, arch) {
                            all_insns = insns;
                        }
                        break;
                    }
                }
            }
        }

        Ok((info, all_insns))
    }

    fn reject_large_file(&mut self, path: &std::path::Path, limit: u64, viewer: &str) -> bool {
        match std::fs::metadata(path) {
            Ok(meta) if meta.len() > limit => {
                self.push_log(
                    LogLevel::Warn,
                    &format!(
                        "Refusing to open huge file in {}: {} ({:.1} MB > {:.1} MB)",
                        viewer,
                        path.display(),
                        meta.len() as f64 / 1_048_576.0,
                        limit as f64 / 1_048_576.0
                    ),
                );
                self.status_message = format!("File too large for {}", viewer);
                true
            }
            Ok(_) => false,
            Err(e) => {
                self.push_log(LogLevel::Error, &format!("Failed to stat {}: {}", path.display(), e));
                true
            }
        }
    }

    pub fn save_active_tab(&mut self) -> anyhow::Result<()> {
        let mut msg = None;
        if let Some(idx) = self.active_tab {
            if let Some(tab) = self.open_tabs.get_mut(idx) {
                if let TabContent::Code(content) = &tab.content {
                    std::fs::write(&tab.path, content)?;
                    tab.modified = false;
                    msg = Some(format!("Saved: {}", tab.path.display()));
                }
            }
        }
        if let Some(m) = msg {
            self.push_log(LogLevel::Info, &m);
        }
        Ok(())
    }

    pub fn close_tab(&mut self, index: usize) {
        if index < self.open_tabs.len() {
            self.open_tabs.remove(index);
            if self.open_tabs.is_empty() {
                self.active_tab = None;
                self.active_tab_right = None;
            } else {
                if let Some(active) = self.active_tab {
                    if active == index {
                        self.active_tab = Some(self.open_tabs.len().saturating_sub(1));
                    } else if active > index {
                        self.active_tab = Some(active - 1);
                    }
                }
                if let Some(active_r) = self.active_tab_right {
                    if active_r == index {
                        self.active_tab_right = None;
                    } else if active_r > index {
                        self.active_tab_right = Some(active_r - 1);
                    }
                }
            }
        }
    }

    pub fn push_terminal_line(&mut self, text: impl Into<String>, kind: TermOutputKind) {
        self.terminal_output.push_back(TermOutputLine {
            text: text.into(),
            kind,
        });
        while self.terminal_output.len() > 1000 {
            self.terminal_output.pop_front();
        }
    }

    /// Run a terminal command in the background and capture output in the integrated terminal.
    pub fn run_terminal_command_async(state: Arc<Mutex<AppState>>, cmd: String, cwd: Option<std::path::PathBuf>) {
        {
            let mut s = state.lock().unwrap();
            s.push_terminal_line(format!("$ {}", cmd), TermOutputKind::System);
        }

        std::thread::spawn(move || {
            match Self::run_shell_command_with_timeout(&cmd, std::time::Duration::from_secs(120), cwd.as_deref()) {
                Ok(out) => {
                    let stdout = String::from_utf8_lossy(&out.stdout);
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    let mut s = state.lock().unwrap();
                    for line in stdout.lines() {
                        s.push_terminal_line(line, TermOutputKind::Stdout);
                    }
                    for line in stderr.lines() {
                        s.push_terminal_line(line, TermOutputKind::Stderr);
                    }
                    if !out.status.success() {
                        s.push_terminal_line(
                            format!("exit status: {}", out.status),
                            TermOutputKind::System,
                        );
                    }
                }
                Err(e) => {
                    state
                        .lock()
                        .unwrap()
                        .push_terminal_line(format!("error: {}", e), TermOutputKind::Stderr);
                }
            }
        });
    }

    fn run_shell_command_with_timeout(
        cmd: &str,
        timeout: std::time::Duration,
        cwd: Option<&std::path::Path>,
    ) -> anyhow::Result<std::process::Output> {
        use std::process::{Command, Stdio};
        use std::time::Instant;

        let mut command = if cfg!(target_os = "windows") {
            let mut command = Command::new("cmd");
            command.arg("/C").arg(cmd);
            command
        } else {
            let mut command = Command::new("sh");
            command.arg("-c").arg(cmd);
            command
        };
        if let Some(dir) = cwd {
            command.current_dir(dir);
        }

        let mut child = command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let started = Instant::now();

        loop {
            if child.try_wait()?.is_some() {
                return child.wait_with_output().map_err(Into::into);
            }

            if started.elapsed() >= timeout {
                let _ = child.kill();
                let _ = child.wait();
                anyhow::bail!("command timed out after {}s", timeout.as_secs());
            }

            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
}

pub struct RevEngApp {
    pub state: Arc<Mutex<AppState>>,
    layout: IdeLayout,
    tokio_rt: tokio::runtime::Runtime,
}

impl RevEngApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> anyhow::Result<Self> {
        // ── Load premium system fonts ──────────────────────────────────────
        {
            let mut fonts = egui::FontDefinitions::default();

            // Code font: Cascadia Mono (ships with Windows Terminal, Win10+) → Consolas fallback
            let mono_candidates = [
                "C:\\Windows\\Fonts\\CascadiaMono.ttf",
                "C:\\Windows\\Fonts\\CascadiaCode.ttf",
                "C:\\Windows\\Fonts\\consola.ttf",
            ];
            for path in &mono_candidates {
                if let Ok(bytes) = std::fs::read(path) {
                    fonts.font_data.insert(
                        "reveng_mono".to_owned(),
                        Arc::new(egui::FontData::from_owned(bytes)),
                    );
                    if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
                        family.insert(0, "reveng_mono".to_owned());
                    }
                    log::info!("Loaded code font: {}", path);
                    break;
                }
            }

            // UI font: Segoe UI (Vista+, always present on Windows)
            let ui_candidates = [
                "C:\\Windows\\Fonts\\segoeui.ttf",
                "C:\\Windows\\Fonts\\calibril.ttf",
            ];
            for path in &ui_candidates {
                if let Ok(bytes) = std::fs::read(path) {
                    fonts.font_data.insert(
                        "reveng_ui".to_owned(),
                        Arc::new(egui::FontData::from_owned(bytes)),
                    );
                    if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
                        family.insert(0, "reveng_ui".to_owned());
                    }
                    log::info!("Loaded UI font: {}", path);
                    break;
                }
            }

            cc.egui_ctx.set_fonts(fonts);
        }

        let state = Arc::new(Mutex::new(AppState::new()));
        let layout = IdeLayout::new();
        let tokio_rt = tokio::runtime::Runtime::new()
            .map_err(|e| anyhow::anyhow!("Failed to create async runtime: {}", e))?;

        {
            let mut s = state.lock().unwrap();
            let results = s.toolchain.verify_all();
            for (tool, ok) in &results {
                if *ok {
                    s.push_log(LogLevel::Info, &format!("Tool ready: {}", tool));
                    if let Some(desc) = s.toolchain.describe_tool(tool) {
                        s.push_log(LogLevel::Debug, &desc);
                    }
                } else {
                    let required = s
                        .toolchain
                        .get(tool)
                        .map(|info| info.required == crate::engine::toolchain::ToolRequirement::Required)
                        .unwrap_or(true);
                    if required {
                        s.push_log(LogLevel::Warn, &format!("Tool missing: {}", tool));
                    } else {
                        s.push_log(LogLevel::Info, &format!("Optional tool missing: {}", tool));
                    }
                }
            }
        }

        Self::start_tool_update_check(&state);

        Ok(Self { state, layout, tokio_rt })
    }

    pub fn start_tool_update_check(state: &Arc<Mutex<AppState>>) {
        {
            let mut s = state.lock().unwrap();
            if !s.settings.check_tool_updates_on_startup || s.tool_update_checking || s.tool_update_installing {
                return;
            }
            s.tool_update_checking = true;
            s.tool_update_error = None;
            s.push_log(LogLevel::Info, "Checking toolbase updates...");
        }

        let state_c = Arc::clone(state);
        std::thread::spawn(move || {
            let toolchain = { state_c.lock().unwrap().toolchain.clone() };
            let result = toolchain.check_for_updates();

            let mut s = state_c.lock().unwrap();
            s.tool_update_checking = false;
            match result {
                Ok(updates) if updates.is_empty() => {
                    s.push_log(LogLevel::Info, "Toolbase is up to date.");
                }
                Ok(updates) => {
                    let count = updates.len();
                    s.tool_updates_available = updates;
                    s.status_message = format!("{} tool update{} available", count, if count == 1 { "" } else { "s" });
                    s.push_log(
                        LogLevel::Warn,
                        &format!("{} tool update{} available. Choose Update Toolbase or Skip.", count, if count == 1 { "" } else { "s" }),
                    );
                }
                Err(e) => {
                    s.tool_update_error = Some(e.to_string());
                    s.push_log(LogLevel::Warn, &format!("Tool update check failed: {}", e));
                }
            }
        });
    }

    pub fn start_tool_update_install(state: &Arc<Mutex<AppState>>) {
        let updates = {
            let mut s = state.lock().unwrap();
            if s.tool_update_installing || s.tool_updates_available.is_empty() {
                return;
            }
            s.tool_update_installing = true;
            s.busy = true;
            s.tool_update_error = None;
            s.status_message = "Updating toolbase...".into();
            s.push_log(LogLevel::Info, "Updating toolbase...");
            s.tool_updates_available.clone()
        };

        let state_c = Arc::clone(state);
        std::thread::spawn(move || {
            let mut toolchain = { state_c.lock().unwrap().toolchain.clone() };
            let result = toolchain.install_updates(&updates);

            let mut s = state_c.lock().unwrap();
            s.tool_update_installing = false;
            s.busy = false;
            match result {
                Ok(installed) => {
                    s.toolchain = toolchain;
                    s.tool_updates_available.clear();
                    s.status_message = "Toolbase updated".into();
                    for item in installed {
                        s.push_log(LogLevel::Info, &format!("Updated {}", item));
                    }
                    let results = s.toolchain.verify_all();
                    for (tool, ok) in results {
                        let level = if ok { LogLevel::Info } else { LogLevel::Warn };
                        let label = if ok { "Tool ready" } else { "Tool missing" };
                        s.push_log(level, &format!("{}: {}", label, tool));
                    }
                }
                Err(e) => {
                    s.tool_update_error = Some(e.to_string());
                    s.status_message = "Toolbase update failed".into();
                    s.push_log(LogLevel::Error, &format!("Toolbase update failed: {}", e));
                }
            }
        });
    }
}

impl eframe::App for RevEngApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        {
            let mut s = self.state.lock().unwrap();
            s.drain_logs();
        }
        self.layout.render(ctx, &self.state, &self.tokio_rt);
    }
}

#[cfg(test)]
mod tests {
    use super::{AppState, LogEntry, LogLevel};
    use std::io::{Seek, Write};
    use std::time::Duration;

    #[test]
    fn terminal_shell_runner_captures_stdout() {
        let output =
            AppState::run_shell_command_with_timeout("printf terminal_ok", Duration::from_secs(2), None)
                .expect("shell command should run");
        assert!(output.status.success());
        assert_eq!(String::from_utf8_lossy(&output.stdout), "terminal_ok");
    }

    #[test]
    fn large_files_are_rejected_before_editor_load() {
        let path = std::env::temp_dir().join(format!(
            "reveng_large_editor_guard_{}_{}.smali",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut file = std::fs::File::create(&path).unwrap();
        file.seek(std::io::SeekFrom::Start(AppState::MAX_TEXT_EDITOR_BYTES + 1))
            .unwrap();
        file.write_all(b"\0").unwrap();
        drop(file);

        let mut state = AppState::new();
        state.open_file(path.clone());

        assert!(state.open_tabs.is_empty());
        assert_eq!(state.status_message, "File too large for text editor");

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn console_log_is_bounded_for_direct_and_background_entries() {
        let mut state = AppState::new();
        for i in 0..(AppState::MAX_CONSOLE_LOG_ENTRIES + 10) {
            state.push_log(LogLevel::Debug, &format!("direct-{i}"));
        }

        assert_eq!(state.console_log.len(), AppState::MAX_CONSOLE_LOG_ENTRIES);
        assert_eq!(state.console_log.first().unwrap().message, "direct-10");

        for i in 0..10 {
            state
                .log_tx
                .send(LogEntry {
                    timestamp: "00:00:00.000".to_string(),
                    level: LogLevel::Info,
                    message: format!("background-{i}"),
                })
                .unwrap();
        }
        state.drain_logs();

        assert_eq!(state.console_log.len(), AppState::MAX_CONSOLE_LOG_ENTRIES);
        assert_eq!(state.console_log.last().unwrap().message, "background-9");
    }

    #[test]
    fn console_log_messages_are_truncated_on_character_boundaries() {
        let mut state = AppState::new();
        let long = format!(
            "{}{}",
            "α".repeat(AppState::MAX_CONSOLE_LOG_MESSAGE_CHARS),
            "中tail"
        );

        state.push_log(LogLevel::Info, &long);

        let stored = &state.console_log.last().unwrap().message;
        assert!(stored.ends_with("... [truncated]"));
        assert!(stored.contains('α'));
        assert!(!stored.contains("中tail"));

        state
            .log_tx
            .send(LogEntry {
                timestamp: "00:00:00.000".to_string(),
                level: LogLevel::Info,
                message: long,
            })
            .unwrap();
        state.drain_logs();

        let drained = &state.console_log.last().unwrap().message;
        assert!(drained.ends_with("... [truncated]"));
        assert!(!drained.contains("中tail"));
    }
}
