//! Symbol Outline Engine — extracts classes, methods, and fields from source files.

use regex::Regex;

#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub line: usize,
    pub kind: SymbolKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SymbolKind {
    Class,
    Method,
    Field,
    Annotation,
}

impl SymbolKind {
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Class => "🗄",
            Self::Method => "Ⓜ",
            Self::Field => "ⓕ",
            Self::Annotation => "ⓐ",
        }
    }
}

pub struct SymbolParser;

impl SymbolParser {
    /// Parses symbols for the outline view based on file language.
    pub fn parse(content: &str, lang: &crate::app::FileLanguage) -> Vec<Symbol> {
        match lang {
            crate::app::FileLanguage::Java => Self::parse_java(content),
            crate::app::FileLanguage::Smali => Self::parse_smali(content),
            _ => Vec::new(),
        }
    }

    fn parse_java(content: &str) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        let re_class = Regex::new(r"(class|interface|enum)\s+([a-zA-Z0-9_]+)").unwrap();
        let re_method = Regex::new(r"(public|private|protected|static).*\s+([a-zA-Z0-9_]+)\s*\(").unwrap();

        for (i, line) in content.lines().enumerate() {
            let line_num = i + 1;
            let trimmed = line.trim();
            if trimmed.starts_with('@') {
                symbols.push(Symbol {
                    name: trimmed.trim_start_matches('@').split('(').next().unwrap_or("annotation").to_string(),
                    line: line_num,
                    kind: SymbolKind::Annotation,
                });
            } else if let Some(cap) = re_class.captures(line) {
                symbols.push(Symbol {
                    name: cap[2].to_string(),
                    line: line_num,
                    kind: SymbolKind::Class,
                });
            } else if let Some(cap) = re_method.captures(line) {
                symbols.push(Symbol {
                    name: cap[2].to_string(),
                    line: line_num,
                    kind: SymbolKind::Method,
                });
            }
        }
        symbols
    }

    fn parse_smali(content: &str) -> Vec<Symbol> {
        let mut symbols = Vec::new();
        for (i, line) in content.lines().enumerate() {
            let line_num = i + 1;
            let trimmed = line.trim();
            if trimmed.starts_with(".class") {
                let name = trimmed.split(' ').next_back().unwrap_or("").replace("L", "").replace(";", "");
                symbols.push(Symbol { name, line: line_num, kind: SymbolKind::Class });
            } else if trimmed.starts_with(".annotation") {
                let name = trimmed.split(' ').next_back().unwrap_or("annotation").replace("L", "").replace(";", "");
                symbols.push(Symbol { name, line: line_num, kind: SymbolKind::Annotation });
            } else if trimmed.starts_with(".method") {
                let name = trimmed.split(' ').find(|s| s.contains('(')).unwrap_or("method").split('(').next().unwrap_or("").to_string();
                symbols.push(Symbol { name, line: line_num, kind: SymbolKind::Method });
            } else if trimmed.starts_with(".field") {
                let name = trimmed.split(':').next().unwrap_or("").split(' ').next_back().unwrap_or("").to_string();
                symbols.push(Symbol { name, line: line_num, kind: SymbolKind::Field });
            }
        }
        symbols
    }
}
