//! `Lexeme` — item unificado da stream lossless (token OU trivia).

use crate::token::TokenKind;
use crate::trivia::TriviaKind;
use nexa_source::{SourceFile, SourceSpan};

/// Categoria de um lexeme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LexemeKind {
    Token(TokenKind),
    Trivia(TriviaKind),
}

/// Item da stream lexical: token ou trivia, com span.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Lexeme {
    pub kind: LexemeKind,
    pub span: SourceSpan,
}

impl Lexeme {
    pub fn token(kind: TokenKind, span: SourceSpan) -> Self {
        Lexeme {
            kind: LexemeKind::Token(kind),
            span,
        }
    }

    pub fn trivia(kind: TriviaKind, span: SourceSpan) -> Self {
        Lexeme {
            kind: LexemeKind::Trivia(kind),
            span,
        }
    }

    pub fn is_token(&self) -> bool {
        matches!(self.kind, LexemeKind::Token(_))
    }

    pub fn is_trivia(&self) -> bool {
        matches!(self.kind, LexemeKind::Trivia(_))
    }

    pub fn as_token(&self) -> Option<TokenKind> {
        match self.kind {
            LexemeKind::Token(k) => Some(k),
            LexemeKind::Trivia(_) => None,
        }
    }

    /// `true` se a trivia cobre bytes que contêm quebra de linha
    /// (ex.: block comment multilinha). Relevante para o parser
    /// (terminação de statements sem semicolon canônico).
    pub fn contains_line_break(&self, source: &SourceFile) -> bool {
        source.span_contains_line_break(self.span)
    }
}
