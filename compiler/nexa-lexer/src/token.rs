//! `TokenKind` — inventário lexical final da Implementação 01.

use crate::keyword::Keyword;
use serde::Serialize;

/// Estilo de string delimitada (para `StringStart`/`StringEnd`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum StringStyle {
    Normal,
    Multiline,
}

impl StringStyle {
    pub fn as_str(&self) -> &'static str {
        match self {
            StringStyle::Normal => "Normal",
            StringStyle::Multiline => "Multiline",
        }
    }
}

/// Tipo de um token.
///
/// O token **não** carrega lexeme copiado: o texto é recuperado via
/// `SourceSpan`. `Keyword` é exceção (enum pequeno).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenKind {
    // literais
    Identifier,
    IntegerLiteral,
    FloatLiteral,
    RawStringLiteral,
    ByteStringLiteral,
    CharLiteral,

    // strings com modo (segmentadas)
    StringStart(StringStyle),
    StringText,
    InterpolationStart,
    InterpolationEnd,
    StringEnd(StringStyle),

    // keywords
    Keyword(Keyword),

    // token de recovery (não faz parte da gramática válida)
    InvalidToken,

    // pontuação
    LeftParen,
    RightParen,
    LeftBrace,
    RightBrace,
    LeftBracket,
    RightBracket,
    Comma,
    Colon,
    DoubleColon,
    Dot,
    DotDot,
    At,
    Underscore,

    // operadores aritméticos
    Plus,
    Minus,
    Star,
    Slash,
    Percent,

    // comparação / lógicos
    Equal,
    EqualEqual,
    Bang,
    BangEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Ampersand,
    Pipe,
    Caret,
    Tilde,
    AmpAmp,
    PipePipe,
    ShiftLeft,
    ShiftRight,

    // compound assignment
    PlusEqual,
    MinusEqual,
    StarEqual,
    SlashEqual,
    PercentEqual,
    AmpEqual,
    PipeEqual,
    CaretEqual,
    ShiftLeftEqual,
    ShiftRightEqual,

    // setas
    Arrow,
    FatArrow,

    Newline,
    Eof,
}
