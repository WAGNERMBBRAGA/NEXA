//! `Diagnostic` — diagnóstico estruturado e machine-readable.

use crate::code::DiagnosticCode;
use crate::severity::Severity;
use nexa_source::SourceSpan;
use serde::Serialize;

/// Valor de um argumento estruturado do diagnóstico.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum ArgumentValue {
    String(String),
    Integer(i64),
    UInteger(u64),
    Boolean(bool),
}

/// Argumento estruturado (key + value).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DiagnosticArgument {
    pub key: &'static str,
    pub value: ArgumentValue,
}

/// Fix sugerido (placeholder estruturado nesta etapa).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DiagnosticFix {
    pub description: String,
}

/// Span relacionado a um diagnóstico (ex.: "borrow starts here").
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RelatedDiagnostic {
    pub message: String,
    pub span: SourceSpan,
    pub severity: Severity,
}

/// Diagnóstico estrutural da NEXA.
///
/// Campos (Diagnostic Schema 1): schemaVersion, code, severity, category,
/// messageKey, message, primarySpan, related, arguments, help, fixes.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub severity: Severity,
    pub category: Option<&'static str>,
    pub message_key: &'static str,
    pub message: String,
    pub primary_span: Option<SourceSpan>,
    #[serde(default)]
    pub related: Vec<RelatedDiagnostic>,
    #[serde(default)]
    pub arguments: Vec<DiagnosticArgument>,
    pub help: Option<String>,
    #[serde(default)]
    pub fixes: Vec<DiagnosticFix>,
}

impl Diagnostic {
    pub fn error(
        code: DiagnosticCode,
        category: &'static str,
        message_key: &'static str,
        message: impl Into<String>,
    ) -> Self {
        Self::new(code, Severity::Error, category, message_key, message)
    }

    pub fn new(
        code: DiagnosticCode,
        severity: Severity,
        category: &'static str,
        message_key: &'static str,
        message: impl Into<String>,
    ) -> Self {
        Diagnostic {
            code,
            severity,
            category: Some(category),
            message_key,
            message: message.into(),
            primary_span: None,
            related: Vec::new(),
            arguments: Vec::new(),
            help: None,
            fixes: Vec::new(),
        }
    }

    pub fn with_primary_span(mut self, span: SourceSpan) -> Self {
        self.primary_span = Some(span);
        self
    }

    pub fn with_related(mut self, related: RelatedDiagnostic) -> Self {
        self.related.push(related);
        self
    }

    pub fn with_argument(mut self, key: &'static str, value: impl Into<ArgumentValue>) -> Self {
        self.arguments.push(DiagnosticArgument {
            key,
            value: value.into(),
        });
        self
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    pub fn with_fix(mut self, description: impl Into<String>) -> Self {
        self.fixes.push(DiagnosticFix {
            description: description.into(),
        });
        self
    }
}

impl From<i64> for ArgumentValue {
    fn from(v: i64) -> Self {
        ArgumentValue::Integer(v)
    }
}

impl From<u64> for ArgumentValue {
    fn from(v: u64) -> Self {
        ArgumentValue::UInteger(v)
    }
}

impl From<bool> for ArgumentValue {
    fn from(v: bool) -> Self {
        ArgumentValue::Boolean(v)
    }
}

impl From<String> for ArgumentValue {
    fn from(v: String) -> Self {
        ArgumentValue::String(v)
    }
}

impl From<&str> for ArgumentValue {
    fn from(v: &str) -> Self {
        ArgumentValue::String(v.to_owned())
    }
}
