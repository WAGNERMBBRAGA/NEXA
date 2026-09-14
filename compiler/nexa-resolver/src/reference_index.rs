use nexa_source::SourceSpan;
use nexa_symbols::{ScopeId, SymbolId};
use std::collections::HashMap;

/// A name reference (use site) in the source code.
#[derive(Debug, Clone)]
pub struct SymbolReference {
    pub symbol: SymbolId,
    pub scope: ScopeId,
    pub span: SourceSpan,
    pub kind: ReferenceKind,
}

/// How a reference is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReferenceKind {
    /// Reading a value.
    Read,
    /// Writing (assignment target).
    Write,
    /// Using as a type.
    Type,
    /// Using as a function/callable.
    Call,
    /// Import reference.
    Import,
}

/// Tracks all name references across the compilation.
pub struct ReferenceIndex {
    references: Vec<SymbolReference>,
    by_scope: HashMap<ScopeId, Vec<usize>>,
    by_symbol: HashMap<SymbolId, Vec<usize>>,
}

impl ReferenceIndex {
    pub fn new() -> Self {
        ReferenceIndex {
            references: Vec::new(),
            by_scope: HashMap::new(),
            by_symbol: HashMap::new(),
        }
    }

    pub fn insert(&mut self, reference: SymbolReference) -> usize {
        let idx = self.references.len();
        let scope = reference.scope;
        let symbol = reference.symbol;
        self.references.push(reference);
        self.by_scope.entry(scope).or_default().push(idx);
        self.by_symbol.entry(symbol).or_default().push(idx);
        idx
    }

    pub fn references_in_scope(&self, scope: ScopeId) -> Vec<&SymbolReference> {
        self.by_scope
            .get(&scope)
            .map(|indices| indices.iter().map(|&i| &self.references[i]).collect())
            .unwrap_or_default()
    }

    pub fn references_to(&self, symbol: SymbolId) -> Vec<&SymbolReference> {
        self.by_symbol
            .get(&symbol)
            .map(|indices| indices.iter().map(|&i| &self.references[i]).collect())
            .unwrap_or_default()
    }

    pub fn count(&self) -> usize {
        self.references.len()
    }

    /// Todas as referências registradas, em ordem de inserção.
    pub fn all(&self) -> &[SymbolReference] {
        &self.references
    }
}

impl Default for ReferenceIndex {
    fn default() -> Self {
        Self::new()
    }
}
