use nexa_source::SourceSpan;
use nexa_symbols::{ModuleId, NameId, ScopeId, SymbolId, SymbolKind, Visibility};

/// Data associated with each declared symbol.
#[derive(Debug, Clone)]
pub struct SymbolData {
    pub id: SymbolId,
    pub name: NameId,
    pub kind: SymbolKind,
    pub module: ModuleId,
    pub scope: ScopeId,
    pub visibility: Visibility,
    /// Span da declaração inteira.
    pub span: SourceSpan,
    /// Span exato do nome (rename/definition, §320).
    pub name_span: Option<SourceSpan>,
    /// Owner (struct/enum/interface) para symbol associados; None = top-level/local.
    pub owner: Option<SymbolId>,
    /// Para module/import-alias symbols, o module alvo.
    pub child_module: Option<ModuleId>,
    /// Para callables: o scope do corpo; para tipos com generics: o generic scope.
    pub body_scope: Option<ScopeId>,
    /// Se o corpo/escopo interno já foi construído.
    pub body_resolved: bool,
}

/// Table of all symbols in the compilation session.
pub struct SymbolTable {
    symbols: Vec<SymbolData>,
}

impl SymbolTable {
    pub fn new() -> Self {
        SymbolTable {
            symbols: Vec::new(),
        }
    }

    pub fn insert(&mut self, data: SymbolData) -> SymbolId {
        let id = data.id;
        self.symbols.push(data);
        id
    }

    pub fn get(&self, id: SymbolId) -> Option<&SymbolData> {
        self.symbols.get(id.0 as usize)
    }

    pub fn get_mut(&mut self, id: SymbolId) -> Option<&mut SymbolData> {
        self.symbols.get_mut(id.0 as usize)
    }

    pub fn next_id(&self) -> SymbolId {
        SymbolId(self.symbols.len() as u32)
    }

    pub fn iter(&self) -> impl Iterator<Item = &SymbolData> {
        self.symbols.iter()
    }

    pub fn count(&self) -> usize {
        self.symbols.len()
    }
}

impl Default for SymbolTable {
    fn default() -> Self {
        Self::new()
    }
}
