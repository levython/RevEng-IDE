//! Smali validation engine — detects syntax errors in Smali code.


#[derive(Debug, Clone)]
pub struct ValidationError {
    pub line: usize,
    pub message: String,
    pub severity: ValidationSeverity,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ValidationSeverity {
    Error,
    Warning,
}

pub struct SmaliValidator;

impl SmaliValidator {
    /// Performs a quick scan of Smali code for common syntax errors.
    pub fn validate(content: &str) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        let mut in_method = false;
        let mut method_line = 0;
        let mut method_has_register_decl = false;

        for (i, line) in content.lines().enumerate() {
            let line_num = i + 1;
            let trimmed = line.trim();

            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            if trimmed.starts_with(".method") {
                if in_method {
                    errors.push(ValidationError {
                        line: line_num,
                        message: "Nested methods are not allowed. Missing '.end method'?".into(),
                        severity: ValidationSeverity::Error,
                    });
                }
                in_method = true;
                method_line = line_num;
                method_has_register_decl = false;
            } else if trimmed.starts_with(".end method") {
                if !in_method {
                    errors.push(ValidationError {
                        line: line_num,
                        message: "'.end method' without a corresponding '.method'".into(),
                        severity: ValidationSeverity::Error,
                    });
                } else if !method_has_register_decl {
                    errors.push(ValidationError {
                        line: method_line,
                        message: "Method has no '.locals' or '.registers' directive".into(),
                        severity: ValidationSeverity::Warning,
                    });
                }
                in_method = false;
            } else if in_method {
                if trimmed.starts_with(".locals") || trimmed.starts_with(".registers") {
                    method_has_register_decl = true;
                }

                // Check for common instruction errors
                if trimmed.contains("invoke-") && !trimmed.contains('{') {
                    errors.push(ValidationError {
                        line: line_num,
                        message: "Invoke instruction missing register list {} ".into(),
                        severity: ValidationSeverity::Error,
                    });
                }
                
                // Check for invalid registers (e.g. vX where X is too large or non-numeric)
                // This is a simplified check
            }
        }

        if in_method {
            errors.push(ValidationError {
                line: method_line,
                message: "Method has no corresponding '.end method'".into(),
                severity: ValidationSeverity::Error,
            });
        }

        errors
    }
}

#[cfg(test)]
mod tests {
    use super::{SmaliValidator, ValidationSeverity};

    #[test]
    fn reports_missing_register_directive_as_warning() {
        let issues = SmaliValidator::validate(
            ".class public Lx/Y;\n\
             .method public foo()V\n\
             return-void\n\
             .end method\n",
        );

        assert!(issues.iter().any(|issue| {
            issue.line == 2
                && issue.severity == ValidationSeverity::Warning
                && issue.message.contains(".locals")
        }));
    }

    #[test]
    fn reports_invoke_without_register_list_as_error() {
        let issues = SmaliValidator::validate(
            ".class public Lx/Y;\n\
             .method public foo()V\n\
             .locals 1\n\
             invoke-virtual Lx/Y;->bar()V\n\
             return-void\n\
             .end method\n",
        );

        assert!(issues.iter().any(|issue| {
            issue.line == 4
                && issue.severity == ValidationSeverity::Error
                && issue.message.contains("missing register list")
        }));
    }
}
