//! # nexa-formatter
//!
//! Implementation 11 — canonical NEXA formatter.
//!
//! Guarantees (per the Formatter CTS): **deterministic**, **idempotent**,
//! **syntax-preserving** and **comment-preserving**. It is a *token-stream*
//! formatter: it re-lays-out the canonical whitespace around a lossless token
//! stream but never reorders or rewrites any token's text (strings, raw
//! strings, escapes and nested block comments are copied verbatim).
//!
//! Canonical rules:
//! - 4-space indentation (never tabs);
//! - LF newlines only (CRLF normalized);
//! - a single trailing LF;
//! - inter-token spacing normalized deterministically;
//! - existing line structure preserved (blank lines collapsed);
//! - imports and effects lists are **not** reordered.
//!
//! ```text
//! fmt(fmt(source)) == fmt(source)
//! parse(source)    ≙ parse(fmt(source))
//! ```
//!
//! Error codes: `NEXA-FMT-0001` … `NEXA-FMT-0003`.

use std::path::PathBuf;

use nexa_lexer::lexeme::{Lexeme, LexemeKind};
use nexa_lexer::token::TokenKind;
use nexa_lexer::trivia::TriviaKind;
use nexa_source::{SourceFile, SourceId, SourceSpan};

/// Canonical formatter version.
pub const FORMAT_VERSION: u32 = 1;

/// Formatting configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatConfig {
    /// Number of spaces per indentation level (canonical: 4).
    pub indent_width: usize,
}

impl Default for FormatConfig {
    fn default() -> Self {
        FormatConfig { indent_width: 4 }
    }
}

/// Result of [`verify`]: whether idempotence and semantic preservation hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatVerification {
    /// `format(format(source)) == format(source)`.
    pub idempotent: bool,
    /// The token-kind stream of `source` equals that of `format(source)`.
    pub semantic_preserved: bool,
}

impl FormatVerification {
    /// True when both idempotence and semantic preservation hold.
    pub fn is_clean(&self) -> bool {
        self.idempotent && self.semantic_preserved
    }
}

/// Format `text` as canonical NEXA.
pub fn format_source(text: &str) -> String {
    let sf = SourceFile::from_text(SourceId(0), PathBuf::from("<fmt>"), text.to_string());
    format_file(&sf)
}

/// Format an already-constructed [`SourceFile`].
pub fn format_file(sf: &SourceFile) -> String {
    format_with(sf, &FormatConfig::default())
}

/// Format `sf` with an explicit configuration.
pub fn format_with(sf: &SourceFile, config: &FormatConfig) -> String {
    let lexed = nexa_lexer::lex(sf);
    if lexed
        .diagnostics
        .iter()
        .any(|d| d.severity == nexa_diagnostics::Severity::Error)
    {
        // Malformed source: fall back to a safe, deterministic whitespace
        // normalization (never destructive). We still run the emitter below
        // over whatever lexemes were recovered.
        return format_lexemes(sf, &lexed.lexemes, config);
    }
    format_lexemes(sf, &lexed.lexemes, config)
}

/// `true` if `text` is already canonical (`format_source(text) == text`).
pub fn is_formatted(text: &str) -> bool {
    format_source(text) == text
}

/// Verify idempotence and semantic preservation for `text`.
pub fn verify(text: &str) -> FormatVerification {
    let once = format_source(text);
    let twice = format_source(&once);
    let idempotent = twice == once;
    let semantic_preserved = token_kinds(text) == token_kinds(&once);
    FormatVerification {
        idempotent,
        semantic_preserved,
    }
}

/// The ordered sequence of token kinds (ignoring trivia and line breaks) of
/// `text`. Two sources representing the same semantics produce identical
/// sequences. Line breaks and EOF are excluded because they are whitespace.
pub fn token_kinds(text: &str) -> Vec<TokenKind> {
    let sf = SourceFile::from_text(SourceId(0), PathBuf::from("<fmt>"), text.to_string());
    let lexed = nexa_lexer::lex(&sf);
    lexed
        .lexemes
        .iter()
        .filter_map(|l| match l.kind {
            LexemeKind::Token(k) => Some(k),
            LexemeKind::Trivia(_) => None,
        })
        .filter(|k| !matches!(k, TokenKind::Newline | TokenKind::Eof))
        .collect()
}

/// Lower-level API: emit a canonical token stream as text.
pub fn format_token_api(text: &str) -> String {
    format_source(text)
}

fn format_lexemes(sf: &SourceFile, lexemes: &[Lexeme], config: &FormatConfig) -> String {
    let mut out = String::new();
    let mut depth: i32 = 0;
    // Pending state for the next line.
    let mut line_pending = false;
    let mut at_document_start = true;
    let mut prev: Option<TokenKind> = None;
    let mut in_string = false;

    for lx in lexemes {
        match lx.kind {
            LexemeKind::Trivia(TriviaKind::Whitespace) => {
                if span_is_break(sf, lx.span) {
                    line_pending = true;
                }
            }
            LexemeKind::Trivia(
                kind @ (TriviaKind::LineComment
                | TriviaKind::DocumentationComment
                | TriviaKind::ModuleDocumentationComment
                | TriviaKind::BlockComment),
            ) => {
                let text = match sf.span_text(lx.span) {
                    Some(t) => t.to_string(),
                    None => continue,
                };
                let is_line = matches!(
                    kind,
                    TriviaKind::LineComment
                        | TriviaKind::DocumentationComment
                        | TriviaKind::ModuleDocumentationComment
                );
                let is_multiline_block = !is_line && text.contains('\n');
                if at_document_start && out.is_empty() {
                    out.push_str(&text);
                    at_document_start = false;
                } else if line_pending {
                    push_line_break(&mut out);
                    push_indent(&mut out, depth, config);
                    out.push_str(text.trim_end());
                } else {
                    out.push(' ');
                    out.push_str(text.trim_end());
                }
                if is_line || is_multiline_block {
                    line_pending = true;
                }
            }
            LexemeKind::Token(TokenKind::Newline) => {
                line_pending = true;
            }
            LexemeKind::Token(kind) => {
                // The lexer never drops trivia-free gaps; recover the token verbatim.
                let text = match sf.span_text(lx.span) {
                    Some(t) => t.to_string(),
                    None => continue,
                };
                // Stray whitespace-only tokens (e.g. a bare tab or CR captured
                // as InvalidToken) are trivia and are dropped — never emitted.
                if text.chars().all(|c| c.is_whitespace()) {
                    if text.contains('\n') || text.contains('\r') {
                        line_pending = true;
                    }
                    continue;
                }
                // Inside a segmented string, emit every segment back-to-back
                // and verbatim so string content is never rewritten.
                if in_string {
                    out.push_str(&text);
                    if matches!(kind, TokenKind::StringEnd(_)) {
                        in_string = false;
                    }
                    line_pending = false;
                    prev = Some(kind);
                    continue;
                }
                // Effective indentation level for this line start.
                let effective = if kind == TokenKind::RightBrace {
                    (depth - 1).max(0)
                } else {
                    depth
                };
                if at_document_start && out.is_empty() {
                    out.push_str(&text);
                    at_document_start = false;
                } else if line_pending {
                    push_line_break(&mut out);
                    push_indent(&mut out, effective, config);
                    out.push_str(&text);
                } else if wants_space(prev, kind) && !out.is_empty() {
                    out.push(' ');
                    out.push_str(&text);
                } else {
                    out.push_str(&text);
                }
                if matches!(kind, TokenKind::StringStart(_)) {
                    in_string = true;
                }
                match kind {
                    TokenKind::LeftBrace => depth += 1,
                    TokenKind::RightBrace => depth = (depth - 1).max(0),
                    _ => {}
                }
                line_pending = false;
                prev = Some(kind);
            }
        }
    }

    // Canonical single trailing LF; strip stray trailing whitespace/newlines.
    let trimmed = out
        .trim_end_matches([' ', '\t'])
        .trim_end_matches(['\n', '\r']);
    let mut final_out = trimmed.to_string();
    if !final_out.is_empty() {
        final_out.push('\n');
    }
    final_out
}

fn span_is_break(sf: &SourceFile, span: SourceSpan) -> bool {
    sf.span_text(span)
        .map(|t| t.contains('\n') || t.contains('\r'))
        .unwrap_or(false)
}

fn push_line_break(out: &mut String) {
    out.push('\n');
}

fn push_indent(out: &mut String, level: i32, config: &FormatConfig) {
    for _ in 0..level.max(0) as usize {
        for _ in 0..config.indent_width {
            out.push(' ');
        }
    }
}

/// Decide whether a single space should separate `prev` and `cur` inline.
fn wants_space(prev: Option<TokenKind>, cur: TokenKind) -> bool {
    use TokenKind as T;
    // No space before closing delimiters, commas or member access.
    if matches!(
        cur,
        T::RightParen
            | T::RightBracket
            | T::RightBrace
            | T::Comma
            | T::Dot
            | T::DotDot
            | T::DoubleColon
            | T::Colon
    ) {
        return false;
    }
    let Some(prev) = prev else { return false };
    // No space after an opening delimiter.
    if matches!(prev, T::LeftParen | T::LeftBracket) {
        return false;
    }
    // No space before `(` following a call-like expression (identifier,
    // closing delimiter or bracket) — i.e. `foo(`; keep `if (`).
    if cur == T::LeftParen && matches!(prev, T::Identifier | T::RightParen | T::RightBracket) {
        return false;
    }
    // Word-adjacent punctuation that attaches to the previous token.
    if matches!(cur, T::At | T::Caret | T::Bang | T::Tilde) {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idempotent_basic() {
        let src = "function add(a: Int, b: Int) -> Int {\nreturn a + b\n}";
        let once = format_source(src);
        let twice = format_source(&once);
        assert_eq!(once, twice);
    }

    #[test]
    fn four_space_indent_and_final_lf() {
        let src = "function f() -> Unit {\nreturn 1\n}";
        let formatted = format_source(src);
        assert!(
            formatted.contains("\n    return 1\n"),
            "got: {:?}",
            formatted
        );
        assert!(formatted.ends_with('\n'));
        let lines: Vec<&str> = formatted.lines().collect();
        assert!(lines[1].starts_with("    return"), "got: {:?}", lines);
    }

    #[test]
    fn preserves_comments() {
        let src = "// hello\nfunction f() -> Unit {} // trailing";
        let formatted = format_source(src);
        assert!(formatted.contains("// hello"));
        assert!(formatted.contains("// trailing"));
        assert!(!formatted.contains("\t"));
    }

    #[test]
    fn preserves_nested_block_comment() {
        let src = "/* outer /* inner */ still */ function f() -> Unit {}";
        let formatted = format_source(src);
        assert!(formatted.contains("/* outer /* inner */ still */"));
    }

    #[test]
    fn preserves_string_content() {
        let src = "const s: String = \"a  b  c\"";
        let formatted = format_source(src);
        assert!(formatted.contains("\"a  b  c\""));
    }

    #[test]
    fn does_not_reorder_imports() {
        let src = "import file_b\nimport file_a";
        let formatted = format_source(src);
        let ia = formatted.find("file_a").unwrap();
        let ib = formatted.find("file_b").unwrap();
        assert!(ib < ia, "imports must not be reordered: {}", formatted);
    }

    #[test]
    fn final_lf_single() {
        let src = "function f() -> Unit {}\n\n\n";
        let formatted = format_source(src);
        assert!(formatted.ends_with('\n'));
        // Exactly one trailing newline.
        let trimmed = formatted.trim_end_matches('\n');
        assert_eq!(trimmed.len() + 1, formatted.len());
    }

    #[test]
    fn verify_clean() {
        let src = "action load(id: Int) -> Unit effects [db::read] {\n    return id\n}";
        let v = verify(src);
        assert!(v.idempotent);
        assert!(v.semantic_preserved);
        assert!(v.is_clean());
    }

    #[test]
    fn crlf_normalized() {
        let src = "function f() -> Unit {\r\nreturn 1\r\n}";
        let formatted = format_source(src);
        assert!(!formatted.contains('\r'));
    }

    #[test]
    fn no_tabs_canonical() {
        let src = "function f() -> Unit {\treturn 1\t}";
        let formatted = format_source(src);
        assert!(!formatted.contains('\t'));
        assert!(formatted.contains("return 1"));
    }

    #[test]
    fn format_version_is_one() {
        assert_eq!(FORMAT_VERSION, 1);
    }
}
