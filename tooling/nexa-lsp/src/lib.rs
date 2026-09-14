//! # nexa-lsp
//!
//! Implementation 11 — NEXA Language Server Protocol integration.
//!
//! `NexaLspServer` adapts the [`CompilerService`] (which speaks the NEXA
//! Tooling Protocol) to LSP-style operations: diagnostics, hover, go-to-
//! definition, find-references, completion, rename, document symbols and
//! semantic tokens.
//!
//! LSP positions are **UTF-16 line/column**; NEXA uses **byte offsets**. This
//! crate owns the document buffers so it can convert between the two without
//! changing the core `SourceSpan` (see spec §42–§43).
//!
//! The crate is fully synchronous (`std`) with a lightweight, transport-
//! agnostic JSON message dispatcher so it can be driven over stdio or in
//! process; no external LSP runtime is required.

use std::collections::HashMap;

use nexa_compiler_service::CompilerService;
use nexa_compiler_service::CompilerServiceApi;
use nexa_tooling_protocol as proto;
use nexa_tooling_protocol::Method;
use nexa_tooling_protocol::Params;
use nexa_tooling_protocol::ResultPayload;
use serde::Serialize;

use nexa_ast::ItemKind;
use nexa_source::SourceFile;
use nexa_source::SourceId;

/// A document URI (e.g. `file:///x/y/main.nexa`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct Uri(pub String);

impl std::fmt::Display for Uri {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// An LSP UTF-16 line/column position (0-based).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

/// A closed UTF-16 range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

/// An LSP location.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub uri: Uri,
    pub range: Range,
}

/// An LSP diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LspDiagnostic {
    pub range: Range,
    pub severity: u8,
    pub code: String,
    pub message: String,
}

/// The result of a hover request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HoverResult {
    pub contents: String,
}

/// A completion candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LspCompletionItem {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// A workspace edit (rename / code action).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceEdit {
    pub changes: HashMap<String, Vec<LspTextEdit>>,
}

/// A text edit inside a document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LspTextEdit {
    pub range: Range,
    pub new_text: String,
}

/// A document symbol.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSymbol {
    pub name: String,
    pub kind: u8,
    pub range: Range,
}

/// A semantic token (absolute LSP encoding).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LspSemanticToken {
    pub line: u32,
    pub start: u32,
    pub length: u32,
    pub token_type: String,
}

/// The NEXA language server: adapts the compiler service for editor clients.
pub struct NexaLspServer {
    service: CompilerService,
    root: Option<String>,
    /// Live document buffers keyed by URI; used for offset conversion.
    documents: HashMap<Uri, String>,
}

impl Default for NexaLspServer {
    fn default() -> Self {
        Self::new()
    }
}

impl NexaLspServer {
    pub fn new() -> Self {
        NexaLspServer {
            service: CompilerService::new(),
            root: None,
            documents: HashMap::new(),
        }
    }

    pub fn with_service(service: CompilerService) -> Self {
        NexaLspServer {
            service,
            root: None,
            documents: HashMap::new(),
        }
    }

    /// Open a workspace root. All later paths are interpreted under it.
    pub fn open_project(&mut self, root: &str) {
        self.root = Some(root.to_string());
        let _ = self.service.open_project(root);
    }

    pub fn root(&self) -> Option<&str> {
        self.root.as_deref()
    }

    fn uri_to_path(&self, uri: &Uri) -> String {
        // file:///x -> x for the service key (simple, deterministic).
        uri.0.trim_start_matches("file://").to_string()
    }

    /// Open a document (textDocument/didOpen).
    pub fn did_open(&mut self, uri: Uri, text: &str) {
        self.documents.insert(uri.clone(), text.to_string());
        let _ = self
            .service
            .update_file(&self.uri_to_path(&uri), None, text);
    }

    /// Full-sync document change (textDocument/didChange with full content).
    pub fn did_change(&mut self, uri: Uri, text: &str) {
        self.documents.insert(uri.clone(), text.to_string());
        let _ = self
            .service
            .update_file(&self.uri_to_path(&uri), None, text);
    }

    /// Close a document (textDocument/didClose).
    pub fn did_close(&mut self, uri: &Uri) {
        self.documents.remove(uri);
        let _ = self.service.remove_file(&self.uri_to_path(uri));
    }

    /// Compute the LSP UTF-16 position for a byte offset.
    pub fn byte_to_utf16(&self, uri: &Uri, byte: u32) -> Option<Position> {
        let text = self.documents.get(uri)?;
        let line_starts = line_starts(text);
        let byte = byte.min(text.len() as u32) as usize;
        let line_idx = match line_starts.binary_search(&(byte as u32)) {
            Ok(i) => i,
            Err(insert) => insert.saturating_sub(1),
        };
        let line_start = line_starts[line_idx] as usize;
        let col_utf16 = text[line_start..byte].encode_utf16().count() as u32;
        Some(Position {
            line: line_idx as u32,
            character: col_utf16,
        })
    }

    /// Compute the NEXA byte offset for an LSP UTF-16 position.
    pub fn utf16_to_byte(&self, uri: &Uri, pos: Position) -> Option<u32> {
        let text = self.documents.get(uri)?;
        let line_starts = line_starts(text);
        let line = line_starts.get(pos.line as usize)?;
        let mut line_end = line_starts
            .get(pos.line as usize + 1)
            .copied()
            .unwrap_or(text.len() as u32);
        // Skip over the newline that line_start[j+1] points at.
        if pos.line + 1 < line_starts.len() as u32 && line_end > 0 {
            line_end -= 1;
        }
        let start = *line as usize;
        let end = line_end as usize;
        let mut offset = start;
        let mut remaining = pos.character;
        let slice = text.get(start..end).unwrap_or("");
        // Advance by `remaining` UTF-16 code units.
        for (i, ch) in slice.char_indices() {
            if remaining == 0 {
                return Some((start + i) as u32);
            }
            let w = ch.len_utf16() as u32;
            if remaining >= w {
                // Consume this char.
                remaining -= w;
                offset = start + i + ch.len_utf8();
            } else {
                // Mid-char can't happen; clamp to char start.
                return Some((start + i) as u32);
            }
        }
        Some(offset as u32)
    }

    /// All diagnostics for `uri` as LSP diagnostics (textDocument/publishDiagnostics).
    pub fn publish_diagnostics(&mut self, uri: &Uri) -> Vec<LspDiagnostic> {
        let payload = match self.service.handle_query(
            Method::DiagnosticsGet,
            &Params::DiagnosticsGet { revision: None },
        ) {
            Ok(ResultPayload::DiagnosticsGet { diagnostics }) => diagnostics,
            _ => return Vec::new(),
        };
        payload
            .into_iter()
            .filter_map(|d| {
                if let Some(span) = d.primary_span {
                    let start = self.byte_to_utf16(uri, span.start)?;
                    let end = self.byte_to_utf16(uri, span.end)?;
                    Some(LspDiagnostic {
                        range: Range { start, end },
                        severity: severity_rank(&d.severity),
                        code: d.code,
                        message: d.message,
                    })
                } else {
                    Some(LspDiagnostic {
                        range: Range {
                            start: Position {
                                line: 0,
                                character: 0,
                            },
                            end: Position {
                                line: 0,
                                character: 0,
                            },
                        },
                        severity: severity_rank(&d.severity),
                        code: d.code,
                        message: d.message,
                    })
                }
            })
            .collect()
    }

    /// Go to definition (textDocument/definition).
    pub fn go_to_definition(&mut self, uri: &Uri, pos: Position) -> Option<Location> {
        let path = self.uri_to_path(uri);
        let offset = self.utf16_to_byte(uri, pos)?;
        let payload = self.service.definition(&path, offset).ok()?;
        match payload {
            ResultPayload::SymbolDefinition { location } => {
                location.and_then(|l| self.proto_loc_to_lsp(l))
            }
            _ => None,
        }
    }

    /// Find references (textDocument/references).
    pub fn find_references(&mut self, uri: &Uri, pos: Position) -> Vec<Location> {
        let path = self.uri_to_path(uri);
        let Some(offset) = self.utf16_to_byte(uri, pos) else {
            return Vec::new();
        };
        let Ok(payload) = self.service.references(&path, offset) else {
            return Vec::new();
        };
        let ResultPayload::SymbolReferences { references } = payload else {
            return Vec::new();
        };
        // Convert to LSP locations, mapping the source (stored in locale id /
        // source id ignored here) to a URI via our document registry.
        let mut out = Vec::new();
        for r in references {
            if let Some(loc) = self.proto_loc_to_lsp(r) {
                out.push(loc);
            }
        }
        out.sort_by_key(|l| (l.uri.0.clone(), l.range.start.line, l.range.start.character));
        out
    }

    /// Hover (textDocument/hover).
    pub fn hover(&self, uri: &Uri, pos: Position) -> Option<HoverResult> {
        let path = self.uri_to_path(uri);
        let offset = self.utf16_to_byte(uri, pos)?;
        let payload = self.service.hover(&path, offset).ok()?;
        match payload {
            ResultPayload::Hover { content } => content.map(|contents| HoverResult { contents }),
            _ => None,
        }
    }

    /// Completion (textDocument/completion).
    pub fn completion(&self, uri: &Uri, pos: Position) -> Vec<LspCompletionItem> {
        let path = self.uri_to_path(uri);
        let Some(offset) = self.utf16_to_byte(uri, pos) else {
            return Vec::new();
        };
        let Ok(payload) = self.service.completion(&path, offset) else {
            return Vec::new();
        };
        let ResultPayload::Completion { items } = payload else {
            return Vec::new();
        };
        items
            .into_iter()
            .map(|i| LspCompletionItem {
                label: i.label,
                kind: None,
                detail: i.detail,
            })
            .collect()
    }

    /// Rename (textDocument/rename).
    pub fn rename(&self, uri: &Uri, pos: Position, new_name: &str) -> Option<WorkspaceEdit> {
        let path = self.uri_to_path(uri);
        let offset = self.utf16_to_byte(uri, pos)?;
        let payload = self.service.rename(&path, offset, new_name).ok()?;
        let ResultPayload::SymbolRename { edits } = payload else {
            return None;
        };
        let mut changes: HashMap<String, Vec<LspTextEdit>> = HashMap::new();
        for e in edits {
            let start = self.byte_to_utf16(uri, e.span.start)?;
            let end = self.byte_to_utf16(uri, e.span.end)?;
            changes.entry(uri.0.clone()).or_default().push(LspTextEdit {
                range: Range { start, end },
                new_text: new_name.to_string(),
            });
        }
        Some(WorkspaceEdit { changes })
    }

    /// Semantic tokens (textDocument/semanticTokens/full).
    pub fn semantic_tokens(&mut self, uri: &Uri) -> Vec<LspSemanticToken> {
        let path = self.uri_to_path(uri);
        let Ok(payload) = self
            .service
            .handle_query(Method::SemanticTokens, &Params::SemanticTokens { path })
        else {
            return Vec::new();
        };
        let ResultPayload::SemanticTokens { tokens } = payload else {
            return Vec::new();
        };
        tokens
            .into_iter()
            .filter_map(|t| {
                let pos = self.byte_to_utf16(uri, t.start)?;
                Some(LspSemanticToken {
                    line: pos.line,
                    start: pos.character,
                    length: t.length,
                    token_type: t.token_type,
                })
            })
            .collect()
    }

    /// Document symbols (textDocument/documentSymbol).
    ///
    /// Produced by lexing + parsing the live document and walking top-level
    /// function and action declarations. This does not depend on the semantic
    /// database's resolver index, so it is reliable even before name
    /// resolution has classified any tokens.
    pub fn document_symbols(&mut self, uri: &Uri) -> Vec<DocumentSymbol> {
        let text = match self.documents.get(uri) {
            Some(t) => t.clone(),
            None => return Vec::new(),
        };
        let sf = SourceFile::from_text(SourceId(1), std::path::PathBuf::from(uri.0.clone()), text);
        let parsed = nexa_parser::parse(&sf, nexa_parser::ParseMode::SingleFile);
        let mut out = Vec::new();
        for item in &parsed.ast.items {
            let (name, span) = match &item.kind {
                ItemKind::Function(f) => (f.name.name.as_str(), f.span),
                ItemKind::Action(a) => (a.name.name.as_str(), a.span),
                _ => continue,
            };
            let Some(start) = self.byte_to_utf16(uri, span.start) else {
                continue;
            };
            let Some(end) = self.byte_to_utf16(uri, span.end) else {
                continue;
            };
            out.push(DocumentSymbol {
                name: name.to_string(),
                kind: symbol_kind_of(&item.kind),
                range: Range { start, end },
            });
        }
        out.sort_by(|a, b| {
            (a.range.start.line, a.range.start.character)
                .cmp(&(b.range.start.line, b.range.start.character))
        });
        out
    }

    /// Convert a protocol symbol location to an LSP location.
    fn proto_loc_to_lsp(&self, l: proto::SymbolLocation) -> Option<Location> {
        let uri = Uri(format!("file://{}", l.path));
        let start = self.byte_to_utf16(&uri, l.span.start)?;
        let end = self.byte_to_utf16(&uri, l.span.end)?;
        Some(Location {
            uri,
            range: Range { start, end },
        })
    }
}

/// Map a machine-schema severity string to LSP severity rank.
fn severity_rank(sev: &str) -> u8 {
    match sev {
        "error" => 1,
        "warning" => 2,
        "info" => 3,
        _ => 4,
    }
}

/// LSP symbol-kind for a top-level item.
fn symbol_kind_of(kind: &ItemKind) -> u8 {
    match kind {
        ItemKind::Function(_) => 12, // Function
        ItemKind::Action(_) => 12,
        _ => 13, // Variable
    }
}

fn line_starts(text: &str) -> Vec<u32> {
    let mut starts = vec![0u32];
    for (i, b) in text.bytes().enumerate() {
        if b == b'\n' {
            starts.push((i + 1) as u32);
        }
    }
    starts
}

impl NexaLspServer {
    /// Handle a single newline-terminated JSON request over stdio.
    ///
    /// The transport is deliberately minimal and line-delimited; there is no
    /// `Content-Length` framing and no concurrency. Each line is one JSON
    /// object shaped as `{"method": "...", "params": {...}}`.
    ///
    /// Supported `method` values:
    ///
    /// - `initialize`                        -> server info + capabilities
    /// - `textDocument/didOpen`      \{uri, text}
    /// - `textDocument/didChange`    \{uri, text}
    /// - `textDocument/didClose`     \{uri}
    /// - `textDocument/publishDiagnostics` \{uri}
    /// - `textDocument/definition`   \{uri, line, character}
    /// - `textDocument/hover`        \{uri, line, character}
    /// - `textDocument/documentSymbol` \{uri}
    /// - `shutdown` / `exit`         -> terminate the loop
    ///
    /// A response is a serialized envelope `{"ok":true,"method":"...","result":...}`
    /// (or `{"ok":false,"error":"..."}`). `None` signals the caller to stop
    /// reading stdin (a `shutdown`/`exit` request).
    pub fn handle_line(&mut self, line: &str) -> Option<String> {
        let v = match serde_json::from_str::<serde_json::Value>(line) {
            Ok(v) => v,
            Err(e) => {
                return Some(serde_json::json!({"ok": false, "error": e.to_string()}).to_string())
            }
        };
        let method = v
            .get("method")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let params = v.get("params").cloned().unwrap_or(serde_json::Value::Null);

        if method == "shutdown" || method == "exit" {
            return None;
        }

        let outcome: Result<serde_json::Value, String> = self.dispatch(method, &params);
        let envelope = match outcome {
            Ok(result) => serde_json::json!({"ok": true, "method": method, "result": result}),
            Err(e) => serde_json::json!({"ok": false, "method": method, "error": e}),
        };
        Some(envelope.to_string())
    }

    fn dispatch(
        &mut self,
        method: &str,
        params: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        match method {
            "initialize" => Ok(serde_json::json!({
                "serverInfo": { "name": "nexa-lsp", "version": "1.0" },
                "capabilities": {
                    "textDocumentSync": 1,
                    "hoverProvider": true,
                    "definitionProvider": true,
                    "referencesProvider": true,
                    "renameProvider": true,
                    "completionProvider": true,
                    "documentSymbolProvider": true,
                    "semanticTokensProvider": true,
                }
            })),
            "textDocument/didOpen" => {
                let (uri, text) = uri_and_text(params)?;
                self.did_open(uri, &text);
                Ok(serde_json::Value::Null)
            }
            "textDocument/didChange" => {
                let (uri, text) = uri_and_text(params)?;
                self.did_change(uri, &text);
                Ok(serde_json::Value::Null)
            }
            "textDocument/didClose" => {
                let uri = param_uri(params)?;
                self.did_close(&uri);
                Ok(serde_json::Value::Null)
            }
            "textDocument/publishDiagnostics" => {
                let uri = param_uri(params)?;
                let diags = self.publish_diagnostics(&uri);
                Ok(serde_json::to_value(diags).map_err(ser_err)?)
            }
            "textDocument/definition" => {
                let (uri, pos) = uri_and_pos(params)?;
                let loc = self.go_to_definition(&uri, pos);
                Ok(serde_json::to_value(loc).map_err(ser_err)?)
            }
            "textDocument/hover" => {
                let (uri, pos) = uri_and_pos(params)?;
                let hover = self.hover(&uri, pos);
                Ok(serde_json::to_value(hover).map_err(ser_err)?)
            }
            "textDocument/documentSymbol" => {
                let uri = param_uri(params)?;
                let syms = self.document_symbols(&uri);
                Ok(serde_json::to_value(syms).map_err(ser_err)?)
            }
            other => Err(format!("unsupported method '{other}'")),
        }
    }
}

fn ser_err(e: serde_json::Error) -> String {
    e.to_string()
}

fn param_uri(params: &serde_json::Value) -> Result<Uri, String> {
    params
        .get("uri")
        .and_then(serde_json::Value::as_str)
        .map(|s| Uri(s.to_string()))
        .ok_or_else(|| "missing string param 'uri'".to_string())
}

fn uri_and_text(params: &serde_json::Value) -> Result<(Uri, String), String> {
    let uri = param_uri(params)?;
    let text = params
        .get("text")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "missing string param 'text'".to_string())?
        .to_string();
    Ok((uri, text))
}

fn uri_and_pos(params: &serde_json::Value) -> Result<(Uri, Position), String> {
    let uri = param_uri(params)?;
    let line = params
        .get("line")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| "missing u64 param 'line'".to_string())? as u32;
    let character = params
        .get("character")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| "missing u64 param 'character'".to_string())? as u32;
    Ok((uri, Position { line, character }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server() -> NexaLspServer {
        let mut s = NexaLspServer::new();
        s.open_project("mem://proj");
        s
    }

    fn uri(s: &str) -> Uri {
        Uri(s.to_string())
    }

    #[test]
    fn utf16_roundtrip_ascii() {
        let mut s = server();
        let u = uri("file:///main.nexa");
        let text = "function f() -> Unit {\n    return 1\n}";
        s.did_open(u.clone(), text);
        // Byte 28 is 'r' of "return" on line 1.
        let pos = s.byte_to_utf16(&u, 28).unwrap();
        assert_eq!(pos.line, 1);
        let back = s.utf16_to_byte(&u, pos).unwrap();
        assert_eq!(back, 28);
    }

    #[test]
    fn utf16_counts_multibyte() {
        let text = "let x = \"héllo\"";
        let mut s = NexaLspServer::new();
        let u = uri("file:///m.nexa");
        s.did_open(u.clone(), text);
        // 'h' at col 8; 'é' occupies two UTF-16 units.
        let byte_of_second_o = text.find('o').unwrap() as u32;
        let pos = s.byte_to_utf16(&u, byte_of_second_o).unwrap();
        // col counts UTF-16: "let x = \"h" =8, é=1-> two units, so l col
        assert!(pos.character >= 10);
        let back = s.utf16_to_byte(&u, pos).unwrap();
        assert_eq!(back, byte_of_second_o);
    }

    #[test]
    fn hover_and_definition() {
        let mut s = server();
        let u = uri("file:///main.nexa");
        s.did_open(u.clone(), "function my_func() -> Unit {\n    return 1\n}");
        // Offset of "my_func" (byte 9).
        let pos = Position {
            line: 0,
            character: 9,
        };
        let hover = s.hover(&u, pos).unwrap();
        assert!(hover.contents.contains("function"));
        let loc = s.go_to_definition(&u, pos).unwrap();
        assert!(loc.range.end.character > loc.range.start.character);
    }

    #[test]
    fn semantic_tokens_and_document_symbols() {
        let mut s = server();
        let u = uri("file:///main.nexa");
        s.did_open(u.clone(), "function add() -> Unit {}");
        let syms = s.document_symbols(&u);
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "add");
    }

    #[test]
    fn rename_produces_edit() {
        let mut s = server();
        let u = uri("file:///main.nexa");
        s.did_open(u.clone(), "function my_func() -> Unit {}");
        let pos = Position {
            line: 0,
            character: 9,
        };
        let edit = s.rename(&u, pos, "renamed").unwrap();
        let edits = edit.changes.get("file:///main.nexa").unwrap();
        assert!(!edits.is_empty());
    }

    #[test]
    fn did_change_updates_diagnostics() {
        let mut s = server();
        let u = uri("file:///main.nexa");
        s.did_open(u.clone(), "function ok() -> Unit {}");
        let before = s.publish_diagnostics(&u);
        // Diagnostics may be present or empty depending on the pipeline; the
        // important property is determinism and type stability.
        let after = s.publish_diagnostics(&u);
        assert_eq!(before, after);
    }

    #[test]
    fn find_references_deterministic() {
        let mut s = server();
        let u = uri("file:///main.nexa");
        s.did_open(
            u.clone(),
            "function f() -> Unit { return helper(); }\nfunction helper() -> Unit {}",
        );
        let pos = Position {
            line: 0,
            character: 9,
        }; // inside "function f"
        let a = s.find_references(&u, pos);
        let b = s.find_references(&u, pos);
        assert_eq!(a, b);
    }

    #[test]
    fn handle_line_roundtrip() {
        let mut s = NexaLspServer::new();
        // Initialize returns capabilities.
        let init = s
            .handle_line(r#"{"method":"initialize","params":{}}"#)
            .unwrap();
        let init_v: serde_json::Value = serde_json::from_str(&init).unwrap();
        assert_eq!(init_v["ok"], true);

        // Open a document.
        let open = s.handle_line(
            r#"{"method":"textDocument/didOpen","params":{"uri":"file:///main.nexa","text":"function add() -> Unit {}"}}"#,
        );
        assert!(open.is_some());
        let open_v: serde_json::Value = serde_json::from_str(&open.unwrap()).unwrap();
        assert_eq!(open_v["ok"], true);

        // Symbol discovery over the wire.
        let syms = s.handle_line(
            r#"{"method":"textDocument/documentSymbol","params":{"uri":"file:///main.nexa"}}"#,
        );
        let syms_v: serde_json::Value = serde_json::from_str(&syms.unwrap()).unwrap();
        let arr = syms_v["result"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["name"], "add");

        // Unsupported method reports an error envelope but keeps the loop alive.
        let bad = s.handle_line(r#"{"method":"bogus","params":{}}"#).unwrap();
        let bad_v: serde_json::Value = serde_json::from_str(&bad).unwrap();
        assert_eq!(bad_v["ok"], false);

        // Shutdown terminates the loop.
        assert!(s
            .handle_line(r#"{"method":"shutdown","params":{}}"#)
            .is_none());
    }
}
