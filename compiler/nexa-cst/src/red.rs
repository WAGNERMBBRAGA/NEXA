use crate::green::{GreenNode, GreenToken, GreenTokenKind, SyntaxKind};
use crate::node::NodeOrToken;
use nexa_source::SourceSpan;

pub struct RedTree {
    green: Vec<NodeOrToken>,
}

impl RedTree {
    pub fn new(green: Vec<NodeOrToken>) -> Self {
        RedTree { green }
    }

    pub fn root(&self) -> Option<RedNode<'_>> {
        match self.green.first() {
            Some(NodeOrToken::Node(GreenNode::Node { kind, .. })) => Some(RedNode {
                tree: self,
                idx: 0,
                kind: *kind,
            }),
            _ => None,
        }
    }

    pub fn kind_at(&self, idx: usize) -> Option<SyntaxKind> {
        self.green.get(idx).map(|n| n.kind())
    }

    pub fn span_at(&self, idx: usize) -> Option<SourceSpan> {
        self.green.get(idx).map(|n| n.span())
    }
}

pub struct RedNode<'a> {
    tree: &'a RedTree,
    idx: usize,
    kind: SyntaxKind,
}

impl<'a> RedNode<'a> {
    pub fn kind(&self) -> SyntaxKind {
        self.kind
    }

    pub fn span(&self) -> SourceSpan {
        self.tree
            .span_at(self.idx)
            .unwrap_or(SourceSpan::point(nexa_source::SourceId(0), 0))
    }

    fn green_children(&self) -> &'a [NodeOrToken] {
        match &self.tree.green[self.idx] {
            NodeOrToken::Node(GreenNode::Node { children, .. }) => children,
            _ => &[],
        }
    }

    pub fn children(&self) -> impl Iterator<Item = NodeOrToken> + '_ {
        self.green_children().iter().cloned()
    }

    pub fn child_tokens(&self) -> impl Iterator<Item = &'a GreenToken> {
        self.green_children().iter().filter_map(|c| match c {
            NodeOrToken::Token(t) => Some(t),
            _ => None,
        })
    }

    pub fn first_token(&self) -> Option<&'a GreenToken> {
        self.child_tokens().next()
    }

    pub fn kind_name(&self) -> &'static str {
        syntax_kind_name(self.kind)
    }
}

pub struct RedToken<'a> {
    green: &'a GreenToken,
}

impl<'a> RedToken<'a> {
    pub fn new(green: &'a GreenToken) -> Self {
        RedToken { green }
    }

    pub fn kind(&self) -> SyntaxKind {
        match self.green.kind {
            GreenTokenKind::Token(k) => crate::node::token_kind_to_syntax(k),
            GreenTokenKind::Trivia(t) => crate::node::trivia_kind_to_syntax(t),
        }
    }

    pub fn span(&self) -> SourceSpan {
        self.green.span
    }

    pub fn kind_name(&self) -> &'static str {
        token_kind_name(&self.green.kind)
    }
}

pub fn syntax_kind_name(kind: SyntaxKind) -> &'static str {
    match kind.0 {
        1 => "ROOT",
        2 => "MODULE_DECL",
        3 => "IMPORT_DECL",
        4 => "IMPORT_ALIAS",
        5 => "QUALIFIED_NAME",
        6 => "TOP_LEVEL_DECL",
        7 => "TYPE_DECL",
        10 => "FUNCTION_DECL",
        11 => "ACTION_DECL",
        12 => "STRUCT_DECL",
        13 => "STRUCT_FIELD",
        14 => "ENUM_DECL",
        15 => "ENUM_VARIANT",
        16 => "ENUM_VARIANT_TUPLE",
        17 => "ENUM_VARIANT_STRUCT",
        18 => "INTERFACE_DECL",
        19 => "INTERFACE_METHOD",
        20 => "IMPL_DECL",
        21 => "TYPE_ALIAS",
        22 => "CONST_DECL",
        23 => "CONST_STMT",
        30 => "PARAM_LIST",
        31 => "PARAM",
        32 => "RETURN_TYPE",
        33 => "EFFECTS_CLAUSE",
        34 => "EFFECT_LIST",
        35 => "EFFECT_PATH",
        40 => "WHERE_CLAUSE",
        41 => "WHERE_PREDICATE",
        42 => "GENERIC_PARAMS",
        43 => "GENERIC_PARAM",
        44 => "GENERIC_ARG_LIST",
        45 => "REQUIRE_CLAUSE",
        46 => "ENSURE_CLAUSE",
        50 => "BLOCK",
        51 => "STMT_LIST",
        60 => "LET_STMT",
        61 => "VAR_STMT",
        62 => "ASSIGN_STMT",
        63 => "COMPOUND_ASSIGN",
        64 => "EXPR_STMT",
        65 => "RETURN_STMT",
        66 => "BREAK_STMT",
        67 => "CONTINUE_STMT",
        68 => "DISCARD_STMT",
        69 => "UNSAFE_STMT",
        70 => "IF_EXPR",
        71 => "IF_ARM",
        72 => "ELSE_ARM",
        73 => "MATCH_EXPR",
        74 => "MATCH_ARM",
        75 => "MATCH_GUARD",
        76 => "FOR_EXPR",
        77 => "WHILE_EXPR",
        78 => "LOOP_EXPR",
        80 => "CALL_EXPR",
        81 => "INDEX_EXPR",
        82 => "FIELD_EXPR",
        83 => "METHOD_CALL",
        84 => "AWAIT_EXPR",
        85 => "TRY_EXPR",
        86 => "UNSAFE_EXPR",
        87 => "BLOCK_EXPR",
        88 => "PAREN_EXPR",
        89 => "BINARY_EXPR",
        90 => "UNARY_EXPR",
        91 => "REF_EXPR",
        92 => "REF_MUT_EXPR",
        93 => "MOVE_EXPR",
        100 => "TYPE_EXPR",
        101 => "PATH_TYPE",
        102 => "GENERIC_TYPE",
        103 => "ARRAY_TYPE",
        104 => "OPTION_TYPE",
        105 => "RESULT_TYPE",
        106 => "REF_TYPE",
        107 => "REF_MUT_TYPE",
        108 => "TUPLE_TYPE",
        109 => "FUNCTION_TYPE",
        110 => "ATTR",
        111 => "ATTR_ARGS",
        120 => "STRING_LITERAL",
        121 => "BYTE_STRING_LITERAL",
        122 => "RAW_STRING_LITERAL",
        130 => "TUPLE_EXPR",
        131 => "ARRAY_EXPR",
        132 => "STRUCT_LITERAL",
        140 => "LITERAL_PATTERN",
        141 => "IDENT_PATTERN",
        142 => "WILDCARD_PATTERN",
        143 => "TUPLE_PATTERN",
        144 => "STRUCT_PATTERN",
        145 => "ENUM_PATTERN",
        146 => "REST_PATTERN",
        147 => "GUARD_PATTERN",
        148 => "RENAME_PATTERN",
        149 => "OR_PATTERN",
        200 => "TRIVIA_WS",
        201 => "TRIVIA_NEWLINE",
        202 => "TRIVIA_LINE_COMMENT",
        203 => "TRIVIA_BLOCK_COMMENT",
        300 => "IDENTIFIER",
        301 => "INT_LITERAL",
        302 => "FLOAT_LITERAL",
        303 => "CHAR_LITERAL",
        304 => "KEYWORD",
        310 => "L_PAREN",
        311 => "R_PAREN",
        312 => "L_BRACE",
        313 => "R_BRACE",
        314 => "L_BRACKET",
        315 => "R_BRACKET",
        321 => "ARROW",
        322 => "FAT_ARROW",
        323 => "EQUAL",
        344 => "NEWLINE",
        345 => "EOF",
        u16::MAX => "ERROR",
        _ => "UNKNOWN",
    }
}

pub fn token_kind_name(kind: &GreenTokenKind) -> &'static str {
    match kind {
        GreenTokenKind::Token(t) => match t {
            nexa_lexer::TokenKind::Identifier => "Identifier",
            nexa_lexer::TokenKind::IntegerLiteral => "IntegerLiteral",
            nexa_lexer::TokenKind::FloatLiteral => "FloatLiteral",
            nexa_lexer::TokenKind::CharLiteral => "CharLiteral",
            nexa_lexer::TokenKind::Keyword(k) => k.as_str(),
            nexa_lexer::TokenKind::LeftParen => "(",
            nexa_lexer::TokenKind::RightParen => ")",
            nexa_lexer::TokenKind::LeftBrace => "{",
            nexa_lexer::TokenKind::RightBrace => "}",
            nexa_lexer::TokenKind::LeftBracket => "[",
            nexa_lexer::TokenKind::RightBracket => "]",
            nexa_lexer::TokenKind::Comma => ",",
            nexa_lexer::TokenKind::Colon => ":",
            nexa_lexer::TokenKind::DoubleColon => "::",
            nexa_lexer::TokenKind::Dot => ".",
            nexa_lexer::TokenKind::Arrow => "->",
            nexa_lexer::TokenKind::FatArrow => "=>",
            nexa_lexer::TokenKind::Equal => "=",
            nexa_lexer::TokenKind::EqualEqual => "==",
            nexa_lexer::TokenKind::Bang => "!",
            nexa_lexer::TokenKind::Plus => "+",
            nexa_lexer::TokenKind::Minus => "-",
            nexa_lexer::TokenKind::Star => "*",
            nexa_lexer::TokenKind::Slash => "/",
            nexa_lexer::TokenKind::Percent => "%",
            nexa_lexer::TokenKind::Newline => "\\n",
            nexa_lexer::TokenKind::Eof => "EOF",
            _ => "token",
        },
        GreenTokenKind::Trivia(t) => match t {
            nexa_lexer::TriviaKind::Whitespace => "Whitespace",
            nexa_lexer::TriviaKind::LineComment => "LineComment",
            nexa_lexer::TriviaKind::BlockComment => "BlockComment",
            nexa_lexer::TriviaKind::DocumentationComment => "DocumentationComment",
            nexa_lexer::TriviaKind::ModuleDocumentationComment => "ModuleDocumentationComment",
        },
    }
}
