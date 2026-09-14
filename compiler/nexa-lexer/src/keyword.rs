//! `Keyword` — keywords core da NEXA 1.0 (freeze candidate).
//!
//! Apenas as keywords **ativas/reservadas** entram neste enum. As keywords
//! contextuais (`as`, `where`, `in`, `self`, `Self`, `distinct`) **não**
//! entram: são lexadas como `Identifier` e reconhecidas contextualmente pelo
//! parser. Ver `CONTEXTUAL_KEYWORDS`.

use crate::token::TokenKind;
use serde::Serialize;

/// Textos das keywords contextuais (lexadas como `Identifier`).
///
/// O lexer não toma decisões semânticas que pertencem ao parser (Impl 01 §88-89).
pub const CONTEXTUAL_KEYWORDS: &[&str] = &["as", "where", "in", "self", "Self", "distinct"];

/// Keywords core da NEXA (case-sensitive, lowercase).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Keyword {
    Module,
    Import,
    Export,
    Struct,
    Enum,
    Interface,
    Implement,
    Type,
    Function,
    Action,
    Async,
    Let,
    Var,
    Const,
    If,
    Else,
    Match,
    For,
    While,
    Loop,
    Break,
    Continue,
    Return,
    Discard,
    Effects,
    Require,
    Ensure,
    Ref,
    Mut,
    Move,
    Await,
    Try,
    Unsafe,
    True,
    False,
}

impl Keyword {
    /// Texto exato (lowercase) da keyword.
    pub fn as_str(&self) -> &'static str {
        match self {
            Keyword::Module => "module",
            Keyword::Import => "import",
            Keyword::Export => "export",
            Keyword::Struct => "struct",
            Keyword::Enum => "enum",
            Keyword::Interface => "interface",
            Keyword::Implement => "implement",
            Keyword::Type => "type",
            Keyword::Function => "function",
            Keyword::Action => "action",
            Keyword::Async => "async",
            Keyword::Let => "let",
            Keyword::Var => "var",
            Keyword::Const => "const",
            Keyword::If => "if",
            Keyword::Else => "else",
            Keyword::Match => "match",
            Keyword::For => "for",
            Keyword::While => "while",
            Keyword::Loop => "loop",
            Keyword::Break => "break",
            Keyword::Continue => "continue",
            Keyword::Return => "return",
            Keyword::Discard => "discard",
            Keyword::Effects => "effects",
            Keyword::Require => "require",
            Keyword::Ensure => "ensure",
            Keyword::Ref => "ref",
            Keyword::Mut => "mut",
            Keyword::Move => "move",
            Keyword::Await => "await",
            Keyword::Try => "try",
            Keyword::Unsafe => "unsafe",
            Keyword::True => "true",
            Keyword::False => "false",
        }
    }
}

/// Lookup de keyword por texto exato (sem HashMap, match estático).
pub fn keyword(text: &str) -> Option<Keyword> {
    let k = match text {
        "module" => Keyword::Module,
        "import" => Keyword::Import,
        "export" => Keyword::Export,
        "struct" => Keyword::Struct,
        "enum" => Keyword::Enum,
        "interface" => Keyword::Interface,
        "implement" => Keyword::Implement,
        "type" => Keyword::Type,
        "function" => Keyword::Function,
        "action" => Keyword::Action,
        "async" => Keyword::Async,
        "let" => Keyword::Let,
        "var" => Keyword::Var,
        "const" => Keyword::Const,
        "if" => Keyword::If,
        "else" => Keyword::Else,
        "match" => Keyword::Match,
        "for" => Keyword::For,
        "while" => Keyword::While,
        "loop" => Keyword::Loop,
        "break" => Keyword::Break,
        "continue" => Keyword::Continue,
        "return" => Keyword::Return,
        "discard" => Keyword::Discard,
        "effects" => Keyword::Effects,
        "require" => Keyword::Require,
        "ensure" => Keyword::Ensure,
        "ref" => Keyword::Ref,
        "mut" => Keyword::Mut,
        "move" => Keyword::Move,
        "await" => Keyword::Await,
        "try" => Keyword::Try,
        "unsafe" => Keyword::Unsafe,
        "true" => Keyword::True,
        "false" => Keyword::False,
        _ => return None,
    };
    Some(k)
}

/// Quantidade de keywords core (usada em testes de cobertura).
pub const KEYWORD_COUNT: usize = 35;

/// Todos os keywords, na ordem de declaração (para CTS de cobertura).
pub const ALL_KEYWORDS: &[Keyword] = &[
    Keyword::Module,
    Keyword::Import,
    Keyword::Export,
    Keyword::Struct,
    Keyword::Enum,
    Keyword::Interface,
    Keyword::Implement,
    Keyword::Type,
    Keyword::Function,
    Keyword::Action,
    Keyword::Async,
    Keyword::Let,
    Keyword::Var,
    Keyword::Const,
    Keyword::If,
    Keyword::Else,
    Keyword::Match,
    Keyword::For,
    Keyword::While,
    Keyword::Loop,
    Keyword::Break,
    Keyword::Continue,
    Keyword::Return,
    Keyword::Discard,
    Keyword::Effects,
    Keyword::Require,
    Keyword::Ensure,
    Keyword::Ref,
    Keyword::Mut,
    Keyword::Move,
    Keyword::Await,
    Keyword::Try,
    Keyword::Unsafe,
    Keyword::True,
    Keyword::False,
];

impl From<Keyword> for TokenKind {
    fn from(k: Keyword) -> Self {
        TokenKind::Keyword(k)
    }
}
