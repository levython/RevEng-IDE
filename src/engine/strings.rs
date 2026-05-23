//! String extractor — scans smali const-string values and resource XML strings,
//! then auto-categorises each entry (URL, API key, secret, file path, etc.).

use std::path::{Path, PathBuf};

use anyhow::Result;
use rayon::prelude::*;
use regex::Regex;

/// High-level category for an extracted string.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum StringCategory {
    Url,
    ApiKey,
    Secret,
    PackageName,
    FilePath,
    IpAddress,
    Email,
    Base64,
    Other,
}

impl StringCategory {
    pub fn label(&self) -> &str {
        match self {
            Self::Url => "URL",
            Self::ApiKey => "API Key",
            Self::Secret => "Secret",
            Self::PackageName => "Package",
            Self::FilePath => "Path",
            Self::IpAddress => "IP",
            Self::Email => "Email",
            Self::Base64 => "Base64",
            Self::Other => "Other",
        }
    }

    pub fn color(&self) -> egui::Color32 {
        match self {
            Self::Url => egui::Color32::from_rgb(130, 170, 255),
            Self::ApiKey => egui::Color32::from_rgb(255, 150, 80),
            Self::Secret => egui::Color32::from_rgb(255, 100, 100),
            Self::PackageName => egui::Color32::from_rgb(180, 220, 130),
            Self::FilePath => egui::Color32::from_rgb(200, 200, 140),
            Self::IpAddress => egui::Color32::from_rgb(200, 160, 255),
            Self::Email => egui::Color32::from_rgb(130, 220, 200),
            Self::Base64 => egui::Color32::from_rgb(220, 180, 130),
            Self::Other => egui::Color32::from_rgb(160, 160, 160),
        }
    }

    /// All categories for filter UI.
    pub fn all() -> &'static [StringCategory] {
        &[
            Self::Url,
            Self::ApiKey,
            Self::Secret,
            Self::PackageName,
            Self::FilePath,
            Self::IpAddress,
            Self::Email,
            Self::Base64,
            Self::Other,
        ]
    }
}

/// A single extracted string with its source location and category.
#[derive(Clone, Debug)]
pub struct ExtractedString {
    pub value: String,
    pub category: StringCategory,
    pub source_file: PathBuf,
    pub line: usize,
    pub context: StringContext,
}

/// Where the string was found.
#[derive(Clone, Debug, PartialEq)]
pub enum StringContext {
    SmaliConstString { class: String, method: String },
    ResourceXml { res_file: String },
}

impl StringContext {
    pub fn source_kind(&self) -> &'static str {
        match self {
            Self::SmaliConstString { .. } => "SMALI",
            Self::ResourceXml { .. } => "XML",
        }
    }
}

impl ExtractedString {
    pub fn location_label(&self) -> String {
        match &self.context {
            StringContext::SmaliConstString { class, method } => {
                if method.is_empty() {
                    format!("{}  line {}", class, self.line)
                } else {
                    format!("{}  {}  line {}", class, method, self.line)
                }
            }
            StringContext::ResourceXml { res_file } => format!("{}  line {}", res_file, self.line),
        }
    }

    pub fn searchable_text(&self) -> String {
        let mut blob = self.value.to_lowercase();
        blob.push(' ');
        blob.push_str(self.category.label().to_lowercase().as_str());
        blob.push(' ');
        blob.push_str(self.location_label().to_lowercase().as_str());
        blob.push(' ');
        blob.push_str(self.source_file.to_string_lossy().to_lowercase().as_str());
        blob
    }
}

pub struct StringExtractor;

impl StringExtractor {
    const MAX_STRING_SOURCE_FILE_BYTES: u64 = 16 * 1024 * 1024;
    const MAX_STRINGS_PER_FILE: usize = 500;
    const MAX_TOTAL_STRINGS: usize = 10_000;
    const MAX_STRING_VALUE_CHARS: usize = 2_000;

    /// Extract and categorize all strings from smali files under the given root.
    pub fn extract_from_smali(smali_root: &Path) -> Vec<ExtractedString> {
        let smali_files: Vec<PathBuf> = walkdir::WalkDir::new(smali_root)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_type().is_file()
                    && e.path().extension().map(|x| x == "smali").unwrap_or(false)
            })
            .map(|e| e.into_path())
            .collect();

        let mut strings: Vec<_> = smali_files
            .par_iter()
            .flat_map(|path| Self::parse_smali_file(path).unwrap_or_default())
            .collect();
        strings.truncate(Self::MAX_TOTAL_STRINGS);
        strings
    }

    /// Extract strings from resource XML files (res/values*/strings.xml).
    pub fn extract_from_resources(decoded_root: &Path) -> Vec<ExtractedString> {
        let res_dir = decoded_root.join("res");
        if !res_dir.exists() {
            return Vec::new();
        }

        let mut results = Vec::new();
        let re_string =
            Regex::new(r#"(?s)<string\b[^>]*name="([^"]*)"[^>]*>(.*?)</string>"#).unwrap();
        let re_tags = Regex::new(r"<[^>]+>").unwrap();

        for entry in walkdir::WalkDir::new(&res_dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name != "strings.xml" {
                continue;
            }
            let parent_name = path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("");
            if !parent_name.starts_with("values") {
                continue;
            }

            if Self::file_too_large(path) {
                continue;
            }
            let content = match std::fs::read_to_string(path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            for caps in re_string.captures_iter(&content) {
                let Some(full_match) = caps.get(0) else {
                    continue;
                };
                let raw_value = caps.get(2).map(|m| m.as_str()).unwrap_or("");
                let value = Self::truncate_value(&Self::decode_xml_string(&re_tags.replace_all(raw_value, "")));
                if value.is_empty() {
                    continue;
                }

                let line_num = content[..full_match.start()]
                    .bytes()
                    .filter(|b| *b == b'\n')
                    .count()
                    + 1;
                let res_file = path
                    .strip_prefix(decoded_root)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .to_string();
                let category = Self::categorize(&value);
                results.push(ExtractedString {
                    value,
                    category,
                    source_file: path.to_path_buf(),
                    line: line_num,
                    context: StringContext::ResourceXml { res_file },
                });
                if results.len() >= Self::MAX_TOTAL_STRINGS {
                    return results;
                }
            }
        }

        results
    }

    /// Extract all strings from workspace (smali + resources combined).
    pub fn extract_all(decoded_root: &Path) -> Vec<ExtractedString> {
        let mut all = Vec::new();

        // Smali directories (smali, smali_classes2, smali_classes3, ...)
        for entry in std::fs::read_dir(decoded_root)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
        {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with("smali") && entry.path().is_dir() {
                all.extend(Self::extract_from_smali(&entry.path()));
                all.truncate(Self::MAX_TOTAL_STRINGS);
                if all.len() >= Self::MAX_TOTAL_STRINGS {
                    break;
                }
            }
        }

        // Resources
        if all.len() < Self::MAX_TOTAL_STRINGS {
            all.extend(Self::extract_from_resources(decoded_root));
            all.truncate(Self::MAX_TOTAL_STRINGS);
        }

        // Sort: interesting categories first, then alphabetical
        all.sort_by(|a, b| {
            let cat_ord = |c: &StringCategory| match c {
                StringCategory::Secret => 0,
                StringCategory::ApiKey => 1,
                StringCategory::Url => 2,
                StringCategory::IpAddress => 3,
                StringCategory::Email => 4,
                StringCategory::Base64 => 5,
                StringCategory::FilePath => 6,
                StringCategory::PackageName => 7,
                StringCategory::Other => 8,
            };
            cat_ord(&a.category)
                .cmp(&cat_ord(&b.category))
                .then_with(|| a.value.cmp(&b.value))
        });

        all
    }

    fn parse_smali_file(path: &Path) -> Result<Vec<ExtractedString>> {
        if Self::file_too_large(path) {
            return Ok(Vec::new());
        }
        let content = std::fs::read_to_string(path)?;
        let re_const =
            Regex::new(r#"const-string(?:/jumbo)?\s+\S+,\s*"((?:\\.|[^"\\])*)""#).unwrap();
        let re_class = Regex::new(r"^\.class\s+.*?(L[^;]+;)").unwrap();
        let re_method = Regex::new(r"^\.method\s+.*?([^\s(]+)\(").unwrap();

        let mut results = Vec::new();
        let mut class_name = String::new();
        let mut method_name = String::new();

        for (line_num, line) in content.lines().enumerate() {
            let trimmed = line.trim();

            if let Some(caps) = re_class.captures(trimmed) {
                class_name = caps.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
                continue;
            }
            if let Some(caps) = re_method.captures(trimmed) {
                method_name = caps.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
                continue;
            }
            if trimmed.starts_with(".end method") {
                method_name.clear();
                continue;
            }

            if let Some(caps) = re_const.captures(trimmed) {
                let raw_value = caps.get(1).map(|m| m.as_str()).unwrap_or("");
                let value = Self::truncate_value(&Self::decode_smali_string(raw_value));
                if value.is_empty() {
                    continue;
                }
                let category = Self::categorize(&value);
                results.push(ExtractedString {
                    value,
                    category,
                    source_file: path.to_path_buf(),
                    line: line_num + 1,
                    context: StringContext::SmaliConstString {
                        class: class_name.clone(),
                        method: method_name.clone(),
                    },
                });
                if results.len() >= Self::MAX_STRINGS_PER_FILE {
                    break;
                }
            }
        }

        Ok(results)
    }

    fn file_too_large(path: &Path) -> bool {
        std::fs::metadata(path)
            .map(|meta| meta.len() > Self::MAX_STRING_SOURCE_FILE_BYTES)
            .unwrap_or(true)
    }

    fn truncate_value(value: &str) -> String {
        if value.chars().count() <= Self::MAX_STRING_VALUE_CHARS {
            return value.to_string();
        }
        let mut out: String = value.chars().take(Self::MAX_STRING_VALUE_CHARS).collect();
        out.push_str("...");
        out
    }

    fn decode_smali_string(raw: &str) -> String {
        let mut out = String::with_capacity(raw.len());
        let mut chars = raw.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch != '\\' {
                out.push(ch);
                continue;
            }

            match chars.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('u') => {
                    let mut hex = String::new();
                    for _ in 0..4 {
                        if let Some(next) = chars.next() {
                            hex.push(next);
                        }
                    }
                    if let Ok(value) = u32::from_str_radix(&hex, 16) {
                        if let Some(decoded) = char::from_u32(value) {
                            out.push(decoded);
                        }
                    }
                }
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        }

        out
    }

    fn decode_xml_string(raw: &str) -> String {
        raw.replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Auto-categorize a string based on pattern matching.
    pub fn categorize(value: &str) -> StringCategory {
        // URLs
        if value.starts_with("http://") || value.starts_with("https://") || value.starts_with("ftp://") {
            return StringCategory::Url;
        }

        // Email
        if Regex::new(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$")
            .unwrap()
            .is_match(value)
        {
            return StringCategory::Email;
        }

        // IP address
        if Regex::new(r"^\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}(:\d+)?$")
            .unwrap()
            .is_match(value)
        {
            return StringCategory::IpAddress;
        }

        // API key patterns (long alphanumeric, mixed case, often with hyphens/underscores)
        let lv = value.to_lowercase();
        if lv.contains("api_key")
            || lv.contains("apikey")
            || lv.contains("api-key")
            || lv.contains("access_token")
            || lv.contains("client_secret")
            || lv.contains("app_key")
        {
            return StringCategory::ApiKey;
        }

        // Secrets / credentials
        if lv.contains("password")
            || lv.contains("passwd")
            || lv.contains("secret")
            || lv.contains("private_key")
            || lv.contains("-----begin")
        {
            return StringCategory::Secret;
        }

        // Long hex or alphanumeric blobs that look like keys/tokens
        if value.len() >= 32
            && value.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return StringCategory::ApiKey;
        }

        // Base64 (at least 20 chars, matches base64 charset, ends with optional =)
        if value.len() >= 20
            && Regex::new(r"^[A-Za-z0-9+/]+={0,2}$")
                .unwrap()
                .is_match(value)
        {
            return StringCategory::Base64;
        }

        // Package names (com.x.y.z pattern)
        if Regex::new(r"^[a-z][a-z0-9]*(\.[a-z][a-z0-9]*){2,}$")
            .unwrap()
            .is_match(value)
        {
            return StringCategory::PackageName;
        }

        // File paths
        if value.starts_with('/')
            || value.contains("sdcard")
            || value.contains("/data/")
            || Regex::new(r"\.\w{1,4}$").unwrap().is_match(value) && value.contains('/')
        {
            return StringCategory::FilePath;
        }

        StringCategory::Other
    }
}

#[cfg(test)]
mod tests {
    use super::{StringCategory, StringContext, StringExtractor};

    use std::fs;
    use std::io::{Seek, Write};
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("reveng_strings_{}_{}_{}", std::process::id(), nonce, name))
    }

    #[test]
    fn parses_smali_const_strings_with_escapes() {
        let path = temp_path("sample.smali");
        fs::write(
            &path,
            r#"
.class public Lcom/example/Test;
.super Ljava/lang/Object;

.method public static demo()V
    const-string v0, "https://example.com/api"
    const-string v1, "hello \"quoted\" world"
    return-void
.end method
"#,
        )
        .unwrap();

        let results = StringExtractor::parse_smali_file(&path).unwrap();
        fs::remove_file(&path).ok();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].category, StringCategory::Url);
        assert_eq!(results[1].value, "hello \"quoted\" world");
        assert_eq!(
            results[1].context,
            StringContext::SmaliConstString {
                class: "Lcom/example/Test;".to_string(),
                method: "demo".to_string(),
            }
        );
    }

    #[test]
    fn extracts_multiline_resource_strings() {
        let decoded_root = temp_path("decoded");
        let values_dir = decoded_root.join("res").join("values");
        fs::create_dir_all(&values_dir).unwrap();
        let strings_path = values_dir.join("strings.xml");

        fs::write(
            &strings_path,
            r#"<resources>
    <string name="api_host">
        https://api.example.com/v1
    </string>
    <string name="welcome_text">Welcome &amp; enjoy</string>
</resources>"#,
        )
        .unwrap();

        let results = StringExtractor::extract_from_resources(&decoded_root);
        fs::remove_dir_all(&decoded_root).ok();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].category, StringCategory::Url);
        assert_eq!(results[0].value, "https://api.example.com/v1");
        assert_eq!(results[1].value, "Welcome & enjoy");
    }

    #[test]
    fn smali_string_extraction_caps_per_file_and_value_length() {
        let path = temp_path("many.smali");
        let long_value = "é".repeat(StringExtractor::MAX_STRING_VALUE_CHARS + 10);
        let body = (0..(StringExtractor::MAX_STRINGS_PER_FILE + 10))
            .map(|idx| format!("    const-string v{}, \"{}\"", idx % 10, long_value))
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(
            &path,
            format!(
                ".class public Lx/Test;\n.method public static demo()V\n{}\n.end method\n",
                body
            ),
        )
        .unwrap();

        let results = StringExtractor::parse_smali_file(&path).unwrap();
        fs::remove_file(&path).ok();

        assert_eq!(results.len(), StringExtractor::MAX_STRINGS_PER_FILE);
        assert!(results[0].value.ends_with("..."));
        assert!(results[0].value.is_char_boundary(results[0].value.len()));
    }

    #[test]
    fn string_extraction_skips_oversized_source_files() {
        let path = temp_path("huge.smali");
        let mut file = fs::File::create(&path).unwrap();
        file.seek(std::io::SeekFrom::Start(
            StringExtractor::MAX_STRING_SOURCE_FILE_BYTES + 1,
        ))
        .unwrap();
        file.write_all(b"const-string v0, \"secret\"").unwrap();
        drop(file);

        let results = StringExtractor::parse_smali_file(&path).unwrap();
        fs::remove_file(&path).ok();

        assert!(results.is_empty());
    }
}
