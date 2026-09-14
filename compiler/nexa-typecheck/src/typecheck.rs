//! Type checker (Implementação 04).
//!
//! Consome o `SemanticIndex` produzido pelo resolver (§455) e o AST do módulo:
//! (T1-T3) registra tipos das declarações, (T4-T6) resolve assinaturas e
//! implementações, (T7) verifica corpos. Tipos são internados no `TypeStore`
//! e um modelo semântico tipado é produzido como side table (spans → tipo).

use crate::diagnostics::{TypeDiagnostic, TypeDiagnosticCode};
use crate::model::*;
use nexa_ast::{
    Block, EnumDecl, Expr, ExprKind, FunctionDecl, Item, ItemKind, MatchArm, Pattern, PatternKind,
    QualifiedName, ReturnStmt, SourceUnit, Stmt, StmtKind, Type as AstType, TypeKind,
};
use nexa_resolver::SemanticIndex;
use nexa_source::{SourceId, SourceSpan};
use nexa_symbols::{ModuleId, PackageInstanceId, SymbolId, SymbolKind, Visibility};
use nexa_types::compatibility;
use nexa_types::id::{GenericParamId, NominalTypeId, TypeId};
use nexa_types::prelude::{
    bootstrap_prelude, bootstrap_prelude_nominal, PreludeNominalBootstrap, PreludeSymbols,
    PreludeTypes,
};
use nexa_types::store::TypeStore;
use nexa_types::ty::{
    self, CallableKind, CallableType, FieldDefinition, NominalType, NominalTypeKind,
    SemanticValidity, TypeDefinition, ValueCategory, VariantDefinition, VariantKind,
};
use std::collections::{HashMap, HashSet};

/// Generic context: nome do parâmetro → GenericParamId (por nível de scope).
pub type GenericCtx = HashMap<String, GenericParamId>;

/// Variante de enum: owner enum (base TypeId) + payload (tipos já resolvidos).
#[derive(Debug, Clone)]
struct VariantRecord {
    owner: TypeId,
    payload: Vec<TypeId>,
}

pub struct TypeChecker {
    pub store: TypeStore,
    pub prelude: PreludeTypes,
    pub prelude_nominal: PreludeNominalBootstrap,
    pub semantic: TypedSemanticModel,
    pub diagnostics: Vec<TypeDiagnostic>,
    /// Índice semântico do resolver (resoluções span→symbol, scopes, prelude).
    pub index: SemanticIndex,
    pub source_id: SourceId,
    pub module_id: ModuleId,
    /// Contexto de corpo atual.
    current_return_type: Option<TypeId>,
    /// Tipo de symbols de valor (params, locals, consts, receivers).
    local_types: HashMap<SymbolId, TypeId>,
    /// Assinaturas de callables por symbol.
    callable_by_symbol: HashMap<SymbolId, CallableType>,
    /// Tipo nominal/alias por symbol de declaração.
    type_by_symbol: HashMap<SymbolId, TypeId>,
    /// Variantes de enums por symbol (construtores).
    variant_by_symbol: HashMap<SymbolId, VariantRecord>,
    /// Symbol dos construtores `Some/None/Success/Failure` do prelude.
    prelude_variants: PreludeSymbolsIds,
    /// Generic params registrados por item (ordem nome → gpid).
    item_generics: HashMap<SymbolId, Vec<(String, GenericParamId)>>,
    /// Lookup de declaração por (start,end) do span do nome.
    decl_by_name_span: HashMap<(u32, u32), SymbolId>,
    /// Span do nome de cada symbol (para posicionar diagnósticos de tipo).
    top_level_spans: HashMap<SymbolId, SourceSpan>,
    /// Se o módulo atual tem erros de declaração (evita cascatas falsas).
    poisoned: bool,
}

#[derive(Debug, Clone, Copy)]
struct PreludeSymbolsIds {
    some: SymbolId,
    none: SymbolId,
    success: SymbolId,
    failure: SymbolId,
}

impl TypeChecker {
    /// Type checker sem índice (uso em testes de expressão/literais).
    pub fn new() -> Self {
        Self::build(TypeCheckerInput {
            index: SemanticIndex::new(),
            module_id: ModuleId(0),
            source_id: SourceId(0),
        })
    }

    /// Type checker integrado com o `SemanticIndex` do resolver.
    pub fn with_index(index: SemanticIndex, module_id: ModuleId, source_id: SourceId) -> Self {
        Self::build(TypeCheckerInput {
            index,
            module_id,
            source_id,
        })
    }

    fn build(input: TypeCheckerInput) -> Self {
        let TypeCheckerInput {
            index,
            module_id,
            source_id,
        } = input;

        let mut store = TypeStore::new();
        let prelude = bootstrap_prelude(&mut store);

        let prelude_symbols = PreludeSymbols {
            optional: index
                .prelude_symbol("Optional", false)
                .unwrap_or(SymbolId(0)),
            result: index.prelude_symbol("Result", false).unwrap_or(SymbolId(1)),
            some: index.prelude_symbol("Some", true).unwrap_or(SymbolId(2)),
            none: index.prelude_symbol("None", true).unwrap_or(SymbolId(3)),
            success: index.prelude_symbol("Success", true).unwrap_or(SymbolId(4)),
            failure: index.prelude_symbol("Failure", true).unwrap_or(SymbolId(5)),
        };
        let prelude_variants = PreludeSymbolsIds {
            some: prelude_symbols.some,
            none: prelude_symbols.none,
            success: prelude_symbols.success,
            failure: prelude_symbols.failure,
        };
        let prelude_nominal = bootstrap_prelude_nominal(&mut store, &prelude_symbols);

        let mut tc = TypeChecker {
            store,
            prelude,
            prelude_nominal,
            semantic: TypedSemanticModel::new(),
            diagnostics: Vec::new(),
            index,
            source_id,
            module_id,
            current_return_type: None,
            local_types: HashMap::new(),
            callable_by_symbol: HashMap::new(),
            type_by_symbol: HashMap::new(),
            variant_by_symbol: HashMap::new(),
            prelude_variants,
            item_generics: HashMap::new(),
            decl_by_name_span: HashMap::new(),
            top_level_spans: HashMap::new(),
            poisoned: false,
        };

        // Registra os construtores `Some/None/Success/Failure` como variantes.
        tc.register_prelude_variants();

        // `assert`/`panic` do prelude como callables conhecidos.
        tc.register_prelude_callables();

        tc
    }

    fn register_prelude_variants(&mut self) {
        let opt = self.prelude_nominal.optional;
        let res = self.prelude_nominal.result;
        let t = self
            .store
            .intern_type(ty::Type::GenericParameter(self.prelude_nominal.optional_t));
        let rt = self
            .store
            .intern_type(ty::Type::GenericParameter(self.prelude_nominal.result_t));
        let re = self
            .store
            .intern_type(ty::Type::GenericParameter(self.prelude_nominal.result_e));
        self.variant_by_symbol.insert(
            self.prelude_variants.some,
            VariantRecord {
                owner: opt,
                payload: vec![t],
            },
        );
        self.variant_by_symbol.insert(
            self.prelude_variants.none,
            VariantRecord {
                owner: opt,
                payload: vec![],
            },
        );
        self.variant_by_symbol.insert(
            self.prelude_variants.success,
            VariantRecord {
                owner: res,
                payload: vec![rt],
            },
        );
        self.variant_by_symbol.insert(
            self.prelude_variants.failure,
            VariantRecord {
                owner: res,
                payload: vec![re],
            },
        );
    }

    fn register_prelude_callables(&mut self) {
        // `assert(Bool) -> Unit`.
        if let Some(sym) = self.index.prelude_symbol("assert", true) {
            self.callable_by_symbol.insert(
                sym,
                CallableType {
                    kind: CallableKind::Function,
                    parameters: vec![self.prelude.bool],
                    return_type: self.prelude.unit,
                    generic_params: Vec::new(),
                },
            );
        }
        // `panic(String) -> Never`.
        if let Some(sym) = self.index.prelude_symbol("panic", true) {
            self.callable_by_symbol.insert(
                sym,
                CallableType {
                    kind: CallableKind::Function,
                    parameters: vec![self.prelude.string],
                    return_type: self.prelude.never,
                    generic_params: Vec::new(),
                },
            );
        }
        if let Some(console) = self.index.prelude_symbol("Console", false) {
            for (name, symbol) in self.index.associated_values_of(console) {
                if self.index.name_of(name) == "write" {
                    self.callable_by_symbol.insert(
                        symbol,
                        CallableType {
                            kind: CallableKind::Action,
                            parameters: vec![self.prelude.string],
                            return_type: self.prelude.unit,
                            generic_params: Vec::new(),
                        },
                    );
                }
            }
        }
    }

    pub fn into_result(self) -> TypeCheckResult {
        TypeCheckResult {
            semantic: self.semantic,
            diagnostics: self.diagnostics,
        }
    }

    pub fn type_store(&self) -> &TypeStore {
        &self.store
    }

    pub fn type_store_mut(&mut self) -> &mut TypeStore {
        &mut self.store
    }

    // ─── Entry points ──────────────────────────────────────────────

    /// Tipo completo de um módulo: registra declarações e verifica corpos.
    pub fn check_module(&mut self, ast: &SourceUnit) {
        self.build_decl_lookup();
        self.register_alias_network(&ast.items);
        self.register_declarations(&ast.items);
        if self.poisoned {
            return;
        }
        // DoD §804: "recursive alias/layout cycles controlled" — a rede de
        // aliases já rejeitou ciclos; agora valida layout por valor (§85.7).
        self.check_layout_cycles();
        self.check_bodies(&ast.items);
        self.check_public_api_visibility(&ast.items);
    }

    /// Compat: registra tipos das declarações top-level.
    pub fn register_declaration_types(&mut self, items: &[Item]) {
        self.register_declarations(items)
    }

    /// Compat: verifica um módulo completo.
    pub fn check_source_unit(&mut self, ast: &SourceUnit) {
        self.check_module(ast)
    }

    // ─── Fase T1-T3: registro de declarações ──────────────────────

    /// Resolve antecipadamente a rede de aliases transparentes (sem forward
    /// dependence), com detecção de ciclo (§98): `type A = B; type B = A`
    /// → NEXA-TYPE-0030. Só aliases sem type params (MVP); distinct passa
    /// pelo fluxo nominal normal.
    fn register_alias_network(&mut self, items: &[Item]) {
        let mut lot: Vec<(SymbolId, String, &nexa_ast::TypeAliasDecl)> = Vec::new();
        for item in items {
            let Some(symbol) = self.item_symbol(item) else {
                continue;
            };
            if let ItemKind::TypeAlias(a) = &item.kind {
                if a.is_distinct || !a.generic_params.is_empty() {
                    continue;
                }
                lot.push((symbol, a.name.name.clone(), a));
            }
        }
        if lot.is_empty() {
            return;
        }
        let target_name = |ty: &AstType| -> Option<String> {
            match &ty.kind {
                TypeKind::Path(p) => p.segments.last().map(|s| s.name.clone()),
                TypeKind::Generic { path, .. } => path.segments.last().map(|s| s.name.clone()),
                _ => None,
            }
        };
        let by_name: HashMap<String, SymbolId> =
            lot.iter().map(|(s, n, _)| (n.clone(), *s)).collect();

        let mut resolved: HashSet<SymbolId> = HashSet::new();
        loop {
            let mut progress = false;
            for (symbol, _name, alias) in &lot {
                if resolved.contains(symbol) {
                    continue;
                }
                // Depende de outro alias do lote ainda não resolvido?
                if let Some(tn) = target_name(&alias.ty) {
                    if lot
                        .iter()
                        .any(|(s, an, _)| an == &tn && !resolved.contains(s))
                    {
                        continue;
                    }
                }
                let target = self.resolve_type_syntax(&alias.ty, &GenericCtx::new());
                self.store.declare_alias(*symbol, target);
                self.type_by_symbol.insert(*symbol, target);
                resolved.insert(*symbol);
                progress = true;
            }
            if !progress {
                break;
            }
        }

        // Pendentes: formam ciclos entre aliases do lote → 0030.
        for (symbol, name, alias) in &lot {
            if resolved.contains(symbol) {
                continue;
            }
            // Garante entrada no store para `alias_is_error` refletir o ciclo.
            self.store.declare_alias(*symbol, self.prelude.unit);
            self.store.set_alias_error(*symbol);
            let mut chain = vec![name.clone()];
            let mut cur: Option<SymbolId> =
                target_name(&alias.ty).and_then(|n| by_name.get(&n).copied());
            while let Some(sid) = cur {
                let (_, n, a) = lot.iter().find(|(s, _, _)| *s == sid).cloned().unwrap();
                chain.push(n);
                if chain.last() == Some(name) {
                    break;
                }
                cur = target_name(&a.ty).and_then(|t| by_name.get(&t).copied());
            }
            let display = chain
                .iter()
                .map(|c| format!("`{c}`"))
                .collect::<Vec<_>>()
                .join(" -> ");
            self.diagnostics.push(TypeDiagnostic::new(
                TypeDiagnosticCode::RecursiveTypeCycle,
                format!("type alias cycle: {display}"),
                alias.span,
            ));
        }
    }

    /// Layout cycles por valor (§87-99, §635): `struct Node { next: Node }`
    /// embute a si mesmo sem passar por storage indireto (Ref/MutRef/Array)
    /// → NEXA-TYPE-0030. Optional inline continua recursivo (§93).
    fn check_layout_cycles(&mut self) {
        let mut nids: Vec<NominalTypeId> = self
            .type_by_symbol
            .values()
            .filter_map(|&t| match self.store.get_type(t) {
                Some(ty::Type::Nominal(n)) => Some(*n),
                _ => None,
            })
            .collect();
        nids.sort_unstable();
        nids.dedup();
        for nid in nids {
            let mut guard = HashSet::new();
            if self.store.nominal_contains(nid, nid, &mut guard) {
                let name = self.nominal_name(nid);
                if let Some(def) = self.store.get_nominal_definition(nid) {
                    let span = match def {
                        TypeDefinition::Struct(d) => self.idents_of_symbol(d.symbol),
                        TypeDefinition::Enum(d) => self.idents_of_symbol(d.symbol),
                        TypeDefinition::Distinct(d) => self.idents_of_symbol(d.symbol),
                        TypeDefinition::Interface(_) => None,
                    };
                    self.diagnostics.push(TypeDiagnostic::new(
                        TypeDiagnosticCode::RecursiveTypeCycle,
                        format!("type `{name}` has a recursive layout cycle"),
                        span.unwrap_or_else(|| SourceSpan::new(self.source_id, 0, 0)),
                    ));
                }
            }
        }
    }

    fn idents_of_symbol(&self, symbol: SymbolId) -> Option<SourceSpan> {
        self.top_level_spans.get(&symbol).copied()
    }

    fn build_decl_lookup(&mut self) {
        self.decl_by_name_span.clear();
        self.top_level_spans.clear();
        for sym in self.index.symbols.iter() {
            if let Some(ns) = sym.name_span {
                self.top_level_spans.insert(sym.id, ns);
                if ns.source == self.source_id && !ns.is_zero_width() {
                    self.decl_by_name_span.insert((ns.start, ns.end), sym.id);
                }
            }
        }
    }

    fn item_symbol(&self, item: &Item) -> Option<SymbolId> {
        let ident = decl_ident(&item.kind)?;
        self.decl_by_name_span
            .get(&(ident.span.start, ident.span.end))
            .copied()
    }

    fn register_declarations(&mut self, items: &[Item]) {
        for item in items {
            // Implement não tem nome; registra direto (sem symbol top-level).
            if let ItemKind::Implement(imp) = &item.kind {
                self.register_implementation(item, SymbolId(0), imp);
                continue;
            }
            let Some(symbol) = self.item_symbol(item) else {
                continue;
            };
            let generic_ids = self.register_generic_params(item);
            self.item_generics.insert(symbol, generic_ids.clone());
            let ctx = generic_ctx(&generic_ids);
            match &item.kind {
                ItemKind::TypeAlias(alias) => self.register_type_alias(item, symbol, alias, &ctx),
                ItemKind::Struct(s) => self.register_struct(item, symbol, s, &ctx),
                ItemKind::Enum(e) => self.register_enum(item, symbol, e, &ctx),
                ItemKind::Interface(iface) => self.register_interface(item, symbol, iface, &ctx),
                ItemKind::Function(f) => self.register_function(symbol, f, &ctx),
                ItemKind::Action(a) => self.register_action(symbol, a, &ctx),
                ItemKind::Const(c) => self.register_const_decl(symbol, c),
                ItemKind::Implement(imp) => self.register_implementation(item, symbol, imp),
            }
        }
    }

    fn register_generic_params(&mut self, item: &Item) -> Vec<(String, GenericParamId)> {
        let generic_params: Vec<&nexa_ast::GenericParam> = match &item.kind {
            ItemKind::Struct(s) => s.generic_params.iter().collect(),
            ItemKind::Enum(e) => e.generic_params.iter().collect(),
            ItemKind::Interface(i) => i.generic_params.iter().collect(),
            ItemKind::TypeAlias(a) => a.generic_params.iter().collect(),
            ItemKind::Function(f) => f.generic_params.iter().collect(),
            ItemKind::Action(a) => a.generic_params.iter().collect(),
            ItemKind::Implement(imp) => imp.generic_params.iter().collect(),
            _ => Vec::new(),
        };
        let where_predicates: Vec<&nexa_ast::WherePredicate> = match &item.kind {
            ItemKind::Function(f) => f.where_clause.iter().collect(),
            ItemKind::Action(a) => a.where_clause.iter().collect(),
            _ => Vec::new(),
        };
        let mut out = Vec::new();
        for p in generic_params {
            let gpid = self
                .store
                .create_generic_param(nexa_types::ty::GenericParameterInfo {
                    symbol: SymbolId(0), // resolvido depois; identidade por índice
                    constraints: Vec::new(),
                });
            // Constraints: bounds inline `T: Show` + predicados `where T: Show`
            // (§104-108). Resolves para interface; não-interface → 0032.
            let mut constraints: Vec<nexa_types::ty::Constraint> = Vec::new();
            for b in &p.bounds {
                if let Some(iface) = self.resolve_bound_interface(&b.path, &b.generic_args, b.span)
                {
                    constraints.push(nexa_types::ty::Constraint {
                        parameter: gpid,
                        interface: iface,
                    });
                }
            }
            for wp in &where_predicates {
                if wp.type_name.name == p.name.name {
                    for b in &wp.bounds {
                        if let Some(iface) =
                            self.resolve_bound_interface(&b.path, &b.generic_args, b.span)
                        {
                            constraints.push(nexa_types::ty::Constraint {
                                parameter: gpid,
                                interface: iface,
                            });
                        }
                    }
                }
            }
            self.store.set_generic_param_constraints(gpid, constraints);
            out.push((p.name.name.clone(), gpid));
        }
        out
    }

    /// Resolve um `TypeBound` para o `TypeId` de uma interface. Se o bound não
    /// apontar para uma interface → NEXA-TYPE-0032. Tipos não resolvidos
    /// (declarados depois na ordem de registro) são ignorados (`None`).
    fn resolve_bound_interface(
        &mut self,
        path: &QualifiedName,
        args: &[AstType],
        span: SourceSpan,
    ) -> Option<TypeId> {
        let kind = if args.is_empty() {
            TypeKind::Path(path.clone())
        } else {
            TypeKind::Generic {
                path: path.clone(),
                args: args.to_vec(),
            }
        };
        let tid = self.resolve_type_syntax(&nexa_ast::Type { kind, span }, &GenericCtx::new());
        if matches!(self.store.get_type(tid), Some(ty::Type::Error)) {
            return None;
        }
        if !compatibility::is_interface_type(&self.store, tid) {
            let bound_name = path
                .segments
                .last()
                .map(|s| s.name.as_str())
                .unwrap_or("<bound>");
            self.diagnostics.push(TypeDiagnostic::new(
                TypeDiagnosticCode::ConstraintMustBeInterface,
                format!("constraint `{bound_name}` is not an interface"),
                span,
            ));
            return None;
        }
        Some(tid)
    }

    fn register_type_alias(
        &mut self,
        item: &Item,
        symbol: SymbolId,
        alias: &nexa_ast::TypeAliasDecl,
        ctx: &GenericCtx,
    ) {
        let tid;
        if alias.is_distinct {
            let nominal = NominalType {
                symbol,
                kind: NominalTypeKind::Distinct,
                generic_params: self.store_all_generic_params(ctx),
                package: PackageInstanceId(0),
                module: self.module_id,
            };
            let (t, nid) = self.store.create_nominal(nominal);
            tid = t;
            // O nome do distinct fica disponível antes da base, para
            // `type A = distinct A` resolver para si mesmo (§99).
            self.type_by_symbol.insert(symbol, tid);
            let base = self.resolve_type_syntax(&alias.ty, ctx);
            self.store.register_nominal_definition(
                nid,
                TypeDefinition::Distinct(nexa_types::ty::DistinctTypeDefinition {
                    name: alias.name.name.clone(),
                    ty: tid,
                    base,
                    visibility: visibility_of(item),
                    symbol,
                }),
            );
        } else {
            // Alias transparente: normalmente já resolvido pela rede de
            // aliases (register_alias_network); re-resolver é idempotente.
            // Aliases em ciclo já diagnosticado (0030) não re-resolvem, para
            // não re-emitir `unknown type` sobre o outro membro do ciclo.
            let target = if self.store.alias_is_error(symbol) {
                self.prelude.unit
            } else {
                let t = self.resolve_type_syntax(&alias.ty, ctx);
                self.store.declare_alias(symbol, t);
                t
            };
            let target_tid = self.store.alias_target(symbol).unwrap_or(target);
            self.type_by_symbol.insert(symbol, target_tid);
            tid = target_tid;
        };
        self.semantic.declaration_types.insert(DeclarationTypeInfo {
            symbol,
            type_id: tid,
            callable_signature: None,
        });
    }

    fn store_all_generic_params(&mut self, ctx: &GenericCtx) -> Vec<GenericParamId> {
        let mut ids = Vec::new();
        for val in ctx.values() {
            if !ids.contains(val) {
                ids.push(*val);
            }
        }
        ids
    }

    fn register_struct(
        &mut self,
        item: &Item,
        symbol: SymbolId,
        s: &nexa_ast::StructDecl,
        ctx: &GenericCtx,
    ) {
        let generic_params = self.store_all_generic_params(ctx);
        let (tid, nid) = self.store.create_nominal(NominalType {
            symbol,
            kind: NominalTypeKind::Struct,
            generic_params: generic_params.clone(),
            package: PackageInstanceId(0),
            module: self.module_id,
        });
        // O nome do nominal fica disponível antes de resolver os campos, para
        // `struct Node { next: Node }` resolver para si mesmo (§86-88).
        self.type_by_symbol.insert(symbol, tid);
        let fields = s
            .fields
            .iter()
            .map(|f| FieldDefinition {
                name: f.name.name.clone(),
                ty: self.resolve_type_syntax(&f.ty, ctx),
                exported: f.exported,
                symbol: symbol_of_associated(self, f.name.span),
            })
            .collect();
        self.store.register_nominal_definition(
            nid,
            TypeDefinition::Struct(nexa_types::ty::StructTypeDefinition {
                name: s.name.name.clone(),
                ty: tid,
                visibility: visibility_of(item),
                symbol,
                fields,
            }),
        );
        self.semantic.declaration_types.insert(DeclarationTypeInfo {
            symbol,
            type_id: tid,
            callable_signature: None,
        });
    }

    fn register_enum(&mut self, item: &Item, symbol: SymbolId, e: &EnumDecl, ctx: &GenericCtx) {
        let generic_params = self.store_all_generic_params(ctx);
        let (tid, nid) = self.store.create_nominal(NominalType {
            symbol,
            kind: NominalTypeKind::Enum,
            generic_params: generic_params.clone(),
            package: PackageInstanceId(0),
            module: self.module_id,
        });
        // O nome do nominal fica disponível antes de resolver os payloads.
        self.type_by_symbol.insert(symbol, tid);
        let mut variants = Vec::new();
        for v in &e.variants {
            let kind = match &v.kind {
                nexa_ast::EnumVariantKind::Unit => VariantKind::Unit,
                nexa_ast::EnumVariantKind::Tuple(ts) => VariantKind::Tuple(
                    ts.iter()
                        .map(|t| self.resolve_type_syntax(t, ctx))
                        .collect(),
                ),
                nexa_ast::EnumVariantKind::Struct(fs) => VariantKind::Struct(
                    fs.iter()
                        .map(|f| FieldDefinition {
                            name: f.name.name.clone(),
                            ty: self.resolve_type_syntax(&f.ty, ctx),
                            exported: f.exported,
                            symbol: symbol_of_associated(self, f.name.span),
                        })
                        .collect(),
                ),
            };
            let vsym = symbol_of_associated(self, v.name.span);
            let payload = match &kind {
                VariantKind::Tuple(ts) => ts.clone(),
                _ => Vec::new(),
            };
            self.variant_by_symbol.insert(
                vsym,
                VariantRecord {
                    owner: tid,
                    payload,
                },
            );
            variants.push(VariantDefinition {
                name: v.name.name.clone(),
                symbol: vsym,
                kind,
            });
        }
        self.store.register_nominal_definition(
            nid,
            TypeDefinition::Enum(nexa_types::ty::EnumTypeDefinition {
                name: e.name.name.clone(),
                ty: tid,
                visibility: visibility_of(item),
                symbol,
                variants,
            }),
        );
        self.semantic.declaration_types.insert(DeclarationTypeInfo {
            symbol,
            type_id: tid,
            callable_signature: None,
        });
    }

    fn register_interface(
        &mut self,
        item: &Item,
        symbol: SymbolId,
        iface: &nexa_ast::InterfaceDecl,
        ctx: &GenericCtx,
    ) {
        let generic_params = self.store_all_generic_params(ctx);
        let (tid, nid) = self.store.create_nominal(NominalType {
            symbol,
            kind: NominalTypeKind::Interface,
            generic_params: generic_params.clone(),
            package: PackageInstanceId(0),
            module: self.module_id,
        });
        // Registra cedo para que o nome da interface resolva em assinaturas
        // de membros que se referem à própria interface (`area(...) -> Shape`).
        self.type_by_symbol.insert(symbol, tid);
        let members = iface
            .methods
            .iter()
            .map(|m| {
                let msym = symbol_of_associated(self, m.name.span);
                nexa_types::ty::InterfaceMemberSignature {
                    symbol: msym,
                    callable: self.callable_of(
                        CallableKind::Function,
                        &m.params,
                        m.return_type.as_ref(),
                        ctx,
                    ),
                }
            })
            .collect();
        self.store.register_nominal_definition(
            nid,
            TypeDefinition::Interface(nexa_types::ty::InterfaceDefinition {
                name: iface.name.name.clone(),
                ty: tid,
                visibility: visibility_of(item),
                symbol,
                members,
            }),
        );
        self.type_by_symbol.insert(symbol, tid);
        self.semantic.declaration_types.insert(DeclarationTypeInfo {
            symbol,
            type_id: tid,
            callable_signature: None,
        });
    }

    fn callable_of(
        &mut self,
        kind: CallableKind,
        params: &[nexa_ast::Param],
        ret: Option<&AstType>,
        ctx: &GenericCtx,
    ) -> CallableType {
        let parameters = params
            .iter()
            .map(|p| self.resolve_type_syntax(&p.ty, ctx))
            .collect();
        let return_type = ret
            .map(|r| self.resolve_type_syntax(r, ctx))
            .unwrap_or(self.prelude.unit);
        let mut generic_params: Vec<GenericParamId> = ctx.values().copied().collect();
        generic_params.dedup();
        generic_params.sort();
        CallableType {
            kind,
            parameters,
            return_type,
            generic_params,
        }
    }

    fn register_function(&mut self, symbol: SymbolId, f: &FunctionDecl, ctx: &GenericCtx) {
        let sig = self.callable_of(
            CallableKind::Function,
            &f.params,
            f.return_type.as_ref(),
            ctx,
        );
        self.callable_by_symbol.insert(symbol, sig.clone());
        self.semantic
            .callable_signatures
            .insert(symbol, sig.clone());
        self.semantic.declaration_types.insert(DeclarationTypeInfo {
            symbol,
            type_id: sig.return_type,
            callable_signature: Some(sig),
        });
    }

    fn register_action(&mut self, symbol: SymbolId, a: &nexa_ast::ActionDecl, ctx: &GenericCtx) {
        let kind = if a.is_async {
            CallableKind::AsyncAction
        } else {
            CallableKind::Action
        };
        let sig = self.callable_of(kind, &a.params, a.return_type.as_ref(), ctx);
        self.callable_by_symbol.insert(symbol, sig.clone());
        self.semantic
            .callable_signatures
            .insert(symbol, sig.clone());
        self.semantic.declaration_types.insert(DeclarationTypeInfo {
            symbol,
            type_id: sig.return_type,
            callable_signature: Some(sig),
        });
    }

    fn register_const_decl(&mut self, symbol: SymbolId, c: &nexa_ast::ConstDecl) {
        let declared =
            c.ty.as_ref()
                .map(|t| self.resolve_type_syntax(t, &GenericCtx::new()));
        let tid = declared.unwrap_or(self.prelude.unit);
        self.type_by_symbol.insert(symbol, tid);
        self.semantic.declaration_types.insert(DeclarationTypeInfo {
            symbol,
            type_id: tid,
            callable_signature: None,
        });
    }

    fn register_implementation(
        &mut self,
        item: &Item,
        _symbol: SymbolId,
        imp: &nexa_ast::ImplDecl,
    ) {
        let iface = self.resolve_type_syntax(
            &nexa_ast::Type {
                kind: TypeKind::Path(imp.trait_path.clone()),
                span: imp.trait_path.span,
            },
            &GenericCtx::new(),
        );
        let target = self.resolve_type_syntax(&imp.for_type, &GenericCtx::new());
        if !compatibility::is_interface_type(&self.store, iface) {
            self.diagnostics.push(TypeDiagnostic::new(
                TypeDiagnosticCode::InvalidInterfaceImplementation,
                "impl target is not an interface".to_string(),
                imp.trait_path.span,
            ));
            return;
        }
        // Coherence (§374-378): o `implement` só é legal no pacote dono da
        // interface OU dono do tipo nominal alvo (sem órfãos).
        if let (Some(iface_mod), Some(target_mod)) =
            (self.nominal_module(iface), self.nominal_module(target))
        {
            if iface_mod != self.module_id && target_mod != self.module_id {
                self.diagnostics.push(TypeDiagnostic::new(
                    TypeDiagnosticCode::CoherenceViolation,
                    "orphan implementation: package owns neither the interface nor the target type"
                        .to_string(),
                    imp.trait_path.span,
                ));
                return;
            }
        }
        // Duplicatas/overlapping (§379-384): Core 1.0 sem especialização —
        // dois `implement` para o mesmo par (interface, target) são rejeitados.
        // O par exatamente idêntico (§379) é uma duplicata (0027); padrões que
        // se sobrepõem sem serem idênticos (§381-384) são overlapping (0042).
        if self.store.has_interface(target, iface).is_some() {
            let iface_name = imp
                .trait_path
                .segments
                .last()
                .map(|s| s.name.as_str())
                .unwrap_or("<interface>");
            if self.store.has_exact_implementation(target, iface) {
                self.diagnostics.push(TypeDiagnostic::new(
                    TypeDiagnosticCode::DuplicateImplementation,
                    format!(
                        "duplicate implementation: `{iface_name}` is already implemented for this exact type"
                    ),
                    imp.trait_path.span,
                ));
            } else {
                self.diagnostics.push(TypeDiagnostic::new(
                    TypeDiagnosticCode::OverlappingImplementation,
                    format!(
                        "overlapping implementation: `{iface_name}` is already implemented for this type"
                    ),
                    imp.trait_path.span,
                ));
            }
            return;
        }
        let Some((_, iface_def, _)) = self.interface_def(iface) else {
            return;
        };
        let member_names: Vec<String> = iface_def
            .members
            .deref_members()
            .iter()
            .filter_map(|mm| self.interface_member_name(mm))
            .collect();
        // Cobertura (§385-388 + cobertura da interface): cada membro da
        // interface deve ter correspondente no `implement`.
        let sig_by_name: Vec<(String, CallableType)> = iface_def
            .members
            .deref_members()
            .iter()
            .filter_map(|mm| Some((self.interface_member_name(mm)?, mm.callable.clone())))
            .collect();
        let mut matched: HashSet<String> = HashSet::new();
        let mut members = Vec::new();
        let mut fatal = false;
        for m in &imp.methods {
            let msym = symbol_of_associated(self, m.name.span);
            members.push(msym);
            match sig_by_name.iter().find(|(n, _)| *n == m.name.name) {
                Some((name, msig)) => {
                    matched.insert(name.clone());
                    let sig = self.callable_of(
                        CallableKind::Function,
                        &m.params,
                        m.return_type.as_ref(),
                        &GenericCtx::new(),
                    );
                    let arity_ok = m.params.len() == msig.parameters.len();
                    let ret_ok = sig.return_type == msig.return_type
                        || self.same_nominal(sig.return_type, msig.return_type);
                    if !(arity_ok && ret_ok) {
                        self.diagnostics.push(TypeDiagnostic::new(
                            TypeDiagnosticCode::IncompatibleInterfaceMember,
                            format!(
                                "member `{}` does not match the interface signature",
                                m.name.name
                            ),
                            m.name.span,
                        ));
                        fatal = true;
                    }
                }
                None => {
                    if !self.variant_by_symbol.contains_key(&msym) {
                        self.diagnostics.push(TypeDiagnostic::new(
                            TypeDiagnosticCode::UnexpectedInterfaceImplementationMember,
                            format!("member `{}` is not part of the interface", m.name.name),
                            m.name.span,
                        ));
                        fatal = true;
                    }
                }
            }
        }
        for name in member_names {
            if !matched.contains(&name) {
                self.diagnostics.push(TypeDiagnostic::new(
                    TypeDiagnosticCode::MissingInterfaceMember,
                    format!("interface member `{}` is not implemented", name),
                    imp.trait_path.span,
                ));
                fatal = true;
            }
        }
        if fatal {
            return;
        }
        let generic_params = self.store_all_generic_params(&GenericCtx::new());
        let impl_id = self
            .store
            .register_implementation(nexa_types::ty::Implementation {
                id: nexa_types::id::ImplementationId(0),
                interface: Some(iface),
                target,
                generic_params,
                constraints: Vec::new(),
                members,
            });
        let _ = (item, impl_id);
    }

    fn nominal_module(&self, ty: TypeId) -> Option<ModuleId> {
        let nid = match self.store.get_type(ty) {
            Some(ty::Type::Nominal(nid)) => *nid,
            Some(ty::Type::Applied { base, .. }) => match self.store.get_type(*base) {
                Some(ty::Type::Nominal(nid)) => *nid,
                _ => return None,
            },
            _ => return None,
        };
        self.store.get_nominal(nid).map(|n| n.module)
    }

    fn interface_member_name(
        &self,
        member: &nexa_types::ty::InterfaceMemberSignature,
    ) -> Option<String> {
        self.index
            .symbols
            .get(member.symbol)
            .map(|s| self.index.name_of(s.name).to_string())
    }

    fn interface_name(&self, iface: TypeId) -> String {
        self.interface_def(iface)
            .map(|(_, def, _)| def.name.clone())
            .or_else(|| {
                let nid = match self.store.get_type(iface) {
                    Some(ty::Type::Nominal(nid)) => Some(*nid),
                    _ => None,
                };
                nid.and_then(|n| self.store.get_nominal(n).map(|m| m.symbol))
                    .and_then(|s| {
                        self.index
                            .symbols
                            .get(s)
                            .map(|sd| self.index.name_of(sd.name).to_string())
                    })
            })
            .unwrap_or_else(|| "<interface>".to_string())
    }

    fn interface_def(
        &self,
        iface: TypeId,
    ) -> Option<(
        &NominalType,
        &nexa_types::ty::InterfaceDefinition,
        NominalTypeId,
    )> {
        match self.store.get_type(iface) {
            Some(ty::Type::Nominal(nid)) => {
                let n = self.store.get_nominal(*nid)?;
                match self.store.get_nominal_definition(*nid) {
                    Some(TypeDefinition::Interface(d)) => Some((n, d, *nid)),
                    _ => None,
                }
            }
            Some(ty::Type::Applied { base, .. }) => match self.store.get_type(*base) {
                Some(ty::Type::Nominal(nid)) => {
                    let n = self.store.get_nominal(*nid)?;
                    match self.store.get_nominal_definition(*nid) {
                        Some(TypeDefinition::Interface(d)) => Some((n, d, *nid)),
                        _ => None,
                    }
                }
                _ => None,
            },
            _ => None,
        }
    }

    // ─── Fase T7: corpos ───────────────────────────────────────────

    /// DoD §804: "public API type visibility validation works". Toda
    /// declaração `export` vira API pública; tipos nominais não-públicos
    /// referenciados pela sua assinatura pública vazam → NEXA-TYPE-0044.
    fn check_public_api_visibility(&mut self, items: &[Item]) {
        for item in items {
            if !item.exported {
                continue;
            }
            let Some(ident) = decl_ident(&item.kind) else {
                continue;
            };
            let Some(symbol) = self.item_symbol(item) else {
                continue;
            };
            let mut signature: Vec<TypeId> = Vec::new();
            match &item.kind {
                ItemKind::Function(_) | ItemKind::Action(_) => {
                    if let Some(sig) = self.callable_by_symbol.get(&symbol) {
                        signature.extend(sig.parameters.iter().copied());
                        signature.push(sig.return_type);
                    }
                }
                ItemKind::Const(_) => {
                    if let Some(&t) = self.type_by_symbol.get(&symbol) {
                        signature.push(t);
                    }
                }
                ItemKind::TypeAlias(_) => {
                    if let Some(t) = self.store.alias_target(symbol) {
                        signature.push(t);
                    }
                }
                ItemKind::Struct(_) | ItemKind::Enum(_) => {
                    // fields/variants fazem parte da API pública quando o
                    // próprio type é exportado (§116: fields seguem a
                    // visibilidade do type).
                    if let Some(&t) = self.type_by_symbol.get(&symbol) {
                        if let Some(ty::Type::Nominal(nid)) = self.store.get_type(t) {
                            if let Some(def) = self.store.get_nominal_definition(*nid) {
                                match def {
                                    TypeDefinition::Struct(sd) => {
                                        for f in &sd.fields {
                                            signature.push(f.ty);
                                        }
                                    }
                                    TypeDefinition::Enum(ed) => {
                                        for v in &ed.variants {
                                            match &v.kind {
                                                VariantKind::Tuple(ts) => {
                                                    signature.extend(ts.iter().copied())
                                                }
                                                VariantKind::Struct(fs) => {
                                                    for f in fs {
                                                        signature.push(f.ty);
                                                    }
                                                }
                                                VariantKind::Unit => {}
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                ItemKind::Interface(_) => {
                    if let Some(&t) = self.type_by_symbol.get(&symbol) {
                        if let Some(ty::Type::Nominal(nid)) = self.store.get_type(t) {
                            if let Some(TypeDefinition::Interface(idef)) =
                                self.store.get_nominal_definition(*nid)
                            {
                                for m in &idef.members {
                                    signature.extend(m.callable.parameters.iter().copied());
                                    signature.push(m.callable.return_type);
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
            for t in signature {
                let mut noms: Vec<NominalTypeId> = Vec::new();
                self.collect_nominals(t, &mut noms);
                for &n in &noms {
                    if !Self::nominal_is_public(&self.store, n) {
                        self.diagnostics.push(TypeDiagnostic::new(
                            TypeDiagnosticCode::PublicApiExposesNonPublicType,
                            format!(
                                "public API of `{}` exposes non-public type `{}`",
                                ident.name,
                                self.nominal_name(n)
                            ),
                            ident.span,
                        ));
                    }
                }
            }
        }
    }

    /// Coleta os tipos nominais referenciados por um tipo, transitivamente
    /// (Ref/Array/Task/Applied/Callable). `GenericParameter` e primitivos não
    /// são nominais e portanto não entram.
    fn collect_nominals(&self, tid: TypeId, out: &mut Vec<NominalTypeId>) {
        match self.store.get_type(tid) {
            Some(ty::Type::Nominal(n)) => {
                if !out.contains(n) {
                    out.push(*n);
                }
            }
            Some(ty::Type::Ref(i))
            | Some(ty::Type::MutRef(i))
            | Some(ty::Type::Array(i))
            | Some(ty::Type::Task(i)) => self.collect_nominals(*i, out),
            Some(ty::Type::Applied { base, arguments }) => {
                self.collect_nominals(*base, out);
                for a in arguments {
                    self.collect_nominals(*a, out);
                }
            }
            Some(ty::Type::Callable(c)) => {
                for p in &c.parameters {
                    self.collect_nominals(*p, out);
                }
                self.collect_nominals(c.return_type, out);
            }
            _ => {}
        }
    }

    fn nominal_name(&self, nid: NominalTypeId) -> String {
        if let Some(def) = self.store.get_nominal_definition(nid) {
            return match def {
                TypeDefinition::Struct(d) => d.name.clone(),
                TypeDefinition::Enum(d) => d.name.clone(),
                TypeDefinition::Interface(d) => d.name.clone(),
                TypeDefinition::Distinct(d) => d.name.clone(),
            };
        }
        self.store
            .get_nominal(nid)
            .and_then(|m| self.index.symbols.get(m.symbol))
            .map(|s| self.index.name_of(s.name).to_string())
            .unwrap_or_else(|| "<type>".to_string())
    }

    /// Um nominal é acessível pela API pública se a sua definição não é
    /// module-private (export = package/public no MVP single-module).
    fn nominal_is_public(store: &TypeStore, nid: NominalTypeId) -> bool {
        match store.get_nominal_definition(nid) {
            Some(TypeDefinition::Struct(d)) => d.visibility != Visibility::ModulePrivate,
            Some(TypeDefinition::Enum(d)) => d.visibility != Visibility::ModulePrivate,
            Some(TypeDefinition::Interface(d)) => d.visibility != Visibility::ModulePrivate,
            Some(TypeDefinition::Distinct(d)) => d.visibility != Visibility::ModulePrivate,
            None => true,
        }
    }

    fn check_bodies(&mut self, items: &[Item]) {
        for item in items {
            if let ItemKind::Implement(imp) = &item.kind {
                self.check_impl_bodies(imp);
                continue;
            }
            let Some(symbol) = self.item_symbol(item) else {
                continue;
            };
            let ctx = self
                .item_generics
                .get(&symbol)
                .map(|v| generic_ctx(v))
                .unwrap_or_default();
            match &item.kind {
                ItemKind::Function(f) => self.check_function_body(symbol, f, &ctx),
                ItemKind::Action(a) => self.check_action_body(symbol, a, &ctx),
                ItemKind::Const(c) => {
                    // Public const sem tipo explícito (§467-469) → 0043.
                    if item.exported && c.ty.is_none() {
                        self.diagnostics.push(TypeDiagnostic::new(
                            TypeDiagnosticCode::PublicConstRequiresExplicitType,
                            "public const requires an explicit type".to_string(),
                            c.span,
                        ));
                    }
                    self.check_const_init(symbol, c);
                }
                ItemKind::Implement(imp) => self.check_impl_bodies(imp),
                _ => {}
            }
        }
    }

    fn check_function_body(&mut self, _symbol: SymbolId, f: &FunctionDecl, ctx: &GenericCtx) {
        let ret_type = f
            .return_type
            .as_ref()
            .map(|rt| self.resolve_type_syntax(rt, ctx))
            .unwrap_or(self.prelude.unit);
        self.enter_callable(&f.params, ctx, ret_type);
        for r in &f.requires {
            self.check_contract(r.expr.span, &r.expr);
        }
        for e in &f.ensures {
            self.check_contract(e.expr.span, &e.expr);
        }
        self.check_block(&f.body, None);
        self.leave_callable();
    }

    fn check_action_body(&mut self, _symbol: SymbolId, a: &nexa_ast::ActionDecl, ctx: &GenericCtx) {
        let ret_type = a
            .return_type
            .as_ref()
            .map(|rt| self.resolve_type_syntax(rt, ctx))
            .unwrap_or(self.prelude.unit);
        let wrapped = if a.is_async {
            self.store.intern_type(ty::Type::Task(ret_type))
        } else {
            ret_type
        };
        self.enter_callable(&a.params, ctx, wrapped);
        for r in &a.requires {
            self.check_contract(r.expr.span, &r.expr);
        }
        for e in &a.ensures {
            self.check_contract(e.expr.span, &e.expr);
        }
        self.check_block(&a.body, None);
        self.leave_callable();
    }

    fn check_contract(&mut self, span: SourceSpan, expr: &Expr) {
        let ty = self.check_expression(expr, Some(self.prelude.bool));
        if !compatibility::is_bool(&self.store, ty) {
            self.diagnostics.push(TypeDiagnostic::new(
                TypeDiagnosticCode::TypeMismatch,
                "contract condition must be Bool".to_string(),
                span,
            ));
        }
    }

    fn check_const_init(&mut self, symbol: SymbolId, c: &nexa_ast::ConstDecl) {
        let expected =
            c.ty.as_ref()
                .map(|t| self.resolve_type_syntax(t, &GenericCtx::new()));
        let ty = self.check_expression(&c.init, expected);
        if let Some(exp) = expected {
            if !compatibility::assignable(&self.store, ty, exp) {
                self.diagnostics.push(TypeDiagnostic::new(
                    self.implicit_conversion_code(ty, exp),
                    "const initializer type mismatch".to_string(),
                    c.span,
                ));
            }
        } else {
            // Inferência: atualiza o tipo registrado na fase T1-T3.
            if let Some(info) = self.semantic.declaration_types.get_mut(symbol) {
                info.type_id = ty;
            }
        }
    }

    fn enter_callable(&mut self, params: &[nexa_ast::Param], ctx: &GenericCtx, ret: TypeId) {
        self.local_types.clear();
        for p in params {
            let pty = self.resolve_type_syntax(&p.ty, ctx);
            if let Some(sym) = self.symbol_lookup(p.name.span) {
                self.local_types.insert(sym, pty);
            }
        }
        self.current_return_type = Some(ret);
    }

    fn leave_callable(&mut self) {
        self.local_types.clear();
        self.current_return_type = None;
    }

    /// Classifica mismatch de atribuição: uso implícito de distinct
    /// (base↔distinct, §33-39) é 0006; inteiro→inteiro distinto é 0003
    /// (§74-75); demais seguem 0002.
    fn implicit_conversion_code(&self, from: TypeId, to: TypeId) -> TypeDiagnosticCode {
        if from != to && (self.is_distinct_type(from) || self.is_distinct_type(to)) {
            return TypeDiagnosticCode::InvalidDistinctTypeUse;
        }
        if compatibility::is_integer(&self.store, from)
            && compatibility::is_integer(&self.store, to)
        {
            return TypeDiagnosticCode::InvalidImplicitConversion;
        }
        TypeDiagnosticCode::TypeMismatch
    }

    fn is_distinct_type(&self, id: TypeId) -> bool {
        matches!(
            self.store.get_type(id),
            Some(ty::Type::Nominal(nid))
                if self
                    .store
                    .get_nominal(*nid)
                    .is_some_and(|n| n.kind == NominalTypeKind::Distinct)
        )
    }

    fn check_impl_bodies(&mut self, imp: &nexa_ast::ImplDecl) {
        for m in &imp.methods {
            let Some(body) = &m.body else { continue };
            let ret_type = m
                .return_type
                .as_ref()
                .map(|rt| self.resolve_type_syntax(rt, &GenericCtx::new()))
                .unwrap_or(self.prelude.unit);
            self.local_types.clear();
            // Receiver implícito.
            let receiver_ty = self.resolve_type_syntax(&imp.for_type, &GenericCtx::new());
            let receiver_ty = match m.receiver {
                nexa_ast::ReceiverKind::RefSelf => {
                    self.store.intern_type(ty::Type::Ref(receiver_ty))
                }
                nexa_ast::ReceiverKind::RefMutSelf => {
                    self.store.intern_type(ty::Type::MutRef(receiver_ty))
                }
                nexa_ast::ReceiverKind::Self_ => receiver_ty,
            };
            // Bind do receiver `self` (§960/§1102): o resolver registra um
            // symbol de receiver por método; ligá-lo ao tipo do impl permite
            // `self.field`/`self.method()` nos corpos.
            for sd in self.index.symbols.iter() {
                if sd.kind == SymbolKind::Receiver
                    && sd.span.source == body.span.source
                    && !(sd.span.end <= body.span.start || body.span.end <= sd.span.start)
                {
                    self.local_types.insert(sd.id, receiver_ty);
                }
            }
            self.current_return_type = Some(ret_type);
            for p in &m.params {
                let pty = self.resolve_type_syntax(&p.ty, &GenericCtx::new());
                if let Some(sym) = self.symbol_lookup(p.name.span) {
                    self.local_types.insert(sym, pty);
                }
            }
            self.check_block(body, None);
            self.local_types.clear();
            self.current_return_type = None;
        }
    }

    fn check_block(&mut self, block: &Block, expected: Option<TypeId>) -> TypeId {
        for stmt in &block.stmts {
            self.check_statement(stmt);
        }
        // Bloco como expressão: última statement Expr dá o valor.
        if let Some(Stmt {
            kind: StmtKind::Expr(e),
            ..
        }) = block.stmts.last()
        {
            return self.check_expression(e, expected);
        }
        let _ = expected;
        self.prelude.unit
    }

    fn check_statement(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::Let(let_b) => {
                let declared = let_b
                    .ty
                    .as_ref()
                    .map(|t| self.resolve_type_syntax(t, &GenericCtx::new()));
                let init_ty = self.check_expression(&let_b.init, declared);
                if let Some(sym) = self.symbol_lookup(let_b.name.span) {
                    self.local_types.insert(sym, init_ty);
                }
                if let Some(d) = declared {
                    if !compatibility::assignable(&self.store, init_ty, d) {
                        let code = self.implicit_conversion_code(init_ty, d);
                        self.diagnostics.push(TypeDiagnostic::new(
                            code,
                            format!(
                                "let binding type mismatch: expected {}, got {}",
                                nexa_types::format::format_type(&self.store, d),
                                nexa_types::format::format_type(&self.store, init_ty)
                            ),
                            let_b.span,
                        ));
                    }
                }
            }
            StmtKind::Var(var_b) => {
                let declared = var_b
                    .ty
                    .as_ref()
                    .map(|t| self.resolve_type_syntax(t, &GenericCtx::new()));
                let init_ty = var_b
                    .init
                    .as_ref()
                    .map(|i| self.check_expression(i, declared))
                    .unwrap_or(declared.unwrap_or(self.prelude.unit));
                if let Some(sym) = self.symbol_lookup(var_b.name.span) {
                    self.local_types.insert(sym, init_ty);
                }
                if let Some(d) = declared {
                    if let Some(i) = &var_b.init {
                        let ty = self.check_expression(i, Some(d));
                        if !compatibility::assignable(&self.store, ty, d) {
                            self.diagnostics.push(TypeDiagnostic::new(
                                self.implicit_conversion_code(ty, d),
                                "var binding type mismatch".to_string(),
                                var_b.span,
                            ));
                        }
                    }
                }
            }
            StmtKind::Const(const_b) => {
                let declared = const_b
                    .ty
                    .as_ref()
                    .map(|t| self.resolve_type_syntax(t, &GenericCtx::new()));
                let init_ty = self.check_expression(&const_b.init, declared);
                if let Some(sym) = self.symbol_lookup(const_b.name.span) {
                    self.local_types.insert(sym, init_ty);
                }
                if let Some(d) = declared {
                    if !compatibility::assignable(&self.store, init_ty, d) {
                        self.diagnostics.push(TypeDiagnostic::new(
                            self.implicit_conversion_code(init_ty, d),
                            "const binding type mismatch".to_string(),
                            const_b.span,
                        ));
                    }
                }
            }
            StmtKind::Expr(expr) => {
                self.check_expression(expr, None);
            }
            StmtKind::Return(ret) => {
                self.check_return(ret);
            }
            StmtKind::Assign(target, _op, expr) => {
                self.check_assign_target(target);
                self.check_expression(expr, None);
            }
            StmtKind::Break(brk) => {
                if let Some(ref val) = brk.value {
                    self.check_expression(val, None);
                }
            }
            StmtKind::Continue(_) => {}
            StmtKind::Discard(d) => {
                self.check_expression(&d.expr, None);
            }
        }
    }

    fn check_assign_target(&mut self, target: &nexa_ast::AssignTarget) {
        match target {
            nexa_ast::AssignTarget::Ident(ident) => {
                if let Some(sym) = self.symbol_at(ident.span) {
                    if let Some(d) = self.index.symbols.get(sym) {
                        if d.kind == SymbolKind::LocalLet || d.kind == SymbolKind::Const {
                            self.diagnostics.push(TypeDiagnostic::new(
                                TypeDiagnosticCode::TypeMismatch,
                                "cannot assign to immutable binding".to_string(),
                                ident.span,
                            ));
                        }
                    }
                }
            }
            nexa_ast::AssignTarget::Field(target, _name) => {
                self.check_expression(target, None);
            }
            nexa_ast::AssignTarget::Index(target, index) => {
                let t = self.check_expression(target, None);
                let i = self.check_expression(index, None);
                let _ = self.check_index(t, i, index.span);
            }
        }
    }

    fn check_return(&mut self, ret: &ReturnStmt) {
        let expected = self.current_return_type;
        match (&ret.value, expected) {
            (Some(expr), Some(ret_ty)) => {
                let expr_ty = self.check_expression(expr, Some(ret_ty));
                if !compatibility::return_compatible(&self.store, expr_ty, ret_ty) {
                    self.diagnostics.push(TypeDiagnostic::new(
                        TypeDiagnosticCode::InvalidReturnType,
                        "return type mismatch".to_string(),
                        ret.span,
                    ));
                }
            }
            (None, Some(ret_ty)) if !compatibility::is_unit(&self.store, ret_ty) => {
                self.diagnostics.push(TypeDiagnostic::new(
                    TypeDiagnosticCode::InvalidReturnType,
                    "bare return in non-unit function".to_string(),
                    ret.span,
                ));
            }
            _ => {}
        }
    }

    // ─── Type syntax ───────────────────────────────────────────────

    /// Resolve um tipo sintático a um TypeId (symbol-aware).
    pub fn resolve_type_syntax(&mut self, ty: &AstType, generic_map: &GenericCtx) -> TypeId {
        match &ty.kind {
            TypeKind::Path(path) => self.resolve_type_path(path, generic_map, ty.span),
            TypeKind::Unit => self.prelude.unit,
            TypeKind::Generic { path, args } => {
                let base = self.resolve_type_path(path, generic_map, path.span);
                let arg_ids: Vec<TypeId> = args
                    .iter()
                    .map(|a| self.resolve_type_syntax(a, generic_map))
                    .collect();
                // Optional<T>/Result<T,E> nominal do prelude.
                if self.is_prelude_optional(base) && arg_ids.len() == 1 {
                    return self
                        .prelude_nominal
                        .optional_of(&mut self.store, arg_ids[0]);
                }
                if self.is_prelude_result(base) && arg_ids.len() == 2 {
                    return self
                        .prelude_nominal
                        .result_of(&mut self.store, arg_ids[0], arg_ids[1]);
                }
                // Arity nominal (§101, NEXA-TYPE-0031).
                let arity = match self.store.get_type(base) {
                    Some(ty::Type::Nominal(nid)) => {
                        self.store.get_nominal(*nid).map(|n| n.generic_params.len())
                    }
                    Some(ty::Type::Applied { base: b, .. }) => match self.store.get_type(*b) {
                        Some(ty::Type::Nominal(nid)) => {
                            self.store.get_nominal(*nid).map(|n| n.generic_params.len())
                        }
                        _ => None,
                    },
                    _ => None,
                };
                if let Some(arity) = arity {
                    if arity != arg_ids.len() {
                        let name = path
                            .segments
                            .last()
                            .map(|s| s.name.as_str())
                            .unwrap_or("<type>");
                        self.diagnostics.push(TypeDiagnostic::new(
                            TypeDiagnosticCode::GenericArgumentCountMismatch,
                            format!(
                                "type `{name}` expects {arity} generic argument(s), found {}",
                                arg_ids.len()
                            ),
                            ty.span,
                        ));
                    }
                }
                self.store.create_applied(base, arg_ids)
            }
            TypeKind::Array(inner) => {
                let inner_id = self.resolve_type_syntax(inner, generic_map);
                self.store.intern_type(ty::Type::Array(inner_id))
            }
            TypeKind::Optional(inner) => {
                let inner_id = self.resolve_type_syntax(inner, generic_map);
                self.prelude_nominal.optional_of(&mut self.store, inner_id)
            }
            TypeKind::Result { ok, err } => {
                let ok_id = self.resolve_type_syntax(ok, generic_map);
                let err_id = self.resolve_type_syntax(err, generic_map);
                self.prelude_nominal
                    .result_of(&mut self.store, ok_id, err_id)
            }
            TypeKind::Ref(inner) => {
                let inner_id = self.resolve_type_syntax(inner, generic_map);
                self.store.intern_type(ty::Type::Ref(inner_id))
            }
            TypeKind::RefMut(inner) => {
                let inner_id = self.resolve_type_syntax(inner, generic_map);
                self.store.intern_type(ty::Type::MutRef(inner_id))
            }
            TypeKind::Tuple(types) => {
                // Tuplas sem suporte dedicado nesta fase: via Applied de Unit
                // como placeholder — spec exige tuplas reais (§A2); cleanup futuro.
                let args: Vec<TypeId> = types
                    .iter()
                    .map(|t| self.resolve_type_syntax(t, generic_map))
                    .collect();
                if args.len() == 1 {
                    args[0]
                } else if args.is_empty() {
                    self.prelude.unit
                } else {
                    self.store.create_applied(self.prelude.unit, args)
                }
            }
            TypeKind::Function { params, ret } => {
                let param_ids: Vec<TypeId> = params
                    .iter()
                    .map(|p| self.resolve_type_syntax(p, generic_map))
                    .collect();
                let ret_id = self.resolve_type_syntax(ret, generic_map);
                self.store.intern_type(ty::Type::Callable(CallableType {
                    kind: CallableKind::Function,
                    parameters: param_ids,
                    return_type: ret_id,
                    generic_params: Vec::new(),
                }))
            }
        }
    }

    fn is_prelude_optional(&self, base: TypeId) -> bool {
        self.prelude_nominal.optional == base
    }

    fn is_prelude_result(&self, base: TypeId) -> bool {
        self.prelude_nominal.result == base
    }

    fn resolve_type_path(
        &mut self,
        path: &QualifiedName,
        generic_map: &GenericCtx,
        span: SourceSpan,
    ) -> TypeId {
        if path.segments.len() == 1 {
            let name = &path.segments[0].name;
            if let Some(&gpid) = generic_map.get(name) {
                return self.store.intern_type(ty::Type::GenericParameter(gpid));
            }
        }
        let joined = path
            .segments
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>()
            .join("::");

        // Primitivos do prelude por nome (funciona mesmo sem índice).
        if let Some(t) = self.prelude_type_from_name(&joined) {
            return t;
        }

        // Optional/Result do prelude nominal (sem índice também).
        if joined == "Optional" {
            return self.prelude_nominal.optional;
        }
        if joined == "Result" {
            return self.prelude_nominal.result;
        }

        // Resolução via símbolo do resolver (nome simples ou qualificado).
        let sym = self
            .symbol_at(span)
            .or_else(|| path.segments.first().and_then(|s| self.symbol_at(s.span)))
            .or_else(|| self.index.prelude_symbol(&joined, false));

        match sym {
            Some(symbol) => {
                let data = self.index.symbols.get(symbol);
                let kind = data.map(|d| d.kind);
                match kind {
                    Some(SymbolKind::GenericParameter) => {
                        // Parâmetro genérico resolvido pelo resolver.
                        if let Some(&gpid) = generic_map.get(&joined) {
                            self.store.intern_type(ty::Type::GenericParameter(gpid))
                        } else {
                            self.prelude.unit
                        }
                    }
                    _ => match self.type_by_symbol.get(&symbol) {
                        Some(&t) => t,
                        None => {
                            if self.index.is_prelude_name(&joined, false) {
                                self.prelude.unit
                            } else {
                                self.diagnostics.push(TypeDiagnostic::new(
                                    TypeDiagnosticCode::UnknownType,
                                    format!("unknown type `{}`", joined),
                                    span,
                                ));
                                self.store.intern_type(ty::Type::Error)
                            }
                        }
                    },
                }
            }
            None => {
                self.diagnostics.push(TypeDiagnostic::new(
                    TypeDiagnosticCode::UnknownType,
                    format!("unknown type `{}`", joined),
                    span,
                ));
                self.store.intern_type(ty::Type::Error)
            }
        }
    }

    fn prelude_type_from_name(&self, name: &str) -> Option<TypeId> {
        match name {
            "Unit" => Some(self.prelude.unit),
            "Never" => Some(self.prelude.never),
            "Bool" => Some(self.prelude.bool),
            "Int" => Some(self.prelude.int),
            "UInt" => Some(self.prelude.uint),
            "Int8" => Some(self.prelude.int8),
            "Int16" => Some(self.prelude.int16),
            "Int32" => Some(self.prelude.int32),
            "Int64" => Some(self.prelude.int64),
            "UInt8" => Some(self.prelude.uint8),
            "UInt16" => Some(self.prelude.uint16),
            "UInt32" => Some(self.prelude.uint32),
            "UInt64" => Some(self.prelude.uint64),
            "Float32" => Some(self.prelude.float32),
            "Float64" => Some(self.prelude.float64),
            "Byte" => Some(self.prelude.byte),
            "Char" => Some(self.prelude.char),
            "String" => Some(self.prelude.string),
            "Bytes" => Some(self.prelude.bytes),
            _ => None,
        }
    }

    fn symbol_at(&self, span: SourceSpan) -> Option<SymbolId> {
        self.index.resolutions.symbol_at(self.source_id, span.start)
    }

    /// Símbolo alvo de um path qualificado: o resolver registra a resolução
    /// do membro no span do segmento final (leaf), logo iteramos de trás
    /// para frente — o primeiro segmento (módulo/tipo raiz) nunca é o alvo
    /// de valor de `Type::Variant`/`Type::associated` (§534).
    fn path_leaf_symbol(&self, path: &QualifiedName) -> Option<SymbolId> {
        path.segments
            .iter()
            .rev()
            .find_map(|s| self.symbol_at(s.span))
    }

    /// Lookup de span que cobre declarações (params, lets, patterns, for-vars)
    /// via `name_span` dos símbolos, com fallback para o mapa de usos.
    ///
    /// Público para o pipeline de fluxo (nexa-flow) resolver symbols a partir
    /// de spans do AST (Implementação 05).
    pub fn symbol_lookup(&self, span: SourceSpan) -> Option<SymbolId> {
        self.decl_by_name_span
            .get(&(span.start, span.end))
            .copied()
            .or_else(|| self.symbol_at(span))
    }

    // ─── Expressions ───────────────────────────────────────────────

    /// Infere o tipo de uma expressão.
    pub fn check_expression(&mut self, expr: &Expr, expected: Option<TypeId>) -> TypeId {
        let ty = match &expr.kind {
            ExprKind::IntLiteral(val) => {
                if let Some(exp) = expected {
                    self.materialize_int(*val, exp, expr.span)
                } else {
                    self.prelude.int
                }
            }
            ExprKind::FloatLiteral(ref val) => {
                if let Some(exp) = expected {
                    if compatibility::is_float(&self.store, exp) || *val < 0.0 {
                        exp
                    } else {
                        self.prelude.float64
                    }
                } else {
                    self.prelude.float64
                }
            }
            ExprKind::BoolLiteral(_) => self.prelude.bool,
            ExprKind::CharLiteral(_) => self.prelude.char,
            ExprKind::StringLiteral(_) => self.prelude.string,
            ExprKind::ByteStringLiteral(_) => self.prelude.bytes,
            ExprKind::Ident(ident) => self.check_ident(ident.span, expected, expr.span),
            ExprKind::Path(path) => self.check_path_value(path, expected, expr.span),
            ExprKind::Underscore => {
                if let Some(exp) = expected {
                    exp
                } else {
                    self.diagnostics.push(TypeDiagnostic::new(
                        TypeDiagnosticCode::CannotInferType,
                        "cannot infer type of _".to_string(),
                        expr.span,
                    ));
                    self.prelude.unit
                }
            }
            ExprKind::Binary(op, left, right) => {
                let lty = self.check_expression(left, None);
                let rty = self.check_expression(right, None);
                self.validate_operator(*op, lty, rty, expr.span)
            }
            ExprKind::Unary(op, inner) => {
                let ity = self.check_expression(inner, None);
                self.validate_unary_operator(*op, ity, expr.span)
            }
            ExprKind::Call { callee, args } => self.check_call(callee, args, expr.span),
            ExprKind::Index { target, index } => {
                let target_ty = self.check_expression(target, None);
                let index_ty = self.check_expression(index, None);
                self.check_index(target_ty, index_ty, expr.span)
            }
            ExprKind::Field { target, name } => {
                let target_ty = self.check_expression(target, None);
                self.check_field_access(target_ty, &name.name, expr.span)
            }
            ExprKind::MethodCall { target, name, args } => {
                let target_ty = self.check_expression(target, None);
                self.check_method_call(target_ty, &name.name, args, expr.span)
            }
            ExprKind::Await(inner) => {
                let inner_ty = self.check_expression(inner, None);
                self.check_await(inner_ty, expr.span)
            }
            ExprKind::Try(inner) => {
                let inner_ty = self.check_expression(inner, None);
                self.check_try(inner_ty, expr.span)
            }
            ExprKind::Ref(inner) => {
                let inner_ty = self.check_expression(inner, None);
                self.store.intern_type(ty::Type::Ref(inner_ty))
            }
            ExprKind::RefMut(inner) => {
                let inner_ty = self.check_expression(inner, None);
                self.store.intern_type(ty::Type::MutRef(inner_ty))
            }
            ExprKind::Move(inner) => self.check_expression(inner, None),
            ExprKind::If(if_expr) => self.check_if(if_expr, expected),
            ExprKind::Match(match_expr) => self.check_match(match_expr, expected),
            ExprKind::For(for_expr) => {
                let elem = self
                    .iterable_element(for_expr.iterable.span, &for_expr.iterable, None)
                    .unwrap_or(self.prelude.unit);
                if let Some(sym) = self.symbol_lookup(for_expr.variable.span) {
                    self.local_types.insert(sym, elem);
                }
                self.check_block(&for_expr.body, None);
                self.prelude.unit
            }
            ExprKind::While(while_expr) => {
                self.check_expression(&while_expr.condition, Some(self.prelude.bool));
                self.check_block(&while_expr.body, None);
                self.prelude.unit
            }
            ExprKind::Loop(loop_expr) => {
                let break_ty = expected.unwrap_or(self.prelude.never);
                self.check_block(&loop_expr.body, Some(break_ty));
                self.prelude.never
            }
            ExprKind::Block(block) => self.check_block(block, expected),
            ExprKind::Unsafe(block) => self.check_block(block, expected),
            ExprKind::Tuple(exprs) => {
                let args: Vec<TypeId> = exprs
                    .iter()
                    .map(|e| self.check_expression(e, None))
                    .collect();
                if args.len() == 1 {
                    args[0]
                } else if args.is_empty() {
                    self.prelude.unit
                } else {
                    self.store.create_applied(self.prelude.unit, args)
                }
            }
            ExprKind::Array(exprs) => {
                if exprs.is_empty() {
                    if let Some(exp) = expected {
                        match self.store.get_type(exp) {
                            Some(ty::Type::Array(_)) => exp,
                            _ => self.store.intern_type(ty::Type::Array(exp)),
                        }
                    } else {
                        self.diagnostics.push(TypeDiagnostic::new(
                            TypeDiagnosticCode::CannotInferType,
                            "cannot infer type of empty array".to_string(),
                            expr.span,
                        ));
                        self.prelude.unit
                    }
                } else {
                    let first_ty = self.check_expression(&exprs[0], expected);
                    for e in &exprs[1..] {
                        let elem_ty = self.check_expression(e, Some(first_ty));
                        if !nexa_types::same_type(&self.store, first_ty, elem_ty) {
                            self.diagnostics.push(TypeDiagnostic::new(
                                TypeDiagnosticCode::TypeMismatch,
                                "array element type mismatch".to_string(),
                                e.span,
                            ));
                        }
                    }
                    self.store.intern_type(ty::Type::Array(first_ty))
                }
            }
            ExprKind::StructConstruct(sc) => self.check_struct_construct(sc, expected),
            ExprKind::Paren(inner) => self.check_expression(inner, expected),
            ExprKind::AssignExpr(target, _op, value) => {
                self.check_assign_target(target);
                self.check_expression(value, None);
                self.prelude.unit
            }
        };

        // Record expression info.
        self.semantic.expression_info.insert(
            expr.span,
            ExprInfo {
                ty,
                category: ValueCategory::Value,
                resolved_symbol: self
                    .index
                    .resolutions
                    .symbol_at(self.source_id, expr.span.start),
                validity: SemanticValidity::Valid,
            },
        );
        ty
    }

    fn check_struct_construct(
        &mut self,
        sc: &nexa_ast::StructConstructExpr,
        _expected: Option<TypeId>,
    ) -> TypeId {
        let struct_ty = self.resolve_type_path(&sc.path, &GenericCtx::new(), sc.path.span);
        let nid = match self.store.get_type(struct_ty) {
            Some(ty::Type::Applied { base, .. }) => match self.store.get_type(*base) {
                Some(ty::Type::Nominal(nid)) => Some(*nid),
                _ => None,
            },
            Some(ty::Type::Nominal(nid)) => Some(*nid),
            _ => None,
        };
        let joined = sc
            .path
            .segments
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>()
            .join("::");
        let Some(nid) = nid else {
            self.diagnostics.push(TypeDiagnostic::new(
                TypeDiagnosticCode::InvalidStructConstruction,
                format!("`{}` is not a struct type", joined),
                sc.path.span,
            ));
            return self.store.intern_type(ty::Type::Error);
        };
        // Clona os fields para encerrar o borrow do store antes de checar os
        // valores (check_expression precisa de `&mut self`).
        let fields: Vec<FieldDefinition> = match self.store.get_nominal_definition(nid) {
            Some(TypeDefinition::Struct(sd)) => sd.fields.clone(),
            _ => {
                self.diagnostics.push(TypeDiagnostic::new(
                    TypeDiagnosticCode::InvalidStructConstruction,
                    format!("`{}` is not a struct type", joined),
                    sc.path.span,
                ));
                return self.store.intern_type(ty::Type::Error);
            }
        };
        // Valida (§132): field existe, aparece uma vez, todos fornecidos, valor compatível.
        let mut seen: Vec<&str> = Vec::new();
        for f in &sc.fields {
            let df = fields.iter().find(|df| df.name == f.name.name);
            let Some(df) = df else {
                self.diagnostics.push(TypeDiagnostic::new(
                    TypeDiagnosticCode::UnknownStructField,
                    format!("unknown field `{}`", f.name.name),
                    f.name.span,
                ));
                continue;
            };
            if seen.contains(&f.name.name.as_str()) {
                self.diagnostics.push(TypeDiagnostic::new(
                    TypeDiagnosticCode::DuplicateStructField,
                    format!("duplicate field `{}`", f.name.name),
                    f.name.span,
                ));
                continue;
            }
            seen.push(f.name.name.as_str());
            // §132/§138: field não exportado não pode ser construído fora do
            // módulo de origem (mesmo módulo → permitido, §139).
            let struct_module = self
                .store
                .get_nominal(nid)
                .map(|n| n.module)
                .unwrap_or(self.module_id);
            if !df.exported && struct_module != self.module_id {
                self.diagnostics.push(TypeDiagnostic::new(
                    TypeDiagnosticCode::MemberNotAccessible,
                    format!("field `{}` is not accessible", f.name.name),
                    f.name.span,
                ));
            }
            let vt = self.check_expression(&f.expr, Some(df.ty));
            if !nexa_types::argument_compatible(&self.store, vt, df.ty) {
                self.diagnostics.push(TypeDiagnostic::new(
                    TypeDiagnosticCode::TypeMismatch,
                    format!(
                        "field `{}` type mismatch: expected {}, got {}",
                        f.name.name,
                        nexa_types::format::format_type(&self.store, df.ty),
                        nexa_types::format::format_type(&self.store, vt)
                    ),
                    f.span,
                ));
            }
        }
        for df in &fields {
            if !seen.contains(&df.name.as_str()) {
                self.diagnostics.push(TypeDiagnostic::new(
                    TypeDiagnosticCode::MissingStructField,
                    format!("missing field `{}`", df.name),
                    sc.span,
                ));
            }
        }
        struct_ty
    }

    fn check_ident(
        &mut self,
        span: SourceSpan,
        expected: Option<TypeId>,
        whole: SourceSpan,
    ) -> TypeId {
        let Some(sym) = self.symbol_at(span) else {
            self.diagnostics.push(TypeDiagnostic::new(
                TypeDiagnosticCode::UnknownType,
                "unknown identifier".to_string(),
                whole,
            ));
            return self.store.intern_type(ty::Type::Error);
        };
        let kind = self.index.symbols.get(sym).map(|d| d.kind);
        match kind {
            Some(SymbolKind::LocalLet)
            | Some(SymbolKind::LocalVar)
            | Some(SymbolKind::LocalConst)
            | Some(SymbolKind::Parameter)
            | Some(SymbolKind::Receiver) => {
                self.local_types.get(&sym).copied().unwrap_or_else(|| {
                    // Fallback: parâmetro de callable corrente não bindado.
                    self.diagnostics.push(TypeDiagnostic::new(
                        TypeDiagnosticCode::CannotInferType,
                        "cannot determine type of local binding".to_string(),
                        whole,
                    ));
                    self.store.intern_type(ty::Type::Error)
                })
            }
            Some(SymbolKind::Const) => self
                .type_by_symbol
                .get(&sym)
                .copied()
                .unwrap_or(self.prelude.unit),
            Some(SymbolKind::Function) | Some(SymbolKind::Action) => self
                .callable_by_symbol
                .get(&sym)
                .cloned()
                .map(|sig| self.store.intern_type(ty::Type::Callable(sig)))
                .unwrap_or(self.prelude.unit),
            Some(SymbolKind::EnumVariant) => self.check_variant_value(sym, expected, whole),
            Some(SymbolKind::Field) => {
                self.diagnostics.push(TypeDiagnostic::new(
                    TypeDiagnosticCode::UnknownMember,
                    "field used as standalone value".to_string(),
                    whole,
                ));
                self.store.intern_type(ty::Type::Error)
            }
            _ => {
                self.diagnostics.push(TypeDiagnostic::new(
                    TypeDiagnosticCode::UnknownType,
                    "not a value expression".to_string(),
                    whole,
                ));
                self.store.intern_type(ty::Type::Error)
            }
        }
    }

    fn check_path_value(
        &mut self,
        path: &QualifiedName,
        expected: Option<TypeId>,
        whole: SourceSpan,
    ) -> TypeId {
        let sym = self
            .path_leaf_symbol(path)
            .or_else(|| self.symbol_at(path.span));
        let Some(sym) = sym else {
            self.diagnostics.push(TypeDiagnostic::new(
                TypeDiagnosticCode::UnknownType,
                "unknown path".to_string(),
                whole,
            ));
            return self.store.intern_type(ty::Type::Error);
        };
        let kind = self.index.symbols.get(sym).map(|d| d.kind);
        match kind {
            Some(SymbolKind::EnumVariant) => self.check_variant_value(sym, expected, whole),
            Some(SymbolKind::Function) | Some(SymbolKind::Action) => self
                .callable_by_symbol
                .get(&sym)
                .cloned()
                .map(|sig| self.store.intern_type(ty::Type::Callable(sig)))
                .unwrap_or(self.prelude.unit),
            Some(SymbolKind::Const) => self
                .type_by_symbol
                .get(&sym)
                .copied()
                .unwrap_or(self.prelude.unit),
            _ => {
                self.diagnostics.push(TypeDiagnostic::new(
                    TypeDiagnosticCode::UnknownType,
                    "path does not denote a value".to_string(),
                    whole,
                ));
                self.store.intern_type(ty::Type::Error)
            }
        }
    }

    fn check_variant_value(
        &mut self,
        sym: SymbolId,
        expected: Option<TypeId>,
        whole: SourceSpan,
    ) -> TypeId {
        let Some(rec) = self.variant_by_symbol.get(&sym).cloned() else {
            return self.store.intern_type(ty::Type::Error);
        };
        if !rec.payload.is_empty() {
            // Variante com payload exige construção (Call).
            self.diagnostics.push(TypeDiagnostic::new(
                TypeDiagnosticCode::CannotInferType,
                "enum variant with payload requires construction".to_string(),
                whole,
            ));
            return self.store.intern_type(ty::Type::Error);
        }
        // Variante unitária (None atalho do prelude ou enumerada).
        if self.is_optional_like(rec.owner) {
            if let Some(exp) = expected {
                if self.same_nominal(exp, rec.owner) {
                    return exp;
                }
            }
            self.diagnostics.push(TypeDiagnostic::new(
                TypeDiagnosticCode::CannotInferType,
                "cannot infer type argument of Optional from None".to_string(),
                whole,
            ));
            return self.store.intern_type(ty::Type::Error);
        }
        // Enum genérico: aplica com args desconhecidos → usa o nominal base.
        rec.owner
    }

    fn is_optional_like(&self, ty: TypeId) -> bool {
        self.prelude_nominal.optional == ty
    }

    fn same_nominal(&self, a: TypeId, b: TypeId) -> bool {
        let nominal_of = |t: TypeId| -> Option<nexa_types::id::NominalTypeId> {
            match self.store.get_type(t) {
                Some(ty::Type::Nominal(nid)) => Some(*nid),
                Some(ty::Type::Applied { base, .. }) => match self.store.get_type(*base) {
                    Some(ty::Type::Nominal(nid)) => Some(*nid),
                    _ => None,
                },
                _ => None,
            }
        };
        nominal_of(a).is_some() && nominal_of(a) == nominal_of(b)
    }

    fn check_call(&mut self, callee: &Expr, args: &[Expr], span: SourceSpan) -> TypeId {
        // Callee resolvido a symbol (leaf do path primeiro: módulo/tipo raiz
        // nunca é o alvo de valor).
        let callee_sym = match &callee.kind {
            ExprKind::Ident(i) => self
                .symbol_at(i.span)
                .or_else(|| self.symbol_at(callee.span)),
            ExprKind::Path(p) => self
                .path_leaf_symbol(p)
                .or_else(|| self.symbol_at(callee.span)),
            _ => self.symbol_at(callee.span),
        };

        if let Some(sym) = callee_sym {
            let kind = self.index.symbols.get(sym).map(|d| d.kind);
            if kind == Some(SymbolKind::EnumVariant) {
                return self.check_variant_call(sym, args, span);
            }
            if let Some(sig) = self.callable_by_symbol.get(&sym).cloned() {
                return self.check_callable_call(sig, args, span);
            }
        }

        // Callee como valor callable.
        if let Some(sig) = self.callable_value_of(callee) {
            return self.check_callable_call(sig, args, span);
        }

        self.diagnostics.push(TypeDiagnostic::new(
            TypeDiagnosticCode::NotCallable,
            "expression is not callable".to_string(),
            span,
        ));
        self.store.intern_type(ty::Type::Error)
    }

    fn callable_value_of(&mut self, callee: &Expr) -> Option<CallableType> {
        let ty = self.check_expression(callee, None);
        match self.store.get_type(ty) {
            Some(ty::Type::Callable(ct)) => Some(ct.clone()),
            _ => None,
        }
    }

    fn check_callable_call(
        &mut self,
        sig: CallableType,
        args: &[Expr],
        span: SourceSpan,
    ) -> TypeId {
        if args.len() != sig.parameters.len() {
            self.diagnostics.push(TypeDiagnostic::new(
                TypeDiagnosticCode::ArgumentCountMismatch,
                format!(
                    "expected {} arguments, got {}",
                    sig.parameters.len(),
                    args.len()
                ),
                span,
            ));
            return self.store.intern_type(ty::Type::Error);
        }
        // 1. Checa os argumentos (com o tipo declarado como contexto) e
        //    coleta os tipos reais para validação e inferência de genéricos.
        let arg_tys: Vec<TypeId> = args
            .iter()
            .zip(sig.parameters.iter())
            .map(|(a, pt)| self.check_expression(a, Some(*pt)))
            .collect();
        // 2. Inferência de genéricos (§109): bind do tipo do argumento para
        //    cada `GenericParamId` da assinatura; depois valida com a versão
        //    substituída e propaga a substituição para o retorno.
        let mut map: HashMap<GenericParamId, TypeId> = default_map();
        for (pt, at) in sig.parameters.iter().zip(arg_tys.iter()) {
            self.infer_generic_bindings(*pt, *at, &mut map);
        }
        // Constraints (§385-388): cada `GenericParamId` com bound deve ter um
        // implement aplicável para o tipo inferido; se o tipo permanece
        // genérico no call site, o caller genérico satisfaz simbolicamente.
        for gpid in &sig.generic_params {
            let constraints = self.store.get_generic_param_constraints(*gpid);
            if constraints.is_empty() {
                continue;
            }
            let param_ty = self.store.intern_type(ty::Type::GenericParameter(*gpid));
            let resolved = self.store.substitute_type(param_ty, &map);
            if matches!(
                self.store.get_type(resolved),
                Some(ty::Type::GenericParameter(_))
            ) {
                continue;
            }
            for c in constraints {
                if self.store.has_interface(resolved, c.interface).is_none() {
                    self.diagnostics.push(TypeDiagnostic::new(
                        TypeDiagnosticCode::GenericConstraintNotSatisfied,
                        format!(
                            "type {} does not satisfy interface `{}`",
                            nexa_types::format::format_type(&self.store, resolved),
                            self.interface_name(c.interface)
                        ),
                        span,
                    ));
                }
            }
        }
        let sub_params: Vec<TypeId> = sig
            .parameters
            .iter()
            .map(|p| self.store.substitute_type(*p, &map))
            .collect();
        for ((arg, at), st) in args.iter().zip(arg_tys.iter()).zip(sub_params.iter()) {
            if !nexa_types::argument_compatible(&self.store, *at, *st) {
                self.diagnostics.push(TypeDiagnostic::new(
                    TypeDiagnosticCode::InvalidArgumentType,
                    format!(
                        "argument type mismatch: expected {}, got {}",
                        nexa_types::format::format_type(&self.store, *st),
                        nexa_types::format::format_type(&self.store, *at)
                    ),
                    arg.span,
                ));
            }
        }
        self.store.substitute_type(sig.return_type, &map)
    }

    /// Unifica estruturalmente o tipo do argumento com o parâmetro declarado,
    /// registrando bindings de genéricos no `map` (§109). Nada é reportado:
    /// inconsistências nascem na validação pós-substituição.
    fn infer_generic_bindings(
        &self,
        param_ty: TypeId,
        arg_ty: TypeId,
        map: &mut HashMap<GenericParamId, TypeId>,
    ) {
        match (self.store.get_type(param_ty), self.store.get_type(arg_ty)) {
            (Some(ty::Type::GenericParameter(gp)), _) => {
                map.entry(*gp).or_insert(arg_ty);
            }
            (Some(ty::Type::Ref(ip)), Some(ty::Type::Ref(ia)))
            | (Some(ty::Type::MutRef(ip)), Some(ty::Type::MutRef(ia))) => {
                self.infer_generic_bindings(*ip, *ia, map);
            }
            (Some(ty::Type::Array(ip)), Some(ty::Type::Array(ia))) => {
                self.infer_generic_bindings(*ip, *ia, map);
            }
            (
                Some(ty::Type::Applied { arguments: pa, .. }),
                Some(ty::Type::Applied { arguments: aa, .. }),
            ) if pa.len() == aa.len() => {
                for (p, a) in pa.iter().zip(aa.iter()) {
                    self.infer_generic_bindings(*p, *a, map);
                }
            }
            _ => {}
        }
    }

    fn check_variant_call(&mut self, sym: SymbolId, args: &[Expr], _span: SourceSpan) -> TypeId {
        let Some(rec) = self.variant_by_symbol.get(&sym).cloned() else {
            return self.store.intern_type(ty::Type::Error);
        };
        if args.len() != rec.payload.len() {
            self.diagnostics.push(TypeDiagnostic::new(
                TypeDiagnosticCode::InvalidEnumVariantPayload,
                format!(
                    "variant expects {} payload value(s), got {}",
                    rec.payload.len(),
                    args.len()
                ),
                _span,
            ));
            return self.store.intern_type(ty::Type::Error);
        }
        let payload_types: Vec<TypeId> = rec
            .payload
            .iter()
            .zip(args.iter())
            .map(|(pt, arg)| {
                let expected = self.store.substitute_type(*pt, &default_map());
                let arg_ty = self.check_expression(arg, Some(expected));
                // Payload genérico (ex.: `Optional<T>`) é inferido do
                // argumento; só valida-se contra payload concreto.
                let concrete = !matches!(
                    self.store.get_type(expected),
                    Some(ty::Type::GenericParameter(_))
                );
                if concrete && !compatibility::assignable(&self.store, arg_ty, expected) {
                    self.diagnostics.push(TypeDiagnostic::new(
                        TypeDiagnosticCode::InvalidEnumVariantPayload,
                        format!(
                            "variant payload type does not match: expected `{}`, found `{}`",
                            nexa_types::format::format_type(&self.store, expected),
                            nexa_types::format::format_type(&self.store, arg_ty)
                        ),
                        arg.span,
                    ));
                }
                arg_ty
            })
            .collect();

        // Optional/Result do prelude.
        if self.is_optional_like(rec.owner) && args.len() == 1 {
            return self
                .prelude_nominal
                .optional_of(&mut self.store, payload_types[0]);
        }
        if self.same_nominal(self.prelude_nominal.result, rec.owner) && args.len() == 1 {
            return self.prelude_nominal.result_of(
                &mut self.store,
                if sym == self.prelude_variants.success {
                    payload_types[0]
                } else {
                    self.prelude.never
                },
                if sym == self.prelude_variants.failure {
                    payload_types[0]
                } else {
                    self.prelude.never
                },
            );
        }
        // Enum de usuário.
        rec.owner
    }

    fn check_if(&mut self, if_expr: &nexa_ast::IfExpr, expected: Option<TypeId>) -> TypeId {
        self.check_expression(&if_expr.condition, Some(self.prelude.bool));
        let then_ty = self.check_block(&if_expr.then_block, expected);
        let mut result_ty = then_ty;
        for ei in &if_expr.else_ifs {
            self.check_expression(&ei.condition, Some(self.prelude.bool));
            let branch_ty = self.check_block(&ei.block, expected);
            result_ty = self.join_types(branch_ty, result_ty);
        }
        if let Some(else_block) = &if_expr.else_block {
            if self.check_block(else_block, expected) == self.prelude.never {
                // else diverge: tipo = then.
            } else {
                let else_ty = self.check_block(else_block, expected);
                result_ty = self.join_types(result_ty, else_ty);
            }
        } else {
            // Sem else: if é statement-like → Unit.
            if !self.is_never(result_ty) {
                result_ty = self.prelude.unit;
            }
        }
        result_ty
    }

    fn join_types(&mut self, a: TypeId, b: TypeId) -> TypeId {
        if self.is_never(a) {
            return b;
        }
        if self.is_never(b) {
            return a;
        }
        if nexa_types::same_type(&self.store, a, b) {
            return a;
        }
        self.prelude.unit
    }

    fn is_never(&self, ty: TypeId) -> bool {
        matches!(self.store.get_type(ty), Some(ty::Type::Never))
    }

    fn check_match(
        &mut self,
        match_expr: &nexa_ast::MatchExpr,
        expected: Option<TypeId>,
    ) -> TypeId {
        let scrutinee_ty = self.check_expression(&match_expr.scrutinee, None);
        let mut result_ty: Option<TypeId> = None;
        for arm in &match_expr.arms {
            let arm_ty = self.check_arm(arm, scrutinee_ty, expected);
            if let Some(r) = result_ty.as_ref() {
                // Compatibilidade entre o arm corrente e os anteriores (§match
                // arm type compatibility): arm de tipo divergente é 0023.
                if !self.is_never(arm_ty)
                    && !self.is_never(*r)
                    && !compatibility::assignable(&self.store, arm_ty, *r)
                {
                    self.diagnostics.push(TypeDiagnostic::new(
                        TypeDiagnosticCode::MatchArmTypeMismatch,
                        format!(
                            "match arm type does not match previous arms: expected `{}`, found `{}`",
                            nexa_types::format::format_type(&self.store, *r),
                            nexa_types::format::format_type(&self.store, arm_ty)
                        ),
                        arm.span,
                    ));
                    continue;
                }
            }
            result_ty = Some(match result_ty {
                None => arm_ty,
                Some(r) => self.join_types(r, arm_ty),
            });
        }
        match result_ty {
            Some(t) => t,
            None => self.prelude.never,
        }
    }

    fn check_arm(
        &mut self,
        arm: &MatchArm,
        scrutinee_ty: TypeId,
        expected: Option<TypeId>,
    ) -> TypeId {
        self.check_pattern(&arm.pattern, scrutinee_ty);
        if let Some(guard) = &arm.guard {
            let gt = self.check_expression(guard, Some(self.prelude.bool));
            if !compatibility::is_bool(&self.store, gt) {
                self.diagnostics.push(TypeDiagnostic::new(
                    TypeDiagnosticCode::MatchGuardMustBeBool,
                    "match guard must be Bool".to_string(),
                    arm.span,
                ));
            }
        }
        self.check_expression(&arm.body, expected)
    }

    fn check_pattern(&mut self, pat: &Pattern, scrutinee_ty: TypeId) {
        match &pat.kind {
            PatternKind::Literal(expr) => {
                let lt = self.check_expression(expr, Some(scrutinee_ty));
                if !compatibility::assignable(&self.store, lt, scrutinee_ty) {
                    self.diagnostics.push(TypeDiagnostic::new(
                        TypeDiagnosticCode::InvalidPatternType,
                        "pattern literal type does not match scrutinee".to_string(),
                        pat.span,
                    ));
                }
            }
            PatternKind::Ident(ident) => {
                if let Some(sym) = self.symbol_lookup(ident.span) {
                    self.local_types.insert(sym, scrutinee_ty);
                }
            }
            PatternKind::Wildcard | PatternKind::Rest => {}
            PatternKind::Tuple(patterns) => {
                let item = self.store_unpacked(scrutinee_ty);
                for (i, p) in patterns.iter().enumerate() {
                    let t = item.get(i).copied().unwrap_or(self.prelude.unit);
                    self.check_pattern(p, t);
                }
            }
            PatternKind::Struct { path, fields } => {
                let ty = self.resolve_type_path(path, &GenericCtx::new(), path.span);
                if !self.same_nominal(ty, scrutinee_ty)
                    && !compatibility::assignable(&self.store, ty, scrutinee_ty)
                {
                    self.diagnostics.push(TypeDiagnostic::new(
                        TypeDiagnosticCode::InvalidPatternType,
                        "struct pattern does not match scrutinee type".to_string(),
                        pat.span,
                    ));
                }
                let _ = fields;
            }
            PatternKind::Enum {
                variant, pattern, ..
            } => {
                let vsym = variant.name.as_str();
                let variant_ts = self.variant_payload_types_for(scrutinee_ty, vsym);
                match variant_ts {
                    Some(pts) if pts.len() == 1 && pattern.is_some() => {
                        if let Some(p) = pattern {
                            self.check_pattern(p, pts[0]);
                        }
                    }
                    _ => {
                        if let Some(p) = pattern {
                            self.check_pattern(p, self.prelude.unit);
                        }
                    }
                }
            }
            PatternKind::Or(patterns) => {
                for p in patterns {
                    self.check_pattern(p, scrutinee_ty);
                }
            }
            PatternKind::Rename { pattern, alias } => {
                if let Some(sym) = self.symbol_lookup(alias.span) {
                    self.local_types.insert(sym, scrutinee_ty);
                }
                self.check_pattern(pattern, scrutinee_ty);
            }
            PatternKind::Guard { pattern, condition } => {
                let gt = self.check_expression(condition, Some(self.prelude.bool));
                if !compatibility::is_bool(&self.store, gt) {
                    self.diagnostics.push(TypeDiagnostic::new(
                        TypeDiagnosticCode::MatchGuardMustBeBool,
                        "pattern guard must be Bool".to_string(),
                        pat.span,
                    ));
                }
                self.check_pattern(pattern, scrutinee_ty);
            }
        }
    }

    fn store_unpacked(&self, ty: TypeId) -> Vec<TypeId> {
        match self.store.get_type(ty) {
            Some(ty::Type::Applied { arguments, .. }) => arguments.clone(),
            _ => Vec::new(),
        }
    }

    fn variant_payload_types_for(&self, ty: TypeId, variant_name: &str) -> Option<Vec<TypeId>> {
        // Prelude Optional/Result.
        if self.same_nominal(self.prelude_nominal.optional, ty) {
            let inner = match self.store.get_type(ty) {
                Some(ty::Type::Applied { arguments, .. }) => arguments.first().copied(),
                _ => None,
            };
            return match variant_name {
                "Some" => inner.map(|t| vec![t]),
                "None" => Some(vec![]),
                _ => None,
            };
        }
        if self.same_nominal(self.prelude_nominal.result, ty) {
            let args = match self.store.get_type(ty) {
                Some(ty::Type::Applied { arguments, .. }) => arguments.clone(),
                _ => Vec::new(),
            };
            return match variant_name {
                "Success" => args.first().map(|t| vec![*t]),
                "Failure" => args.get(1).map(|t| vec![*t]),
                _ => None,
            };
        }
        // Enum de usuário: procura definição nominal.
        let nid = match self.store.get_type(ty) {
            Some(ty::Type::Nominal(nid)) => *nid,
            Some(ty::Type::Applied { base, .. }) => match self.store.get_type(*base) {
                Some(ty::Type::Nominal(nid)) => *nid,
                _ => return None,
            },
            _ => return None,
        };
        match self.store.get_nominal_definition(nid) {
            Some(TypeDefinition::Enum(e)) => e
                .variants
                .iter()
                .find(|v| v.name == variant_name)
                .map(|v| match &v.kind {
                    VariantKind::Unit => vec![],
                    VariantKind::Tuple(ts) => ts.clone(),
                    VariantKind::Struct(_) => Vec::new(),
                }),
            _ => None,
        }
    }

    fn iterable_element(
        &mut self,
        span: SourceSpan,
        iterable: &Expr,
        expected: Option<TypeId>,
    ) -> Option<TypeId> {
        let ty = self.check_expression(iterable, expected);
        match self.store.get_type(ty) {
            Some(ty::Type::Array(inner)) => Some(*inner),
            Some(ty::Type::Bytes) => Some(self.prelude.uint8),
            _ => {
                self.diagnostics.push(TypeDiagnostic::new(
                    TypeDiagnosticCode::NotIterable,
                    "value is not iterable".to_string(),
                    span,
                ));
                None
            }
        }
    }

    fn check_field_access(
        &mut self,
        target_ty: TypeId,
        field_name: &str,
        span: SourceSpan,
    ) -> TypeId {
        // Desvenda Ref/MutRef para chegar ao nominal.
        let base = match self.store.get_type(target_ty) {
            Some(ty::Type::Ref(inner)) | Some(ty::Type::MutRef(inner)) => *inner,
            _ => target_ty,
        };
        // Tupla placeholder `Applied(Unit, args)`: `.0`, `.1`, ... indexam a
        // posição. Índice fora do range ou não-numérico → UnknownMember.
        if let Some(ty::Type::Applied { base: b, arguments }) = self.store.get_type(base) {
            if *b == self.prelude.unit && field_name.chars().all(|c| c.is_ascii_digit()) {
                if let Ok(idx) = field_name.parse::<usize>() {
                    if let Some(t) = arguments.get(idx) {
                        return *t;
                    }
                }
                self.diagnostics.push(TypeDiagnostic::new(
                    TypeDiagnosticCode::UnknownMember,
                    format!("tuple index `{}` out of range", field_name),
                    span,
                ));
                return self.store.intern_type(ty::Type::Error);
            }
        }
        let nid = match self.store.get_type(base) {
            Some(ty::Type::Nominal(nid)) => Some(*nid),
            Some(ty::Type::Applied { base: b, .. }) => match self.store.get_type(*b) {
                Some(ty::Type::Nominal(nid)) => Some(*nid),
                _ => None,
            },
            _ => None,
        };
        let Some(nid) = nid else {
            self.diagnostics.push(TypeDiagnostic::new(
                TypeDiagnosticCode::UnknownMember,
                format!("type does not have fields: {}", field_name),
                span,
            ));
            return self.store.intern_type(ty::Type::Error);
        };
        match self.store.get_nominal_definition(nid) {
            Some(TypeDefinition::Struct(s)) => match s.fields.iter().find(|f| f.name == field_name)
            {
                Some(f) => f.ty,
                None => {
                    self.diagnostics.push(TypeDiagnostic::new(
                        TypeDiagnosticCode::UnknownMember,
                        format!("unknown field `{}`", field_name),
                        span,
                    ));
                    self.store.intern_type(ty::Type::Error)
                }
            },
            Some(TypeDefinition::Enum(e)) => match e.variants.iter().find(|v| v.name == field_name)
            {
                // N/A: campos de enum são acessados via pattern; UnknownMember.
                Some(_) => {
                    self.diagnostics.push(TypeDiagnostic::new(
                        TypeDiagnosticCode::UnknownMember,
                        "enum variants are matched, not accessed as fields".to_string(),
                        span,
                    ));
                    self.store.intern_type(ty::Type::Error)
                }
                None => {
                    self.diagnostics.push(TypeDiagnostic::new(
                        TypeDiagnosticCode::UnknownMember,
                        format!("unknown member `{}`", field_name),
                        span,
                    ));
                    self.store.intern_type(ty::Type::Error)
                }
            },
            _ => {
                self.diagnostics.push(TypeDiagnostic::new(
                    TypeDiagnosticCode::UnknownMember,
                    format!("unknown member `{}`", field_name),
                    span,
                ));
                self.store.intern_type(ty::Type::Error)
            }
        }
    }

    fn check_method_call(
        &mut self,
        target_ty: TypeId,
        method_name: &str,
        args: &[Expr],
        span: SourceSpan,
    ) -> TypeId {
        // Busca o método via implementações de interface para o nominal de target.
        let recv_ty = match self.store.get_type(target_ty) {
            Some(ty::Type::Ref(inner)) | Some(ty::Type::MutRef(inner)) => *inner,
            _ => target_ty,
        };
        let interfaces = self.store.interfaces_of(recv_ty);
        let mut candidates: Vec<(CallableType, TypeId)> = Vec::new();
        // Receptor é um parâmetro genérico com constraints (§385-393):
        // `where T: Show` permite `x.show()` dentro do corpo genérico.
        if let Some(ty::Type::GenericParameter(gpid)) = self.store.get_type(recv_ty) {
            let ccts = self.store.get_generic_param_constraints(*gpid);
            eprintln!(
                "DBG method-call generic: gpid={:?} recv={:?} name={:?} constraints={:?}",
                gpid,
                self.store.get_type(recv_ty),
                method_name,
                ccts.iter()
                    .map(|c| self.interface_name(c.interface))
                    .collect::<Vec<_>>()
            );
            for c in self.store.get_generic_param_constraints(*gpid) {
                if let Some((_, def, _)) = self.interface_def(c.interface) {
                    for member in def.members.deref_members() {
                        if self
                            .index
                            .symbols
                            .get(member.symbol)
                            .map(|s| self.index.name_of(s.name) == method_name)
                            .unwrap_or(false)
                        {
                            candidates.push((member.callable.clone(), c.interface));
                        }
                    }
                }
            }
        }
        // Receptor é diretamente um tipo de interface: o método busca nos
        // próprios membros da interface (ex.: `let s: Shape = r; s.area()`
        // — dispatch via tipo de interface).
        if let Some((_, def, _)) = self.interface_def(recv_ty) {
            for member in def.members.deref_members() {
                if self
                    .index
                    .symbols
                    .get(member.symbol)
                    .map(|s| self.index.name_of(s.name) == method_name)
                    .unwrap_or(false)
                {
                    candidates.push((member.callable.clone(), recv_ty));
                }
            }
        }
        for (iface, _impl_id) in interfaces {
            if let Some((_, def, _)) = self.interface_def(iface) {
                for member in def.members.deref_members() {
                    if self
                        .index
                        .symbols
                        .get(member.symbol)
                        .map(|s| self.index.name_of(s.name) == method_name)
                        .unwrap_or(false)
                    {
                        candidates.push((member.callable.clone(), iface));
                    }
                }
            }
        }
        if candidates.is_empty() {
            self.diagnostics.push(TypeDiagnostic::new(
                TypeDiagnosticCode::UnknownMember,
                format!("unknown method `{}`", method_name),
                span,
            ));
            return self.store.intern_type(ty::Type::Error);
        }
        // Nome único entre membros aplicáveis (§176-178): a chamada só
        // resolve quando ≤1 interface distinta fornece o método; de outra
        // forma o acesso é ambíguo (NEXA-TYPE-0034).
        let distinct_ifaces: HashSet<TypeId> = candidates.iter().map(|(_, i)| *i).collect();
        if distinct_ifaces.len() > 1 {
            self.diagnostics.push(TypeDiagnostic::new(
                TypeDiagnosticCode::AmbiguousMember,
                format!(
                    "member `{}` is ambiguous: found in {} applicable interfaces",
                    method_name,
                    distinct_ifaces.len()
                ),
                span,
            ));
            return self.store.intern_type(ty::Type::Error);
        }
        let (sig, _iface) = candidates.swap_remove(0);
        // Params do membro de interface não incluem o receiver (§98-103):
        // compare os argumentos contra todos os parâmetros explícitos.
        let param_slice = &sig.parameters;
        if args.len() != param_slice.len() {
            self.diagnostics.push(TypeDiagnostic::new(
                TypeDiagnosticCode::ArgumentCountMismatch,
                format!(
                    "method `{}` expects {} argument(s), got {}",
                    method_name,
                    param_slice.len(),
                    args.len()
                ),
                span,
            ));
            return self.store.intern_type(ty::Type::Error);
        }
        for (arg, param_ty) in args.iter().zip(param_slice.iter()) {
            let at = self.check_expression(arg, Some(*param_ty));
            if !nexa_types::argument_compatible(&self.store, at, *param_ty) {
                self.diagnostics.push(TypeDiagnostic::new(
                    TypeDiagnosticCode::InvalidArgumentType,
                    "method argument type mismatch".to_string(),
                    arg.span,
                ));
            }
        }
        sig.return_type
    }

    fn check_index(&mut self, target_ty: TypeId, index_ty: TypeId, span: SourceSpan) -> TypeId {
        match self.store.get_type(target_ty) {
            Some(ty::Type::Array(inner)) => {
                if !compatibility::is_integer(&self.store, index_ty) {
                    self.diagnostics.push(TypeDiagnostic::new(
                        TypeDiagnosticCode::InvalidIndexType,
                        "array index must be integer".to_string(),
                        span,
                    ));
                }
                *inner
            }
            Some(ty::Type::Bytes) => {
                if !compatibility::is_integer(&self.store, index_ty) {
                    self.diagnostics.push(TypeDiagnostic::new(
                        TypeDiagnosticCode::InvalidIndexType,
                        "bytes index must be integer".to_string(),
                        span,
                    ));
                }
                self.prelude.uint8
            }
            _ => {
                self.diagnostics.push(TypeDiagnostic::new(
                    TypeDiagnosticCode::NotIndexable,
                    "type is not indexable".to_string(),
                    span,
                ));
                self.store.intern_type(ty::Type::Error)
            }
        }
    }

    fn check_await(&mut self, inner_ty: TypeId, span: SourceSpan) -> TypeId {
        match self.store.get_type(inner_ty) {
            Some(ty::Type::Task(inner)) => *inner,
            _ => {
                self.diagnostics.push(TypeDiagnostic::new(
                    TypeDiagnosticCode::AwaitRequiresTask,
                    "await requires Task<T>".to_string(),
                    span,
                ));
                self.store.intern_type(ty::Type::Error)
            }
        }
    }

    fn check_try(&mut self, inner_ty: TypeId, span: SourceSpan) -> TypeId {
        match self.store.get_type(inner_ty) {
            Some(ty::Type::Applied { base, arguments }) if self.store.get_type(*base).is_some() => {
                // Result<T,E>: extrai T.
                if self.same_nominal(self.prelude_nominal.result, inner_ty) && !arguments.is_empty()
                {
                    arguments[0]
                } else {
                    self.prelude.unit
                }
            }
            _ => {
                self.diagnostics.push(TypeDiagnostic::new(
                    TypeDiagnosticCode::TryRequiresResultContext,
                    "try requires Result<T,E>".to_string(),
                    span,
                ));
                self.store.intern_type(ty::Type::Error)
            }
        }
    }

    // ─── Operators ─────────────────────────────────────────────────

    fn validate_operator(
        &mut self,
        op: nexa_ast::BinaryOp,
        lty: TypeId,
        rty: TypeId,
        span: SourceSpan,
    ) -> TypeId {
        use nexa_ast::BinaryOp::*;
        let not_unit = |t: TypeId| self.store.get_type(t) != Some(&ty::Type::Unit);
        match op {
            Add | Sub | Mul | Div | Rem => {
                if !self.promote_numeric(&lty, &rty)
                    || !nexa_types::same_type(&self.store, lty, rty)
                {
                    self.diagnostics.push(TypeDiagnostic::new(
                        TypeDiagnosticCode::InvalidOperatorOperands,
                        "arithmetic operands must be numeric of the same type".to_string(),
                        span,
                    ));
                    return self.store.intern_type(ty::Type::Error);
                }
                if !not_unit(lty) {
                    self.diagnostics.push(TypeDiagnostic::new(
                        TypeDiagnosticCode::InvalidOperatorOperands,
                        "arithmetic operands must be numeric".to_string(),
                        span,
                    ));
                    return self.store.intern_type(ty::Type::Error);
                }
                if !compatibility::is_numeric(&self.store, lty) {
                    self.diagnostics.push(TypeDiagnostic::new(
                        TypeDiagnosticCode::InvalidOperatorOperands,
                        "arithmetic operator requires numeric types".to_string(),
                        span,
                    ));
                    return self.store.intern_type(ty::Type::Error);
                }
                lty
            }
            Eq | Ne => {
                if !nexa_types::same_type(&self.store, lty, rty) {
                    self.diagnostics.push(TypeDiagnostic::new(
                        TypeDiagnosticCode::InvalidOperatorOperands,
                        "equality operands must be same type".to_string(),
                        span,
                    ));
                    return self.store.intern_type(ty::Type::Error);
                }
                if !compatibility::is_eq_capable(&self.store, lty) {
                    self.diagnostics.push(TypeDiagnostic::new(
                        TypeDiagnosticCode::InvalidOperatorOperands,
                        "equality requires Eq-capable type".to_string(),
                        span,
                    ));
                    return self.store.intern_type(ty::Type::Error);
                }
                self.prelude.bool
            }
            Lt | Le | Gt | Ge => {
                if !nexa_types::same_type(&self.store, lty, rty) {
                    self.diagnostics.push(TypeDiagnostic::new(
                        TypeDiagnosticCode::InvalidOperatorOperands,
                        "comparison operands must be same type".to_string(),
                        span,
                    ));
                    return self.store.intern_type(ty::Type::Error);
                }
                if !compatibility::is_comparable(&self.store, lty) {
                    self.diagnostics.push(TypeDiagnostic::new(
                        TypeDiagnosticCode::InvalidOperatorOperands,
                        "comparison requires Comparable type".to_string(),
                        span,
                    ));
                    return self.store.intern_type(ty::Type::Error);
                }
                self.prelude.bool
            }
            And | Or => {
                if !compatibility::is_bool(&self.store, lty)
                    || !compatibility::is_bool(&self.store, rty)
                {
                    self.diagnostics.push(TypeDiagnostic::new(
                        TypeDiagnosticCode::InvalidOperatorOperands,
                        "logical operators require Bool".to_string(),
                        span,
                    ));
                    return self.store.intern_type(ty::Type::Error);
                }
                self.prelude.bool
            }
            BitAnd | BitOr | BitXor => {
                if !nexa_types::same_type(&self.store, lty, rty) {
                    self.diagnostics.push(TypeDiagnostic::new(
                        TypeDiagnosticCode::InvalidOperatorOperands,
                        "bitwise operands must be same integer type".to_string(),
                        span,
                    ));
                    return self.store.intern_type(ty::Type::Error);
                }
                if !compatibility::is_integer(&self.store, lty) {
                    self.diagnostics.push(TypeDiagnostic::new(
                        TypeDiagnosticCode::InvalidOperatorOperands,
                        "bitwise operators require integer types".to_string(),
                        span,
                    ));
                    return self.store.intern_type(ty::Type::Error);
                }
                lty
            }
            Shl | Shr => {
                if !compatibility::is_integer(&self.store, lty) {
                    self.diagnostics.push(TypeDiagnostic::new(
                        TypeDiagnosticCode::InvalidOperatorOperands,
                        "shift requires integer type on LHS".to_string(),
                        span,
                    ));
                    return self.store.intern_type(ty::Type::Error);
                }
                if !compatibility::is_integer(&self.store, rty) {
                    self.diagnostics.push(TypeDiagnostic::new(
                        TypeDiagnosticCode::InvalidOperatorOperands,
                        "shift RHS must be integer".to_string(),
                        span,
                    ));
                    return self.store.intern_type(ty::Type::Error);
                }
                lty
            }
        }
    }

    /// Promoção literal: um literal (Int) pode casar com Int32 context.
    fn promote_numeric(&self, _l: &TypeId, _r: &TypeId) -> bool {
        // Ambos inteiros → promoção automática é ok (default Int);
        // consequentemente requiremos same_type depois.
        true
    }

    fn validate_unary_operator(
        &mut self,
        op: nexa_ast::UnaryOp,
        ity: TypeId,
        span: SourceSpan,
    ) -> TypeId {
        use nexa_ast::UnaryOp::*;
        match op {
            Not => {
                if !compatibility::is_bool(&self.store, ity) {
                    self.diagnostics.push(TypeDiagnostic::new(
                        TypeDiagnosticCode::InvalidOperatorOperands,
                        "! requires Bool".to_string(),
                        span,
                    ));
                    self.store.intern_type(ty::Type::Error)
                } else {
                    self.prelude.bool
                }
            }
            Neg => {
                if !compatibility::is_signed_integer(&self.store, ity)
                    && !compatibility::is_float(&self.store, ity)
                {
                    self.diagnostics.push(TypeDiagnostic::new(
                        TypeDiagnosticCode::InvalidOperatorOperands,
                        "unary minus requires signed integer or float".to_string(),
                        span,
                    ));
                    self.store.intern_type(ty::Type::Error)
                } else {
                    ity
                }
            }
            BitNot => {
                if !compatibility::is_integer(&self.store, ity) {
                    self.diagnostics.push(TypeDiagnostic::new(
                        TypeDiagnosticCode::InvalidOperatorOperands,
                        "~ requires integer type".to_string(),
                        span,
                    ));
                    self.store.intern_type(ty::Type::Error)
                } else {
                    ity
                }
            }
            Plus => {
                if !compatibility::is_integer(&self.store, ity)
                    && !compatibility::is_float(&self.store, ity)
                {
                    self.diagnostics.push(TypeDiagnostic::new(
                        TypeDiagnosticCode::InvalidOperatorOperands,
                        "unary plus requires numeric type".to_string(),
                        span,
                    ));
                    self.store.intern_type(ty::Type::Error)
                } else {
                    ity
                }
            }
        }
    }

    fn materialize_int(&mut self, val: i64, expected: TypeId, span: SourceSpan) -> TypeId {
        if !compatibility::is_integer(&self.store, expected)
            && !compatibility::is_float(&self.store, expected)
            && !self.is_never(expected)
        {
            return self.prelude.int;
        }
        let fits = match self.store.get_type(expected) {
            Some(ty::Type::Int) => true,
            Some(ty::Type::UInt) => val >= 0,
            Some(ty::Type::Int8) => (i8::MIN as i64) <= val && val <= (i8::MAX as i64),
            Some(ty::Type::Int16) => (i16::MIN as i64) <= val && val <= (i16::MAX as i64),
            Some(ty::Type::Int32) => (i32::MIN as i64) <= val && val <= (i32::MAX as i64),
            Some(ty::Type::Int64) => true,
            Some(ty::Type::UInt8) => 0 <= val && val <= (u8::MAX as i64),
            Some(ty::Type::UInt16) => 0 <= val && val <= (u16::MAX as i64),
            Some(ty::Type::UInt32) => 0 <= val && val <= (u32::MAX as i64),
            Some(ty::Type::UInt64) => 0 <= val,
            Some(ty::Type::Float32) | Some(ty::Type::Float64) => true,
            _ => true,
        };
        if !fits {
            self.diagnostics.push(TypeDiagnostic::new(
                TypeDiagnosticCode::NumericLiteralOutOfRange,
                "numeric literal out of range for type".to_string(),
                span,
            ));
        }
        expected
    }
}

impl Default for TypeChecker {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Helpers ────────────────────────────────────────────────────────

struct TypeCheckerInput {
    index: SemanticIndex,
    module_id: ModuleId,
    source_id: SourceId,
}

fn decl_ident(kind: &ItemKind) -> Option<&nexa_ast::Ident> {
    match kind {
        ItemKind::Function(f) => Some(&f.name),
        ItemKind::Action(a) => Some(&a.name),
        ItemKind::Struct(s) => Some(&s.name),
        ItemKind::Enum(e) => Some(&e.name),
        ItemKind::Interface(i) => Some(&i.name),
        ItemKind::TypeAlias(a) => Some(&a.name),
        ItemKind::Const(c) => Some(&c.name),
        ItemKind::Implement(_) => None,
    }
}

fn generic_ctx(params: &[(String, GenericParamId)]) -> GenericCtx {
    params.iter().cloned().collect()
}

fn visibility_of(item: &Item) -> Visibility {
    if item.exported {
        Visibility::Public
    } else {
        Visibility::ModulePrivate
    }
}

fn symbol_of_associated(tc: &TypeChecker, span: SourceSpan) -> SymbolId {
    // Assoc definitions (fields, variants, methods) são registrados pelo
    // resolver; procuramos por span do nome da declaração associada.
    for sym in tc.index.symbols.iter() {
        if let Some(ns) = sym.name_span {
            if ns == span {
                return sym.id;
            }
        }
    }
    SymbolId(0)
}

fn default_map() -> HashMap<GenericParamId, TypeId> {
    HashMap::new()
}

// small extension helper for interface member iteration
trait MemberSlice {
    fn deref_members(&self) -> &[nexa_types::ty::InterfaceMemberSignature];
}

impl MemberSlice for Vec<nexa_types::ty::InterfaceMemberSignature> {
    fn deref_members(&self) -> &[nexa_types::ty::InterfaceMemberSignature] {
        self.as_slice()
    }
}
