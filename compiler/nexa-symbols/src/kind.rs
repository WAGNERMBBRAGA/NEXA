//! Classification of declared symbols.

use serde::Serialize;

/// What a symbol represents in the semantic model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum SymbolKind {
    Module,
    ImportAlias,
    Struct,
    Enum,
    Interface,
    TypeAlias,
    DistinctType,
    Function,
    Action,
    Const,
    EnumVariant,
    Parameter,
    LocalLet,
    LocalVar,
    LocalConst,
    GenericParameter,
    Receiver,
    SelfType,
    ContractResult,
    Field,
    DependencyAlias,
}

impl SymbolKind {
    /// Namespace(s) this symbol kind occupies.
    pub fn default_namespace(self) -> &'static [crate::namespace::NamespaceKind] {
        use crate::namespace::NamespaceKind;
        match self {
            SymbolKind::Module | SymbolKind::ImportAlias | SymbolKind::DependencyAlias => {
                &[NamespaceKind::Module]
            }
            SymbolKind::Struct
            | SymbolKind::Enum
            | SymbolKind::Interface
            | SymbolKind::TypeAlias
            | SymbolKind::DistinctType
            | SymbolKind::GenericParameter
            | SymbolKind::SelfType => &[NamespaceKind::Type],
            SymbolKind::Function
            | SymbolKind::Action
            | SymbolKind::Const
            | SymbolKind::EnumVariant
            | SymbolKind::Parameter
            | SymbolKind::LocalLet
            | SymbolKind::LocalVar
            | SymbolKind::LocalConst
            | SymbolKind::Receiver
            | SymbolKind::ContractResult
            | SymbolKind::Field => &[NamespaceKind::Value],
        }
    }

    /// Is this a local binding (parameter, let, var, const inside a callable)?
    pub fn is_local(self) -> bool {
        matches!(
            self,
            SymbolKind::Parameter
                | SymbolKind::LocalLet
                | SymbolKind::LocalVar
                | SymbolKind::LocalConst
                | SymbolKind::Receiver
                | SymbolKind::GenericParameter
                | SymbolKind::ContractResult
        )
    }

    /// Is this a top-level (module-scope) declaration?
    pub fn is_top_level(self) -> bool {
        matches!(
            self,
            SymbolKind::Module
                | SymbolKind::ImportAlias
                | SymbolKind::Struct
                | SymbolKind::Enum
                | SymbolKind::Interface
                | SymbolKind::TypeAlias
                | SymbolKind::DistinctType
                | SymbolKind::Function
                | SymbolKind::Action
                | SymbolKind::Const
                | SymbolKind::DependencyAlias
        )
    }
}
