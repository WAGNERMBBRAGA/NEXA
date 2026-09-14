//! NEXA — Project Model
//!
//! Grafo de projeto **em memória**: instâncias de package, unidades de módulo,
//! aliases de dependência e o modelo de projeto já parseado. NÃO faz parsing de
//! manifest e NÃO resolve registry (§54, §624). O resolver recebe este modelo
//! pré-materializado.
//!
//! ```text
//! ParsedProject
//! ├── packages: Vec<ParsedPackage>          (várias instâncias)
//! │   └── modules: Vec<ParsedModule>
//! │       ├── package:      PackageInstanceId
//! │       ├── module_path:  ModulePath (ex.: users::service)
//! │       ├── source_id, ast, is_public
//! └── current_package: PackageInstanceId
//! ```
//!
//! Regra Core (§71-91): imports são por module (`self::`, alias de dependência,
//! `as`); parses um único module; NEXA não possui symbol imports.

use nexa_ast::SourceUnit;
use nexa_source::SourceId;
use nexa_symbols::PackageInstanceId;
use serde::Serialize;

/// Path canônico de um module dentro de um package (ex.: `users::service`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct ModulePath {
    pub segments: Vec<String>,
}

impl ModulePath {
    pub fn new(segments: Vec<String>) -> Self {
        ModulePath { segments }
    }

    /// Último segmento do path (usado como alias local de import, §84).
    pub fn last_segment(&self) -> Option<&str> {
        self.segments.last().map(|s| s.as_str())
    }

    pub fn display(&self) -> String {
        self.segments.join("::")
    }

    /// Compara usando o path canônico (ordem determinística por segmentos).
    pub fn cmp_canonical(&self, other: &Self) -> std::cmp::Ordering {
        // Ordena por número de segmentos e depois lexicograficamente, para que
        // módulos "pais" apareçam antes de seus submódulos (§305).
        let len_cmp = self.segments.len().cmp(&other.segments.len());
        if len_cmp != std::cmp::Ordering::Equal {
            return len_cmp;
        }
        self.segments.cmp(&other.segments)
    }
}

/// Instância exata de um package (sem registry; identidade sintética do
/// harness para fixtures, §630).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct PackageInstance {
    pub id: PackageInstanceId,
    pub logical_name: String,
    /// Identidade semântica estável dentro da sessão (ex.: "app@0.0.0-test").
    pub semantic_id: String,
}

impl PackageInstance {
    pub fn new(id: PackageInstanceId, logical_name: impl Into<String>) -> Self {
        let logical_name = logical_name.into();
        let semantic_id = format!("{logical_name}@0.0.0-test");
        PackageInstance {
            id,
            logical_name,
            semantic_id,
        }
    }
}

/// Alias de dependência do project (manifest `dependency "http" { ... }` → alias
/// `http`). Determina o que o source pode usar como raiz de import cross-package
/// (§76-78). Somente dependências diretas (§79).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct DependencyAlias {
    pub alias: String,
    pub package: PackageInstanceId,
}

/// Um módulo de projeto já parseado (1 arquivo definidor, §17/§454).
#[derive(Debug, Clone)]
pub struct ParsedModule {
    pub package: PackageInstanceId,
    pub module_path: ModulePath,
    pub source_id: SourceId,
    pub ast: SourceUnit,
    /// Module público (vem do manifest; NÃO inferir de `export`, §59-60).
    pub is_public: bool,
}

/// Um package com seus módulos parseados.
#[derive(Debug, Clone)]
pub struct ParsedPackage {
    pub instance: PackageInstance,
    pub modules: Vec<ParsedModule>,
}

/// Projeto completo já parseado (múltiplas instâncias de package, §451-453).
#[derive(Debug, Clone)]
pub struct ParsedProject {
    pub current_package: PackageInstanceId,
    pub packages: Vec<ParsedPackage>,
}

impl ParsedProject {
    /// Todos os módulos do projeto, em ordem canônica determinística
    /// (current package primeiro; depois dependências por identidade estável;
    /// módulos por `ModulePath` canônico, §305-307).
    pub fn modules_sorted(&self) -> Vec<&ParsedModule> {
        let mut all: Vec<&ParsedModule> = self
            .packages
            .iter()
            .flat_map(|p| p.modules.iter())
            .collect();
        all.sort_by(|a, b| {
            let pa = a.package.0;
            let pb = b.package.0;
            let pkg_cmp = pa
                .cmp(&pb)
                .then_with(|| a.module_path.cmp_canonical(&b.module_path));
            pkg_cmp.then_with(|| a.module_path.display().cmp(&b.module_path.display()))
        });
        all
    }

    pub fn module_count(&self) -> usize {
        self.packages.iter().map(|p| p.modules.len()).sum()
    }

    /// Busca um módulo por (package, path).
    pub fn find_module(
        &self,
        package: PackageInstanceId,
        path: &ModulePath,
    ) -> Option<&ParsedModule> {
        self.packages
            .iter()
            .find(|p| p.instance.id == package)
            .and_then(|p| p.modules.iter().find(|m| &m.module_path == path))
    }
}

// ---------------------------------------------------------------------------
// Configuração do resolver (fatos derivados do project, §617).
// ---------------------------------------------------------------------------

/// Fatos de projeto que o resolver precisa, sem expor switches semânticos (§617).
#[derive(Debug, Clone, Default)]
pub struct ResolverConfig {
    /// Aliases de dependência direta do projeto (alias → package instance).
    pub dependency_aliases: Vec<DependencyAlias>,
}

impl ResolverConfig {
    pub fn package_for_alias(&self, alias: &str) -> Option<PackageInstanceId> {
        self.dependency_aliases
            .iter()
            .find(|a| a.alias == alias)
            .map(|a| a.package)
    }

    pub fn has_alias(&self, alias: &str) -> bool {
        self.dependency_aliases.iter().any(|a| a.alias == alias)
    }
}

// ---------------------------------------------------------------------------
// Construtores de conveniência (single-module, testes/harness).
// ---------------------------------------------------------------------------

/// Projeto com um único módulo (`module` declaration como path). Usado pelo
/// CLI `nexa resolve <FILE>` (modo single-module, §459) e pela API `resolve`.
pub fn single_module_project(
    current_package: PackageInstanceId,
    package_name: &str,
    module_path: ModulePath,
    source_id: SourceId,
    ast: SourceUnit,
    is_public: bool,
) -> ParsedProject {
    let instance = PackageInstance::new(current_package, package_name);
    let module = ParsedModule {
        package: current_package,
        module_path,
        source_id,
        ast,
        is_public,
    };
    ParsedProject {
        current_package,
        packages: vec![ParsedPackage {
            instance,
            modules: vec![module],
        }],
    }
}
