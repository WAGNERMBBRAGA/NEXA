use nexa_source::SourceSpan;
use nexa_symbols::{ModuleId, NameId, ScopeId, SymbolId};
use std::collections::HashMap;

/// Kind of scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScopeKind {
    Package,
    Module,
    Callable,
    Block,
    MatchArm,
    Loop,
    GenericParameters,
    Implement,
    Interface,
}

/// A single scope node in the scope graph.
#[derive(Debug, Clone)]
pub struct Scope {
    pub id: ScopeId,
    pub kind: ScopeKind,
    pub parent: Option<ScopeId>,
    pub module: ModuleId,
    pub span: SourceSpan,
    /// Type namespace: NameId -> SymbolId
    pub type_names: HashMap<NameId, SymbolId>,
    /// Value namespace: NameId -> SymbolId
    pub value_names: HashMap<NameId, SymbolId>,
    /// Module namespace: NameId -> SymbolId
    pub module_names: HashMap<NameId, SymbolId>,
}

impl Scope {
    pub fn new(
        id: ScopeId,
        kind: ScopeKind,
        parent: Option<ScopeId>,
        module: ModuleId,
        span: SourceSpan,
    ) -> Self {
        Scope {
            id,
            kind,
            parent,
            module,
            span,
            type_names: HashMap::new(),
            value_names: HashMap::new(),
            module_names: HashMap::new(),
        }
    }

    /// Look up a name in this scope only (not parents).
    pub fn lookup_local(&self, name: NameId, ns: NamespaceChoice) -> Option<SymbolId> {
        match ns {
            NamespaceChoice::Type => self.type_names.get(&name).copied(),
            NamespaceChoice::Value => self.value_names.get(&name).copied(),
            NamespaceChoice::Module => self.module_names.get(&name).copied(),
        }
    }

    /// Insert a name into the appropriate namespace.
    pub fn insert(
        &mut self,
        name: NameId,
        symbol: SymbolId,
        ns: NamespaceChoice,
    ) -> Option<SymbolId> {
        let table = match ns {
            NamespaceChoice::Type => &mut self.type_names,
            NamespaceChoice::Value => &mut self.value_names,
            NamespaceChoice::Module => &mut self.module_names,
        };
        table.insert(name, symbol)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NamespaceChoice {
    Type,
    Value,
    Module,
}

/// The scope graph for a compilation session.
pub struct ScopeGraph {
    scopes: Vec<Scope>,
    module_scope: HashMap<ModuleId, ScopeId>,
}

impl ScopeGraph {
    pub fn new() -> Self {
        ScopeGraph {
            scopes: Vec::new(),
            module_scope: HashMap::new(),
        }
    }

    pub fn add_scope(
        &mut self,
        kind: ScopeKind,
        parent: Option<ScopeId>,
        module: ModuleId,
        span: SourceSpan,
    ) -> ScopeId {
        let id = ScopeId(self.scopes.len() as u32);
        let scope = Scope::new(id, kind, parent, module, span);
        self.scopes.push(scope);
        if kind == ScopeKind::Module {
            self.module_scope.insert(module, id);
        }
        id
    }

    pub fn scope(&self, id: ScopeId) -> Option<&Scope> {
        self.scopes.get(id.0 as usize)
    }

    pub fn scope_mut(&mut self, id: ScopeId) -> Option<&mut Scope> {
        self.scopes.get_mut(id.0 as usize)
    }

    pub fn module_scope(&self, module: ModuleId) -> Option<ScopeId> {
        self.module_scope.get(&module).copied()
    }

    /// Walk the scope chain from `scope_id` upward.
    pub fn ancestors(&self, scope_id: ScopeId) -> ScopeChain<'_> {
        ScopeChain {
            graph: self,
            current: Some(scope_id),
        }
    }

    /// Find the innermost callable scope ancestor.
    pub fn innermost_callable(&self, scope_id: ScopeId) -> Option<ScopeId> {
        for ancestor in self.ancestors(scope_id) {
            if ancestor.kind == ScopeKind::Callable {
                return Some(ancestor.id);
            }
        }
        None
    }

    pub fn scope_count(&self) -> usize {
        self.scopes.len()
    }
}

impl Default for ScopeGraph {
    fn default() -> Self {
        Self::new()
    }
}

pub struct ScopeChain<'a> {
    graph: &'a ScopeGraph,
    current: Option<ScopeId>,
}

impl<'a> Iterator for ScopeChain<'a> {
    type Item = &'a Scope;

    fn next(&mut self) -> Option<Self::Item> {
        let id = self.current?;
        let scope = self.graph.scope(id)?;
        self.current = scope.parent;
        Some(scope)
    }
}
