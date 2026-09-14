//! `SourceLocation` — posição derivada de um byte offset.

use serde::Serialize;

/// Posição derivada, 0-based, dentro de um arquivo.
///
/// Não é identidade: a autoridade é o byte offset. Linha/coluna são
/// calculadas sob demanda a partir do line index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct SourceLocation {
    pub line: u32,
    pub column: u32,
}

impl SourceLocation {
    pub fn new(line: u32, column: u32) -> Self {
        Self { line, column }
    }
}
