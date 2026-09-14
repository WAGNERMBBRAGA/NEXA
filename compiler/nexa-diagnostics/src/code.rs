//! `DiagnosticCode` — código estável e machine-readable.
//!
//! Nunca usar String livre como contrato de diagnóstico. Códigos são
//! constantes estáveis (`NEXA-<DOMAIN>-NNNN`) e não mudam com locale.

use serde::Serialize;
use std::fmt;

/// Código estável de um diagnóstico.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct DiagnosticCode(&'static str);

impl DiagnosticCode {
    pub const fn new(code: &'static str) -> Self {
        DiagnosticCode(code)
    }

    pub fn as_str(&self) -> &'static str {
        self.0
    }
}

impl fmt::Display for DiagnosticCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

// ---------------------------------------------------------------------------
// Lexer diagnostics obrigatórios (Implementação 01).
// Baseline fixo: NEXA-LEX-0001 .. NEXA-LEX-0006.
// Candidatos adicionados: 0007, 0008, 0009.
// NUNCA renumerar os existentes.
// ---------------------------------------------------------------------------

/// NEXA-LEX-0001 — source não é UTF-8 válido.
pub const LEX_INVALID_UTF8: DiagnosticCode = DiagnosticCode::new("NEXA-LEX-0001");
/// NEXA-LEX-0002 — token inválido (com `reason` estruturada).
pub const LEX_INVALID_TOKEN: DiagnosticCode = DiagnosticCode::new("NEXA-LEX-0002");
/// NEXA-LEX-0003 — string literal não terminada.
pub const LEX_UNTERMINATED_STRING: DiagnosticCode = DiagnosticCode::new("NEXA-LEX-0003");
/// NEXA-LEX-0004 — sequência de escape inválida (incl. `\u{...}` inválido).
pub const LEX_INVALID_ESCAPE: DiagnosticCode = DiagnosticCode::new("NEXA-LEX-0004");
/// NEXA-LEX-0005 — block comment não terminado.
pub const LEX_UNTERMINATED_COMMENT: DiagnosticCode = DiagnosticCode::new("NEXA-LEX-0005");
/// NEXA-LEX-0006 — literal numérico inválido.
pub const LEX_INVALID_NUMERIC_LITERAL: DiagnosticCode = DiagnosticCode::new("NEXA-LEX-0006");
/// NEXA-LEX-0007 — BOM inesperado no início do source (rejeitado em 1.0).
pub const LEX_UNEXPECTED_BOM: DiagnosticCode = DiagnosticCode::new("NEXA-LEX-0007");
/// NEXA-LEX-0008 — char literal inválido.
pub const LEX_INVALID_CHAR_LITERAL: DiagnosticCode = DiagnosticCode::new("NEXA-LEX-0008");
/// NEXA-LEX-0009 — interpolação não terminada.
pub const LEX_UNTERMINATED_INTERPOLATION: DiagnosticCode = DiagnosticCode::new("NEXA-LEX-0009");

/// Código reservado para internal compiler error (fronteira CLI → exit 101).
pub const ICE: DiagnosticCode = DiagnosticCode::new("NEXA-ICE");

/// Códigos de tool/host errors (ex.: arquivo não encontrado).
pub const TOOL_IO: DiagnosticCode = DiagnosticCode::new("NEXA-TOOL-0001");

// ---------------------------------------------------------------------------
// Parser diagnostics (Implementação 02).
// Baseline: NEXA-PARSE-0001 .. NEXA-PARSE-0015 (Impl 02 §328-331, §346, §423, §517).
// NUNCA renumerar os existentes.
// ---------------------------------------------------------------------------

/// NEXA-PARSE-0001 — token inesperado no contexto atual.
pub const PARSE_UNEXPECTED_TOKEN: DiagnosticCode = DiagnosticCode::new("NEXA-PARSE-0001");
/// NEXA-PARSE-0002 — token esperado não encontrado (args: expected, found).
pub const PARSE_EXPECTED_TOKEN: DiagnosticCode = DiagnosticCode::new("NEXA-PARSE-0002");
/// NEXA-PARSE-0003 — declaração inválida.
pub const PARSE_INVALID_DECLARATION: DiagnosticCode = DiagnosticCode::new("NEXA-PARSE-0003");
/// NEXA-PARSE-0004 — expressão inválida.
pub const PARSE_INVALID_EXPRESSION: DiagnosticCode = DiagnosticCode::new("NEXA-PARSE-0004");
/// NEXA-PARSE-0005 — pattern inválido.
pub const PARSE_INVALID_PATTERN: DiagnosticCode = DiagnosticCode::new("NEXA-PARSE-0005");
/// NEXA-PARSE-0006 — comparação/igualdade encadeada (não-associativas).
pub const PARSE_CHAINED_COMPARISON: DiagnosticCode = DiagnosticCode::new("NEXA-PARSE-0006");
/// NEXA-PARSE-0007 — alvo de atribuição inválido.
pub const PARSE_INVALID_ASSIGNMENT_TARGET: DiagnosticCode = DiagnosticCode::new("NEXA-PARSE-0007");
/// NEXA-PARSE-0008 — terminador de statement ausente (newline/`}`/EOF).
pub const PARSE_MISSING_STATEMENT_TERMINATOR: DiagnosticCode =
    DiagnosticCode::new("NEXA-PARSE-0008");
/// NEXA-PARSE-0009 — ordem de modificadores inválida (`export`/`async`).
pub const PARSE_INVALID_MODIFIER_ORDER: DiagnosticCode = DiagnosticCode::new("NEXA-PARSE-0009");
/// NEXA-PARSE-0010 — declaração callable inválida (ex.: effects em function, async function).
pub const PARSE_INVALID_CALLABLE_DECL: DiagnosticCode = DiagnosticCode::new("NEXA-PARSE-0010");
/// NEXA-PARSE-0011 — import inválido (ex.: wildcard, import após declaração).
pub const PARSE_INVALID_IMPORT: DiagnosticCode = DiagnosticCode::new("NEXA-PARSE-0011");
/// NEXA-PARSE-0012 — múltiplos `..` (rest) em um pattern.
pub const PARSE_DUPLICATE_REST_PATTERN: DiagnosticCode = DiagnosticCode::new("NEXA-PARSE-0012");
/// NEXA-PARSE-0013 — limite de profundidade de aninhamento excedido.
pub const PARSE_NESTING_LIMIT_EXCEEDED: DiagnosticCode = DiagnosticCode::new("NEXA-PARSE-0013");
/// NEXA-PARSE-0014 — múltiplas declarações `module`.
pub const PARSE_DUPLICATE_MODULE: DiagnosticCode = DiagnosticCode::new("NEXA-PARSE-0014");
/// NEXA-PARSE-0015 — limite de diagnostics por source excedido.
pub const PARSE_TOO_MANY_ERRORS: DiagnosticCode = DiagnosticCode::new("NEXA-PARSE-0015");

// ---------------------------------------------------------------------------
// Module diagnostics (Implementação 03).
// Baseline: NEXA-MODULE-0001 .. NEXA-MODULE-0007 (Impl 03 §102, §528, §642).
// NUNCA renumerar os existentes.
// ---------------------------------------------------------------------------

/// NEXA-MODULE-0001 — módulo duplicado no mesmo package (project build error).
pub const MODULE_DUPLICATE_MODULE: DiagnosticCode = DiagnosticCode::new("NEXA-MODULE-0001");
/// NEXA-MODULE-0002 — módulo de destino de import não encontrado.
pub const MODULE_UNKNOWN_MODULE: DiagnosticCode = DiagnosticCode::new("NEXA-MODULE-0002");
/// NEXA-MODULE-0003 — módulo alvo sem visibilidade suficiente (ex.: não-publico cross-package).
pub const MODULE_MODULE_NOT_VISIBLE: DiagnosticCode = DiagnosticCode::new("NEXA-MODULE-0003");
/// NEXA-MODULE-0004 — alias de dependência do project desconhecido (não é dependência direta).
pub const MODULE_UNKNOWN_DEPENDENCY_ALIAS: DiagnosticCode = DiagnosticCode::new("NEXA-MODULE-0004");
/// NEXA-MODULE-0005 — alias de import duplicado no mesmo module.
pub const MODULE_DUPLICATE_IMPORT_ALIAS: DiagnosticCode = DiagnosticCode::new("NEXA-MODULE-0005");
/// NEXA-MODULE-0006 — alias de import conflita com nome visível no TYPE namespace do module.
pub const MODULE_IMPORT_NAME_CONFLICT: DiagnosticCode = DiagnosticCode::new("NEXA-MODULE-0006");
/// NEXA-MODULE-0007 — dependência transitiva acessada sem dependência direta (sem `as`).
pub const MODULE_TRANSITIVE_DEPENDENCY_NOT_DIRECTLY_ACCESSIBLE: DiagnosticCode =
    DiagnosticCode::new("NEXA-MODULE-0007");

// ---------------------------------------------------------------------------
// Semantic diagnostics (Implementação 03).
// Baseline: NEXA-SEM-0001 .. NEXA-SEM-0010 (Impl 03 §221, §224).
// NUNCA renumerar os existentes.
// ---------------------------------------------------------------------------

/// NEXA-SEM-0001 — nome não encontrado (args: name, namespace).
pub const SEM_UNKNOWN_NAME: DiagnosticCode = DiagnosticCode::new("NEXA-SEM-0001");
/// NEXA-SEM-0002 — declaração duplicada no mesmo namespace e scope.
pub const SEM_DUPLICATE_DECLARATION: DiagnosticCode = DiagnosticCode::new("NEXA-SEM-0002");
/// NEXA-SEM-0003 — símbolo sem visibilidade suficiente no ponto de acesso.
pub const SEM_SYMBOL_NOT_VISIBLE: DiagnosticCode = DiagnosticCode::new("NEXA-SEM-0003");
/// NEXA-SEM-0004 — nome usado em namespace inválido para o símbolo.
pub const SEM_INVALID_NAMESPACE: DiagnosticCode = DiagnosticCode::new("NEXA-SEM-0004");
/// NEXA-SEM-0005 — nome ambíguo (configuração de import inválida).
pub const SEM_AMBIGUOUS_NAME: DiagnosticCode = DiagnosticCode::new("NEXA-SEM-0005");
/// NEXA-SEM-0006 — shadowing inválido de binding local/parameter dentro do mesmo callable.
pub const SEM_INVALID_SHADOWING: DiagnosticCode = DiagnosticCode::new("NEXA-SEM-0006");
/// NEXA-SEM-0007 — referência inválida a `self` sem receiver.
pub const SEM_INVALID_SELF_REFERENCE: DiagnosticCode = DiagnosticCode::new("NEXA-SEM-0007");
/// NEXA-SEM-0008 — referência inválida a `Self` fora de contexto de tipo.
pub const SEM_INVALID_SELF_TYPE_REFERENCE: DiagnosticCode = DiagnosticCode::new("NEXA-SEM-0008");
/// NEXA-SEM-0009 — declaração conflita com nome reservado do Prelude no mesmo namespace.
pub const SEM_PRELUDE_NAME_CONFLICT: DiagnosticCode = DiagnosticCode::new("NEXA-SEM-0009");
/// NEXA-SEM-0010 — path qualificado inválido (segmento não possui namespace associado).
pub const SEM_INVALID_QUALIFIED_PATH: DiagnosticCode = DiagnosticCode::new("NEXA-SEM-0010");
