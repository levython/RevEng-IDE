//! APKTool integration — decode and rebuild APKs.

use anyhow::Result;

/// Wrapper around APKTool operations.
/// NOTE: Actual execution is delegated through the ToolchainManager
/// via the ApkProcessor. This module provides utility helpers.
pub struct ApkToolRunner;

impl ApkToolRunner {
    /// Build the decode arguments for APKTool.
    pub fn decode_args(apk_path: &str, output_dir: &str) -> Vec<String> {
        vec![
            "d".into(),
            apk_path.into(),
            "-o".into(),
            output_dir.into(),
            "-f".into(), // force overwrite
        ]
    }

    /// Build the rebuild arguments for APKTool.
    pub fn build_args(source_dir: &str, output_apk: &str) -> Vec<String> {
        vec![
            "b".into(),
            source_dir.into(),
            "-o".into(),
            output_apk.into(),
        ]
    }

    /// Validate that a decoded directory looks correct.
    pub fn validate_decoded(dir: &std::path::Path) -> Result<()> {
        let manifest = dir.join("AndroidManifest.xml");
        if !manifest.exists() {
            anyhow::bail!(
                "Invalid decoded directory: AndroidManifest.xml not found in {}",
                dir.display()
            );
        }
        Ok(())
    }
}
