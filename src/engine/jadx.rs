//! JADX integration — decompile DEX/APK to Java source.

use anyhow::Result;

/// Wrapper around JADX operations.
pub struct JadxRunner;

impl JadxRunner {
    /// Build decompile arguments for JADX CLI.
    pub fn decompile_args(input: &str, output_dir: &str) -> Vec<String> {
        vec![
            input.into(),
            "-d".into(),
            output_dir.into(),
            "--no-res".into(),       // skip resources (APKTool handles them)
            "--show-bad-code".into(), // show decompilation even if imperfect
        ]
    }

    /// Validate that a decompiled directory contains Java sources.
    pub fn validate_decompiled(dir: &std::path::Path) -> Result<()> {
        if !dir.exists() {
            anyhow::bail!("Decompiled directory does not exist: {}", dir.display());
        }

        // Check for any .java files
        let has_java = walkdir::WalkDir::new(dir)
            .max_depth(6)
            .into_iter()
            .filter_map(|e| e.ok())
            .any(|e| {
                e.path()
                    .extension()
                    .map(|ext| ext == "java")
                    .unwrap_or(false)
            });

        if !has_java {
            anyhow::bail!("No .java files found in decompiled output");
        }

        Ok(())
    }
}
