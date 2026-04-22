use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenizedLine {
    pub file: PathBuf,
    pub line_number: usize,
    pub raw: String,
    pub tokens: Vec<Token>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub lexeme: String,
    pub column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Directive,
    Identifier,
    Number,
    StringLiteral,
    Symbol,
}

pub fn tokenize_line(path: &Path, line_number: usize, raw: &str) -> TokenizedLine {
    let mut tokens = Vec::new();
    let chars = raw.char_indices().collect::<Vec<_>>();
    let mut idx = 0usize;

    while idx < chars.len() {
        let (byte_idx, ch) = chars[idx];
        if ch.is_whitespace() {
            idx += 1;
            continue;
        }

        if ch == '#' {
            break;
        }

        if ch == '@' {
            let start = byte_idx;
            idx += 1;
            while idx < chars.len() {
                let (_, next) = chars[idx];
                if next.is_ascii_alphanumeric() || next == '_' || next == '-' {
                    idx += 1;
                    continue;
                }
                break;
            }
            let end = if idx < chars.len() {
                chars[idx].0
            } else {
                raw.len()
            };
            tokens.push(Token {
                kind: TokenKind::Directive,
                lexeme: raw[start..end].to_string(),
                column: start + 1,
            });
            continue;
        }

        if ch == '"' {
            let start = byte_idx;
            idx += 1;
            while idx < chars.len() {
                let (_, next) = chars[idx];
                idx += 1;
                if next == '"' {
                    break;
                }
            }
            let end = if idx < chars.len() {
                chars[idx].0
            } else {
                raw.len()
            };
            tokens.push(Token {
                kind: TokenKind::StringLiteral,
                lexeme: raw[start..end].to_string(),
                column: start + 1,
            });
            continue;
        }

        if ch.is_ascii_alphabetic() || ch == '_' || ch == '$' || ch == ':' {
            let start = byte_idx;
            idx += 1;
            while idx < chars.len() {
                let (_, next) = chars[idx];
                if next.is_ascii_alphanumeric()
                    || matches!(next, '_' | '$' | ':' | '-' | '^' | '.' | '[' | ']')
                {
                    idx += 1;
                    continue;
                }
                break;
            }
            let end = if idx < chars.len() {
                chars[idx].0
            } else {
                raw.len()
            };
            tokens.push(Token {
                kind: TokenKind::Identifier,
                lexeme: raw[start..end].to_string(),
                column: start + 1,
            });
            continue;
        }

        if ch.is_ascii_digit() {
            let start = byte_idx;
            idx += 1;
            while idx < chars.len() {
                let (_, next) = chars[idx];
                if next.is_ascii_alphanumeric() || next == 'x' {
                    idx += 1;
                    continue;
                }
                break;
            }
            let end = if idx < chars.len() {
                chars[idx].0
            } else {
                raw.len()
            };
            tokens.push(Token {
                kind: TokenKind::Number,
                lexeme: raw[start..end].to_string(),
                column: start + 1,
            });
            continue;
        }

        tokens.push(Token {
            kind: TokenKind::Symbol,
            lexeme: ch.to_string(),
            column: byte_idx + 1,
        });
        idx += 1;
    }

    TokenizedLine {
        file: path.to_path_buf(),
        line_number,
        raw: raw.to_string(),
        tokens,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_directive_and_include_path() {
        let line = tokenize_line(
            Path::new("/tmp/test.slaspec"),
            1,
            "@include \"x86.slaspec\"",
        );
        assert_eq!(line.tokens.len(), 2);
        assert_eq!(line.tokens[0].kind, TokenKind::Directive);
        assert_eq!(line.tokens[0].lexeme, "@include");
        assert_eq!(line.tokens[1].kind, TokenKind::StringLiteral);
        assert_eq!(line.tokens[1].lexeme, "\"x86.slaspec\"");
    }

    #[test]
    fn tokenize_constructor_signature() {
        let line = tokenize_line(
            Path::new("/tmp/test.sinc"),
            9,
            ":ADC^lockx m64,simm32 is $(LONGMODE_ON) & byte=0x81 {",
        );
        assert!(line
            .tokens
            .iter()
            .any(|token| token.lexeme == ":ADC^lockx"));
        assert!(line
            .tokens
            .iter()
            .any(|token| token.lexeme == "m64"));
    }
}
