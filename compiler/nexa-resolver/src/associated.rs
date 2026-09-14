//! Associated symbol indexing (Impl 03 §237-244, §397-403).
//!
//! Cada owner (struct/enum/interface) pode ter:
//! - associated **values**: métodos, funções associadas, variants de enum
//!   (mesma mesa de valores; nome duplicado = erro, §490);
//! - **fields** (mesa própria; duplicadas = erro).
//!
//! Não é namespace lexical normal (§234): campos/métodos não entram nos
//! scopes de module/callable.

use nexa_symbols::{NameId, SymbolId};
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct AssociatedSymbolIndex {
    values: HashMap<(SymbolId, NameId), SymbolId>,
    fields: HashMap<(SymbolId, NameId), SymbolId>,
}

impl AssociatedSymbolIndex {
    pub fn new() -> Self {
        Default::default()
    }

    /// Insere um nome associado de valor. Retorna o símbolo anterior se
    /// duplicado no mesmo owner/name (SEM-0002).
    pub fn insert_value(
        &mut self,
        owner: SymbolId,
        name: NameId,
        member: SymbolId,
    ) -> Option<SymbolId> {
        self.values.insert((owner, name), member)
    }

    pub fn insert_field(
        &mut self,
        owner: SymbolId,
        name: NameId,
        member: SymbolId,
    ) -> Option<SymbolId> {
        self.fields.insert((owner, name), member)
    }

    pub fn lookup_value(&self, owner: SymbolId, name: NameId) -> Option<SymbolId> {
        self.values.get(&(owner, name)).copied()
    }

    pub fn lookup_field(&self, owner: SymbolId, name: NameId) -> Option<SymbolId> {
        self.fields.get(&(owner, name)).copied()
    }

    /// Todos os valores associados de um owner, ordenados por NameId
    /// (saída determinística, §603).
    pub fn values_of(&self, owner: SymbolId) -> Vec<(NameId, SymbolId)> {
        let mut out: Vec<_> = self
            .values
            .iter()
            .filter(|((o, _), _)| *o == owner)
            .map(|((_, n), s)| (*n, *s))
            .collect();
        out.sort_by_key(|(n, _)| n.0);
        out
    }

    pub fn fields_of(&self, owner: SymbolId) -> Vec<(NameId, SymbolId)> {
        let mut out: Vec<_> = self
            .fields
            .iter()
            .filter(|((o, _), _)| *o == owner)
            .map(|((_, n), s)| (*n, *s))
            .collect();
        out.sort_by_key(|(n, _)| n.0);
        out
    }
}
