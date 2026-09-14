use nexa_source::SourceSpan;
use serde::Serialize;

use nexa_diagnostic_schema::{MachineDiagnostic, SchemaSpan};

pub const PROTOCOL_NAME: &str = "NEXA Tooling Protocol";
pub const PROTOCOL_NAME_SHORT: &str = "Tooling Protocol";
pub const PROTOCOL_VERSION: u32 = 1;

/// Transport-agnostic, semantic identity for a symbol inside a project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct SymbolId(pub u32);

impl SymbolId {
    pub const INVALID: SymbolId = SymbolId(u32::MAX);

    pub fn new(id: u32) -> Self {
        SymbolId(id)
    }
}

/// Opaque request identifier used for correlation and cancellation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct RequestId(pub u64);

/// Line/column position in UTF-16 code units (LSP-facing).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Utf16Position {
    pub line: u32,
    pub character: u32,
}

/// A single replacement of source region [start, end) with new text.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextEdit {
    pub span: SchemaSpan,
    pub new_text: String,
}

impl TextEdit {
    pub fn new(span: SourceSpan, new_text: impl Into<String>) -> Self {
        TextEdit {
            span: SchemaSpan {
                source: span.source.0,
                start: span.start,
                end: span.end,
            },
            new_text: new_text.into(),
        }
    }
}

/// A source file identity: path plus a monotonically increasing revision.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FileIdentity {
    pub path: String,
    pub revision: u64,
}

/// Semantic query methods exposed by the NEXA Tooling Protocol 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Method {
    ProjectOpen,
    SourceUpdate,
    SourceRemove,
    DiagnosticsGet,
    SymbolAt,
    SymbolDefinition,
    SymbolReferences,
    SymbolRename,
    TypeOf,
    EffectOf,
    EffectsOf,
    CallGraph,
    Hover,
    Completion,
    SemanticTokens,
    CancelRequest,
}

impl Method {
    pub fn as_str(&self) -> &'static str {
        match self {
            Method::ProjectOpen => "project.open",
            Method::SourceUpdate => "source.update",
            Method::SourceRemove => "source.remove",
            Method::DiagnosticsGet => "diagnostics.get",
            Method::SymbolAt => "symbol.at",
            Method::SymbolDefinition => "symbol.definition",
            Method::SymbolReferences => "symbol.references",
            Method::SymbolRename => "symbol.rename",
            Method::TypeOf => "type.of",
            Method::EffectOf => "effect.of",
            Method::EffectsOf => "effects.of",
            Method::CallGraph => "callGraph.for",
            Method::Hover => "hover",
            Method::Completion => "completion",
            Method::SemanticTokens => "semantic.tokens",
            Method::CancelRequest => "cancel.request",
        }
    }

    pub fn parse(s: &str) -> Option<Method> {
        Some(match s {
            "project.open" => Method::ProjectOpen,
            "source.update" => Method::SourceUpdate,
            "source.remove" => Method::SourceRemove,
            "diagnostics.get" => Method::DiagnosticsGet,
            "symbol.at" => Method::SymbolAt,
            "symbol.definition" => Method::SymbolDefinition,
            "symbol.references" => Method::SymbolReferences,
            "symbol.rename" => Method::SymbolRename,
            "type.of" => Method::TypeOf,
            "effect.of" => Method::EffectOf,
            "effects.of" => Method::EffectsOf,
            "callGraph.for" => Method::CallGraph,
            "hover" => Method::Hover,
            "completion" => Method::Completion,
            "semantic.tokens" => Method::SemanticTokens,
            "cancel.request" => Method::CancelRequest,
            _ => return None,
        })
    }
}

impl serde::Serialize for Method {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

/// Parameter payloads for each method.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Params {
    ProjectOpen {
        root: String,
    },
    SourceUpdate {
        path: String,
        revision: u64,
        text: String,
    },
    SourceRemove {
        path: String,
    },
    DiagnosticsGet {
        /// Optional revision to query; when absent, use latest.
        revision: Option<u64>,
    },
    SymbolAt {
        path: String,
        offset: u32,
    },
    SymbolDefinition {
        symbol: SymbolId,
    },
    SymbolReferences {
        symbol: SymbolId,
    },
    SymbolRename {
        source_path: String,
        offset: u32,
        new_name: String,
    },
    TypeOf {
        path: String,
        offset: u32,
    },
    EffectOf {
        path: String,
        offset: u32,
    },
    EffectsOf {
        path: String,
        offset: u32,
    },
    CallGraph {
        symbol: SymbolId,
    },
    Hover {
        path: String,
        offset: u32,
    },
    Completion {
        path: String,
        offset: u32,
    },
    SemanticTokens {
        path: String,
    },
    CancelRequest {
        request_id: RequestId,
    },
}

/// A resolved symbol reference (definition or use site).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolLocation {
    pub symbol: SymbolId,
    pub name: String,
    pub path: String,
    pub span: SchemaSpan,
}

/// One entry in the semantic token stream.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticToken {
    pub start: u32,
    pub length: u32,
    pub token_type: String,
}

/// Result payloads for each method.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ResultPayload {
    ProjectOpen { revision: u64 },
    SourceUpdate { revision: u64 },
    SourceRemove { revision: u64 },
    DiagnosticsGet { diagnostics: Vec<MachineDiagnostic> },
    SymbolAt { symbol: Option<SymbolLocation> },
    SymbolDefinition { location: Option<SymbolLocation> },
    SymbolReferences { references: Vec<SymbolLocation> },
    SymbolRename { edits: Vec<TextEdit> },
    TypeOf { ty: Option<String> },
    EffectOf { effect: Option<String> },
    EffectsOf { effects: Vec<String> },
    CallGraph { calls: Vec<SymbolLocation> },
    Hover { content: Option<String> },
    Completion { items: Vec<CompletionItem> },
    SemanticTokens { tokens: Vec<SemanticToken> },
    Cancelled,
}

/// A completion candidate surfaced to completions clients.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionItem {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

/// Request envelope: protocolVersion + requestId + method + params.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestEnvelope {
    pub protocol_version: u32,
    pub request_id: RequestId,
    pub method: Method,
    pub params: Params,
}

/// Response envelope: requestId + result (or error) + diagnostics.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseEnvelope {
    pub protocol_version: u32,
    pub request_id: RequestId,
    pub result: ResultPayload,
    pub diagnostics: Vec<MachineDiagnostic>,
}

/// Protocol-level errors raised before a method can execute.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ToolingProtocolError {
    #[error("NEXA-TOOLPROTO-0001: Unsupported protocol version {version} (current {current})")]
    UnsupportedVersion { version: u32, current: u32 },
    #[error("NEXA-TOOLPROTO-0002: Unknown method '{method}'")]
    UnknownMethod { method: String },
    #[error("NEXA-TOOLPROTO-0003: Unknown source path '{path}'")]
    UnknownSource { path: String },
    #[error("NEXA-TOOLPROTO-0004: Request refers to a stale source revision")]
    StaleRevision,
    #[error("NEXA-TOOLPROTO-0005: The request was cancelled")]
    Cancelled,
}

impl RequestEnvelope {
    pub fn new(request_id: RequestId, method: Method, params: Params) -> Self {
        RequestEnvelope {
            protocol_version: PROTOCOL_VERSION,
            request_id,
            method,
            params,
        }
    }

    /// Validate that the envelope carries the current protocol version.
    pub fn validate_version(&self) -> Result<(), ToolingProtocolError> {
        if self.protocol_version != PROTOCOL_VERSION {
            return Err(ToolingProtocolError::UnsupportedVersion {
                version: self.protocol_version,
                current: PROTOCOL_VERSION,
            });
        }
        Ok(())
    }
}

impl ResponseEnvelope {
    pub fn success(request_id: RequestId, result: ResultPayload) -> Self {
        ResponseEnvelope {
            protocol_version: PROTOCOL_VERSION,
            request_id,
            result,
            diagnostics: Vec::new(),
        }
    }

    pub fn new(
        request_id: RequestId,
        result: ResultPayload,
        diagnostics: Vec<MachineDiagnostic>,
    ) -> Self {
        ResponseEnvelope {
            protocol_version: PROTOCOL_VERSION,
            request_id,
            result,
            diagnostics,
        }
    }
}

/// Convert a NEXA byte offset span into an LSP UTF-16 position on a line.
///
/// `line_starts` is the byte offset of each line start; `utf16` should be the
/// total UTF-16 code-unit offset for the given byte column (computed by the
/// LSP adapter).
pub fn byte_offset_to_utf16(
    offset: u32,
    line_starts: &[u32],
    utf16_of: &impl Fn(u32) -> u32,
) -> Option<Utf16Position> {
    if line_starts.is_empty() {
        return None;
    }
    let mut line = 0u32;
    for (i, &start) in line_starts.iter().enumerate() {
        if start <= offset {
            line = i as u32;
        } else {
            break;
        }
    }
    let line_start = line_starts[line as usize];
    let byte_col = offset - line_start;
    let character = utf16_of(byte_col);
    Some(Utf16Position { line, character })
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexa_source::SourceId;

    #[test]
    fn protocol_version_is_one() {
        assert_eq!(PROTOCOL_VERSION, 1);
        assert_eq!(PROTOCOL_NAME_SHORT, "Tooling Protocol");
    }

    #[test]
    fn envelope_round_trips_camel_case() {
        let req = RequestEnvelope::new(
            RequestId(7),
            Method::DiagnosticsGet,
            Params::DiagnosticsGet { revision: Some(3) },
        );
        let v = serde_json::to_value(&req).unwrap();
        assert_eq!(v["protocolVersion"], 1);
        assert_eq!(v["requestId"], 7);
        assert_eq!(v["method"], "diagnostics.get");
        assert_eq!(v["params"]["diagnosticsGet"]["revision"], 3);
    }

    #[test]
    fn method_str_round_trip() {
        for m in [
            Method::ProjectOpen,
            Method::SourceUpdate,
            Method::DiagnosticsGet,
            Method::SymbolAt,
            Method::CallGraph,
            Method::SemanticTokens,
        ] {
            assert_eq!(Method::parse(m.as_str()), Some(m));
        }
        assert_eq!(Method::parse("nope"), None);
    }

    #[test]
    fn validate_version_rejects_unsupported() {
        let mut req = RequestEnvelope::new(
            RequestId(1),
            Method::Hover,
            Params::Hover {
                path: "a.nexa".into(),
                offset: 0,
            },
        );
        assert!(req.validate_version().is_ok());
        req.protocol_version = 99;
        match req.validate_version() {
            Err(ToolingProtocolError::UnsupportedVersion { version, .. }) => {
                assert_eq!(version, 99)
            }
            _ => panic!("expected unsupported version"),
        }
    }

    #[test]
    fn text_edit_builds_schema_span() {
        let edit = TextEdit::new(SourceSpan::new(SourceId(0), 4, 8), "new");
        assert_eq!(edit.span.start, 4);
        assert_eq!(edit.span.end, 8);
        assert_eq!(edit.new_text, "new");
    }

    #[test]
    fn byte_offset_to_utf16_selects_line() {
        let line_starts = [0u32, 5, 12];
        let utf16 = |col: u32| col;
        assert_eq!(
            byte_offset_to_utf16(6, &line_starts, &utf16),
            Some(Utf16Position {
                line: 1,
                character: 1
            })
        );
        assert_eq!(
            byte_offset_to_utf16(3, &line_starts, &utf16),
            Some(Utf16Position {
                line: 0,
                character: 3
            })
        );
        assert_eq!(byte_offset_to_utf16(0, &[], &utf16), None);
    }

    #[test]
    fn symbol_id_sentinel() {
        assert_eq!(SymbolId::INVALID, SymbolId(u32::MAX));
        assert_eq!(SymbolId::new(3), SymbolId(3));
    }
}
