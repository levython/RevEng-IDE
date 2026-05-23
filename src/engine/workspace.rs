//! Workspace manager — handles workspace directory structure for APK analysis.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// Manages the workspace directory layout for an opened APK.
pub struct WorkspaceManager {
    /// Path to the original APK file.
    apk_path: Option<PathBuf>,
    /// Root workspace directory for this APK.
    root: Option<PathBuf>,
}

impl WorkspaceManager {
    pub fn new() -> Self {
        Self {
            apk_path: None,
            root: None,
        }
    }

    pub fn restore(&mut self, apk_path: Option<PathBuf>, root: Option<PathBuf>) {
        self.apk_path = apk_path;
        self.root = root;
    }

    /// Initialize a new workspace for the given APK.
    pub fn init(&mut self, apk_path: &Path) -> anyhow::Result<PathBuf> {
        let apk_file_name = apk_path
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("APK path has no file name: {}", apk_path.display()))?;
        let apk_stem = apk_path.file_stem().and_then(|s| s.to_str()).unwrap_or("workspace");
        let workspace_name = Self::workspace_name(apk_path, apk_stem);

        // Create workspace under ./workspace/<apk_name>/
        let ws_base = PathBuf::from("workspace");
        let ws_root = ws_base.join(workspace_name);

        // Create directory structure
        std::fs::create_dir_all(ws_root.join("decoded"))?;
        std::fs::create_dir_all(ws_root.join("decompiled"))?;
        std::fs::create_dir_all(ws_root.join("native"))?;
        std::fs::create_dir_all(ws_root.join("build"))?;

        // Copy original APK into workspace
        let apk_dest = ws_root.join(apk_file_name);
        if !apk_dest.exists() {
            std::fs::copy(apk_path, &apk_dest)?;
        }

        self.apk_path = Some(apk_dest);
        self.root = Some(ws_root.clone());

        Ok(ws_root)
    }

    fn workspace_name(apk_path: &Path, apk_stem: &str) -> String {
        let readable = Self::sanitize_workspace_component(apk_stem);
        let mut hasher = DefaultHasher::new();
        apk_path.to_string_lossy().hash(&mut hasher);
        let suffix = hasher.finish();
        format!("{}_{:016x}", readable, suffix)
    }

    fn sanitize_workspace_component(value: &str) -> String {
        let mut out = String::with_capacity(value.len().min(64));
        let mut last_was_sep = false;
        for ch in value.chars() {
            let normalized = if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            };

            if normalized == '_' {
                if last_was_sep {
                    continue;
                }
                last_was_sep = true;
            } else {
                last_was_sep = false;
            }

            out.push(normalized);
            if out.len() >= 64 {
                break;
            }
        }

        let trimmed = out.trim_matches('_');
        if trimmed.is_empty() {
            "workspace".to_string()
        } else {
            trimmed.to_string()
        }
    }

    pub fn has_apk(&self) -> bool {
        self.apk_path.is_some()
    }

    pub fn apk_path(&self) -> Option<&Path> {
        self.apk_path.as_deref()
    }

    pub fn root_dir(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    pub fn decoded_dir(&self) -> Option<PathBuf> {
        self.root.as_ref().map(|r| r.join("decoded"))
    }

    pub fn decompiled_dir(&self) -> Option<PathBuf> {
        self.root.as_ref().map(|r| r.join("decompiled"))
    }

    pub fn native_dir(&self) -> Option<PathBuf> {
        self.root.as_ref().map(|r| r.join("native"))
    }

    pub fn build_dir(&self) -> Option<PathBuf> {
        self.root.as_ref().map(|r| r.join("build"))
    }
}

#[cfg(test)]
mod tests {
    use super::WorkspaceManager;
    use std::path::Path;

    #[test]
    fn workspace_component_is_sanitized() {
        assert_eq!(
            WorkspaceManager::sanitize_workspace_component("my app../β/debug"),
            "my_app_debug"
        );
        assert_eq!(WorkspaceManager::sanitize_workspace_component("///"), "workspace");
    }

    #[test]
    fn workspace_name_includes_path_specific_suffix() {
        let a = WorkspaceManager::workspace_name(Path::new("/tmp/a/base.apk"), "base");
        let b = WorkspaceManager::workspace_name(Path::new("/tmp/b/base.apk"), "base");

        assert_ne!(a, b);
        assert!(a.starts_with("base_"));
        assert!(b.starts_with("base_"));
    }
}
