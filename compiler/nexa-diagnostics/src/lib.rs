//! NEXA — Diagnostics
//!
//! Diagnostics estruturados e machine-readable. O texto humano é apenas a
//! renderização padrão; o contrato é o struct `Diagnostic` com
//! `schemaVersion` 1, código estável, severidade, categoria, message key,
//! spans primários/relacionados, argumentos, help e fixes.

pub mod code;
pub mod diagnostic;
pub mod render;
pub mod severity;

pub use code::DiagnosticCode;
pub use diagnostic::{
    ArgumentValue, Diagnostic, DiagnosticArgument, DiagnosticFix, RelatedDiagnostic,
};
pub use render::render_diagnostics;
pub use severity::Severity;

/// Schema version dos diagnostics machine-readable (Diagnostic Schema 1).
pub const DIAGNOSTIC_SCHEMA_VERSION: u32 = 1;
