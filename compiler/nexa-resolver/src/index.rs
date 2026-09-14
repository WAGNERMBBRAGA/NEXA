use crate::associated::AssociatedSymbolIndex;
use crate::module_index::{ModuleEntry, ModuleIndex};
use crate::name_interner::NameInterner;
use crate::reference_index::{ReferenceIndex, SymbolReference};
use crate::resolution::ResolutionMap;
use crate::scope::ScopeGraph;
use crate::symbol::{SymbolData, SymbolTable};
use nexa_project::ModulePath;
use nexa_source::{SourceId, SourceSpan};
use nexa_symbols::{ModuleId, NameId, PackageInstanceId, ScopeId, SymbolId};
use std::collections::HashMap;

/// O semantic index: toda a informação semântica resolvida da sessão.
///
/// Queries mínimas (§499):
/// - `symbol`, `module`, `scope`
/// - `symbol_at`, `definition_of`, `references_to`, `module_by_path`,
///   `resolve_debug`, `associated_values_of`
pub struct SemanticIndex {
    pub interner: NameInterner,
    pub symbols: SymbolTable,
    pub scopes: ScopeGraph,
    pub modules: ModuleIndex,
    pub references: ReferenceIndex,
    /// Símbolos associados (métodos, fields, variants) por owner.
    pub associated: AssociatedSymbolIndex,
    /// Span → SymbolId (side table, sem mutar AST).
    pub resolutions: ResolutionMap,
    /// A raiz do projeto sendo compilado.
    pub root_source: Option<SourceId>,
    /// Module sintético do Prelude 1.0 (§173-180).
    pub prelude_module: Option<ModuleId>,
    /// Nomes de tipos do Prelude → SymbolId (type namespace).
    prelude_types: HashMap<NameId, SymbolId>,
    /// Nomes de valores do Prelude → SymbolId (value namespace).
    prelude_values: HashMap<NameId, SymbolId>,
}

impl SemanticIndex {
    pub fn new() -> Self {
        SemanticIndex {
            interner: NameInterner::new(),
            symbols: SymbolTable::new(),
            scopes: ScopeGraph::new(),
            modules: ModuleIndex::new(),
            references: ReferenceIndex::new(),
            associated: AssociatedSymbolIndex::new(),
            resolutions: ResolutionMap::new(),
            root_source: None,
            prelude_module: None,
            prelude_types: HashMap::new(),
            prelude_values: HashMap::new(),
        }
    }

    pub fn set_prelude(
        &mut self,
        module: Option<ModuleId>,
        types: HashMap<NameId, SymbolId>,
        values: HashMap<NameId, SymbolId>,
    ) {
        self.prelude_module = module;
        self.prelude_types = types;
        self.prelude_values = values;
    }

    /// Símbolo do Prelude por nome textual (type ou value namespace).
    pub fn prelude_symbol(&self, name: &str, value: bool) -> Option<SymbolId> {
        let name_id = self.interner.lookup(name)?;
        let table = if value {
            &self.prelude_values
        } else {
            &self.prelude_types
        };
        table.get(&name_id).copied()
    }

    /// O nome `name` é reservado no namespace dado pelo Prelude?
    pub fn is_prelude_name(&self, name: &str, value: bool) -> bool {
        self.prelude_symbol(name, value).is_some()
    }

    // ─── Queries básicas ─────────────────────────────────────────

    pub fn symbol(&self, id: SymbolId) -> Option<&SymbolData> {
        self.symbols.get(id)
    }

    pub fn module(&self, id: ModuleId) -> Option<&ModuleEntry> {
        self.modules.get(id)
    }

    pub fn scope(&self, id: ScopeId) -> Option<&crate::scope::Scope> {
        self.scopes.scope(id)
    }

    pub fn name_of(&self, id: NameId) -> &str {
        self.interner.resolve(id)
    }

    pub fn symbol_count(&self) -> usize {
        self.symbols.count()
    }

    pub fn scope_count(&self) -> usize {
        self.scopes.scope_count()
    }

    pub fn module_count(&self) -> usize {
        self.modules.count()
    }

    pub fn reference_count(&self) -> usize {
        self.references.count()
    }

    // ─── Queries semânticas de tooling (§499) ────────────────────

    /// Símbolo no (source, byte) exato (go-to-definition / hover).
    pub fn symbol_at(&self, source: SourceId, byte: u32) -> Option<SymbolId> {
        self.resolutions.symbol_at(source, byte)
    }

    /// Span da declaração (definition) de um símbolo.
    pub fn definition_of(&self, symbol: SymbolId) -> Option<SourceSpan> {
        self.symbols.get(symbol).map(|d| d.span)
    }

    /// Todas as referências para um símbolo (find references), ordenadas.
    pub fn references_to(&self, symbol: SymbolId) -> Vec<&SymbolReference> {
        let mut v = self.references.references_to(symbol);
        v.sort_by_key(|r| (r.span.source.0, r.span.start, r.span.end));
        v
    }

    /// Module por (package, path).
    pub fn module_by_path(
        &self,
        package: PackageInstanceId,
        path: &ModulePath,
    ) -> Option<ModuleId> {
        self.modules.find_by_package_path(package, path)
    }

    /// Resolução registrada para um span (resolve_debug).
    pub fn resolve_debug(&self, span: SourceSpan) -> Option<SymbolId> {
        self.resolutions.symbol_at(span.source, span.start)
    }

    pub fn associated_values_of(&self, owner: SymbolId) -> Vec<(NameId, SymbolId)> {
        self.associated.values_of(owner)
    }

    pub fn associated_fields_of(&self, owner: SymbolId) -> Vec<(NameId, SymbolId)> {
        self.associated.fields_of(owner)
    }

    /// Scope mais interno cujo span cobre o byte (completion).
    pub fn scope_at(&self, source: SourceId, byte: u32) -> Option<ScopeId> {
        let mut best: Option<ScopeId> = None;
        let mut best_size = u32::MAX;
        for so in 0..self.scopes.scope_count() {
            let id = ScopeId(so as u32);
            if let Some(s) = self.scopes.scope(id) {
                if s.span.source == source
                    && s.span.start <= byte
                    && (s.span.end > byte || s.span.end == s.span.start)
                    && (s.span.end - s.span.start) <= best_size
                {
                    best_size = s.span.end - s.span.start;
                    best = Some(id);
                }
            }
        }
        best
    }
}

impl Default for SemanticIndex {
    fn default() -> Self {
        Self::new()
    }
}
