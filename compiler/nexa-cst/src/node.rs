use crate::green::{GreenNode, GreenToken, GreenTokenKind, SyntaxKind};
use nexa_lexer::{TokenKind, TriviaKind};
use nexa_source::SourceSpan;

/// Token sintético de recovery (zero-width), marcado para não ser
/// serializado na reconstrução lossless do source original.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MissingToken {
    /// Kind de "token" sintético (vem do mapeamento `token_kind_to_syntax`).
    pub kind: SyntaxKind,
    /// Span zero-width no ponto de inserção (§325).
    pub span: SourceSpan,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NodeOrToken {
    Node(GreenNode),
    Token(GreenToken),
    Missing(MissingToken),
}

impl NodeOrToken {
    pub fn token(kind: TokenKind, span: SourceSpan) -> Self {
        NodeOrToken::Token(GreenToken {
            kind: GreenTokenKind::Token(kind),
            span,
        })
    }

    pub fn trivia(kind: TriviaKind, span: SourceSpan) -> Self {
        NodeOrToken::Token(GreenToken {
            kind: GreenTokenKind::Trivia(kind),
            span,
        })
    }

    pub fn node(kind: SyntaxKind, children: Vec<NodeOrToken>) -> Self {
        NodeOrToken::Node(GreenNode::Node { kind, children })
    }

    pub fn missing(kind: SyntaxKind, span: SourceSpan) -> Self {
        NodeOrToken::Missing(MissingToken { kind, span })
    }

    pub fn kind(&self) -> SyntaxKind {
        match self {
            NodeOrToken::Node(n) => match n {
                GreenNode::Node { kind, .. } => *kind,
                GreenNode::Token { .. } => unreachable!("GreenNode::Token in kind()"),
            },
            NodeOrToken::Token(t) => match t.kind {
                GreenTokenKind::Token(k) => token_kind_to_syntax(k),
                GreenTokenKind::Trivia(t) => trivia_kind_to_syntax(t),
            },
            NodeOrToken::Missing(m) => m.kind,
        }
    }

    pub fn span(&self) -> SourceSpan {
        match self {
            NodeOrToken::Node(n) => match n {
                GreenNode::Node { children, .. } => {
                    let mut start = u32::MAX;
                    let mut end = 0u32;
                    for child in children {
                        let s = child.span();
                        if s.start < start {
                            start = s.start;
                        }
                        if s.end > end {
                            end = s.end;
                        }
                    }
                    if start == u32::MAX {
                        start = 0;
                    }
                    SourceSpan::new(
                        children
                            .first()
                            .and_then(|c| match c {
                                NodeOrToken::Token(t) => Some(t.span.source),
                                NodeOrToken::Node(GreenNode::Token { span, .. }) => {
                                    Some(span.source)
                                }
                                NodeOrToken::Node(GreenNode::Node { .. })
                                | NodeOrToken::Missing(_) => None,
                            })
                            .unwrap_or(nexa_source::SourceId(0)),
                        start,
                        end,
                    )
                }
                GreenNode::Token { span, .. } => *span,
            },
            NodeOrToken::Token(t) => t.span,
            NodeOrToken::Missing(m) => m.span,
        }
    }

    pub fn is_missing(&self) -> bool {
        matches!(self, NodeOrToken::Missing(_))
    }

    pub fn as_token(&self) -> Option<&GreenToken> {
        match self {
            NodeOrToken::Token(t) => Some(t),
            _ => None,
        }
    }

    pub fn children(&self) -> Option<&[NodeOrToken]> {
        match self {
            NodeOrToken::Node(GreenNode::Node { children, .. }) => Some(children),
            _ => None,
        }
    }
}

pub(crate) fn trivia_kind_to_syntax(kind: TriviaKind) -> SyntaxKind {
    use nexa_lexer::TriviaKind as T;
    match kind {
        T::Whitespace => SyntaxKind(200),
        T::LineComment => SyntaxKind(202),
        T::BlockComment => SyntaxKind(203),
        T::DocumentationComment => SyntaxKind(204),
        T::ModuleDocumentationComment => SyntaxKind(205),
    }
}

pub fn token_kind_to_syntax(kind: TokenKind) -> SyntaxKind {
    match kind {
        TokenKind::Identifier => SyntaxKind(300),
        TokenKind::IntegerLiteral => SyntaxKind(301),
        TokenKind::FloatLiteral => SyntaxKind(302),
        TokenKind::CharLiteral => SyntaxKind(303),
        TokenKind::Keyword(_) => SyntaxKind(304),
        TokenKind::LeftParen => SyntaxKind(310),
        TokenKind::RightParen => SyntaxKind(311),
        TokenKind::LeftBrace => SyntaxKind(312),
        TokenKind::RightBrace => SyntaxKind(313),
        TokenKind::LeftBracket => SyntaxKind(314),
        TokenKind::RightBracket => SyntaxKind(315),
        TokenKind::Comma => SyntaxKind(316),
        TokenKind::Colon => SyntaxKind(317),
        TokenKind::DoubleColon => SyntaxKind(318),
        TokenKind::Dot => SyntaxKind(319),
        TokenKind::DotDot => SyntaxKind(320),
        TokenKind::Arrow => SyntaxKind(321),
        TokenKind::FatArrow => SyntaxKind(322),
        TokenKind::Equal => SyntaxKind(323),
        TokenKind::EqualEqual => SyntaxKind(324),
        TokenKind::Bang => SyntaxKind(325),
        TokenKind::BangEqual => SyntaxKind(326),
        TokenKind::Less => SyntaxKind(327),
        TokenKind::LessEqual => SyntaxKind(328),
        TokenKind::Greater => SyntaxKind(329),
        TokenKind::GreaterEqual => SyntaxKind(330),
        TokenKind::Plus => SyntaxKind(331),
        TokenKind::Minus => SyntaxKind(332),
        TokenKind::Star => SyntaxKind(333),
        TokenKind::Slash => SyntaxKind(334),
        TokenKind::Percent => SyntaxKind(335),
        TokenKind::Ampersand => SyntaxKind(336),
        TokenKind::Pipe => SyntaxKind(337),
        TokenKind::Caret => SyntaxKind(338),
        TokenKind::Tilde => SyntaxKind(339),
        TokenKind::AmpAmp => SyntaxKind(340),
        TokenKind::PipePipe => SyntaxKind(341),
        TokenKind::At => SyntaxKind(342),
        TokenKind::Underscore => SyntaxKind(343),
        TokenKind::Newline => SyntaxKind(344),
        TokenKind::Eof => SyntaxKind(345),
        TokenKind::StringStart(s) => match s {
            nexa_lexer::StringStyle::Normal => SyntaxKind(350),
            nexa_lexer::StringStyle::Multiline => SyntaxKind(351),
        },
        TokenKind::StringText => SyntaxKind(352),
        TokenKind::InterpolationStart => SyntaxKind(353),
        TokenKind::InterpolationEnd => SyntaxKind(354),
        TokenKind::StringEnd(s) => match s {
            nexa_lexer::StringStyle::Normal => SyntaxKind(355),
            nexa_lexer::StringStyle::Multiline => SyntaxKind(356),
        },
        TokenKind::RawStringLiteral => SyntaxKind(357),
        TokenKind::ByteStringLiteral => SyntaxKind(358),
        TokenKind::InvalidToken => SyntaxKind(u16::MAX),
        TokenKind::ShiftLeft => SyntaxKind(360),
        TokenKind::ShiftRight => SyntaxKind(361),
        TokenKind::PlusEqual => SyntaxKind(362),
        TokenKind::MinusEqual => SyntaxKind(363),
        TokenKind::StarEqual => SyntaxKind(364),
        TokenKind::SlashEqual => SyntaxKind(365),
        TokenKind::PercentEqual => SyntaxKind(366),
        TokenKind::AmpEqual => SyntaxKind(367),
        TokenKind::PipeEqual => SyntaxKind(368),
        TokenKind::CaretEqual => SyntaxKind(369),
        TokenKind::ShiftLeftEqual => SyntaxKind(370),
        TokenKind::ShiftRightEqual => SyntaxKind(371),
    }
}

pub struct CstToken {
    pub kind: TokenKind,
    pub text: String,
    pub span: SourceSpan,
}

pub struct CstNode {
    pub kind: SyntaxKind,
    pub children: Vec<NodeOrToken>,
    pub span: SourceSpan,
}
