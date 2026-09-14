//! Serialização e reconstrução lossless da CST.
//!
//! Invariante (Impl 02 §391): para source lexicamente válido,
//! `reconstruct(cst, source) == source.text`. Tokens synthetic (`Missing`)
//! são excluídos da reconstrução (§392).

use crate::green::{GreenNode, GreenToken, GreenTokenKind, SyntaxKind};
use crate::node::NodeOrToken;
use crate::red::{syntax_kind_name, token_kind_name};
use nexa_source::{SourceFile, SourceSpan};

/// Reconstrução exata do source a partir dos tokens reais (exclui Missing).
pub fn reconstruct(tree: &[NodeOrToken], source: &SourceFile) -> String {
    let mut out = String::with_capacity(source.byte_len() as usize);
    for node in tree {
        reconstruct_into(node, source, &mut out);
    }
    out
}

fn reconstruct_into(node: &NodeOrToken, source: &SourceFile, out: &mut String) {
    match node {
        NodeOrToken::Node(GreenNode::Node { children, .. }) => {
            for child in children {
                reconstruct_into(child, source, out);
            }
        }
        NodeOrToken::Node(GreenNode::Token { span, .. }) => {
            if let Some(text) = source.span_text(*span) {
                out.push_str(text);
            }
        }
        NodeOrToken::Token(GreenToken { span, .. }) => {
            if let Some(text) = source.span_text(*span) {
                out.push_str(text);
            }
        }
        NodeOrToken::Missing(_) => {}
    }
}

/// Nó de projeção JSON da CST (debug/tooling; não é formato normativo).
/// Campos: `kind` (nome canônico), `startByte`, `endByte` e `children`.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CstDebugNode {
    pub kind: String,
    pub start_byte: u32,
    pub end_byte: u32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<CstDebugNode>,
}

fn kind_debug_name(kind: SyntaxKind) -> String {
    syntax_kind_name(kind).to_owned()
}

fn token_debug_name(token: &GreenToken) -> String {
    token_kind_name(&token.kind).to_owned()
}

impl CstDebugNode {
    pub fn from_root(tree: &[NodeOrToken]) -> Option<CstDebugNode> {
        tree.first().map(node_to_debug)
    }
}

fn node_to_debug(node: &NodeOrToken) -> CstDebugNode {
    match node {
        NodeOrToken::Node(GreenNode::Node { kind, children }) => {
            let span = node.span();
            CstDebugNode {
                kind: kind_debug_name(*kind),
                start_byte: span.start,
                end_byte: span.end,
                children: children.iter().map(node_to_debug).collect(),
            }
        }
        NodeOrToken::Node(GreenNode::Token { kind, span }) => CstDebugNode {
            kind: format!("token:{}", token_kind_name(&GreenTokenKind::Token(*kind))),
            start_byte: span.start,
            end_byte: span.end,
            children: Vec::new(),
        },
        NodeOrToken::Token(token) => CstDebugNode {
            kind: token_debug_name(token),
            start_byte: token.span.start,
            end_byte: token.span.end,
            children: Vec::new(),
        },
        NodeOrToken::Missing(missing) => CstDebugNode {
            kind: format!("missing:{}", kind_debug_name(missing.kind)),
            start_byte: missing.span.start,
            end_byte: missing.span.end,
            children: Vec::new(),
        },
    }
}

/// Range máximo coberto pelos filhos reais de um nó (para spans de nós CST).
pub fn node_span_of(children: &[NodeOrToken], fallback: SourceSpan) -> SourceSpan {
    let mut start = u32::MAX;
    let mut end = 0u32;
    let mut source = fallback.source;
    for child in children {
        let s = child.span();
        if child.is_missing() {
            continue;
        }
        if s.start < start {
            start = s.start;
        }
        if s.end > end {
            end = s.end;
        }
        if !child.is_missing() {
            source = s.source;
        }
    }
    if start == u32::MAX {
        fallback
    } else {
        SourceSpan::new(source, start, end)
    }
}
