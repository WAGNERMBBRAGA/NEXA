//! Cursor sobre a stream lossless (`Lexeme`) que alimenta a CST e expõe
//! lookahead para o parser.
//!
//! - `peek_*` nunca consome nem registra na CST (lookahead puro);
//! - `bump`/`skip_trivia`/`skip_soft` consomem **e registram** cada lexeme
//!   na CST (invariante lossless);
//! - trivia que contém quebra de linha (ex.: block comment multilinha)
//!   conta como statement break (§385-386).

use crate::cst::CstBuilder;
use nexa_lexer::{Lexeme, LexemeKind, TokenKind, TriviaKind};
use nexa_source::{SourceFile, SourceSpan};

pub struct Cursor<'a> {
    source: &'a SourceFile,
    lexemes: &'a [Lexeme],
    pos: usize,
    pub cst: CstBuilder,
    /// Closes virtuais pendentes criadas por um único token `>>` sendo
    /// re-interpretado como dois `>` no split contextual do parser de tipos
    /// (§358-362). O token real é registrado uma única vez na CST; as closes
    /// virtuais adicionais não produzem leaf.
    shr_virtual: u8,
    /// Span do último token real `>>` consumido por `bump_greater_virtual`
    /// (reutilizado para as closes virtuais extras).
    shr_virtual_span: SourceSpan,
}

impl<'a> Cursor<'a> {
    pub fn new(source: &'a SourceFile, lexemes: &'a [Lexeme]) -> Self {
        Cursor {
            source,
            lexemes,
            pos: 0,
            cst: CstBuilder::new(),
            shr_virtual: 0,
            shr_virtual_span: SourceSpan::point(source.id, source.byte_len()),
        }
    }

    pub fn source(&self) -> &'a SourceFile {
        self.source
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    fn significant_at(&self, mut skip: usize) -> Option<(TokenKind, SourceSpan)> {
        let mut i = self.pos;
        while i < self.lexemes.len() {
            let lx = &self.lexemes[i];
            match lx.kind {
                LexemeKind::Trivia(_) => i += 1,
                LexemeKind::Token(t) => {
                    if skip == 0 {
                        return Some((t, lx.span));
                    }
                    skip -= 1;
                    i += 1;
                }
            }
        }
        // EOF sempre presente no fim da stream.
        Some((
            TokenKind::Eof,
            SourceSpan::point(self.source.id, self.source.byte_len()),
        ))
    }

    pub fn peek_kind(&self) -> TokenKind {
        self.significant_at(0)
            .map(|(k, _)| k)
            .unwrap_or(TokenKind::Eof)
    }

    pub fn peek_span(&self) -> SourceSpan {
        self.significant_at(0)
            .map(|(_, s)| s)
            .unwrap_or_else(|| SourceSpan::point(self.source.id, self.source.byte_len()))
    }

    pub fn peek_kind_at(&self, offset: usize) -> TokenKind {
        self.significant_at(offset)
            .map(|(k, _)| k)
            .unwrap_or(TokenKind::Eof)
    }

    pub fn at(&self, kind: TokenKind) -> bool {
        self.peek_kind() == kind
    }

    pub fn at_any(&self, kinds: &[TokenKind]) -> bool {
        let k = self.peek_kind();
        kinds.contains(&k)
    }

    pub fn at_keyword(&self, kw: nexa_lexer::Keyword) -> bool {
        self.peek_kind() == TokenKind::Keyword(kw)
    }

    /// Primeiro token significativo ignorando trivia **e** newlines, sem
    /// consumir. Usado para decidir se uma construção continua na linha
    /// seguinte (ex.: `else` após `}` do `if`, §206) sem "queimar" o newline
    /// terminador de statement quando a continuação não existe.
    pub fn peek_kind_skipping_newlines(&self) -> TokenKind {
        let mut i = self.pos;
        while i < self.lexemes.len() {
            match self.lexemes[i].kind {
                LexemeKind::Trivia(_) | LexemeKind::Token(TokenKind::Newline) => i += 1,
                LexemeKind::Token(t) => return t,
            }
        }
        TokenKind::Eof
    }

    pub fn eof(&self) -> bool {
        self.peek_kind() == TokenKind::Eof
    }

    /// Texto do próximo token significativo (apenas se Identifier).
    pub fn peek_identifier_text(&self) -> Option<&'a str> {
        if self.peek_kind() != TokenKind::Identifier {
            return None;
        }
        self.source.span_text(self.peek_span())
    }

    /// Texto do token significativo na posição `offset` (apenas se Identifier).
    pub fn peek_identifier_text_at(&self, offset: usize) -> Option<&'a str> {
        if self.peek_kind_at(offset) != TokenKind::Identifier {
            return None;
        }
        self.source.span_text(self.lookahead_span(offset))
    }

    /// Texto do primeiro token significativo ignorando trivia **e** newlines,
    /// sem consumir (apenas se Identifier). Usado p/ construções contextuais
    /// que aparecem na linha seguinte (ex.: `where`, §89-95).
    pub fn peek_identifier_text_skipping_newlines(&self) -> Option<&'a str> {
        let src: &'a SourceFile = self.source;
        let mut i = self.pos;
        let mut span = None;
        while i < self.lexemes.len() {
            match self.lexemes[i].kind {
                LexemeKind::Trivia(_) | LexemeKind::Token(TokenKind::Newline) => i += 1,
                LexemeKind::Token(TokenKind::Identifier) => {
                    span = Some(self.lexemes[i].span);
                    break;
                }
                LexemeKind::Token(_) => return None,
            }
        }
        match span {
            Some(s) => src.span_text(s),
            None => None,
        }
    }

    /// Consome trivia (registrando na CST e avançando). Retorna `true` se
    /// alguma trivia continha quebra de linha (statement break via comentário).
    pub fn skip_trivia(&mut self) -> bool {
        let mut broke = false;
        while let Some(lx) = self.lexemes.get(self.pos) {
            match lx.kind {
                LexemeKind::Trivia(t) => {
                    if lx.contains_line_break(self.source) {
                        broke = true;
                    }
                    self.cst.record_trivia(t, lx.span);
                    self.pos += 1;
                }
                LexemeKind::Token(_) => break,
            }
        }
        broke
    }

    /// Consome trivia e newlines (soft newline): usado dentro de delimitadores
    /// e onde o parser está sintaticamente esperando continuação (§138-139).
    pub fn skip_soft(&mut self) {
        loop {
            let had_trivia = self.skip_trivia();
            let _ = had_trivia;
            match self.lexemes.get(self.pos).map(|lx| lx.kind) {
                Some(LexemeKind::Token(TokenKind::Newline)) => {
                    let lx = self.lexemes[self.pos];
                    self.cst.record_token(TokenKind::Newline, lx.span);
                    self.pos += 1;
                }
                _ => break,
            }
        }
    }

    /// Consome o próximo token significativo (após trivia), registrando tudo
    /// na CST. Retorna `(kind, span)` do token.
    pub fn bump(&mut self) -> (TokenKind, SourceSpan) {
        self.skip_trivia();
        let (kind, span) = self.significant_at(0).unwrap_or((
            TokenKind::Eof,
            SourceSpan::point(self.source.id, self.source.byte_len()),
        ));
        if let Some(lx) = self.lexemes.get(self.pos) {
            debug_assert!(matches!(lx.kind, LexemeKind::Token(_)));
            self.cst.record_token(kind, span);
            self.pos += 1;
        }
        (kind, span)
    }

    /// Consome `kind` se for o próximo token significativo. Registra na CST.
    pub fn bump_if(&mut self, kind: TokenKind) -> Option<SourceSpan> {
        if self.at(kind) {
            Some(self.bump().1)
        } else {
            None
        }
    }

    /// Consome trivia, ignorando o resultado (para posições onde o line break
    /// via trivia é irrelevante). Não consome newlines.
    pub fn eat_trivia(&mut self) {
        self.skip_trivia();
    }

    /// Span do próximo token significativo (lookahead sem consumo).
    pub fn lookahead_span(&self, offset: usize) -> SourceSpan {
        self.significant_at(offset)
            .map(|(_, s)| s)
            .unwrap_or_else(|| SourceSpan::point(self.source.id, self.source.byte_len()))
    }

    /// `true` se há quebra de linha dura (token `Newline` ou trivia com quebra
    /// de linha) entre o último token significante consumido e o próximo token
    /// significante (§147: `has_line_break_since_previous_significant_token`).
    pub fn at_hard_line_break(&self) -> bool {
        let mut i = self.pos;
        let mut saw_break = false;
        while let Some(lx) = self.lexemes.get(i) {
            match lx.kind {
                LexemeKind::Trivia(_) => {
                    if lx.contains_line_break(self.source) {
                        saw_break = true;
                    }
                    i += 1;
                }
                LexemeKind::Token(TokenKind::Newline) => {
                    saw_break = true;
                    i += 1;
                }
                LexemeKind::Token(_) => return saw_break,
            }
        }
        saw_break
    }

    /// `true` se o próximo token pode fechar uma lista genérica de tipos:
    /// `>`, um `>>` não-consumido, ou uma close virtual pendente (§358-362).
    pub fn at_greater_virtual(&self) -> bool {
        self.shr_virtual > 0 || self.at(TokenKind::Greater) || self.at(TokenKind::ShiftRight)
    }

    /// Consome uma close `>` de lista genérica, aplicando o split contextual
    /// de `>>`/`>>>` quando necessário. O token real é registrado uma única
    /// vez na CST (no frame mais interno); closes virtuais extras não criam
    /// leaf (lossless preservado).
    pub fn bump_greater_virtual(&mut self) -> Option<SourceSpan> {
        if self.shr_virtual > 0 {
            self.shr_virtual -= 1;
            let span = self.shr_virtual_span;
            return Some(span);
        }
        if self.at(TokenKind::Greater) {
            return Some(self.bump().1);
        }
        if self.at(TokenKind::ShiftRight) {
            let span = self.bump().1;
            self.shr_virtual = 1;
            self.shr_virtual_span = span;
            return Some(span);
        }
        None
    }

    /// Texto do source em um span (auxiliar).
    pub fn span_text(&self, span: SourceSpan) -> &'a str {
        self.source.span_text(span).unwrap_or("")
    }

    pub fn trivia_kind_of_span(&self, span: SourceSpan) -> Option<TriviaKind> {
        let mut i = self.pos;
        while let Some(lx) = self.lexemes.get(i) {
            if lx.span.start >= span.start && lx.span.end <= span.end {
                if let LexemeKind::Trivia(t) = lx.kind {
                    return Some(t);
                }
            }
            i += 1;
        }
        None
    }
}
