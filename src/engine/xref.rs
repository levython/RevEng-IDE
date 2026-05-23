//! Smali cross-reference engine — indexes method calls, field accesses, class
//! hierarchy, string constants, and type references across all smali files.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use rayon::prelude::*;
use regex::Regex;

/// A fully qualified method reference: Lcom/example/Foo;->bar(II)V
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MethodRef {
    pub class: String,
    pub name: String,
    pub descriptor: String,
}

impl MethodRef {
    pub fn full_signature(&self) -> String {
        format!("{}->{}({})", self.class, self.name, self.descriptor)
    }

    pub fn short_name(&self) -> String {
        let class_short = self.class.rsplit('/').next().unwrap_or(&self.class)
            .trim_end_matches(';');
        format!("{}.{}", class_short, self.name)
    }
}

/// A fully qualified field reference: Lcom/example/Foo;->baz:I
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FieldRef {
    pub class: String,
    pub name: String,
    pub field_type: String,
}

impl FieldRef {
    pub fn full_signature(&self) -> String {
        format!("{}->{}:{}", self.class, self.name, self.field_type)
    }
}

/// A location in the code where a reference occurs.
#[derive(Debug, Clone)]
pub struct CodeSite {
    pub file: PathBuf,
    pub line: usize,
    pub in_class: String,
    pub in_method: String,
    pub instruction: String,
}

/// Information about a class.
#[derive(Debug, Clone)]
pub struct ClassInfo {
    pub name: String,
    pub super_class: String,
    pub interfaces: Vec<String>,
    pub file: PathBuf,
    pub methods: Vec<MethodRef>,
    pub fields: Vec<FieldRef>,
    pub is_abstract: bool,
    pub is_interface: bool,
}

/// The master cross-reference database.
pub struct SmaliXrefDb {
    /// Method callers: callee -> list of call sites
    pub method_callers: HashMap<MethodRef, Vec<CodeSite>>,
    /// Field read sites: field -> list of read locations
    pub field_reads: HashMap<FieldRef, Vec<CodeSite>>,
    /// Field write sites: field -> list of write locations
    pub field_writes: HashMap<FieldRef, Vec<CodeSite>>,
    /// Class hierarchy
    pub classes: HashMap<String, ClassInfo>,
    /// String constant references: value -> usage sites
    pub string_refs: HashMap<String, Vec<CodeSite>>,
    /// Type references: class_name -> sites where it appears
    pub type_refs: HashMap<String, Vec<CodeSite>>,
    /// Total files indexed
    pub file_count: usize,
}

impl SmaliXrefDb {
    fn new() -> Self {
        Self {
            method_callers: HashMap::new(),
            field_reads: HashMap::new(),
            field_writes: HashMap::new(),
            classes: HashMap::new(),
            string_refs: HashMap::new(),
            type_refs: HashMap::new(),
            file_count: 0,
        }
    }

    /// Find all callers of a method.
    pub fn find_callers(&self, method: &MethodRef) -> Vec<&CodeSite> {
        self.method_callers.get(method).map(|v| v.iter().collect()).unwrap_or_default()
    }

    /// Find all callers by method name (ignoring descriptor for fuzzy match).
    pub fn find_callers_by_name(&self, class: &str, method_name: &str) -> Vec<(&MethodRef, &[CodeSite])> {
        self.method_callers.iter()
            .filter(|(k, _)| k.class == class && k.name == method_name)
            .map(|(k, v)| (k, v.as_slice()))
            .collect()
    }

    /// Find all sites that reference a class type.
    pub fn find_type_usages(&self, class_name: &str) -> Vec<&CodeSite> {
        self.type_refs.get(class_name).map(|v| v.iter().collect()).unwrap_or_default()
    }

    /// Find all sites where a string constant appears.
    pub fn find_string_usages(&self, value: &str) -> Vec<&CodeSite> {
        self.string_refs.get(value).map(|v| v.iter().collect()).unwrap_or_default()
    }

    /// Get ancestors (super chain) for a class.
    pub fn get_class_hierarchy(&self, class_name: &str) -> Vec<String> {
        let mut chain = Vec::new();
        let mut current = class_name.to_string();
        let mut seen = std::collections::HashSet::new();
        while let Some(info) = self.classes.get(&current) {
            if !seen.insert(current.clone()) { break; }
            if !info.super_class.is_empty() && info.super_class != "Ljava/lang/Object;" {
                chain.push(info.super_class.clone());
                current = info.super_class.clone();
            } else {
                break;
            }
        }
        chain
    }

    /// Get all classes that implement a given interface.
    pub fn find_implementors(&self, interface: &str) -> Vec<&ClassInfo> {
        self.classes.values()
            .filter(|c| c.interfaces.contains(&interface.to_string()))
            .collect()
    }

    /// Get all subclasses of a given class.
    pub fn find_subclasses(&self, class_name: &str) -> Vec<&ClassInfo> {
        self.classes.values()
            .filter(|c| c.super_class == class_name)
            .collect()
    }
}

/// Per-file extraction result (before merging).
struct FileXrefResult {
    class_info: Option<ClassInfo>,
    method_calls: Vec<(MethodRef, CodeSite)>,
    field_reads: Vec<(FieldRef, CodeSite)>,
    field_writes: Vec<(FieldRef, CodeSite)>,
    string_refs: Vec<(String, CodeSite)>,
    type_refs: Vec<(String, CodeSite)>,
}

pub struct XrefIndexer;

impl XrefIndexer {
    const MAX_XREF_SOURCE_FILE_BYTES: u64 = 16 * 1024 * 1024;
    const MAX_REFS_PER_FILE_KIND: usize = 2_000;
    const MAX_SITES_PER_KEY: usize = 200;
    const MAX_STRING_REF_CHARS: usize = 1_000;

    /// Build the cross-reference database from all smali files.
    pub fn index_workspace(smali_root: &Path) -> SmaliXrefDb {
        let smali_files: Vec<PathBuf> = walkdir::WalkDir::new(smali_root)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_type().is_file()
                    && e.path().extension().map(|x| x == "smali").unwrap_or(false)
            })
            .map(|e| e.into_path())
            .collect();

        let results: Vec<FileXrefResult> = smali_files
            .par_iter()
            .filter_map(|path| Self::parse_file(path).ok())
            .collect();

        let mut db = SmaliXrefDb::new();
        db.file_count = results.len();

        for result in results {
            if let Some(ci) = result.class_info {
                db.classes.insert(ci.name.clone(), ci);
            }
            for (method, site) in result.method_calls {
                Self::push_limited_site(db.method_callers.entry(method).or_default(), site);
            }
            for (field, site) in result.field_reads {
                Self::push_limited_site(db.field_reads.entry(field).or_default(), site);
            }
            for (field, site) in result.field_writes {
                Self::push_limited_site(db.field_writes.entry(field).or_default(), site);
            }
            for (s, site) in result.string_refs {
                Self::push_limited_site(db.string_refs.entry(s).or_default(), site);
            }
            for (t, site) in result.type_refs {
                Self::push_limited_site(db.type_refs.entry(t).or_default(), site);
            }
        }

        db
    }

    fn parse_file(path: &Path) -> Result<FileXrefResult> {
        if Self::file_too_large(path) {
            return Ok(FileXrefResult::empty());
        }
        let content = std::fs::read_to_string(path)?;

        let re_class = Regex::new(r"^\.class\s+(.*?)\s*(L[^;]+;)").unwrap();
        let re_super = Regex::new(r"^\.super\s+(L[^;]+;)").unwrap();
        let re_implements = Regex::new(r"^\.implements\s+(L[^;]+;)").unwrap();
        let re_method_def = Regex::new(r"^\.method\s+(.+?)\s*([^\s(]+)\(([^)]*)\)(.+)").unwrap();
        let re_field_def = Regex::new(r"^\.field\s+(.+?)\s+([^\s:]+):(.+)").unwrap();
        let re_invoke = Regex::new(r"invoke-\w+(?:/range)?\s+\{[^}]*\},\s*(L[^;]+;)->([^(]+)\(([^)]*)\)(.+)").unwrap();
        let re_field_get = Regex::new(r"[is]get(?:-\w+)?\s+\w+,\s*(L[^;]+;)->([^:]+):(.+)").unwrap();
        let re_field_put = Regex::new(r"[is]put(?:-\w+)?\s+\w+,\s*(L[^;]+;)->([^:]+):(.+)").unwrap();
        let re_const_string = Regex::new(r#"const-string(?:/jumbo)?\s+\w+,\s*"([^"]*)""#).unwrap();
        let re_type = Regex::new(r"(L[a-zA-Z0-9_$/]+;)").unwrap();

        let mut class_name = String::new();
        let mut super_class = String::new();
        let mut interfaces = Vec::new();
        let mut class_methods = Vec::new();
        let mut class_fields = Vec::new();
        let mut is_abstract = false;
        let mut is_interface = false;

        let mut current_method = String::new();
        let mut in_method = false;

        let mut method_calls = Vec::new();
        let mut field_reads = Vec::new();
        let mut field_writes = Vec::new();
        let mut string_refs = Vec::new();
        let mut type_refs = Vec::new();

        for (line_num, line) in content.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            // .class directive
            if let Some(caps) = re_class.captures(trimmed) {
                let flags = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                class_name = caps.get(2).map(|m| m.as_str().to_string()).unwrap_or_default();
                is_abstract = flags.contains("abstract");
                is_interface = flags.contains("interface");
                continue;
            }

            // .super directive
            if let Some(caps) = re_super.captures(trimmed) {
                super_class = caps.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
                continue;
            }

            // .implements directive
            if let Some(caps) = re_implements.captures(trimmed) {
                if let Some(iface) = caps.get(1) {
                    interfaces.push(iface.as_str().to_string());
                }
                continue;
            }

            // .method definition
            if let Some(caps) = re_method_def.captures(trimmed) {
                let name = caps.get(2).map(|m| m.as_str().to_string()).unwrap_or_default();
                let params = caps.get(3).map(|m| m.as_str().to_string()).unwrap_or_default();
                let ret = caps.get(4).map(|m| m.as_str().to_string()).unwrap_or_default();
                let method_ref = MethodRef {
                    class: class_name.clone(),
                    name: name.clone(),
                    descriptor: format!("{}){}", params, ret),
                };
                class_methods.push(method_ref);
                current_method = name;
                in_method = true;
                continue;
            }

            if trimmed.starts_with(".end method") {
                in_method = false;
                current_method.clear();
                continue;
            }

            // .field definition
            if let Some(caps) = re_field_def.captures(trimmed) {
                let name = caps.get(2).map(|m| m.as_str().to_string()).unwrap_or_default();
                let ftype = caps.get(3).map(|m| m.as_str().trim().to_string()).unwrap_or_default();
                class_fields.push(FieldRef {
                    class: class_name.clone(),
                    name,
                    field_type: ftype,
                });
                continue;
            }

            // Only process instructions inside methods
            if !in_method {
                continue;
            }

            let make_site = |instruction: &str| CodeSite {
                file: path.to_path_buf(),
                line: line_num + 1,
                in_class: class_name.clone(),
                in_method: current_method.clone(),
                instruction: instruction.to_string(),
            };

            // invoke-* instructions
            if let Some(caps) = re_invoke.captures(trimmed) {
                if method_calls.len() < Self::MAX_REFS_PER_FILE_KIND {
                    let callee_class = caps.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
                    let callee_name = caps.get(2).map(|m| m.as_str().to_string()).unwrap_or_default();
                    let callee_params = caps.get(3).map(|m| m.as_str().to_string()).unwrap_or_default();
                    let callee_ret = caps.get(4).map(|m| m.as_str().to_string()).unwrap_or_default();
                    let method_ref = MethodRef {
                        class: callee_class,
                        name: callee_name,
                        descriptor: format!("{}){}", callee_params, callee_ret),
                    };
                    method_calls.push((method_ref, make_site(trimmed)));
                }
            }

            // Field get (read)
            if let Some(caps) = re_field_get.captures(trimmed) {
                if field_reads.len() < Self::MAX_REFS_PER_FILE_KIND {
                    let fc = caps.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
                    let fn_ = caps.get(2).map(|m| m.as_str().to_string()).unwrap_or_default();
                    let ft = caps.get(3).map(|m| m.as_str().trim().to_string()).unwrap_or_default();
                    field_reads.push((FieldRef { class: fc, name: fn_, field_type: ft }, make_site(trimmed)));
                }
            }

            // Field put (write)
            if let Some(caps) = re_field_put.captures(trimmed) {
                if field_writes.len() < Self::MAX_REFS_PER_FILE_KIND {
                    let fc = caps.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
                    let fn_ = caps.get(2).map(|m| m.as_str().to_string()).unwrap_or_default();
                    let ft = caps.get(3).map(|m| m.as_str().trim().to_string()).unwrap_or_default();
                    field_writes.push((FieldRef { class: fc, name: fn_, field_type: ft }, make_site(trimmed)));
                }
            }

            // const-string
            if let Some(caps) = re_const_string.captures(trimmed) {
                if string_refs.len() < Self::MAX_REFS_PER_FILE_KIND {
                    if let Some(val) = caps.get(1) {
                        string_refs.push((Self::truncate_string_key(val.as_str()), make_site(trimmed)));
                    }
                }
            }

            // Type references (all L...; patterns in instruction lines)
            for caps in re_type.captures_iter(trimmed) {
                if let Some(t) = caps.get(1) {
                    let type_name = t.as_str().to_string();
                    if type_name != class_name {
                        if type_refs.len() < Self::MAX_REFS_PER_FILE_KIND {
                            type_refs.push((type_name, make_site(trimmed)));
                        }
                    }
                }
            }
        }

        let class_info = if !class_name.is_empty() {
            Some(ClassInfo {
                name: class_name,
                super_class,
                interfaces,
                file: path.to_path_buf(),
                methods: class_methods,
                fields: class_fields,
                is_abstract,
                is_interface,
            })
        } else {
            None
        };

        Ok(FileXrefResult {
            class_info,
            method_calls,
            field_reads,
            field_writes,
            string_refs,
            type_refs,
        })
    }

    fn file_too_large(path: &Path) -> bool {
        std::fs::metadata(path)
            .map(|meta| meta.len() > Self::MAX_XREF_SOURCE_FILE_BYTES)
            .unwrap_or(true)
    }

    fn push_limited_site(sites: &mut Vec<CodeSite>, site: CodeSite) {
        if sites.len() < Self::MAX_SITES_PER_KEY {
            sites.push(site);
        }
    }

    fn truncate_string_key(value: &str) -> String {
        if value.chars().count() <= Self::MAX_STRING_REF_CHARS {
            return value.to_string();
        }
        let mut out: String = value.chars().take(Self::MAX_STRING_REF_CHARS).collect();
        out.push_str("...");
        out
    }
}

impl FileXrefResult {
    fn empty() -> Self {
        Self {
            class_info: None,
            method_calls: Vec::new(),
            field_reads: Vec::new(),
            field_writes: Vec::new(),
            string_refs: Vec::new(),
            type_refs: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::XrefIndexer;
    use std::fs;
    use std::io::{Seek, Write};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("reveng_xref_test_{}_{}", name, nonce))
    }

    #[test]
    fn xref_skips_oversized_source_files() {
        let root = temp_dir("huge");
        fs::create_dir_all(&root).unwrap();
        let path = root.join("Huge.smali");
        let mut file = fs::File::create(&path).unwrap();
        file.seek(std::io::SeekFrom::Start(XrefIndexer::MAX_XREF_SOURCE_FILE_BYTES + 1))
            .unwrap();
        file.write_all(b".class public LHuge;").unwrap();
        drop(file);

        let db = XrefIndexer::index_workspace(&root);

        assert!(db.classes.is_empty());
        assert!(db.method_callers.is_empty());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn xref_caps_sites_per_key_and_truncates_string_keys() {
        let root = temp_dir("caps");
        fs::create_dir_all(&root).unwrap();
        for i in 0..(XrefIndexer::MAX_SITES_PER_KEY + 10) {
            let path = root.join(format!("C{i}.smali"));
            fs::write(
                path,
                ".class public LC;\n\
                 .super Ljava/lang/Object;\n\
                 .method public static m()V\n\
                 invoke-static {}, LTarget;->hit()V\n\
                 return-void\n\
                 .end method\n",
            )
            .unwrap();
        }
        let long_string = "é".repeat(XrefIndexer::MAX_STRING_REF_CHARS + 10);
        fs::write(
            root.join("String.smali"),
            format!(
                ".class public LS;\n.super Ljava/lang/Object;\n.method public static s()V\nconst-string v0, \"{}\"\n.end method\n",
                long_string
            ),
        )
        .unwrap();

        let db = XrefIndexer::index_workspace(&root);
        let sites = db.method_callers.values().next().unwrap();

        assert_eq!(sites.len(), XrefIndexer::MAX_SITES_PER_KEY);
        let key = db.string_refs.keys().next().unwrap();
        assert!(key.ends_with("..."));
        assert!(key.is_char_boundary(key.len()));

        let _ = fs::remove_dir_all(root);
    }
}
