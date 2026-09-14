//! Resolution map: ocorrência (span) → SymbolId.
//!
//! Implementação 03 (§341-344): usamos span como chave determinística para
//! "qual declaração é esta?" sem mutar o AST (side table, §338-339).

use nexa_source::{SourceId, SourceSpan};
use nexa_symbols::SymbolId;

#[derive(Debug, Clone, Default)]
pub struct ResolutionMap {
    /// Entradas na ordem de travessia (determinística).
    entries: Vec<(SourceSpan, SymbolId)>,
}

impl ResolutionMap {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn insert(&mut self, span: SourceSpan, symbol: SymbolId) {
        if !span.is_zero_width() {
            self.entries.push((span, symbol));
        }
    }

    pub fn count(&self) -> usize {
        self.entries.len()
    }

    /// Símbolo cujo span cobre o byte dado. Retorna a ocorrência mais
    /// interna (última inserida que contém o ponto).
    pub fn symbol_at(&self, source: SourceId, byte: u32) -> Option<SymbolId> {
        self.entries
            .iter()
            .rev()
            .find(|(sp, _)| {
                sp.source == source && sp.start <= byte && (sp.end > byte || sp.end == sp.start)
            })
            .map(|(_, sym)| *sym)
    }

    pub fn references_in_source(&self, source: SourceId) -> Vec<(SourceSpan, SymbolId)> {
        self.entries
            .iter()
            .filter(|(sp, _)| sp.source == source)
            .copied()
            .collect()
    }
}
