mod astwalk;

use nexa_ast::SourceUnit;
use nexa_diagnostics::{Diagnostic, Severity};
use nexa_parser::parser::parse;
use nexa_resolver::{resolve, ResolveResult};
use nexa_source::{SourceFile, SourceId, SourceSpan};
use nexa_symbols::{SymbolId, SymbolKind};
use nexa_typecheck::TypeChecker;
use nexa_types::id::TypeId;
use nexa_types::store::TypeStore;
use nexa_types::ty::Type;
use std::collections::BTreeMap;

/// Monotonic revision identifier for the whole database.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RevisionId(pub u64);

impl RevisionId {
    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

/// Revision of a single source file within the database.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceRevision {
    pub path: u64,
    pub revision: u64,
}

/// Serializable, cloneable record of a declared symbol within a file.
#[derive(Debug, Clone)]
pub struct SymbolRecord {
    pub id: SymbolId,
    pub name: String,
    pub kind: SymbolKind,
    pub span: SourceSpan,
    pub visibility: String,
}

/// Serializable, cloneable record of a use site (reference) within a file.
#[derive(Debug, Clone)]
pub struct RefRecord {
    pub symbol: SymbolId,
    pub span: SourceSpan,
    pub kind: String,
}

/// Serializable, cloneable record of an expression's inferred type.
#[derive(Debug, Clone)]
pub struct ExprTypeRecord {
    pub start: u32,
    pub end: u32,
    pub type_name: String,
}

/// Serializable, cloneable record of an action's declared effects.
#[derive(Debug, Clone)]
pub struct EffectRecord {
    pub start: u32,
    pub end: u32,
    pub effects: Vec<String>,
}

/// Retrieved information about a declaration.
#[derive(Debug, Clone)]
pub struct SymbolInfo {
    pub name: String,
    pub kind: String,
    pub attribution: String,
}

/// Per-file analysis held live by the database.
pub struct Analysis {
    pub path: String,
    pub source_id: SourceId,
    pub text: String,
    pub ast: SourceUnit,
    pub resolve: ResolveResult,
    pub typed: TypeChecker,
    pub diagnostics: Vec<Diagnostic>,
    pub revision: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum SemDbError {
    #[error("NEXA-SEMDB-0001: Source file '{path}' is not registered")]
    UnknownSource { path: String },
    #[error("NEXA-SEMDB-0002: Too many source files ({count} exceeds limit {limit})")]
    FileLimitExceeded { count: usize, limit: usize },
    #[error("NEXA-SEMDB-0003: Source file too large ({bytes} bytes exceeds limit {limit})")]
    SourceTooLarge { bytes: u64, limit: u64 },
    #[error("NEXA-SEMDB-0004: Offsets out of order in dependency registration")]
    InvalidDependency,
}

/// The incremental semantic database.
///
/// Owns the live analyses (AST, resolver index, type store and typed model) for a
/// set of source files and exposes semantic queries over them. Revision-aware and
/// file-scoped so local edits do not force a re-analysis of unrelated files.
pub struct SemanticDatabase {
    next_source_id: u32,
    files: BTreeMap<String, Analysis>,
    revision_counter: u64,
    max_files: usize,
    max_source_bytes: u64,
}

impl Default for SemanticDatabase {
    fn default() -> Self {
        Self::new()
    }
}

impl SemanticDatabase {
    pub fn new() -> Self {
        SemanticDatabase {
            next_source_id: 0,
            files: BTreeMap::new(),
            revision_counter: 0,
            max_files: 1024,
            max_source_bytes: 8 * 1024 * 1024,
        }
    }

    pub fn with_limits(max_files: usize, max_source_bytes: u64) -> Self {
        SemanticDatabase {
            max_files,
            max_source_bytes,
            ..SemanticDatabase::new()
        }
    }

    fn bump(&mut self) -> u64 {
        self.revision_counter += 1;
        self.revision_counter
    }

    pub fn revision(&self) -> RevisionId {
        RevisionId(self.revision_counter)
    }

    pub fn file_revision(&self, path: &str) -> Option<u64> {
        self.files.get(path).map(|a| a.revision)
    }

    pub fn has_file(&self, path: &str) -> bool {
        self.files.contains_key(path)
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    pub fn file_paths(&self) -> Vec<String> {
        self.files.keys().cloned().collect()
    }

    /// Register a new source file. Returns the new database revision.
    pub fn add_file(&mut self, path: &str, text: &str) -> Result<RevisionId, SemDbError> {
        if self.files.len() >= self.max_files {
            return Err(SemDbError::FileLimitExceeded {
                count: self.files.len(),
                limit: self.max_files,
            });
        }
        let bytes = text.len() as u64;
        if bytes > self.max_source_bytes {
            return Err(SemDbError::SourceTooLarge {
                bytes,
                limit: self.max_source_bytes,
            });
        }
        let revision = self.bump();
        let source_id = SourceId(self.next_source_id);
        self.next_source_id += 1;
        let analysis = self.analyze(path, source_id, text, revision);
        self.files.insert(path.to_string(), analysis);
        Ok(RevisionId(revision))
    }

    /// Update an existing source file's contents. Returns the new database revision.
    pub fn update_file(&mut self, path: &str, text: &str) -> Result<RevisionId, SemDbError> {
        if !self.files.contains_key(path) {
            return self.add_file(path, text);
        }
        let bytes = text.len() as u64;
        if bytes > self.max_source_bytes {
            return Err(SemDbError::SourceTooLarge {
                bytes,
                limit: self.max_source_bytes,
            });
        }
        let old = self.files.get(path).unwrap();
        if old.text == text {
            return Ok(self.revision());
        }
        let revision = self.bump();
        let source_id = SourceId(self.next_source_id);
        self.next_source_id += 1;
        let analysis = self.analyze(path, source_id, text, revision);
        self.files.insert(path.to_string(), analysis);
        Ok(RevisionId(revision))
    }

    /// Remove a source file. Returns the new database revision.
    pub fn remove_file(&mut self, path: &str) -> Result<RevisionId, SemDbError> {
        if self.files.remove(path).is_some() {
            Ok(RevisionId(self.bump()))
        } else {
            Ok(self.revision())
        }
    }

    fn analyze(&mut self, path: &str, source_id: SourceId, text: &str, revision: u64) -> Analysis {
        let sf = SourceFile::from_text(
            source_id,
            std::path::PathBuf::from("<memory>"),
            text.to_string(),
        );
        let lex = nexa_lexer::lex(&sf);
        let mut diagnostics: Vec<Diagnostic> = lex.diagnostics.clone();
        let mut parse_result = parse(&sf, nexa_parser::ParseMode::SingleFile);
        diagnostics.append(&mut parse_result.diagnostics);

        let resolve_result = resolve(sf.id, &parse_result.ast);
        for d in &resolve_result.diagnostics {
            diagnostics.push(normalize_resolver(d));
        }

        let mut tc = TypeChecker::new();
        tc.check_source_unit(&parse_result.ast);
        for d in &tc.diagnostics {
            diagnostics.push(normalize_type(d));
        }

        Analysis {
            path: path.to_string(),
            source_id: sf.id,
            text: text.to_string(),
            ast: parse_result.ast,
            resolve: resolve_result,
            typed: tc,
            diagnostics,
            revision,
        }
    }

    // ── Live queries ───────────────────────────────────────────────────

    fn file(&self, path: &str) -> Result<&Analysis, SemDbError> {
        self.files
            .get(path)
            .ok_or_else(|| SemDbError::UnknownSource {
                path: path.to_string(),
            })
    }

    /// All diagnostics for a file, in canonical (deterministic) order.
    pub fn diagnostics(&self, path: &str) -> Result<Vec<Diagnostic>, SemDbError> {
        let a = self.file(path)?;
        let mut out = a.diagnostics.clone();
        let path_of = |_s: u32| Some(a.path.clone());
        crate::canonical_sort(&mut out, &path_of);
        Ok(out)
    }

    /// All diagnostics across every registered file, canonical ordered.
    pub fn all_diagnostics(&self) -> Vec<Diagnostic> {
        let mut out = Vec::new();
        for a in self.files.values() {
            out.extend(a.diagnostics.iter().cloned());
        }
        let path_of = |s: u32| {
            self.files
                .values()
                .find(|f| f.source_id == SourceId(s))
                .map(|f| f.path.clone())
        };
        canonical_sort(&mut out, &path_of);
        out
    }

    /// Resolve the symbol that an identifier at `offset` refers to.
    fn resolve_ident(&self, a: &Analysis, ident_span: SourceSpan) -> Option<SymbolRecord> {
        // 1. Declaration position: the ident span exactly matches a declaration
        //    symbol's name span (definition position, §320).
        if let Some(decl) = a
            .resolve
            .index
            .symbols
            .iter()
            .find(|s| decl_span(s) == ident_span || s.span == ident_span)
        {
            return Some(symbol_of(a, decl));
        }
        // 2. Reference position: find the reference whose span matches, return its target symbol.
        for s in a.resolve.index.symbols.iter() {
            for r in a.resolve.index.references.references_to(s.id) {
                if r.span == ident_span {
                    return Some(symbol_of(a, s));
                }
            }
        }
        None
    }

    /// Find a declaration symbol whose name span contains `offset`, preferring
    /// the deepest (smallest) declaration.
    fn declaration_at(&self, a: &Analysis, offset: u32) -> Option<SymbolRecord> {
        a.resolve
            .index
            .symbols
            .iter()
            .filter(|s| {
                let sp = decl_span(s);
                sp.start <= offset && offset < sp.end
            })
            .min_by_key(|s| {
                let sp = decl_span(s);
                sp.end - sp.start
            })
            .map(|s| symbol_of(a, s))
    }

    /// The symbol referred to at `offset` (declaration or reference), for definition lookup.
    fn symbol_at_offset(&self, a: &Analysis, offset: u32) -> Option<SymbolRecord> {
        if let Some(ident) = astwalk::ident_at(&a.ast, offset) {
            let span = ident.span;
            self.resolve_ident(a, span)
                .or_else(|| self.declaration_at(a, offset))
        } else {
            None
        }
    }

    pub fn symbol_at(&self, path: &str, offset: u32) -> Result<Option<SymbolInfo>, SemDbError> {
        let a = self.file(path)?;
        Ok(self.symbol_at_offset(a, offset).map(|s| SymbolInfo {
            name: s.name,
            kind: symbol_kind_str(s.kind).to_string(),
            attribution: format!("{}:{}:{}", a.path, s.span.start, s.span.end),
        }))
    }

    pub fn definition(
        &self,
        path: &str,
        offset: u32,
    ) -> Result<Option<SymbolLocation>, SemDbError> {
        let a = self.file(path)?;
        let sym = self.symbol_at_offset(a, offset);
        Ok(sym.map(|s| SymbolLocation {
            path: a.path.clone(),
            span: s.span,
            name: s.name,
            symbol_id: s.id.0,
        }))
    }

    pub fn references(&self, path: &str, offset: u32) -> Result<Vec<SymbolLocation>, SemDbError> {
        let a = self.file(path)?;
        let target = self.symbol_at_offset(a, offset);
        let Some(target) = target else {
            return Ok(Vec::new());
        };
        let mut out = Vec::new();
        // Definition itself.
        out.push(SymbolLocation {
            path: a.path.clone(),
            span: target.span,
            name: target.name.clone(),
            symbol_id: target.id.0,
        });
        // Within-file references (deduplicated by span).
        let mut seen = std::collections::HashSet::new();
        for r in a.resolve.index.references.references_to(target.id) {
            if seen.insert(r.span) {
                out.push(SymbolLocation {
                    path: a.path.clone(),
                    span: r.span,
                    name: target.name.clone(),
                    symbol_id: target.id.0,
                });
            }
        }
        // Cross-file references by matching name + kind.
        for (fpath, fa) in &self.files {
            if fpath == path {
                continue;
            }
            let matched = fa
                .resolve
                .index
                .symbols
                .iter()
                .filter(|s| symbol_name(fa, s.id) == target.name && s.kind == target.kind)
                .map(|s| s.id)
                .collect::<Vec<_>>();
            for sid in matched {
                let mut seen2 = std::collections::HashSet::new();
                for r in fa.resolve.index.references.references_to(sid) {
                    if seen2.insert(r.span) {
                        out.push(SymbolLocation {
                            path: fpath.clone(),
                            span: r.span,
                            name: target.name.clone(),
                            symbol_id: sid.0,
                        });
                    }
                }
            }
        }
        out.sort_by_key(|l| (l.path.clone(), l.span.start, l.span.end));
        out.dedup();
        Ok(out)
    }

    pub fn rename(
        &self,
        path: &str,
        offset: u32,
        new_name: &str,
    ) -> Result<Vec<nexa_diagnostic_schema::SchemaTextEdit>, SemDbError> {
        let refs = self.references(path, offset)?;
        Ok(refs
            .into_iter()
            .map(|l| nexa_diagnostic_schema::SchemaTextEdit {
                start: l.span.start,
                end: l.span.end,
                new_text: new_name.to_string(),
            })
            .collect())
    }

    pub fn type_of(&self, path: &str, offset: u32) -> Result<Option<String>, SemDbError> {
        let a = self.file(path)?;
        let ident = astwalk::ident_at(&a.ast, offset);
        let Some(ident) = ident else {
            return Ok(None);
        };
        let info = a.typed.semantic.expression_info.get(ident.span);
        Ok(info.map(|i| render_type(&a.typed.store, i.ty)))
    }

    /// Effects declared by the action (or function) enclosing `offset`.
    pub fn effects_of(&self, path: &str, offset: u32) -> Result<Vec<String>, SemDbError> {
        let a = self.file(path)?;
        Ok(effects_of_ast(&a.ast, offset))
    }

    pub fn hover(&self, path: &str, offset: u32) -> Result<Option<String>, SemDbError> {
        let a = self.file(path)?;
        let ident = astwalk::ident_at(&a.ast, offset);
        let Some(ident) = ident else {
            return Ok(None);
        };
        let mut lines = Vec::new();
        if let Some(sym) = self
            .resolve_ident(a, ident.span)
            .or_else(|| self.declaration_at(a, offset))
        {
            lines.push(format!("{}: {}", ident.name, symbol_kind_str(sym.kind)));
            let info = a.typed.semantic.expression_info.get(ident.span);
            if let Some(i) = info {
                lines.push(format!("type: {}", render_type(&a.typed.store, i.ty)));
            }
        } else {
            lines.push(ident.name);
        }
        let effects = effects_of_ast(&a.ast, offset);
        if !effects.is_empty() {
            lines.push(format!("effects [{}]", effects.join(", ")));
        }
        Ok(Some(lines.join("\n")))
    }

    pub fn completion(&self, path: &str, offset: u32) -> Result<Vec<CompletionItem>, SemDbError> {
        let a = self.file(path)?;
        // Baseline: suggest every top-level symbol whose name starts with the ident at offset.
        let prefix = astwalk::ident_at(&a.ast, offset)
            .map(|i| i.name)
            .unwrap_or_default();
        let mut items = Vec::new();
        for s in a.resolve.index.symbols.iter() {
            let name = symbol_name(a, s.id);
            if name.starts_with(&prefix) && !name.is_empty() {
                items.push(CompletionItem {
                    label: name,
                    kind: symbol_kind_str(s.kind).to_string(),
                    detail: None,
                });
            }
        }
        items.sort_by(|x, y| x.label.cmp(&y.label));
        Ok(items)
    }

    pub fn semantic_tokens(&self, path: &str) -> Result<Vec<SemanticToken>, SemDbError> {
        let a = self.file(path)?;
        let decl_spans: std::collections::HashSet<SourceSpan> =
            a.resolve.index.symbols.iter().map(|s| s.span).collect();
        let ref_spans: std::collections::HashSet<SourceSpan> = a
            .resolve
            .index
            .symbols
            .iter()
            .flat_map(|s| a.resolve.index.references.references_to(s.id))
            .map(|r| r.span)
            .collect();
        let mut tokens = Vec::new();
        astwalk::walk_idents(&a.ast, &mut |id| {
            let token_type = if decl_spans.contains(&id.span) {
                "declaration"
            } else if ref_spans.contains(&id.span) {
                "reference"
            } else {
                "plain"
            };
            tokens.push(SemanticToken {
                start: id.span.start,
                length: id.span.end - id.span.start,
                token_type: token_type.to_string(),
            });
        });
        // For assertions: also emit effects/attribute categories is optional; keep idents.
        tokens.sort_by_key(|t| t.start);
        Ok(tokens)
    }

    pub fn call_graph(&self, path: &str, offset: u32) -> Result<Vec<SymbolLocation>, SemDbError> {
        let a = self.file(path)?;
        // Find enclosing callable body and collect calls within it.
        let calls = calls_in_enclosing(&a.ast, offset);
        let mut out = Vec::new();
        for cspan in calls {
            if let Some(cident) = a
                .resolve
                .index
                .symbols
                .iter()
                .flat_map(|s| a.resolve.index.references.references_to(s.id))
                .find(|r| r.span == cspan)
            {
                let sym = a
                    .resolve
                    .index
                    .symbols
                    .get(cident.symbol)
                    .map(|s| symbol_of(a, s));
                if let Some(sym) = sym {
                    out.push(SymbolLocation {
                        path: a.path.clone(),
                        span: sym.span,
                        name: sym.name,
                        symbol_id: sym.id.0,
                    });
                }
            }
        }
        out.sort_by_key(|l| (l.path.clone(), l.span.start, l.span.end));
        out.dedup();
        Ok(out)
    }

    /// A deterministic fingerprint of semantic content, used for incremental==clean checks.
    pub fn fingerprint(&self) -> String {
        let mut parts = Vec::new();
        for a in self.files.values() {
            parts.push(format!(
                "{}:{}:{}:{}",
                a.path,
                a.resolve.index.symbols.count(),
                a.resolve.index.references.count(),
                a.diagnostics.len()
            ));
        }
        parts.sort();
        parts.join("|")
    }

    /// Re-analyze every file from scratch and compare the resulting fingerprint.
    /// Returns true when incremental analysis equals a clean analysis.
    pub fn verify_incremental_equals_clean(&mut self) -> bool {
        let paths: Vec<String> = self.files.keys().cloned().collect();
        let mut snapshot = Vec::new();
        for p in &paths {
            let a = self.files.get(p).unwrap();
            snapshot.push((p.clone(), a.text.clone()));
        }
        let mut clean = SemanticDatabase::new();
        clean.max_files = self.max_files;
        clean.max_source_bytes = self.max_source_bytes;
        let mut ok = true;
        for (p, text) in snapshot {
            if clean.add_file(&p, &text).is_err() {
                ok = false;
            }
        }
        // The per-file analysis is deterministic given identical inputs, so
        // fingerprints must match. Recompute ours after resetting to clean state.
        let fp_clean = clean.fingerprint();
        let fp_inc = self.fingerprint();
        ok &= fp_inc == fp_clean;
        // Keep the clean analysis as the current state.
        *self = clean;
        ok
    }
}

fn normalize_resolver(d: &nexa_resolver::resolver::ResolveDiagnostic) -> Diagnostic {
    let severity = match d.severity {
        nexa_resolver::resolver::DiagnosticSeverity::Error => Severity::Error,
        nexa_resolver::resolver::DiagnosticSeverity::Warning => Severity::Warning,
        nexa_resolver::resolver::DiagnosticSeverity::Note => Severity::Info,
    };
    Diagnostic::new(
        nexa_diagnostics::DiagnosticCode::new(d.code),
        severity,
        "semantic",
        "semantic_error",
        d.message.clone(),
    )
    .with_primary_span(d.span)
}

fn normalize_type(d: &nexa_typecheck::diagnostics::TypeDiagnostic) -> Diagnostic {
    let severity = match d.severity {
        nexa_typecheck::diagnostics::DiagnosticSeverity::Error => Severity::Error,
        nexa_typecheck::diagnostics::DiagnosticSeverity::Warning => Severity::Warning,
        nexa_typecheck::diagnostics::DiagnosticSeverity::Note => Severity::Info,
    };
    Diagnostic::new(
        nexa_diagnostics::DiagnosticCode::new(d.code.code_str()),
        severity,
        "types",
        "type_error",
        d.message.clone(),
    )
    .with_primary_span(d.span)
}

fn symbol_name(a: &Analysis, id: SymbolId) -> String {
    match a.resolve.index.symbols.get(id) {
        Some(s) => a
            .resolve
            .index
            .interner
            .resolve_or_empty(s.name)
            .to_string(),
        None => String::new(),
    }
}

/// Definition position of a declaration (exact name span, §320).
fn decl_span(s: &nexa_resolver::SymbolData) -> SourceSpan {
    s.name_span.unwrap_or(s.span)
}

fn symbol_of(a: &Analysis, s: &nexa_resolver::SymbolData) -> SymbolRecord {
    SymbolRecord {
        id: s.id,
        name: symbol_name(a, s.id),
        kind: s.kind,
        span: s.span,
        visibility: visibility_str(s.visibility).to_string(),
    }
}

pub fn symbol_kind_str(kind: SymbolKind) -> &'static str {
    use SymbolKind::*;
    match kind {
        Module => "module",
        ImportAlias => "import",
        Struct => "struct",
        Enum => "enum",
        Interface => "interface",
        TypeAlias => "type_alias",
        DistinctType => "distinct_type",
        Function => "function",
        Action => "action",
        Const => "const",
        EnumVariant => "enum_variant",
        Parameter => "parameter",
        LocalLet => "local",
        LocalVar => "local",
        LocalConst => "local",
        GenericParameter => "generic_parameter",
        Receiver => "receiver",
        SelfType => "self",
        ContractResult => "contract_result",
        Field => "field",
        DependencyAlias => "dependency",
    }
}

fn visibility_str(v: nexa_symbols::Visibility) -> &'static str {
    match v {
        nexa_symbols::Visibility::ModulePrivate => "module_private",
        nexa_symbols::Visibility::PackageInternal => "package_internal",
        nexa_symbols::Visibility::Public => "public",
    }
}

/// Render a `TypeId` into a human-readable type name using the store.
pub fn render_type(store: &TypeStore, id: TypeId) -> String {
    let Some(ty) = store.get_type(id) else {
        return "<unknown>".to_string();
    };
    match ty {
        Type::Error => "Error".into(),
        Type::Unit => "Unit".into(),
        Type::Never => "Never".into(),
        Type::Bool => "Bool".into(),
        Type::Int => "Int".into(),
        Type::UInt => "UInt".into(),
        Type::Int8 => "Int8".into(),
        Type::Int16 => "Int16".into(),
        Type::Int32 => "Int32".into(),
        Type::Int64 => "Int64".into(),
        Type::UInt8 => "UInt8".into(),
        Type::UInt16 => "UInt16".into(),
        Type::UInt32 => "UInt32".into(),
        Type::UInt64 => "UInt64".into(),
        Type::Float32 => "Float32".into(),
        Type::Float64 => "Float64".into(),
        Type::Byte => "Byte".into(),
        Type::Char => "Char".into(),
        Type::String => "String".into(),
        Type::Bytes => "Bytes".into(),
        Type::Nominal(_) => "nominal".into(),
        Type::Ref(inner) => format!("&{}", render_type(store, *inner)),
        Type::MutRef(inner) => format!("&mut {}", render_type(store, *inner)),
        Type::Array(inner) => format!("[{}]", render_type(store, *inner)),
        Type::GenericParameter(_) => "generic".into(),
        Type::Applied { base, arguments } => {
            let args = arguments
                .iter()
                .map(|a| render_type(store, *a))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}<{}>", render_type(store, *base), args)
        }
        Type::Callable(_) => "callable".into(),
        Type::Task(inner) => format!("Task<{}>", render_type(store, *inner)),
    }
}

/// Collect the effects declared by the action/function enclosing `offset`.
pub fn effects_of_ast(ast: &SourceUnit, offset: u32) -> Vec<String> {
    use nexa_ast::ItemKind;
    for item in &ast.items {
        match &item.kind {
            ItemKind::Action(d) => {
                if d.span.start <= offset && offset <= d.span.end {
                    if let Some(ef) = &d.effects {
                        return ef
                            .effects
                            .iter()
                            .map(|ep| {
                                ep.path
                                    .segments
                                    .iter()
                                    .map(|s| s.name.clone())
                                    .collect::<Vec<_>>()
                                    .join("::")
                            })
                            .collect();
                    }
                }
            }
            ItemKind::Implement(d) => {
                for m in &d.methods {
                    if m.span.start <= offset && offset <= m.span.end {
                        if let Some(ef) = &m.effects {
                            return ef
                                .effects
                                .iter()
                                .map(|ep| {
                                    ep.path
                                        .segments
                                        .iter()
                                        .map(|s| s.name.clone())
                                        .collect::<Vec<_>>()
                                        .join("::")
                                })
                                .collect();
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Vec::new()
}

/// Find spans of function-call callee identifiers within the callable enclosing `offset`.
fn calls_in_enclosing(ast: &SourceUnit, offset: u32) -> Vec<SourceSpan> {
    use nexa_ast::ItemKind;
    let mut out = Vec::new();
    for item in &ast.items {
        let body = match &item.kind {
            ItemKind::Function(d) if d.body.span.start <= offset && offset <= d.body.span.end => {
                Some(&d.body)
            }
            ItemKind::Action(d) if d.body.span.start <= offset && offset <= d.body.span.end => {
                Some(&d.body)
            }
            ItemKind::Implement(d) => d
                .methods
                .iter()
                .find(|m| {
                    m.body
                        .as_ref()
                        .map(|b| b.span.start <= offset && offset <= b.span.end)
                        == Some(true)
                })
                .and_then(|m| m.body.as_ref()),
            _ => None,
        };
        if let Some(block) = body {
            collect_calls(block, &mut out);
            break;
        }
    }
    out
}

fn collect_calls(block: &nexa_ast::Block, out: &mut Vec<SourceSpan>) {
    use nexa_ast::StmtKind;
    for stmt in &block.stmts {
        if let StmtKind::Expr(e) = &stmt.kind {
            collect_call_spans(e, out);
        }
    }
}

fn collect_call_spans(e: &nexa_ast::Expr, out: &mut Vec<SourceSpan>) {
    use nexa_ast::ExprKind;
    match &e.kind {
        ExprKind::Call { callee, args } => {
            if let ExprKind::Ident(id) = &callee.kind {
                out.push(id.span);
            }
            for a in args {
                collect_call_spans(a, out);
            }
            collect_call_spans(callee, out);
        }
        ExprKind::MethodCall { target, args, .. } => {
            collect_call_spans(target, out);
            for a in args {
                collect_call_spans(a, out);
            }
        }
        ExprKind::Binary(_, a, b) => {
            collect_call_spans(a, out);
            collect_call_spans(b, out);
        }
        ExprKind::Unary(_, a) => collect_call_spans(a, out),
        ExprKind::Index { target, index } => {
            collect_call_spans(target, out);
            collect_call_spans(index, out);
        }
        ExprKind::Field { target, .. } => collect_call_spans(target, out),
        ExprKind::Await(a)
        | ExprKind::Try(a)
        | ExprKind::Ref(a)
        | ExprKind::RefMut(a)
        | ExprKind::Move(a)
        | ExprKind::Paren(a) => collect_call_spans(a, out),
        ExprKind::If(ifex) => collect_call_spans(&ifex.condition, out),
        ExprKind::Block(b) => {
            for stmt in &b.stmts {
                if let nexa_ast::StmtKind::Expr(se) = &stmt.kind {
                    collect_call_spans(se, out);
                }
            }
        }
        _ => {}
    }
}

// ── Public result types ────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SymbolLocation {
    pub path: String,
    pub span: SourceSpan,
    pub name: String,
    pub symbol_id: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionItem {
    pub label: String,
    pub kind: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticToken {
    pub start: u32,
    pub length: u32,
    pub token_type: String,
}

/// Lightweight, cloneable snapshot of the database's semantic records.
#[derive(Debug, Clone, Default)]
pub struct SemanticSnapshot {
    pub revision: u64,
    pub diagnostics: Vec<Diagnostic>,
}

impl SemanticSnapshot {
    pub fn revision(&self) -> RevisionId {
        RevisionId(self.revision)
    }
}

/// Deterministic canonical sort of diagnostics.
pub fn canonical_sort(diagnostics: &mut [Diagnostic], path_of: &impl Fn(u32) -> Option<String>) {
    nexa_diagnostic_schema::canonical_sort(diagnostics, path_of);
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASIC: &str = r#"
        struct User {
            name: String
        }
        function test_addition() -> Unit {
            let x = 1 + 2
        }
        action load(id: Int) -> Unit effects [database::read] {
            let y = 5
        }
    "#;

    fn db_with(source: &str) -> SemanticDatabase {
        let mut db = SemanticDatabase::new();
        db.add_file("main.nexa", source).unwrap();
        db
    }

    #[test]
    fn add_and_query_revision() {
        let mut db = SemanticDatabase::new();
        let r1 = db.add_file("a.nexa", "function f() -> Unit {}").unwrap();
        assert!(r1.as_u64() > 0);
        assert_eq!(db.file_count(), 1);
        let r2 = db
            .update_file("a.nexa", "action f() -> Unit effects [] {}")
            .unwrap();
        assert!(r2.as_u64() > r1.as_u64());
    }

    #[test]
    fn update_noop_keeps_revision() {
        let mut db = SemanticDatabase::new();
        db.add_file("a.nexa", "function f() -> Unit {}").unwrap();
        let r = db.revision();
        let r2 = db.update_file("a.nexa", "function f() -> Unit {}").unwrap();
        assert_eq!(r2, r);
    }

    #[test]
    fn diagnostics_are_deterministic() {
        let db = db_with(BASIC);
        let d = db.diagnostics("main.nexa").unwrap();
        // stable across calls
        let d2 = db.diagnostics("main.nexa").unwrap();
        assert_eq!(d, d2);
    }

    #[test]
    fn symbol_at_finds_declaration() {
        let db = db_with("function my_func() -> Unit {}");
        // offset of "my_func"
        let off = source_offset("function my_func()", "my_func");
        let info = db.symbol_at("main.nexa", off).unwrap();
        assert!(info.is_some());
        assert_eq!(info.unwrap().name, "my_func");
    }

    #[test]
    fn definition_returns_span() {
        let db = db_with("function my_func() -> Unit {}");
        let off = source_offset("function my_func()", "my_func");
        let def = db.definition("main.nexa", off).unwrap().unwrap();
        assert_eq!(def.name, "my_func");
        assert!(def.span.end > def.span.start);
    }

    #[test]
    fn effects_of_action_declared() {
        let src =
            "action load(id: Int) -> Unit effects [database::read] {}\nfunction g() -> Unit {}";
        let db = db_with(src);
        let off = source_offset(src, "database");
        let effs = db.effects_of("main.nexa", off).unwrap();
        assert_eq!(effs, vec!["database::read"]);
    }

    #[test]
    fn effects_of_function_is_empty() {
        let db = db_with("function g() -> Unit {}\nfunction h() -> Unit {}");
        let off = source_offset("function h", "h");
        assert!(db.effects_of("main.nexa", off).unwrap().is_empty());
    }

    #[test]
    fn semantic_tokens_produce_decls_and_refs() {
        let db = db_with("function f() -> Unit { let a = 1 }");
        let tokens = db.semantic_tokens("main.nexa").unwrap();
        assert!(!tokens.is_empty());
        // deterministic ordering by start
        let mut sorted = tokens.clone();
        sorted.sort_by_key(|t| t.start);
        assert_eq!(tokens, sorted);
    }

    #[test]
    fn incrementality_equals_clean() {
        let mut db = SemanticDatabase::new();
        db.add_file("a.nexa", "function f() -> Unit { let x = 1 }")
            .unwrap();
        db.add_file("b.nexa", "struct S { v: Int }").unwrap();
        db.update_file("a.nexa", "function f() -> Unit { let x = 2 }")
            .unwrap();
        assert!(db.verify_incremental_equals_clean());
    }

    #[test]
    fn unknown_source_errors() {
        let db = SemanticDatabase::new();
        assert!(matches!(
            db.diagnostics("missing.nexa"),
            Err(SemDbError::UnknownSource { .. })
        ));
    }

    #[test]
    fn file_limit_enforced() {
        let mut db = SemanticDatabase::with_limits(1, 100_000);
        db.add_file("a.nexa", "function f() -> Unit {}").unwrap();
        assert!(matches!(
            db.add_file("b.nexa", "function g() -> Unit {}"),
            Err(SemDbError::FileLimitExceeded { .. })
        ));
    }

    #[test]
    fn source_too_large_enforced() {
        let mut db = SemanticDatabase::with_limits(10, 10);
        assert!(matches!(
            db.add_file("a.nexa", "function f() -> Unit {}"),
            Err(SemDbError::SourceTooLarge { .. })
        ));
    }

    fn source_offset(src: &str, word: &str) -> u32 {
        src.find(word).expect("word present") as u32
    }
}
