//! `SourceFile` — arquivo de source validado, com line index.

use crate::id::SourceId;
use crate::location::SourceLocation;
use crate::span::SourceSpan;
use std::path::PathBuf;

/// Um arquivo de source **válido** (UTF-8 garantido) já carregado.
///
/// Campos:
/// - `id`: identidade interna da session;
/// - `display_path`: path para exibição (não é identidade);
/// - `text`: conteúdo UTF-8;
/// - `line_starts`: byte offsets de início de cada linha.
#[derive(Debug)]
pub struct SourceFile {
    pub id: SourceId,
    pub display_path: PathBuf,
    pub text: String,
    line_starts: Vec<u32>,
}

impl SourceFile {
    /// Constrói um `SourceFile` a partir de texto UTF-8 **já validado**.
    /// O path é apenas informativo/exibição.
    pub fn from_text(id: SourceId, display_path: PathBuf, text: String) -> Self {
        let line_starts = build_line_starts(&text);
        Self {
            id,
            display_path,
            text,
            line_starts,
        }
    }

    /// Tamanho do source em bytes UTF-8.
    pub fn byte_len(&self) -> u32 {
        self.text.len() as u32
    }

    /// Nome amigável para exibição (lossy em paths não-UTF-8).
    pub fn display_name(&self) -> String {
        self.display_path.to_string_lossy().into_owned()
    }

    /// Retorna os bytes cobertos por `span`, se o span for válido e
    /// estiver em boundaries de caracteres.
    pub fn span_text(&self, span: SourceSpan) -> Option<&str> {
        if !self.is_valid_span(span) {
            return None;
        }
        let start = span.start as usize;
        let end = span.end as usize;
        self.text.get(start..end)
    }

    /// Valida que o span pertence a este arquivo, está dentro dos limites
    /// e satisfaz `start <= end`.
    pub fn is_valid_span(&self, span: SourceSpan) -> bool {
        span.source == self.id && span.start <= span.end && span.end <= self.byte_len()
    }

    /// Localização (linha/coluna 0-based) de um byte offset.
    /// Colunas são contadas em **Unicode scalar columns**.
    ///
    /// `offset` é clampado ao tamanho do arquivo.
    pub fn location(&self, offset: u32) -> SourceLocation {
        let offset = offset.min(self.byte_len()) as usize;
        let line_idx = self.line_index_for_offset(offset);
        let line_start = self.line_starts[line_idx] as usize;
        let column = self.text[line_start..offset].chars().count() as u32;
        SourceLocation::new(line_idx as u32, column)
    }

    /// Texto de uma linha (0-based), sem o newline final.
    pub fn line_text(&self, line: u32) -> Option<&str> {
        let line = line as usize;
        let start = *self.line_starts.get(line)? as usize;
        let end = match self.line_starts.get(line + 1) {
            Some(&end) => (end as usize).saturating_sub(1), // exclui '\n'
            None => self.text.len(),
        };
        let mut s = self.text.get(start..end)?;
        if s.ends_with('\r') {
            s = &s[..s.len() - 1];
        }
        Some(s)
    }

    /// Número de linhas (0-based: última linha = line_count - 1).
    pub fn line_count(&self) -> u32 {
        self.line_starts.len() as u32
    }

    /// `true` se o texto coberto pelo span contém quebra de linha
    /// (`\n` ou `\r`). Útil para trivia (ex.: block comment multilinha).
    pub fn span_contains_line_break(&self, span: SourceSpan) -> bool {
        match self.span_text(span) {
            Some(text) => text.contains('\n') || text.contains('\r'),
            None => false,
        }
    }

    fn line_index_for_offset(&self, offset: usize) -> usize {
        // Maior line_start <= offset.
        match self.line_starts.binary_search(&(offset as u32)) {
            Ok(i) => i,
            Err(insert) => insert - 1,
        }
    }
}

fn build_line_starts(text: &str) -> Vec<u32> {
    let mut starts = Vec::with_capacity(16);
    starts.push(0u32);
    for (i, b) in text.bytes().enumerate() {
        if b == b'\n' {
            starts.push((i + 1) as u32);
        }
    }
    starts
}
