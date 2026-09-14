use nexa_source::SourceSpan;
use nexa_symbols::SymbolId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchKind {
    Direct,
    InterfaceContract,
    CallableValue,
}

#[derive(Debug, Clone)]
pub struct CallEdge {
    pub caller: SymbolId,
    pub callee: SymbolId,
    pub span: SourceSpan,
    pub dispatch: DispatchKind,
}

#[derive(Debug, Clone)]
pub struct CallGraph {
    pub edges: Vec<CallEdge>,
}

impl CallGraph {
    pub fn new() -> Self {
        Self { edges: Vec::new() }
    }

    pub fn add_edge(&mut self, edge: CallEdge) {
        self.edges.push(edge);
    }

    pub fn callees_of(&self, caller: SymbolId) -> Vec<&CallEdge> {
        self.edges.iter().filter(|e| e.caller == caller).collect()
    }

    pub fn callers_of(&self, callee: SymbolId) -> Vec<&CallEdge> {
        self.edges.iter().filter(|e| e.callee == callee).collect()
    }

    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }
}

impl Default for CallGraph {
    fn default() -> Self {
        Self::new()
    }
}
