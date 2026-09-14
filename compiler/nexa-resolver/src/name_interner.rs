use nexa_symbols::NameId;
use std::collections::HashMap;

pub struct NameInterner {
    names: Vec<String>,
    map: HashMap<String, NameId>,
}

impl NameInterner {
    pub fn new() -> Self {
        NameInterner {
            names: Vec::new(),
            map: HashMap::new(),
        }
    }

    pub fn intern(&mut self, name: &str) -> NameId {
        if let Some(&id) = self.map.get(name) {
            return id;
        }
        let id = NameId(self.names.len() as u32);
        self.names.push(name.to_string());
        self.map.insert(name.to_string(), id);
        id
    }

    /// Lookup read-only: NaN (None) se o nome ainda não foi internado.
    pub fn lookup(&self, name: &str) -> Option<NameId> {
        self.map.get(name).copied()
    }

    pub fn resolve(&self, id: NameId) -> &str {
        self.names
            .get(id.0 as usize)
            .map(|s| s.as_str())
            .unwrap_or("")
    }

    pub fn resolve_or_empty(&self, id: NameId) -> &str {
        self.resolve(id)
    }

    pub fn count(&self) -> usize {
        self.names.len()
    }

    pub fn ids(&self) -> impl Iterator<Item = NameId> {
        (0..self.names.len() as u32).map(NameId)
    }
}

impl Default for NameInterner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intern_and_resolve() {
        let mut interner = NameInterner::new();
        let foo = interner.intern("foo");
        let bar = interner.intern("bar");
        let foo2 = interner.intern("foo");
        assert_eq!(foo, foo2);
        assert_ne!(foo, bar);
        assert_eq!(interner.resolve(foo), "foo");
        assert_eq!(interner.resolve(bar), "bar");
        assert_eq!(interner.count(), 2);
    }
}
