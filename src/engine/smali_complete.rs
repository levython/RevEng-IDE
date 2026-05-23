//! Smali autocomplete — opcode, class, and method name suggestions.

use crate::engine::smali_opcodes::SmaliOpcodes;

/// A single autocomplete suggestion.
#[derive(Clone, Debug)]
pub struct Suggestion {
    pub text: String,
    pub detail: String,
    pub kind: SuggestionKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SuggestionKind {
    Opcode,
    ClassName,
    MethodName,
    Register,
}

impl SuggestionKind {
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Opcode     => "⚙",
            Self::ClassName  => "C",
            Self::MethodName => "M",
            Self::Register   => "R",
        }
    }
}

pub struct SmaliCompleter;

impl SmaliCompleter {
    /// Return up to `limit` suggestions for `query` in a `.smali` file.
    ///
    /// `class_names` — class names from the xref DB (may be empty).
    pub fn suggest(query: &str, class_names: &[String], method_names: &[String], limit: usize) -> Vec<Suggestion> {
        let query_lower = query.to_lowercase();
        let mut results = Vec::new();

        // 1. Opcode completions
        for info in SmaliOpcodes::prefix_match(&query_lower) {
            results.push(Suggestion {
                text: info.mnemonic.to_string(),
                detail: info.description.to_string(),
                kind: SuggestionKind::Opcode,
            });
            if results.len() >= limit { return results; }
        }

        // 2. Register names (v0..v15, p0..p4)
        if query_lower.starts_with('v') || query_lower.starts_with('p') {
            let prefix = if query_lower.starts_with('v') { 'v' } else { 'p' };
            let max = if prefix == 'v' { 16 } else { 5 };
            for n in 0..max {
                let reg = format!("{}{}", prefix, n);
                if reg.starts_with(&query_lower) {
                    results.push(Suggestion {
                        text: reg.clone(),
                        detail: format!("register {}", reg),
                        kind: SuggestionKind::Register,
                    });
                    if results.len() >= limit { return results; }
                }
            }
        }

        // 3. Class name completions
        for name in class_names {
            if name.to_lowercase().contains(&query_lower) {
                results.push(Suggestion {
                    text: name.clone(),
                    detail: "class".to_string(),
                    kind: SuggestionKind::ClassName,
                });
                if results.len() >= limit { return results; }
            }
        }

        // 4. Method name completions from the xref DB
        for name in method_names {
            if name.to_lowercase().contains(&query_lower) {
                results.push(Suggestion {
                    text: name.clone(),
                    detail: "method".to_string(),
                    kind: SuggestionKind::MethodName,
                });
                if results.len() >= limit { return results; }
            }
        }

        results
    }

    /// Extract the current word from the cursor position in the text.
    ///
    /// Returns `(word, word_start_byte_offset)`.
    pub fn current_word(text: &str, cursor_char_idx: usize) -> (String, usize) {
        let chars: Vec<char> = text.chars().collect();
        let cursor = cursor_char_idx.min(chars.len());

        // Word characters for smali: alphanumeric, `-`, `/`, `_`, `$`, `.`
        let is_word = |c: char| c.is_alphanumeric() || "-/_$.<>".contains(c);

        let mut start = cursor;
        while start > 0 && is_word(chars[start - 1]) {
            start -= 1;
        }

        let word: String = chars[start..cursor].iter().collect();
        // Convert char start to byte offset
        let byte_start = text.char_indices().nth(start).map(|(b, _)| b).unwrap_or(0);
        (word, byte_start)
    }
}
