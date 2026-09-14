//! Renderer humano de diagnostics (default English).
//!
//! A renderização **não** é autoridade semântica; o struct `Diagnostic` é.
//! Códigos e message keys nunca mudam com locale.

use crate::diagnostic::Diagnostic;
use nexa_source::{SourceManager, SourceSpan};

/// Renderiza todos os diagnostics em texto humano determinístico.
pub fn render_diagnostics(sources: &SourceManager, diagnostics: &[Diagnostic]) -> String {
    let mut out = String::new();
    for (i, d) in diagnostics.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        render_one(sources, d, &mut out);
    }
    out
}

fn render_one(sources: &SourceManager, d: &Diagnostic, out: &mut String) {
    use std::fmt::Write;
    let _ = write!(out, "{}[{}]: {}", d.severity.as_str(), d.code, d.message);

    if let Some(span) = d.primary_span {
        if let Some(file) = sources.source(span.source) {
            let loc = file.location(span.start);
            let line = loc.line + 1;
            let column = loc.column + 1;
            let _ = write!(out, "\n  --> {}:{}:{}", file.display_name(), line, column);
            let _ = write!(out, "\n   |");
            if let Some(line_text) = file.line_text(loc.line) {
                let _ = writeln!(out, "\n {line} | {line_text}");
                // caret sob o span (limitado ao fim da linha, em colunas escalares)
                let line_bytes = file.span_text(SourceSpan::new(span.source, span.start, span.end));
                let caret_len = match line_bytes {
                    Some(text) => {
                        let until_end = line_text.chars().count() as u32 - loc.column;
                        text.chars().count().min(until_end as usize).max(1)
                    }
                    None => 1,
                };
                let caret = "^".repeat(caret_len);
                let pad = " ".repeat(loc.column as usize);
                let _ = writeln!(out, "   | {pad}{caret} {}", d.message);
            } else {
                let _ = write!(out, "\n   |");
            }
        }
    }
}
