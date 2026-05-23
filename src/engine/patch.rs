//! Patch engine — apply smali and resource patches.
//!
//! This module provides helpers for modifying decoded APK contents
//! before rebuilding. Future versions will support structured patch files.

use anyhow::Result;
use std::path::Path;

pub struct PatchEngine;

impl PatchEngine {
    const MAX_SEARCH_FILE_BYTES: u64 = 16 * 1024 * 1024;
    const MAX_SEARCH_RESULT_FILES: usize = 500;
    const MAX_SEARCH_MATCHES_PER_FILE: usize = 50;
    const MAX_SEARCH_LINE_CHARS: usize = 500;

    /// Replace all occurrences of `find` with `replace` in a file.
    pub fn patch_file(path: &Path, find: &str, replace: &str) -> Result<usize> {
        let content = std::fs::read_to_string(path)?;
        let count = content.matches(find).count();

        if count == 0 {
            return Ok(0);
        }

        let patched = content.replace(find, replace);
        std::fs::write(path, patched)?;

        Ok(count)
    }

    /// Search for a pattern across all files in a directory using rayon for parallelism.
    pub fn search_in_dir(
        dir: &Path,
        query: &str,
        extensions: &[&str],
    ) -> Result<Vec<SearchResult>> {
        use rayon::prelude::*;

        let entries: Vec<_> = walkdir::WalkDir::new(dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path().is_file()
                    && e.path()
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .map(|ext| extensions.is_empty() || extensions.contains(&ext))
                        .unwrap_or(false)
            })
            .map(|e| e.path().to_path_buf())
            .collect();

        let mut results: Vec<SearchResult> = entries
            .par_iter()
            .filter_map(|path| {
                if std::fs::metadata(path).ok()?.len() > Self::MAX_SEARCH_FILE_BYTES {
                    return None;
                }
                let content = std::fs::read_to_string(path).ok()?;
                let query_lower = query.to_lowercase();
                let matches: Vec<SearchMatch> = content
                    .lines()
                    .enumerate()
                    .filter(|(_, line)| line.to_lowercase().contains(&query_lower))
                    .take(Self::MAX_SEARCH_MATCHES_PER_FILE)
                    .map(|(i, line)| SearchMatch {
                        line_number: i + 1,
                        line_content: Self::truncate_search_line(line),
                    })
                    .collect();

                if matches.is_empty() {
                    None
                } else {
                    Some(SearchResult {
                        path: path.clone(),
                        matches,
                    })
                }
            })
            .collect();

        results.sort_by(|a, b| a.path.cmp(&b.path));
        results.truncate(Self::MAX_SEARCH_RESULT_FILES);

        Ok(results)
    }

    fn truncate_search_line(line: &str) -> String {
        if line.chars().count() <= Self::MAX_SEARCH_LINE_CHARS {
            return line.to_string();
        }
        let mut out: String = line.chars().take(Self::MAX_SEARCH_LINE_CHARS).collect();
        out.push_str("...");
        out
    }
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub path: std::path::PathBuf,
    pub matches: Vec<SearchMatch>,
}

#[derive(Debug, Clone)]
pub struct SearchMatch {
    pub line_number: usize,
    pub line_content: String,
}

#[cfg(test)]
mod tests {
    use super::PatchEngine;
    use std::fs;
    use std::io::{Seek, Write};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("reveng_patch_test_{}_{}", name, nonce))
    }

    #[test]
    fn search_caps_matches_and_truncates_lines_on_character_boundaries() {
        let root = temp_dir("search_caps");
        fs::create_dir_all(&root).unwrap();
        let long_line = format!("needle {}", "é".repeat(PatchEngine::MAX_SEARCH_LINE_CHARS + 10));
        let content = (0..(PatchEngine::MAX_SEARCH_MATCHES_PER_FILE + 10))
            .map(|_| long_line.clone())
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(root.join("Many.smali"), content).unwrap();

        let results = PatchEngine::search_in_dir(&root, "needle", &["smali"]).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].matches.len(), PatchEngine::MAX_SEARCH_MATCHES_PER_FILE);
        assert!(results[0].matches[0].line_content.ends_with("..."));
        assert!(results[0].matches[0].line_content.is_char_boundary(results[0].matches[0].line_content.len()));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn search_skips_oversized_files() {
        let root = temp_dir("search_big");
        fs::create_dir_all(&root).unwrap();
        let path = root.join("Huge.smali");
        let mut file = fs::File::create(&path).unwrap();
        file.seek(std::io::SeekFrom::Start(PatchEngine::MAX_SEARCH_FILE_BYTES + 1))
            .unwrap();
        file.write_all(b"needle").unwrap();
        drop(file);

        let results = PatchEngine::search_in_dir(&root, "needle", &["smali"]).unwrap();

        assert!(results.is_empty());

        let _ = fs::remove_dir_all(root);
    }
}
