//! Session-local identity types for symbols, scopes, modules, and names.

use serde::Serialize;
use std::fmt;

/// Session-local identity of a declared symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct SymbolId(pub u32);

impl fmt::Display for SymbolId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "symbol#{}", self.0)
    }
}

/// Session-local identity of a scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct ScopeId(pub u32);

impl fmt::Display for ScopeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "scope#{}", self.0)
    }
}

/// Session-local identity of a module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct ModuleId(pub u32);

impl fmt::Display for ModuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "module#{}", self.0)
    }
}

/// Session-local identity of a package instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct PackageInstanceId(pub u32);

impl fmt::Display for PackageInstanceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "package#{}", self.0)
    }
}

/// Interned name identifier for O(1) equality comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct NameId(pub u32);

impl fmt::Display for NameId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "name#{}", self.0)
    }
}

/// Sentinel: invalid/unresolved symbol.
pub const INVALID_SYMBOL: SymbolId = SymbolId(u32::MAX);
/// Sentinel: invalid/unresolved scope.
pub const INVALID_SCOPE: ScopeId = ScopeId(u32::MAX);
/// Sentinel: invalid/unresolved module.
pub const INVALID_MODULE: ModuleId = ModuleId(u32::MAX);
