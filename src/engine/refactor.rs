//! Refactoring engine — project-wide renaming for Smali and Java.

use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use anyhow::Result;

pub struct RefactoringEngine;

impl RefactoringEngine {
    pub fn manifest_package(decoded_root: &Path) -> Option<String> {
        let manifest_path = decoded_root.join("AndroidManifest.xml");
        let content = std::fs::read_to_string(manifest_path).ok()?;
        let re = regex::Regex::new(r#"\bpackage\s*=\s*"([^"]+)""#).ok()?;
        let cap = re.captures(&content)?;
        Some(cap[1].to_string())
    }

    pub fn rename_package(
        decoded_root: &Path,
        decompiled_root: Option<&Path>,
        new_package: &str,
    ) -> Result<(String, usize)> {
        let new_package = new_package.trim();
        if new_package.is_empty() {
            anyhow::bail!("Package name cannot be empty");
        }

        let package_re = regex::Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*(\.[A-Za-z_][A-Za-z0-9_]*)+$")
            .map_err(|e| anyhow::anyhow!("Regex error: {}", e))?;
        if !package_re.is_match(new_package) {
            anyhow::bail!("Invalid package name format: {}", new_package);
        }

        let manifest_path = decoded_root.join("AndroidManifest.xml");
        if !manifest_path.exists() {
            anyhow::bail!("AndroidManifest.xml not found in decoded workspace");
        }

        let old_package = Self::manifest_package(decoded_root)
            .ok_or_else(|| anyhow::anyhow!("Could not read package name from manifest"))?;

        if old_package == new_package {
            return Ok((old_package, 0));
        }

        let mut touched = 0usize;

        {
            let mut content = std::fs::read_to_string(&manifest_path)?;
            let attr_re = regex::Regex::new(r#"(\bpackage\s*=\s*")[^"]+(")"#)
                .map_err(|e| anyhow::anyhow!("Regex error: {}", e))?;
            if attr_re.is_match(&content) {
                content = attr_re
                    .replace(&content, format!("${{1}}{}${{2}}", new_package))
                    .to_string();
            }
            content = content.replace(&old_package, new_package);
            std::fs::write(&manifest_path, content)?;
            touched += 1;
        }

        let old_pkg_slash = old_package.replace('.', "/");
        let new_pkg_slash = new_package.replace('.', "/");

        if let Ok(entries) = std::fs::read_dir(decoded_root) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                if path.is_dir() && name.starts_with("smali") {
                    let from = path.join(&old_pkg_slash);
                    let to = path.join(&new_pkg_slash);
                    if from.exists() {
                        if let Some(parent) = to.parent() {
                            std::fs::create_dir_all(parent)?;
                        }
                        std::fs::rename(&from, &to)?;
                        touched += 1;
                    }

                    for file in WalkDir::new(&path).into_iter().filter_map(|e| e.ok()) {
                        if !file.path().is_file() {
                            continue;
                        }
                        let ext = file
                            .path()
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("")
                            .to_ascii_lowercase();
                        if ext != "smali" && ext != "xml" {
                            continue;
                        }

                        if let Ok(mut text) = std::fs::read_to_string(file.path()) {
                            let before = text.clone();
                            text = text.replace(&old_package, new_package);
                            text = text.replace(&old_pkg_slash, &new_pkg_slash);
                            if text != before {
                                std::fs::write(file.path(), text)?;
                                touched += 1;
                            }
                        }
                    }
                }
            }
        }

        if let Some(decompiled) = decompiled_root {
            let src_root = decompiled.join("sources");
            let from = src_root.join(&old_pkg_slash);
            let to = src_root.join(&new_pkg_slash);
            if from.exists() {
                if let Some(parent) = to.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if std::fs::rename(&from, &to).is_ok() {
                    touched += 1;
                }
            }
        }

        Ok((old_package, touched))
    }

    /// Renames a class project-wide. 
    /// This updates filenames and all references in .smali and .java files.
    pub fn rename_class(
        root: &Path,
        old_class: &str, // e.g., "com.example.OldClass"
        new_class: &str, // e.g., "com.example.NewClass"
    ) -> Result<usize> {
        let old_class = Self::normalize_class_name(old_class)?;
        let new_class = Self::normalize_class_name(new_class)?;
        if old_class == new_class {
            return Ok(0);
        }

        let old_slash = old_class.replace('.', "/");
        let new_slash = new_class.replace('.', "/");
        let old_smali_type = format!("L{};", old_slash);
        let new_smali_type = format!("L{};", new_slash);
        
        let files: Vec<PathBuf> = WalkDir::new(root)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                let ext = e.path().extension().and_then(|ext| ext.to_str());
                matches!(ext, Some("smali") | Some("java") | Some("xml"))
            })
            .map(|e| e.path().to_path_buf())
            .collect();

        let mut total_changes = 0usize;
        for path in files {
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let mut new_content = content.replace(&old_smali_type, &new_smali_type);
            new_content = new_content.replace(&old_slash, &new_slash);
            
            let ext = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
            if matches!(ext, "java" | "xml") {
                new_content = new_content.replace(&old_class, &new_class);
            }

            if new_content != content {
                std::fs::write(&path, new_content)?;
                total_changes += 1;
            }
        }

        let old_rel = format!("{}.smali", old_slash);
        let new_rel = format!("{}.smali", new_slash);
        for entry in WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|ext| ext.to_str()) != Some("smali") {
                continue;
            }
            let rel = path
                .strip_prefix(root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");
            if !rel.ends_with(&old_rel) {
                continue;
            }

            let Some(smali_root_rel) = rel.strip_suffix(&old_rel) else {
                continue;
            };
            let target = root.join(smali_root_rel).join(&new_rel);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::rename(path, target)?;
            total_changes += 1;
        }

        Ok(total_changes)
    }

    fn normalize_class_name(class_name: &str) -> Result<String> {
        let mut value = class_name.trim().trim_end_matches(';').to_string();
        if let Some(stripped) = value.strip_prefix('L') {
            value = stripped.to_string();
        }
        value = value.replace('/', ".");
        if value.is_empty() || !value.contains('.') {
            anyhow::bail!("Expected fully-qualified class name, got '{}'", class_name);
        }
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::RefactoringEngine;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("reveng_refactor_test_{}", nonce))
    }

    #[test]
    fn renames_smali_references_and_class_file() {
        let root = temp_dir();
        let smali_dir = root.join("smali/com/example");
        fs::create_dir_all(&smali_dir).unwrap();
        fs::write(
            smali_dir.join("OldClass.smali"),
            ".class public Lcom/example/OldClass;\n\
             invoke-static {}, Lcom/example/OldClass;->x()V\n",
        )
        .unwrap();

        let changed = RefactoringEngine::rename_class(
            &root,
            "com.example.OldClass",
            "com.example.NewClass",
        )
        .unwrap();

        let new_path = smali_dir.join("NewClass.smali");
        assert!(changed >= 2);
        assert!(new_path.exists());
        assert!(!smali_dir.join("OldClass.smali").exists());
        let content = fs::read_to_string(new_path).unwrap();
        assert!(content.contains("Lcom/example/NewClass;"));
        assert!(!content.contains("OldClass"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn renames_manifest_package_and_smali_tree() {
        let root = temp_dir();
        let smali_dir = root.join("decoded/smali/com/old/app");
        fs::create_dir_all(&smali_dir).unwrap();
        fs::write(
            root.join("decoded/AndroidManifest.xml"),
            r#"<manifest package="com.old.app"><application android:name="com.old.app.Main"/></manifest>"#,
        )
        .unwrap();
        fs::write(
            smali_dir.join("Main.smali"),
            ".class public Lcom/old/app/Main;\nconst-string v0, \"com.old.app\"\n",
        )
        .unwrap();

        let (old_package, changed) =
            RefactoringEngine::rename_package(&root.join("decoded"), None, "com.new.app").unwrap();

        assert_eq!(old_package, "com.old.app");
        assert!(changed >= 2);
        assert!(root.join("decoded/smali/com/new/app/Main.smali").exists());
        let manifest = fs::read_to_string(root.join("decoded/AndroidManifest.xml")).unwrap();
        assert!(manifest.contains(r#"package="com.new.app""#));
        assert!(!manifest.contains("com.old.app"));
        let smali = fs::read_to_string(root.join("decoded/smali/com/new/app/Main.smali")).unwrap();
        assert!(smali.contains("Lcom/new/app/Main;"));
        assert!(smali.contains("\"com.new.app\""));

        let _ = fs::remove_dir_all(root);
    }
}
