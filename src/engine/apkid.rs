//! APKiD integration — identifies packers, compilers, and protectors in APK files.

use crate::app::{AppState, LogLevel};
use anyhow::Result;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// A single detection entry from APKiD.
#[derive(Clone, Debug)]
pub struct ApkIdEntry {
    /// Detection category (e.g., "compiler", "packer", "anti_vm").
    pub category: String,
    /// Human-readable description of the detected pattern.
    pub description: String,
}

/// Results for one analysis target file.
#[derive(Clone, Debug)]
pub struct ApkIdResult {
    /// Path or name of the analyzed file (DEX, ELF, etc.).
    pub file: String,
    /// All detections found in this file.
    pub detections: Vec<ApkIdEntry>,
}

pub struct ApkIdAnalyzer;

impl ApkIdAnalyzer {
    /// Run APKiD on the given APK path and parse results.
    ///
    /// Requires `apkid` to be installed (`pip install apkid`).
    pub fn analyze(apk_path: &Path, state: &Arc<Mutex<AppState>>) -> Result<Vec<ApkIdResult>> {
        {
            state.lock().unwrap().push_log(
                LogLevel::Info,
                &format!("[apkid] Analyzing {} …", apk_path.display()),
            );
        }

        // Run apkid --json through the configured toolchain path.
        let stdout = {
            let s = state.lock().unwrap();
            s.toolchain
                .run_tool_string("apkid", &["--json", "--", &apk_path.to_string_lossy()])
        }
        .map_err(|e| anyhow::anyhow!("apkid failed or is unavailable. Install with `pip install apkid`, then refresh toolchain. Details: {}", e))?;

        // Parse JSON: { "files": { "path": { "category": ["desc", ...] } } }
        let results = Self::parse_json(&stdout).unwrap_or_else(|_| Self::parse_text(&stdout));

        let count: usize = results.iter().map(|r| r.detections.len()).sum();
        state.lock().unwrap().push_log(
            LogLevel::Info,
            &format!("[apkid] Found {} detection(s) across {} file(s).", count, results.len()),
        );

        Ok(results)
    }

    /// Parse APKiD --json output.
    fn parse_json(json: &str) -> Result<Vec<ApkIdResult>> {
        // Minimal hand-written JSON parser — avoids adding serde_json complexity
        // Expected shape: {"files": {"classes.dex": {"compiler": ["dexlib 2.x"], ...}}}
        let mut results = Vec::new();

        // Find "files" object
        let files_start = json.find("\"files\"").ok_or_else(|| anyhow::anyhow!("no files key"))?;
        let json_tail = &json[files_start..];

        // Simple split on quoted file entries — relies on apkid's stable output format
        // For each top-level key in files: grab the filename and its sub-object
        let mut depth = 0i32;
        let mut in_string = false;
        let mut current_file: Option<String> = None;
        let mut current_detections: Vec<ApkIdEntry> = Vec::new();
        let mut current_cat: Option<String> = None;
        let chars: Vec<char> = json_tail.chars().collect();
        let mut i = 0;

        // Skip to the first `{` after "files":
        while i < chars.len() && chars[i] != '{' { i += 1; }
        i += 1; // skip the "files" opening `{`

        while i < chars.len() {
            let ch = chars[i];
            match ch {
                '"' if !in_string => {
                    in_string = true;
                    i += 1;
                    let mut key = String::new();
                    while i < chars.len() && chars[i] != '"' {
                        if chars[i] == '\\' { i += 1; }
                        key.push(chars[i]);
                        i += 1;
                    }
                    // Determine role by depth
                    if depth == 0 {
                        // file name
                        if let Some(prev_file) = current_file.take() {
                            if !current_detections.is_empty() {
                                results.push(ApkIdResult { file: prev_file, detections: std::mem::take(&mut current_detections) });
                            }
                        }
                        current_file = Some(key);
                    } else if depth == 1 {
                        current_cat = Some(key);
                    } else if depth == 2 {
                        if let Some(cat) = current_cat.clone() {
                            current_detections.push(ApkIdEntry { category: cat, description: key });
                        }
                    }
                }
                '{' if !in_string => { depth += 1; }
                '}' if !in_string => {
                    depth -= 1;
                    if depth <= 0 {
                        if let Some(file) = current_file.take() {
                            if !current_detections.is_empty() {
                                results.push(ApkIdResult { file, detections: std::mem::take(&mut current_detections) });
                            }
                        }
                        break;
                    }
                }
                '"' if in_string => { in_string = false; }
                _ => {}
            }
            i += 1;
        }

        Ok(results)
    }

    /// Fallback: parse plain-text apkid output.
    ///
    /// Text format:
    /// ```
    /// [RESULTS]
    /// File: classes.dex
    /// compiler : dexlib 2.x
    /// ```
    fn parse_text(text: &str) -> Vec<ApkIdResult> {
        let mut results = Vec::new();
        let mut current_file = String::new();
        let mut detections = Vec::new();

        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('[') { continue; }
            if let Some(rest) = line.strip_prefix("File:") {
                if !current_file.is_empty() && !detections.is_empty() {
                    results.push(ApkIdResult { file: current_file.clone(), detections: std::mem::take(&mut detections) });
                }
                current_file = rest.trim().to_string();
            } else if let Some(colon) = line.find(':') {
                let category    = line[..colon].trim().to_string();
                let description = line[colon+1..].trim().to_string();
                if !description.is_empty() {
                    detections.push(ApkIdEntry { category, description });
                }
            }
        }

        if !current_file.is_empty() && !detections.is_empty() {
            results.push(ApkIdResult { file: current_file, detections });
        }

        results
    }
}
