//! Navigation engine — indexes Java-to-Smali mappings.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use rayon::prelude::*;
use regex::Regex;
use walkdir::WalkDir;

/// A location in a Smali file.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SmaliLocation {
    pub path: PathBuf,
    pub line_number: usize,
}

/// Key used to look up Smali code from a Java context.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct JavaContext {
    pub class_name: String, // e.g., "com.ext.Target"
    pub method_name: String,
    pub line_number: usize,
}

/// The mapping table representing the entire workspace.
#[derive(Default, serde::Serialize, serde::Deserialize)]
pub struct NavIndex {
    /// Forward mapping: Java (Class, Method, Line) -> Smali Location
    pub mappings: HashMap<JavaContext, SmaliLocation>,
    /// Fallback mapping: Java class name -> representative Smali location.
    ///
    /// Used when debug line metadata is stripped and `.line` directives are missing.
    pub class_mappings: HashMap<String, SmaliLocation>,
    /// Reverse mapping: Smali file + line -> Java file + line
    pub reverse_mappings: HashMap<(String, usize), (String, usize)>,
}

pub struct NavIndexer;

impl NavIndexer {
    const MAX_NAV_SOURCE_FILE_BYTES: u64 = 16 * 1024 * 1024;
    const MAX_LINE_MAPPINGS_PER_FILE: usize = 2_000;
    const MAX_TOTAL_LINE_MAPPINGS: usize = 50_000;

    /// Scans the decoded/smali directory to build the navigation index.
    pub fn index_workspace(smali_root: &Path) -> NavIndex {
        let mut index = NavIndex::default();

        // Find all smali files
        let files: Vec<PathBuf> = WalkDir::new(smali_root)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "smali"))
            .map(|e| e.path().to_path_buf())
            .collect();

        // Regex for extraction
        let re_class = Regex::new(r"^\.class.*L([^;]+);").unwrap();
        let re_method = Regex::new(r"^\.method.* ([^\(]+)\(").unwrap();
        let re_line = Regex::new(r"^\.line\s+(\d+)").unwrap();

        // Per-file result: forward + reverse entries
        struct FileResult {
            forward: HashMap<JavaContext, SmaliLocation>,
            class_fallbacks: HashMap<String, SmaliLocation>,
            reverse: Vec<(String, usize, String, usize)>, // smali_path, smali_line, class_name, java_line
        }

        // Process files in parallel
        let results: Vec<FileResult> = files.par_iter().map(|path| {
            let mut forward = HashMap::new();
            let mut class_fallbacks = HashMap::new();
            let mut reverse = Vec::new();
            if Self::file_too_large(path) {
                return FileResult {
                    forward,
                    class_fallbacks,
                    reverse,
                };
            }
            let content = std::fs::read_to_string(path).unwrap_or_default();
            let smali_path_str = path.to_string_lossy().to_string();

            let mut current_class = String::new();
            let mut current_method = String::new();

            for (i, line) in content.lines().enumerate() {
                let line_num = i + 1;
                let trimmed = line.trim();

                if trimmed.starts_with(".class") {
                    if let Some(cap) = re_class.captures(trimmed) {
                        current_class = cap[1].replace("/", ".");
                        class_fallbacks.entry(current_class.clone()).or_insert_with(|| SmaliLocation {
                            path: path.clone(),
                            line_number: line_num,
                        });
                    }
                } else if trimmed.starts_with(".method") {
                    if let Some(cap) = re_method.captures(trimmed) {
                        current_method = cap[1].to_string();
                        if !current_class.is_empty() {
                            // Fallback mapping for builds without `.line` debug metadata.
                            forward.entry(JavaContext {
                                class_name: current_class.clone(),
                                method_name: current_method.clone(),
                                line_number: 0,
                            }).or_insert_with(|| SmaliLocation {
                                path: path.clone(),
                                line_number: line_num,
                            });
                            class_fallbacks.entry(current_class.clone()).or_insert_with(|| SmaliLocation {
                                path: path.clone(),
                                line_number: line_num,
                            });
                        }
                    }
                } else if trimmed.starts_with(".line") {
                    if let Some(cap) = re_line.captures(trimmed) {
                        let java_line: usize = cap[1].parse().unwrap_or(0);
                        if !current_class.is_empty() && !current_method.is_empty() {
                            if forward.len() < Self::MAX_LINE_MAPPINGS_PER_FILE {
                                forward.insert(
                                    JavaContext {
                                        class_name: current_class.clone(),
                                        method_name: current_method.clone(),
                                        line_number: java_line,
                                    },
                                    SmaliLocation {
                                        path: path.clone(),
                                        line_number: line_num,
                                    },
                                );
                            }
                            if reverse.len() < Self::MAX_LINE_MAPPINGS_PER_FILE {
                                // Reverse: smali_path + smali_line -> class_name + java_line
                                reverse.push((
                                    smali_path_str.clone(),
                                    line_num,
                                    current_class.clone(),
                                    java_line,
                                ));
                            }
                        }
                    }
                }
            }
            FileResult {
                forward,
                class_fallbacks,
                reverse,
            }
        })
        .collect();

        // Merge results
        for result in results {
            for (ctx, loc) in result.forward {
                if index.mappings.len() < Self::MAX_TOTAL_LINE_MAPPINGS {
                    index.mappings.insert(ctx, loc);
                }
            }
            for (class_name, loc) in result.class_fallbacks {
                index.class_mappings.entry(class_name).or_insert(loc);
            }
            for (smali_path, smali_line, class_name, java_line) in result.reverse {
                if index.reverse_mappings.len() < Self::MAX_TOTAL_LINE_MAPPINGS {
                    index.reverse_mappings.insert((smali_path, smali_line), (class_name, java_line));
                }
            }
        }

        index
    }

    fn file_too_large(path: &Path) -> bool {
        std::fs::metadata(path)
            .map(|meta| meta.len() > Self::MAX_NAV_SOURCE_FILE_BYTES)
            .unwrap_or(true)
    }
}

#[cfg(test)]
mod tests {
    use super::{JavaContext, NavIndexer};
    use std::fs;
    use std::io::{Seek, Write};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn indexes_trimmed_line_directives() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("reveng-nav-test-{}", unique));
        let smali_dir = root.join("smali").join("com").join("example");
        fs::create_dir_all(&smali_dir).unwrap();

        let smali_path = smali_dir.join("MainActivity.smali");
        fs::write(
            &smali_path,
            r#"
.class public Lcom/example/MainActivity;
.super Ljava/lang/Object;

.method public onCreate()V
    .locals 0
    .line 42
    return-void
.end method
"#,
        )
        .unwrap();

        let index = NavIndexer::index_workspace(&root);
        let ctx = JavaContext {
            class_name: "com.example.MainActivity".to_string(),
            method_name: "onCreate".to_string(),
            line_number: 42,
        };

        assert!(index.mappings.contains_key(&ctx));
        assert!(index.class_mappings.contains_key("com.example.MainActivity"));
        assert_eq!(index.reverse_mappings.len(), 1);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn skips_oversized_smali_files() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("reveng-nav-huge-test-{}", unique));
        fs::create_dir_all(&root).unwrap();
        let smali_path = root.join("Huge.smali");
        let mut file = fs::File::create(&smali_path).unwrap();
        file.seek(std::io::SeekFrom::Start(NavIndexer::MAX_NAV_SOURCE_FILE_BYTES + 1))
            .unwrap();
        file.write_all(b".class public LHuge;").unwrap();
        drop(file);

        let index = NavIndexer::index_workspace(&root);

        assert!(index.mappings.is_empty());
        assert!(index.class_mappings.is_empty());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn caps_line_mappings_per_file() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("reveng-nav-cap-test-{}", unique));
        fs::create_dir_all(&root).unwrap();
        let lines = (0..(NavIndexer::MAX_LINE_MAPPINGS_PER_FILE + 50))
            .map(|idx| format!("    .line {}", idx + 1))
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(
            root.join("ManyLines.smali"),
            format!(
                ".class public LManyLines;\n.super Ljava/lang/Object;\n.method public m()V\n{}\n.end method\n",
                lines
            ),
        )
        .unwrap();

        let index = NavIndexer::index_workspace(&root);

        assert_eq!(index.mappings.len(), NavIndexer::MAX_LINE_MAPPINGS_PER_FILE);
        assert_eq!(index.reverse_mappings.len(), NavIndexer::MAX_LINE_MAPPINGS_PER_FILE);

        let _ = fs::remove_dir_all(&root);
    }
}
