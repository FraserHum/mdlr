use mdlr_cpd::{FileTokens, NORMALIZED_ID, NORMALIZED_LIT, Token};
use std::path::PathBuf;

/// JavaScript/TypeScript keywords (ES2024 + TS keywords).
const JS_KEYWORDS: &[&str] = &[
    "abstract",
    "as",
    "async",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "declare",
    "default",
    "delete",
    "do",
    "else",
    "enum",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "from",
    "function",
    "get",
    "if",
    "implements",
    "import",
    "in",
    "infer",
    "instanceof",
    "interface",
    "is",
    "keyof",
    "let",
    "module",
    "namespace",
    "new",
    "null",
    "of",
    "override",
    "private",
    "protected",
    "public",
    "readonly",
    "return",
    "satisfies",
    "set",
    "static",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "type",
    "typeof",
    "undefined",
    "unique",
    "var",
    "void",
    "while",
    "with",
    "yield",
];

/// Tokenize a JS/TS source file for CPD analysis.
///
/// Uses a simple scanner that:
/// - Recognizes keywords, identifiers, string/number/regex literals, operators, punctuation
/// - Strips comments (line and block) and whitespace
/// - Normalizes identifiers to $ID and literals to $LIT
/// - Respects `mdlr:ignore-start` / `mdlr:ignore-end` markers
pub fn tokenize_ts(
    source: &str,
    source_path: &str,
    generation_id: u64,
) -> FileTokens {
    let chars: Vec<(usize, char)> = source.char_indices().collect();
    let n = chars.len();
    let source_len = source.len();
    let mut i = 0;
    let mut line: u32 = 1;
    let mut col: u16 = 0;
    let mut ignoring = false;
    let mut tokens = Vec::new();

    let byte_at = |idx: usize| -> usize {
        if idx < n { chars[idx].0 } else { source_len }
    };

    while i < n {
        let c = chars[i].1;

        // Newline
        if c == '\n' {
            i += 1;
            line += 1;
            col = 0;
            continue;
        }

        // Whitespace
        if c == ' ' || c == '\t' || c == '\r' {
            i += 1;
            col += 1;
            continue;
        }

        // Line comment
        if c == '/' && nth_char(&chars, i + 1) == Some('/') {
            let start = chars[i].0;
            i += 2;
            while i < n && chars[i].1 != '\n' {
                i += 1;
            }
            let comment = &source[start..byte_at(i)];
            if comment.contains("mdlr:ignore-start") {
                ignoring = true;
            } else if comment.contains("mdlr:ignore-end") {
                ignoring = false;
            }
            continue;
        }

        // Block comment
        if c == '/' && nth_char(&chars, i + 1) == Some('*') {
            let start = chars[i].0;
            i += 2;
            while i + 1 < n && !(chars[i].1 == '*' && chars[i + 1].1 == '/') {
                if chars[i].1 == '\n' {
                    line += 1;
                    col = 0;
                } else {
                    col += 1;
                }
                i += 1;
            }
            if i + 1 < n {
                i += 2; // skip */
                col += 2;
            }
            let comment = &source[start..byte_at(i)];
            if comment.contains("mdlr:ignore-start") {
                ignoring = true;
            } else if comment.contains("mdlr:ignore-end") {
                ignoring = false;
            }
            continue;
        }

        // Skip tokens in ignored regions (but still track newlines for line counting)
        if ignoring {
            if c == '\n' {
                line += 1;
                col = 0;
            } else {
                col += 1;
            }
            i += 1;
            continue;
        }

        let token_line = line;
        let token_col = col;

        // String literal (single or double quoted)
        if c == '\'' || c == '"' {
            let quote = c;
            i += 1;
            col += 1;
            while i < n && chars[i].1 != quote {
                if chars[i].1 == '\\' && i + 1 < n {
                    i += 2;
                    col += 2;
                } else {
                    if chars[i].1 == '\n' {
                        line += 1;
                        col = 0;
                    } else {
                        col += 1;
                    }
                    i += 1;
                }
            }
            if i < n {
                i += 1; // skip closing quote
                col += 1;
            }
            tokens.push(Token {
                value: NORMALIZED_LIT.to_string(),
                line: token_line,
                col: token_col,
            });
            continue;
        }

        // Template literal
        if c == '`' {
            i += 1;
            col += 1;
            let mut depth = 0;
            while i < n {
                let cc = chars[i].1;
                if cc == '\\' && i + 1 < n {
                    i += 2;
                    col += 2;
                } else if cc == '$' && i + 1 < n && chars[i + 1].1 == '{' {
                    depth += 1;
                    i += 2;
                    col += 2;
                } else if cc == '}' && depth > 0 {
                    depth -= 1;
                    i += 1;
                    col += 1;
                } else if cc == '`' && depth == 0 {
                    i += 1;
                    col += 1;
                    break;
                } else {
                    if cc == '\n' {
                        line += 1;
                        col = 0;
                    } else {
                        col += 1;
                    }
                    i += 1;
                }
            }
            tokens.push(Token {
                value: NORMALIZED_LIT.to_string(),
                line: token_line,
                col: token_col,
            });
            continue;
        }

        // Number literal
        if c.is_ascii_digit()
            || (c == '.'
                && nth_char(&chars, i + 1)
                    .map_or(false, |p| p.is_ascii_digit()))
        {
            while i < n {
                let cc = chars[i].1;
                if cc.is_ascii_alphanumeric() || cc == '.' || cc == '_' {
                    i += 1;
                    col += 1;
                } else {
                    break;
                }
            }
            tokens.push(Token {
                value: NORMALIZED_LIT.to_string(),
                line: token_line,
                col: token_col,
            });
            continue;
        }

        // Identifier or keyword (Unicode-aware: supports `π`, etc.)
        if is_ident_start(c) {
            let start = chars[i].0;
            i += 1;
            col += 1;
            while i < n && is_ident_continue(chars[i].1) {
                i += 1;
                col += 1;
            }
            let word = &source[start..byte_at(i)];
            let value = if JS_KEYWORDS.contains(&word) {
                word.to_string()
            } else {
                NORMALIZED_ID.to_string()
            };
            tokens.push(Token { value, line: token_line, col: token_col });
            continue;
        }

        // Multi-character operators (all ASCII)
        if i + 2 < n {
            let three = (chars[i].1, chars[i + 1].1, chars[i + 2].1);
            let op3 = match three {
                ('=', '=', '=') => Some("==="),
                ('!', '=', '=') => Some("!=="),
                ('>', '>', '>') => Some(">>>"),
                ('*', '*', '=') => Some("**="),
                ('&', '&', '=') => Some("&&="),
                ('|', '|', '=') => Some("||="),
                ('?', '?', '=') => Some("??="),
                ('.', '.', '.') => Some("..."),
                ('<', '<', '=') => Some("<<="),
                ('>', '>', '=') => Some(">>="),
                _ => None,
            };
            if let Some(op) = op3 {
                tokens.push(Token {
                    value: op.to_string(),
                    line: token_line,
                    col: token_col,
                });
                i += 3;
                col += 3;
                continue;
            }
        }
        if i + 1 < n {
            let two = (chars[i].1, chars[i + 1].1);
            let op2 = match two {
                ('=', '=') => Some("=="),
                ('!', '=') => Some("!="),
                ('<', '=') => Some("<="),
                ('>', '=') => Some(">="),
                ('&', '&') => Some("&&"),
                ('|', '|') => Some("||"),
                ('+', '+') => Some("++"),
                ('-', '-') => Some("--"),
                ('+', '=') => Some("+="),
                ('-', '=') => Some("-="),
                ('*', '=') => Some("*="),
                ('/', '=') => Some("/="),
                ('%', '=') => Some("%="),
                ('=', '>') => Some("=>"),
                ('*', '*') => Some("**"),
                ('?', '?') => Some("??"),
                ('?', '.') => Some("?."),
                ('<', '<') => Some("<<"),
                ('>', '>') => Some(">>"),
                ('&', '=') => Some("&="),
                ('|', '=') => Some("|="),
                ('^', '=') => Some("^="),
                _ => None,
            };
            if let Some(op) = op2 {
                tokens.push(Token {
                    value: op.to_string(),
                    line: token_line,
                    col: token_col,
                });
                i += 2;
                col += 2;
                continue;
            }
        }

        // Single-character tokens (operators, punctuation, stray Unicode)
        tokens.push(Token {
            value: c.to_string(),
            line: token_line,
            col: token_col,
        });
        i += 1;
        col += 1;
    }

    FileTokens {
        source_path: PathBuf::from(source_path),
        tokens,
        cached_at: generation_id,
    }
}

fn nth_char(chars: &[(usize, char)], idx: usize) -> Option<char> {
    chars.get(idx).map(|&(_, c)| c)
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic()
        || c == '_'
        || c == '$'
        || (!c.is_ascii() && c.is_alphabetic())
}

fn is_ident_continue(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || c == '_'
        || c == '$'
        || (!c.is_ascii() && (c.is_alphabetic() || c.is_numeric()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_js() {
        let source = r#"function foo(x) {
    return x + 1;
}"#;
        let ft = tokenize_ts(source, "test.js", 1);
        let values: Vec<&str> =
            ft.tokens.iter().map(|t| t.value.as_str()).collect();
        assert_eq!(
            values,
            vec![
                "function", "$ID", "(", "$ID", ")", "{", "return", "$ID", "+",
                "$LIT", ";", "}"
            ]
        );
    }

    #[test]
    fn test_comments_stripped() {
        let source = r#"// comment
const x = 5; /* block comment */
const y = 10;"#;
        let ft = tokenize_ts(source, "test.ts", 1);
        let values: Vec<&str> =
            ft.tokens.iter().map(|t| t.value.as_str()).collect();
        assert_eq!(
            values,
            vec![
                "const", "$ID", "=", "$LIT", ";", "const", "$ID", "=", "$LIT",
                ";"
            ]
        );
    }

    #[test]
    fn test_string_literals() {
        let source = r#"const a = "hello"; const b = 'world';"#;
        let ft = tokenize_ts(source, "test.ts", 1);
        let values: Vec<&str> =
            ft.tokens.iter().map(|t| t.value.as_str()).collect();
        assert_eq!(
            values,
            vec![
                "const", "$ID", "=", "$LIT", ";", "const", "$ID", "=", "$LIT",
                ";"
            ]
        );
    }

    #[test]
    fn test_template_literal() {
        let source = "const s = `hello ${name}`;";
        let ft = tokenize_ts(source, "test.ts", 1);
        let values: Vec<&str> =
            ft.tokens.iter().map(|t| t.value.as_str()).collect();
        assert_eq!(values, vec!["const", "$ID", "=", "$LIT", ";"]);
    }

    #[test]
    fn test_ignore_markers() {
        let source = r#"const a = 1;
// mdlr:ignore-start
const ignored = 2;
// mdlr:ignore-end
const b = 3;"#;
        let ft = tokenize_ts(source, "test.ts", 1);
        let values: Vec<&str> =
            ft.tokens.iter().map(|t| t.value.as_str()).collect();
        assert_eq!(
            values,
            vec![
                "const", "$ID", "=", "$LIT", ";", "const", "$ID", "=", "$LIT",
                ";"
            ]
        );
    }

    #[test]
    fn test_arrow_function() {
        let source = "const f = (x) => x * 2;";
        let ft = tokenize_ts(source, "test.ts", 1);
        let values: Vec<&str> =
            ft.tokens.iter().map(|t| t.value.as_str()).collect();
        assert_eq!(
            values,
            vec![
                "const", "$ID", "=", "(", "$ID", ")", "=>", "$ID", "*",
                "$LIT", ";"
            ]
        );
    }

    #[test]
    fn test_typescript_keywords() {
        let source = "interface Foo { readonly bar: string; }";
        let ft = tokenize_ts(source, "test.ts", 1);
        let values: Vec<&str> =
            ft.tokens.iter().map(|t| t.value.as_str()).collect();
        assert_eq!(
            values,
            vec![
                "interface",
                "$ID",
                "{",
                "readonly",
                "$ID",
                ":",
                "$ID",
                ";",
                "}"
            ]
        );
        // Note: "string" is not in our keywords list, so it's $ID. This is fine
        // for CPD since we normalize identifiers anyway.
    }

    #[test]
    fn test_non_ascii_does_not_panic() {
        // Smart quotes, emoji, currency, em-dash, JSX text — all things that
        // previously triggered "byte index N is not a char boundary" panics.
        let source = r#"
            // Comment with emoji ✅ and smart quote ’ and €
            const a = "hello — world";
            const jsx = <div>Welcome € friend</div>;
            const π = 3.14;
            // mdlr:ignore-start
            const ignored = "🟢";
            // mdlr:ignore-end
        "#;
        let ft = tokenize_ts(source, "test.tsx", 1);
        // Should produce some tokens and not panic; exact content not asserted
        // because stray Unicode in JSX is best-effort.
        assert!(!ft.tokens.is_empty());
    }

    #[test]
    fn test_unicode_identifier() {
        let source = "const π = 3.14;";
        let ft = tokenize_ts(source, "test.ts", 1);
        let values: Vec<&str> =
            ft.tokens.iter().map(|t| t.value.as_str()).collect();
        assert_eq!(values, vec!["const", "$ID", "=", "$LIT", ";"]);
    }
}
