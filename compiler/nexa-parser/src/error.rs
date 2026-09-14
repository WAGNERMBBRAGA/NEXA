use nexa_diagnostics::code::PARSE_UNEXPECTED_TOKEN;
use nexa_diagnostics::Diagnostic;
use nexa_source::SourceSpan;

#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub span: SourceSpan,
    pub expected: Vec<String>,
    pub found: String,
}

impl ParseError {
    pub fn new(
        message: impl Into<String>,
        span: SourceSpan,
        expected: Vec<String>,
        found: impl Into<String>,
    ) -> Self {
        ParseError {
            message: message.into(),
            span,
            expected,
            found: found.into(),
        }
    }

    pub fn to_diagnostic(&self) -> Diagnostic {
        let expected_str = self.expected.join(", ");
        Diagnostic::error(
            PARSE_UNEXPECTED_TOKEN,
            "parser",
            "parser.unexpected_token",
            format!("expected {expected_str}, found {}", self.found),
        )
        .with_primary_span(self.span)
    }
}
