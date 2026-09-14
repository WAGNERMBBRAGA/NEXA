//! Parser NEXA — Implementação 02 (Core grammar, CST lossless, recovery).
//!
//! Consome a stream lossless de `Lexeme` (tokens + trivia) via `Cursor`,
//! produzindo simultaneamente:
//!   - AST (`SourceUnit`) com recovery estrutural (Missing/placeholders);
//!   - CST lossless (invariante `reconstruct == source.text`);
//!   - `Diagnostic` estruturados (`NEXA-PARSE-*`).
//!
//! Invariantes principais (Impl 02):
//!   - statement termina em newline/`}`/EOF (§131);
//!   - soft newline: o parser continua quando espera continuação (§138-139);
//!   - comparação/`==`/`!=` não-associativos com `NEXA-PARSE-0006`;
//!   - `ParseMode::ProjectSource` exige `module` como primeira declaração
//!     (§420) com import-before-module e import-after-declaration inválidos;
//!   - profundidade de aninhamento limitada (256, §343-345);
//!   - cap de diagnostics (100, §514-517);
//!   - token sintético Missing zero-width, excluído da reconstrução (§392).

use crate::cursor::Cursor;
use nexa_ast::{node::*, SourceUnit};
use nexa_cst::{serialize::reconstruct, token_kind_to_syntax, NodeOrToken, SyntaxKind};
use nexa_diagnostics::code::*;
use nexa_diagnostics::{Diagnostic, DiagnosticCode, Severity};
use nexa_lexer::{Keyword, Lexeme, StringStyle, TokenKind};
use nexa_source::{SourceFile, SourceSpan};

/// Profundidade máxima de aninhamento do parser (Impl 02 §345).
pub const DEFAULT_PARSE_DEPTH_LIMIT: usize = 256;
/// Cap de diagnostics por source (Impl 02 §515).
pub const DEFAULT_DIAGNOSTIC_LIMIT: usize = 100;
/// Binding power dos operadores prefixados (sempre acima dos infixos).
const PREFIX_BP: u8 = 21;

/// Modo de parse da unidade de compilação (§418-427).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseMode {
    /// Source de projeto: `module` é a primeira declaração significativa;
    /// imports antes do module e após uma declaração são inválidos.
    ProjectSource,
    /// Arquivo isolado (entrada única, ex.: REPL/estudo): sem obrigação de
    /// `module`; imports antes de declarações são aceitos.
    SingleFile,
}

/// Resultado completo do parse (§470-479).
#[derive(Debug, Clone)]
pub struct ParseResult {
    pub ast: SourceUnit,
    pub cst: Vec<NodeOrToken>,
    pub diagnostics: Vec<Diagnostic>,
    /// Reconstrução lossless do source a partir da CST (exclui Missing).
    pub reconstruction: String,
}

impl ParseResult {
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }
}

/// Parse conveniência: lexa internamente e mescla diagnostics do lexer e do
/// parser (lexer primeiro, preservando ordem de offset).
pub fn parse(source: &SourceFile, mode: ParseMode) -> ParseResult {
    let lex_result = nexa_lexer::lex(source);
    let mut result = parse_with_lexemes(source, &lex_result.lexemes, mode);
    let mut all = lex_result.diagnostics;
    all.append(&mut result.diagnostics);
    result.diagnostics = all;
    result
}

/// Parse a partir de uma stream lexical já produzida (pipeline).
pub fn parse_with_lexemes(source: &SourceFile, lexemes: &[Lexeme], mode: ParseMode) -> ParseResult {
    let mut parser = Parser::new(source, lexemes, mode);
    let ast = parser.parse_source_unit();
    let cst = parser.cursor.cst.finish_root();
    let reconstruction = reconstruct(&cst, source);
    ParseResult {
        ast,
        cst,
        diagnostics: parser.diagnostics,
        reconstruction,
    }
}

struct Parser<'a> {
    cursor: Cursor<'a>,
    mode: ParseMode,
    diagnostics: Vec<Diagnostic>,
    /// Deduplica erros reportados na mesma offset (§336).
    last_error_offset: Option<u32>,
    /// Quando o cap de diagnostics é atingido, para de reportar (§514-517).
    frozen: bool,
    /// Profundidade de delimitadores `()`, `[]` onde newlines são "soft"
    /// (separadores trivia-like, §134).
    delim_depth: usize,
    /// Desliga struct literal durante o parse de condição/scrutinee de
    /// `if`/`while`/`match`/`for`, onde `{` pertence ao constructo (§422).
    allow_struct_literal: bool,
}

impl<'a> Parser<'a> {
    fn new(source: &'a SourceFile, lexemes: &'a [Lexeme], mode: ParseMode) -> Self {
        Parser {
            cursor: Cursor::new(source, lexemes),
            mode,
            diagnostics: Vec::new(),
            last_error_offset: None,
            frozen: false,
            delim_depth: 0,
            allow_struct_literal: true,
        }
    }

    // ------------------------------------------------------------------
    // helpers de span / tokens
    // ------------------------------------------------------------------

    /// Hull de dois spans do mesmo source (`min start .. max end`).
    /// Não exige sobreposição/adjacência: itens como `function` cobrem do
    /// keyword até o corpo mesmo com newline/contratos entre eles.
    fn cover(&self, a: SourceSpan, b: SourceSpan) -> SourceSpan {
        if a.source != b.source {
            return a;
        }
        SourceSpan::new(a.source, a.start.min(b.start), a.end.max(b.end))
    }

    fn start_span(&self) -> SourceSpan {
        self.cursor.peek_span()
    }

    fn peek_ident_text(&self) -> Option<&'a str> {
        self.cursor.peek_identifier_text()
    }

    fn at_keyword(&self, kw: Keyword) -> bool {
        self.cursor.at_keyword(kw)
    }

    fn at_ident(&self) -> bool {
        self.cursor.peek_kind() == TokenKind::Identifier
    }

    fn at(&self, kind: TokenKind) -> bool {
        self.cursor.at(kind)
    }

    fn bump(&mut self) -> SourceSpan {
        self.cursor.bump().1
    }

    fn describe_current(&self) -> String {
        match self.cursor.peek_kind() {
            TokenKind::Eof => "end of file".to_string(),
            TokenKind::Newline => "a new line".to_string(),
            TokenKind::Identifier => format!(
                "identifier `{}`",
                self.cursor.peek_identifier_text().unwrap_or("_")
            ),
            TokenKind::Keyword(k) => format!("keyword `{}`", k.as_str()),
            TokenKind::IntegerLiteral => "integer literal".to_string(),
            TokenKind::FloatLiteral => "float literal".to_string(),
            TokenKind::CharLiteral => "char literal".to_string(),
            TokenKind::StringStart(_) | TokenKind::StringEnd(_) | TokenKind::StringText => {
                "string literal".to_string()
            }
            TokenKind::RawStringLiteral => "raw string literal".to_string(),
            TokenKind::ByteStringLiteral => "byte string literal".to_string(),
            _ => format!("`{:?}`", self.cursor.peek_kind()),
        }
    }

    fn report(
        &mut self,
        code: DiagnosticCode,
        span: SourceSpan,
        key: &'static str,
        message: String,
    ) {
        if self.frozen {
            return;
        }
        if self.last_error_offset == Some(span.start) {
            return;
        }
        self.last_error_offset = Some(span.start);
        if self.diagnostics.len() >= DEFAULT_DIAGNOSTIC_LIMIT {
            self.frozen = true;
            self.diagnostics.push(
                Diagnostic::error(
                    PARSE_TOO_MANY_ERRORS,
                    "parser",
                    "parser.too_many_errors",
                    format!(
                        "too many errors (limit of {} reached)",
                        DEFAULT_DIAGNOSTIC_LIMIT
                    ),
                )
                .with_primary_span(span),
            );
            return;
        }
        self.diagnostics
            .push(Diagnostic::error(code, "parser", key, message).with_primary_span(span));
    }

    fn report_expected(&mut self, expected: &str, found: String, span: SourceSpan) {
        self.report(
            PARSE_EXPECTED_TOKEN,
            span,
            "parser.expected_token",
            format!("expected {expected}, found {found}"),
        );
    }

    fn record_missing_token(&mut self, kind: TokenKind, span: SourceSpan) {
        self.cursor
            .cst
            .record_missing(token_kind_to_syntax(kind), span);
    }

    /// Heurística conservadora de recuperação: se o próximo token significativo
    /// é `}` (bloco), o delimitador aberto está simplesmente ausente — emite o
    /// erro de token esperado e para a iteração, em vez de engolir o conteúdo do
    /// bloco seguinte (§464-465: `function broken( -> Int {` não deve consumir o
    /// corpo de `function good()`).
    fn break_on_block_close(&mut self, expected: &str) -> bool {
        if self.cursor.peek_kind() == TokenKind::RightBrace {
            let span = self.cursor.peek_span();
            self.report_expected(expected, self.describe_current(), span);
            true
        } else {
            false
        }
    }

    /// A próxima declaração de topo aparece onde um `expected` (ex.: `}` ) ainda
    /// não foi visto — ex.: `struct`/`enum` sem fechamento antes do próximo
    /// `function`. Encerra o laço do corpo para não engolir a declaração (§465).
    fn break_on_item_start(&mut self, expected: &str) -> bool {
        let is_item_start = matches!(
            self.cursor.peek_kind(),
            TokenKind::Keyword(Keyword::Function)
                | TokenKind::Keyword(Keyword::Action)
                | TokenKind::Keyword(Keyword::Struct)
                | TokenKind::Keyword(Keyword::Enum)
                | TokenKind::Keyword(Keyword::Interface)
                | TokenKind::Keyword(Keyword::Implement)
                | TokenKind::Keyword(Keyword::Type)
                | TokenKind::Keyword(Keyword::Const)
                | TokenKind::Keyword(Keyword::Module)
                | TokenKind::Keyword(Keyword::Import)
        );
        if is_item_start {
            let span = self.cursor.peek_span();
            self.report_expected(expected, self.describe_current(), span);
            true
        } else {
            false
        }
    }

    fn expect_bump(&mut self, kind: TokenKind, what: &str) -> SourceSpan {
        if self.at(kind) {
            self.bump()
        } else {
            let span = self.cursor.peek_span();
            self.report_expected(what, self.describe_current(), span);
            self.record_missing_token(kind, span);
            span
        }
    }

    fn expect_ident(&mut self, what: &str) -> Ident {
        let start = self.cursor.peek_span();
        if self.at_ident() {
            let name = self
                .cursor
                .peek_identifier_text()
                .unwrap_or("_")
                .to_string();
            let end = self.bump();
            Ident {
                name,
                span: self.cover(start, end),
            }
        } else {
            let found = self.describe_current();
            self.report_expected(what, found, start);
            self.record_missing_token(TokenKind::Identifier, start);
            Ident {
                name: "_".to_string(),
                span: start,
            }
        }
    }

    /// Garante progresso do cursor após um statement: se nada foi consumido
    /// e não estamos em EOF, consome um token (recovery anti-loop).
    fn ensure_progress(&mut self, entry: SourceSpan) {
        if entry == self.cursor.peek_span() && !self.cursor.eof() {
            self.bump();
        }
    }

    /// Corpos com itens separados por newline (§50): struct fields, enum
    /// variants, interface/impl membros e match arms **não** usam `,`
    /// como separador. Um `,` órfão é erro, mas deve ser consumido para
    /// nunca travar o parser em loop.
    fn skip_stray_body_comma(&mut self, what: &str) -> bool {
        if self.at(TokenKind::Comma) {
            let sp = self.cursor.peek_span();
            self.report(
                PARSE_INVALID_DECLARATION,
                sp,
                "parser.stray_comma_in_body",
                format!("{what} are separated by new lines; `,` is not allowed here (§50)"),
            );
            self.bump();
            true
        } else {
            false
        }
    }

    // ------------------------------------------------------------------
    // recovery / limpeza de contexto
    // ------------------------------------------------------------------

    fn skip_soft(&mut self) {
        self.cursor.skip_soft();
    }

    fn sync_to_next_declaration(&mut self) {
        loop {
            self.skip_soft();
            match self.cursor.peek_kind() {
                TokenKind::Eof => return,
                TokenKind::Keyword(
                    Keyword::Module
                    | Keyword::Import
                    | Keyword::Export
                    | Keyword::Async
                    | Keyword::Function
                    | Keyword::Action
                    | Keyword::Struct
                    | Keyword::Enum
                    | Keyword::Interface
                    | Keyword::Implement
                    | Keyword::Type
                    | Keyword::Const,
                ) => return,
                _ => {
                    self.bump();
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // limite de profundidade (§341-347)
    // ------------------------------------------------------------------

    fn depth_exceeded(&mut self, depth: usize, span: SourceSpan) -> bool {
        if depth > DEFAULT_PARSE_DEPTH_LIMIT {
            self.report(
                PARSE_NESTING_LIMIT_EXCEEDED,
                span,
                "parser.nesting_limit_exceeded",
                format!(
                    "nesting limit exceeded (max {DEFAULT_PARSE_DEPTH_LIMIT}); input may be malformed"
                ),
            );
            true
        } else {
            false
        }
    }

    // ------------------------------------------------------------------
    // unidade de compilação: module / imports / items (§418-427)
    // ------------------------------------------------------------------

    fn parse_source_unit(&mut self) -> SourceUnit {
        let start = self.start_span();
        self.cursor.cst.start(SyntaxKind::ROOT);
        let mut module: Option<ModuleDeclaration> = None;
        let mut imports: Vec<ImportDeclaration> = Vec::new();
        let mut items: Vec<Item> = Vec::new();
        let mut saw_module = false;
        let mut seen_decl = false;
        // Uma declaração de topo (não-module, não-import) encerra a fase de
        // imports; um `import` posterior é inválido em qualquer modo (§417-427).
        let mut saw_declaration = false;

        loop {
            self.skip_soft();
            if self.cursor.eof() {
                break;
            }
            match self.cursor.peek_kind() {
                TokenKind::Keyword(Keyword::Module) => {
                    if saw_module {
                        let sp = self.cursor.peek_span();
                        self.report(
                            PARSE_DUPLICATE_MODULE,
                            sp,
                            "parser.duplicate_module",
                            "duplicate module declaration".to_string(),
                        );
                    }
                    saw_module = true;
                    seen_decl = true;
                    if module.is_none() {
                        module = Some(self.parse_module_decl());
                    } else {
                        let m = self.parse_module_decl();
                        let _ = m;
                    }
                }
                TokenKind::Keyword(Keyword::Import) => {
                    if self.mode == ParseMode::ProjectSource && !saw_module && !seen_decl {
                        let sp = self.cursor.peek_span();
                        self.report(
                            PARSE_INVALID_IMPORT,
                            sp,
                            "parser.import_before_module",
                            "import before the `module` declaration is not allowed in project sources"
                                .to_string(),
                        );
                    }
                    if saw_declaration {
                        // import after any top-level declaration — out of the
                        // import phase (§417-427)
                        let sp = self.cursor.peek_span();
                        self.report(
                            PARSE_INVALID_IMPORT,
                            sp,
                            "parser.import_after_declaration",
                            "import after a declaration is not allowed".to_string(),
                        );
                    }
                    imports.push(self.parse_import_decl());
                    seen_decl = true;
                }
                _ => {
                    if self.mode == ParseMode::ProjectSource && !saw_module && !seen_decl {
                        let sp = self.cursor.peek_span();
                        self.report(
                            PARSE_INVALID_DECLARATION,
                            sp,
                            "parser.missing_module",
                            "project source must begin with a `module` declaration".to_string(),
                        );
                    }
                    if self.mode == ParseMode::ProjectSource
                        && !saw_module
                        && !seen_decl
                        && imports.is_empty()
                    {
                        // caso em que o primeiro token já é uma declaração:
                        // o erro acima já foi emitido; parser segue.
                    }
                    items.push(self.parse_item());
                    seen_decl = true;
                    saw_declaration = true;
                }
            }
        }

        let end = self.cursor.peek_span();
        self.cursor.cst.finish(SyntaxKind::ROOT);
        SourceUnit {
            module,
            imports,
            items,
            span: self.cover(start, end),
        }
    }

    fn parse_module_decl(&mut self) -> ModuleDeclaration {
        let start = self.start_span();
        self.cursor.cst.start(SyntaxKind::MODULE_DECL);
        self.bump(); // `module`
        let name = self.parse_qualified_name();
        let span = self.cover(start, name.span);
        self.cursor.cst.finish(SyntaxKind::MODULE_DECL);
        let _ = self.expect_statement_end_if_newline();
        ModuleDeclaration { name, span }
    }

    fn parse_import_decl(&mut self) -> ImportDeclaration {
        let start = self.start_span();
        self.cursor.cst.start(SyntaxKind::IMPORT_DECL);
        self.bump(); // `import`
        self.cursor.cst.start(SyntaxKind::QUALIFIED_NAME);
        let mut segments: Vec<Ident> = Vec::new();
        let seg = self.expect_ident("identifier");
        let mut last_span = seg.span;
        segments.push(seg);
        loop {
            if self.at(TokenKind::DoubleColon) {
                self.bump();
                if self.at(TokenKind::Star) {
                    let star = self.bump();
                    last_span = star;
                    self.report(
                        PARSE_INVALID_IMPORT,
                        star,
                        "parser.wildcard_import",
                        "wildcard imports are not allowed".to_string(),
                    );
                    break;
                }
                let s = self.expect_ident("identifier");
                last_span = s.span;
                segments.push(s);
            } else {
                break;
            }
        }
        let path = QualifiedName {
            segments,
            span: self.cover(start, last_span),
        };
        self.cursor.cst.finish(SyntaxKind::QUALIFIED_NAME);
        let mut alias: Option<Ident> = None;
        if self.peek_ident_text() == Some("as") {
            self.cursor.cst.start(SyntaxKind::IMPORT_ALIAS);
            self.bump(); // `as`
            let a = self.expect_ident("alternative name");
            let _ = a.span;
            self.cursor.cst.finish(SyntaxKind::IMPORT_ALIAS);
            alias = Some(a);
        }
        let span = match &alias {
            Some(a) => self.cover(start, a.span),
            None => path.span,
        };
        self.cursor.cst.finish(SyntaxKind::IMPORT_DECL);
        let _ = self.expect_statement_end_if_newline();
        ImportDeclaration { path, alias, span }
    }

    fn parse_qualified_name(&mut self) -> QualifiedName {
        let start = self.start_span();
        self.cursor.cst.start(SyntaxKind::QUALIFIED_NAME);
        let mut segments: Vec<Ident> = Vec::new();
        let seg = self.expect_ident("identifier");
        let mut last_span = seg.span;
        segments.push(seg);
        while self.at(TokenKind::DoubleColon) {
            let cc = self.bump();
            last_span = cc;
            if self.at_ident() {
                let s = self.expect_ident("identifier");
                last_span = s.span;
                segments.push(s);
            } else {
                self.report_expected(
                    "identifier",
                    self.describe_current(),
                    self.cursor.peek_span(),
                );
                break;
            }
        }
        let span = self.cover(start, last_span);
        self.cursor.cst.finish(SyntaxKind::QUALIFIED_NAME);
        QualifiedName { segments, span }
    }

    // ------------------------------------------------------------------
    // itens de topo
    // ------------------------------------------------------------------

    fn parse_item(&mut self) -> Item {
        self.cursor.cst.start(SyntaxKind::TOP_LEVEL_DECL);
        let item_start = self.start_span();
        let attrs = self.parse_attributes();

        let mut exported = false;
        if self.at_keyword(Keyword::Export) {
            exported = true;
            self.bump();
        }
        let mut is_async = false;
        if self.at_keyword(Keyword::Async) {
            is_async = true;
            self.bump();
        }

        let keyword_start = self.start_span();
        let kind = match self.cursor.peek_kind() {
            TokenKind::Keyword(Keyword::Function) => {
                if is_async {
                    let sp = self.cursor.peek_span();
                    self.report(
                        PARSE_INVALID_CALLABLE_DECL,
                        sp,
                        "parser.async_function",
                        "`async` is only valid on actions, not functions".to_string(),
                    );
                }
                self.parse_function(keyword_start, exported)
            }
            TokenKind::Keyword(Keyword::Action) => {
                self.parse_action(keyword_start, exported, is_async)
            }
            TokenKind::Keyword(Keyword::Struct) => self.parse_struct(keyword_start),
            TokenKind::Keyword(Keyword::Enum) => self.parse_enum(keyword_start),
            TokenKind::Keyword(Keyword::Interface) => self.parse_interface(keyword_start),
            TokenKind::Keyword(Keyword::Implement) => self.parse_implement(keyword_start),
            TokenKind::Keyword(Keyword::Type) => self.parse_type_alias(keyword_start),
            TokenKind::Keyword(Keyword::Const) => self.parse_const_decl(keyword_start),
            TokenKind::Keyword(Keyword::Export) => {
                self.report(
                    PARSE_INVALID_MODIFIER_ORDER,
                    keyword_start,
                    "parser.invalid_modifier_order",
                    "invalid modifier order".to_string(),
                );
                self.bump();
                self.sync_to_next_declaration();
                ItemKind::Const(ConstDecl {
                    name: Ident {
                        name: "_".to_string(),
                        span: keyword_start,
                    },
                    ty: None,
                    init: self.sentinel_expr(keyword_start),
                    span: keyword_start,
                })
            }
            TokenKind::Keyword(Keyword::Let) | TokenKind::Keyword(Keyword::Var) => {
                self.report(
                    PARSE_INVALID_DECLARATION,
                    self.start_span(),
                    "parser.invalid_top_level_let",
                    "`let`/`var` are not allowed at module level (§427)".to_string(),
                );
                self.sync_to_next_declaration();
                ItemKind::Const(ConstDecl {
                    name: Ident {
                        name: "_".to_string(),
                        span: keyword_start,
                    },
                    ty: None,
                    init: self.sentinel_expr(keyword_start),
                    span: keyword_start,
                })
            }
            _ => {
                let sp = self.start_span();
                self.report(
                    PARSE_INVALID_DECLARATION,
                    sp,
                    "parser.invalid_top_level_expr",
                    format!("expected a declaration, found {}", self.describe_current()),
                );
                self.sync_to_next_declaration();
                ItemKind::Const(ConstDecl {
                    name: Ident {
                        name: "_".to_string(),
                        span: keyword_start,
                    },
                    ty: None,
                    init: self.sentinel_expr(keyword_start),
                    span: keyword_start,
                })
            }
        };
        let span = self.cover(item_start, kind_span(&kind));
        self.cursor.cst.finish(SyntaxKind::TOP_LEVEL_DECL);
        Item {
            attrs,
            exported,
            kind,
            span,
        }
    }

    fn parse_function(&mut self, start: SourceSpan, _exported: bool) -> ItemKind {
        self.cursor.cst.start(SyntaxKind::FUNCTION_DECL);
        self.bump(); // `function`
        let name = self.expect_ident("function name");
        let generic_params = self.parse_generic_params_if_any();
        let (_, params) = self.parse_param_list(false);
        let return_type = self.parse_return_type();
        if self.at_keyword(Keyword::Effects) {
            let sp = self.cursor.peek_span();
            self.report(
                PARSE_INVALID_CALLABLE_DECL,
                sp,
                "parser.effects_on_function",
                "`effects` is only valid on actions, not functions".to_string(),
            );
            let _ = self.parse_effects_clause();
        }
        let where_clause = self.parse_optional_where_clause();
        let (requires, ensures) = self.parse_contracts();
        let body = self.parse_block();
        let span = self.cover(start, body.span);
        if return_type.is_none() {
            let sp = name.span;
            self.report_expected("`->` return type", self.describe_current(), sp);
        }
        self.cursor.cst.finish(SyntaxKind::FUNCTION_DECL);
        ItemKind::Function(FunctionDecl {
            name,
            generic_params,
            params,
            return_type,
            where_clause,
            requires,
            ensures,
            body,
            span,
        })
    }

    fn parse_action(&mut self, start: SourceSpan, _exported: bool, is_async: bool) -> ItemKind {
        self.cursor.cst.start(SyntaxKind::ACTION_DECL);
        self.bump(); // `action`
        let name = self.expect_ident("action name");
        let generic_params = self.parse_generic_params_if_any();
        let (_, params) = self.parse_param_list(false);
        let return_type = self.parse_return_type();
        let where_clause = self.parse_optional_where_clause();
        let effects = if self.at_keyword(Keyword::Effects) {
            Some(self.parse_effects_clause())
        } else {
            None
        };
        let (requires, ensures) = self.parse_contracts();
        let body = self.parse_block();
        let span = self.cover(start, body.span);
        if return_type.is_none() {
            let sp = name.span;
            self.report_expected("`->` return type", self.describe_current(), sp);
        }
        self.cursor.cst.finish(SyntaxKind::ACTION_DECL);
        ItemKind::Action(ActionDecl {
            name,
            generic_params,
            params,
            return_type,
            is_async,
            effects,
            where_clause,
            requires,
            ensures,
            body,
            span,
        })
    }

    fn parse_struct(&mut self, start: SourceSpan) -> ItemKind {
        self.cursor.cst.start(SyntaxKind::STRUCT_DECL);
        self.bump(); // `struct`
        let name = self.expect_ident("struct name");
        let generic_params = self.parse_generic_params_if_any();
        self.expect_bump(TokenKind::LeftBrace, "'{'");
        let mut fields = Vec::new();
        loop {
            self.skip_soft();
            if self.at(TokenKind::RightBrace) {
                self.bump();
                break;
            }
            if self.cursor.eof() {
                self.report_expected("'}'", self.describe_current(), self.cursor.peek_span());
                break;
            }
            if self.break_on_item_start("'}'") {
                break;
            }
            if self.skip_stray_body_comma("struct fields") {
                continue;
            }
            let entry = self.start_span();
            fields.push(self.parse_struct_field());
            self.ensure_progress(entry);
        }
        let span: SourceSpan = fields
            .last()
            .map(|f| self.cover(start, f.span))
            .unwrap_or(self.cover(start, name.span));
        self.cursor.cst.finish(SyntaxKind::STRUCT_DECL);
        ItemKind::Struct(StructDecl {
            name,
            generic_params,
            fields,
            span,
        })
    }

    fn parse_struct_field(&mut self) -> StructField {
        let start = self.start_span();
        self.cursor.cst.start(SyntaxKind::STRUCT_FIELD);
        let mut exported = false;
        if self.at_keyword(Keyword::Export) {
            exported = true;
            self.bump();
        }
        let name = self.expect_ident("field name");
        self.expect_bump(TokenKind::Colon, "':'");
        let ty = self.parse_type();
        let span = self.cover(start, ty.span);
        self._field_end_if_newline();
        self.cursor.cst.finish(SyntaxKind::STRUCT_FIELD);
        StructField {
            name,
            ty,
            exported,
            span,
        }
    }

    fn parse_enum(&mut self, start: SourceSpan) -> ItemKind {
        self.cursor.cst.start(SyntaxKind::ENUM_DECL);
        self.bump(); // `enum`
        let name = self.expect_ident("enum name");
        let generic_params = self.parse_generic_params_if_any();
        self.expect_bump(TokenKind::LeftBrace, "'{'");
        let mut variants = Vec::new();
        loop {
            self.skip_soft();
            if self.at(TokenKind::RightBrace) {
                self.bump();
                break;
            }
            if self.cursor.eof() {
                self.report_expected("'}'", self.describe_current(), self.cursor.peek_span());
                break;
            }
            if self.break_on_item_start("'}'") {
                break;
            }
            if self.skip_stray_body_comma("enum variants") {
                continue;
            }
            let entry = self.start_span();
            variants.push(self.parse_enum_variant());
            self.ensure_progress(entry);
        }
        let span: SourceSpan = variants
            .last()
            .map(|v| self.cover(start, v.span))
            .unwrap_or(self.cover(start, name.span));
        self.cursor.cst.finish(SyntaxKind::ENUM_DECL);
        ItemKind::Enum(EnumDecl {
            name,
            generic_params,
            variants,
            span,
        })
    }

    fn parse_enum_variant(&mut self) -> EnumVariant {
        let start = self.start_span();
        if self.at_keyword(Keyword::Export) {
            let sp = self.cursor.peek_span();
            self.report(
                PARSE_INVALID_DECLARATION,
                sp,
                "parser.export_on_variant",
                "enum variants cannot be individually exported (§57-58)".to_string(),
            );
            self.bump();
        }
        let name = self.expect_ident("variant name");
        self.cursor.cst.start(SyntaxKind::ENUM_VARIANT);
        let kind = if self.at(TokenKind::LeftParen) {
            self.cursor.cst.finish(SyntaxKind::ENUM_VARIANT);
            let payload = self.parse_tuple_types_closure();
            let span = self.cover(start, last_type_span(&payload, name.span));
            self.cursor.cst.start(SyntaxKind::ENUM_VARIANT_TUPLE);
            let _ = self.cover(span, span);
            self.cursor.cst.finish(SyntaxKind::ENUM_VARIANT_TUPLE);
            EnumVariant {
                name,
                kind: EnumVariantKind::Tuple(payload),
                span,
            }
        } else if self.at(TokenKind::LeftBrace) {
            self.cursor.cst.finish(SyntaxKind::ENUM_VARIANT);
            let fields = self.parse_struct_pattern_fields_as_types();
            let span = self.cover(start, fields_types_span(&fields, name.span));
            self.cursor.cst.start(SyntaxKind::ENUM_VARIANT_STRUCT);
            let _ = fields_types_span(&fields, name.span);
            self.cursor.cst.finish(SyntaxKind::ENUM_VARIANT_STRUCT);
            EnumVariant {
                name,
                kind: EnumVariantKind::Struct(fields),
                span,
            }
        } else {
            let span = self.cover(start, name.span);
            self.cursor.cst.finish(SyntaxKind::ENUM_VARIANT);
            EnumVariant {
                name,
                kind: EnumVariantKind::Unit,
                span,
            }
        };
        self._field_end_if_newline();
        kind
    }

    /// Suporte: payloads `(T, U)` de variants (comma-separated).
    fn parse_tuple_types_closure(&mut self) -> Vec<Type> {
        let mut types = Vec::new();
        self.delim_depth += 1;
        self.expect_bump(TokenKind::LeftParen, "'('");
        loop {
            self.skip_soft();
            if self.at(TokenKind::RightParen) {
                self.bump();
                break;
            }
            if self.cursor.eof() {
                self.report_expected("')'", self.describe_current(), self.cursor.peek_span());
                break;
            }
            let t = self.parse_type();
            types.push(t);
            self.skip_soft();
            if self.at(TokenKind::Comma) {
                self.bump();
                continue;
            }
            if self.at(TokenKind::RightParen) {
                self.bump();
                break;
            }
            self.report_expected(
                "',' or ')'",
                self.describe_current(),
                self.cursor.peek_span(),
            );
            break;
        }
        self.delim_depth -= 1;
        types
    }

    /// Suporte: payloads `{ name: T }` de variants struct-like.
    fn parse_struct_pattern_fields_as_types(&mut self) -> Vec<StructField> {
        let mut fields = Vec::new();
        self.delim_depth += 1;
        self.expect_bump(TokenKind::LeftBrace, "'{'");
        loop {
            self.skip_soft();
            if self.at(TokenKind::RightBrace) {
                self.bump();
                break;
            }
            if self.cursor.eof() {
                self.report_expected("'}'", self.describe_current(), self.cursor.peek_span());
                break;
            }
            let fstart = self.start_span();
            self.cursor.cst.start(SyntaxKind::STRUCT_FIELD);
            let mut exported = false;
            if self.at_keyword(Keyword::Export) {
                exported = true;
                self.bump();
            }
            let name = self.expect_ident("field name");
            self.expect_bump(TokenKind::Colon, "':'");
            let ty = self.parse_type();
            let span = self.cover(fstart, ty.span);
            self.cursor.cst.finish(SyntaxKind::STRUCT_FIELD);
            fields.push(StructField {
                name,
                ty,
                exported,
                span,
            });
            self.skip_soft();
            if self.at(TokenKind::Comma) {
                self.bump();
                continue;
            }
            if self.at(TokenKind::RightBrace) {
                self.bump();
                break;
            }
            self.report_expected(
                "',' or '}'",
                self.describe_current(),
                self.cursor.peek_span(),
            );
            break;
        }
        self.delim_depth -= 1;
        fields
    }

    fn parse_interface(&mut self, start: SourceSpan) -> ItemKind {
        self.cursor.cst.start(SyntaxKind::INTERFACE_DECL);
        self.bump(); // `interface`
        let name = self.expect_ident("interface name");
        let generic_params = self.parse_generic_params_if_any();
        self.expect_bump(TokenKind::LeftBrace, "'{'");
        let mut methods = Vec::new();
        loop {
            self.skip_soft();
            if self.at(TokenKind::RightBrace) {
                self.bump();
                break;
            }
            if self.cursor.eof() {
                self.report_expected("'}'", self.describe_current(), self.cursor.peek_span());
                break;
            }
            if self.skip_stray_body_comma("interface methods") {
                continue;
            }
            let entry = self.start_span();
            methods.push(self.parse_interface_method());
            self.ensure_progress(entry);
        }
        let span = methods
            .last()
            .map(|m| self.cover(start, m.span))
            .unwrap_or(self.cover(start, name.span));
        self.cursor.cst.finish(SyntaxKind::INTERFACE_DECL);
        ItemKind::Interface(InterfaceDecl {
            name,
            generic_params,
            methods,
            span,
        })
    }

    fn parse_interface_method(&mut self) -> InterfaceMethod {
        let start = self.start_span();
        self.cursor.cst.start(SyntaxKind::INTERFACE_METHOD);
        if self.at_keyword(Keyword::Export) {
            self.bump();
        }
        let name = self.expect_ident("method name");
        let (receiver, params) = self.parse_param_list(true);
        let return_type = self.parse_return_type();
        let effects = if self.at_keyword(Keyword::Effects) {
            Some(self.parse_effects_clause())
        } else {
            None
        };
        let (requires, ensures) = self.parse_contracts();
        let span = match &return_type {
            Some(t) => self.cover(start, t.span),
            None => self.cover(start, name.span),
        };
        if self.at(TokenKind::LeftBrace) {
            let sp = self.cursor.peek_span();
            self.report(
                PARSE_INVALID_CALLABLE_DECL,
                sp,
                "parser.interface_member_body",
                "interface members cannot have a body (§436)".to_string(),
            );
            let _ = self.parse_block();
        }
        self.cursor.cst.finish(SyntaxKind::INTERFACE_METHOD);
        let _ = receiver;
        self._field_end_if_newline();
        InterfaceMethod {
            name,
            receiver,
            params,
            return_type,
            effects,
            requires,
            ensures,
            span,
        }
    }

    fn parse_implement(&mut self, start: SourceSpan) -> ItemKind {
        self.cursor.cst.start(SyntaxKind::IMPL_DECL);
        self.bump(); // `implement`
                     // Head: `Interface for Type` ou `Type`.
        let trait_path = self.parse_qualified_name();
        let for_type: Type = if self.at_keyword(Keyword::For) {
            let fstart = self.start_span();
            self.bump(); // `for`
            let mut t = self.parse_hang_type();
            t.span = self.cover(fstart, t.span);
            t
        } else {
            let sp = trait_path.span;
            Type {
                kind: TypeKind::Path(trait_path.clone()),
                span: sp,
            }
        };
        self.expect_bump(TokenKind::LeftBrace, "'{'");
        let mut methods = Vec::new();
        loop {
            self.skip_soft();
            if self.at(TokenKind::RightBrace) {
                self.bump();
                break;
            }
            if self.cursor.eof() {
                self.report_expected("'}'", self.describe_current(), self.cursor.peek_span());
                break;
            }
            if self.skip_stray_body_comma("implementation members") {
                continue;
            }
            let entry = self.start_span();
            methods.push(self.parse_impl_member());
            self.ensure_progress(entry);
        }
        let span = methods
            .last()
            .map(|m| self.cover(start, m.span))
            .unwrap_or(self.cover(start, trait_path.span));
        self.cursor.cst.finish(SyntaxKind::IMPL_DECL);
        ItemKind::Implement(ImplDecl {
            trait_path,
            for_type,
            generic_params: Vec::new(),
            methods,
            span,
        })
    }

    fn parse_impl_member(&mut self) -> ImplMethod {
        let start = self.start_span();
        self.cursor.cst.start(SyntaxKind::INTERFACE_METHOD);
        let mut exported = false;
        if self.at_keyword(Keyword::Export) {
            exported = true;
            self.bump();
        }
        // Métodos do implement podem ser `function`/`action` explícitos ou
        // forma abreviada (`name(ref self, ...)`).
        let is_async = if self.at_keyword(Keyword::Async) {
            self.bump();
            true
        } else {
            false
        };
        let explicit_callable = matches!(
            self.cursor.peek_kind(),
            TokenKind::Keyword(Keyword::Function) | TokenKind::Keyword(Keyword::Action)
        );
        if explicit_callable {
            self.bump();
        }
        let name = self.expect_ident("member name");
        let (receiver, params) = self.parse_param_list(true);
        let return_type = self.parse_return_type();
        let effects = if self.at_keyword(Keyword::Effects) {
            Some(self.parse_effects_clause())
        } else {
            None
        };
        let mut body: Option<Block> = None;
        if self.at(TokenKind::LeftBrace) {
            body = Some(self.parse_block());
        } else {
            let sp = self.cursor.peek_span();
            self.report(
                PARSE_INVALID_CALLABLE_DECL,
                sp,
                "parser.implement_member_body",
                "implement members require a body (§437)".to_string(),
            );
        }
        let span = match &body {
            Some(b) => self.cover(start, b.span),
            None => self.cover(start, name.span),
        };
        self.cursor.cst.finish(SyntaxKind::INTERFACE_METHOD);
        self._end_if_newline();
        let _ = (exported, is_async);
        ImplMethod {
            name,
            receiver,
            params,
            return_type,
            effects,
            body,
            span,
        }
    }

    fn parse_type_alias(&mut self, start: SourceSpan) -> ItemKind {
        self.cursor.cst.start(SyntaxKind::TYPE_ALIAS);
        self.bump(); // `type`
        let name = self.expect_ident("type name");
        let generic_params = self.parse_generic_params_if_any();
        let is_distinct = self.peek_ident_text() == Some("distinct");
        if is_distinct {
            self.bump();
        }
        self.expect_bump(TokenKind::Equal, "'='");
        let ty = self.parse_type();
        let span = self.cover(start, ty.span);
        self.cursor.cst.finish(SyntaxKind::TYPE_ALIAS);
        self._end_if_newline();
        ItemKind::TypeAlias(TypeAliasDecl {
            name,
            generic_params,
            is_distinct,
            ty,
            span,
        })
    }

    fn parse_const_decl(&mut self, start: SourceSpan) -> ItemKind {
        self.cursor.cst.start(SyntaxKind::CONST_DECL);
        self.bump(); // `const`
        let name = self.expect_ident("constant name");
        let mut ty = None;
        if self.at(TokenKind::Colon) {
            self.bump();
            ty = Some(self.parse_type());
        }
        let init = if self.at(TokenKind::Equal) {
            self.bump();
            self.skip_soft();
            self.parse_expression()
        } else {
            let sp = self.start_span();
            self.report_expected("'='", self.describe_current(), sp);
            self.record_missing_token(TokenKind::Equal, sp);
            self.sentinel_expr(sp)
        };
        let span = self.cover(start, init.span);
        self.cursor.cst.finish(SyntaxKind::CONST_DECL);
        self._end_if_newline();
        ItemKind::Const(ConstDecl {
            name,
            ty,
            init,
            span,
        })
    }

    // ------------------------------------------------------------------
    // atributos (§293-307)
    // ------------------------------------------------------------------

    fn parse_attributes(&mut self) -> Vec<Attribute> {
        let mut attrs = Vec::new();
        loop {
            self.skip_soft();
            if self.at(TokenKind::At) {
                attrs.push(self.parse_attribute());
            } else {
                break;
            }
        }
        attrs
    }

    fn parse_attribute(&mut self) -> Attribute {
        let start = self.start_span();
        self.cursor.cst.start(SyntaxKind::ATTR);
        self.bump(); // `@`
        let name = self.parse_qualified_name();
        let mut args = Vec::new();
        if self.at(TokenKind::LeftParen) {
            self.cursor.cst.start(SyntaxKind::ATTR_ARGS);
            self.delim_depth += 1;
            self.bump(); // `(`
            loop {
                self.skip_soft();
                if self.at(TokenKind::RightParen) {
                    self.bump();
                    break;
                }
                if self.cursor.eof() {
                    self.report_expected("')'", self.describe_current(), self.cursor.peek_span());
                    break;
                }
                if self.break_on_block_close("')'") {
                    break;
                }
                let e = self.parse_expression();
                args.push(e);
                self.skip_soft();
                if self.at(TokenKind::Comma) {
                    self.bump();
                    continue;
                }
                if self.at(TokenKind::RightParen) {
                    self.bump();
                    break;
                }
                if self.break_on_block_close("')'") {
                    break;
                }
                self.report_expected(
                    "',' or ')'",
                    self.describe_current(),
                    self.cursor.peek_span(),
                );
                break;
            }
            self.delim_depth -= 1;
            self.cursor.cst.finish(SyntaxKind::ATTR_ARGS);
        }
        let span = self.cover(start, args.last().map(|a| a.span).unwrap_or(name.span));
        self.cursor.cst.finish(SyntaxKind::ATTR);
        Attribute { name, args, span }
    }

    // ------------------------------------------------------------------
    // generics / parametros / receiver (§60-64, §98-103, §289-290, §448-450)
    // ------------------------------------------------------------------

    fn parse_generic_params_if_any(&mut self) -> Vec<GenericParam> {
        if self.at(TokenKind::Less) {
            self.parse_generic_params()
        } else {
            Vec::new()
        }
    }

    fn parse_generic_params(&mut self) -> Vec<GenericParam> {
        let mut params = Vec::new();
        self.cursor.cst.start(SyntaxKind::GENERIC_PARAMS);
        self.expect_bump(TokenKind::Less, "'<'");
        loop {
            self.skip_soft();
            if self.at(TokenKind::Greater) {
                self.bump();
                break;
            }
            if self.cursor.eof() {
                self.report_expected("'>'", self.describe_current(), self.cursor.peek_span());
                break;
            }
            if self.break_on_block_close("'>'") {
                break;
            }
            if !self.at_ident() {
                let sp = self.start_span();
                self.report(
                    PARSE_INVALID_DECLARATION,
                    sp,
                    "parser.invalid_generic_param",
                    format!(
                        "generic parameters must be identifiers, found {}",
                        self.describe_current()
                    ),
                );
                self.bump();
                continue;
            }
            let gstart = self.start_span();
            self.cursor.cst.start(SyntaxKind::GENERIC_PARAM);
            let name = self.expect_ident("generic parameter name");
            let span = self.cover(gstart, name.span);
            self.cursor.cst.finish(SyntaxKind::GENERIC_PARAM);
            params.push(GenericParam {
                name,
                bounds: Vec::new(),
                span,
            });
            self.skip_soft();
            if self.at(TokenKind::Comma) {
                self.bump();
                continue;
            }
            if self.at(TokenKind::Greater) {
                self.bump();
                break;
            }
            if self.break_on_block_close("'>'") {
                break;
            }
            self.report_expected(
                "',' or '>'",
                self.describe_current(),
                self.cursor.peek_span(),
            );
            break;
        }
        self.cursor.cst.finish(SyntaxKind::GENERIC_PARAMS);
        params
    }

    /// Lista de parâmetros `(name: Type, ...)`. `allow_receiver` habilita o
    /// receiver `self`/`ref self`/`ref mut self` na primeira posição
    /// (interface/implement, §98-103). Retorna `(receiver, params)`.
    fn parse_param_list(&mut self, allow_receiver: bool) -> (ReceiverKind, Vec<Param>) {
        let mut receiver = ReceiverKind::Self_;
        let mut params = Vec::new();
        self.cursor.cst.start(SyntaxKind::PARAM_LIST);
        self.expect_bump(TokenKind::LeftParen, "'('");
        loop {
            self.skip_soft();
            if self.at(TokenKind::RightParen) {
                self.bump();
                break;
            }
            if self.cursor.eof() {
                self.report_expected("')'", self.describe_current(), self.cursor.peek_span());
                break;
            }
            if self.break_on_block_close("')'") {
                break;
            }
            if params.is_empty() && allow_receiver {
                if let Some(kind) = self.try_receiver() {
                    receiver = kind;
                    self.consume_receiver();
                    // Receiver não tem `: Type`; parâmetros seguintes
                    // exigem o separador `,` (`area(self, factor: Int)`).
                    self.skip_soft();
                    if self.at(TokenKind::Comma) {
                        self.bump();
                    }
                    continue;
                }
            }
            let p = self.parse_param();
            params.push(p);
            self.skip_soft();
            if self.at(TokenKind::Comma) {
                self.bump();
                continue;
            }
            if self.at(TokenKind::RightParen) {
                self.bump();
                break;
            }
            if self.break_on_block_close("')'") {
                break;
            }
            if self.cursor.eof() {
                self.report_expected("')'", self.describe_current(), self.cursor.peek_span());
                break;
            }
            self.report_expected(
                "',' or ')'",
                self.describe_current(),
                self.cursor.peek_span(),
            );
            self.bump();
        }
        self.cursor.cst.finish(SyntaxKind::PARAM_LIST);
        (receiver, params)
    }

    fn try_receiver(&self) -> Option<ReceiverKind> {
        if self.peek_ident_text() == Some("self") {
            return Some(ReceiverKind::Self_);
        }
        if self.at_keyword(Keyword::Ref) {
            if self.cursor.peek_kind_at(1) == TokenKind::Keyword(Keyword::Mut)
                && self.cursor.peek_identifier_text_at(2) == Some("self")
            {
                return Some(ReceiverKind::RefMutSelf);
            }
            if self.cursor.peek_identifier_text_at(1) == Some("self") {
                return Some(ReceiverKind::RefSelf);
            }
        }
        None
    }

    fn consume_receiver(&mut self) {
        if self.peek_ident_text() == Some("self") {
            self.bump();
            return;
        }
        if self.at_keyword(Keyword::Ref) {
            self.bump();
            if self.at_keyword(Keyword::Mut) {
                self.bump();
            }
            if self.peek_ident_text() == Some("self") {
                self.bump();
            } else {
                self.report_expected("`self`", self.describe_current(), self.start_span());
            }
        }
    }

    fn parse_param(&mut self) -> Param {
        let start = self.start_span();
        self.cursor.cst.start(SyntaxKind::PARAM);
        let name = self.expect_ident("parameter name");
        self.expect_bump(TokenKind::Colon, "':'");
        let ty = self.parse_type();
        let span = self.cover(start, ty.span);
        self.cursor.cst.finish(SyntaxKind::PARAM);
        Param {
            name,
            ty,
            mutable: false,
            span,
        }
    }

    fn parse_return_type(&mut self) -> Option<Type> {
        if self.at(TokenKind::Arrow) {
            self.cursor.cst.start(SyntaxKind::RETURN_TYPE);
            let arrow = self.bump();
            self.skip_soft();
            let ty = self.parse_type();
            let span = self.cover(arrow, ty.span);
            self.cursor.cst.finish(SyntaxKind::RETURN_TYPE);
            let mut t = ty;
            t.span = span;
            Some(t)
        } else {
            let sp = self.start_span();
            self.report(
                PARSE_INVALID_CALLABLE_DECL,
                sp,
                "parser.missing_return_type",
                "named callables require an explicit `->` return type (§370-372)".to_string(),
            );
            None
        }
    }

    // ------------------------------------------------------------------
    // where / effects / require-ensure (§273-288)
    // ------------------------------------------------------------------

    fn parse_optional_where_clause(&mut self) -> Vec<WherePredicate> {
        if self.cursor.peek_identifier_text_skipping_newlines() == Some("where") {
            self.skip_soft();
            self.parse_where_clause()
        } else {
            Vec::new()
        }
    }

    fn parse_where_clause(&mut self) -> Vec<WherePredicate> {
        let mut preds = Vec::new();
        self.cursor.cst.start(SyntaxKind::WHERE_CLAUSE);
        self.bump(); // `where`
        loop {
            let p = self.parse_where_predicate();
            preds.push(p);
            self.skip_soft();
            if self.at(TokenKind::Comma) {
                self.bump();
                continue;
            }
            break;
        }
        self.cursor.cst.finish(SyntaxKind::WHERE_CLAUSE);
        preds
    }

    fn parse_where_predicate(&mut self) -> WherePredicate {
        let start = self.start_span();
        self.cursor.cst.start(SyntaxKind::WHERE_PREDICATE);
        let type_name = self.expect_ident("type parameter");
        self.expect_bump(TokenKind::Colon, "':'");
        let mut bound = self.parse_type_bound();
        let span = self.cover(start, bound.span);
        if self.at(TokenKind::Plus) {
            let sp = self.cursor.peek_span();
            self.report(
                PARSE_INVALID_DECLARATION,
                sp,
                "parser.multiple_bounds_unsupported",
                "multiple bounds (`+`) are not supported yet".to_string(),
            );
        }
        bound.span = span;
        let span = self.cover(start, self.start_span());
        self.cursor.cst.finish(SyntaxKind::WHERE_PREDICATE);
        WherePredicate {
            type_name,
            bounds: vec![bound],
            span,
        }
    }

    fn parse_type_bound(&mut self) -> TypeBound {
        let start = self.start_span();
        let path = self.parse_qualified_name();
        let mut generic_args = Vec::new();
        if self.at(TokenKind::Less) {
            generic_args = self.parse_generic_arg_list(0);
        }
        let span = self.cover(
            start,
            generic_args.last().map(|t| t.span).unwrap_or(path.span),
        );
        TypeBound {
            path,
            generic_args,
            span,
        }
    }

    fn parse_effects_clause(&mut self) -> EffectsClause {
        let start = self.start_span();
        self.cursor.cst.start(SyntaxKind::EFFECTS_CLAUSE);
        self.bump(); // `effects`
        self.cursor.cst.start(SyntaxKind::EFFECT_LIST);
        self.expect_bump(TokenKind::LeftBracket, "'['");
        let mut effects = Vec::new();
        loop {
            self.skip_soft();
            if self.at(TokenKind::RightBracket) {
                self.bump();
                break;
            }
            if self.cursor.eof() {
                self.report_expected("']'", self.describe_current(), self.cursor.peek_span());
                break;
            }
            if self.break_on_block_close("']'") {
                break;
            }
            let e = self.parse_effect_path();
            effects.push(e);
            self.skip_soft();
            if self.at(TokenKind::Comma) {
                self.bump();
                continue;
            }
            if self.at(TokenKind::RightBracket) {
                self.bump();
                break;
            }
            if self.cursor.eof() {
                self.report_expected("']'", self.describe_current(), self.cursor.peek_span());
                break;
            }
            // nova linha separando efeitos (sem vírgula)
            if !self.at_ident() && !self.at(TokenKind::DoubleColon) {
                break;
            }
        }
        self.cursor.cst.finish(SyntaxKind::EFFECT_LIST);
        let span = self.cover(
            start,
            effects.last().map(|e| e.span).unwrap_or(self.start_span()),
        );
        self.cursor.cst.finish(SyntaxKind::EFFECTS_CLAUSE);
        EffectsClause { effects, span }
    }

    fn parse_effect_path(&mut self) -> EffectPath {
        self.cursor.cst.start(SyntaxKind::EFFECT_PATH);
        let path = self.parse_qualified_name();
        let span = path.span;
        self.cursor.cst.finish(SyntaxKind::EFFECT_PATH);
        EffectPath { path, span }
    }

    fn parse_contracts(&mut self) -> (Vec<ContractExpr>, Vec<ContractExpr>) {
        let mut requires = Vec::new();
        let mut ensures = Vec::new();
        loop {
            // Não queima o newline de separação quando não há clause: em
            // métodos de interface o newline separa o método seguinte (§463)
            // e `parse_block` cuida do corpo em linha própria.
            let next = self.cursor.peek_kind_skipping_newlines();
            if next != TokenKind::Keyword(Keyword::Require)
                && next != TokenKind::Keyword(Keyword::Ensure)
            {
                break;
            }
            self.skip_soft();
            if self.at_keyword(Keyword::Require) {
                self.cursor.cst.start(SyntaxKind::REQUIRE_CLAUSE);
                let c = self.parse_contract_expr(ContractKind::Require);
                self.cursor.cst.finish(SyntaxKind::REQUIRE_CLAUSE);
                requires.push(c);
            } else if self.at_keyword(Keyword::Ensure) {
                self.cursor.cst.start(SyntaxKind::ENSURE_CLAUSE);
                let c = self.parse_contract_expr(ContractKind::Ensure);
                self.cursor.cst.finish(SyntaxKind::ENSURE_CLAUSE);
                ensures.push(c);
            } else {
                break;
            }
        }
        (requires, ensures)
    }

    fn parse_contract_expr(&mut self, kind: ContractKind) -> ContractExpr {
        let start = self.start_span();
        self.bump(); // `require`/`ensure`
        self.skip_soft();
        let expr = self.parse_expression();
        let span = self.cover(start, expr.span);
        ContractExpr { kind, expr, span }
    }

    // ------------------------------------------------------------------
    // terminadores de linha (§131-146, §384-386)
    // ------------------------------------------------------------------

    fn expect_statement_end(&mut self) {
        let broke = self.cursor.skip_trivia();
        if broke {
            return;
        }
        match self.cursor.peek_kind() {
            TokenKind::Newline | TokenKind::RightBrace | TokenKind::Eof => {}
            _ => {
                let sp = self.cursor.peek_span();
                self.report(
                    PARSE_MISSING_STATEMENT_TERMINATOR,
                    sp,
                    "parser.missing_statement_terminator",
                    format!("expected a new line, found {}", self.describe_current()),
                );
            }
        }
    }

    /// Termina declaração/linha sem exigir (útil p/ linhas de declarações).
    fn expect_statement_end_if_newline(&mut self) -> Option<()> {
        self.expect_statement_end();
        None
    }

    fn _field_end_if_newline(&mut self) {
        self.expect_statement_end();
    }

    fn _end_if_newline(&mut self) {
        self.expect_statement_end();
    }

    // ------------------------------------------------------------------
    // sentinels
    // ------------------------------------------------------------------

    fn sentinel_expr(&mut self, span: SourceSpan) -> Expr {
        Expr {
            kind: ExprKind::Underscore,
            span,
        }
    }

    // ------------------------------------------------------------------
    // tipos (§104-113, §358-362)
    // ------------------------------------------------------------------

    fn parse_type(&mut self) -> Type {
        self.parse_type_d(0)
    }

    fn parse_hang_type(&mut self) -> Type {
        // igual a parse_type mas sem o report de "unexpected" no head quando
        // o primeiro token indica um tipo genérico `implement Foo for Type`.
        self.parse_type()
    }

    fn parse_type_d(&mut self, depth: usize) -> Type {
        let start = self.start_span();
        if self.depth_exceeded(depth, start) {
            return Type {
                kind: TypeKind::Unit,
                span: start,
            };
        }
        match self.cursor.peek_kind() {
            TokenKind::Identifier if self.peek_ident_text() == Some("Unit") => {
                // Tipo `Unit` é contextual (lexado como Identifier) (§366-368).
                self.cursor.cst.start(SyntaxKind::PATH_TYPE);
                self.bump();
                let span = self.cover(start, self.start_span());
                self.cursor.cst.finish(SyntaxKind::PATH_TYPE);
                Type {
                    kind: TypeKind::Unit,
                    span,
                }
            }
            TokenKind::Keyword(Keyword::Ref) => self.parse_ref_type(depth, start),
            TokenKind::Keyword(Keyword::Mut) => {
                // `mut T` sem `ref` não é um tipo válido; report e segue.
                self.report(
                    PARSE_INVALID_EXPRESSION,
                    start,
                    "parser.invalid_type",
                    "`mut` type requires `ref` (`ref mut T`)".to_string(),
                );
                self.bump();
                self.parse_type_d(depth + 1)
            }
            TokenKind::Identifier => self.parse_path_type(depth, start),
            TokenKind::DoubleColon => self.parse_path_type(depth, start),
            TokenKind::LeftParen => self.parse_tuple_or_paren_type(depth, start),
            _ => {
                self.report(
                    PARSE_INVALID_EXPRESSION,
                    start,
                    "parser.invalid_type",
                    format!("expected a type, found {}", self.describe_current()),
                );
                self.record_missing_token(TokenKind::Identifier, start);
                Type {
                    kind: TypeKind::Unit,
                    span: start,
                }
            }
        }
    }

    /// `()` type é Unit; `(T)` é agrupamento. Tuplas `(T, U, ...)` são
    /// excluídas do Core 1.0 (§798/§347): o parser rejeita com erro e
    /// recupera consumindo até o `)` (CTS-PARSE-0116).
    fn parse_tuple_or_paren_type(&mut self, depth: usize, start: SourceSpan) -> Type {
        self.cursor.cst.start(SyntaxKind::TUPLE_TYPE);
        self.bump(); // '('
        if self.at(TokenKind::RightParen) {
            self.bump(); // ')'
            let span = self.cover(start, self.start_span());
            self.cursor.cst.finish(SyntaxKind::TUPLE_TYPE);
            return Type {
                kind: TypeKind::Unit,
                span,
            };
        }
        let first = self.parse_type_d(depth + 1);
        if self.at(TokenKind::Comma) {
            let sp = self.cursor.peek_span();
            self.report(
                PARSE_INVALID_EXPRESSION,
                sp,
                "parser.tuple_type",
                "general tuple types are not supported in NEXA 1.0 (`()` is Unit only) (§347/§798)"
                    .to_string(),
            );
            while self.at(TokenKind::Comma) {
                self.bump();
                if self.at(TokenKind::RightParen) {
                    break;
                }
                let _ = self.parse_type_d(depth + 1);
            }
            self.expect_bump(TokenKind::RightParen, "')'");
            let span = self.cover(start, self.start_span());
            self.cursor.cst.finish(SyntaxKind::TUPLE_TYPE);
            Type {
                kind: TypeKind::Unit,
                span,
            }
        } else {
            // Parêntesis de agrupamento: o tipo interno é o resultado.
            self.expect_bump(TokenKind::RightParen, "')'");
            let span = self.cover(start, self.start_span());
            self.cursor.cst.finish(SyntaxKind::TUPLE_TYPE);
            Type {
                kind: first.kind,
                span,
            }
        }
    }

    fn parse_ref_type(&mut self, depth: usize, start: SourceSpan) -> Type {
        self.cursor.cst.start(SyntaxKind::REF_TYPE);
        self.bump(); // `ref`
        if self.at_keyword(Keyword::Mut) {
            self.bump(); // `mut`
            self.cursor.cst.set_top_kind(SyntaxKind::REF_MUT_TYPE);
            let inner = self.parse_type_d(depth + 1);
            let span = self.cover(start, inner.span);
            self.cursor.cst.finish(SyntaxKind::REF_MUT_TYPE);
            return Type {
                kind: TypeKind::RefMut(Box::new(inner)),
                span,
            };
        }
        let inner = self.parse_type_d(depth + 1);
        let span = self.cover(start, inner.span);
        self.cursor.cst.finish(SyntaxKind::REF_TYPE);
        Type {
            kind: TypeKind::Ref(Box::new(inner)),
            span,
        }
    }

    fn parse_path_type(&mut self, depth: usize, start: SourceSpan) -> Type {
        self.cursor.cst.start(SyntaxKind::PATH_TYPE);
        let is_container = {
            let t = self.peek_ident_text();
            matches!(t, Some("Array") | Some("Optional") | Some("Result"))
                && self.cursor.peek_kind_at(1) == TokenKind::Less
        };
        let container_kind = if is_container {
            match self.peek_ident_text() {
                Some("Array") => Some(SyntaxKind::ARRAY_TYPE),
                Some("Optional") => Some(SyntaxKind::OPTION_TYPE),
                Some("Result") => Some(SyntaxKind::RESULT_TYPE),
                _ => None,
            }
        } else {
            None
        };
        if let Some(frame) = container_kind {
            self.cursor.cst.set_top_kind(frame);
            self.bump(); // `Array`/`Optional`/`Result`
            let args = self.parse_generic_arg_list(depth);
            let span = self.cover(start, args.last().map(|t| t.span).unwrap_or(start));
            self.cursor.cst.finish(frame);
            let unit = || Type {
                kind: TypeKind::Unit,
                span: start,
            };
            let kind = if frame == SyntaxKind::ARRAY_TYPE {
                TypeKind::Array(Box::new(args.first().cloned().unwrap_or_else(unit)))
            } else if frame == SyntaxKind::OPTION_TYPE {
                TypeKind::Optional(Box::new(args.first().cloned().unwrap_or_else(unit)))
            } else {
                TypeKind::Result {
                    ok: Box::new(args.first().cloned().unwrap_or_else(unit)),
                    err: Box::new(args.get(1).cloned().unwrap_or_else(unit)),
                }
            };
            return Type { kind, span };
        }

        // path → possível generic
        let segments = self.parse_type_path_segments();
        let qn = QualifiedName {
            segments,
            span: self.cover(start, self.start_span()),
        };
        if self.at(TokenKind::Less) {
            self.cursor.cst.set_top_kind(SyntaxKind::GENERIC_TYPE);
            let args = self.parse_generic_arg_list(depth);
            let span = self.cover(start, args.last().map(|t| t.span).unwrap_or(qn.span));
            self.cursor.cst.finish(SyntaxKind::GENERIC_TYPE);
            Type {
                kind: TypeKind::Generic { path: qn, args },
                span,
            }
        } else {
            let span = self.cover(start, self.start_span());
            self.cursor.cst.finish(SyntaxKind::PATH_TYPE);
            let mut t = Type {
                kind: TypeKind::Path(qn),
                span,
            };
            t.span = self.cover(start, self.start_span());
            t
        }
    }

    fn parse_type_path_segments(&mut self) -> Vec<Ident> {
        let mut segments = Vec::new();
        let seg = self.expect_ident("type name");
        segments.push(seg);
        while self.at(TokenKind::DoubleColon) {
            self.bump();
            if self.at_ident() {
                let s = self.expect_ident("type name");
                segments.push(s);
            } else {
                self.report_expected(
                    "identifier",
                    self.describe_current(),
                    self.cursor.peek_span(),
                );
                break;
            }
        }
        segments
    }

    fn parse_generic_arg_list(&mut self, depth: usize) -> Vec<Type> {
        let mut args = Vec::new();
        self.cursor.cst.start(SyntaxKind::GENERIC_ARG_LIST);
        self.expect_bump(TokenKind::Less, "'<'");
        loop {
            self.skip_soft();
            if self.cursor.at_greater_virtual() {
                break;
            }
            if self.cursor.eof() {
                self.report_expected("'>'", self.describe_current(), self.cursor.peek_span());
                break;
            }
            if self.break_on_block_close("'>'") {
                break;
            }
            let t = self.parse_type_d(depth + 1);
            args.push(t);
            self.skip_soft();
            if self.at(TokenKind::Comma) {
                self.bump();
                continue;
            }
            if self.cursor.at_greater_virtual() {
                break;
            }
            if self.break_on_block_close("'>'") {
                break;
            }
            if self.cursor.eof() {
                self.report_expected("'>'", self.describe_current(), self.cursor.peek_span());
                break;
            }
            self.report_expected(
                "',' or '>'",
                self.describe_current(),
                self.cursor.peek_span(),
            );
            break;
        }
        if self.cursor.at_greater_virtual() {
            self.cursor.bump_greater_virtual();
        } else {
            let sp = self.cursor.peek_span();
            self.record_missing_token(TokenKind::Greater, sp);
            self.report_expected("'>'", self.describe_current(), sp);
        }
        self.cursor.cst.finish(SyntaxKind::GENERIC_ARG_LIST);
        args
    }

    // ------------------------------------------------------------------
    // expressões (Pratt, §150-162)
    // ------------------------------------------------------------------

    fn parse_expression(&mut self) -> Expr {
        self.parse_expr_bp(0, 0)
    }

    /// Expressão em posição de condição/scrutinee/iterable (`if`, `while`,
    /// `match`, `for`): struct literal desligado porque o `{` seguinte
    /// pertence ao constructo, não ao literal (§422).
    fn parse_expr_no_struct(&mut self, depth: usize) -> Expr {
        let prev = self.allow_struct_literal;
        self.allow_struct_literal = false;
        let e = self.parse_expr_bp(0, depth);
        self.allow_struct_literal = prev;
        e
    }

    fn parse_expr_bp(&mut self, min_bp: u8, depth: usize) -> Expr {
        let start = self.start_span();
        if self.depth_exceeded(depth, start) {
            return self.sentinel_expr(start);
        }
        let mut lhs = self.parse_prefix(depth);
        let mut saw_comparison = false;
        loop {
            if self.delim_depth == 0 && self.cursor.at_hard_line_break() {
                break;
            }
            // postfix
            match self.cursor.peek_kind() {
                TokenKind::LeftParen => {
                    lhs = self.parse_call(lhs, depth);
                    continue;
                }
                TokenKind::LeftBracket => {
                    lhs = self.parse_index(lhs, depth);
                    continue;
                }
                TokenKind::Dot => {
                    lhs = self.parse_dot_access(lhs, depth);
                    continue;
                }
                _ => {}
            }
            if let Some((op, l_bp, r_bp)) = self.peek_binop() {
                if l_bp < min_bp {
                    break;
                }
                if is_comparison_op(op) {
                    if saw_comparison {
                        let sp = self.cursor.peek_span();
                        self.report(
                            PARSE_CHAINED_COMPARISON,
                            sp,
                            "parser.chained_comparison",
                            "chained comparisons are not allowed (§157-162)".to_string(),
                        );
                    } else {
                        saw_comparison = true;
                    }
                }
                self.bump();
                // §139: expressão incompleta continua na próxima linha —
                // newline após operador binário é soft, não termina a linha.
                self.skip_soft();
                let rhs = self.parse_expr_bp(r_bp, depth + 1);
                let span = self.cover(lhs.span, rhs.span);
                lhs = Expr {
                    kind: ExprKind::Binary(op, Box::new(lhs), Box::new(rhs)),
                    span,
                };
                continue;
            }
            break;
        }
        lhs
    }

    fn parse_prefix(&mut self, depth: usize) -> Expr {
        let start = self.start_span();
        match self.cursor.peek_kind() {
            TokenKind::IntegerLiteral => {
                let sp = self.bump();
                let text = self.cursor.span_text(sp).to_string();
                let value = self.decode_int(&text, sp);
                Expr {
                    kind: ExprKind::IntLiteral(value),
                    span: sp,
                }
            }
            TokenKind::FloatLiteral => {
                let sp = self.bump();
                let text = self.cursor.span_text(sp).to_string();
                let value = self.decode_float(&text, sp);
                Expr {
                    kind: ExprKind::FloatLiteral(value),
                    span: sp,
                }
            }
            TokenKind::CharLiteral => {
                let sp = self.bump();
                let text = self.cursor.span_text(sp).to_string();
                let value = self.decode_char(&text, sp);
                Expr {
                    kind: ExprKind::CharLiteral(value),
                    span: sp,
                }
            }
            TokenKind::RawStringLiteral => {
                self.cursor.cst.start(SyntaxKind::RAW_STRING_LITERAL);
                let sp = self.bump();
                let text = self.cursor.span_text(sp).to_string();
                let value = decode_raw_string(&text);
                self.cursor.cst.finish(SyntaxKind::RAW_STRING_LITERAL);
                Expr {
                    kind: ExprKind::StringLiteral(StringLiteral {
                        parts: vec![StringPart::Text(value)],
                        multiline: false,
                    }),
                    span: sp,
                }
            }
            TokenKind::ByteStringLiteral => {
                self.cursor.cst.start(SyntaxKind::BYTE_STRING_LITERAL);
                let sp = self.bump();
                let text = self.cursor.span_text(sp).to_string();
                let value = decode_byte_string(&text);
                self.cursor.cst.finish(SyntaxKind::BYTE_STRING_LITERAL);
                Expr {
                    kind: ExprKind::ByteStringLiteral(value),
                    span: sp,
                }
            }
            TokenKind::StringStart(style) => self.parse_string_expr(style),
            TokenKind::Keyword(Keyword::True) => {
                let sp = self.bump();
                Expr {
                    kind: ExprKind::BoolLiteral(true),
                    span: sp,
                }
            }
            TokenKind::Keyword(Keyword::False) => {
                let sp = self.bump();
                Expr {
                    kind: ExprKind::BoolLiteral(false),
                    span: sp,
                }
            }
            TokenKind::Identifier => self.parse_ident_or_path_atom(),
            TokenKind::Underscore => {
                let sp = self.bump();
                Expr {
                    kind: ExprKind::Underscore,
                    span: sp,
                }
            }
            TokenKind::Minus => {
                self.bump();
                let operand = self.parse_expr_bp(PREFIX_BP, depth + 1);
                let span = self.cover(start, operand.span);
                Expr {
                    kind: ExprKind::Unary(UnaryOp::Neg, Box::new(operand)),
                    span,
                }
            }
            TokenKind::Plus => {
                self.bump();
                let operand = self.parse_expr_bp(PREFIX_BP, depth + 1);
                let span = self.cover(start, operand.span);
                Expr {
                    kind: ExprKind::Unary(UnaryOp::Plus, Box::new(operand)),
                    span,
                }
            }
            TokenKind::Bang => {
                self.bump();
                let operand = self.parse_expr_bp(PREFIX_BP, depth + 1);
                let span = self.cover(start, operand.span);
                Expr {
                    kind: ExprKind::Unary(UnaryOp::Not, Box::new(operand)),
                    span,
                }
            }
            TokenKind::Tilde => {
                self.bump();
                let operand = self.parse_expr_bp(PREFIX_BP, depth + 1);
                let span = self.cover(start, operand.span);
                Expr {
                    kind: ExprKind::Unary(UnaryOp::BitNot, Box::new(operand)),
                    span,
                }
            }
            TokenKind::Keyword(Keyword::Ref) => {
                self.bump();
                if self.at_keyword(Keyword::Mut) {
                    self.bump();
                    self.cursor.cst.start(SyntaxKind::REF_MUT_EXPR);
                    let operand = self.parse_expr_bp(PREFIX_BP, depth + 1);
                    self.cursor.cst.finish(SyntaxKind::REF_MUT_EXPR);
                    let span = self.cover(start, operand.span);
                    return Expr {
                        kind: ExprKind::RefMut(Box::new(operand)),
                        span,
                    };
                }
                self.cursor.cst.start(SyntaxKind::REF_EXPR);
                let operand = self.parse_expr_bp(PREFIX_BP, depth + 1);
                self.cursor.cst.finish(SyntaxKind::REF_EXPR);
                let span = self.cover(start, operand.span);
                Expr {
                    kind: ExprKind::Ref(Box::new(operand)),
                    span,
                }
            }
            TokenKind::Keyword(Keyword::Move) => {
                self.cursor.cst.start(SyntaxKind::MOVE_EXPR);
                self.bump();
                let operand = self.parse_expr_bp(PREFIX_BP, depth + 1);
                self.cursor.cst.finish(SyntaxKind::MOVE_EXPR);
                let span = self.cover(start, operand.span);
                Expr {
                    kind: ExprKind::Move(Box::new(operand)),
                    span,
                }
            }
            TokenKind::Keyword(Keyword::Await) => {
                self.cursor.cst.start(SyntaxKind::AWAIT_EXPR);
                self.bump();
                let operand = self.parse_expr_bp(PREFIX_BP, depth + 1);
                self.cursor.cst.finish(SyntaxKind::AWAIT_EXPR);
                let span = self.cover(start, operand.span);
                Expr {
                    kind: ExprKind::Await(Box::new(operand)),
                    span,
                }
            }
            TokenKind::Keyword(Keyword::Try) => {
                self.cursor.cst.start(SyntaxKind::TRY_EXPR);
                self.bump();
                let operand = self.parse_expr_bp(PREFIX_BP, depth + 1);
                self.cursor.cst.finish(SyntaxKind::TRY_EXPR);
                let span = self.cover(start, operand.span);
                Expr {
                    kind: ExprKind::Try(Box::new(operand)),
                    span,
                }
            }
            TokenKind::Keyword(Keyword::Unsafe) => {
                self.cursor.cst.start(SyntaxKind::UNSAFE_EXPR);
                self.bump();
                let block = self.parse_block();
                self.cursor.cst.finish(SyntaxKind::UNSAFE_EXPR);
                let span = self.cover(start, block.span);
                Expr {
                    kind: ExprKind::Unsafe(block),
                    span,
                }
            }
            TokenKind::Keyword(Keyword::If) => self.parse_if_expr(0),
            TokenKind::Keyword(Keyword::Match) => self.parse_match_expr(0),
            TokenKind::Keyword(Keyword::While) => self.parse_while_expr(0),
            TokenKind::Keyword(Keyword::Loop) => self.parse_loop_expr(0),
            TokenKind::Keyword(Keyword::For) => self.parse_for_expr(0),
            TokenKind::LeftParen => self.parse_paren_or_unit(),
            TokenKind::LeftBracket => self.parse_array_expr(),
            TokenKind::LeftBrace => {
                self.cursor.cst.start(SyntaxKind::BLOCK_EXPR);
                let block = self.parse_block();
                let span = block.span;
                self.cursor.cst.finish(SyntaxKind::BLOCK_EXPR);
                Expr {
                    kind: ExprKind::Block(block),
                    span,
                }
            }
            _ => {
                // erro: consume o token (exceto Eof) p/ garantir progresso
                let found = self.describe_current();
                self.report(
                    PARSE_INVALID_EXPRESSION,
                    start,
                    "parser.invalid_expression",
                    format!("expected an expression, found {found}"),
                );
                if !self.cursor.eof() {
                    self.bump();
                }
                self.sentinel_expr(start)
            }
        }
    }

    fn parse_ident_or_path_atom(&mut self) -> Expr {
        let start = self.start_span();
        let mut segments: Vec<Ident> = Vec::new();
        let seg = self.expect_ident("identifier");
        segments.push(seg);
        let is_path = self.at(TokenKind::DoubleColon);
        if is_path {
            while self.at(TokenKind::DoubleColon) {
                self.bump();
                if self.at_ident() {
                    let s = self.expect_ident("identifier");
                    segments.push(s);
                } else {
                    self.report_expected(
                        "identifier",
                        self.describe_current(),
                        self.cursor.peek_span(),
                    );
                    break;
                }
            }
        }
        let span = self.cover(start, self.cursor.peek_span());
        // Struct literal `User { id: id }` / `pkg::User { id }` (§130-141):
        // um path (multi ou single segment) seguido de `{`. Em
        // condição/scrutinee (`if`/`while`/`match`/`for`) o `{` pertence
        // ao constructo e o literal fica desligado.
        if self.allow_struct_literal && self.at(TokenKind::LeftBrace) {
            let qn = QualifiedName { segments, span };
            return self.parse_struct_literal(qn, start);
        }
        if is_path {
            let qn = QualifiedName {
                segments,
                span: self.cover(start, self.cursor.peek_span()),
            };
            Expr {
                kind: ExprKind::Path(qn),
                span: self.cover(start, self.cursor.peek_span()),
            }
        } else {
            let _ = span;
            Expr {
                kind: ExprKind::Ident(segments.remove(0)),
                span: self.cover(start, self.cursor.peek_span()),
            }
        }
    }

    fn parse_struct_literal(&mut self, qn: QualifiedName, start: SourceSpan) -> Expr {
        self.cursor.cst.start(SyntaxKind::STRUCT_LITERAL);
        self.bump(); // '{'
        let mut fields: Vec<StructLiteralField> = Vec::new();
        loop {
            self.skip_soft();
            if self.at(TokenKind::RightBrace) {
                self.bump();
                break;
            }
            if self.cursor.eof() {
                self.report_expected("'}'", self.describe_current(), self.cursor.peek_span());
                break;
            }
            if self.break_on_block_close("'}'") {
                break;
            }
            let fstart = self.start_span();
            let name = self.expect_ident("field name");
            self.skip_soft();
            let field = if self.at(TokenKind::Colon) {
                self.bump();
                let expr = self.parse_expr_bp(0, 0);
                let span = self.cover(fstart, expr.span);
                StructLiteralField { name, expr, span }
            } else {
                // Shorthand `User { id }` == `id: id` (§134-136).
                let span = self.cover(fstart, name.span);
                let expr = Expr {
                    kind: ExprKind::Ident(name.clone()),
                    span: name.span,
                };
                StructLiteralField { name, expr, span }
            };
            fields.push(field);
            self.skip_soft();
            if self.at(TokenKind::Comma) {
                self.bump();
                continue;
            }
            if self.at(TokenKind::RightBrace) {
                self.bump();
                break;
            }
            if self.break_on_block_close("'}'") {
                break;
            }
            if self.cursor.eof() {
                self.report_expected("'}'", self.describe_current(), self.cursor.peek_span());
                break;
            }
            self.report_expected(
                "',' or '}'",
                self.describe_current(),
                self.cursor.peek_span(),
            );
            self.bump();
        }
        let span = self.cover(start, self.start_span());
        self.cursor.cst.finish(SyntaxKind::STRUCT_LITERAL);
        Expr {
            kind: ExprKind::StructConstruct(StructConstructExpr {
                path: qn,
                fields,
                span,
            }),
            span,
        }
    }

    fn parse_call(&mut self, callee: Expr, depth: usize) -> Expr {
        self.delim_depth += 1;
        self.expect_bump(TokenKind::LeftParen, "'('");
        let mut args = Vec::new();
        loop {
            self.skip_soft();
            if self.at(TokenKind::RightParen) {
                self.bump();
                break;
            }
            if self.cursor.eof() {
                self.report_expected("')'", self.describe_current(), self.cursor.peek_span());
                break;
            }
            if self.break_on_block_close("')'") {
                break;
            }
            let e = self.parse_expr_bp(0, depth + 1);
            args.push(e);
            self.skip_soft();
            if self.at(TokenKind::Comma) {
                self.bump();
                continue;
            }
            if self.at(TokenKind::RightParen) {
                self.bump();
                break;
            }
            if self.break_on_block_close("')'") {
                break;
            }
            if self.cursor.eof() {
                self.report_expected("')'", self.describe_current(), self.cursor.peek_span());
                break;
            }
            self.report_expected(
                "',' or ')'",
                self.describe_current(),
                self.cursor.peek_span(),
            );
            break;
        }
        self.delim_depth -= 1;
        let span = self.cover(callee.span, self.start_span());
        Expr {
            kind: ExprKind::Call {
                callee: Box::new(callee),
                args,
            },
            span,
        }
    }

    fn parse_index(&mut self, target: Expr, depth: usize) -> Expr {
        self.delim_depth += 1;
        self.expect_bump(TokenKind::LeftBracket, "'['");
        let index = if self.at(TokenKind::RightBracket) || self.cursor.eof() {
            let sp = self.start_span();
            self.report_expected("an expression", self.describe_current(), sp);
            self.sentinel_expr(sp)
        } else {
            self.parse_expr_bp(0, depth + 1)
        };
        self.expect_bump(TokenKind::RightBracket, "']'");
        self.delim_depth -= 1;
        let span = self.cover(target.span, self.start_span());
        Expr {
            kind: ExprKind::Index {
                target: Box::new(target),
                index: Box::new(index),
            },
            span,
        }
    }

    fn parse_dot_access(&mut self, target: Expr, depth: usize) -> Expr {
        self.bump(); // `.`
                     // Membro nomeado `.field`/`.method` ou índice numérico de tupla `.0`.
        let name = if self.at(TokenKind::IntegerLiteral) {
            let sp = self.bump();
            let text = self.cursor.span_text(sp).to_string();
            Ident {
                name: text,
                span: sp,
            }
        } else {
            self.expect_ident("member name")
        };
        let is_call = self.at(TokenKind::LeftParen);
        if is_call {
            self.delim_depth += 1;
            self.bump(); // `(`
            let mut args = Vec::new();
            loop {
                self.skip_soft();
                if self.at(TokenKind::RightParen) {
                    self.bump();
                    break;
                }
                if self.cursor.eof() {
                    self.report_expected("')'", self.describe_current(), self.cursor.peek_span());
                    break;
                }
                if self.break_on_block_close("')'") {
                    break;
                }
                let e = self.parse_expr_bp(0, depth + 1);
                args.push(e);
                self.skip_soft();
                if self.at(TokenKind::Comma) {
                    self.bump();
                    continue;
                }
                if self.at(TokenKind::RightParen) {
                    self.bump();
                    break;
                }
                if self.break_on_block_close("')'") {
                    break;
                }
                self.report_expected(
                    "',' or ')'",
                    self.describe_current(),
                    self.cursor.peek_span(),
                );
                break;
            }
            self.delim_depth -= 1;
            let span = self.cover(target.span, self.start_span());
            Expr {
                kind: ExprKind::MethodCall {
                    target: Box::new(target),
                    name,
                    args,
                },
                span,
            }
        } else {
            let span = self.cover(target.span, name.span);
            Expr {
                kind: ExprKind::Field {
                    target: Box::new(target),
                    name,
                },
                span,
            }
        }
    }

    fn parse_paren_or_unit(&mut self) -> Expr {
        let start = self.start_span();
        self.cursor.cst.start(SyntaxKind::PAREN_EXPR);
        self.delim_depth += 1;
        self.expect_bump(TokenKind::LeftParen, "'('");
        self.skip_soft();
        if self.at(TokenKind::RightParen) {
            self.bump();
            self.delim_depth -= 1;
            let span = self.cover(start, self.start_span());
            self.cursor.cst.finish(SyntaxKind::PAREN_EXPR);
            // `()` = literal Unit (§366-368)
            return Expr {
                kind: ExprKind::Tuple(Vec::new()),
                span,
            };
        }
        // §551-553: dentro de `( ... )` o `{` já não colide com o bloco da
        // estrutura condicional, então struct literal volta a ser permitido
        // mesmo quando o parêntese está numa condição/scrutinee/iterable.
        let prev_allow = self.allow_struct_literal;
        self.allow_struct_literal = true;
        let inner = self.parse_expr_bp(0, 0);
        self.allow_struct_literal = prev_allow;
        self.skip_soft();
        if self.at(TokenKind::Comma) {
            let sp = self.cursor.peek_span();
            self.report(
                PARSE_INVALID_EXPRESSION,
                sp,
                "parser.tuple_literal",
                "tuple literals are not supported (only `()` = Unit) (§365-368)".to_string(),
            );
            loop {
                self.skip_soft();
                if self.at(TokenKind::RightParen) {
                    self.bump();
                    break;
                }
                if self.cursor.eof() {
                    self.report_expected("')'", self.describe_current(), self.cursor.peek_span());
                    break;
                }
                if self.break_on_block_close("')'") {
                    break;
                }
                self.bump();
            }
            self.delim_depth -= 1;
            let span = self.cover(start, self.start_span());
            self.cursor.cst.finish(SyntaxKind::PAREN_EXPR);
            let _ = inner;
            return self.sentinel_expr(span);
        }
        self.expect_bump(TokenKind::RightParen, "')'");
        self.delim_depth -= 1;
        let span = self.cover(start, self.start_span());
        self.cursor.cst.finish(SyntaxKind::PAREN_EXPR);
        Expr {
            kind: ExprKind::Paren(Box::new(inner)),
            span,
        }
    }

    fn parse_array_expr(&mut self) -> Expr {
        let start = self.start_span();
        self.cursor.cst.start(SyntaxKind::ARRAY_EXPR);
        self.delim_depth += 1;
        self.expect_bump(TokenKind::LeftBracket, "'['");
        let mut elems = Vec::new();
        loop {
            self.skip_soft();
            if self.at(TokenKind::RightBracket) {
                self.bump();
                break;
            }
            if self.cursor.eof() {
                self.report_expected("']'", self.describe_current(), self.cursor.peek_span());
                break;
            }
            if self.break_on_block_close("']'") {
                break;
            }
            let e = self.parse_expr_bp(0, 0);
            elems.push(e);
            self.skip_soft();
            if self.at(TokenKind::Comma) {
                self.bump();
                continue;
            }
            if self.at(TokenKind::RightBracket) {
                self.bump();
                break;
            }
            if self.break_on_block_close("']'") {
                break;
            }
            self.report_expected(
                "',' or ']'",
                self.describe_current(),
                self.cursor.peek_span(),
            );
            break;
        }
        self.delim_depth -= 1;
        let span = self.cover(start, self.start_span());
        self.cursor.cst.finish(SyntaxKind::ARRAY_EXPR);
        Expr {
            kind: ExprKind::Array(elems),
            span,
        }
    }

    // ------------------------------------------------------------------
    // strings (§112-string, §200-202, normativa strings)
    // ------------------------------------------------------------------

    fn parse_string_expr(&mut self, style: StringStyle) -> Expr {
        let start = self.start_span();
        self.cursor.cst.start(SyntaxKind::STRING_LITERAL);
        let multiline = style == StringStyle::Multiline;
        let mut parts: Vec<StringPart> = Vec::new();
        let mut last = self.bump(); // StringStart
        loop {
            match self.cursor.peek_kind() {
                TokenKind::StringText => {
                    let sp = self.bump();
                    let text = self.cursor.span_text(sp).to_string();
                    last = sp;
                    parts.push(StringPart::Text(decode_escapes(&text)));
                }
                TokenKind::InterpolationStart => {
                    let is = self.bump();
                    self.delim_depth += 1;
                    let e = self.parse_expr_bp(0, 0);
                    self.delim_depth -= 1;
                    let ie = if self.at(TokenKind::InterpolationEnd) {
                        self.bump()
                    } else {
                        let sp = self.cursor.peek_span();
                        self.report_expected("'}'", self.describe_current(), sp);
                        sp
                    };
                    last = ie;
                    parts.push(StringPart::Interpolation {
                        expr: Box::new(e),
                        span: self.cover(is, ie),
                    });
                }
                TokenKind::StringEnd(_) => {
                    last = self.bump();
                    break;
                }
                TokenKind::Eof => {
                    self.report_expected("'\"'", self.describe_current(), self.cursor.peek_span());
                    break;
                }
                _ => {
                    let sp = self.cursor.peek_span();
                    self.report_expected("'\"'", self.describe_current(), sp);
                    if !self.cursor.eof() {
                        self.bump();
                    }
                    break;
                }
            }
        }
        let span = self.cover(start, last);
        self.cursor.cst.finish(SyntaxKind::STRING_LITERAL);
        Expr {
            kind: ExprKind::StringLiteral(StringLiteral { parts, multiline }),
            span,
        }
    }

    // ------------------------------------------------------------------
    // if / match / loops (§205-224)
    // ------------------------------------------------------------------

    fn parse_if_expr(&mut self, depth: usize) -> Expr {
        let start = self.start_span();
        self.cursor.cst.start(SyntaxKind::IF_EXPR);
        self.bump(); // `if`
        let condition = Box::new(self.parse_expr_no_struct(depth + 1));
        let then_block = self.parse_block();
        let mut else_ifs: Vec<ElseIf> = Vec::new();
        let mut else_block: Option<Block> = None;
        loop {
            if self.cursor.peek_kind_skipping_newlines() != TokenKind::Keyword(Keyword::Else) {
                break;
            }
            self.skip_soft();
            self.bump(); // `else`
            if self.at_keyword(Keyword::If) {
                self.cursor.cst.start(SyntaxKind::IF_ARM);
                let cstart = self.start_span();
                self.bump(); // `if`
                let c2 = self.parse_expr_no_struct(depth + 1);
                let b2 = self.parse_block();
                let span = self.cover(cstart, b2.span);
                self.cursor.cst.finish(SyntaxKind::IF_ARM);
                else_ifs.push(ElseIf {
                    condition: Box::new(c2),
                    block: b2,
                    span,
                });
            } else {
                self.cursor.cst.start(SyntaxKind::ELSE_ARM);
                let b = self.parse_block();
                self.cursor.cst.finish(SyntaxKind::ELSE_ARM);
                else_block = Some(b);
                break;
            }
        }
        let span = self.cover(start, self.start_span());
        self.cursor.cst.finish(SyntaxKind::IF_EXPR);
        Expr {
            kind: ExprKind::If(IfExpr {
                condition,
                then_block,
                else_ifs,
                else_block,
                span,
            }),
            span,
        }
    }

    fn parse_match_expr(&mut self, depth: usize) -> Expr {
        let start = self.start_span();
        self.cursor.cst.start(SyntaxKind::MATCH_EXPR);
        self.bump(); // `match`
        let scrutinee = Box::new(self.parse_expr_no_struct(depth + 1));
        self.expect_bump(TokenKind::LeftBrace, "'{'");
        let mut arms = Vec::new();
        loop {
            self.skip_soft();
            if self.at(TokenKind::RightBrace) {
                self.bump();
                break;
            }
            if self.cursor.eof() {
                self.report_expected("'}'", self.describe_current(), self.cursor.peek_span());
                break;
            }
            if self.skip_stray_body_comma("match arms") {
                continue;
            }
            let entry = self.start_span();
            let arm = self.parse_match_arm(depth + 1);
            arms.push(arm);
            self.ensure_progress(entry);
            // terminador de arm: newline ou `}` (§251-253)
            let broke = self.cursor.skip_trivia();
            if !broke {
                match self.cursor.peek_kind() {
                    TokenKind::Newline | TokenKind::RightBrace | TokenKind::Eof => {}
                    _ => {
                        let sp = self.cursor.peek_span();
                        self.report(
                            PARSE_MISSING_STATEMENT_TERMINATOR,
                            sp,
                            "parser.missing_statement_terminator",
                            "expected a new line or closing brace after match arm".to_string(),
                        );
                    }
                }
            }
        }
        let span = self.cover(start, self.start_span());
        self.cursor.cst.finish(SyntaxKind::MATCH_EXPR);
        Expr {
            kind: ExprKind::Match(MatchExpr {
                scrutinee,
                arms,
                span,
            }),
            span,
        }
    }

    fn parse_match_arm(&mut self, depth: usize) -> MatchArm {
        let start = self.start_span();
        self.cursor.cst.start(SyntaxKind::MATCH_ARM);
        let pattern = self.parse_pattern();
        let mut guard: Option<Box<Expr>> = None;
        if self.at_keyword(Keyword::If) {
            self.cursor.cst.start(SyntaxKind::MATCH_GUARD);
            self.bump(); // `if`
            let g = self.parse_expr_bp(0, depth + 1);
            self.cursor.cst.finish(SyntaxKind::MATCH_GUARD);
            guard = Some(Box::new(g));
        }
        self.expect_bump(TokenKind::FatArrow, "'=>'");
        let body = if self.at(TokenKind::LeftBrace) {
            let b = self.parse_block();
            let sp = b.span;
            Expr {
                kind: ExprKind::Block(b),
                span: sp,
            }
        } else {
            self.parse_expr_bp(0, depth + 1)
        };
        let span = self.cover(start, body.span);
        self.cursor.cst.finish(SyntaxKind::MATCH_ARM);
        MatchArm {
            pattern,
            guard,
            body,
            span,
        }
    }

    fn parse_while_expr(&mut self, depth: usize) -> Expr {
        let start = self.start_span();
        self.cursor.cst.start(SyntaxKind::WHILE_EXPR);
        self.bump(); // `while`
        let condition = Box::new(self.parse_expr_no_struct(depth + 1));
        let body = self.parse_block();
        let span = self.cover(start, body.span);
        self.cursor.cst.finish(SyntaxKind::WHILE_EXPR);
        Expr {
            kind: ExprKind::While(WhileExpr {
                condition,
                body,
                span,
            }),
            span,
        }
    }

    fn parse_loop_expr(&mut self, _depth: usize) -> Expr {
        let start = self.start_span();
        self.cursor.cst.start(SyntaxKind::LOOP_EXPR);
        self.bump(); // `loop`
        let body = self.parse_block();
        let span = self.cover(start, body.span);
        self.cursor.cst.finish(SyntaxKind::LOOP_EXPR);
        Expr {
            kind: ExprKind::Loop(LoopExpr { body, span }),
            span,
        }
    }

    fn parse_for_expr(&mut self, depth: usize) -> Expr {
        let start = self.start_span();
        self.cursor.cst.start(SyntaxKind::FOR_EXPR);
        self.bump(); // `for`
        let mut binding_mode = ForBindingMode::Value;
        if self.at_keyword(Keyword::Ref) {
            self.bump();
            if self.at_keyword(Keyword::Mut) {
                self.bump();
                binding_mode = ForBindingMode::RefMut;
            } else {
                binding_mode = ForBindingMode::Ref;
            }
        } else if self.at_keyword(Keyword::Move) {
            self.bump();
            binding_mode = ForBindingMode::Move;
        }
        let variable = self.expect_ident("loop variable");
        if self.peek_ident_text() != Some("in") {
            self.report_expected("'in'", self.describe_current(), self.cursor.peek_span());
        } else {
            self.bump();
        }
        let iterable = Box::new(self.parse_expr_no_struct(depth + 1));
        let body = self.parse_block();
        let span = self.cover(start, body.span);
        self.cursor.cst.finish(SyntaxKind::FOR_EXPR);
        Expr {
            kind: ExprKind::For(ForExpr {
                variable,
                binding_mode,
                iterable,
                body,
                span,
            }),
            span,
        }
    }

    // ------------------------------------------------------------------
    // statements (§114-146, §222-247)
    // ------------------------------------------------------------------

    fn parse_block(&mut self) -> Block {
        // Corpo pode abrir na linha seguinte à assinatura (§274); newlines
        // antes de `{` são trivia (soft) e não quebram a estrutura.
        self.skip_soft();
        let start = self.start_span();
        self.cursor.cst.start(SyntaxKind::BLOCK);
        self.expect_bump(TokenKind::LeftBrace, "'{'");
        self.cursor.cst.start(SyntaxKind::STMT_LIST);
        let mut stmts = Vec::new();
        loop {
            self.skip_soft();
            if self.at(TokenKind::RightBrace) {
                self.bump();
                break;
            }
            if self.cursor.eof() {
                self.report_expected("'}'", self.describe_current(), self.cursor.peek_span());
                break;
            }
            let stmt = self.parse_statement();
            stmts.push(stmt);
            self.expect_statement_end();
        }
        let span = self.cover(start, self.start_span());
        self.cursor.cst.finish(SyntaxKind::STMT_LIST);
        self.cursor.cst.finish(SyntaxKind::BLOCK);
        Block { stmts, span }
    }

    fn parse_statement(&mut self) -> Stmt {
        let entry = self.start_span();
        let stmt = match self.cursor.peek_kind() {
            TokenKind::Keyword(Keyword::Let) => self.parse_let_stmt(),
            TokenKind::Keyword(Keyword::Var) => self.parse_var_stmt(),
            TokenKind::Keyword(Keyword::Const) => self.parse_const_stmt(),
            TokenKind::Keyword(Keyword::Return) => self.parse_return_stmt(),
            TokenKind::Keyword(Keyword::Break) => self.parse_break_stmt(),
            TokenKind::Keyword(Keyword::Continue) => self.parse_continue_stmt(),
            TokenKind::Keyword(Keyword::Discard) => self.parse_discard_stmt(),
            TokenKind::Keyword(Keyword::Unsafe) => self.parse_unsafe_stmt(),
            _ => self.parse_expr_or_assign_stmt(),
        };
        self.ensure_progress(entry);
        stmt
    }

    fn parse_let_stmt(&mut self) -> Stmt {
        let start = self.start_span();
        self.cursor.cst.start(SyntaxKind::LET_STMT);
        self.bump(); // `let`
        let name = self.expect_ident("variable name");
        let mut ty = None;
        if self.at(TokenKind::Colon) {
            self.bump();
            ty = Some(self.parse_type());
        }
        let init = if self.at(TokenKind::Equal) {
            self.bump();
            self.skip_soft();
            self.parse_expression()
        } else {
            let sp = self.start_span();
            self.report_expected("'='", self.describe_current(), sp);
            self.record_missing_token(TokenKind::Equal, sp);
            self.sentinel_expr(sp)
        };
        let span = self.cover(start, init.span);
        self.cursor.cst.finish(SyntaxKind::LET_STMT);
        Stmt {
            kind: StmtKind::Let(LetBinding {
                name,
                ty,
                init,
                span,
            }),
            span,
        }
    }

    fn parse_var_stmt(&mut self) -> Stmt {
        let start = self.start_span();
        self.cursor.cst.start(SyntaxKind::VAR_STMT);
        self.bump(); // `var`
        let name = self.expect_ident("variable name");
        let mut ty = None;
        if self.at(TokenKind::Colon) {
            self.bump();
            ty = Some(self.parse_type());
        }
        let init = if self.at(TokenKind::Equal) {
            self.bump();
            self.skip_soft();
            Some(self.parse_expression())
        } else {
            // `var` admite declaração sem inicializador (inicialização
            // adiada; ver definite assignment, Impl 05 §-FLOW-0005/0006).
            None
        };
        let span = init
            .as_ref()
            .map(|i| self.cover(start, i.span))
            .unwrap_or(self.cover(start, name.span));
        self.cursor.cst.finish(SyntaxKind::VAR_STMT);
        Stmt {
            kind: StmtKind::Var(VarBinding {
                name,
                ty,
                init,
                span,
            }),
            span,
        }
    }

    fn parse_const_stmt(&mut self) -> Stmt {
        let start = self.start_span();
        self.cursor.cst.start(SyntaxKind::CONST_STMT);
        self.bump(); // `const`
        let name = self.expect_ident("constant name");
        let mut ty = None;
        if self.at(TokenKind::Colon) {
            self.bump();
            ty = Some(self.parse_type());
        }
        let init = if self.at(TokenKind::Equal) {
            self.bump();
            self.skip_soft();
            self.parse_expression()
        } else {
            let sp = self.start_span();
            self.report_expected("'='", self.describe_current(), sp);
            self.record_missing_token(TokenKind::Equal, sp);
            self.sentinel_expr(sp)
        };
        let span = self.cover(start, init.span);
        self.cursor.cst.finish(SyntaxKind::CONST_STMT);
        Stmt {
            kind: StmtKind::Const(ConstBinding {
                name,
                ty,
                init,
                span,
            }),
            span,
        }
    }

    fn parse_return_stmt(&mut self) -> Stmt {
        let start = self.start_span();
        self.cursor.cst.start(SyntaxKind::RETURN_STMT);
        self.bump(); // `return`
        let value = if self.cursor.at_hard_line_break()
            || self.at(TokenKind::RightBrace)
            || self.cursor.eof()
        {
            None
        } else {
            Some(self.parse_expression())
        };
        let span = value
            .as_ref()
            .map(|v| self.cover(start, v.span))
            .unwrap_or(self.cover(start, self.start_span()));
        self.cursor.cst.finish(SyntaxKind::RETURN_STMT);
        Stmt {
            kind: StmtKind::Return(ReturnStmt { value, span }),
            span,
        }
    }

    fn parse_break_stmt(&mut self) -> Stmt {
        let start = self.start_span();
        self.cursor.cst.start(SyntaxKind::BREAK_STMT);
        self.bump(); // `break`
        let span = self.cover(start, self.start_span());
        self.cursor.cst.finish(SyntaxKind::BREAK_STMT);
        Stmt {
            kind: StmtKind::Break(BreakStmt { value: None, span }),
            span,
        }
    }

    fn parse_continue_stmt(&mut self) -> Stmt {
        let start = self.start_span();
        self.cursor.cst.start(SyntaxKind::CONTINUE_STMT);
        self.bump(); // `continue`
        let span = self.cover(start, self.start_span());
        self.cursor.cst.finish(SyntaxKind::CONTINUE_STMT);
        Stmt {
            kind: StmtKind::Continue(ContinueStmt { span }),
            span,
        }
    }

    fn parse_discard_stmt(&mut self) -> Stmt {
        let start = self.start_span();
        self.cursor.cst.start(SyntaxKind::DISCARD_STMT);
        self.bump(); // `discard`
        let expr = if self.cursor.at_hard_line_break()
            || self.at(TokenKind::RightBrace)
            || self.cursor.eof()
        {
            let sp = self.start_span();
            self.report(
                PARSE_INVALID_EXPRESSION,
                sp,
                "parser.discard_requires_expression",
                "`discard` requires an expression (§230-231)".to_string(),
            );
            self.record_missing_token(TokenKind::Identifier, sp);
            self.sentinel_expr(sp)
        } else {
            self.parse_expression()
        };
        let span = self.cover(start, expr.span);
        self.cursor.cst.finish(SyntaxKind::DISCARD_STMT);
        Stmt {
            kind: StmtKind::Discard(DiscardStmt { expr, span }),
            span,
        }
    }

    fn parse_unsafe_stmt(&mut self) -> Stmt {
        let start = self.start_span();
        self.cursor.cst.start(SyntaxKind::UNSAFE_STMT);
        self.bump(); // `unsafe`
        let block = if self.at(TokenKind::LeftBrace) {
            Some(self.parse_block())
        } else {
            let sp = self.start_span();
            self.report_expected("'{'", self.describe_current(), sp);
            None
        };
        let span = self.cover(start, self.start_span());
        self.cursor.cst.finish(SyntaxKind::UNSAFE_STMT);
        let block = block.unwrap_or(Block {
            stmts: Vec::new(),
            span,
        });
        Stmt {
            kind: StmtKind::Expr(Expr {
                kind: ExprKind::Unsafe(block),
                span,
            }),
            span,
        }
    }

    fn parse_expr_or_assign_stmt(&mut self) -> Stmt {
        let start = self.start_span();
        let lhs = self.parse_expression();
        let assign_op = if self.delim_depth == 0 && !self.cursor.at_hard_line_break() {
            self.peek_assign_op()
        } else {
            None
        };
        if let Some(op) = assign_op {
            let op_span = self.bump();
            self.cursor.cst.start(SyntaxKind::ASSIGN_STMT);
            let (target, valid) = self.expr_to_assign_target(lhs, op_span);
            if !valid {
                let sp = op_span;
                self.report(
                    PARSE_INVALID_ASSIGNMENT_TARGET,
                    sp,
                    "parser.invalid_assignment_target",
                    "expression is not a valid assignment target (§244-245)".to_string(),
                );
            }
            let rhs = self.parse_expression();
            let span = self.cover(start, rhs.span);
            if op != AssignOp::Equal {
                self.cursor.cst.set_top_kind(SyntaxKind::COMPOUND_ASSIGN);
            }
            self.cursor.cst.finish(SyntaxKind::ASSIGN_STMT);
            return Stmt {
                kind: StmtKind::Assign(target, op, rhs),
                span,
            };
        }
        self.cursor.cst.start(SyntaxKind::EXPR_STMT);
        let span = lhs.span;
        self.cursor.cst.finish(SyntaxKind::EXPR_STMT);
        Stmt {
            kind: StmtKind::Expr(lhs),
            span,
        }
    }

    fn peek_assign_op(&self) -> Option<AssignOp> {
        match self.cursor.peek_kind() {
            TokenKind::Equal => Some(AssignOp::Equal),
            TokenKind::PlusEqual => Some(AssignOp::PlusEqual),
            TokenKind::MinusEqual => Some(AssignOp::MinusEqual),
            TokenKind::StarEqual => Some(AssignOp::StarEqual),
            TokenKind::SlashEqual => Some(AssignOp::SlashEqual),
            TokenKind::PercentEqual => Some(AssignOp::PercentEqual),
            TokenKind::AmpEqual => Some(AssignOp::AmpEqual),
            TokenKind::PipeEqual => Some(AssignOp::PipeEqual),
            TokenKind::CaretEqual => Some(AssignOp::CaretEqual),
            TokenKind::ShiftLeftEqual => Some(AssignOp::ShiftLeftEqual),
            TokenKind::ShiftRightEqual => Some(AssignOp::ShiftRightEqual),
            _ => None,
        }
    }

    /// Binding power dos operadores infixos (Pratt), §152-162.
    ///
    /// `* / %` > `+ -` > `<< >>` > `< <= > >=` (não-assoc) > `== !=` (não-assoc)
    /// > `&` > `^` > `|` > `&&` > `||`. Com `rbp = lbp + 1` o RHS não consome o
    ///
    /// O próximo operador do mesmo nível não é consumido (associatividade à
    /// esquerda; comparação encadeada fica para o laço reportar
    /// `NEXA-PARSE-0006`).
    fn peek_binop(&self) -> Option<(BinaryOp, u8, u8)> {
        let op = match self.cursor.peek_kind() {
            TokenKind::Plus => BinaryOp::Add,
            TokenKind::Minus => BinaryOp::Sub,
            TokenKind::Star => BinaryOp::Mul,
            TokenKind::Slash => BinaryOp::Div,
            TokenKind::Percent => BinaryOp::Rem,
            TokenKind::EqualEqual => BinaryOp::Eq,
            TokenKind::BangEqual => BinaryOp::Ne,
            TokenKind::Less => BinaryOp::Lt,
            TokenKind::LessEqual => BinaryOp::Le,
            TokenKind::Greater => BinaryOp::Gt,
            TokenKind::GreaterEqual => BinaryOp::Ge,
            TokenKind::AmpAmp => BinaryOp::And,
            TokenKind::PipePipe => BinaryOp::Or,
            TokenKind::Ampersand => BinaryOp::BitAnd,
            TokenKind::Pipe => BinaryOp::BitOr,
            TokenKind::Caret => BinaryOp::BitXor,
            TokenKind::ShiftLeft => BinaryOp::Shl,
            TokenKind::ShiftRight => BinaryOp::Shr,
            _ => return None,
        };
        let prec = match op {
            BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem => 9,
            BinaryOp::Add | BinaryOp::Sub => 8,
            BinaryOp::Shl | BinaryOp::Shr => 7,
            BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => 6,
            BinaryOp::Eq | BinaryOp::Ne => 5,
            BinaryOp::BitAnd => 4,
            BinaryOp::BitXor => 3,
            BinaryOp::BitOr => 2,
            BinaryOp::And => 1,
            BinaryOp::Or => 0,
        };
        Some((op, prec, prec + 1))
    }

    fn expr_to_assign_target(&mut self, e: Expr, _op_span: SourceSpan) -> (AssignTarget, bool) {
        match e.kind {
            ExprKind::Ident(id) => (AssignTarget::Ident(id), true),
            ExprKind::Field { target, name } => (AssignTarget::Field(target, name), true),
            ExprKind::Index { target, index } => (AssignTarget::Index(target, index), true),
            _ => {
                let sp = e.span;
                let name = Ident {
                    name: "_".to_string(),
                    span: sp,
                };
                (AssignTarget::Ident(name), false)
            }
        }
    }

    // ------------------------------------------------------------------
    // padrões (§260-272)
    // ------------------------------------------------------------------

    fn parse_pattern(&mut self) -> Pattern {
        self.parse_pattern_d(0)
    }

    fn parse_pattern_d(&mut self, depth: usize) -> Pattern {
        let start = self.start_span();
        if self.depth_exceeded(depth, start) {
            return Pattern {
                kind: PatternKind::Ident(Ident {
                    name: "_".to_string(),
                    span: start,
                }),
                span: start,
            };
        }
        match self.cursor.peek_kind() {
            TokenKind::Underscore => {
                self.cursor.cst.start(SyntaxKind::WILDCARD_PATTERN);
                let sp = self.bump();
                self.cursor.cst.finish(SyntaxKind::WILDCARD_PATTERN);
                Pattern {
                    kind: PatternKind::Wildcard,
                    span: sp,
                }
            }
            TokenKind::IntegerLiteral
            | TokenKind::FloatLiteral
            | TokenKind::CharLiteral
            | TokenKind::StringStart(_)
            | TokenKind::RawStringLiteral
            | TokenKind::ByteStringLiteral
            | TokenKind::Keyword(Keyword::True)
            | TokenKind::Keyword(Keyword::False)
            | TokenKind::Minus => {
                let exp = self.parse_pattern_literal();
                let span = exp.span;
                Pattern {
                    kind: PatternKind::Literal(exp),
                    span,
                }
            }
            TokenKind::Identifier => self.parse_named_pattern(depth, start),
            TokenKind::LeftParen => self.parse_tuple_pattern(depth, start),
            _ => {
                self.report(
                    PARSE_INVALID_PATTERN,
                    start,
                    "parser.invalid_pattern",
                    format!("expected a pattern, found {}", self.describe_current()),
                );
                self.record_missing_token(TokenKind::Identifier, start);
                Pattern {
                    kind: PatternKind::Ident(Ident {
                        name: "_".to_string(),
                        span: start,
                    }),
                    span: start,
                }
            }
        }
    }

    fn parse_named_pattern(&mut self, depth: usize, start: SourceSpan) -> Pattern {
        let mut segments: Vec<Ident> = Vec::new();
        let seg = self.expect_ident("pattern name");
        let mut last_span = seg.span;
        segments.push(seg);
        let mut is_path = false;
        while self.at(TokenKind::DoubleColon) {
            is_path = true;
            let cc = self.bump();
            last_span = cc;
            if self.at_ident() {
                let s = self.expect_ident("pattern name");
                last_span = s.span;
                segments.push(s);
            } else {
                self.report_expected(
                    "identifier",
                    self.describe_current(),
                    self.cursor.peek_span(),
                );
                break;
            }
        }
        let path_segments = segments.clone();

        if self.at(TokenKind::LeftParen) {
            // payload variante tuple-like: `Some(x)` / `A::Some(x)`
            let payload = self.parse_paren_pattern_list(depth + 1);
            let (path, variant) = split_pattern_path(path_segments, start);
            let span = self.cover(start, self.start_span());
            let pattern = collapse_payload(payload);
            self.cursor.cst.start(SyntaxKind::ENUM_PATTERN);
            self.cursor.cst.finish(SyntaxKind::ENUM_PATTERN);
            return Pattern {
                kind: PatternKind::Enum {
                    path,
                    variant,
                    pattern,
                },
                span,
            };
        }
        if self.at(TokenKind::LeftBrace) {
            // struct pattern: `Failure { code, message }` / `User { id, .. }`
            self.cursor.cst.start(SyntaxKind::STRUCT_PATTERN);
            let fields = self.parse_struct_pattern_fields(depth + 1);
            let path = QualifiedName {
                segments: path_segments,
                span: self.cover(start, self.start_span()),
            };
            let span = self.cover(start, self.start_span());
            self.cursor.cst.finish(SyntaxKind::STRUCT_PATTERN);
            return Pattern {
                kind: PatternKind::Struct { path, fields },
                span,
            };
        }

        if is_path {
            // path sem payload: referência de variante unitária
            let (path, variant) = split_pattern_path(path_segments, start);
            Pattern {
                kind: PatternKind::Enum {
                    path,
                    variant,
                    pattern: None,
                },
                span: self.cover(start, last_span),
            }
        } else {
            // binding simples
            self.cursor.cst.start(SyntaxKind::IDENT_PATTERN);
            let first = segments.remove(0);
            let span = self.cover(start, first.span);
            self.cursor.cst.finish(SyntaxKind::IDENT_PATTERN);
            Pattern {
                kind: PatternKind::Ident(first),
                span,
            }
        }
    }

    fn parse_pattern_literal(&mut self) -> Expr {
        let start = self.start_span();
        match self.cursor.peek_kind() {
            TokenKind::Minus => {
                self.bump();
                let operand = self.parse_pattern_literal();
                let span = self.cover(start, operand.span);
                Expr {
                    kind: ExprKind::Unary(UnaryOp::Neg, Box::new(operand)),
                    span,
                }
            }
            TokenKind::IntegerLiteral => {
                let sp = self.bump();
                let text = self.cursor.span_text(sp).to_string();
                Expr {
                    kind: ExprKind::IntLiteral(self.decode_int(&text, sp)),
                    span: sp,
                }
            }
            TokenKind::FloatLiteral => {
                let sp = self.bump();
                let text = self.cursor.span_text(sp).to_string();
                Expr {
                    kind: ExprKind::FloatLiteral(self.decode_float(&text, sp)),
                    span: sp,
                }
            }
            TokenKind::CharLiteral => {
                let sp = self.bump();
                let text = self.cursor.span_text(sp).to_string();
                Expr {
                    kind: ExprKind::CharLiteral(self.decode_char(&text, sp)),
                    span: sp,
                }
            }
            TokenKind::StringStart(style) => self.parse_string_expr(style),
            TokenKind::RawStringLiteral => {
                self.cursor.cst.start(SyntaxKind::RAW_STRING_LITERAL);
                let sp = self.bump();
                let text = self.cursor.span_text(sp).to_string();
                let v = decode_raw_string(&text);
                self.cursor.cst.finish(SyntaxKind::RAW_STRING_LITERAL);
                Expr {
                    kind: ExprKind::StringLiteral(StringLiteral {
                        parts: vec![StringPart::Text(v)],
                        multiline: false,
                    }),
                    span: sp,
                }
            }
            TokenKind::Keyword(Keyword::True) => {
                let sp = self.bump();
                Expr {
                    kind: ExprKind::BoolLiteral(true),
                    span: sp,
                }
            }
            TokenKind::Keyword(Keyword::False) => {
                let sp = self.bump();
                Expr {
                    kind: ExprKind::BoolLiteral(false),
                    span: sp,
                }
            }
            _ => {
                let found = self.describe_current();
                self.report(
                    PARSE_INVALID_PATTERN,
                    start,
                    "parser.invalid_pattern_literal",
                    format!("expected a literal, found {found}"),
                );
                if !self.cursor.eof() {
                    self.bump();
                }
                self.sentinel_expr(start)
            }
        }
    }

    fn parse_paren_pattern_list(&mut self, depth: usize) -> Vec<Pattern> {
        let mut items = Vec::new();
        self.delim_depth += 1;
        self.expect_bump(TokenKind::LeftParen, "'('");
        loop {
            self.skip_soft();
            if self.at(TokenKind::RightParen) {
                self.bump();
                break;
            }
            if self.cursor.eof() {
                self.report_expected("')'", self.describe_current(), self.cursor.peek_span());
                break;
            }
            if self.break_on_block_close("')'") {
                break;
            }
            let p = self.parse_pattern_d(depth + 1);
            items.push(p);
            self.skip_soft();
            if self.at(TokenKind::Comma) {
                self.bump();
                continue;
            }
            if self.at(TokenKind::RightParen) {
                self.bump();
                break;
            }
            if self.break_on_block_close("')'") {
                break;
            }
            self.report_expected(
                "',' or ')'",
                self.describe_current(),
                self.cursor.peek_span(),
            );
            break;
        }
        self.delim_depth -= 1;
        items
    }

    fn parse_tuple_pattern(&mut self, depth: usize, start: SourceSpan) -> Pattern {
        self.cursor.cst.start(SyntaxKind::TUPLE_PATTERN);
        let items = self.parse_paren_pattern_list(depth + 1);
        let span = self.cover(start, self.start_span());
        self.cursor.cst.finish(SyntaxKind::TUPLE_PATTERN);
        Pattern {
            kind: PatternKind::Tuple(items),
            span,
        }
    }

    fn parse_struct_pattern_fields(&mut self, depth: usize) -> Vec<FieldPattern> {
        let mut fields = Vec::new();
        let mut has_rest = false;
        self.delim_depth += 1;
        self.expect_bump(TokenKind::LeftBrace, "'{'");
        loop {
            self.skip_soft();
            if self.at(TokenKind::RightBrace) {
                self.bump();
                break;
            }
            if self.cursor.eof() {
                self.report_expected("'}'", self.describe_current(), self.cursor.peek_span());
                break;
            }
            if self.at(TokenKind::DotDot) {
                self.cursor.cst.start(SyntaxKind::REST_PATTERN);
                self.bump();
                self.cursor.cst.finish(SyntaxKind::REST_PATTERN);
                if has_rest {
                    let sp = self.start_span();
                    self.report(
                        PARSE_DUPLICATE_REST_PATTERN,
                        sp,
                        "parser.duplicate_rest_pattern",
                        "only one `..` rest is allowed per struct pattern (§267)".to_string(),
                    );
                }
                has_rest = true;
                self.skip_soft();
                if self.at(TokenKind::Comma) {
                    self.bump();
                }
                continue;
            }
            let fstart = self.start_span();
            let name = self.expect_ident("field name");
            let mut pattern = None;
            if self.at(TokenKind::Colon) {
                self.bump();
                pattern = Some(self.parse_pattern_d(depth + 1));
            }
            let span = pattern
                .as_ref()
                .map(|p| self.cover(fstart, p.span))
                .unwrap_or(self.cover(fstart, name.span));
            fields.push(FieldPattern {
                name,
                pattern,
                span,
            });
            self.skip_soft();
            if self.at(TokenKind::Comma) {
                self.bump();
                continue;
            }
            if self.at(TokenKind::RightBrace) {
                self.bump();
                break;
            }
            self.report_expected(
                "',' or '}'",
                self.describe_current(),
                self.cursor.peek_span(),
            );
            break;
        }
        self.delim_depth -= 1;
        let _ = has_rest;
        fields
    }

    // ------------------------------------------------------------------
    // decodificação de literais (§200-202)
    // ------------------------------------------------------------------

    fn decode_int(&mut self, text: &str, span: SourceSpan) -> i64 {
        let clean = strip_suffix(text);
        let cleaned: String = clean.chars().filter(|&c| c != '_').collect();
        let (radix, digits) = if let Some(rest) = cleaned
            .strip_prefix("0x")
            .or_else(|| cleaned.strip_prefix("0X"))
        {
            (16, rest)
        } else if let Some(rest) = cleaned
            .strip_prefix("0o")
            .or_else(|| cleaned.strip_prefix("0O"))
        {
            (8, rest)
        } else if let Some(rest) = cleaned
            .strip_prefix("0b")
            .or_else(|| cleaned.strip_prefix("0B"))
        {
            (2, rest)
        } else {
            (10, cleaned.as_str())
        };
        if digits.is_empty() {
            self.report(
                PARSE_INVALID_EXPRESSION,
                span,
                "parser.invalid_integer_literal",
                "invalid integer literal".to_string(),
            );
            return 0;
        }
        match u64::from_str_radix(digits, radix) {
            Ok(v) => v as i64,
            Err(_) => {
                self.report(
                    PARSE_INVALID_EXPRESSION,
                    span,
                    "parser.integer_literal_out_of_range",
                    "integer literal out of range".to_string(),
                );
                0
            }
        }
    }

    fn decode_float(&mut self, text: &str, span: SourceSpan) -> f64 {
        let clean = strip_suffix(text);
        let cleaned: String = clean.chars().filter(|&c| c != '_').collect();
        match cleaned.parse::<f64>() {
            Ok(v) => v,
            Err(_) => {
                self.report(
                    PARSE_INVALID_EXPRESSION,
                    span,
                    "parser.invalid_float_literal",
                    "invalid float literal".to_string(),
                );
                0.0
            }
        }
    }

    fn decode_char(&mut self, text: &str, span: SourceSpan) -> char {
        let inner = text.get(1..text.len().saturating_sub(1)).unwrap_or("");
        let decoded = decode_escape_or_text(inner);
        match decoded.chars().count() {
            1 => decoded.chars().next().unwrap_or('\0'),
            _ => {
                self.report(
                    PARSE_INVALID_EXPRESSION,
                    span,
                    "parser.invalid_char_literal",
                    "invalid char literal".to_string(),
                );
                '\0'
            }
        }
    }
}

// ----------------------------------------------------------------------
// helpers não-mutativos
// ----------------------------------------------------------------------

fn kind_span(kind: &ItemKind) -> SourceSpan {
    match kind {
        ItemKind::Function(d) => d.span,
        ItemKind::Action(d) => d.span,
        ItemKind::Struct(d) => d.span,
        ItemKind::Enum(d) => d.span,
        ItemKind::Interface(d) => d.span,
        ItemKind::Implement(d) => d.span,
        ItemKind::TypeAlias(d) => d.span,
        ItemKind::Const(d) => d.span,
    }
}

fn last_type_span(types: &[Type], fallback: SourceSpan) -> SourceSpan {
    types.last().map(|t| t.span).unwrap_or(fallback)
}

fn fields_types_span(fields: &[StructField], fallback: SourceSpan) -> SourceSpan {
    fields.last().map(|f| f.span).unwrap_or(fallback)
}

fn split_pattern_path(segments: Vec<Ident>, _start: SourceSpan) -> (QualifiedName, Ident) {
    let mut path_segments = segments;
    let last = path_segments.pop();
    let empty_span = last
        .as_ref()
        .map(|s| s.span)
        .unwrap_or_else(|| SourceSpan::point(nexa_source::SourceId(0), 0));
    let path = QualifiedName {
        segments: path_segments,
        span: empty_span,
    };
    let variant = last.unwrap_or(Ident {
        name: "_".to_string(),
        span: empty_span,
    });
    (path, variant)
}

fn collapse_payload(items: Vec<Pattern>) -> Option<Box<Pattern>> {
    match items.len() {
        0 => None,
        1 => Some(Box::new(items.into_iter().next().unwrap())),
        _ => {
            let mut it = items;
            let first = it.remove(0);
            let span = first.span;
            let rest = it;
            Some(Box::new(Pattern {
                kind: PatternKind::Tuple(rest),
                span,
            }))
        }
    }
}

fn is_comparison_op(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge | BinaryOp::Eq | BinaryOp::Ne
    )
}

const NUMERIC_SUFFIXES: [&str; 10] = [
    "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f32", "f64",
];

fn strip_suffix(text: &str) -> &str {
    for s in NUMERIC_SUFFIXES {
        if let Some(rest) = text.strip_suffix(s) {
            return rest;
        }
    }
    text
}

fn decode_raw_string(text: &str) -> String {
    if text.len() >= 3 {
        if let Some(inner) = text
            .strip_prefix("r")
            .and_then(|t| t.strip_prefix('"'))
            .and_then(|t| t.strip_suffix('"'))
        {
            return inner.to_string();
        }
    }
    text.get(2..text.len().saturating_sub(2))
        .unwrap_or("")
        .to_string()
}

fn decode_byte_string(text: &str) -> Vec<u8> {
    let inner = text
        .strip_prefix("b")
        .and_then(|t| t.strip_prefix('"'))
        .and_then(|t| t.strip_suffix('"'))
        .unwrap_or("");
    let mut out: Vec<u8> = Vec::new();
    decode_escapes_into_bytes(inner, &mut out);
    out
}

fn decode_escapes_into_bytes(s: &str, out: &mut Vec<u8>) {
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('\\') => out.push(b'\\'),
                Some('"') => out.push(b'"'),
                Some('n') => out.push(b'\n'),
                Some('r') => out.push(b'\r'),
                Some('t') => out.push(b'\t'),
                Some('0') => out.push(b'\0'),
                Some('u') => {
                    // \u{...}
                    let mut hex = String::new();
                    if chars.peek() == Some(&'{') {
                        chars.next();
                        for _ in 0..8 {
                            match chars.peek() {
                                Some(&ch) if ch.is_ascii_hexdigit() => {
                                    hex.push(ch);
                                    chars.next();
                                }
                                Some(&'}') => {
                                    chars.next();
                                    break;
                                }
                                _ => break,
                            }
                        }
                    }
                    if let Ok(v) = u32::from_str_radix(&hex, 16) {
                        if let Some(ch) = char::from_u32(v) {
                            let mut buf = [0u8; 4];
                            let s = ch.encode_utf8(&mut buf);
                            out.extend_from_slice(s.as_bytes());
                        }
                    }
                }
                Some(other) => {
                    out.push(b'\\');
                    out.extend(other.to_string().as_bytes());
                }
                None => out.push(b'\\'),
            }
        } else {
            out.extend(c.to_string().as_bytes());
        }
    }
}

fn decode_escapes(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('\\') => out.push('\\'),
                Some('"') => out.push('"'),
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('0') => out.push('\0'),
                Some('u') => {
                    let mut hex = String::new();
                    if chars.peek() == Some(&'{') {
                        chars.next();
                        for _ in 0..8 {
                            match chars.peek() {
                                Some(&ch) if ch.is_ascii_hexdigit() => {
                                    hex.push(ch);
                                    chars.next();
                                }
                                Some(&'}') => {
                                    chars.next();
                                    break;
                                }
                                _ => break,
                            }
                        }
                    }
                    if let Ok(v) = u32::from_str_radix(&hex, 16) {
                        if let Some(ch) = char::from_u32(v) {
                            out.push(ch);
                        }
                    }
                }
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn decode_escape_or_text(s: &str) -> String {
    decode_escapes(s)
}
