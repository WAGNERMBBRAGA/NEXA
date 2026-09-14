//! `SourceManager` — carrega sources, atribui `SourceId` e faz lookup.

use crate::error::SourceLoadError;
use crate::id::SourceId;
use crate::source_file::SourceFile;
use std::path::PathBuf;

/// Coleção de sources carregados na sessão.
///
/// A `SourceId` é estável dentro da sessão: o índice do vetor nunca é
/// reutilizado (mesmo que um arquivo seja removido no futuro, os IDs
/// permanecem válidos enquanto a sessão existir).
#[derive(Debug, Default)]
pub struct SourceManager {
    files: Vec<SourceFile>,
}

impl SourceManager {
    pub fn new() -> Self {
        Self { files: Vec::new() }
    }

    /// Carrega bytes de um arquivo, validando UTF-8.
    ///
    /// - bytes UTF-8 válidos → `Ok(SourceId)`;
    /// - bytes inválidos → `Err(SourceLoadError::InvalidUtf8)`;
    /// - I/O (abrir/ler) é responsabilidade do chamador; aqui só recebemos
    ///   os bytes já lidos.
    pub fn load_bytes(
        &mut self,
        display_path: PathBuf,
        bytes: Vec<u8>,
    ) -> Result<SourceId, SourceLoadError> {
        match String::from_utf8(bytes) {
            Ok(text) => Ok(self.load_text(display_path, text)),
            Err(err) => {
                let byte_offset = err.utf8_error().valid_up_to();
                Err(SourceLoadError::InvalidUtf8 {
                    path: display_path,
                    byte_offset,
                })
            }
        }
    }

    /// Carrega texto já UTF-8 válido (ex.: testes in-memory).
    pub fn load_text(&mut self, display_path: PathBuf, text: String) -> SourceId {
        let id = SourceId(self.files.len() as u32);
        self.files
            .push(SourceFile::from_text(id, display_path, text));
        id
    }

    /// Lookup por `SourceId`. `None` se o id não pertence a esta sessão.
    pub fn source(&self, id: SourceId) -> Option<&SourceFile> {
        self.files.get(id.0 as usize)
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &SourceFile> {
        self.files.iter()
    }
}
