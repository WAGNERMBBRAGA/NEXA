//! `Resolver` — resolução semântica de símbolos (Implementação 03).
//!
//! Pipeline (§245, §455):
//!
//! ```text
//! A. index modules            (grafo de projeto materializado)
//! B. index top-level nominal/value declarations
//! C. resolve import aliases   (self::, aliases de dependência, `as`)
//! D. resolve implement target/interface headers + Self
//! E. index associated members (fields, variants, métodos) + self receiver
//! F. resolve signatures       (params, returns, fields, where, generic args)
//! G. resolve bodies           (exprs, locals, patterns, contracts)
//! ```
//!
//! Policy baseline (§476, §135-148): locals/params podem fazer shadowing de
//! symbols de module/prelude, mas NÃO de outro local/parameter na mesma
//! ancestry lexical do callable (SEM-0006). Duplicates sempre = error,
//! primeira declaração válida permanece (§360-364). Prelude 1.0 names são
//! reservados no seu namespace semântico no nível de module (§188).

use crate::index::SemanticIndex;
use crate::module_index::ModuleEntry;
use crate::reference_index::{ReferenceKind, SymbolReference};
use crate::scope::{NamespaceChoice, ScopeKind};
use crate::symbol::SymbolData;
use nexa_ast::{
    Block, ContractExpr, Expr, ExprKind, GenericParam, ImplMethod, InterfaceMethod, ItemKind,
    MatchArm, Param, Pattern, PatternKind, SourceUnit, Stmt, StmtKind, Type, TypeKind,
    WherePredicate,
};
use nexa_diagnostics::code::{
    MODULE_DUPLICATE_IMPORT_ALIAS, MODULE_DUPLICATE_MODULE, MODULE_IMPORT_NAME_CONFLICT,
    MODULE_MODULE_NOT_VISIBLE, MODULE_TRANSITIVE_DEPENDENCY_NOT_DIRECTLY_ACCESSIBLE,
    MODULE_UNKNOWN_DEPENDENCY_ALIAS, MODULE_UNKNOWN_MODULE, SEM_DUPLICATE_DECLARATION,
    SEM_INVALID_QUALIFIED_PATH, SEM_INVALID_SELF_REFERENCE, SEM_INVALID_SELF_TYPE_REFERENCE,
    SEM_INVALID_SHADOWING, SEM_PRELUDE_NAME_CONFLICT, SEM_SYMBOL_NOT_VISIBLE, SEM_UNKNOWN_NAME,
};
use nexa_project::{ModulePath, ParsedModule, ParsedProject, ResolverConfig};
use nexa_source::{SourceId, SourceSpan};
use nexa_symbols::prelude::{PreludeNameKind, PRELUDE_NAMES};
use nexa_symbols::{
    ModuleId, NameId, PackageInstanceId, ScopeId, SymbolId, SymbolKind, Visibility,
};
use std::collections::HashMap;

/// Resultado de um pass de resolução.
pub struct ResolveResult {
    pub index: SemanticIndex,
    pub diagnostics: Vec<ResolveDiagnostic>,
}

/// Diagnostic produzido durante a resolução.
#[derive(Debug, Clone)]
pub struct ResolveDiagnostic {
    pub code: &'static str,
    pub message: String,
    pub span: SourceSpan,
    pub severity: DiagnosticSeverity,
    pub context: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Note,
}

/// Política de shadowing (tabela interna, NUNCA configurável por usuário; §475-480).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShadowingPolicy {
    /// Baseline NEXA 1.0 (§135-148, R0021): um binding local/parameter não
    /// pode shadowar outro binding local/parameter na mesma ancestry do callable.
    #[default]
    NoLocalShadowingWithinCallable,
}

/// Contexto de um caminho qualificado (§481-482).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PathContext {
    Type,
    Value,
}

/// Cursor de resolução dentro de um module ou owner associado.
enum Cursor {
    Module(ModuleId),
    Symbol(SymbolId),
}

/// O resolver.
pub struct Resolver {
    index: SemanticIndex,
    diagnostics: Vec<ResolveDiagnostic>,
    current_package: PackageInstanceId,
    /// Alias de dependência direta (alias → package instance).
    dependency_aliases: Vec<(String, PackageInstanceId)>,
    /// Todos os packages do projeto (logical_name → instance) p/ detecção de transitiva.
    package_instances: HashMap<String, PackageInstanceId>,
    /// Prelude.
    prelude_module: Option<ModuleId>,
    prelude_types: HashMap<NameId, SymbolId>,
    prelude_values: HashMap<NameId, SymbolId>,
    /// Política interna de shadowing.
    policy: ShadowingPolicy,
    /// Owner resolvido por span de impl (decisão de paridade entre declarações do mesmo item):
    impl_owners: HashMap<SourceSpan, Option<SymbolId>>,
    /// Generic/type scope por span de impl.
    impl_scopes: HashMap<SourceSpan, ScopeId>,
    /// Generic scope com Type entries por owner symbol.
    type_scopes: HashMap<SymbolId, ScopeId>,
}

impl Resolver {
    pub fn new() -> Self {
        let mut resolver = Resolver {
            index: SemanticIndex::new(),
            diagnostics: Vec::new(),
            current_package: PackageInstanceId(0),
            dependency_aliases: Vec::new(),
            package_instances: HashMap::new(),
            prelude_module: None,
            prelude_types: HashMap::new(),
            prelude_values: HashMap::new(),
            policy: ShadowingPolicy::default(),
            impl_owners: HashMap::new(),
            impl_scopes: HashMap::new(),
            type_scopes: HashMap::new(),
        };
        resolver.register_prelude();
        resolver
    }

    pub fn into_result(mut self) -> ResolveResult {
        self.index
            .set_prelude(self.prelude_module, self.prelude_types, self.prelude_values);
        ResolveResult {
            index: self.index,
            diagnostics: self.diagnostics,
        }
    }

    pub fn index(&self) -> &SemanticIndex {
        &self.index
    }

    pub fn index_mut(&mut self) -> &mut SemanticIndex {
        &mut self.index
    }

    pub fn diagnostics(&self) -> &[ResolveDiagnostic] {
        &self.diagnostics
    }

    // ─── Prelude (synthetic package/module index, §173-180) ───────

    fn register_prelude(&mut self) {
        let mod_id = self.index.modules.next_id();
        let root_scope = self.index.scopes.add_scope(
            ScopeKind::Module,
            None,
            mod_id,
            SourceSpan::new(SourceId(0), 0, 0),
        );
        let path = ModulePath::new(vec!["nexa_prelude".to_string()]);
        self.index.modules.insert(ModuleEntry {
            id: mod_id,
            name: "nexa_prelude".to_string(),
            package: self.current_package,
            path,
            root_scope,
            source_id: None,
            span: SourceSpan::new(SourceId(0), 0, 0),
            children: Vec::new(),
            parent: None,
            is_public: true,
            symbol: None,
        });
        self.prelude_module = Some(mod_id);

        // Tipos do Prelude.
        let mut optional = None;
        let mut result_t = None;
        for entry in PRELUDE_NAMES {
            let kind = match entry.kind {
                PreludeNameKind::Type
                    if matches!(
                        entry.name,
                        "Copy"
                            | "Clone"
                            | "Eq"
                            | "Hash"
                            | "Comparable"
                            | "Ordering"
                            | "Display"
                            | "Send"
                            | "Share"
                    ) =>
                {
                    SymbolKind::Interface
                }
                PreludeNameKind::Type => SymbolKind::Struct,
                PreludeNameKind::Value => match entry.name {
                    "Some" | "None" | "Success" | "Failure" => SymbolKind::EnumVariant,
                    _ => SymbolKind::Function,
                },
            };
            let sym_id =
                self.spawn_symbol(entry.name, kind, mod_id, root_scope, Visibility::Public);
            let name_id = self.intern(entry.name);
            match (entry.kind, entry.name) {
                (PreludeNameKind::Type, "Optional") => optional = Some(sym_id),
                (PreludeNameKind::Type, "Result") => result_t = Some(sym_id),
                (PreludeNameKind::Type, _) => {}
                (PreludeNameKind::Value, _) => {
                    self.prelude_values.insert(name_id, sym_id);
                    // Construtores especiais de Optional/Result (§197, §231).
                    let owner = match entry.name {
                        "Some" | "None" => optional,
                        "Success" | "Failure" => result_t,
                        _ => None,
                    };
                    if let Some(owner) = owner {
                        self.index_mut()
                            .associated
                            .insert_value(owner, name_id, sym_id);
                        if let Some(sym) = self.index.symbols.get_mut(sym_id) {
                            sym.owner = Some(owner);
                        }
                    }
                }
            }
            if entry.kind == PreludeNameKind::Type {
                self.prelude_types.insert(name_id, sym_id);
            }
        }

        // `Console::write` is a synthetic standard-runtime action until the
        // package-backed standard library is linked by the project system.
        let console_name = self.intern("Console");
        if let Some(console) = self.prelude_types.get(&console_name).copied() {
            let write_name = self.intern("write");
            let write = self.spawn_symbol(
                "write",
                SymbolKind::Action,
                mod_id,
                root_scope,
                Visibility::Public,
            );
            self.index_mut()
                .associated
                .insert_value(console, write_name, write);
            if let Some(symbol) = self.index.symbols.get_mut(write) {
                symbol.owner = Some(console);
            }
        }
    }

    // ─── Registro de modules ──────────────────────────────────────

    fn register_module_entry(
        &mut self,
        package: PackageInstanceId,
        path: ModulePath,
        source_id: Option<SourceId>,
        span: SourceSpan,
        is_public: bool,
    ) -> ModuleId {
        // Duplicate path no mesmo package = projeto inválido (§99, NEXA-MODULE-0001).
        if let Some(existing) = self.index.modules.find_by_package_path(package, &path) {
            self.diagnostic(
                MODULE_DUPLICATE_MODULE.as_str(),
                span,
                format!("duplicate module '{}' in package", path.display()),
                Some(format!("previously registered as module#{}", existing.0)),
            );
            return existing;
        }
        let name = path.last_segment().unwrap_or("mod").to_string();
        let mod_id = self.index.modules.next_id();
        let root_scope = self
            .index
            .scopes
            .add_scope(ScopeKind::Module, None, mod_id, span);
        let symbol = self.spawn_symbol(&name, SymbolKind::Module, mod_id, root_scope, {
            if is_public {
                Visibility::Public
            } else {
                Visibility::PackageInternal
            }
        });
        // O symbol do module fica registrado (não no scope) como alvo de
        // referências uniformes (§328-330).
        self.index.modules.insert(ModuleEntry {
            id: mod_id,
            name,
            package,
            path,
            root_scope,
            source_id,
            span,
            children: Vec::new(),
            parent: None,
            is_public,
            symbol: Some(symbol),
        });
        mod_id
    }

    /// Registra todos os modules do projeto (ordem determinística).
    fn index_modules<'a>(
        &mut self,
        project: &'a ParsedProject,
    ) -> Vec<(ModuleId, &'a ParsedModule)> {
        self.current_package = project.current_package;
        self.package_instances = project
            .packages
            .iter()
            .map(|p| (p.instance.logical_name.clone(), p.instance.id))
            .collect();
        let mut registered: Vec<(ModuleId, &ParsedModule)> = Vec::new();
        for pm in project.modules_sorted() {
            let mod_id = self.register_module_entry(
                pm.package,
                pm.module_path.clone(),
                Some(pm.source_id),
                pm.ast.span,
                pm.is_public,
            );
            registered.push((mod_id, pm));
        }
        // Hierarquia parent/children por prefixo de path (mesmo package).
        for (mod_id, _) in &registered {
            let entry = self.index.modules.get(*mod_id).unwrap();
            let segs = &entry.path.segments;
            if segs.len() > 1 {
                let parent_path = ModulePath::new(segs[..segs.len() - 1].to_vec());
                if let Some(parent) = self
                    .index
                    .modules
                    .find_by_package_path(entry.package, &parent_path)
                {
                    if let Some(pentry) = self.index.modules.get_mut(parent) {
                        pentry.children.push(*mod_id);
                    }
                    if let Some(mentry) = self.index.modules.get_mut(*mod_id) {
                        mentry.parent = Some(parent);
                    }
                }
            }
        }
        registered
    }

    // ─── API pública de registro (baseline, usada por harness) ────

    pub fn register_root_module(
        &mut self,
        name: &str,
        source_id: SourceId,
        span: SourceSpan,
    ) -> ModuleId {
        self.register_module_entry(
            self.current_package,
            ModulePath::new(vec![name.to_string()]),
            Some(source_id),
            span,
            true,
        )
    }

    pub fn register_submodule(
        &mut self,
        name: &str,
        parent: ModuleId,
        span: SourceSpan,
    ) -> ModuleId {
        let parent_entry = self.index.modules.get(parent).unwrap();
        let mut segments = parent_entry.path.segments.clone();
        segments.push(name.to_string());
        let mod_id = self.register_module_entry(
            parent_entry.package,
            ModulePath::new(segments),
            None,
            span,
            parent_entry.is_public,
        );
        if let Some(pentry) = self.index.modules.get_mut(parent) {
            pentry.children.push(mod_id);
        }
        if let Some(mentry) = self.index.modules.get_mut(mod_id) {
            mentry.parent = Some(parent);
        }
        mod_id
    }

    /// Symbol de module no scope do parent (usado por `collect_module_header`).
    fn spawn_module_symbol(
        &mut self,
        name: &str,
        parent: ModuleId,
        visibility: Visibility,
        span: SourceSpan,
        child_module: ModuleId,
    ) -> SymbolId {
        let parent_scope = self.index.modules.get(parent).unwrap().root_scope;
        let sym_id = self.declare_header(
            name,
            SymbolKind::Module,
            parent,
            parent_scope,
            visibility,
            span,
            Some(span),
            None,
        );
        if let Some(sym) = self.index.symbols.get_mut(sym_id) {
            sym.child_module = Some(child_module);
        }
        sym_id
    }

    pub fn collect_module_header(
        &mut self,
        name: &str,
        parent: ModuleId,
        visibility: Visibility,
        span: SourceSpan,
    ) -> (SymbolId, ModuleId) {
        let child_mod = self.register_submodule(name, parent, span);
        let sym_id = self.spawn_module_symbol(name, parent, visibility, span, child_mod);
        (sym_id, child_mod)
    }

    /// Cria um symbol bruto (com registry na tabela, sem bind de scope).
    fn spawn_symbol(
        &mut self,
        name: &str,
        kind: SymbolKind,
        module: ModuleId,
        scope: ScopeId,
        visibility: Visibility,
    ) -> SymbolId {
        let name_id = self.intern(name);
        let sym_id = self.index.symbols.next_id();
        let sym = SymbolData {
            id: sym_id,
            name: name_id,
            kind,
            module,
            scope,
            visibility,
            span: self
                .index
                .scopes
                .scope(scope)
                .map(|s| s.span)
                .unwrap_or_else(|| SourceSpan::new(SourceId(0), 0, 0)),
            name_span: None,
            owner: None,
            child_module: None,
            body_scope: None,
            body_resolved: false,
        };
        self.index.symbols.insert(sym);
        sym_id
    }

    /// Declara header em namespace do scope, com keep-first em duplicados (§360-364)
    /// e conflito de Prelude em module-level (§188).
    #[allow(clippy::too_many_arguments)]
    fn declare_header(
        &mut self,
        name: &str,
        kind: SymbolKind,
        module: ModuleId,
        scope: ScopeId,
        visibility: Visibility,
        span: SourceSpan,
        name_span: Option<SourceSpan>,
        owner: Option<SymbolId>,
    ) -> SymbolId {
        let name_id = self.intern(name);
        let sym_id = self.spawn_symbol(name, kind, module, scope, visibility);
        if let Some(sym) = self.index.symbols.get_mut(sym_id) {
            sym.span = span;
            sym.name_span = name_span.or(Some(span));
            sym.owner = owner;
        }

        let is_module_scope = matches!(
            self.index.scopes.scope(scope).map(|s| s.kind),
            Some(ScopeKind::Module)
        );
        // Conflito com Prelude: reservado no mesmo namespace apenas em module-level.
        if is_module_scope && self.prelude_conflict(name_id, kind) {
            self.diagnostic(
                SEM_PRELUDE_NAME_CONFLICT.as_str(),
                span,
                format!(
                    "name '{}' is reserved by the NEXA Prelude in this namespace",
                    name
                ),
                Some(format!(
                    "namespace={}",
                    kind.default_namespace()[0].as_str()
                )),
            );
            return sym_id;
        }

        for &kind_ns in kind.default_namespace() {
            let choice = namespace_choice(kind_ns);
            let old = self
                .index
                .scopes
                .scope(scope)
                .and_then(|s| s.lookup_local(name_id, choice));
            if let Some(old) = old {
                let old_name = self
                    .index
                    .symbols
                    .get(old)
                    .map(|d| self.index.interner.resolve(d.name))
                    .unwrap_or("?");
                self.diagnostic(
                    SEM_DUPLICATE_DECLARATION.as_str(),
                    span,
                    format!(
                        "duplicate declaration of '{}' in {} namespace",
                        name,
                        kind_ns.as_str()
                    ),
                    Some(format!(
                        "previous declaration is '{}' (symbol#{})",
                        old_name, old.0
                    )),
                );
                return sym_id; // keep-first: não insere
            }
            if let Some(scope) = self.index.scopes.scope_mut(scope) {
                scope.insert(name_id, sym_id, choice);
            }
        }
        sym_id
    }

    /// Tem um nome do Prelude no mesmo namespace da declaração?
    fn prelude_conflict(&self, name: NameId, kind: SymbolKind) -> bool {
        kind.default_namespace().iter().any(|ns| match ns {
            nexa_symbols::namespace::NamespaceKind::Type => self.prelude_types.contains_key(&name),
            nexa_symbols::namespace::NamespaceKind::Value => {
                self.prelude_values.contains_key(&name)
            }
            nexa_symbols::namespace::NamespaceKind::Module => false,
        })
    }

    /// Declara um local/parameter (em qualquer scope). Aplica policy de
    /// shadowing e duplicates.
    fn declare_local(
        &mut self,
        name: &str,
        kind: SymbolKind,
        module: ModuleId,
        scope: ScopeId,
        span: SourceSpan,
        name_span: Option<SourceSpan>,
    ) -> SymbolId {
        let name_id = self.intern(name);
        // Duplicado no mesmo scope → SEM-0002.
        let duplicated = self
            .index
            .scopes
            .scope(scope)
            .and_then(|s| s.lookup_local(name_id, NamespaceChoice::Value));
        if let Some(old) = duplicated {
            self.diagnostic(
                SEM_DUPLICATE_DECLARATION.as_str(),
                span,
                format!("duplicate declaration of '{}' in this scope", name),
                Some(format!("previous declaration is symbol#{}", old.0)),
            );
            return self.spawn_symbol(name, kind, module, scope, Visibility::ModulePrivate);
        }
        // Shadowing policy: qualquer ancestor dentro do mesmo callable já
        // contém local/parameter com o mesmo nome → SEM-0006 (§135-148).
        if matches!(self.policy, ShadowingPolicy::NoLocalShadowingWithinCallable) {
            for ancestor in self.index.scopes.ancestors(scope) {
                if matches!(ancestor.kind, ScopeKind::Module) {
                    break;
                }
                if let Some(sym) = ancestor.lookup_local(name_id, NamespaceChoice::Value) {
                    let kind = self.index.symbols.get(sym).map(|d| d.kind);
                    let is_local = matches!(
                        kind,
                        Some(SymbolKind::Parameter)
                            | Some(SymbolKind::LocalLet)
                            | Some(SymbolKind::LocalVar)
                            | Some(SymbolKind::LocalConst)
                            | Some(SymbolKind::Receiver)
                            | Some(SymbolKind::ContractResult)
                    );
                    if is_local {
                        self.diagnostic(
                            SEM_INVALID_SHADOWING.as_str(),
                            span,
                            format!(
                                "invalid shadowing: '{}' already exists as a local/parameter in this callable",
                                name
                            ),
                            Some(format!("existing binding is symbol#{}", sym.0)),
                        );
                        break;
                    }
                    // Sombra de module/prelude value é permitida (§144-148).
                    break;
                }
            }
        }
        let sym_id = self.spawn_symbol(name, kind, module, scope, Visibility::ModulePrivate);
        if let Some(sym) = self.index.symbols.get_mut(sym_id) {
            sym.span = span;
            sym.name_span = name_span.or(Some(span));
        }
        if let Some(scope) = self.index.scopes.scope_mut(scope) {
            scope.insert(name_id, sym_id, NamespaceChoice::Value);
        }
        sym_id
    }

    /// Declara um generic param (Type namespace). Duplicado no mesmo scope → SEM-0002.
    fn declare_generic(
        &mut self,
        name: &str,
        module: ModuleId,
        scope: ScopeId,
        span: SourceSpan,
        name_span: Option<SourceSpan>,
    ) -> SymbolId {
        let name_id = self.intern(name);
        if let Some(scope) = self.index.scopes.scope(scope) {
            if let Some(old) = scope.lookup_local(name_id, NamespaceChoice::Type) {
                self.diagnostic(
                    SEM_DUPLICATE_DECLARATION.as_str(),
                    span,
                    format!("duplicate generic parameter '{}'", name),
                    Some(format!("previous generic parameter is symbol#{}", old.0)),
                );
            }
        }
        let sym_id = self.spawn_symbol(
            name,
            SymbolKind::GenericParameter,
            module,
            scope,
            Visibility::ModulePrivate,
        );
        if let Some(sym) = self.index.symbols.get_mut(sym_id) {
            sym.span = span;
            sym.name_span = name_span.or(Some(span));
        }
        if let Some(scope) = self.index.scopes.scope_mut(scope) {
            scope.insert(name_id, sym_id, NamespaceChoice::Type);
        }
        sym_id
    }

    /// `collect_header` — API baseline (manter assinatura; agora keep-first).
    pub fn collect_header(
        &mut self,
        name: &str,
        kind: SymbolKind,
        module: ModuleId,
        scope: ScopeId,
        visibility: Visibility,
        span: SourceSpan,
    ) -> SymbolId {
        self.declare_header(
            name,
            kind,
            module,
            scope,
            visibility,
            span,
            Some(span),
            None,
        )
    }

    pub fn create_body_scope(
        &mut self,
        parent_scope: ScopeId,
        module: ModuleId,
        span: SourceSpan,
    ) -> ScopeId {
        self.index
            .scopes
            .add_scope(ScopeKind::Callable, Some(parent_scope), module, span)
    }

    pub fn collect_local(
        &mut self,
        name: &str,
        kind: SymbolKind,
        module: ModuleId,
        scope: ScopeId,
        span: SourceSpan,
    ) -> SymbolId {
        self.declare_local(name, kind, module, scope, span, Some(span))
    }

    // ─── Interner ─────────────────────────────────────────────────

    fn intern(&mut self, name: &str) -> NameId {
        self.index.interner.intern(name)
    }

    fn module_scope(&self, module: ModuleId) -> Option<ScopeId> {
        self.index.scopes.module_scope(module)
    }

    fn module(&self, module: ModuleId) -> &ModuleEntry {
        self.index.modules.get(module).expect("module registered")
    }

    fn symbol(&self, id: SymbolId) -> Option<&SymbolData> {
        self.index.symbols.get(id)
    }

    fn diagnostic(
        &mut self,
        code: &'static str,
        span: SourceSpan,
        message: String,
        context: Option<String>,
    ) {
        self.diagnostics.push(ResolveDiagnostic {
            code,
            message,
            span,
            severity: DiagnosticSeverity::Error,
            context,
        });
    }

    // ─── Name resolution ──────────────────────────────────────────

    pub fn resolve_name(
        &mut self,
        name: &str,
        scope_id: ScopeId,
        ns: NamespaceChoice,
    ) -> Option<SymbolId> {
        let name_id = self.intern(name);
        self.resolve_name_by_id(name_id, scope_id, ns)
    }

    pub fn resolve_name_by_id(
        &mut self,
        name_id: NameId,
        scope_id: ScopeId,
        ns: NamespaceChoice,
    ) -> Option<SymbolId> {
        let accessor_module = self.index.scopes.scope(scope_id).map(|s| s.module)?;
        for scope in self.index.scopes.ancestors(scope_id) {
            if let Some(sym) = scope.lookup_local(name_id, ns) {
                let sym_data = self.index.symbols.get(sym)?;
                let accessor_package = self
                    .index
                    .modules
                    .get(accessor_module)
                    .map(|m| m.package)
                    .unwrap_or(self.current_package);
                if self.is_visible(sym_data, accessor_module, accessor_package) {
                    return Some(sym);
                }
            }
        }
        // Fallback Prelude (§177-178, §510-511).
        match ns {
            NamespaceChoice::Type => self.prelude_types.get(&name_id).copied(),
            NamespaceChoice::Value => self.prelude_values.get(&name_id).copied(),
            NamespaceChoice::Module => None,
        }
    }

    fn is_visible(
        &self,
        sym: &SymbolData,
        accessor_module: ModuleId,
        accessor_package: PackageInstanceId,
    ) -> bool {
        let decl_package = self
            .index
            .modules
            .get(sym.module)
            .map(|m| m.package)
            .unwrap_or(accessor_package);
        sym.visibility.is_accessible_from(
            sym.module,
            decl_package,
            accessor_module,
            accessor_package,
        )
    }

    pub fn record_reference(
        &mut self,
        symbol: SymbolId,
        scope: ScopeId,
        kind: ReferenceKind,
        span: SourceSpan,
    ) {
        self.index.references.insert(SymbolReference {
            symbol,
            scope,
            span,
            kind,
        });
        self.index.resolutions.insert(span, symbol);
    }

    // ─── Qualified path (module exports + associated) ─────────────

    pub fn resolve_qualified(&mut self, segments: &[&str], scope_id: ScopeId) -> Option<SymbolId> {
        if segments.is_empty() {
            return None;
        }
        let spans: Vec<SourceSpan> = segments
            .iter()
            .map(|_| {
                self.index
                    .scopes
                    .scope(scope_id)
                    .map(|s| s.span)
                    .unwrap_or_else(|| SourceSpan::new(SourceId(0), 0, 0))
            })
            .collect();
        // Tenta value context primeiro; depois type (para caminhos de tipo).
        self.resolve_path(segments, &spans, scope_id, PathContext::Value)
            .or_else(|| self.resolve_path(segments, &spans, scope_id, PathContext::Type))
    }

    fn accessor_info(&self, scope_id: ScopeId) -> (ModuleId, PackageInstanceId) {
        let m = self
            .index
            .scopes
            .scope(scope_id)
            .map(|s| s.module)
            .unwrap_or(ModuleId(0));
        let p = self
            .index
            .modules
            .get(m)
            .map(|e| e.package)
            .unwrap_or(self.current_package);
        (m, p)
    }

    fn resolve_path(
        &mut self,
        segments: &[&str],
        spans: &[SourceSpan],
        scope_id: ScopeId,
        ctx: PathContext,
    ) -> Option<SymbolId> {
        let (_accessor_module, _accessor_package) = self.accessor_info(scope_id);
        let first = segments[0];
        let first_id = self.intern(first);
        let first_span = spans[0];

        // 1. Módulo (import alias / module symbol) como raiz → cursor module.
        if let Some(sym) = self.resolve_name_by_id(first_id, scope_id, NamespaceChoice::Module) {
            self.record_reference(sym, scope_id, ReferenceKind::Import, first_span);
            if let Some(m) = self.symbol(sym).and_then(|d| d.child_module) {
                if segments.len() == 1 {
                    return Some(sym);
                }
                return self.resolve_in_module(sym, m, &segments[1..], &spans[1..], scope_id, ctx);
            }
        }

        // 2. Tipo como raiz (associated members: Type::Variant / Type::function).
        if let Some(type_sym) = self.resolve_name_by_id(first_id, scope_id, NamespaceChoice::Type) {
            self.record_reference(type_sym, scope_id, ReferenceKind::Type, first_span);
            if segments.len() == 1 {
                return Some(type_sym);
            }
            return self.resolve_through_owner(
                type_sym,
                &segments[1..],
                &spans[1..],
                scope_id,
                ctx,
            );
        }

        // 3. Value-only root → InvalidQualifiedPath (§534).
        if self
            .resolve_name_by_id(first_id, scope_id, NamespaceChoice::Value)
            .is_some()
        {
            self.diagnostic(
                SEM_INVALID_QUALIFIED_PATH.as_str(),
                first_span,
                format!("'{}' does not have an associated namespace for '::'", first),
                None,
            );
            return None;
        }

        // 4. Raiz desconhecida → UnknownName.
        self.diagnostic(
            SEM_UNKNOWN_NAME.as_str(),
            first_span,
            format!("unknown name '{}'", first),
            Some(format!("namespace={:?}", ctx_ns(ctx))),
        );
        None
    }

    /// Caminho dentro de um module alvo (import bound: um único module, §388).
    fn resolve_in_module(
        &mut self,
        alias_sym: SymbolId,
        module: ModuleId,
        segments: &[&str],
        spans: &[SourceSpan],
        accessor_scope: ScopeId,
        ctx: PathContext,
    ) -> Option<SymbolId> {
        let (accessor_module, accessor_package) = self.accessor_info(accessor_scope);
        let mut cursor = Cursor::Module(module);
        for (idx, (&name, &span)) in segments.iter().zip(spans.iter()).enumerate() {
            let last = idx == segments.len() - 1;
            let name_id = self.intern(name);
            match cursor {
                Cursor::Module(m) => {
                    let mod_scope = self.module_scope(m)?;
                    // (a) submodule (somente se um symbol de child module existir)
                    if let Some(sym) = self.lookup_in_module(
                        m,
                        mod_scope,
                        name_id,
                        NamespaceChoice::Module,
                        accessor_module,
                        accessor_package,
                        span,
                    ) {
                        self.record_reference(sym, accessor_scope, ReferenceKind::Import, span);
                        if let Some(child) = self.symbol(sym).and_then(|d| d.child_module) {
                            cursor = Cursor::Module(child);
                            continue;
                        }
                        if last {
                            return Some(sym);
                        }
                        continue;
                    }
                    // (b) raiz de tipo (associated) dentro do module
                    if let Some(tsym) = self.lookup_in_module(
                        m,
                        mod_scope,
                        name_id,
                        NamespaceChoice::Type,
                        accessor_module,
                        accessor_package,
                        span,
                    ) {
                        self.record_reference(tsym, accessor_scope, ReferenceKind::Type, span);
                        if last {
                            return Some(tsym);
                        }
                        cursor = Cursor::Symbol(tsym);
                        continue;
                    }
                    // (c) value alvo (somente último segmento)
                    if last {
                        if let Some(vsym) = self.lookup_in_module(
                            m,
                            mod_scope,
                            name_id,
                            NamespaceChoice::Value,
                            accessor_module,
                            accessor_package,
                            span,
                        ) {
                            self.record_reference(vsym, accessor_scope, ReferenceKind::Read, span);
                            return Some(vsym);
                        }
                    }
                    // (d) sem associação
                    if last {
                        self.diagnostic(
                            SEM_UNKNOWN_NAME.as_str(),
                            span,
                            format!(
                                "'{}' was not found in module '{}'",
                                name,
                                self.module(m).path.display()
                            ),
                            Some(format!("namespace={:?}", ctx_ns(ctx))),
                        );
                    } else {
                        self.diagnostic(
                            SEM_INVALID_QUALIFIED_PATH.as_str(),
                            span,
                            format!("'{}' is not a module or associated root", name),
                            None,
                        );
                    }
                    return None;
                }
                Cursor::Symbol(owner) => {
                    if let Some(member) = self.index.associated.lookup_value(owner, name_id) {
                        self.record_reference(member, accessor_scope, ReferenceKind::Read, span);
                        if last {
                            return Some(member);
                        }
                        let _ = alias_sym;
                        self.diagnostic(
                            SEM_INVALID_QUALIFIED_PATH.as_str(),
                            span,
                            format!("'{}' cannot be nested further", name),
                            None,
                        );
                        return None;
                    }
                    self.diagnostic(
                        SEM_UNKNOWN_NAME.as_str(),
                        span,
                        format!("'{}' was not found in this associated member set", name),
                        None,
                    );
                    return None;
                }
            }
        }
        None
    }

    /// Resolve segmentos restantes através de um owner (Type::A::B).
    fn resolve_through_owner(
        &mut self,
        owner: SymbolId,
        segments: &[&str],
        spans: &[SourceSpan],
        scope_id: ScopeId,
        ctx: PathContext,
    ) -> Option<SymbolId> {
        let (accessor_module, accessor_package) = self.accessor_info(scope_id);
        let name = segments[0];
        let name_id = self.intern(name);
        let span = spans[0];
        if let Some(member) = self.index.associated.lookup_value(owner, name_id) {
            self.record_reference(member, scope_id, ReferenceKind::Read, span);
            if segments.len() > 1 {
                self.diagnostic(
                    SEM_INVALID_QUALIFIED_PATH.as_str(),
                    span,
                    format!("'{}' cannot be nested further", segments[1]),
                    None,
                );
                return None;
            }
            return Some(member);
        }
        // Resultado: nome não encontrado no owner.
        self.diagnostic(
            SEM_UNKNOWN_NAME.as_str(),
            span,
            format!("'{}' was not found in this type's associated members", name),
            Some(format!("namespace={:?}", ctx_ns(ctx))),
        );
        let _ = (accessor_module, accessor_package);
        None
    }

    /// Lookup em um module com checagem de visibilidade (§214-218).
    #[allow(clippy::too_many_arguments)]
    fn lookup_in_module(
        &mut self,
        module: ModuleId,
        scope: ScopeId,
        name_id: NameId,
        ns: NamespaceChoice,
        accessor_module: ModuleId,
        accessor_package: PackageInstanceId,
        span: SourceSpan,
    ) -> Option<SymbolId> {
        let target = self.index.scopes.scope(scope)?.lookup_local(name_id, ns)?;
        let data = self.index.symbols.get(target)?;
        if !self.is_visible(data, accessor_module, accessor_package) {
            self.diagnostic(
                SEM_SYMBOL_NOT_VISIBLE.as_str(),
                span,
                format!(
                    "symbol '{0}' is not visible from this module (visibility: {1})",
                    self.index.interner.resolve(name_id),
                    data.visibility.as_str()
                ),
                None,
            );
            let _ = module;
            return None;
        }
        Some(target)
    }

    // ─── Type resolution ──────────────────────────────────────────

    fn resolve_type(&mut self, ty: &Type, scope_id: ScopeId) {
        match &ty.kind {
            TypeKind::Unit => {}
            TypeKind::Path(q) => {
                let segs: Vec<&str> = q.segments.iter().map(|s| s.name.as_str()).collect();
                let spans: Vec<SourceSpan> = q.segments.iter().map(|s| s.span).collect();
                if segs.len() == 1 {
                    let name_id = self.intern(segs[0]);
                    if let Some(sym) =
                        self.resolve_name_by_id(name_id, scope_id, NamespaceChoice::Type)
                    {
                        self.record_reference(sym, scope_id, ReferenceKind::Type, spans[0]);
                    } else if segs[0] == "Self" {
                        self.diagnostic(
                            SEM_INVALID_SELF_TYPE_REFERENCE.as_str(),
                            spans[0],
                            "'Self' is only valid inside an interface or implement block"
                                .to_string(),
                            None,
                        );
                    } else {
                        self.diagnostic(
                            SEM_UNKNOWN_NAME.as_str(),
                            spans[0],
                            format!("unknown type '{}'", segs[0]),
                            Some("namespace=type".to_string()),
                        );
                    }
                } else {
                    self.resolve_path(&segs, &spans, scope_id, PathContext::Type);
                }
            }
            TypeKind::Generic { path, args } => {
                let segs: Vec<&str> = path.segments.iter().map(|s| s.name.as_str()).collect();
                let spans: Vec<SourceSpan> = path.segments.iter().map(|s| s.span).collect();
                if segs.len() == 1 {
                    let name_id = self.intern(segs[0]);
                    if let Some(sym) =
                        self.resolve_name_by_id(name_id, scope_id, NamespaceChoice::Type)
                    {
                        self.record_reference(sym, scope_id, ReferenceKind::Type, spans[0]);
                    } else {
                        self.diagnostic(
                            SEM_UNKNOWN_NAME.as_str(),
                            spans[0],
                            format!("unknown type '{}'", segs[0]),
                            Some("namespace=type".to_string()),
                        );
                    }
                } else {
                    self.resolve_path(&segs, &spans, scope_id, PathContext::Type);
                }
                for a in args {
                    self.resolve_type(a, scope_id);
                }
            }
            TypeKind::Array(inner)
            | TypeKind::Optional(inner)
            | TypeKind::Ref(inner)
            | TypeKind::RefMut(inner) => self.resolve_type(inner, scope_id),
            TypeKind::Result { ok, err } => {
                self.resolve_type(ok, scope_id);
                self.resolve_type(err, scope_id);
            }
            TypeKind::Tuple(items) => {
                for t in items {
                    self.resolve_type(t, scope_id);
                }
            }
            TypeKind::Function { params, ret } => {
                for p in params {
                    self.resolve_type(p, scope_id);
                }
                self.resolve_type(ret, scope_id);
            }
        }
    }

    // ─── Fases do projeto ─────────────────────────────────────────

    // Fase B: toplevel headers + generic/param scopes.
    fn phase_collect_top_level(&mut self, module: ModuleId, ast: &SourceUnit) {
        let root_scope = self.module_scope(module).unwrap();
        let module_public = self
            .index
            .modules
            .get(module)
            .map(|m| m.is_public)
            .unwrap_or(true);
        for item in &ast.items {
            let base = if item.exported {
                Visibility::PackageInternal
            } else {
                Visibility::ModulePrivate
            };
            let vis = if item.exported && module_public {
                Visibility::Public
            } else {
                base
            };
            match &item.kind {
                ItemKind::Struct(s) => {
                    let sym = self.declare_header(
                        &s.name.name,
                        SymbolKind::Struct,
                        module,
                        root_scope,
                        vis,
                        s.span,
                        Some(s.name.span),
                        None,
                    );
                    let generic_scope = self.index_mut().scopes.add_scope(
                        ScopeKind::GenericParameters,
                        Some(root_scope),
                        module,
                        s.span,
                    );
                    self.type_scopes.insert(sym, generic_scope);
                    for gp in &s.generic_params {
                        self.declare_generic(
                            &gp.name.name,
                            module,
                            generic_scope,
                            gp.span,
                            Some(gp.name.span),
                        );
                    }
                }
                ItemKind::Enum(e) => {
                    let sym = self.declare_header(
                        &e.name.name,
                        SymbolKind::Enum,
                        module,
                        root_scope,
                        vis,
                        e.span,
                        Some(e.name.span),
                        None,
                    );
                    let generic_scope = self.index_mut().scopes.add_scope(
                        ScopeKind::GenericParameters,
                        Some(root_scope),
                        module,
                        e.span,
                    );
                    self.type_scopes.insert(sym, generic_scope);
                    for gp in &e.generic_params {
                        self.declare_generic(
                            &gp.name.name,
                            module,
                            generic_scope,
                            gp.span,
                            Some(gp.name.span),
                        );
                    }
                }
                ItemKind::Interface(i) => {
                    let sym = self.declare_header(
                        &i.name.name,
                        SymbolKind::Interface,
                        module,
                        root_scope,
                        vis,
                        i.span,
                        Some(i.name.span),
                        None,
                    );
                    let generic_scope = self.index_mut().scopes.add_scope(
                        ScopeKind::Interface,
                        Some(root_scope),
                        module,
                        i.span,
                    );
                    self.type_scopes.insert(sym, generic_scope);
                    for gp in &i.generic_params {
                        self.declare_generic(
                            &gp.name.name,
                            module,
                            generic_scope,
                            gp.span,
                            Some(gp.name.span),
                        );
                    }
                    // SelfType owned by interface (§166, §168).
                    let self_sym = self.spawn_symbol(
                        "Self",
                        SymbolKind::SelfType,
                        module,
                        generic_scope,
                        Visibility::ModulePrivate,
                    );
                    if let Some(sd) = self.index.symbols.get_mut(self_sym) {
                        sd.owner = Some(sym);
                        sd.span = i.span;
                    }
                    let self_id = self.intern("Self");
                    if let Some(scope) = self.index_mut().scopes.scope_mut(generic_scope) {
                        scope.insert(self_id, self_sym, NamespaceChoice::Type);
                    }
                }
                ItemKind::Function(f) => {
                    let sym = self.declare_header(
                        &f.name.name,
                        SymbolKind::Function,
                        module,
                        root_scope,
                        vis,
                        f.span,
                        Some(f.name.span),
                        None,
                    );
                    self.build_callable_scope(
                        sym,
                        &f.generic_params,
                        &f.params,
                        module,
                        root_scope,
                        f.span,
                    );
                }
                ItemKind::Action(a) => {
                    let sym = self.declare_header(
                        &a.name.name,
                        SymbolKind::Action,
                        module,
                        root_scope,
                        vis,
                        a.span,
                        Some(a.name.span),
                        None,
                    );
                    self.build_callable_scope(
                        sym,
                        &a.generic_params,
                        &a.params,
                        module,
                        root_scope,
                        a.span,
                    );
                }
                ItemKind::Const(c) => {
                    self.declare_header(
                        &c.name.name,
                        SymbolKind::Const,
                        module,
                        root_scope,
                        vis,
                        c.span,
                        Some(c.name.span),
                        None,
                    );
                }
                ItemKind::TypeAlias(t) => {
                    let kind = if t.is_distinct {
                        SymbolKind::DistinctType
                    } else {
                        SymbolKind::TypeAlias
                    };
                    let sym = self.declare_header(
                        &t.name.name,
                        kind,
                        module,
                        root_scope,
                        vis,
                        t.span,
                        Some(t.name.span),
                        None,
                    );
                    let generic_scope = self.index_mut().scopes.add_scope(
                        ScopeKind::GenericParameters,
                        Some(root_scope),
                        module,
                        t.span,
                    );
                    self.type_scopes.insert(sym, generic_scope);
                    for gp in &t.generic_params {
                        self.declare_generic(
                            &gp.name.name,
                            module,
                            generic_scope,
                            gp.span,
                            Some(gp.name.span),
                        );
                    }
                }
                ItemKind::Implement(_) => {}
            }
        }
    }

    /// Cria o scope de callable (generic params em TYPE, params em VALUE, antes do body).
    fn build_callable_scope(
        &mut self,
        sym: SymbolId,
        generics: &[GenericParam],
        params: &[Param],
        module: ModuleId,
        parent_scope: ScopeId,
        span: SourceSpan,
    ) {
        let callable_scope = self.index_mut().scopes.add_scope(
            ScopeKind::Callable,
            Some(parent_scope),
            module,
            span,
        );
        for gp in generics {
            self.declare_generic(
                &gp.name.name,
                module,
                callable_scope,
                gp.span,
                Some(gp.name.span),
            );
        }
        for p in params {
            self.declare_local(
                &p.name.name,
                SymbolKind::Parameter,
                module,
                callable_scope,
                p.span,
                Some(p.name.span),
            );
        }
        if let Some(sd) = self.index.symbols.get_mut(sym) {
            sd.body_scope = Some(callable_scope);
            sd.body_resolved = true;
        }
    }

    // Fase C: imports.
    fn phase_resolve_imports(&mut self, module: ModuleId, ast: &SourceUnit) {
        let root_scope = self.module_scope(module).unwrap();
        let module_package = self.index.modules.get(module).map(|m| m.package).unwrap();
        for imp in &ast.imports {
            let segs: Vec<String> = imp.path.segments.iter().map(|s| s.name.clone()).collect();
            let alias = imp
                .alias
                .as_ref()
                .map(|a| a.name.clone())
                .unwrap_or_else(|| segs.last().cloned().unwrap_or_default());
            let alias_span = imp
                .alias
                .as_ref()
                .map(|a| a.span)
                .unwrap_or_else(|| imp.path.span);
            let target = self.resolve_import_target(module_package, module, &segs, imp.span);
            if let Some(target_module) = target {
                self.bind_import_alias(
                    module,
                    root_scope,
                    &alias,
                    alias_span,
                    target_module,
                    imp.span,
                );
            }
        }
    }

    /// Resolve o module alvo de um import (§93): `self::path`, alias de
    /// dependência, ou erro de alias desconhecido/transitivo.
    fn resolve_import_target(
        &mut self,
        module_package: PackageInstanceId,
        module: ModuleId,
        segs: &[String],
        span: SourceSpan,
    ) -> Option<ModuleId> {
        let first = segs.first()?;
        if first == "self" {
            if segs.len() < 2 {
                self.diagnostic(
                    MODULE_UNKNOWN_MODULE.as_str(),
                    span,
                    "`self::` requires a module path".to_string(),
                    None,
                );
                return None;
            }
            let target_path = ModulePath::new(segs[1..].to_vec());
            match self
                .index
                .modules
                .find_by_package_path(module_package, &target_path)
            {
                Some(t) => {
                    // Mesmo package: PackageInternal/Public ok (§95).
                    Some(t)
                }
                None => {
                    self.diagnostic(
                        MODULE_UNKNOWN_MODULE.as_str(),
                        span,
                        format!(
                            "module '{}' was not found in this package",
                            target_path.display()
                        ),
                        None,
                    );
                    None
                }
            }
        } else if let Some(&pkg) = self
            .dependency_aliases
            .iter()
            .find(|(a, _)| a == first)
            .map(|(_, p)| p)
        {
            // Dependência direta. Requer um module (pelo menos 1 segmento além do root).
            if segs.len() < 2 {
                self.diagnostic(
                    MODULE_UNKNOWN_MODULE.as_str(),
                    span,
                    format!("dependency alias '{}' does not name a module", first),
                    None,
                );
                return None;
            }
            let target_path = ModulePath::new(segs[1..].to_vec());
            match self.index.modules.find_by_package_path(pkg, &target_path) {
                Some(t) => {
                    if !self.index.modules.is_public(t) {
                        self.diagnostic(
                            MODULE_MODULE_NOT_VISIBLE.as_str(),
                            span,
                            format!(
                                "module '{}' in package '{}' is not public (cross-package import denied)",
                                target_path.display(),
                                self.index.modules.get(t).map(|m| m.package.0).unwrap_or(0)
                            ),
                            None,
                        );
                        return None;
                    }
                    Some(t)
                }
                None => {
                    self.diagnostic(
                        MODULE_UNKNOWN_MODULE.as_str(),
                        span,
                        format!(
                            "module '{}' was not found in package '{}'",
                            target_path.display(),
                            first
                        ),
                        None,
                    );
                    None
                }
            }
        } else if self.package_instances.contains_key(first) {
            // Existe no projeto mas não é dependência direta (§79, NEXA-MODULE-0007).
            self.diagnostic(
                MODULE_TRANSITIVE_DEPENDENCY_NOT_DIRECTLY_ACCESSIBLE.as_str(),
                span,
                format!(
                    "package '{}' is not a direct dependency of this package; declare it or use a direct dependency alias",
                    first
                ),
                None,
            );
            None
        } else {
            self.diagnostic(
                MODULE_UNKNOWN_DEPENDENCY_ALIAS.as_str(),
                span,
                format!("unknown dependency alias '{}'", first),
                None,
            );
            let _ = module;
            None
        }
    }

    /// Cria o binding de import alias (module namespace), com NEXA-MODULE-0005/0006.
    fn bind_import_alias(
        &mut self,
        module: ModuleId,
        root_scope: ScopeId,
        alias: &str,
        alias_span: SourceSpan,
        target: ModuleId,
        imp_span: SourceSpan,
    ) -> Option<SymbolId> {
        let name_id = self.intern(alias);
        // Duplicado no module namespace → NEXA-MODULE-0005.
        if let Some(scope) = self.index.scopes.scope(root_scope) {
            if scope
                .lookup_local(name_id, NamespaceChoice::Module)
                .is_some()
            {
                self.diagnostic(
                    MODULE_DUPLICATE_IMPORT_ALIAS.as_str(),
                    imp_span,
                    format!("duplicate import alias '{}'", alias),
                    None,
                );
                return None;
            }
        }
        // Conflito com type do module ou Prelude type → NEXA-MODULE-0006 (§522-531).
        let type_conflict = self
            .index
            .scopes
            .scope(root_scope)
            .map(|s| s.lookup_local(name_id, NamespaceChoice::Type).is_some())
            .unwrap_or(false)
            || self.prelude_types.contains_key(&name_id);
        if type_conflict {
            self.diagnostic(
                MODULE_IMPORT_NAME_CONFLICT.as_str(),
                imp_span,
                format!(
                    "import alias '{}' conflicts with a type name visible in this module",
                    alias
                ),
                None,
            );
            return None;
        }
        let sym_id = self.declare_header(
            alias,
            SymbolKind::ImportAlias,
            module,
            root_scope,
            Visibility::ModulePrivate,
            imp_span,
            Some(alias_span),
            None,
        );
        if let Some(sd) = self.index.symbols.get_mut(sym_id) {
            sd.child_module = Some(target);
        }
        // Referência de import: alias → module alvo (§324-327).
        if let Some(target_sym) = self.index.modules.get(target).and_then(|m| m.symbol) {
            self.record_reference(target_sym, root_scope, ReferenceKind::Import, alias_span);
        }
        self.record_reference(sym_id, root_scope, ReferenceKind::Import, alias_span);
        Some(sym_id)
    }

    // Fase D: implement targets + Self.
    fn phase_implement_targets(&mut self, module: ModuleId, ast: &SourceUnit) {
        let root_scope = self.module_scope(module).unwrap();
        for item in &ast.items {
            if let ItemKind::Implement(imp) = &item.kind {
                let impl_scope = self.index_mut().scopes.add_scope(
                    ScopeKind::Implement,
                    Some(root_scope),
                    module,
                    imp.span,
                );
                for gp in &imp.generic_params {
                    self.declare_generic(
                        &gp.name.name,
                        module,
                        impl_scope,
                        gp.span,
                        Some(gp.name.span),
                    );
                }
                self.impl_scopes.insert(imp.span, impl_scope);

                // Trait path (se presente) resolve no module/impl scope.
                if !imp.trait_path.segments.is_empty() {
                    let segs: Vec<&str> = imp
                        .trait_path
                        .segments
                        .iter()
                        .map(|s| s.name.as_str())
                        .collect();
                    let spans: Vec<SourceSpan> =
                        imp.trait_path.segments.iter().map(|s| s.span).collect();
                    self.resolve_path(&segs, &spans, impl_scope, PathContext::Type);
                }

                // Target `for_type` → owner.
                let owner = self.resolve_type_owner(&imp.for_type, impl_scope);
                self.impl_owners.insert(imp.span, owner);

                // Self → owner ou SelfType sintético (§166-169).
                let self_sym = self.spawn_symbol(
                    "Self",
                    SymbolKind::SelfType,
                    module,
                    impl_scope,
                    Visibility::ModulePrivate,
                );
                if let Some(sd) = self.index.symbols.get_mut(self_sym) {
                    sd.owner = owner;
                    sd.span = imp.span;
                }
                let self_id = self.intern("Self");
                if let Some(scope) = self.index_mut().scopes.scope_mut(impl_scope) {
                    scope.insert(self_id, self_sym, NamespaceChoice::Type);
                }
            }
        }
    }

    fn resolve_type_owner(&mut self, ty: &Type, scope_id: ScopeId) -> Option<SymbolId> {
        // Resolve o caminho principal do tipo e retorna o símbolo (sem diag duplicado).
        let path_segments = match &ty.kind {
            TypeKind::Path(q) => Some(&q.segments),
            TypeKind::Generic { path, .. } => Some(&path.segments),
            _ => None,
        }?;
        let segs: Vec<&str> = path_segments.iter().map(|s| s.name.as_str()).collect();
        if segs.len() == 1 {
            let name_id = self.intern(segs[0]);
            let r = self.resolve_name_by_id(name_id, scope_id, NamespaceChoice::Type);
            if let Some(sym) = r {
                self.record_reference(sym, scope_id, ReferenceKind::Type, path_segments[0].span);
                if is_path_like(&ty.kind) {
                    // resolve args se houver
                    if let TypeKind::Generic { args, .. } = &ty.kind {
                        for a in args {
                            self.resolve_type(a, scope_id);
                        }
                    }
                }
            }
            r
        } else {
            let spans: Vec<SourceSpan> = path_segments.iter().map(|s| s.span).collect();
            self.resolve_path(&segs, &spans, scope_id, PathContext::Type)
        }
    }

    // Fase E: associated members + self receiver.
    fn phase_collect_associated(&mut self, module: ModuleId, ast: &SourceUnit) {
        let root_scope = self.module_scope(module).unwrap();
        for item in &ast.items {
            match &item.kind {
                ItemKind::Struct(s) => {
                    if let Some(owner) =
                        self.resolve_name(&s.name.name, root_scope, NamespaceChoice::Type)
                    {
                        let generic_scope =
                            self.type_scopes.get(&owner).copied().unwrap_or(root_scope);
                        for f in &s.fields {
                            let f_sym = self.spawn_symbol(
                                &f.name.name,
                                SymbolKind::Field,
                                module,
                                generic_scope,
                                field_visibility(f),
                            );
                            if let Some(sd) = self.index.symbols.get_mut(f_sym) {
                                sd.owner = Some(owner);
                                sd.span = f.span;
                                sd.name_span = Some(f.name.span);
                            }
                            let f_nid = self.intern(&f.name.name);
                            if let Some(old) = self
                                .index_mut()
                                .associated
                                .insert_field(owner, f_nid, f_sym)
                            {
                                self.diagnostic(
                                    SEM_DUPLICATE_DECLARATION.as_str(),
                                    f.span,
                                    format!("duplicate field '{}' in struct", f.name.name),
                                    Some(format!("previous field is symbol#{}", old.0)),
                                );
                            }
                        }
                    }
                }
                ItemKind::Enum(e) => {
                    if let Some(owner) =
                        self.resolve_name(&e.name.name, root_scope, NamespaceChoice::Type)
                    {
                        let generic_scope =
                            self.type_scopes.get(&owner).copied().unwrap_or(root_scope);
                        for v in &e.variants {
                            let v_sym = self.spawn_symbol(
                                &v.name.name,
                                SymbolKind::EnumVariant,
                                module,
                                generic_scope,
                                Visibility::ModulePrivate,
                            );
                            if let Some(sd) = self.index.symbols.get_mut(v_sym) {
                                sd.owner = Some(owner);
                                sd.span = v.span;
                                sd.name_span = Some(v.name.span);
                            }
                            let v_nid = self.intern(&v.name.name);
                            if let Some(old) = self
                                .index_mut()
                                .associated
                                .insert_value(owner, v_nid, v_sym)
                            {
                                self.diagnostic(
                                    SEM_DUPLICATE_DECLARATION.as_str(),
                                    v.span,
                                    format!("duplicate variant '{}' in enum", v.name.name),
                                    Some(format!("previous variant is symbol#{}", old.0)),
                                );
                            }
                        }
                    }
                }
                ItemKind::Interface(i) => {
                    if let Some(owner) =
                        self.resolve_name(&i.name.name, root_scope, NamespaceChoice::Type)
                    {
                        let generic_scope =
                            self.type_scopes.get(&owner).copied().unwrap_or(root_scope);
                        for m in &i.methods {
                            let m_sym = self.spawn_symbol(
                                &m.name.name,
                                SymbolKind::Function,
                                module,
                                generic_scope,
                                Visibility::ModulePrivate,
                            );
                            if let Some(sd) = self.index.symbols.get_mut(m_sym) {
                                sd.owner = Some(owner);
                                sd.span = m.span;
                                sd.name_span = Some(m.name.span);
                            }
                            let m_nid = self.intern(&m.name.name);
                            if let Some(old) = self
                                .index_mut()
                                .associated
                                .insert_value(owner, m_nid, m_sym)
                            {
                                self.diagnostic(
                                    SEM_DUPLICATE_DECLARATION.as_str(),
                                    m.span,
                                    format!("duplicate method '{}' in interface", m.name.name),
                                    Some(format!("previous method is symbol#{}", old.0)),
                                );
                            }
                            // Scope de método (para sign/body futuras) com `self`.
                            let method_scope = self.index_mut().scopes.add_scope(
                                ScopeKind::Callable,
                                Some(generic_scope),
                                module,
                                m.span,
                            );
                            if has_receiver_iface(m) {
                                self.declare_local(
                                    "self",
                                    SymbolKind::Receiver,
                                    module,
                                    method_scope,
                                    m.span,
                                    None,
                                );
                            }
                            if let Some(sd) = self.index.symbols.get_mut(m_sym) {
                                sd.body_scope = Some(method_scope);
                                sd.body_resolved = true;
                            }
                        }
                    }
                }
                ItemKind::Implement(imp) => {
                    let owner = self.impl_owners.get(&imp.span).copied().flatten();
                    let impl_scope = self
                        .impl_scopes
                        .get(&imp.span)
                        .copied()
                        .unwrap_or(root_scope);
                    for method in &imp.methods {
                        let m_sym = if let Some(o) = owner {
                            let sym = self.spawn_symbol(
                                &method.name.name,
                                SymbolKind::Function,
                                module,
                                impl_scope,
                                Visibility::ModulePrivate,
                            );
                            if let Some(sd) = self.index.symbols.get_mut(sym) {
                                sd.owner = Some(o);
                                sd.span = method.span;
                                sd.name_span = Some(method.name.span);
                            }
                            let method_nid = self.intern(&method.name.name);
                            if let Some(old) =
                                self.index_mut().associated.insert_value(o, method_nid, sym)
                            {
                                self.diagnostic(
                                    SEM_DUPLICATE_DECLARATION.as_str(),
                                    method.span,
                                    format!("duplicate method '{}' on type", method.name.name),
                                    Some(format!("previous method is symbol#{}", old.0)),
                                );
                            }
                            sym
                        } else {
                            // Owner não resolvido: ainda cria símbolo órfão (partial index).
                            self.spawn_symbol(
                                &method.name.name,
                                SymbolKind::Function,
                                module,
                                impl_scope,
                                Visibility::ModulePrivate,
                            )
                        };
                        // Scope de método com receiver + params (§159-162).
                        let method_scope = self.index_mut().scopes.add_scope(
                            ScopeKind::Callable,
                            Some(impl_scope),
                            module,
                            method.span,
                        );
                        if has_receiver_impl(method) {
                            self.declare_local(
                                "self",
                                SymbolKind::Receiver,
                                module,
                                method_scope,
                                method.span,
                                None,
                            );
                        }
                        for p in &method.params {
                            self.declare_local(
                                &p.name.name,
                                SymbolKind::Parameter,
                                module,
                                method_scope,
                                p.span,
                                Some(p.name.span),
                            );
                        }
                        if let Some(sd) = self.index.symbols.get_mut(m_sym) {
                            sd.body_scope = Some(method_scope);
                            sd.body_resolved = true;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // Fase F: signatures.
    fn phase_resolve_signatures(&mut self, module: ModuleId, ast: &SourceUnit) {
        let root_scope = self.module_scope(module).unwrap();
        for item in &ast.items {
            match &item.kind {
                ItemKind::Function(f) => {
                    let callable_scope = self.symbol_lookup_scope(
                        &f.name.name,
                        module,
                        root_scope,
                        NamespaceChoice::Value,
                    );
                    self.resolve_callable_signature(
                        &f.params,
                        &f.return_type,
                        &f.where_clause,
                        callable_scope,
                    );
                }
                ItemKind::Action(a) => {
                    let callable_scope = self.symbol_lookup_scope(
                        &a.name.name,
                        module,
                        root_scope,
                        NamespaceChoice::Value,
                    );
                    self.resolve_callable_signature(
                        &a.params,
                        &a.return_type,
                        &a.where_clause,
                        callable_scope,
                    );
                }
                ItemKind::Const(c) => {
                    let scope = self.symbol_lookup_scope(
                        &c.name.name,
                        module,
                        root_scope,
                        NamespaceChoice::Value,
                    );
                    if let Some(ref t) = c.ty {
                        self.resolve_type(t, scope);
                    }
                }
                ItemKind::Struct(s) => {
                    if let Some(owner) =
                        self.resolve_name(&s.name.name, root_scope, NamespaceChoice::Type)
                    {
                        let generic_scope =
                            self.type_scopes.get(&owner).copied().unwrap_or(root_scope);
                        for f in &s.fields {
                            let f_nid = self.intern(&f.name.name);
                            if let Some(f_sym) = self.index.associated.lookup_field(owner, f_nid) {
                                if let Some(sd) = self.index.symbols.get_mut(f_sym) {
                                    sd.body_scope = Some(generic_scope);
                                }
                            }
                            self.resolve_type(&f.ty, generic_scope);
                        }
                    }
                }
                ItemKind::Enum(e) => {
                    if let Some(owner) =
                        self.resolve_name(&e.name.name, root_scope, NamespaceChoice::Type)
                    {
                        let generic_scope =
                            self.type_scopes.get(&owner).copied().unwrap_or(root_scope);
                        for v in &e.variants {
                            match &v.kind {
                                nexa_ast::EnumVariantKind::Unit => {}
                                nexa_ast::EnumVariantKind::Tuple(types) => {
                                    for t in types {
                                        self.resolve_type(t, generic_scope);
                                    }
                                }
                                nexa_ast::EnumVariantKind::Struct(fields) => {
                                    for f in fields {
                                        self.resolve_type(&f.ty, generic_scope);
                                    }
                                }
                            }
                        }
                    }
                }
                ItemKind::Interface(i) => {
                    if let Some(owner) =
                        self.resolve_name(&i.name.name, root_scope, NamespaceChoice::Type)
                    {
                        let generic_scope =
                            self.type_scopes.get(&owner).copied().unwrap_or(root_scope);
                        for m in &i.methods {
                            let m_nid = self.intern(&m.name.name);
                            let scope = self
                                .index
                                .associated
                                .lookup_value(owner, m_nid)
                                .and_then(|ms| self.index.symbols.get(ms))
                                .and_then(|sd| sd.body_scope)
                                .unwrap_or(generic_scope);
                            self.resolve_callable_signature(&m.params, &m.return_type, &[], scope);
                        }
                    }
                }
                ItemKind::Implement(imp) => {
                    let owner = self.impl_owners.get(&imp.span).copied().flatten();
                    let impl_scope = self
                        .impl_scopes
                        .get(&imp.span)
                        .copied()
                        .unwrap_or(root_scope);
                    for method in &imp.methods {
                        let scope = if let Some(o) = owner {
                            let m_nid = self.intern(&method.name.name);
                            self.index
                                .associated
                                .lookup_value(o, m_nid)
                                .and_then(|ms| self.index.symbols.get(ms))
                                .and_then(|sd| sd.body_scope)
                                .unwrap_or(impl_scope)
                        } else {
                            impl_scope
                        };
                        self.resolve_callable_signature(
                            &method.params,
                            &method.return_type,
                            &[],
                            scope,
                        );
                    }
                }
                ItemKind::TypeAlias(t) => {
                    if let Some(owner) =
                        self.resolve_name(&t.name.name, root_scope, NamespaceChoice::Type)
                    {
                        let generic_scope =
                            self.type_scopes.get(&owner).copied().unwrap_or(root_scope);
                        self.resolve_type(&t.ty, generic_scope);
                    }
                }
            }
        }
    }

    /// Scope do símbolo (body_scope) por nome no module.
    fn symbol_lookup_scope(
        &self,
        name: &str,
        module: ModuleId,
        root_scope: ScopeId,
        ns: NamespaceChoice,
    ) -> ScopeId {
        let mut found = root_scope;
        if let Some(scope) = self.index.scopes.scope(root_scope) {
            if let Some(sym) = scope.lookup_local(self.intern_ns(name), ns) {
                if let Some(sd) = self.index.symbols.get(sym) {
                    if let Some(bs) = sd.body_scope {
                        found = bs;
                    }
                }
            }
        }
        let _ = module;
        found
    }

    fn intern_ns(&self, name: &str) -> NameId {
        // Não mutável; intern no resolver é mutável. Usar apenas para lookup
        // com NameId pré-existente não é possível; fazemos lookup por string?
        // Manter método de conveniência: busca por nome nos maps deste scope.
        // Para isso precisamos do NameId — mas interner exige &mut self.
        // Solução: foo.
        self.name_id_fallback(name)
    }

    fn name_id_fallback(&self, name: &str) -> NameId {
        for id in self.index.interner.ids() {
            if self.index.interner.resolve(id) == name {
                return id;
            }
        }
        NameId(u32::MAX)
    }

    /// Resolve os tipos de params/return/where num scope.
    fn resolve_callable_signature(
        &mut self,
        params: &[Param],
        return_type: &Option<Type>,
        where_clause: &[WherePredicate],
        scope: ScopeId,
    ) {
        for p in params {
            self.resolve_type(&p.ty, scope);
        }
        if let Some(ref r) = return_type {
            self.resolve_type(r, scope);
        }
        for w in where_clause {
            for b in &w.bounds {
                let segs: Vec<&str> = b.path.segments.iter().map(|s| s.name.as_str()).collect();
                let spans: Vec<SourceSpan> = b.path.segments.iter().map(|s| s.span).collect();
                if segs.len() == 1 {
                    if let Some(sym) = self.resolve_name(segs[0], scope, NamespaceChoice::Type) {
                        self.record_reference(sym, scope, ReferenceKind::Type, spans[0]);
                    } else {
                        self.diagnostic(
                            SEM_UNKNOWN_NAME.as_str(),
                            spans[0],
                            format!("unknown type '{}'", segs[0]),
                            Some("namespace=type".to_string()),
                        );
                    }
                } else {
                    self.resolve_path(&segs, &spans, scope, PathContext::Type);
                }
                for ga in &b.generic_args {
                    self.resolve_type(ga, scope);
                }
            }
        }
    }

    // Fase G: bodies.
    fn phase_resolve_bodies(&mut self, module: ModuleId, ast: &SourceUnit) {
        let root_scope = self.module_scope(module).unwrap();
        for item in &ast.items {
            match &item.kind {
                ItemKind::Function(f) => {
                    if let Some(sym) = self.index.scopes.scope(root_scope).and_then(|s| {
                        s.lookup_local(self.name_id_fallback(&f.name.name), NamespaceChoice::Value)
                    }) {
                        self.resolve_contracts(&f.requires, &f.ensures, sym);
                        if let Some(body_scope) =
                            self.index.symbols.get(sym).and_then(|d| d.body_scope)
                        {
                            if let Some(block_scope) = self.push_block(body_scope, &f.body) {
                                self.resolve_block(&f.body, module, block_scope);
                            }
                        }
                    }
                }
                ItemKind::Action(a) => {
                    if let Some(sym) = self.index.scopes.scope(root_scope).and_then(|s| {
                        s.lookup_local(self.name_id_fallback(&a.name.name), NamespaceChoice::Value)
                    }) {
                        self.resolve_contracts(&a.requires, &a.ensures, sym);
                        if let Some(body_scope) =
                            self.index.symbols.get(sym).and_then(|d| d.body_scope)
                        {
                            if let Some(block_scope) = self.push_block(body_scope, &a.body) {
                                self.resolve_block(&a.body, module, block_scope);
                            }
                        }
                    }
                }
                ItemKind::Const(c) => {
                    if let Some(scope) = self.symbol_lookup_scope_or_root(
                        &c.name.name,
                        module,
                        root_scope,
                        NamespaceChoice::Value,
                    ) {
                        self.resolve_expr(&c.init, module, scope);
                    }
                }
                ItemKind::Implement(imp) => {
                    let owner = self.impl_owners.get(&imp.span).copied().flatten();
                    let impl_scope = self
                        .impl_scopes
                        .get(&imp.span)
                        .copied()
                        .unwrap_or(root_scope);
                    for method in &imp.methods {
                        let method_scope = if let Some(o) = owner {
                            self.index
                                .associated
                                .lookup_value(o, self.name_id_fallback(&method.name.name))
                                .and_then(|ms| self.index.symbols.get(ms))
                                .and_then(|sd| sd.body_scope)
                                .unwrap_or(impl_scope)
                        } else {
                            impl_scope
                        };
                        if let Some(ref body) = method.body {
                            if let Some(block_scope) = self.push_block(method_scope, body) {
                                self.resolve_block(body, module, block_scope);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn resolve_contracts(
        &mut self,
        requires: &[ContractExpr],
        ensures: &[ContractExpr],
        sym: SymbolId,
    ) {
        // Names em contract exprs resolvem normalmente (§422); `result`/`old` deferidos (§442).
        for c in requires.iter().chain(ensures.iter()) {
            let module = self.index.symbols.get(sym).map(|d| d.module);
            let scope = self.index.symbols.get(sym).and_then(|d| d.body_scope);
            if let (Some(m), Some(s)) = (module, scope) {
                self.resolve_expr(&c.expr, m, s);
            }
        }
    }

    /// Cria scope Block filho de `parent_scope` e retorna ele.
    fn push_block(&mut self, parent_scope: ScopeId, block: &Block) -> Option<ScopeId> {
        let module = self.index.scopes.scope(parent_scope).map(|s| s.module)?;
        Some(self.index_mut().scopes.add_scope(
            ScopeKind::Block,
            Some(parent_scope),
            module,
            block.span,
        ))
    }

    fn symbol_lookup_scope_or_root(
        &self,
        name: &str,
        module: ModuleId,
        root_scope: ScopeId,
        ns: NamespaceChoice,
    ) -> Option<ScopeId> {
        let scope = self.symbol_lookup_scope(name, module, root_scope, ns);
        Some(scope)
    }

    // ─── Statement/Expr resolution ────────────────────────────────

    fn resolve_block(&mut self, block: &Block, module: ModuleId, scope: ScopeId) {
        for stmt in &block.stmts {
            self.resolve_stmt(stmt, module, scope);
        }
    }

    fn resolve_stmt(&mut self, stmt: &Stmt, module: ModuleId, scope: ScopeId) {
        match &stmt.kind {
            StmtKind::Let(lb) => {
                if let Some(ref t) = lb.ty {
                    self.resolve_type(t, scope);
                }
                self.resolve_expr(&lb.init, module, scope);
                self.declare_local(
                    &lb.name.name,
                    SymbolKind::LocalLet,
                    module,
                    scope,
                    lb.span,
                    Some(lb.name.span),
                );
            }
            StmtKind::Var(vb) => {
                if let Some(ref t) = vb.ty {
                    self.resolve_type(t, scope);
                }
                if let Some(ref init) = vb.init {
                    self.resolve_expr(init, module, scope);
                }
                self.declare_local(
                    &vb.name.name,
                    SymbolKind::LocalVar,
                    module,
                    scope,
                    vb.span,
                    Some(vb.name.span),
                );
            }
            StmtKind::Const(cb) => {
                if let Some(ref t) = cb.ty {
                    self.resolve_type(t, scope);
                }
                self.resolve_expr(&cb.init, module, scope);
                self.declare_local(
                    &cb.name.name,
                    SymbolKind::LocalConst,
                    module,
                    scope,
                    cb.span,
                    Some(cb.name.span),
                );
            }
            StmtKind::Expr(e) => self.resolve_expr(e, module, scope),
            StmtKind::Return(r) => {
                if let Some(ref v) = r.value {
                    self.resolve_expr(v, module, scope);
                }
            }
            StmtKind::Assign(target, _op, expr) => {
                self.resolve_assign_target(target, module, scope);
                self.resolve_expr(expr, module, scope);
            }
            StmtKind::Break(b) => {
                if let Some(ref v) = b.value {
                    self.resolve_expr(v, module, scope);
                }
            }
            StmtKind::Continue(_) => {}
            StmtKind::Discard(d) => self.resolve_expr(&d.expr, module, scope),
        }
    }

    fn resolve_assign_target(
        &mut self,
        target: &nexa_ast::AssignTarget,
        module: ModuleId,
        scope: ScopeId,
    ) {
        match target {
            nexa_ast::AssignTarget::Ident(ident) => {
                if let Some(sym) = self.resolve_name(&ident.name, scope, NamespaceChoice::Value) {
                    self.record_reference(sym, scope, ReferenceKind::Write, ident.span);
                } else {
                    self.diagnostic(
                        SEM_UNKNOWN_NAME.as_str(),
                        ident.span,
                        format!("unknown name '{}'", ident.name),
                        Some("namespace=value".to_string()),
                    );
                }
            }
            nexa_ast::AssignTarget::Field(expr, _name) => {
                self.resolve_expr(expr, module, scope);
            }
            nexa_ast::AssignTarget::Index(target, index) => {
                self.resolve_expr(target, module, scope);
                self.resolve_expr(index, module, scope);
            }
        }
    }

    fn resolve_expr(&mut self, expr: &Expr, module: ModuleId, scope: ScopeId) {
        match &expr.kind {
            ExprKind::Ident(ident) => {
                if let Some(sym) = self.resolve_name(&ident.name, scope, NamespaceChoice::Value) {
                    self.record_reference(sym, scope, ReferenceKind::Read, ident.span);
                } else if ident.name == "self" {
                    self.diagnostic(
                        SEM_INVALID_SELF_REFERENCE.as_str(),
                        ident.span,
                        "'self' is only valid inside a method with a receiver".to_string(),
                        None,
                    );
                } else {
                    self.diagnostic(
                        SEM_UNKNOWN_NAME.as_str(),
                        ident.span,
                        format!("unknown name '{}'", ident.name),
                        Some("namespace=value".to_string()),
                    );
                }
            }
            ExprKind::Path(q) => {
                let segs: Vec<&str> = q.segments.iter().map(|s| s.name.as_str()).collect();
                let spans: Vec<SourceSpan> = q.segments.iter().map(|s| s.span).collect();
                self.resolve_path(&segs, &spans, scope, PathContext::Value);
            }
            ExprKind::Binary(_op, l, r) => {
                self.resolve_expr(l, module, scope);
                self.resolve_expr(r, module, scope);
            }
            ExprKind::Unary(_op, inner) => self.resolve_expr(inner, module, scope),
            ExprKind::Call { callee, args } => {
                self.resolve_callee(callee, module, scope);
                for a in args {
                    self.resolve_expr(a, module, scope);
                }
            }
            ExprKind::MethodCall {
                target,
                name: _,
                args,
            } => {
                self.resolve_expr(target, module, scope);
                for a in args {
                    self.resolve_expr(a, module, scope);
                }
            }
            ExprKind::Index { target, index } => {
                self.resolve_expr(target, module, scope);
                self.resolve_expr(index, module, scope);
            }
            ExprKind::Field { target, name: _ } => {
                // `.field` não é resolvido nesta fase (§204, §337).
                self.resolve_expr(target, module, scope);
            }
            ExprKind::Await(inner)
            | ExprKind::Try(inner)
            | ExprKind::Ref(inner)
            | ExprKind::RefMut(inner)
            | ExprKind::Move(inner)
            | ExprKind::Paren(inner) => self.resolve_expr(inner, module, scope),
            ExprKind::If(if_expr) => {
                self.resolve_expr(&if_expr.condition, module, scope);
                self.resolve_branched_block(&if_expr.then_block, module, scope);
                for ei in &if_expr.else_ifs {
                    self.resolve_expr(&ei.condition, module, scope);
                    self.resolve_branched_block(&ei.block, module, scope);
                }
                if let Some(ref eb) = if_expr.else_block {
                    self.resolve_branched_block(eb, module, scope);
                }
            }
            ExprKind::Match(match_expr) => {
                self.resolve_expr(&match_expr.scrutinee, module, scope);
                for arm in &match_expr.arms {
                    self.resolve_match_arm(arm, module, scope);
                }
            }
            ExprKind::For(for_expr) => {
                // Iterator expression resolvida no scope outer (§269-270).
                self.resolve_expr(&for_expr.iterable, module, scope);
                let loop_scope = self.index_mut().scopes.add_scope(
                    ScopeKind::Loop,
                    Some(scope),
                    module,
                    for_expr.span,
                );
                self.declare_local(
                    &for_expr.variable.name,
                    SymbolKind::LocalLet,
                    module,
                    loop_scope,
                    for_expr.variable.span,
                    Some(for_expr.variable.span),
                );
                self.push_block(loop_scope, &for_expr.body);
                self.resolve_block(&for_expr.body, module, loop_scope);
            }
            ExprKind::While(while_expr) => {
                self.resolve_expr(&while_expr.condition, module, scope);
                let loop_scope = self.index_mut().scopes.add_scope(
                    ScopeKind::Loop,
                    Some(scope),
                    module,
                    while_expr.span,
                );
                self.resolve_block(&while_expr.body, module, loop_scope);
            }
            ExprKind::Loop(loop_expr) => {
                let loop_scope = self.index_mut().scopes.add_scope(
                    ScopeKind::Loop,
                    Some(scope),
                    module,
                    loop_expr.span,
                );
                self.resolve_block(&loop_expr.body, module, loop_scope);
            }
            ExprKind::Block(block) => {
                let block_scope = self.index_mut().scopes.add_scope(
                    ScopeKind::Block,
                    Some(scope),
                    module,
                    block.span,
                );
                self.resolve_block(block, module, block_scope);
            }
            ExprKind::Unsafe(block) => {
                let block_scope = self.index_mut().scopes.add_scope(
                    ScopeKind::Block,
                    Some(scope),
                    module,
                    block.span,
                );
                self.resolve_block(block, module, block_scope);
            }
            ExprKind::Tuple(exprs) | ExprKind::Array(exprs) => {
                for e in exprs {
                    self.resolve_expr(e, module, scope);
                }
            }
            ExprKind::StructConstruct(sc) => {
                // O path-alvo do literal resolve como uso de nome, registrando
                // a resolução que o type checker consulta via `symbol_at`.
                if !sc.path.segments.is_empty() {
                    let segs: Vec<&str> =
                        sc.path.segments.iter().map(|s| s.name.as_str()).collect();
                    let spans: Vec<SourceSpan> = sc.path.segments.iter().map(|s| s.span).collect();
                    self.resolve_path(&segs, &spans, scope, PathContext::Type);
                }
                for f in &sc.fields {
                    self.resolve_expr(&f.expr, module, scope);
                }
            }
            ExprKind::AssignExpr(target, _op, value) => {
                self.resolve_assign_target(target, module, scope);
                self.resolve_expr(value, module, scope);
            }
            ExprKind::IntLiteral(_)
            | ExprKind::FloatLiteral(_)
            | ExprKind::StringLiteral(_)
            | ExprKind::ByteStringLiteral(_)
            | ExprKind::CharLiteral(_)
            | ExprKind::BoolLiteral(_)
            | ExprKind::Underscore => {}
        }
    }

    /// Bloco com scope próprio (if/else branch, §272).
    fn resolve_branched_block(&mut self, block: &Block, module: ModuleId, scope: ScopeId) {
        let block_scope =
            self.index_mut()
                .scopes
                .add_scope(ScopeKind::Block, Some(scope), module, block.span);
        self.resolve_block(block, module, block_scope);
    }

    fn resolve_callee(&mut self, callee: &Expr, module: ModuleId, scope: ScopeId) {
        match &callee.kind {
            ExprKind::Ident(ident) => {
                if let Some(sym) = self.resolve_name(&ident.name, scope, NamespaceChoice::Value) {
                    self.record_reference(sym, scope, ReferenceKind::Call, ident.span);
                } else {
                    self.diagnostic(
                        SEM_UNKNOWN_NAME.as_str(),
                        ident.span,
                        format!("unknown call target '{}'", ident.name),
                        Some("namespace=value".to_string()),
                    );
                }
            }
            ExprKind::Path(q) => {
                let segs: Vec<&str> = q.segments.iter().map(|s| s.name.as_str()).collect();
                let spans: Vec<SourceSpan> = q.segments.iter().map(|s| s.span).collect();
                // Call target: resolve e registra com kind Call.
                if let Some(sym) = self.resolve_path(&segs, &spans, scope, PathContext::Value) {
                    self.record_reference(
                        sym,
                        scope,
                        ReferenceKind::Call,
                        spans.last().copied().unwrap_or(callee.span),
                    );
                }
            }
            _ => self.resolve_expr(callee, module, scope),
        }
    }

    // ─── Match arms / patterns (Impl 03 §260-268, §552-575) ───────

    fn resolve_match_arm(&mut self, arm: &MatchArm, module: ModuleId, scope: ScopeId) {
        let arm_scope =
            self.index_mut()
                .scopes
                .add_scope(ScopeKind::MatchArm, Some(scope), module, arm.span);
        // Bindings são coletadas antes de guard/body (§267).
        self.collect_pattern_bindings(&arm.pattern, module, arm_scope);
        if let Some(ref guard) = arm.guard {
            self.resolve_expr(guard, module, arm_scope);
        }
        self.resolve_expr(&arm.body, module, arm_scope);
    }

    fn collect_pattern_bindings(&mut self, pattern: &Pattern, module: ModuleId, scope: ScopeId) {
        match &pattern.kind {
            PatternKind::Ident(ident) => {
                // Bare identifier: se resolve a um EnumVariant de zero payload,
                // trata como variant (§557-568); caso contrário é binding.
                if let Some(sym) = self.resolve_name(&ident.name, scope, NamespaceChoice::Value) {
                    let kind = self.index.symbols.get(sym).map(|d| d.kind);
                    if matches!(kind, Some(SymbolKind::EnumVariant)) {
                        self.record_reference(sym, scope, ReferenceKind::Read, ident.span);
                        return;
                    }
                }
                self.declare_local(
                    &ident.name,
                    SymbolKind::LocalLet,
                    module,
                    scope,
                    pattern.span,
                    Some(ident.span),
                );
            }
            PatternKind::Enum {
                path,
                variant,
                pattern: inner,
            } => {
                // Path + variant: resolve via associated values.
                let mut segs: Vec<&str> = path.segments.iter().map(|s| s.name.as_str()).collect();
                segs.push(&variant.name);
                let mut spans: Vec<SourceSpan> = path.segments.iter().map(|s| s.span).collect();
                spans.push(variant.span);
                self.resolve_path(&segs, &spans, scope, PathContext::Value);
                if let Some(ref inner) = inner {
                    self.collect_pattern_bindings(inner, module, scope);
                }
            }
            PatternKind::Struct { path, fields } => {
                let segs: Vec<&str> = path.segments.iter().map(|s| s.name.as_str()).collect();
                let spans: Vec<SourceSpan> = path.segments.iter().map(|s| s.span).collect();
                if segs.len() == 1 {
                    if let Some(sym) = self.resolve_name(segs[0], scope, NamespaceChoice::Type) {
                        self.record_reference(sym, scope, ReferenceKind::Type, spans[0]);
                    } else {
                        self.diagnostic(
                            SEM_UNKNOWN_NAME.as_str(),
                            spans[0],
                            format!("unknown type '{}'", segs[0]),
                            Some("namespace=type".to_string()),
                        );
                    }
                } else {
                    self.resolve_path(&segs, &spans, scope, PathContext::Type);
                }
                for f in fields {
                    if let Some(ref p) = f.pattern {
                        self.collect_pattern_bindings(p, module, scope);
                    }
                }
            }
            PatternKind::Tuple(ps) | PatternKind::Or(ps) => {
                for p in ps {
                    self.collect_pattern_bindings(p, module, scope);
                }
            }
            PatternKind::Rename {
                pattern: inner,
                alias,
            } => {
                // Resolve o variant path se existir, e declara o alias.
                self.collect_pattern_bindings(inner, module, scope);
                self.declare_local(
                    &alias.name,
                    SymbolKind::LocalLet,
                    module,
                    scope,
                    pattern.span,
                    Some(alias.span),
                );
            }
            PatternKind::Guard { pattern: inner, .. } => {
                self.collect_pattern_bindings(inner, module, scope);
            }
            PatternKind::Literal(_) | PatternKind::Wildcard | PatternKind::Rest => {}
        }
    }
}

// ─── helpers ─────────────────────────────────────────────────────

fn namespace_choice(ns: nexa_symbols::NamespaceKind) -> NamespaceChoice {
    match ns {
        nexa_symbols::NamespaceKind::Type => NamespaceChoice::Type,
        nexa_symbols::NamespaceKind::Value => NamespaceChoice::Value,
        nexa_symbols::NamespaceKind::Module => NamespaceChoice::Module,
    }
}

fn ctx_ns(ctx: PathContext) -> &'static str {
    match ctx {
        PathContext::Type => "type",
        PathContext::Value => "value",
    }
}

fn has_receiver(method: &nexa_ast::ReceiverKind) -> bool {
    matches!(
        method,
        nexa_ast::ReceiverKind::Self_
            | nexa_ast::ReceiverKind::RefSelf
            | nexa_ast::ReceiverKind::RefMutSelf
    )
}

impl Default for Resolver {
    fn default() -> Self {
        Self::new()
    }
}

fn is_path_like(kind: &TypeKind) -> bool {
    matches!(kind, TypeKind::Path(_) | TypeKind::Generic { .. })
}

fn field_visibility(field: &nexa_ast::StructField) -> Visibility {
    if field.exported {
        Visibility::PackageInternal
    } else {
        Visibility::ModulePrivate
    }
}

fn has_receiver_iface(method: &InterfaceMethod) -> bool {
    matches!(
        method.receiver,
        nexa_ast::ReceiverKind::Self_
            | nexa_ast::ReceiverKind::RefSelf
            | nexa_ast::ReceiverKind::RefMutSelf
    )
}

fn has_receiver_impl(method: &ImplMethod) -> bool {
    has_receiver(&method.receiver)
}

// ─── Entry points ────────────────────────────────────────────────

/// Resolve um projeto inteiro (múltiplos packages/modules).
pub fn resolve_project(project: &ParsedProject, config: &ResolverConfig) -> ResolveResult {
    let mut resolver = Resolver::new();
    resolver.current_package = project.current_package;
    resolver.dependency_aliases = config
        .dependency_aliases
        .iter()
        .map(|a| (a.alias.clone(), a.package))
        .collect();
    let mods = resolver.index_modules(project);

    // Fases A-G (§245).
    for &(m, pm) in &mods {
        resolver.phase_collect_top_level(m, &pm.ast);
    }
    for &(m, pm) in &mods {
        resolver.phase_resolve_imports(m, &pm.ast);
    }
    for &(m, pm) in &mods {
        resolver.phase_implement_targets(m, &pm.ast);
    }
    for &(m, pm) in &mods {
        resolver.phase_collect_associated(m, &pm.ast);
    }
    for &(m, pm) in &mods {
        resolver.phase_resolve_signatures(m, &pm.ast);
    }
    for &(m, pm) in &mods {
        resolver.phase_resolve_bodies(m, &pm.ast);
    }
    resolver.into_result()
}

/// Resolve uma SourceUnit única (single-module mode, §459).
pub fn resolve(source_id: SourceId, ast: &SourceUnit) -> ResolveResult {
    let project = nexa_project::single_module_project(
        PackageInstanceId(0),
        "app",
        ModulePath::new(vec!["main".to_string()]),
        source_id,
        ast.clone(),
        true,
    );
    let config = ResolverConfig::default();
    resolve_project(&project, &config)
}
