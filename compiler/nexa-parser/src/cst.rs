//! Builder da CST lossless: cada lexeme consumido pelo parser e cada
//! token sintético (`Missing`) são anexados ao frame aberto mais interno.
//!
//! Invariante: `reconstruct(cst, source) == source.text` para source
//! lexicamente válido (Impl 02 §391).

use nexa_cst::{NodeOrToken, SyntaxKind};
use nexa_lexer::{TokenKind, TriviaKind};
use nexa_source::SourceSpan;

pub struct CstFrame {
    pub kind: SyntaxKind,
    pub children: Vec<NodeOrToken>,
}

pub struct CstBuilder {
    stack: Vec<CstFrame>,
    root: Vec<NodeOrToken>,
}

impl Default for CstBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl CstBuilder {
    pub fn new() -> Self {
        CstBuilder {
            stack: Vec::new(),
            root: Vec::new(),
        }
    }

    /// Abre um frame de nó CST (ex.: `FUNCTION_DECL`).
    pub fn start(&mut self, kind: SyntaxKind) {
        self.stack.push(CstFrame {
            kind,
            children: Vec::new(),
        });
    }

    pub fn record_token(&mut self, kind: TokenKind, span: SourceSpan) {
        let leaf = NodeOrToken::token(kind, span);
        self.attach(leaf);
    }

    pub fn record_trivia(&mut self, kind: TriviaKind, span: SourceSpan) {
        let leaf = NodeOrToken::trivia(kind, span);
        self.attach(leaf);
    }

    pub fn record_missing(&mut self, kind: SyntaxKind, span: SourceSpan) {
        let leaf = NodeOrToken::missing(kind, span);
        self.attach(leaf);
    }

    fn attach(&mut self, leaf: NodeOrToken) {
        match self.stack.last_mut() {
            Some(frame) => frame.children.push(leaf),
            None => self.root.push(leaf),
        }
    }

    /// Fecha o frame do topo (verifica o kind) e anexa o nó ao pai/root.
    pub fn finish(&mut self, kind: SyntaxKind) {
        let frame = self
            .stack
            .pop()
            .expect("cst: finish() without matching start()");
        debug_assert_eq!(frame.kind, kind, "cst: frame kind mismatch");
        let node = NodeOrToken::node(kind, frame.children);
        match self.stack.last_mut() {
            Some(parent) => parent.children.push(node),
            None => self.root.push(node),
        }
    }

    /// True se o builder está dentro de pelo menos um frame.
    pub fn is_in_node(&self) -> bool {
        !self.stack.is_empty()
    }

    /// Reclassifica o frame aberto mais interno (usado pelo parser de tipos
    /// para escolher o kind do nó após inspecionar o primeiro token, ex.:
    /// `Array`, `Optional`, `Result` vs. path/generic).
    pub fn set_top_kind(&mut self, kind: SyntaxKind) {
        if let Some(frame) = self.stack.last_mut() {
            frame.kind = kind;
        }
    }

    /// Nós raiz produzidos (um único nó `ROOT` normalmente).
    pub fn finish_root(&mut self) -> Vec<NodeOrToken> {
        debug_assert!(self.stack.is_empty(), "cst: unfinished frames at root");
        std::mem::take(&mut self.root)
    }
}
