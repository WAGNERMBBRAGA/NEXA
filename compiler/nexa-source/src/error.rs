//! Erros de carregamento de source.

use std::io;
use std::path::PathBuf;

/// Falha ao carregar um source **antes** da criação de um `SourceFile`.
///
/// `InvalidUtf8` pertence à fronteira de loading (o `SourceFile` só existe
/// com UTF-8 válido). A conversão para `Diagnostic` (NEXA-LEX-0001) é feita
/// pelo consumidor, para não criar ciclo de dependência com nexa-diagnostics.
#[derive(Debug)]
pub enum SourceLoadError {
    /// Erro de I/O de arquivo (host/tool error, não lexical).
    Io { path: PathBuf, source: io::Error },
    /// Bytes não são UTF-8 válido; `byte_offset` aponta o primeiro byte
    /// inválido.
    InvalidUtf8 { path: PathBuf, byte_offset: usize },
}

impl std::fmt::Display for SourceLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SourceLoadError::Io { path, source } => {
                write!(f, "failed to read '{}': {}", path.display(), source)
            }
            SourceLoadError::InvalidUtf8 { path, byte_offset } => {
                write!(
                    f,
                    "'{}' is not valid UTF-8 (first invalid byte at {})",
                    path.display(),
                    byte_offset
                )
            }
        }
    }
}

impl std::error::Error for SourceLoadError {}
