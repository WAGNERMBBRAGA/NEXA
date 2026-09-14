//! Motor de análise de fluxo sobre um módulo tipado (Implementação 05 §311-408).
//!
//! Para cada callable com corpo (function/action) do module, constrói o CFG e
//! roda:
//!
//! * retorno (MissingReturn, FLOW-0001);
//! * código inalcançável (UnreachableCode, FLOW-0002, warning);
//! * break/continue inválidos (FLOW-0003/0004);
//! * definite assignment (FLOW-0005/0006).
//!
//! A fronteira é agnóstica de resolver/typecheck: recebe closures de resolução
//! span→symbol e span→tipo.

use crate::analysis::{analyze_reachability, check_missing_return, unreachable_code_diagnostics};
use crate::builder::build_callable_cfg;
use crate::cfg::CfgId;
use crate::definite::check_definite_assignment_diagnostics;
use crate::diagnostics::{FlowDiagnostic, FlowDiagnosticCode};
use nexa_ast::{ItemKind, PatternKind, SourceUnit};
use nexa_source::SourceSpan;
use nexa_symbols::SymbolId;
use nexa_types::id::TypeId;
use nexa_types::ty::CallableType;

/// Resultado da análise de fluxo de um módulo.
#[derive(Debug, Clone, Default)]
pub struct FlowReport {
    pub diagnostics: Vec<FlowDiagnostic>,
    /// Número de CFGs construídos (um por callable com corpo).
    pub cfg_count: usize,
}

/// Analisa um módulo tipado completo.
///
/// * `resolve_span` — span→symbol (ex.: `SemanticIndex::resolve_debug`).
/// * `expr_type` — span de expressão → `TypeId` (ex.: modelo tipado).
/// * `callable_return_type` — symbol de callable → assinatura (ex.:
///   `TypedSemanticModel::callable_signatures`).
/// * `never_ty`/`unit_ty` — ids do prelude para distinguir retorno real.
pub fn analyze_typed_module(
    ast: &SourceUnit,
    resolve_span: &dyn Fn(SourceSpan) -> Option<SymbolId>,
    expr_type: &dyn Fn(SourceSpan) -> Option<TypeId>,
    callable_return_type: &dyn Fn(SymbolId) -> Option<CallableType>,
    never_ty: TypeId,
    unit_ty: TypeId,
) -> FlowReport {
    let mut diagnostics = Vec::new();
    let mut cfg_count = 0usize;

    for item in &ast.items {
        let (symbol, body, span) = match &item.kind {
            ItemKind::Function(f) => {
                let Some(sym) = resolve_span(f.name.span) else {
                    continue;
                };
                (sym, &f.body, f.span)
            }
            ItemKind::Action(a) => {
                let Some(sym) = resolve_span(a.name.span) else {
                    continue;
                };
                (sym, &a.body, a.span)
            }
            _ => continue,
        };

        let params: Vec<SymbolId> = if let ItemKind::Function(f) = &item.kind {
            f.params
                .iter()
                .filter_map(|p| resolve_span(p.name.span))
                .collect()
        } else if let ItemKind::Action(a) = &item.kind {
            a.params
                .iter()
                .filter_map(|p| resolve_span(p.name.span))
                .collect()
        } else {
            Vec::new()
        };

        let return_ty = callable_return_type(symbol)
            .map(|c| c.return_type)
            .unwrap_or(unit_ty);
        let has_return_type = return_ty != unit_ty && return_ty != never_ty;

        let built = build_callable_cfg(
            CfgId(cfg_count as u32),
            symbol,
            body,
            &params,
            resolve_span,
            expr_type,
            never_ty,
        );
        cfg_count += 1;

        let info = analyze_reachability(&built.cfg);
        diagnostics.extend(check_missing_return(
            &built.cfg,
            &info,
            has_return_type,
            Some(span),
        ));
        diagnostics.extend(unreachable_code_diagnostics(&built.cfg));
        diagnostics.extend(check_definite_assignment_diagnostics(
            &built.cfg,
            &built.locals,
        ));
        diagnostics.extend(built.build_diagnostics);
    }

    // --- match exhaustiveness (FLOW-0008/0009) ---
    // Versão conservadora: percorre o corpo da função em busca de expressões `match`.
    // Se houver arm com `_` (wildcard), braços subsequentes são inalcançáveis (FLOW-0008).
    // Se não houver wildcard, reportar non-exaustivo (FLOW-0009) para matches
    // sobre tipos simples (bool, integers) sem precisar de construtores de enum.
    for item in &ast.items {
        if let ItemKind::Function(f) = &item.kind {
            // Busca expressões `match` no corpo da função.
            let mut found_match = false;
            for stmt in &f.body.stmts {
                if found_match {
                    break;
                }
                if let nexa_ast::StmtKind::Expr(nexa_ast::Expr {
                    kind: nexa_ast::ExprKind::Match(match_expr),
                    ..
                }) = &stmt.kind
                {
                    found_match = true;
                    // Verifica se algum arm tem padrão Wildcard `_`.
                    let mut has_wildcard = false;
                    for arm in &match_expr.arms {
                        if matches!(arm.pattern.kind, PatternKind::Wildcard) {
                            has_wildcard = true;
                            // Braços após o wildcard são inalcançáveis (FLOW-0008).
                            let idx = match_expr
                                .arms
                                .iter()
                                .position(|a| matches!(a.pattern.kind, PatternKind::Wildcard))
                                .unwrap();
                            for arm in &match_expr.arms[idx + 1..] {
                                diagnostics.push(FlowDiagnostic::new(
                                        FlowDiagnosticCode::UnreachableMatchArm,
                                        format!("match arm {} is unreachable (wildcard already matched)", idx + 1),
                                    ).with_span(arm.span));
                            }
                            break;
                        }
                    }
                    if !has_wildcard {
                        // Conservador: reportar non-exaustivo (FLOW-0009) para matches
                        // sobre tipos que sabemos serem finitos sem precisar de construtores.
                        // Bool: precisa de true/false ou wildcard.
                        // Integers pequenos: pode ser conservador.
                        // Apenas reportamos se houver 2 ou menos arms e nenhum wildcard.
                        if match_expr.arms.len() <= 2 {
                            diagnostics.push(
                                FlowDiagnostic::new(
                                    FlowDiagnosticCode::NonExhaustiveMatch,
                                    format!(
                                        "match is not exhaustive (no wildcard, {} arms)",
                                        match_expr.arms.len()
                                    ),
                                )
                                .with_span(match_expr.span),
                            );
                        }
                    }
                }
            }
        }
    }

    diagnostics.sort_by_key(|d| d.span.map(|s| s.start).unwrap_or(0));
    FlowReport {
        diagnostics,
        cfg_count,
    }
}

/// Conta diagnósticos de erro numa lista de fluxo (para a fronteira de check).
pub fn flow_error_count(diagnostics: &[FlowDiagnostic]) -> usize {
    diagnostics
        .iter()
        .filter(|d| d.code.severity() == nexa_diagnostics::Severity::Error)
        .count()
}

/// Conta warnings de fluxo.
pub fn flow_warning_count(diagnostics: &[FlowDiagnostic]) -> usize {
    diagnostics
        .iter()
        .filter(|d| d.code.severity() == nexa_diagnostics::Severity::Warning)
        .count()
}

/// Converte para a lista de códigos (para testes e CTS).
pub fn flow_codes(diagnostics: &[FlowDiagnostic]) -> Vec<&'static str> {
    diagnostics.iter().map(|d| d.code.as_str()).collect()
}

/// Códigos da fatia 1 já implementados (baseline §38).
pub const FLOW_CODES_IMPLEMENTED: [&str; 6] = [
    "NEXA-FLOW-0001",
    "NEXA-FLOW-0002",
    "NEXA-FLOW-0003",
    "NEXA-FLOW-0004",
    "NEXA-FLOW-0005",
    "NEXA-FLOW-0006",
];

pub fn flow_code_label(code: FlowDiagnosticCode) -> &'static str {
    code.as_str()
}
