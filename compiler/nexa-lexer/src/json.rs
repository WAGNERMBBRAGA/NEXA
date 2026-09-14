//! Serialização JSON do output do lexer (Diagnostic/Tooling Schema 1).
//!
//! O JSON é determinístico e machine-readable; o formato humano é apenas
//! para debugging.

use crate::lexeme::LexemeKind;
use crate::lexer::LexResult;
use crate::token::TokenKind;
use nexa_diagnostics::Diagnostic;
use serde::Serialize;

/// Registro de token/trivia no schema de tokens (golden CTS).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenRecord {
    pub category: &'static str,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keyword: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    pub start_byte: u32,
    pub end_byte: u32,
}

/// Envelope do output `nexa lex --format json`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LexOutputEnvelope {
    pub schema_version: u32,
    pub tokens: Vec<TokenRecord>,
    pub diagnostics: Vec<Diagnostic>,
}

impl TokenRecord {
    pub fn from_lexeme(lexeme: &crate::lexeme::Lexeme) -> TokenRecord {
        match lexeme.kind {
            LexemeKind::Token(kind) => token_record(kind, lexeme.span.start, lexeme.span.end),
            LexemeKind::Trivia(trivia) => TokenRecord {
                category: "trivia",
                kind: trivia.as_str().to_owned(),
                keyword: None,
                style: None,
                start_byte: lexeme.span.start,
                end_byte: lexeme.span.end,
            },
        }
    }
}

fn token_record(kind: TokenKind, start: u32, end: u32) -> TokenRecord {
    let (name, keyword, style) = describe(kind);
    TokenRecord {
        category: "token",
        kind: name.to_owned(),
        keyword: keyword.map(|k| k.to_owned()),
        style: style.map(|s| s.to_owned()),
        start_byte: start,
        end_byte: end,
    }
}

/// Nome canônico do token, keyword (se houver) e style (se houver).
pub fn describe(kind: TokenKind) -> (&'static str, Option<&'static str>, Option<&'static str>) {
    use TokenKind::*;
    match kind {
        Identifier => ("Identifier", None, None),
        IntegerLiteral => ("IntegerLiteral", None, None),
        FloatLiteral => ("FloatLiteral", None, None),
        RawStringLiteral => ("RawStringLiteral", None, None),
        ByteStringLiteral => ("ByteStringLiteral", None, None),
        CharLiteral => ("CharLiteral", None, None),
        StringStart(style) => ("StringStart", None, Some(style.as_str())),
        StringText => ("StringText", None, None),
        InterpolationStart => ("InterpolationStart", None, None),
        InterpolationEnd => ("InterpolationEnd", None, None),
        StringEnd(style) => ("StringEnd", None, Some(style.as_str())),
        Keyword(k) => ("Keyword", Some(k.as_str()), None),
        InvalidToken => ("InvalidToken", None, None),
        LeftParen => ("LeftParen", None, None),
        RightParen => ("RightParen", None, None),
        LeftBrace => ("LeftBrace", None, None),
        RightBrace => ("RightBrace", None, None),
        LeftBracket => ("LeftBracket", None, None),
        RightBracket => ("RightBracket", None, None),
        Comma => ("Comma", None, None),
        Colon => ("Colon", None, None),
        DoubleColon => ("DoubleColon", None, None),
        Dot => ("Dot", None, None),
        DotDot => ("DotDot", None, None),
        At => ("At", None, None),
        Underscore => ("Underscore", None, None),
        Plus => ("Plus", None, None),
        Minus => ("Minus", None, None),
        Star => ("Star", None, None),
        Slash => ("Slash", None, None),
        Percent => ("Percent", None, None),
        Equal => ("Equal", None, None),
        EqualEqual => ("EqualEqual", None, None),
        Bang => ("Bang", None, None),
        BangEqual => ("BangEqual", None, None),
        Less => ("Less", None, None),
        LessEqual => ("LessEqual", None, None),
        Greater => ("Greater", None, None),
        GreaterEqual => ("GreaterEqual", None, None),
        Ampersand => ("Ampersand", None, None),
        Pipe => ("Pipe", None, None),
        Caret => ("Caret", None, None),
        Tilde => ("Tilde", None, None),
        AmpAmp => ("AmpAmp", None, None),
        PipePipe => ("PipePipe", None, None),
        ShiftLeft => ("ShiftLeft", None, None),
        ShiftRight => ("ShiftRight", None, None),
        PlusEqual => ("PlusEqual", None, None),
        MinusEqual => ("MinusEqual", None, None),
        StarEqual => ("StarEqual", None, None),
        SlashEqual => ("SlashEqual", None, None),
        PercentEqual => ("PercentEqual", None, None),
        AmpEqual => ("AmpEqual", None, None),
        PipeEqual => ("PipeEqual", None, None),
        CaretEqual => ("CaretEqual", None, None),
        ShiftLeftEqual => ("ShiftLeftEqual", None, None),
        ShiftRightEqual => ("ShiftRightEqual", None, None),
        Arrow => ("Arrow", None, None),
        FatArrow => ("FatArrow", None, None),
        Newline => ("Newline", None, None),
        Eof => ("Eof", None, None),
    }
}

/// Nome human-readable (snake_case) para o renderer do CLI.
pub fn human_kind(kind: TokenKind) -> String {
    let (name, keyword, style) = describe(kind);
    let base = to_snake(name);
    match (keyword, style) {
        (Some(k), _) => format!("{base}({k})"),
        (None, Some(s)) => format!("{base}({})", s.to_lowercase()),
        _ => base,
    }
}

fn to_snake(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// Constrói o envelope JSON completo do output do lexer.
pub fn lex_output_json(result: &LexResult) -> LexOutputEnvelope {
    let tokens = result
        .lexemes
        .iter()
        .map(TokenRecord::from_lexeme)
        .collect();
    LexOutputEnvelope {
        schema_version: 1,
        tokens,
        diagnostics: result.diagnostics.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::human_kind;
    use crate::keyword::Keyword;
    use crate::token::{StringStyle, TokenKind};

    #[test]
    fn human_names() {
        assert_eq!(human_kind(TokenKind::Identifier), "identifier");
        assert_eq!(
            human_kind(TokenKind::Keyword(Keyword::Module)),
            "keyword(module)"
        );
        assert_eq!(human_kind(TokenKind::LeftParen), "left_paren");
        assert_eq!(human_kind(TokenKind::ShiftLeftEqual), "shift_left_equal");
        assert_eq!(
            human_kind(TokenKind::StringStart(StringStyle::Normal)),
            "string_start(normal)"
        );
        assert_eq!(
            human_kind(TokenKind::StringEnd(StringStyle::Multiline)),
            "string_end(multiline)"
        );
        assert_eq!(human_kind(TokenKind::Newline), "newline");
        assert_eq!(human_kind(TokenKind::Eof), "eof");
    }
}
