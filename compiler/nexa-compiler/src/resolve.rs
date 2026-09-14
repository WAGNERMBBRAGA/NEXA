//! Resolve pipeline (Implementação 03, §455-465).
//!
//! Extensão do `Pipeline` até o fim da resolução semântica:
//!
//! ```text
//! bytes → UTF-8 validation → SourceFile → Lexer → Parser (SingleFile)
//!      → Resolver (single-module mode, §459) → SemanticIndex + diagnostics
//!      → output (human | json, debug tooling §463-465)
//! ```
//!
//! O JSON é output **de debug** (§464), não formato normativo persistente
//! (§465). O modo multi-module usa o harness de biblioteca (§460-462).

use crate::{Diagnostic, Pipeline, SCHEMA_VERSION};
use nexa_parser::{parse, ParseMode};
use nexa_resolver::resolver::{DiagnosticSeverity, ResolveDiagnostic};
use nexa_resolver::{resolve, SemanticIndex};
use nexa_source::{SourceId, SourceLoadError, SourceManager, SourceSpan};
use nexa_symbols::SymbolKind;
use serde::Serialize;
use std::path::Path;

/// Resultado da fronteira `resolve`: índice semântico + diagnostics (parse e
/// semânticos) já mesclados, com o pipeline de fontes para renderização.
pub struct SemanticResult {
    pub index: SemanticIndex,
    pub source_id: SourceId,
    /// Diagnostics do parser (validação de sintaxe).
    pub parse_diagnostics: Vec<Diagnostic>,
    /// Diagnostics do resolver (scopes, nomes, visibilidade, imports).
    pub semantic_diagnostics: Vec<ResolveDiagnostic>,
}

impl SemanticResult {
    pub fn has_errors(&self) -> bool {
        self.parse_diagnostics
            .iter()
            .any(|d| d.severity == nexa_diagnostics::Severity::Error)
            || self
                .semantic_diagnostics
                .iter()
                .any(|d| d.severity == DiagnosticSeverity::Error)
    }
}

impl Pipeline {
    /// Carrega um texto nomeado, parseia (SingleFile) e resolve o módulo único.
    pub fn resolve_source(&mut self, name: &str, content: &str) -> SemanticResult {
        let id = self.sources.load_text(name.into(), content.to_owned());
        let source = self.sources.source(id).expect("just loaded source");
        let parse_result = parse(source, ParseMode::SingleFile);
        let resolve_result = resolve(id, &parse_result.ast);
        SemanticResult {
            index: resolve_result.index,
            source_id: id,
            parse_diagnostics: parse_result.diagnostics,
            semantic_diagnostics: resolve_result.diagnostics,
        }
    }

    /// Fronteira real de resolve a partir de **bytes** (análogo a
    /// `parse_bytes`): valida UTF-8 e, em bytes inválidos, retorna o
    /// diagnóstico de loading na fronteira (NEXA-LEX-0001).
    #[allow(clippy::result_large_err)]
    pub fn resolve_bytes(
        &mut self,
        display_path: &Path,
        bytes: Vec<u8>,
    ) -> Result<SemanticResult, Diagnostic> {
        match self.sources.load_bytes(display_path.to_owned(), bytes) {
            Ok(id) => {
                let source = self.sources.source(id).expect("just loaded source");
                let parse_result = parse(source, ParseMode::SingleFile);
                let resolve_result = resolve(id, &parse_result.ast);
                Ok(SemanticResult {
                    index: resolve_result.index,
                    source_id: id,
                    parse_diagnostics: parse_result.diagnostics,
                    semantic_diagnostics: resolve_result.diagnostics,
                })
            }
            Err(SourceLoadError::InvalidUtf8 { byte_offset, .. }) => Err(Diagnostic::error(
                crate::LEX_INVALID_UTF8,
                "lexer",
                "lexer.invalid_utf8",
                "source is not valid UTF-8",
            )
            .with_argument("byteOffset", byte_offset as u64)),
            Err(SourceLoadError::Io { .. }) => {
                unreachable!("load_bytes nunca emite SourceLoadError::Io")
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Output de debug (JSON, §464).
// ---------------------------------------------------------------------------

/// Envelope `nexa resolve --format json` (debug tooling, §464-465).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveOutputEnvelope {
    pub schema_version: u32,
    pub source: Option<String>,
    pub parse_diagnostics: Vec<serde_json::Value>,
    pub semantic_diagnostics: Vec<serde_json::Value>,
    pub modules: Vec<serde_json::Value>,
    pub symbols: Vec<serde_json::Value>,
    pub references: Vec<serde_json::Value>,
}

/// Rótulo estável de um `SymbolKind` (debug/CLI).
pub fn symbol_kind_label(kind: SymbolKind) -> &'static str {
    use SymbolKind::*;
    match kind {
        Module => "Module",
        ImportAlias => "ImportAlias",
        DependencyAlias => "DependencyAlias",
        Struct => "Struct",
        Enum => "Enum",
        Interface => "Interface",
        TypeAlias => "TypeAlias",
        DistinctType => "DistinctType",
        Function => "Function",
        Action => "Action",
        Const => "Const",
        EnumVariant => "EnumVariant",
        Parameter => "Parameter",
        LocalLet => "LocalLet",
        LocalVar => "LocalVar",
        LocalConst => "LocalConst",
        GenericParameter => "GenericParameter",
        Receiver => "Receiver",
        SelfType => "SelfType",
        ContractResult => "ContractResult",
        Field => "Field",
    }
}

fn reference_kind_label(kind: nexa_resolver::reference_index::ReferenceKind) -> &'static str {
    use nexa_resolver::reference_index::ReferenceKind;
    match kind {
        ReferenceKind::Read => "Read",
        ReferenceKind::Write => "Write",
        ReferenceKind::Type => "Type",
        ReferenceKind::Call => "Call",
        ReferenceKind::Import => "Import",
    }
}

fn diagnostic_value(d: &ResolveDiagnostic) -> serde_json::Value {
    serde_json::json!({
        "code": d.code,
        "severity": match d.severity {
            DiagnosticSeverity::Error => "error",
            DiagnosticSeverity::Warning => "warning",
            DiagnosticSeverity::Note => "note",
        },
        "message": d.message,
        "span": {
            "source": d.span.source.0,
            "start": d.span.start,
            "end": d.span.end,
        },
        "context": d.context,
    })
}

/// Constrói o envelope JSON do output do resolver (debug, §464).
pub fn resolve_output_json(
    result: &SemanticResult,
    sources: &SourceManager,
) -> ResolveOutputEnvelope {
    let index = &result.index;
    let source = sources.source(result.source_id).map(|s| s.display_name());

    let mut modules = Vec::new();
    for mid in 0..index.modules.count() {
        if let Some(m) = index.modules.get(nexa_symbols::ModuleId(mid as u32)) {
            let sym = m.symbol.and_then(|s| index.symbols.get(s));
            modules.push(serde_json::json!({
                "id": mid,
                "name": m.name,
                "package": m.package.0,
                "path": m.path.display(),
                "public": m.is_public,
                "source": m.source_id.map(|s| s.0),
                "span": { "start": m.span.start, "end": m.span.end },
                "symbol": m.symbol.map(|s| s.0),
                "rootScope": m.root_scope.0,
                "symbolName": sym.map(|s| index.interner.resolve(s.name)),
            }));
        }
    }

    let mut symbols = Vec::new();
    for s in index.symbols.iter() {
        symbols.push(serde_json::json!({
            "id": s.id.0,
            "name": index.interner.resolve(s.name),
            "kind": symbol_kind_label(s.kind),
            "module": s.module.0,
            "scope": s.scope.0,
            "visibility": s.visibility.as_str(),
            "owner": s.owner.map(|o| o.0),
            "nameSpan": s.name_span.map(|sp| [sp.start, sp.end]),
            "bodyScope": s.body_scope.map(|b| b.0),
        }));
    }

    let mut references = Vec::new();
    for r in index.references.all() {
        let target = index
            .symbols
            .get(r.symbol)
            .map(|s| index.interner.resolve(s.name).to_string());
        references.push(serde_json::json!({
            "span": { "source": r.span.source.0, "start": r.span.start, "end": r.span.end },
            "symbol": r.symbol.0,
            "symbolName": target,
            "kind": reference_kind_label(r.kind),
            "scope": r.scope.0,
        }));
    }

    ResolveOutputEnvelope {
        schema_version: SCHEMA_VERSION,
        source,
        parse_diagnostics: result
            .parse_diagnostics
            .iter()
            .map(serde_json::to_value)
            .collect::<Result<_, _>>()
            .unwrap_or_default(),
        semantic_diagnostics: result
            .semantic_diagnostics
            .iter()
            .map(diagnostic_value)
            .collect(),
        modules,
        symbols,
        references,
    }
}

/// Helper de renderização: localização `display:line:col` (1-based) de um span
/// (compatível com a amostra de debug §463).
pub fn location_str(sources: &SourceManager, span: SourceSpan) -> Option<String> {
    let file = sources.source(span.source)?;
    let loc = file.location(span.start);
    Some(format!(
        "{}:{}:{}",
        file.display_name(),
        loc.line + 1,
        loc.column + 1
    ))
}
