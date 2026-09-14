//! NEXA — Compiler
//!
//! Pipeline da Implementação 02 (escopo: até o fim do parsing):
//!
//! ```text
//! bytes → UTF-8 validation → SourceFile (line index) → Lexer
//!      → Lossless Lexeme Stream → Parser → CST + AST + Diagnostics
//!      → output (human | json)
//! ```
//!
//! Este crate é o **driver**: orquestra `nexa-source` + `nexa-diagnostics`
//! + `nexa-lexer` + `nexa-parser` e re-exporta a API pública consumida pelo
//!
//! CLI e pela CTS.

pub mod check;
pub mod codegen;
pub mod project;
pub mod resolve;
pub mod version;

pub use check::{check_output_json, type_severity_label, CheckOutputEnvelope, TypeCheckResult};
pub use codegen::{compile_source_to_wasm, CompileArtifact, CompileError};
use nexa_diagnostics::code::LEX_INVALID_UTF8;
pub use nexa_diagnostics::{render_diagnostics, Diagnostic, Severity};
pub use nexa_lexer::json::{human_kind, lex_output_json, LexOutputEnvelope};
pub use nexa_lexer::{
    lex, Keyword, LexResult, Lexeme, LexemeKind, StringStyle, TokenKind, TriviaKind,
};
pub use nexa_parser::{
    parse, parse_with_lexemes, ParseMode, ParseResult, DEFAULT_DIAGNOSTIC_LIMIT,
    DEFAULT_PARSE_DEPTH_LIMIT,
};
use nexa_source::{SourceLoadError, SourceManager};
pub use project::{ast_projection, PARSER_PROJECTION_SCHEMA_VERSION};
pub use resolve::{
    location_str, resolve_output_json, symbol_kind_label, ResolveOutputEnvelope, SemanticResult,
};
use serde::Serialize;
use std::path::Path;

/// Versão do schema JSON de tooling (golden CTS).
pub const SCHEMA_VERSION: u32 = 1;

/// Envelope `nexa parse --format json` (Impl 02 §478): diagnostics + dump
/// estrutural do AST (projeção de tooling/CTS, §479-483).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParseOutputEnvelope {
    pub schema_version: u32,
    pub diagnostics: Vec<Diagnostic>,
    pub ast_dump: serde_json::Value,
}

/// Constrói o envelope JSON do output do parser.
pub fn parse_output_json(result: &ParseResult) -> ParseOutputEnvelope {
    ParseOutputEnvelope {
        schema_version: SCHEMA_VERSION,
        diagnostics: result.diagnostics.clone(),
        ast_dump: ast_projection(&result.ast),
    }
}

/// Pipeline de compilação da sessão.
#[derive(Debug, Default)]
pub struct Pipeline {
    pub sources: SourceManager,
}

impl Pipeline {
    pub fn new() -> Self {
        Pipeline {
            sources: SourceManager::new(),
        }
    }

    /// Carrega um texto nomeado e roda o lexer. O `SourceFile` fica
    /// registrado em `sources` para renderização de spans.
    pub fn lex_source(&mut self, name: &str, content: &str) -> LexResult {
        let id = self.sources.load_text(name.into(), content.to_owned());
        let source = self.sources.source(id).expect("just loaded source");
        lex(source)
    }

    /// Fronteira de carregamento real (impl 01 §333-341): recebe **bytes**
    /// brutos, valida UTF-8 e lexa. Bytes inválidos viram `NEXA-LEX-0001`
    /// (que pertence ao loading boundary); nenhum `SourceFile` é criado.
    ///
    /// Chamadores externos (CLI, CTS runner) usam esta API — erros de I/O
    /// do filesystem são responsabilidade do chamador.
    pub fn lex_bytes(&mut self, display_path: &Path, bytes: Vec<u8>) -> LexResult {
        match self.sources.load_bytes(display_path.to_owned(), bytes) {
            Ok(id) => {
                let source = self.sources.source(id).expect("just loaded source");
                lex(source)
            }
            Err(SourceLoadError::InvalidUtf8 { byte_offset, .. }) => {
                let diagnostic = Diagnostic::error(
                    LEX_INVALID_UTF8,
                    "lexer",
                    "lexer.invalid_utf8",
                    "source is not valid UTF-8",
                )
                .with_argument("byteOffset", byte_offset as u64);
                LexResult {
                    lexemes: Vec::new(),
                    diagnostics: vec![diagnostic],
                }
            }
            Err(SourceLoadError::Io { .. }) => {
                unreachable!("load_bytes nunca emite SourceLoadError::Io")
            }
        }
    }

    /// Carrega um texto nomeado, lexa e parseia como **ProjectSource** (entrada
    /// de projeto: `module` obrigatório antes de imports/declarações).
    pub fn parse_source(&mut self, name: &str, content: &str) -> ParseResult {
        let id = self.sources.load_text(name.into(), content.to_owned());
        let source = self.sources.source(id).expect("just loaded source");
        parse(source, ParseMode::ProjectSource)
    }

    /// Fronteira real de parse a partir de **bytes** (Impl 02 §528-533): valida
    /// UTF-8 e, em caso de bytes inválidos, não roda o parser — retorna o
    /// diagnóstico de loading na fronteira (análogo a `lex_bytes`).
    #[allow(clippy::result_large_err)]
    pub fn parse_bytes(
        &mut self,
        display_path: &Path,
        bytes: Vec<u8>,
    ) -> Result<ParseResult, Diagnostic> {
        match self.sources.load_bytes(display_path.to_owned(), bytes) {
            Ok(id) => {
                let source = self.sources.source(id).expect("just loaded source");
                Ok(parse(source, ParseMode::ProjectSource))
            }
            Err(SourceLoadError::InvalidUtf8 { byte_offset, .. }) => Err(Diagnostic::error(
                LEX_INVALID_UTF8,
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
