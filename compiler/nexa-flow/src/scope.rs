//! Scope Management
//! Tracks lexical scopes, variable bindings, and lifetimes across blocks

use crate::flow_control::BlockId;
use nexa_source::SourceSpan;
use std::collections::HashMap;

/// Represents a scope level in the program
#[derive(Debug)]
pub struct Scope {
    pub id: ScopeId,
    /// The name of this scope (if known)
    pub name: Option<String>,
    /// Variable bindings and their types/lifetimes in this scope
    pub bindings: HashMap<String, BindingInfo>,
    /// Current block ID for termination tracking
    pub current_block_id: Option<BlockId>,
}

impl Scope {
    fn new(id: ScopeId, name: Option<&str>) -> Self {
        Self {
            id,
            name: name.map(str::to_owned),
            bindings: HashMap::new(),
            current_block_id: None,
        }
    }
}

/// Information about a variable binding
#[derive(Debug)]
pub struct BindingInfo {
    /// The symbol ID for the variable
    pub id: String,
    /// Type or lifetime information (placeholder)
    pub type_info: TypeInfo,
    /// Span where this binding was created
    pub span: Option<SourceSpan>,
}

/// Placeholder for type/lifetime information
#[derive(Debug)]
pub enum TypeInfo {
    /// Variable with a concrete type
    Typing(String),
    /// Lifetimes only (e.g., &str without knowing the string)
    Lifetime(LifetimeInfo),
}

#[derive(Debug, Clone)]
pub struct LifetimeInfo {
    pub span: Option<SourceSpan>,
    pub lifetime_name: String,
}

/// Scope stack for tracking nested scopes
#[derive(Debug)]
pub struct ScopeStack {
    pub root_scope_id: ScopeId,
    /// Stack of all created scopes (top is current)
    pub scopes: Vec<Scope>,
}

impl ScopeStack {
    /// Create a new scope stack with an initial root scope
    pub fn new() -> Self {
        let root_id = ScopeId::new(0);
        let root = Scope::new(root_id, Some("root"));
        Self {
            root_scope_id: root_id,
            scopes: vec![root],
        }
    }

    /// Push a new scope onto the stack
    pub fn enter(&mut self, name: Option<&str>) -> &Scope {
        let id = ScopeId::new(self.scopes.len() as u32);
        let scope = Scope::new(id, name);
        self.scopes.push(scope);
        self.scopes.last().expect("scope was just pushed")
    }

    /// Return current scope reference (last pushed)
    pub fn current(&self) -> &Scope {
        self.scopes.last().unwrap()
    }

    /// Get root scope
    pub fn root(&self) -> &Scope {
        self.scopes.first().unwrap()
    }

    /// Get scope by ID
    pub fn get(&self, id: ScopeId) -> Option<&Scope> {
        self.scopes.iter().find(|scope| scope.id == id)
    }
}

/// Scope identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScopeId(u32);

impl ScopeId {
    pub fn new(id: u32) -> Self {
        Self(id)
    }
}

impl Default for ScopeStack {
    fn default() -> Self {
        Self::new()
    }
}
