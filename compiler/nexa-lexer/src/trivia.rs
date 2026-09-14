//! `TriviaKind` — trivia lexical (whitespace e comentários).
//!
//! Trivia é preservada para o CST lossless e para o formatter futuro.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum TriviaKind {
    Whitespace,
    LineComment,
    BlockComment,
    DocumentationComment,
    ModuleDocumentationComment,
}

impl TriviaKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            TriviaKind::Whitespace => "Whitespace",
            TriviaKind::LineComment => "LineComment",
            TriviaKind::BlockComment => "BlockComment",
            TriviaKind::DocumentationComment => "DocumentationComment",
            TriviaKind::ModuleDocumentationComment => "ModuleDocumentationComment",
        }
    }
}
