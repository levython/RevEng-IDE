//! DEX statistics — method/class/field counts, 64K limit gauge, obfuscation score.

use std::path::Path;

/// Per-DEX directory stats.
#[derive(Clone, Debug)]
pub struct DexDirStats {
    pub name: String,
    pub class_count: usize,
    pub method_count: usize,
    pub field_count: usize,
}

/// Overall DEX statistics for the workspace.
#[derive(Clone, Debug)]
pub struct DexStats {
    pub dirs: Vec<DexDirStats>,
    pub total_classes: usize,
    pub total_methods: usize,
    pub total_fields: usize,
    pub obfuscation_score: f32,
    pub top_packages: Vec<(String, usize)>,
}

impl DexStats {
    /// The Android DEX method limit.
    pub const METHOD_LIMIT: usize = 65536;

    /// Percentage of method limit used by the largest DEX dir.
    pub fn max_method_pct(&self) -> f32 {
        let max = self.dirs.iter().map(|d| d.method_count).max().unwrap_or(0);
        (max as f32 / Self::METHOD_LIMIT as f32) * 100.0
    }
}

pub struct DexAnalyzer;

impl DexAnalyzer {
    const MAX_DEX_STATS_SOURCE_FILE_BYTES: u64 = 16 * 1024 * 1024;

    /// Compute DEX stats from decoded smali directories.
    pub fn analyze(decoded_root: &Path) -> DexStats {
        let mut dirs = Vec::new();
        let mut total_classes = 0;
        let mut total_methods = 0;
        let mut total_fields = 0;
        let mut package_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        let mut short_name_count = 0_usize;
        let mut total_name_count = 0_usize;

        let re_method = regex::Regex::new(r"^\.method\s").unwrap();
        let re_field = regex::Regex::new(r"^\.field\s").unwrap();
        let re_class = regex::Regex::new(r"^\.class\s.*L([^;]+);").unwrap();

        // Scan smali, smali_classes2, etc.
        let mut entries: Vec<_> = std::fs::read_dir(decoded_root)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name();
                let n = name.to_string_lossy();
                n.starts_with("smali") && e.path().is_dir()
            })
            .collect();
        entries.sort_by_key(|e| e.file_name());

        for entry in &entries {
            let dir_name = entry.file_name().to_string_lossy().to_string();
            let mut class_count = 0;
            let mut method_count = 0;
            let mut field_count = 0;

            for file_entry in walkdir::WalkDir::new(entry.path())
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| {
                    e.file_type().is_file()
                        && e.path().extension().map(|x| x == "smali").unwrap_or(false)
                })
            {
                if Self::file_too_large(file_entry.path()) {
                    continue;
                }
                let content = std::fs::read_to_string(file_entry.path()).unwrap_or_default();

                for line in content.lines() {
                    let trimmed = line.trim();
                    if let Some(caps) = re_class.captures(trimmed) {
                        class_count += 1;
                        let class_path = caps.get(1).map(|m| m.as_str()).unwrap_or("");

                        // Package: take first 3 path segments
                        let parts: Vec<&str> = class_path.split('/').collect();
                        let pkg = if parts.len() >= 3 {
                            parts[..3].join(".")
                        } else {
                            parts.join(".")
                        };
                        *package_counts.entry(pkg).or_insert(0) += 1;

                        // Obfuscation: check for short class names
                        let class_simple = parts.last().unwrap_or(&"");
                        total_name_count += 1;
                        if class_simple.len() <= 2 {
                            short_name_count += 1;
                        }
                    } else if re_method.is_match(trimmed) {
                        method_count += 1;
                    } else if re_field.is_match(trimmed) {
                        field_count += 1;
                    }
                }
            }

            total_classes += class_count;
            total_methods += method_count;
            total_fields += field_count;

            dirs.push(DexDirStats {
                name: dir_name,
                class_count,
                method_count,
                field_count,
            });
        }

        // Top packages by class count
        let mut top_packages: Vec<_> = package_counts.into_iter().collect();
        top_packages.sort_by(|a, b| b.1.cmp(&a.1));
        top_packages.truncate(10);

        // Obfuscation score (0-100): % of short names
        let obfuscation_score = if total_name_count > 0 {
            (short_name_count as f32 / total_name_count as f32) * 100.0
        } else {
            0.0
        };

        DexStats {
            dirs,
            total_classes,
            total_methods,
            total_fields,
            obfuscation_score,
            top_packages,
        }
    }

    fn file_too_large(path: &Path) -> bool {
        std::fs::metadata(path)
            .map(|meta| meta.len() > Self::MAX_DEX_STATS_SOURCE_FILE_BYTES)
            .unwrap_or(true)
    }
}

#[cfg(test)]
mod tests {
    use super::DexAnalyzer;
    use std::fs;
    use std::io::{Seek, Write};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir() -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("reveng_dex_stats_test_{}", nonce))
    }

    #[test]
    fn skips_oversized_smali_files() {
        let root = temp_dir();
        let smali = root.join("smali");
        fs::create_dir_all(&smali).unwrap();
        let mut huge = fs::File::create(smali.join("Huge.smali")).unwrap();
        huge.seek(std::io::SeekFrom::Start(
            DexAnalyzer::MAX_DEX_STATS_SOURCE_FILE_BYTES + 1,
        ))
        .unwrap();
        huge.write_all(b".class public LHuge;").unwrap();
        drop(huge);
        fs::write(
            smali.join("Small.smali"),
            ".class public Lcom/example/Small;\n.method public m()V\n.end method\n.field public x:I\n",
        )
        .unwrap();

        let stats = DexAnalyzer::analyze(&root);

        assert_eq!(stats.total_classes, 1);
        assert_eq!(stats.total_methods, 1);
        assert_eq!(stats.total_fields, 1);

        let _ = fs::remove_dir_all(root);
    }
}
