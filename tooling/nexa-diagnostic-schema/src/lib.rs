use serde::Serialize;
use std::cmp::Ordering;

pub const DIAGNOSTIC_SCHEMA_VERSION: u32 = 1;

/// How a structured fix may be applied.
///
/// - `MachineApplicable` is safe to apply automatically.
/// - `MaybeApplicable` requires human/provider review.
/// - `ManualReview` must never be applied by an AI without explicit policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FixApplicability {
    MachineApplicable,
    MaybeApplicable,
    ManualReview,
}

impl FixApplicability {
    pub fn as_str(&self) -> &'static str {
        match self {
            FixApplicability::MachineApplicable => "machineApplicable",
            FixApplicability::MaybeApplicable => "maybeApplicable",
            FixApplicability::ManualReview => "manualReview",
        }
    }
}

/// A single text edit over a source span.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaTextEdit {
    pub start: u32,
    pub end: u32,
    pub new_text: String,
}

/// Structured edit bundled into a diagnostic fix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaDiagnosticFix {
    pub title: String,
    #[serde(default)]
    pub edits: Vec<SchemaTextEdit>,
    pub applicability: FixApplicability,
}

/// A single machine-readable diagnostic following Diagnostic Schema 1.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineDiagnostic {
    pub code: String,
    pub severity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    pub message_key: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_span: Option<SchemaSpan>,
    #[serde(default)]
    pub related: Vec<SchemaRelated>,
    #[serde(default)]
    pub arguments: Vec<SchemaArgument>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
    #[serde(default)]
    pub fixes: Vec<SchemaDiagnosticFix>,
}

/// Byte-offset source span in the machine schema.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaSpan {
    pub source: u32,
    pub start: u32,
    pub end: u32,
}

/// Related diagnostic attached to a machine diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaRelated {
    pub message: String,
    pub span: SchemaSpan,
    pub severity: String,
}

/// Typed argument referenced by the message key.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaArgument {
    pub key: String,
    pub value: SchemaArgumentValue,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum SchemaArgumentValue {
    String(String),
    Integer(i64),
    UInteger(u64),
    Boolean(bool),
}

/// Document that carries the schema version plus machine diagnostics.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaDocument {
    pub schema_version: u32,
    #[serde(default)]
    pub diagnostics: Vec<MachineDiagnostic>,
}

#[derive(Debug, thiserror::Error)]
pub enum DiagnosticSchemaError {
    #[error("NEXA-DIAG-0001: Unsupported schema version '{version}' (current is {current})")]
    UnsupportedSchemaVersion { version: u32, current: u32 },
    #[error("NEXA-DIAG-0002: Fix '{title}' contains an edit whose range is inverted or invalid")]
    InvalidEditRange { title: String },
    #[error(
        "NEXA-DIAG-0003: A fix of applicability '{applicability}' cannot be applied automatically"
    )]
    NotMachineApplicable { applicability: &'static str },
}

impl MachineDiagnostic {
    /// Normalize a `nexa_diagnostics::Diagnostic` into the machine schema.
    pub fn from_diagnostic(diag: &nexa_diagnostics::Diagnostic) -> Self {
        MachineDiagnostic {
            code: diag.code.as_str().to_string(),
            severity: diag.severity.as_str().to_string(),
            category: diag.category.map(|c| c.to_string()),
            message_key: diag.message_key.to_string(),
            message: diag.message.clone(),
            primary_span: diag.primary_span.map(|s| SchemaSpan {
                source: s.source.0,
                start: s.start,
                end: s.end,
            }),
            related: diag
                .related
                .iter()
                .map(|r| SchemaRelated {
                    message: r.message.clone(),
                    span: SchemaSpan {
                        source: r.span.source.0,
                        start: r.span.start,
                        end: r.span.end,
                    },
                    severity: r.severity.as_str().to_string(),
                })
                .collect(),
            arguments: diag
                .arguments
                .iter()
                .map(|a| SchemaArgument {
                    key: a.key.to_string(),
                    value: match &a.value {
                        nexa_diagnostics::ArgumentValue::String(v) => {
                            SchemaArgumentValue::String(v.clone())
                        }
                        nexa_diagnostics::ArgumentValue::Integer(v) => {
                            SchemaArgumentValue::Integer(*v)
                        }
                        nexa_diagnostics::ArgumentValue::UInteger(v) => {
                            SchemaArgumentValue::UInteger(*v)
                        }
                        nexa_diagnostics::ArgumentValue::Boolean(v) => {
                            SchemaArgumentValue::Boolean(*v)
                        }
                    },
                })
                .collect(),
            help: diag.help.clone(),
            fixes: diag
                .fixes
                .iter()
                .map(|f| SchemaDiagnosticFix {
                    title: f.description.clone(),
                    edits: Vec::new(),
                    applicability: FixApplicability::ManualReview,
                })
                .collect(),
        }
    }
}

impl SchemaDiagnosticFix {
    /// Validate that all edits describe non-inverted, in-bounds ranges (lower bound given).
    pub fn validate_ranges(&self, source_len: u32) -> Result<(), DiagnosticSchemaError> {
        for edit in &self.edits {
            if edit.start > edit.end || edit.end > source_len {
                return Err(DiagnosticSchemaError::InvalidEditRange {
                    title: self.title.clone(),
                });
            }
        }
        Ok(())
    }

    /// Whether this fix may be applied automatically by machine/AI tooling.
    pub fn is_machine_applicable(&self) -> bool {
        self.applicability == FixApplicability::MachineApplicable
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SeverityRank {
    Error,
    Warning,
    Info,
    Hint,
}

impl SeverityRank {
    pub fn from_severity(s: &nexa_diagnostics::Severity) -> Self {
        match s {
            nexa_diagnostics::Severity::Error => SeverityRank::Error,
            nexa_diagnostics::Severity::Warning => SeverityRank::Warning,
            nexa_diagnostics::Severity::Info => SeverityRank::Info,
            nexa_diagnostics::Severity::Hint => SeverityRank::Hint,
        }
    }
}

/// Deterministic canonical ordering for diagnostics.
///
/// Ordering is by (file path, primary byte offset, severity, code) and must
/// never depend on hash-map iteration order.
pub fn canonical_sort(
    diagnostics: &mut [nexa_diagnostics::Diagnostic],
    path_of: &impl Fn(u32) -> Option<String>,
) {
    diagnostics.sort_by(|a, b| {
        let a_path = a
            .primary_span
            .as_ref()
            .and_then(|s| path_of(s.source.0))
            .unwrap_or_default();
        let b_path = b
            .primary_span
            .as_ref()
            .and_then(|s| path_of(s.source.0))
            .unwrap_or_default();
        let path_cmp = a_path.cmp(&b_path);
        if path_cmp != Ordering::Equal {
            return path_cmp;
        }
        let a_off = a.primary_span.map(|s| s.start).unwrap_or(0);
        let b_off = b.primary_span.map(|s| s.start).unwrap_or(0);
        let off_cmp = a_off.cmp(&b_off);
        if off_cmp != Ordering::Equal {
            return off_cmp;
        }
        let sev_cmp =
            SeverityRank::from_severity(&a.severity).cmp(&SeverityRank::from_severity(&b.severity));
        if sev_cmp != Ordering::Equal {
            return sev_cmp;
        }
        a.code.as_str().cmp(b.code.as_str())
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexa_diagnostics::{Diagnostic, DiagnosticCode};
    use nexa_source::{SourceId, SourceSpan};

    fn code(c: &'static str) -> DiagnosticCode {
        DiagnosticCode::new(c)
    }

    #[test]
    fn schema_version_is_one() {
        assert_eq!(DIAGNOSTIC_SCHEMA_VERSION, 1);
    }

    #[test]
    fn normalizes_diagnostic_to_machine_schema() {
        let span = SourceSpan::new(SourceId(0), 10, 20);
        let diag = Diagnostic::error(
            code("NEXA-OWN-0001"),
            "ownership",
            "use_after_move",
            "value used after move",
        )
        .with_primary_span(span)
        .with_argument("symbol", "user");

        let m = MachineDiagnostic::from_diagnostic(&diag);
        assert_eq!(m.code, "NEXA-OWN-0001");
        assert_eq!(m.severity, "error");
        assert_eq!(m.category.as_deref(), Some("ownership"));
        assert_eq!(m.message_key, "use_after_move");
        assert_eq!(m.primary_span.as_ref().unwrap().start, 10);
        assert_eq!(m.primary_span.as_ref().unwrap().end, 20);
        assert_eq!(m.arguments.len(), 1);
        assert_eq!(m.arguments[0].key, "symbol");
        match &m.arguments[0].value {
            SchemaArgumentValue::String(v) => assert_eq!(v, "user"),
            _ => panic!("expected string argument"),
        }
    }

    #[test]
    fn serializes_to_camel_case_json() {
        let span = SourceSpan::new(SourceId(0), 1, 2);
        let diag = Diagnostic::error(
            code("NEXA-TYPE-0002"),
            "types",
            "type_mismatch",
            "expected Int",
        )
        .with_primary_span(span);
        let doc = SchemaDocument {
            schema_version: 1,
            diagnostics: vec![MachineDiagnostic::from_diagnostic(&diag)],
        };
        let json = serde_json::to_value(&doc).unwrap();
        assert_eq!(json["schemaVersion"], 1);
        assert_eq!(json["diagnostics"][0]["severity"], "error");
        assert_eq!(json["diagnostics"][0]["messageKey"], "type_mismatch");
        assert_eq!(json["diagnostics"][0]["primarySpan"]["start"], 1);
    }

    #[test]
    fn applicability_round_trip() {
        assert_eq!(
            FixApplicability::MachineApplicable.as_str(),
            "machineApplicable"
        );
        assert_eq!(FixApplicability::ManualReview.as_str(), "manualReview");
    }

    #[test]
    fn machine_applicable_check() {
        let mut fix = SchemaDiagnosticFix {
            title: "fix".into(),
            edits: vec![SchemaTextEdit {
                start: 0,
                end: 1,
                new_text: "x".into(),
            }],
            applicability: FixApplicability::MachineApplicable,
        };
        assert!(fix.is_machine_applicable());
        assert!(fix.validate_ranges(10).is_ok());
        fix.applicability = FixApplicability::ManualReview;
        assert!(!fix.is_machine_applicable());
    }

    #[test]
    fn invalid_edit_range_rejected() {
        let fix = SchemaDiagnosticFix {
            title: "inverted".into(),
            edits: vec![SchemaTextEdit {
                start: 5,
                end: 2,
                new_text: "".into(),
            }],
            applicability: FixApplicability::MachineApplicable,
        };
        assert!(fix.validate_ranges(10).is_err());
    }

    #[test]
    fn canonical_sort_orders_by_offset_then_severity() {
        let mut diags = vec![
            Diagnostic::error(code("NEXA-LEX-0001"), "lex", "a", "later")
                .with_primary_span(SourceSpan::new(SourceId(0), 30, 31)),
            Diagnostic::error(code("NEXA-PARSE-0001"), "parse", "b", "earlier")
                .with_primary_span(SourceSpan::new(SourceId(0), 5, 6)),
        ];
        let path_of = |_: u32| Some("mod.nexa".to_string());
        canonical_sort(&mut diags, &path_of);
        assert_eq!(diags[0].code.as_str(), "NEXA-PARSE-0001");
        assert_eq!(diags[1].code.as_str(), "NEXA-LEX-0001");
    }

    #[test]
    fn severity_rank_ordering() {
        assert!(SeverityRank::Error < SeverityRank::Warning);
        assert!(SeverityRank::Warning < SeverityRank::Info);
        assert!(SeverityRank::Info < SeverityRank::Hint);
    }
}
