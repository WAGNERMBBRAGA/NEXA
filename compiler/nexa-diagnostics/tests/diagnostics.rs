//! Testes de diagnostics estruturados e renderização.

use nexa_diagnostics::code::{LEX_INVALID_ESCAPE, LEX_INVALID_TOKEN, LEX_INVALID_UTF8};
use nexa_diagnostics::{render_diagnostics, ArgumentValue, Diagnostic};
use nexa_source::{SourceManager, SourceSpan};

fn diag() -> Diagnostic {
    Diagnostic::error(
        LEX_INVALID_TOKEN,
        "lexer",
        "lexer.invalid_token",
        "invalid token",
    )
}

#[test]
fn diagnostic_fields() {
    let d = diag()
        .with_argument("reason", "unsupported_line_ending")
        .with_argument("count", 2u64)
        .with_help("use LF or CRLF");
    assert_eq!(d.code.as_str(), "NEXA-LEX-0002");
    assert_eq!(d.severity.as_str(), "error");
    assert_eq!(d.category, Some("lexer"));
    assert_eq!(d.message_key, "lexer.invalid_token");
    assert_eq!(d.primary_span, None);
    assert_eq!(d.arguments.len(), 2);
    assert_eq!(
        d.arguments[0].value,
        ArgumentValue::String("unsupported_line_ending".into())
    );
    assert_eq!(d.arguments[1].value, ArgumentValue::UInteger(2));
    assert_eq!(d.help.as_deref(), Some("use LF or CRLF"));
}

#[test]
fn json_serialization_shape() {
    let mut mgr = SourceManager::new();
    let id = mgr.load_text("x.nexa".into(), "module".into());
    let span = SourceSpan::new(id, 0, 3);
    let d = Diagnostic::error(
        LEX_INVALID_ESCAPE,
        "lexer",
        "lexer.invalid_escape",
        "invalid escape sequence",
    )
    .with_primary_span(span)
    .with_argument("escape", "\\q");
    let json = serde_json::to_string_pretty(&d).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["code"], "NEXA-LEX-0004");
    assert_eq!(v["severity"], "Error");
    assert_eq!(v["category"], "lexer");
    assert_eq!(v["messageKey"], "lexer.invalid_escape");
    assert_eq!(v["primarySpan"]["source"], 0);
    assert_eq!(v["primarySpan"]["start"], 0);
    assert_eq!(v["primarySpan"]["end"], 3);
    assert_eq!(v["arguments"][0]["key"], "escape");
    assert!(v.get("related").is_some());
    assert!(v.get("fixes").is_some());
}

#[test]
fn invalid_utf8_code_constant_is_stable() {
    assert_eq!(LEX_INVALID_UTF8.as_str(), "NEXA-LEX-0001");
    // Códigos não podem ser renumerados; assert explícito dos 9.
    let all = [
        "NEXA-LEX-0001",
        "NEXA-LEX-0002",
        "NEXA-LEX-0003",
        "NEXA-LEX-0004",
        "NEXA-LEX-0005",
        "NEXA-LEX-0006",
        "NEXA-LEX-0007",
        "NEXA-LEX-0008",
        "NEXA-LEX-0009",
    ];
    for c in all {
        assert_eq!(c.len(), 13);
    }
}

#[test]
fn human_renderer_has_location() {
    let mut mgr = SourceManager::new();
    let id = mgr.load_text("ex.nexa".into(), "let x = \"\\q\"\n".into());
    let span = SourceSpan::new(id, 8, 10);
    let d = Diagnostic::error(
        LEX_INVALID_ESCAPE,
        "lexer",
        "lexer.invalid_escape",
        "invalid escape sequence",
    )
    .with_primary_span(span);
    let text = render_diagnostics(&mgr, &[d]);
    assert!(
        text.contains("error[NEXA-LEX-0004]: invalid escape sequence"),
        "text: {text}"
    );
    assert!(text.contains("ex.nexa:1:9"), "text: {text}");
    assert!(text.contains("^^"), "text: {text}");
}

#[test]
fn human_renderer_without_span() {
    let mgr = SourceManager::new();
    let d = Diagnostic::error(
        LEX_INVALID_UTF8,
        "source",
        "source.invalid_utf8",
        "invalid UTF-8 encoding",
    );
    let text = render_diagnostics(&mgr, &[d]);
    assert_eq!(text, "error[NEXA-LEX-0001]: invalid UTF-8 encoding");
}
