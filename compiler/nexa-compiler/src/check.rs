//! Check pipeline (Implementação 04 — type checker).
//!
//! Extensão do `Pipeline` até o fim da verificação de tipos:
//!
//! ```text
//! bytes → UTF-8 validation → SourceFile → Lexer → Parser (SingleFile)
//!      → Resolver (single-module mode, §459) → SemanticIndex
//!      → TypeChecker (index-aware) → TypedSemanticModel + type diagnostics
//!      → output (human | json, debug tooling §464-465)
//! ```
//!
//! O JSON é output **de debug** (§464), não formato normativo persistente
//! (§465). O modo multi-module usa o harness de biblioteca (§460-462).

use crate::{Diagnostic, Pipeline, SCHEMA_VERSION};
use nexa_ast::{Expr, ExprKind, ItemKind, Stmt, StmtKind};
use nexa_effects::{CallEdge, DispatchKind, EffectAnalyzer, EffectDiagnostic};
use nexa_parser::{parse, ParseMode};
use nexa_resolver::resolver::{DiagnosticSeverity as ResolveSeverity, ResolveDiagnostic};
use nexa_source::{SourceId, SourceLoadError, SourceManager, SourceSpan};
use nexa_symbols::{ModuleId, SymbolId};
use nexa_typecheck::{diagnostics::TypeDiagnostic, TypeChecker};
use serde::Serialize;
use std::path::Path;

/// Resultado da fronteira `check`: modelo semântico tipado + diagnostics
/// (parse, semânticos, de tipo e de fluxo) mesclados com o pipeline de fontes.
pub struct TypeCheckResult {
    pub source_id: SourceId,
    /// Diagnostics do parser (validação de sintaxe).
    pub parse_diagnostics: Vec<Diagnostic>,
    /// Diagnostics do resolver (scopes, nomes, visibilidade, imports).
    pub semantic_diagnostics: Vec<ResolveDiagnostic>,
    /// Type checker com índice semântico, modelo tipado e diagnostics de tipo
    /// (`typed.index` dá acesso ao `SemanticIndex`, `typed.semantic` ao
    /// modelo `spans → tipo`).
    pub typed: TypeChecker,
    /// Diagnostics da análise de fluxo (Implementação 05): `NEXA-FLOW-XXXX`.
    pub flow_diagnostics: Vec<nexa_flow::FlowDiagnostic>,
    /// Número de CFGs construídos (um por callable com corpo).
    pub flow_cfg_count: usize,
    /// Diagnostics do modelo de efeitos (Implementação 05): `NEXA-EFFECT-XXXX`.
    pub effect_diagnostics: Vec<EffectDiagnostic>,
    /// Número de arestas do grafo de chamadas usadas na propagação de efeitos.
    pub effect_call_edge_count: usize,
}

/// Roda a análise de fluxo sobre um módulo tipado.
///
/// Anti-cascata: só produz resultados quando o módulo não tem erros de tipo
/// (um corpo mal tipado pode gerar falsos positivos de fluxo).
fn run_flow(
    ast: &nexa_ast::SourceUnit,
    typed: &TypeChecker,
) -> (Vec<nexa_flow::FlowDiagnostic>, usize) {
    use nexa_typecheck::diagnostics::DiagnosticSeverity as TypeSeverity;

    if typed
        .diagnostics
        .iter()
        .any(|d| d.severity == TypeSeverity::Error)
    {
        return (Vec::new(), 0);
    }

    let resolve_span = |span: SourceSpan| {
        typed
            .symbol_lookup(span)
            .or_else(|| typed.index.resolve_debug(span))
    };
    let expr_type = |span: SourceSpan| typed.semantic.expression_info.get(span).map(|i| i.ty);
    let callable_return = |sym: SymbolId| typed.semantic.callable_signatures.get(sym).cloned();

    let report = nexa_flow::analyze_typed_module(
        ast,
        &resolve_span,
        &expr_type,
        &callable_return,
        typed.prelude.never,
        typed.prelude.unit,
    );
    (report.diagnostics, report.cfg_count)
}

/// Roda o modelo de efeitos (Implementação 05) sobre um módulo tipado.
///
/// Ponte de fonte → `nexa_effects::EffectAnalyzer`:
/// 1. cada callable é registrado (function: puro; action: cláusula `effects`
///    declarada, resolvida contra o registry padrão);
/// 2. as chamadas de corpo viram arestas do grafo de chamadas;
/// 3. fixpoint propaga os efeitos; validações emitem `NEXA-EFFECT-XXXX`.
///
/// Anti-cascata idêntica à do fluxo: sem erros de tipo, o corpo é confiável.
fn run_effects(ast: &nexa_ast::SourceUnit, typed: &TypeChecker) -> (Vec<EffectDiagnostic>, usize) {
    use nexa_typecheck::diagnostics::DiagnosticSeverity as TypeSeverity;

    if typed
        .diagnostics
        .iter()
        .any(|d| d.severity == TypeSeverity::Error)
    {
        return (Vec::new(), 0);
    }

    let resolve_span = |span: SourceSpan| {
        typed
            .symbol_lookup(span)
            .or_else(|| typed.index.resolve_debug(span))
    };

    let mut analyzer = EffectAnalyzer::new();
    let mut is_function: std::collections::HashMap<SymbolId, bool> = Default::default();
    let mut exported: std::collections::HashMap<SymbolId, bool> = Default::default();

    // Passo 1 — registra callables e efeitos diretos declarados.
    for item in &ast.items {
        let (name, _body, clause, function, exp) = match &item.kind {
            ItemKind::Function(f) => (&f.name, &f.body, None, true, item.exported),
            ItemKind::Action(a) => (&a.name, &a.body, a.effects.as_ref(), false, item.exported),
            _ => continue,
        };
        let Some(sym) = resolve_span(name.span) else {
            continue;
        };
        is_function.insert(sym, function);
        exported.insert(sym, exp);

        let declared = clause.map(|clause| {
            let paths: Vec<String> = clause
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
            match analyzer.registry.resolve_set(&paths) {
                Ok(set) => {
                    for id in set.iter() {
                        analyzer.record_direct_effect(sym, id);
                    }
                    Some(set)
                }
                Err(unknown) => {
                    for path in unknown {
                        analyzer.diagnostics.push(EffectDiagnostic::new(
                            nexa_effects::EffectDiagnosticCode::UnknownEffect,
                            format!("unknown effect '{path}'"),
                            clause.span,
                        ));
                    }
                    None
                }
            }
        });
        analyzer.register_callable(sym, declared.flatten(), name.span);
    }

    // Passo 2 — arestas de chamada a partir dos corpos.
    for item in &ast.items {
        let (name, body, function) = match &item.kind {
            ItemKind::Function(f) => (&f.name, &f.body, true),
            ItemKind::Action(a) => (&a.name, &a.body, false),
            _ => continue,
        };
        let Some(caller_sym) = resolve_span(name.span) else {
            continue;
        };
        let mut calls: Vec<SourceSpan> = Vec::new();
        walk_block(body, &mut calls);
        for call_span in calls {
            if let Some(callee_sym) = resolve_span(call_span) {
                let callee_is_function = is_function.get(&callee_sym).copied();
                if let Some(callee_is_function) = callee_is_function {
                    analyzer.call_graph.add_edge(CallEdge {
                        caller: caller_sym,
                        callee: callee_sym,
                        span: call_span,
                        dispatch: DispatchKind::Direct,
                    });
                    if function && !callee_is_function {
                        analyzer
                            .validate_action_call_from_function(caller_sym, true, true, call_span);
                    }
                }
            }
        }
    }

    // Passo 3 — fixpoint + validações por callable.
    analyzer.propagate_fixpoint();
    for (sym, function) in &is_function {
        if *function {
            analyzer.validate_function_purity(*sym);
        } else {
            analyzer.validate_action_declaration(*sym, *exported.get(sym).unwrap_or(&false));
        }
    }

    let edge_count = analyzer.call_graph.edge_count();
    (analyzer.diagnostics, edge_count)
}

/// Coleta os spans de callee das chamadas diretas (`f(...)`) num corpo,
/// recursivamente através de statements, blocos e expressões aninhadas.
fn walk_block(block: &nexa_ast::Block, out: &mut Vec<SourceSpan>) {
    for stmt in &block.stmts {
        walk_stmt(stmt, out);
    }
}

fn walk_stmt(stmt: &Stmt, out: &mut Vec<SourceSpan>) {
    match &stmt.kind {
        StmtKind::Let(b) => walk_expr(&b.init, out),
        StmtKind::Var(b) => {
            if let Some(init) = &b.init {
                walk_expr(init, out);
            }
        }
        StmtKind::Const(b) => walk_expr(&b.init, out),
        StmtKind::Assign(_, _, expr) => walk_expr(expr, out),
        StmtKind::Expr(e) => walk_expr(e, out),
        StmtKind::Return(r) => {
            if let Some(v) = &r.value {
                walk_expr(v, out);
            }
        }
        StmtKind::Break(b) => {
            if let Some(v) = &b.value {
                walk_expr(v, out);
            }
        }
        StmtKind::Continue(_) | StmtKind::Discard(_) => {}
    }
}

fn walk_expr(expr: &Expr, out: &mut Vec<SourceSpan>) {
    match &expr.kind {
        ExprKind::Call { callee, args } => {
            out.push(callee.span);
            walk_expr(callee, out);
            for a in args {
                walk_expr(a, out);
            }
        }
        ExprKind::Binary(_, l, r) => {
            walk_expr(l, out);
            walk_expr(r, out);
        }
        ExprKind::Unary(_, inner) => walk_expr(inner, out),
        ExprKind::Index { target, index } => {
            walk_expr(target, out);
            walk_expr(index, out);
        }
        ExprKind::Field { target, .. } => walk_expr(target, out),
        ExprKind::MethodCall { target, args, .. } => {
            walk_expr(target, out);
            for a in args {
                walk_expr(a, out);
            }
        }
        ExprKind::Await(inner)
        | ExprKind::Try(inner)
        | ExprKind::Ref(inner)
        | ExprKind::RefMut(inner)
        | ExprKind::Move(inner)
        | ExprKind::Paren(inner) => walk_expr(inner, out),
        ExprKind::If(i) => {
            walk_expr(&i.condition, out);
            walk_block(&i.then_block, out);
            for ei in &i.else_ifs {
                walk_expr(&ei.condition, out);
                walk_block(&ei.block, out);
            }
            if let Some(eb) = &i.else_block {
                walk_block(eb, out);
            }
        }
        ExprKind::Match(m) => {
            walk_expr(&m.scrutinee, out);
            for arm in &m.arms {
                if let Some(g) = &arm.guard {
                    walk_expr(g, out);
                }
                walk_expr(&arm.body, out);
            }
        }
        ExprKind::For(f) => {
            walk_expr(&f.iterable, out);
            walk_block(&f.body, out);
        }
        ExprKind::While(w) => {
            walk_expr(&w.condition, out);
            walk_block(&w.body, out);
        }
        ExprKind::Loop(l) => walk_block(&l.body, out),
        ExprKind::Block(b) | ExprKind::Unsafe(b) => walk_block(b, out),
        ExprKind::Tuple(items) | ExprKind::Array(items) => {
            for i in items {
                walk_expr(i, out);
            }
        }
        ExprKind::StructConstruct(sc) => {
            for f in &sc.fields {
                walk_expr(&f.expr, out);
            }
        }
        ExprKind::AssignExpr(_, _, v) => walk_expr(v, out),
        ExprKind::Ident(_)
        | ExprKind::Path(_)
        | ExprKind::Underscore
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::StringLiteral(_)
        | ExprKind::ByteStringLiteral(_)
        | ExprKind::CharLiteral(_)
        | ExprKind::BoolLiteral(_) => {}
    }
}

impl TypeCheckResult {
    pub fn has_errors(&self) -> bool {
        self.error_count() > 0
    }

    pub fn error_count(&self) -> usize {
        self.parse_diagnostics
            .iter()
            .filter(|d| d.severity == crate::Severity::Error)
            .count()
            + self
                .semantic_diagnostics
                .iter()
                .filter(|d| d.severity == ResolveSeverity::Error)
                .count()
            + self
                .typed
                .diagnostics
                .iter()
                .filter(|d| d.severity == nexa_typecheck::diagnostics::DiagnosticSeverity::Error)
                .count()
            + self
                .flow_diagnostics
                .iter()
                .filter(|d| d.code.severity() == nexa_diagnostics::Severity::Error)
                .count()
            + self
                .effect_diagnostics
                .iter()
                .filter(|d| d.code.is_error())
                .count()
    }

    pub fn warning_count(&self) -> usize {
        self.parse_diagnostics
            .iter()
            .filter(|d| d.severity == crate::Severity::Warning)
            .count()
            + self
                .semantic_diagnostics
                .iter()
                .filter(|d| d.severity == ResolveSeverity::Warning)
                .count()
            + self
                .typed
                .diagnostics
                .iter()
                .filter(|d| d.severity == nexa_typecheck::diagnostics::DiagnosticSeverity::Warning)
                .count()
            + self
                .flow_diagnostics
                .iter()
                .filter(|d| d.code.severity() == nexa_diagnostics::Severity::Warning)
                .count()
            + self
                .effect_diagnostics
                .iter()
                .filter(|d| !d.code.is_error())
                .count()
    }
}

impl Pipeline {
    /// Carrega um texto nomeado, parseia (SingleFile), resolve e typechecka o
    /// módulo único.
    pub fn check_source(&mut self, name: &str, content: &str) -> TypeCheckResult {
        let id = self.sources.load_text(name.into(), content.to_owned());
        let source = self.sources.source(id).expect("just loaded source");
        let parse_result = parse(source, ParseMode::SingleFile);
        let resolve_result = nexa_resolver::resolve(id, &parse_result.ast);
        let mut typed = TypeChecker::with_index(resolve_result.index, ModuleId(0), id);
        typed.check_module(&parse_result.ast);
        let (flow_diagnostics, flow_cfg_count) = run_flow(&parse_result.ast, &typed);
        let (effect_diagnostics, effect_call_edge_count) = run_effects(&parse_result.ast, &typed);
        TypeCheckResult {
            source_id: id,
            parse_diagnostics: parse_result.diagnostics,
            semantic_diagnostics: resolve_result.diagnostics,
            typed,
            flow_diagnostics,
            flow_cfg_count,
            effect_diagnostics,
            effect_call_edge_count,
        }
    }

    /// Fronteira real de check a partir de **bytes** (análogo a
    /// `resolve_bytes`): valida UTF-8 e, em bytes inválidos, retorna o
    /// diagnóstico de loading na fronteira (NEXA-LEX-0001).
    #[allow(clippy::result_large_err)]
    pub fn check_bytes(
        &mut self,
        display_path: &Path,
        bytes: Vec<u8>,
    ) -> Result<TypeCheckResult, Diagnostic> {
        match self.sources.load_bytes(display_path.to_owned(), bytes) {
            Ok(id) => {
                let source = self.sources.source(id).expect("just loaded source");
                let parse_result = parse(source, ParseMode::SingleFile);
                let resolve_result = nexa_resolver::resolve(id, &parse_result.ast);
                let mut typed = TypeChecker::with_index(resolve_result.index, ModuleId(0), id);
                typed.check_module(&parse_result.ast);
                let (flow_diagnostics, flow_cfg_count) = run_flow(&parse_result.ast, &typed);
                let (effect_diagnostics, effect_call_edge_count) =
                    run_effects(&parse_result.ast, &typed);
                Ok(TypeCheckResult {
                    source_id: id,
                    parse_diagnostics: parse_result.diagnostics,
                    semantic_diagnostics: resolve_result.diagnostics,
                    typed,
                    flow_diagnostics,
                    flow_cfg_count,
                    effect_diagnostics,
                    effect_call_edge_count,
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

/// Rótulo estável de severidade de um diagnóstico de tipo (debug/CLI).
pub fn type_severity_label(
    severity: nexa_typecheck::diagnostics::DiagnosticSeverity,
) -> &'static str {
    match severity {
        nexa_typecheck::diagnostics::DiagnosticSeverity::Error => "error",
        nexa_typecheck::diagnostics::DiagnosticSeverity::Warning => "warning",
        nexa_typecheck::diagnostics::DiagnosticSeverity::Note => "note",
    }
}

// ---------------------------------------------------------------------------
// Output de debug (JSON, §464).
// ---------------------------------------------------------------------------

/// Envelope `nexa check --format json` (debug tooling, §464-465).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckOutputEnvelope {
    pub schema_version: u32,
    pub source: Option<String>,
    pub parse_diagnostics: Vec<serde_json::Value>,
    pub semantic_diagnostics: Vec<serde_json::Value>,
    pub type_diagnostics: Vec<serde_json::Value>,
    pub flow_diagnostics: Vec<serde_json::Value>,
    pub effect_diagnostics: Vec<serde_json::Value>,
    pub summary: serde_json::Value,
}

fn type_diagnostic_value(d: &TypeDiagnostic) -> serde_json::Value {
    serde_json::json!({
        "code": d.code.code_str(),
        "severity": type_severity_label(d.severity),
        "message": d.message,
        "span": {
            "source": d.span.source.0,
            "start": d.span.start,
            "end": d.span.end,
        },
        "context": d.context,
    })
}

fn flow_diagnostic_value(d: &nexa_flow::FlowDiagnostic) -> serde_json::Value {
    serde_json::json!({
        "code": d.code.as_str(),
        "severity": flow_severity_label(d.code.severity()),
        "message": d.message,
        "span": d.span.map(|span| serde_json::json!({
            "source": span.source.0,
            "start": span.start,
            "end": span.end,
        })),
    })
}

fn effect_diagnostic_value(d: &EffectDiagnostic) -> serde_json::Value {
    serde_json::json!({
        "code": d.code.code_str(),
        "severity": if d.code.is_error() { "error" } else { "warning" },
        "message": d.message,
        "span": {
            "source": d.span.source.0,
            "start": d.span.start,
            "end": d.span.end,
        },
    })
}

fn flow_severity_label(severity: nexa_diagnostics::Severity) -> &'static str {
    match severity {
        nexa_diagnostics::Severity::Error => "error",
        nexa_diagnostics::Severity::Warning => "warning",
        _ => "note",
    }
}

/// Constrói o envelope JSON do output do type checker (debug, §464).
pub fn check_output_json(result: &TypeCheckResult, sources: &SourceManager) -> CheckOutputEnvelope {
    let source = sources.source(result.source_id).map(|s| s.display_name());
    CheckOutputEnvelope {
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
            .map(semantic_diagnostic_value)
            .collect(),
        type_diagnostics: result
            .typed
            .diagnostics
            .iter()
            .map(type_diagnostic_value)
            .collect(),
        flow_diagnostics: result
            .flow_diagnostics
            .iter()
            .map(flow_diagnostic_value)
            .collect(),
        effect_diagnostics: result
            .effect_diagnostics
            .iter()
            .map(effect_diagnostic_value)
            .collect(),
        summary: serde_json::json!({
            "errors": result.error_count(),
            "warnings": result.warning_count(),
            "symbols": result.typed.index.symbols.count(),
            "typedExpressions": result.typed.semantic.expression_info.count(),
            "flowCfgs": result.flow_cfg_count,
            "effectCallEdges": result.effect_call_edge_count,
        }),
    }
}

fn semantic_diagnostic_value(d: &ResolveDiagnostic) -> serde_json::Value {
    serde_json::json!({
        "code": d.code,
        "severity": match d.severity {
            ResolveSeverity::Error => "error",
            ResolveSeverity::Warning => "warning",
            ResolveSeverity::Note => "note",
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

#[cfg(test)]
mod tests {
    use super::*;
    use nexa_typecheck::diagnostics::TypeDiagnosticCode;

    fn check(text: &str) -> TypeCheckResult {
        let mut pipeline = Pipeline::new();
        pipeline.check_source("<test>.nexa", text)
    }

    #[test]
    fn check_valid_module_reports_no_errors() {
        let result = check(
            r#"module main

enum Color {
    Red
    Green
}

function describe(c: Color) -> String {
    return match c {
        Color::Red => "red"
        Color::Green => "green"
    }
}

function add(a: Int, b: Int) -> Int {
    return a + b
}
"#,
        );
        assert!(
            !result.has_errors(),
            "valid module should pass: parse={:?} semantic={:?} type={:?}",
            result.parse_diagnostics,
            result.semantic_diagnostics,
            result.typed.diagnostics
        );
        assert!(result.typed.index.symbols.count() > 0, "symbols populated");
        assert!(
            result.typed.semantic.expression_info.count() > 0,
            "typed expressions populated"
        );
    }

    #[test]
    fn check_reports_type_error() {
        let result = check(
            r#"module main

function bad(a: Int) -> Int {
    return a + true
}
"#,
        );
        assert!(result.parse_diagnostics.is_empty());
        assert!(result.semantic_diagnostics.is_empty());
        assert!(result.has_errors(), "type error should fail the check");
        assert!(result.error_count() >= 1);
        let codes = result
            .typed
            .diagnostics
            .iter()
            .map(|d| d.code)
            .collect::<Vec<_>>();
        assert!(
            codes.contains(&TypeDiagnosticCode::InvalidOperatorOperands),
            "expected NEXA-TYPE-0009, got {:?}",
            codes
        );
    }

    #[test]
    fn check_json_envelope_is_well_formed() {
        let mut pipeline = Pipeline::new();
        let result = pipeline.check_source(
            "<test>.nexa",
            "module main\nfunction f(x: Int) -> Int {\n    return x + true\n}\n",
        );
        let envelope = check_output_json(&result, &pipeline.sources);
        let json = serde_json::to_value(&envelope).expect("serializable envelope");
        assert_eq!(json["schemaVersion"], 1);
        assert_eq!(json["source"], "<test>.nexa");
        assert!(!json["typeDiagnostics"].as_array().unwrap().is_empty());
        assert!(json["summary"]["errors"].as_u64().unwrap() >= 1);
    }

    #[test]
    fn flow_missing_return_detected() {
        let result = check(
            "module main\nfunction f(x: Int) -> Int {\n    if x > 0 {\n        return 1\n    }\n}\n",
        );
        let codes: Vec<&str> = result
            .flow_diagnostics
            .iter()
            .map(|d| d.code.as_str())
            .collect();
        assert!(
            codes.contains(&"NEXA-FLOW-0001"),
            "expected NEXA-FLOW-0001, got {:?}",
            codes
        );
        assert!(result.has_errors());
    }

    #[test]
    fn flow_never_function_no_missing_return() {
        let result = check("module main\nfunction boom() -> Never {\n    panic(\"x\")\n}\n");
        let has_missing = result
            .flow_diagnostics
            .iter()
            .any(|d| d.code.as_str() == "NEXA-FLOW-0001");
        assert!(!has_missing, "Never callable must not require a return");
    }

    #[test]
    fn flow_unreachable_code_warning() {
        let result = check(
            "module main\nfunction f(x: Int) -> Int {\n    return x\n    let y = 2\n    return y\n}\n",
        );
        let codes: Vec<&str> = result
            .flow_diagnostics
            .iter()
            .map(|d| d.code.as_str())
            .collect();
        assert!(
            codes.contains(&"NEXA-FLOW-0002"),
            "expected NEXA-FLOW-0002 warning, got {:?}",
            codes
        );
        assert!(
            !result.has_errors(),
            "unreachable code is a warning, not an error"
        );
    }

    #[test]
    fn flow_use_before_initialization() {
        let result = check("module main\nfunction f() -> Int {\n    var x: Int\n    return x\n}\n");
        let codes: Vec<&str> = result
            .flow_diagnostics
            .iter()
            .map(|d| d.code.as_str())
            .collect();
        assert!(
            codes.contains(&"NEXA-FLOW-0005"),
            "expected NEXA-FLOW-0005, got {:?}",
            codes
        );
        assert!(result.has_errors());
    }

    #[test]
    fn flow_possibly_uninitialized() {
        let result = check(
            "module main\nfunction f(c: Bool) -> Int {\n    var x: Int\n    if c {\n        x = 1\n    }\n    return x\n}\n",
        );
        let codes: Vec<&str> = result
            .flow_diagnostics
            .iter()
            .map(|d| d.code.as_str())
            .collect();
        assert!(
            codes.contains(&"NEXA-FLOW-0006"),
            "expected NEXA-FLOW-0006, got {:?}",
            codes
        );
    }

    #[test]
    fn flow_clean_callable_no_diagnostics() {
        let result =
            check("module main\nfunction add(a: Int, b: Int) -> Int {\n    return a + b\n}\n");
        assert!(
            result.flow_diagnostics.is_empty(),
            "clean callable must not produce flow diagnostics: {:?}",
            result.flow_diagnostics
        );
    }
}
