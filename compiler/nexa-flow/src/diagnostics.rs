use nexa_diagnostics::{Diagnostic, DiagnosticCode, Severity};
use nexa_source::SourceSpan;
use std::fmt;

/// Diagnostic codes específicos da análise de fluxo (Implementação 05, §38).
///
/// Baseline normativo (`NEXA — Implementação 05`, §38):
///
/// ```text
/// NEXA-FLOW-0001 MissingReturn
/// NEXA-FLOW-0002 UnreachableCode
/// NEXA-FLOW-0003 InvalidBreak
/// NEXA-FLOW-0004 InvalidContinue
/// NEXA-FLOW-0005 UseBeforeInitialization
/// NEXA-FLOW-0006 PossiblyUninitialized
/// NEXA-FLOW-0007 InvalidControlFlow
/// NEXA-FLOW-0008 UnreachableMatchArm
/// NEXA-FLOW-0009 NonExhaustiveMatch
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FlowDiagnosticCode {
    /// Um caminho alcançável termina sem retornar (callable com retorno ≠ Unit/Never).
    MissingReturn,
    /// Código inalcançável (§39-44): warning por padrão.
    UnreachableCode,
    /// `break` fora de laço.
    InvalidBreak,
    /// `continue` fora de laço.
    InvalidContinue,
    /// Uso de variável antes de qualquer inicialiação.
    UseBeforeInitialization,
    /// Uso de variável possivelmente não inicializada.
    PossiblyUninitialized,
    /// Estrutura de controle inválida.
    InvalidControlFlow,
    /// Arm de match inalcançável (error, §43).
    UnreachableMatchArm,
    /// Match não exaustivo.
    NonExhaustiveMatch,
}

impl FlowDiagnosticCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            FlowDiagnosticCode::MissingReturn => "NEXA-FLOW-0001",
            FlowDiagnosticCode::UnreachableCode => "NEXA-FLOW-0002",
            FlowDiagnosticCode::InvalidBreak => "NEXA-FLOW-0003",
            FlowDiagnosticCode::InvalidContinue => "NEXA-FLOW-0004",
            FlowDiagnosticCode::UseBeforeInitialization => "NEXA-FLOW-0005",
            FlowDiagnosticCode::PossiblyUninitialized => "NEXA-FLOW-0006",
            FlowDiagnosticCode::InvalidControlFlow => "NEXA-FLOW-0007",
            FlowDiagnosticCode::UnreachableMatchArm => "NEXA-FLOW-0008",
            FlowDiagnosticCode::NonExhaustiveMatch => "NEXA-FLOW-0009",
        }
    }

    pub fn severity(&self) -> Severity {
        match self {
            FlowDiagnosticCode::MissingReturn
            | FlowDiagnosticCode::InvalidBreak
            | FlowDiagnosticCode::InvalidContinue
            | FlowDiagnosticCode::UseBeforeInitialization
            | FlowDiagnosticCode::PossiblyUninitialized
            | FlowDiagnosticCode::InvalidControlFlow
            | FlowDiagnosticCode::UnreachableMatchArm
            | FlowDiagnosticCode::NonExhaustiveMatch => Severity::Error,
            FlowDiagnosticCode::UnreachableCode => Severity::Warning,
        }
    }
}

impl fmt::Display for FlowDiagnosticCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Um diagnóstico produzido pela análise de fluxo.
#[derive(Debug, Clone)]
pub struct FlowDiagnostic {
    pub code: FlowDiagnosticCode,
    pub message: String,
    pub span: Option<SourceSpan>,
}

impl FlowDiagnostic {
    pub fn new(code: FlowDiagnosticCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            span: None,
        }
    }

    pub fn with_span(mut self, span: SourceSpan) -> Self {
        self.span = Some(span);
        self
    }

    /// Converte este diagnóstico de fluxo num `Diagnostic` NEXA genérico.
    pub fn to_diagnostic(&self) -> Diagnostic {
        let diag_code = DiagnosticCode::new(self.code.as_str());
        let mut diag = Diagnostic::new(
            diag_code,
            self.code.severity(),
            "flow",
            "flow-analysis",
            &self.message,
        );
        if let Some(span) = self.span {
            diag = diag.with_primary_span(span);
        }
        diag
    }
}
