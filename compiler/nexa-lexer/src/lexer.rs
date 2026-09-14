//! Lexer NEXA — scanner manual lossless com mode stack.
//!
//! Garantias:
//! - cobertura lossless de bytes (tokens + trivia);
//! - spans ordenados e contíguos; EOF zero-width;
//! - `O(n)`, sem backtracking de regex;
//! - recovery: consome ≥ 1 Unicode scalar em token inválido;
//! - sem panic em input hostil.

use crate::keyword::keyword;
use crate::lexeme::Lexeme;
use crate::string::scan_escape;
use crate::token::{StringStyle, TokenKind};
use crate::trivia::TriviaKind;
use nexa_diagnostics::code::{
    LEX_INVALID_CHAR_LITERAL, LEX_INVALID_ESCAPE, LEX_INVALID_NUMERIC_LITERAL, LEX_INVALID_TOKEN,
    LEX_UNEXPECTED_BOM, LEX_UNTERMINATED_COMMENT, LEX_UNTERMINATED_INTERPOLATION,
    LEX_UNTERMINATED_STRING,
};
use nexa_diagnostics::{Diagnostic, Severity};
use nexa_source::{SourceFile, SourceId, SourceSpan};

/// Resultado do lexing: stream lossless + diagnostics.
#[derive(Debug, Clone, Default)]
pub struct LexResult {
    pub lexemes: Vec<Lexeme>,
    pub diagnostics: Vec<Diagnostic>,
}

impl LexResult {
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }
}

/// Lexa um `SourceFile` válido. Retorna tokens + trivia + diagnostics.
pub fn lex(source: &SourceFile) -> LexResult {
    let mut lexer = Lexer::new(source);
    lexer.run();
    LexResult {
        lexemes: lexer.lexemes,
        diagnostics: lexer.diagnostics,
    }
}

#[derive(Debug, Clone, Copy)]
enum Radix {
    Decimal,
    Binary,
    Octal,
    Hex,
}

impl Radix {
    fn digit_value(&self, b: u8) -> Option<u32> {
        let v = match b {
            b'0'..=b'9' => (b - b'0') as u32,
            b'a'..=b'f' => (b - b'a' + 10) as u32,
            b'A'..=b'F' => (b - b'A' + 10) as u32,
            _ => return None,
        };
        match self {
            Radix::Decimal => (v <= 9).then_some(v),
            Radix::Binary => (v <= 1).then_some(v),
            Radix::Octal => (v <= 7).then_some(v),
            Radix::Hex => (v <= 15).then_some(v),
        }
    }

    fn is_prefix(&self) -> bool {
        !matches!(self, Radix::Decimal)
    }
}

#[derive(Debug, Clone, Copy)]
enum Mode {
    String {
        style: StringStyle,
        open_span: SourceSpan,
        segment_start: usize,
    },
    Interpolation {
        brace_depth: u32,
        start_span: SourceSpan,
    },
}

struct Lexer<'a> {
    source: &'a SourceFile,
    bytes: &'a [u8],
    pos: usize,
    lexemes: Vec<Lexeme>,
    diagnostics: Vec<Diagnostic>,
    modes: Vec<Mode>,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a SourceFile) -> Self {
        Lexer {
            source,
            bytes: source.text.as_bytes(),
            pos: 0,
            lexemes: Vec::new(),
            diagnostics: Vec::new(),
            modes: Vec::new(),
        }
    }

    fn run(&mut self) {
        // BOM: rejeitado em strict 1.0 (diagnostic); consumido como trivia
        // para manter a invariante de cobertura lossless.
        if self.bytes.starts_with(b"\xEF\xBB\xBF") {
            let span = self.span(0, 3);
            self.emit_trivia(TriviaKind::Whitespace, span);
            self.diagnostics.push(
                Diagnostic::error(
                    LEX_UNEXPECTED_BOM,
                    "lexer",
                    "lexer.unexpected_bom",
                    "unexpected byte order mark",
                )
                .with_primary_span(span),
            );
            self.pos = 3;
        }

        loop {
            let before = self.pos;
            let modes_before = self.modes.len();

            if self.pos >= self.bytes.len() {
                if self.modes.is_empty() {
                    break;
                }
                self.recover_at_eof();
            } else {
                match self.modes.last() {
                    Some(Mode::String { style, .. }) => self.lex_string_mode(*style),
                    Some(Mode::Interpolation { .. }) => self.lex_interpolation_mode(),
                    None => self.lex_normal(),
                }
            }

            debug_assert!(
                self.pos > before || self.modes.len() != modes_before,
                "lexer made no progress at byte {}",
                before
            );
        }

        let eof = SourceSpan::point(self.source.id, self.bytes.len() as u32);
        self.lexemes.push(Lexeme::token(TokenKind::Eof, eof));
    }

    // ------------------------------------------------------------------
    // modo Normal
    // ------------------------------------------------------------------

    fn lex_normal(&mut self) {
        let pos = self.pos;
        let b = self.bytes[pos];
        match b {
            b' ' | b'\t' => self.scan_horizontal_whitespace(),
            b'\n' => {
                self.pos += 1;
                self.emit_token(TokenKind::Newline, self.span(pos, self.pos));
            }
            b'\r' => {
                if self.bytes.get(pos + 1) == Some(&b'\n') {
                    self.pos += 2;
                    self.emit_token(TokenKind::Newline, self.span(pos, self.pos));
                } else {
                    self.pos += 1;
                    let span = self.span(pos, self.pos);
                    self.emit_token(TokenKind::InvalidToken, span);
                    self.diagnostics.push(
                        Diagnostic::error(
                            LEX_INVALID_TOKEN,
                            "lexer",
                            "lexer.unsupported_line_ending",
                            "unsupported line ending (bare CR)",
                        )
                        .with_primary_span(span)
                        .with_argument("reason", "unsupported_line_ending"),
                    );
                }
            }
            b'/' => match self.bytes.get(pos + 1) {
                Some(b'/') => self.scan_line_comment(),
                Some(b'*') => self.scan_block_comment(),
                _ => {
                    self.pos += 1;
                    self.emit_token(TokenKind::Slash, self.span(pos, self.pos));
                }
            },
            b'"' => self.scan_string_start(),
            b'\'' => self.scan_char_literal(),
            b'0'..=b'9' => self.scan_number(),
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => self.scan_identifier_or_prefixed(),
            _ => self.scan_punctuation_or_invalid(),
        }
    }

    fn scan_horizontal_whitespace(&mut self) {
        let start = self.pos;
        while matches!(self.bytes.get(self.pos), Some(b' ') | Some(b'\t')) {
            self.pos += 1;
        }
        self.emit_trivia(TriviaKind::Whitespace, self.span(start, self.pos));
    }

    fn scan_line_comment(&mut self) {
        let start = self.pos;
        let kind = match self.bytes.get(self.pos + 2) {
            Some(b'/') => TriviaKind::DocumentationComment,
            Some(b'!') => TriviaKind::ModuleDocumentationComment,
            _ => TriviaKind::LineComment,
        };
        self.pos += 2;
        if matches!(
            kind,
            TriviaKind::DocumentationComment | TriviaKind::ModuleDocumentationComment
        ) {
            self.pos += 1;
        }
        while let Some(&b) = self.bytes.get(self.pos) {
            if b == b'\n' || b == b'\r' {
                break;
            }
            self.pos += 1;
        }
        self.emit_trivia(kind, self.span(start, self.pos));
    }

    fn scan_block_comment(&mut self) {
        let start = self.pos;
        self.pos += 2; // "/*"
        let mut depth: u32 = 1;
        while self.pos < self.bytes.len() {
            if self.bytes[self.pos] == b'/' && self.bytes.get(self.pos + 1) == Some(&b'*') {
                depth += 1;
                self.pos += 2;
            } else if self.bytes[self.pos] == b'*' && self.bytes.get(self.pos + 1) == Some(&b'/') {
                depth -= 1;
                self.pos += 2;
                if depth == 0 {
                    break;
                }
            } else {
                self.pos += 1;
            }
        }
        if depth > 0 {
            let open = self.span(start, start + 2);
            self.diagnostics.push(
                Diagnostic::error(
                    LEX_UNTERMINATED_COMMENT,
                    "lexer",
                    "lexer.unterminated_comment",
                    "unterminated block comment",
                )
                .with_primary_span(open),
            );
        }
        self.emit_trivia(TriviaKind::BlockComment, self.span(start, self.pos));
    }

    fn scan_identifier_or_prefixed(&mut self) {
        let start = self.pos;
        let first = self.bytes[start];
        if (first == b'r' || first == b'b') && self.bytes.get(start + 1) == Some(&b'"') {
            if first == b'r' {
                self.scan_raw_string(start);
            } else {
                self.scan_byte_string(start);
            }
            return;
        }
        self.pos += 1;
        while let Some(&b) = self.bytes.get(self.pos) {
            if b.is_ascii_alphanumeric() || b == b'_' {
                self.pos += 1;
            } else {
                break;
            }
        }
        let text = &self.bytes[start..self.pos];
        if text == b"_" {
            self.emit_token(TokenKind::Underscore, self.span(start, self.pos));
        } else if let Ok(s) = std::str::from_utf8(text) {
            if let Some(k) = keyword(s) {
                self.emit_token(TokenKind::Keyword(k), self.span(start, self.pos));
            } else {
                self.emit_token(TokenKind::Identifier, self.span(start, self.pos));
            }
        } else {
            self.emit_token(TokenKind::Identifier, self.span(start, self.pos));
        }
    }

    fn scan_punctuation_or_invalid(&mut self) {
        let pos = self.pos;

        // 3 caracteres (longest match primeiro)
        if self.bytes.get(pos..pos + 3) == Some(b"<<=") {
            self.pos += 3;
            self.emit_token(TokenKind::ShiftLeftEqual, self.span(pos, self.pos));
            return;
        }
        if self.bytes.get(pos..pos + 3) == Some(b">>=") {
            self.pos += 3;
            self.emit_token(TokenKind::ShiftRightEqual, self.span(pos, self.pos));
            return;
        }

        // 2 caracteres
        if let Some(two) = self.bytes.get(pos..pos + 2) {
            let pair = [two[0], two[1]];
            let kind = match &pair {
                b"==" => TokenKind::EqualEqual,
                b"!=" => TokenKind::BangEqual,
                b"<=" => TokenKind::LessEqual,
                b">=" => TokenKind::GreaterEqual,
                b"&&" => TokenKind::AmpAmp,
                b"||" => TokenKind::PipePipe,
                b"<<" => TokenKind::ShiftLeft,
                b">>" => TokenKind::ShiftRight,
                b"+=" => TokenKind::PlusEqual,
                b"-=" => TokenKind::MinusEqual,
                b"*=" => TokenKind::StarEqual,
                b"/=" => TokenKind::SlashEqual,
                b"%=" => TokenKind::PercentEqual,
                b"&=" => TokenKind::AmpEqual,
                b"|=" => TokenKind::PipeEqual,
                b"^=" => TokenKind::CaretEqual,
                b"->" => TokenKind::Arrow,
                b"=>" => TokenKind::FatArrow,
                b"::" => TokenKind::DoubleColon,
                b".." => TokenKind::DotDot,
                _ => {
                    self.pos += 1;
                    self.emit_single_char(pos);
                    return;
                }
            };
            self.pos += 2;
            self.emit_token(kind, self.span(pos, self.pos));
            return;
        }

        self.pos += 1;
        self.emit_single_char(pos);
    }

    fn emit_single_char(&mut self, pos: usize) {
        let token = match self.bytes[pos] {
            b'(' => TokenKind::LeftParen,
            b')' => TokenKind::RightParen,
            b'{' => TokenKind::LeftBrace,
            b'}' => TokenKind::RightBrace,
            b'[' => TokenKind::LeftBracket,
            b']' => TokenKind::RightBracket,
            b',' => TokenKind::Comma,
            b':' => TokenKind::Colon,
            b'.' => TokenKind::Dot,
            b'@' => TokenKind::At,
            b'+' => TokenKind::Plus,
            b'-' => TokenKind::Minus,
            b'*' => TokenKind::Star,
            b'%' => TokenKind::Percent,
            b'=' => TokenKind::Equal,
            b'!' => TokenKind::Bang,
            b'<' => TokenKind::Less,
            b'>' => TokenKind::Greater,
            b'&' => TokenKind::Ampersand,
            b'|' => TokenKind::Pipe,
            b'^' => TokenKind::Caret,
            b'~' => TokenKind::Tilde,
            _ => {
                // token inválido: consome ≥ 1 Unicode scalar
                let width = utf8_char_width(self.bytes, pos);
                self.pos += width;
                let span = self.span(pos, self.pos);
                self.emit_token(TokenKind::InvalidToken, span);
                self.diagnostics.push(
                    Diagnostic::error(
                        LEX_INVALID_TOKEN,
                        "lexer",
                        "lexer.invalid_token",
                        "invalid token",
                    )
                    .with_primary_span(span),
                );
                return;
            }
        };
        self.emit_token(token, self.span(pos, self.pos));
    }

    // ------------------------------------------------------------------
    // literais numéricos
    // ------------------------------------------------------------------

    fn scan_number(&mut self) {
        let start = self.pos;

        let radix = if self.bytes[start] == b'0' {
            match self.bytes.get(start + 1) {
                Some(b'x') | Some(b'X') => Radix::Hex,
                Some(b'o') | Some(b'O') => Radix::Octal,
                Some(b'b') | Some(b'B') => Radix::Binary,
                _ => Radix::Decimal,
            }
        } else {
            Radix::Decimal
        };

        if radix.is_prefix() {
            self.pos += 2;
            let (digit_count, mut first_error) = self.scan_radix_digits(radix);
            if digit_count == 0 {
                // ex.: 0x, 0b, 0o
                let span = self.span(start, self.pos);
                self.emit_token(TokenKind::IntegerLiteral, span);
                self.diagnostics.push(
                    Diagnostic::error(
                        LEX_INVALID_NUMERIC_LITERAL,
                        "lexer",
                        "lexer.invalid_numeric_literal",
                        "invalid numeric literal",
                    )
                    .with_primary_span(span)
                    .with_argument("reason", "missing_digits"),
                );
                return;
            }
            if let Some(err) = first_error.take() {
                self.push_numeric_error(err);
            }
            self.scan_suffix();
            self.emit_token(TokenKind::IntegerLiteral, self.span(start, self.pos));
            return;
        }

        // decimal
        let (_, mut first_error) = self.scan_radix_digits(Radix::Decimal);
        let mut is_float = false;

        // fração: '.' exige dígito após (1.foo → Integer + Dot + Identifier)
        if self.peek_dot_then_digit() {
            is_float = true;
            self.pos += 1; // '.'
            let (_, e) = self.scan_radix_digits(Radix::Decimal);
            if first_error.is_none() {
                first_error = e;
            }
        }

        // expoente
        if matches!(self.bytes.get(self.pos), Some(b'e') | Some(b'E')) {
            let after_sign = self.peek_exponent_after_sign();
            if self
                .bytes
                .get(after_sign)
                .is_some_and(|b| b.is_ascii_digit())
            {
                is_float = true;
                self.pos = after_sign;
                let (_, e) = self.scan_radix_digits(Radix::Decimal);
                if first_error.is_none() {
                    first_error = e;
                }
            } else {
                // 1e / 1e+ / 1e- → expoente sem dígitos
                let exp_start = self.pos;
                let exp_end = (self.pos + 2).min(self.bytes.len());
                let span = self.span(exp_start, exp_end);
                self.diagnostics.push(
                    Diagnostic::error(
                        LEX_INVALID_NUMERIC_LITERAL,
                        "lexer",
                        "lexer.invalid_numeric_literal",
                        "invalid numeric literal",
                    )
                    .with_primary_span(span)
                    .with_argument("reason", "missing_exponent_digits"),
                );
                self.pos = exp_end;
            }
        }

        self.scan_suffix();
        let kind = if is_float {
            TokenKind::FloatLiteral
        } else {
            TokenKind::IntegerLiteral
        };
        self.emit_token(kind, self.span(start, self.pos));

        if let Some(err) = first_error {
            self.push_numeric_error(err);
        }
    }

    fn push_numeric_error(&mut self, err_pos: usize) {
        let span = self.span(err_pos, (err_pos + 1).min(self.bytes.len()));
        self.diagnostics.push(
            Diagnostic::error(
                LEX_INVALID_NUMERIC_LITERAL,
                "lexer",
                "lexer.invalid_numeric_literal",
                "invalid numeric literal",
            )
            .with_primary_span(span)
            .with_argument("reason", "invalid_underscore_separator"),
        );
    }

    /// Consome dígitos (e underscores entre dígitos) do radix.
    /// Retorna `(count_de_dígitos, primeira_posição_de_erro)`.
    fn scan_radix_digits(&mut self, radix: Radix) -> (usize, Option<usize>) {
        let mut count = 0usize;
        let mut first_error: Option<usize> = None;
        loop {
            match self.bytes.get(self.pos) {
                Some(&b'_') => {
                    // separador '_' válido apenas entre dígitos do radix
                    // (ex.: 1_000 ok; 0x_FF, 10_, 1__0 inválidos)
                    let prev_is_digit = self
                        .bytes
                        .get(self.pos.saturating_sub(1))
                        .is_some_and(|pb| radix.digit_value(*pb).is_some());
                    let next_is_digit = self
                        .bytes
                        .get(self.pos + 1)
                        .is_some_and(|nb| radix.digit_value(*nb).is_some());
                    if (!prev_is_digit || !next_is_digit) && first_error.is_none() {
                        first_error = Some(self.pos);
                    }
                    self.pos += 1;
                }
                Some(&b) if radix.digit_value(b).is_some() => {
                    count += 1;
                    self.pos += 1;
                }
                // `e`/`E` pertencem ao expoente (somente decimal); não
                // consomir para o bloco de expoente tratar (1e10, 1.5e-3).
                Some(&b) if matches!(radix, Radix::Decimal) && (b == b'e' || b == b'E') => break,
                Some(&b) if b.is_ascii_alphanumeric() => {
                    // dígito fora do radix (ex.: 0b2, 0xG)
                    if first_error.is_none() {
                        first_error = Some(self.pos);
                    }
                    self.pos += 1;
                }
                _ => break,
            }
        }
        (count, first_error)
    }

    fn scan_suffix(&mut self) {
        const SUFFIXES: [&str; 10] = [
            "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f32", "f64",
        ];
        let rest = &self.bytes[self.pos..];
        for s in SUFFIXES {
            if rest.starts_with(s.as_bytes()) {
                let after = self.pos + s.len();
                let boundary_ok = match self.bytes.get(after) {
                    Some(&b) => !(b.is_ascii_alphanumeric() || b == b'_'),
                    None => true,
                };
                if boundary_ok {
                    self.pos = after;
                }
                return;
            }
        }
    }

    fn peek_dot_then_digit(&self) -> bool {
        self.bytes.get(self.pos) == Some(&b'.')
            && self
                .bytes
                .get(self.pos + 1)
                .is_some_and(|b| b.is_ascii_digit())
    }

    fn peek_exponent_after_sign(&self) -> usize {
        let mut after = self.pos + 1;
        if matches!(self.bytes.get(after), Some(b'+') | Some(b'-')) {
            after += 1;
        }
        after
    }

    // ------------------------------------------------------------------
    // literais de string / char
    // ------------------------------------------------------------------

    fn scan_string_start(&mut self) {
        let pos = self.pos;
        if self.bytes.get(pos..pos + 3) == Some(b"\"\"\"") {
            self.pos += 3;
            let span = self.span(pos, self.pos);
            self.emit_token(TokenKind::StringStart(StringStyle::Multiline), span);
            self.modes.push(Mode::String {
                style: StringStyle::Multiline,
                open_span: span,
                segment_start: self.pos,
            });
        } else {
            self.pos += 1;
            let span = self.span(pos, self.pos);
            self.emit_token(TokenKind::StringStart(StringStyle::Normal), span);
            self.modes.push(Mode::String {
                style: StringStyle::Normal,
                open_span: span,
                segment_start: self.pos,
            });
        }
    }

    fn lex_string_mode(&mut self, style: StringStyle) {
        let pos = self.pos;
        let (open_span, segment_start) = match self.modes.last() {
            Some(Mode::String {
                open_span,
                segment_start,
                ..
            }) => (*open_span, *segment_start),
            _ => return,
        };

        match self.bytes[pos] {
            b'"' if style == StringStyle::Multiline => {
                if self.bytes.get(pos..pos + 3) == Some(b"\"\"\"") {
                    self.flush_string_text(segment_start, pos);
                    self.pos += 3;
                    let span = self.span(pos, self.pos);
                    self.emit_token(TokenKind::StringEnd(StringStyle::Multiline), span);
                    self.modes.pop();
                } else {
                    // aspas simples dentro do texto multiline
                    self.pos += 1;
                }
            }
            b'"' => {
                self.flush_string_text(segment_start, pos);
                self.pos += 1;
                let span = self.span(pos, self.pos);
                self.emit_token(TokenKind::StringEnd(StringStyle::Normal), span);
                self.modes.pop();
            }
            b'$' if self.bytes.get(pos + 1) == Some(&b'{') => {
                self.flush_string_text(segment_start, pos);
                self.pos += 2;
                let span = self.span(pos, self.pos);
                self.emit_token(TokenKind::InterpolationStart, span);
                self.modes.push(Mode::Interpolation {
                    brace_depth: 0,
                    start_span: span,
                });
            }
            b'\n' | b'\r' if style == StringStyle::Normal => {
                // string normal não pode conter newline: unterminated
                self.flush_string_text(segment_start, pos);
                self.diagnostics.push(
                    Diagnostic::error(
                        LEX_UNTERMINATED_STRING,
                        "lexer",
                        "lexer.unterminated_string",
                        "unterminated string literal",
                    )
                    .with_primary_span(open_span),
                );
                self.modes.pop(); // não consome o newline
            }
            b'\\' => {
                let (valid, end) = scan_escape(self.bytes, pos);
                if !valid {
                    self.diagnostics.push(
                        Diagnostic::error(
                            LEX_INVALID_ESCAPE,
                            "lexer",
                            "lexer.invalid_escape",
                            "invalid escape sequence",
                        )
                        .with_primary_span(self.span(pos, end)),
                    );
                }
                self.pos = end;
            }
            _ => {
                self.pos += 1;
            }
        }
    }

    fn lex_interpolation_mode(&mut self) {
        let pos = self.pos;
        let b = self.bytes[pos];
        match b {
            b'{' => {
                self.pos += 1;
                self.emit_token(TokenKind::LeftBrace, self.span(pos, self.pos));
                if let Some(Mode::Interpolation { brace_depth, .. }) = self.modes.last_mut() {
                    *brace_depth += 1;
                }
            }
            b'}' => {
                let depth = match self.modes.last() {
                    Some(Mode::Interpolation { brace_depth, .. }) => *brace_depth,
                    _ => 0,
                };
                if depth > 0 {
                    self.pos += 1;
                    self.emit_token(TokenKind::RightBrace, self.span(pos, self.pos));
                    if let Some(Mode::Interpolation { brace_depth, .. }) = self.modes.last_mut() {
                        *brace_depth -= 1;
                    }
                } else {
                    self.pos += 1;
                    self.emit_token(TokenKind::InterpolationEnd, self.span(pos, self.pos));
                    // Reseta o segmento da string pai para começar após o
                    // InterpolationEnd (o texto que seguir é novo StringText).
                    if let Some(Mode::String {
                        segment_start: s, ..
                    }) = self
                        .modes
                        .len()
                        .checked_sub(2)
                        .and_then(|i| self.modes.get_mut(i))
                    {
                        *s = self.pos;
                    }
                    self.modes.pop(); // volta para a string pai
                }
            }
            _ => self.lex_normal(),
        }
    }

    /// Emite `StringText` para `[segment_start, end)` se não vazio e
    /// atualiza `segment_start` do modo string no topo.
    fn flush_string_text(&mut self, segment_start: usize, end: usize) {
        if end > segment_start {
            self.emit_token(TokenKind::StringText, self.span(segment_start, end));
        }
        if let Some(Mode::String {
            segment_start: s, ..
        }) = self.modes.last_mut()
        {
            *s = end;
        }
    }

    fn scan_raw_string(&mut self, start: usize) {
        // bytes[start] == 'r', bytes[start+1] == '"'
        self.pos = start + 2;
        let mut closed = false;
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            if b == b'"' {
                self.pos += 1;
                closed = true;
                break;
            }
            if b == b'\n' || b == b'\r' {
                break;
            }
            self.pos += 1;
        }
        if !closed {
            self.diagnostics.push(
                Diagnostic::error(
                    LEX_UNTERMINATED_STRING,
                    "lexer",
                    "lexer.unterminated_string",
                    "unterminated string literal",
                )
                .with_primary_span(self.span(start, start + 2)),
            );
        }
        self.emit_token(TokenKind::RawStringLiteral, self.span(start, self.pos));
    }

    fn scan_byte_string(&mut self, start: usize) {
        // bytes[start] == 'b', bytes[start+1] == '"'
        self.pos = start + 2;
        let mut closed = false;
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            match b {
                b'"' => {
                    self.pos += 1;
                    closed = true;
                    break;
                }
                b'\n' | b'\r' => break,
                b'\\' => {
                    let (valid, end) = scan_escape(self.bytes, self.pos);
                    if !valid {
                        self.diagnostics.push(
                            Diagnostic::error(
                                LEX_INVALID_ESCAPE,
                                "lexer",
                                "lexer.invalid_escape",
                                "invalid escape sequence",
                            )
                            .with_primary_span(self.span(self.pos, end)),
                        );
                    }
                    self.pos = end;
                }
                _ => self.pos += 1,
            }
        }
        if !closed {
            self.diagnostics.push(
                Diagnostic::error(
                    LEX_UNTERMINATED_STRING,
                    "lexer",
                    "lexer.unterminated_string",
                    "unterminated string literal",
                )
                .with_primary_span(self.span(start, start + 2)),
            );
        }
        self.emit_token(TokenKind::ByteStringLiteral, self.span(start, self.pos));
    }

    fn scan_char_literal(&mut self) {
        let start = self.pos;
        self.pos += 1; // '
        let mut scalar_count = 0u32;
        let mut escape_error = false;
        let mut closed = false;
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            match b {
                b'\'' => {
                    self.pos += 1;
                    closed = true;
                    break;
                }
                b'\n' | b'\r' => break,
                b'\\' => {
                    let (valid, end) = scan_escape(self.bytes, self.pos);
                    if valid {
                        scalar_count += 1;
                    } else {
                        escape_error = true;
                        self.diagnostics.push(
                            Diagnostic::error(
                                LEX_INVALID_ESCAPE,
                                "lexer",
                                "lexer.invalid_escape",
                                "invalid escape sequence",
                            )
                            .with_primary_span(self.span(self.pos, end)),
                        );
                    }
                    self.pos = end;
                }
                _ => {
                    let width = utf8_char_width(self.bytes, self.pos);
                    scalar_count += 1;
                    self.pos += width;
                }
            }
        }
        let span = self.span(start, self.pos);
        if !closed {
            self.diagnostics.push(
                Diagnostic::error(
                    LEX_INVALID_CHAR_LITERAL,
                    "lexer",
                    "lexer.unterminated_char_literal",
                    "unterminated char literal",
                )
                .with_primary_span(self.span(start, start + 1)),
            );
        } else if !escape_error && scalar_count != 1 {
            self.diagnostics.push(
                Diagnostic::error(
                    LEX_INVALID_CHAR_LITERAL,
                    "lexer",
                    "lexer.char_literal_scalar_count",
                    "char literal must contain exactly one Unicode scalar value",
                )
                .with_primary_span(span)
                .with_argument("scalarCount", u64::from(scalar_count)),
            );
        }
        self.emit_token(TokenKind::CharLiteral, span);
    }

    // ------------------------------------------------------------------
    // recovery no EOF com modos pendentes
    // ------------------------------------------------------------------

    fn recover_at_eof(&mut self) {
        let mode = self.modes.last().copied();
        match mode {
            Some(Mode::Interpolation { start_span, .. }) => {
                self.diagnostics.push(
                    Diagnostic::error(
                        LEX_UNTERMINATED_INTERPOLATION,
                        "lexer",
                        "lexer.unterminated_interpolation",
                        "unterminated interpolation",
                    )
                    .with_primary_span(start_span),
                );
                self.modes.clear();
            }
            Some(Mode::String {
                open_span,
                segment_start,
                ..
            }) => {
                self.flush_string_text(segment_start, self.pos);
                self.diagnostics.push(
                    Diagnostic::error(
                        LEX_UNTERMINATED_STRING,
                        "lexer",
                        "lexer.unterminated_string",
                        "unterminated string literal",
                    )
                    .with_primary_span(open_span),
                );
                self.modes.clear();
            }
            None => {}
        }
    }

    // ------------------------------------------------------------------
    // helpers
    // ------------------------------------------------------------------

    fn span(&self, start: usize, end: usize) -> SourceSpan {
        SourceSpan::new(self.source.id, start as u32, end as u32)
    }

    fn emit_token(&mut self, kind: TokenKind, span: SourceSpan) {
        self.lexemes.push(Lexeme::token(kind, span));
    }

    fn emit_trivia(&mut self, kind: TriviaKind, span: SourceSpan) {
        self.lexemes.push(Lexeme::trivia(kind, span));
    }
}

/// Largura em bytes do primeiro char UTF-8 em `bytes[pos]`.
fn utf8_char_width(bytes: &[u8], pos: usize) -> usize {
    let b = bytes[pos];
    if b < 0x80 {
        1
    } else if b >> 5 == 0b110 {
        2
    } else if b >> 4 == 0b1110 {
        3
    } else if b >> 3 == 0b11110 {
        4
    } else {
        1
    }
}

/// Tipo de referência auxiliar para testes: id do source.
#[allow(dead_code)]
fn source_id_of(source: &SourceFile) -> SourceId {
    source.id
}
