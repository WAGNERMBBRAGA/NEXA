//! # nexa-compiler-service
//!
//! Implementation 11 — `CompilerService`.
//!
//! A thin, synchronous service that wraps [`nexa_semantic_db::SemanticDatabase`]
//! and exposes the NEXA Tooling Protocol surface (`CompilerServiceApi`) plus a
//! reusable envelope dispatcher (`handle_query`/`handle_envelope`).
//!
//! The semantic database itself is incremental; this crate never re-compiles
//! the whole project on every keystroke. Instead each file is analyzed on
//! `update_file`, and every query is answered directly from the stored
//! per-file `Analysis`.
//!
//! Error codes: `NEXA-COMPILER-0001` … `NEXA-COMPILER-0006`.

use std::collections::HashMap;

use nexa_semantic_db as db;
use nexa_tooling_protocol as proto;

use db::SemDbError;
use db::SemanticDatabase;
use nexa_diagnostic_schema::MachineDiagnostic;
use nexa_diagnostic_schema::SchemaSpan;
use proto::CompletionItem;
use proto::Method;
use proto::Params;
use proto::RequestEnvelope;
use proto::ResponseEnvelope;
use proto::ResultPayload;
use proto::SymbolId;
use proto::SymbolLocation;
use proto::TextEdit;
use proto::ToolingProtocolError;

/// The Compiler Service owns a single incremental semantic database.
pub struct CompilerService {
    /// Incremental semantic database holding every analyzed source file.
    pub database: SemanticDatabase,
    /// Root of the currently open project, if any.
    root: Option<String>,
    /// Lazy `symbol_id -> (path, byte offset)` registry, populated from query
    /// results that already carry the resolved id (definition/references/
    /// call_graph). Used to route the `SymbolId`-only protocol methods.
    symbols: HashMap<u32, (String, u32)>,
}

/// Error surfaced by the Compiler Service and its protocol dispatcher.
#[derive(Debug, thiserror::Error)]
pub enum CompilerServiceError {
    #[error("NEXA-COMPILER-0001: No project is open; call open_project first")]
    NoProjectOpen,
    #[error("NEXA-COMPILER-0002: Semantic database error: {0}")]
    Database(#[from] SemDbError),
    #[error("NEXA-COMPILER-0003: Protocol error: {0}")]
    Protocol(#[from] ToolingProtocolError),
    #[error("NEXA-COMPILER-0004: Request refers to a stale source revision")]
    StaleRevision,
    #[error("NEXA-COMPILER-0005: Unknown symbol id {0}")]
    UnknownSymbol(u32),
    #[error("NEXA-COMPILER-0006: {0}")]
    Other(String),
}

/// Bridge that converts protocol errors into Compiler Service errors.
///
/// This mirrors the shapes of `proto::ToolingProtocolError::StaleRevision` and
/// `UnknownSource` so callers can pattern-match a single error type.
#[derive(Debug, thiserror::Error)]
pub enum SnapshotToProtocolError {
    #[error("request refers to a stale source revision")]
    StaleRevision,
    #[error("unknown source path '{path}'")]
    UnknownSource { path: String },
    #[error("semantic database error: {0}")]
    Database(#[from] SemDbError),
    #[error("no project is open")]
    NoProjectOpen,
}

impl From<SnapshotToProtocolError> for CompilerServiceError {
    fn from(value: SnapshotToProtocolError) -> Self {
        match value {
            SnapshotToProtocolError::StaleRevision => CompilerServiceError::StaleRevision,
            SnapshotToProtocolError::UnknownSource { path } => {
                CompilerServiceError::Other(format!("unknown source path '{path}'"))
            }
            SnapshotToProtocolError::Database(e) => CompilerServiceError::Database(e),
            SnapshotToProtocolError::NoProjectOpen => CompilerServiceError::NoProjectOpen,
        }
    }
}

impl Default for CompilerService {
    fn default() -> Self {
        Self::new()
    }
}

impl CompilerService {
    /// Create an empty service with a fresh incremental database.
    pub fn new() -> Self {
        CompilerService {
            database: SemanticDatabase::new(),
            root: None,
            symbols: HashMap::new(),
        }
    }

    pub fn root(&self) -> Option<&str> {
        self.root.as_deref()
    }

    /// Record resolved symbol locations so id-only protocol methods can route.
    /// We remember an interior byte offset (spans matched inclusively at the
    /// left edge) so the offset-based semantic queries can find it again.
    fn remember(&mut self, locs: &[db::SymbolLocation]) {
        for l in locs {
            let off = if l.span.end > l.span.start {
                l.span.start + (l.span.end - l.span.start) / 2
            } else {
                l.span.start
            };
            self.symbols.insert(l.symbol_id, (l.path.clone(), off));
        }
    }

    fn proto_loc(l: &db::SymbolLocation) -> SymbolLocation {
        SymbolLocation {
            symbol: SymbolId(l.symbol_id),
            name: l.name.clone(),
            path: l.path.clone(),
            span: SchemaSpan {
                source: l.span.source.0,
                start: l.span.start,
                end: l.span.end,
            },
        }
    }

    fn proto_completion(c: &db::CompletionItem) -> CompletionItem {
        CompletionItem {
            label: c.label.clone(),
            detail: c.detail.clone(),
            kind: Some(c.kind.clone()),
        }
    }

    fn proto_token(t: &db::SemanticToken) -> proto::SemanticToken {
        proto::SemanticToken {
            start: t.start,
            length: t.length,
            token_type: t.token_type.clone(),
        }
    }

    fn proto_diagnostics(diags: &[nexa_diagnostics::Diagnostic]) -> Vec<MachineDiagnostic> {
        diags
            .iter()
            .map(MachineDiagnostic::from_diagnostic)
            .collect()
    }
}

/// The Compiler Service API surface (NEXA Tooling Protocol semantics).
///
/// Every method returns a protocol [`ResultPayload`] so that the LSP/CLI
/// adapters can forward results unchanged.
pub trait CompilerServiceApi {
    fn open_project(&mut self, root: &str) -> Result<ResultPayload, CompilerServiceError>;
    fn update_file(
        &mut self,
        path: &str,
        revision: Option<u64>,
        text: &str,
    ) -> Result<ResultPayload, CompilerServiceError>;
    fn remove_file(&mut self, path: &str) -> Result<ResultPayload, CompilerServiceError>;
    fn diagnostics(&self) -> Result<ResultPayload, CompilerServiceError>;
    fn symbol_at(&mut self, path: &str, offset: u32)
        -> Result<ResultPayload, CompilerServiceError>;
    fn definition(
        &mut self,
        path: &str,
        offset: u32,
    ) -> Result<ResultPayload, CompilerServiceError>;
    fn references(
        &mut self,
        path: &str,
        offset: u32,
    ) -> Result<ResultPayload, CompilerServiceError>;
    fn hover(&self, path: &str, offset: u32) -> Result<ResultPayload, CompilerServiceError>;
    fn completion(&self, path: &str, offset: u32) -> Result<ResultPayload, CompilerServiceError>;
    fn rename(
        &self,
        path: &str,
        offset: u32,
        new_name: &str,
    ) -> Result<ResultPayload, CompilerServiceError>;
    fn type_of(&self, path: &str, offset: u32) -> Result<ResultPayload, CompilerServiceError>;
    fn effects_of(&self, path: &str, offset: u32) -> Result<ResultPayload, CompilerServiceError>;
    fn call_graph(
        &mut self,
        path: &str,
        offset: u32,
    ) -> Result<ResultPayload, CompilerServiceError>;
}

impl CompilerServiceApi for CompilerService {
    fn open_project(&mut self, root: &str) -> Result<ResultPayload, CompilerServiceError> {
        self.root = Some(root.to_string());
        self.symbols.clear();
        Ok(ResultPayload::ProjectOpen {
            revision: self.database.revision().as_u64(),
        })
    }

    fn update_file(
        &mut self,
        path: &str,
        revision: Option<u64>,
        text: &str,
    ) -> Result<ResultPayload, CompilerServiceError> {
        if let Some(expect) = revision {
            if let Some(current) = self.database.file_revision(path) {
                if current != expect {
                    return Err(CompilerServiceError::StaleRevision);
                }
            }
        }
        let rev = self.database.update_file(path, text)?;
        Ok(ResultPayload::SourceUpdate {
            revision: rev.as_u64(),
        })
    }

    fn remove_file(&mut self, path: &str) -> Result<ResultPayload, CompilerServiceError> {
        let rev = self.database.remove_file(path)?;
        Ok(ResultPayload::SourceRemove {
            revision: rev.as_u64(),
        })
    }

    fn diagnostics(&self) -> Result<ResultPayload, CompilerServiceError> {
        Ok(ResultPayload::DiagnosticsGet {
            diagnostics: Self::proto_diagnostics(&self.database.all_diagnostics()),
        })
    }

    fn symbol_at(
        &mut self,
        path: &str,
        offset: u32,
    ) -> Result<ResultPayload, CompilerServiceError> {
        let locs = match self.database.definition(path, offset)? {
            Some(l) => {
                self.remember(std::slice::from_ref(&l));
                vec![l]
            }
            None => Vec::new(),
        };
        Ok(ResultPayload::SymbolAt {
            symbol: locs.iter().map(Self::proto_loc).next(),
        })
    }

    fn definition(
        &mut self,
        path: &str,
        offset: u32,
    ) -> Result<ResultPayload, CompilerServiceError> {
        let loc = match self.database.definition(path, offset)? {
            Some(l) => {
                self.remember(std::slice::from_ref(&l));
                Some(Self::proto_loc(&l))
            }
            None => None,
        };
        Ok(ResultPayload::SymbolDefinition { location: loc })
    }

    fn references(
        &mut self,
        path: &str,
        offset: u32,
    ) -> Result<ResultPayload, CompilerServiceError> {
        let locs = self.database.references(path, offset)?;
        self.remember(&locs);
        Ok(ResultPayload::SymbolReferences {
            references: locs.iter().map(Self::proto_loc).collect(),
        })
    }

    fn hover(&self, path: &str, offset: u32) -> Result<ResultPayload, CompilerServiceError> {
        Ok(ResultPayload::Hover {
            content: self.database.hover(path, offset)?,
        })
    }

    fn completion(&self, path: &str, offset: u32) -> Result<ResultPayload, CompilerServiceError> {
        let items = self.database.completion(path, offset)?;
        Ok(ResultPayload::Completion {
            items: items.iter().map(Self::proto_completion).collect(),
        })
    }

    fn rename(
        &self,
        path: &str,
        offset: u32,
        new_name: &str,
    ) -> Result<ResultPayload, CompilerServiceError> {
        let edits = self
            .database
            .rename(path, offset, new_name)?
            .into_iter()
            .map(|e| TextEdit {
                span: SchemaSpan {
                    source: 0,
                    start: e.start,
                    end: e.end,
                },
                new_text: e.new_text,
            })
            .collect();
        Ok(ResultPayload::SymbolRename { edits })
    }

    fn type_of(&self, path: &str, offset: u32) -> Result<ResultPayload, CompilerServiceError> {
        Ok(ResultPayload::TypeOf {
            ty: self.database.type_of(path, offset)?,
        })
    }

    fn effects_of(&self, path: &str, offset: u32) -> Result<ResultPayload, CompilerServiceError> {
        let effects = self.database.effects_of(path, offset)?;
        Ok(ResultPayload::EffectsOf { effects })
    }

    fn call_graph(
        &mut self,
        path: &str,
        offset: u32,
    ) -> Result<ResultPayload, CompilerServiceError> {
        let locs = self.database.call_graph(path, offset)?;
        self.remember(&locs);
        Ok(ResultPayload::CallGraph {
            calls: locs.iter().map(Self::proto_loc).collect(),
        })
    }
}

impl CompilerService {
    /// Route a single protocol query and produce the corresponding result.
    pub fn handle_query(
        &mut self,
        method: Method,
        params: &Params,
    ) -> Result<ResultPayload, CompilerServiceError> {
        match (method, params) {
            (Method::ProjectOpen, Params::ProjectOpen { root }) => self.open_project(root),
            (
                Method::SourceUpdate,
                Params::SourceUpdate {
                    path,
                    revision,
                    text,
                },
            ) => self.update_file(path, Some(*revision), text),
            (Method::SourceRemove, Params::SourceRemove { path }) => self.remove_file(path),
            (Method::DiagnosticsGet, Params::DiagnosticsGet { .. }) => self.diagnostics(),
            (Method::SymbolAt, Params::SymbolAt { path, offset }) => self.symbol_at(path, *offset),
            (Method::Hover, Params::Hover { path, offset }) => self.hover(path, *offset),
            (Method::Completion, Params::Completion { path, offset }) => {
                self.completion(path, *offset)
            }
            (Method::SemanticTokens, Params::SemanticTokens { path }) => {
                let tokens = self.database.semantic_tokens(path)?;
                Ok(ResultPayload::SemanticTokens {
                    tokens: tokens.iter().map(Self::proto_token).collect(),
                })
            }
            (Method::TypeOf, Params::TypeOf { path, offset }) => self.type_of(path, *offset),
            (Method::EffectOf, Params::EffectOf { path, offset }) => {
                let effects = self.database.effects_of(path, *offset)?;
                Ok(ResultPayload::EffectOf {
                    effect: effects.first().cloned(),
                })
            }
            (Method::EffectsOf, Params::EffectsOf { path, offset }) => {
                self.effects_of(path, *offset)
            }
            (
                Method::SymbolRename,
                Params::SymbolRename {
                    source_path,
                    offset,
                    new_name,
                },
            ) => self.rename(source_path, *offset, new_name),
            (Method::SymbolDefinition, Params::SymbolDefinition { symbol }) => {
                let (path, offset) = self.locate(symbol.0)?;
                self.definition(&path, offset)
            }
            (Method::SymbolReferences, Params::SymbolReferences { symbol }) => {
                let (path, offset) = self.locate(symbol.0)?;
                self.references(&path, offset)
            }
            (Method::CallGraph, Params::CallGraph { symbol }) => {
                let (path, offset) = self.locate(symbol.0)?;
                self.call_graph(&path, offset)
            }
            (Method::CancelRequest, Params::CancelRequest { .. }) => Ok(ResultPayload::Cancelled),
            _ => Err(CompilerServiceError::Other(format!(
                "method/params mismatch for '{}'",
                method.as_str()
            ))),
        }
    }

    /// Locate the (path, byte offset) of a previously-recorded symbol id.
    fn locate(&self, symbol: u32) -> Result<(String, u32), CompilerServiceError> {
        self.symbols
            .get(&symbol)
            .cloned()
            .ok_or(CompilerServiceError::UnknownSymbol(symbol))
    }

    /// Dispatch a whole request envelope to a response envelope.
    pub fn handle_envelope(&mut self, request: &RequestEnvelope) -> ResponseEnvelope {
        let result = self.handle_query(request.method, &request.params);
        match result {
            Ok(result) => ResponseEnvelope::success(request.request_id, result),
            Err(err) => ResponseEnvelope {
                protocol_version: proto::PROTOCOL_VERSION,
                request_id: request.request_id,
                result: ResultPayload::DiagnosticsGet {
                    diagnostics: Vec::new(),
                },
                diagnostics: vec![MachineDiagnostic {
                    code: "compiler.service".to_string(),
                    severity: "error".to_string(),
                    category: Some("compiler".to_string()),
                    message_key: "compiler.error".to_string(),
                    message: err.to_string(),
                    primary_span: None,
                    related: Vec::new(),
                    arguments: Vec::new(),
                    help: None,
                    fixes: Vec::new(),
                }],
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proto::RequestId;

    fn basic_service() -> CompilerService {
        let mut s = CompilerService::new();
        s.open_project("mem://proj").unwrap();
        s
    }

    #[test]
    fn open_project_sets_root() {
        let mut s = CompilerService::new();
        let payload = s.open_project("/tmp/nexa").unwrap();
        assert!(matches!(
            payload,
            ResultPayload::ProjectOpen { revision: 0 }
        ));
        assert_eq!(s.root(), Some("/tmp/nexa"));
    }

    #[test]
    fn update_file_then_diagnostics() {
        let mut s = basic_service();
        let payload = s
            .update_file(
                "main.nexa",
                None,
                "function add(a: Int, b: Int) -> Int { return a + b }",
            )
            .unwrap();
        let rev = match payload {
            ResultPayload::SourceUpdate { revision } => revision,
            _ => panic!("expected SourceUpdate"),
        };
        assert!(rev > 0);

        let payload = s.diagnostics().unwrap();
        match payload {
            ResultPayload::DiagnosticsGet { diagnostics } => {
                let _ = diagnostics.len();
            }
            _ => panic!("expected DiagnosticsGet"),
        }
    }

    #[test]
    fn definition_and_references_route() {
        let mut s = basic_service();
        s.update_file("main.nexa", None, "function my_func() -> Unit {}")
            .unwrap();
        let payload = s.definition("main.nexa", 11).unwrap();
        let loc = match payload {
            ResultPayload::SymbolDefinition { location } => location,
            _ => panic!("expected SymbolDefinition"),
        };
        assert!(loc.is_some());
        let loc = loc.unwrap();
        assert_eq!(loc.name, "my_func");
        assert!(loc.span.end > loc.span.start);

        // The id seen in the definition is recorded; route via id now.
        let payload = s
            .handle_query(
                Method::SymbolDefinition,
                &Params::SymbolDefinition {
                    symbol: SymbolId(loc.symbol.0),
                },
            )
            .unwrap();
        assert!(matches!(
            payload,
            ResultPayload::SymbolDefinition { location: Some(_) }
        ));
    }

    #[test]
    fn unknown_symbol_is_error() {
        let mut s = basic_service();
        let err = s
            .handle_query(
                Method::SymbolDefinition,
                &Params::SymbolDefinition {
                    symbol: SymbolId(4242),
                },
            )
            .unwrap_err();
        assert!(matches!(err, CompilerServiceError::UnknownSymbol(4242)));
    }

    #[test]
    fn hover_type_and_effects() {
        let mut s = basic_service();
        s.update_file(
            "main.nexa",
            None,
            "action load(id: Int) -> Unit effects [db::read] {}",
        )
        .unwrap();
        let payload = s.hover("main.nexa", 9).unwrap();
        assert!(matches!(payload, ResultPayload::Hover { content: Some(_) }));

        // Offset of "db" in the effects clause.
        let off = "action load(id: Int) -> Unit effects [db::read] {}"
            .find("db")
            .unwrap() as u32;
        let payload = s.effects_of("main.nexa", off).unwrap();
        match payload {
            ResultPayload::EffectsOf { effects } => assert_eq!(effects, vec!["db::read"]),
            _ => panic!("expected EffectsOf"),
        }
    }

    #[test]
    fn rename_produces_edits() {
        let mut s = basic_service();
        s.update_file("main.nexa", None, "function my_func() -> Unit {}")
            .unwrap();
        let payload = s.rename("main.nexa", 11, "renamed").unwrap();
        match payload {
            ResultPayload::SymbolRename { edits } => assert!(!edits.is_empty()),
            _ => panic!("expected SymbolRename"),
        }
    }

    #[test]
    fn stale_revision_rejected() {
        let mut s = basic_service();
        s.update_file("main.nexa", None, "function f() -> Unit {}")
            .unwrap();
        let err = s
            .update_file("main.nexa", Some(0), "function g() -> Unit {}")
            .unwrap_err();
        assert!(matches!(err, CompilerServiceError::StaleRevision));
    }

    #[test]
    fn envelope_dispatch_and_error_payload() {
        let mut s = basic_service();
        let req = RequestEnvelope::new(
            RequestId(1),
            Method::Hover,
            Params::Hover {
                path: "missing.nexa".to_string(),
                offset: 0,
            },
        );
        let resp = s.handle_envelope(&req);
        // Missing file -> error travels in the envelope diagnostics.
        assert_eq!(resp.protocol_version, proto::PROTOCOL_VERSION);
        assert!(
            resp.result
                == ResultPayload::DiagnosticsGet {
                    diagnostics: Vec::new()
                }
        );
        assert!(!resp.diagnostics.is_empty());
    }
}
