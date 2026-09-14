//! Module index: registro de todos os modules da sessão, com lookup por
//! `(PackageInstanceId, ModulePath)` e hierarquia parent/children.
//!
//! Implementação 03 (§98, §1183-1187): o índice é a autoridade para
//! resolução de imports e caminhos qualificados. Paths duplicados no mesmo
//! package = NEXA-MODULE-0001.

use nexa_project::ModulePath;
use nexa_source::{SourceId, SourceSpan};
use nexa_symbols::{ModuleId, PackageInstanceId, ScopeId, SymbolId};
use std::collections::HashMap;

/// Metadata de um module no compilation session.
#[derive(Debug, Clone)]
pub struct ModuleEntry {
    pub id: ModuleId,
    pub name: String,
    pub package: PackageInstanceId,
    pub path: ModulePath,
    pub root_scope: ScopeId,
    /// Source unit que define este module (1 arquivo definidor).
    pub source_id: Option<SourceId>,
    pub span: SourceSpan,
    /// Filhos (submodules) declarados/registrados sob este module.
    pub children: Vec<ModuleId>,
    /// Parent path (None para modules raiz).
    pub parent: Option<ModuleId>,
    /// Module público (do manifest; NUNCA inferido de `export`, §59-60).
    pub is_public: bool,
    /// Symbol representando o module em si (identidade uniforme, §328).
    pub symbol: Option<SymbolId>,
}

/// Índice de todos os modules do projeto.
pub struct ModuleIndex {
    modules: Vec<ModuleEntry>,
    name_to_module: HashMap<String, Vec<ModuleId>>,
    package_path_to_module: HashMap<(PackageInstanceId, String), ModuleId>,
}

impl ModuleIndex {
    pub fn new() -> Self {
        ModuleIndex {
            modules: Vec::new(),
            name_to_module: HashMap::new(),
            package_path_to_module: HashMap::new(),
        }
    }

    pub fn insert(&mut self, entry: ModuleEntry) -> ModuleId {
        let id = entry.id;
        let key = (entry.package, entry.path.display());
        self.name_to_module
            .entry(entry.name.clone())
            .or_default()
            .push(id);
        self.package_path_to_module.insert(key, id);
        self.modules.push(entry);
        id
    }

    pub fn get(&self, id: ModuleId) -> Option<&ModuleEntry> {
        self.modules.get(id.0 as usize)
    }

    pub fn get_mut(&mut self, id: ModuleId) -> Option<&mut ModuleEntry> {
        self.modules.get_mut(id.0 as usize)
    }

    pub fn next_id(&self) -> ModuleId {
        ModuleId(self.modules.len() as u32)
    }

    pub fn iter(&self) -> impl Iterator<Item = &ModuleEntry> {
        self.modules.iter()
    }

    pub fn count(&self) -> usize {
        self.modules.len()
    }

    /// Lookup por (package, path canônico).
    pub fn find_by_package_path(
        &self,
        package: PackageInstanceId,
        path: &ModulePath,
    ) -> Option<ModuleId> {
        self.package_path_to_module
            .get(&(package, path.display()))
            .copied()
    }

    pub fn get_path(&self, id: ModuleId) -> Option<&ModulePath> {
        self.modules.get(id.0 as usize).map(|m| &m.path)
    }

    pub fn get_package(&self, id: ModuleId) -> Option<PackageInstanceId> {
        self.modules.get(id.0 as usize).map(|m| m.package)
    }

    pub fn is_public(&self, id: ModuleId) -> bool {
        self.modules
            .get(id.0 as usize)
            .map(|m| m.is_public)
            .unwrap_or(false)
    }
}

impl Default for ModuleIndex {
    fn default() -> Self {
        Self::new()
    }
}
