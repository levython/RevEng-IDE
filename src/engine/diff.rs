//! APK Diff — compare two decoded APK directories to find changes.
//! Useful for seeing what changed between original and patched APK versions.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;

/// Type of change detected.
#[derive(Clone, Debug, PartialEq)]
pub enum DiffType {
    Added,
    Removed,
    Modified,
}

impl DiffType {
    pub fn label(&self) -> &str {
        match self {
            Self::Added => "ADDED",
            Self::Removed => "REMOVED",
            Self::Modified => "MODIFIED",
        }
    }

}

/// A single file difference.
#[derive(Clone, Debug)]
pub struct FileDiff {
    pub path: String,
    pub diff_type: DiffType,
    pub size_a: u64,
    pub size_b: u64,
}

/// Full APK diff result.
#[derive(Clone, Debug)]
pub struct ApkDiffResult {
    pub dir_a: PathBuf,
    pub dir_b: PathBuf,
    pub diffs: Vec<FileDiff>,
    pub total_files_a: usize,
    pub total_files_b: usize,
}

impl ApkDiffResult {
    pub fn added_count(&self) -> usize {
        self.diffs.iter().filter(|d| d.diff_type == DiffType::Added).count()
    }
    pub fn removed_count(&self) -> usize {
        self.diffs.iter().filter(|d| d.diff_type == DiffType::Removed).count()
    }
    pub fn modified_count(&self) -> usize {
        self.diffs.iter().filter(|d| d.diff_type == DiffType::Modified).count()
    }
}

pub struct ApkDiffer;

impl ApkDiffer {
    /// Compare two decoded APK directories.
    pub fn diff(dir_a: &Path, dir_b: &Path) -> Result<ApkDiffResult> {
        let files_a = Self::scan_files(dir_a)?;
        let files_b = Self::scan_files(dir_b)?;

        let mut diffs = Vec::new();

        // Check files in A
        for (rel_path, (size_a, hash_a)) in &files_a {
            if let Some((size_b, hash_b)) = files_b.get(rel_path) {
                // File exists in both — check if modified
                if hash_a != hash_b {
                    diffs.push(FileDiff {
                        path: rel_path.clone(),
                        diff_type: DiffType::Modified,
                        size_a: *size_a,
                        size_b: *size_b,
                    });
                }
            } else {
                // File only in A — removed in B
                diffs.push(FileDiff {
                    path: rel_path.clone(),
                    diff_type: DiffType::Removed,
                    size_a: *size_a,
                    size_b: 0,
                });
            }
        }

        // Check files only in B — added
        for (rel_path, (size_b, _)) in &files_b {
            if !files_a.contains_key(rel_path) {
                diffs.push(FileDiff {
                    path: rel_path.clone(),
                    diff_type: DiffType::Added,
                    size_a: 0,
                    size_b: *size_b,
                });
            }
        }

        // Sort: modified first, then added, then removed
        diffs.sort_by(|a, b| {
            let type_ord = |t: &DiffType| match t {
                DiffType::Modified => 0,
                DiffType::Added => 1,
                DiffType::Removed => 2,
            };
            type_ord(&a.diff_type)
                .cmp(&type_ord(&b.diff_type))
                .then_with(|| a.path.cmp(&b.path))
        });

        Ok(ApkDiffResult {
            dir_a: dir_a.to_path_buf(),
            dir_b: dir_b.to_path_buf(),
            diffs,
            total_files_a: files_a.len(),
            total_files_b: files_b.len(),
        })
    }

    /// Scan directory and return map of relative_path -> (file_size, content_hash).
    fn scan_files(root: &Path) -> Result<HashMap<String, (u64, u64)>> {
        let mut map = HashMap::new();

        for entry in walkdir::WalkDir::new(root)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            let rel = entry
                .path()
                .strip_prefix(root)
                .unwrap_or(entry.path())
                .to_string_lossy()
                .replace('\\', "/");

            let metadata = std::fs::metadata(entry.path()).ok();
            let size = metadata.map(|m| m.len()).unwrap_or(0);

            // Streaming hash over the full file. APK resources can share size
            // and prefixes, so first-block fingerprints miss real changes.
            let hash = Self::fast_hash(entry.path(), size);

            map.insert(rel, (size, hash));
        }

        Ok(map)
    }

    /// Fast non-cryptographic content hash seeded with file size.
    fn fast_hash(path: &Path, size: u64) -> u64 {
        use std::io::Read;
        let mut hasher: u64 = 0xcbf29ce484222325 ^ size;
        if let Ok(mut file) = std::fs::File::open(path) {
            let mut buf = [0u8; 8192];
            while let Ok(n) = file.read(&mut buf) {
                if n == 0 {
                    break;
                }
                for &b in &buf[..n] {
                    hasher ^= b as u64;
                    hasher = hasher.wrapping_mul(0x100000001b3);
                }
            }
        }
        hasher
    }
}

#[cfg(test)]
mod tests {
    use super::{ApkDiffer, DiffType};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("reveng_diff_test_{}_{}", name, nonce))
    }

    #[test]
    fn reports_added_removed_and_modified_files() {
        let dir_a = temp_dir("a");
        let dir_b = temp_dir("b");
        fs::create_dir_all(dir_a.join("smali")).unwrap();
        fs::create_dir_all(dir_b.join("smali")).unwrap();

        fs::write(dir_a.join("same.txt"), "same").unwrap();
        fs::write(dir_b.join("same.txt"), "same").unwrap();
        fs::write(dir_a.join("removed.txt"), "old").unwrap();
        fs::write(dir_b.join("added.txt"), "new").unwrap();
        fs::write(dir_a.join("smali/Changed.smali"), "before").unwrap();
        fs::write(dir_b.join("smali/Changed.smali"), "after").unwrap();

        let diff = ApkDiffer::diff(&dir_a, &dir_b).unwrap();

        assert_eq!(diff.total_files_a, 3);
        assert_eq!(diff.total_files_b, 3);
        assert_eq!(diff.modified_count(), 1);
        assert_eq!(diff.added_count(), 1);
        assert_eq!(diff.removed_count(), 1);
        assert!(diff
            .diffs
            .iter()
            .any(|d| d.path == "smali/Changed.smali" && d.diff_type == DiffType::Modified));

        let _ = fs::remove_dir_all(dir_a);
        let _ = fs::remove_dir_all(dir_b);
    }

    #[test]
    fn detects_same_size_change_after_first_read_block() {
        let dir_a = temp_dir("late_a");
        let dir_b = temp_dir("late_b");
        fs::create_dir_all(&dir_a).unwrap();
        fs::create_dir_all(&dir_b).unwrap();

        let mut a = vec![b'A'; 10_000];
        let mut b = vec![b'A'; 10_000];
        a[9_500] = b'B';
        b[9_500] = b'C';
        fs::write(dir_a.join("classes.dex"), a).unwrap();
        fs::write(dir_b.join("classes.dex"), b).unwrap();

        let diff = ApkDiffer::diff(&dir_a, &dir_b).unwrap();
        assert_eq!(diff.modified_count(), 1);
        assert!(diff
            .diffs
            .iter()
            .any(|d| d.path == "classes.dex" && d.diff_type == DiffType::Modified));

        let _ = fs::remove_dir_all(dir_a);
        let _ = fs::remove_dir_all(dir_b);
    }
}
