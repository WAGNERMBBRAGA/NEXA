//! Namespace classification for name resolution.

use serde::Serialize;

/// Namespaces that control name lookup separation.
///
/// NEXA separates names into independent namespaces to avoid
/// accidental capture between types, values, and modules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum NamespaceKind {
    /// Type names: struct, enum, interface, type alias, distinct type.
    Type,
    /// Value names: function, action, const, parameters, locals, fields.
    Value,
    /// Module names: modules and import aliases.
    Module,
}

impl NamespaceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            NamespaceKind::Type => "type",
            NamespaceKind::Value => "value",
            NamespaceKind::Module => "module",
        }
    }
}
