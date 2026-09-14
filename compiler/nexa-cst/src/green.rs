use crate::node::NodeOrToken;
use nexa_lexer::{TokenKind, TriviaKind};
use nexa_source::SourceSpan;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GreenNode {
    Token {
        kind: TokenKind,
        span: SourceSpan,
    },
    Node {
        kind: SyntaxKind,
        children: Vec<NodeOrToken>,
    },
}

/// Kind unificado de um leaf da CST: token real ou trivia (lossless).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GreenTokenKind {
    Token(TokenKind),
    Trivia(TriviaKind),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GreenToken {
    pub kind: GreenTokenKind,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SyntaxKind(pub u16);

impl SyntaxKind {
    pub const ROOT: Self = Self(1);
    pub const MODULE_DECL: Self = Self(2);
    pub const IMPORT_DECL: Self = Self(3);
    pub const IMPORT_ALIAS: Self = Self(4);
    pub const QUALIFIED_NAME: Self = Self(5);
    pub const TOP_LEVEL_DECL: Self = Self(6);
    pub const TYPE_DECL: Self = Self(7);
    pub const FUNCTION_DECL: Self = Self(10);
    pub const ACTION_DECL: Self = Self(11);
    pub const STRUCT_DECL: Self = Self(12);
    pub const STRUCT_FIELD: Self = Self(13);
    pub const ENUM_DECL: Self = Self(14);
    pub const ENUM_VARIANT: Self = Self(15);
    pub const ENUM_VARIANT_TUPLE: Self = Self(16);
    pub const ENUM_VARIANT_STRUCT: Self = Self(17);
    pub const INTERFACE_DECL: Self = Self(18);
    pub const INTERFACE_METHOD: Self = Self(19);
    pub const IMPL_DECL: Self = Self(20);
    pub const TYPE_ALIAS: Self = Self(21);
    pub const CONST_DECL: Self = Self(22);
    pub const CONST_STMT: Self = Self(23);
    pub const PARAM_LIST: Self = Self(30);
    pub const PARAM: Self = Self(31);
    pub const RETURN_TYPE: Self = Self(32);
    pub const EFFECTS_CLAUSE: Self = Self(33);
    pub const EFFECT_LIST: Self = Self(34);
    pub const EFFECT_PATH: Self = Self(35);
    pub const WHERE_CLAUSE: Self = Self(40);
    pub const WHERE_PREDICATE: Self = Self(41);
    pub const GENERIC_PARAMS: Self = Self(42);
    pub const GENERIC_PARAM: Self = Self(43);
    pub const GENERIC_ARG_LIST: Self = Self(44);
    pub const REQUIRE_CLAUSE: Self = Self(45);
    pub const ENSURE_CLAUSE: Self = Self(46);
    pub const BLOCK: Self = Self(50);
    pub const STMT_LIST: Self = Self(51);
    pub const LET_STMT: Self = Self(60);
    pub const VAR_STMT: Self = Self(61);
    pub const ASSIGN_STMT: Self = Self(62);
    pub const COMPOUND_ASSIGN: Self = Self(63);
    pub const EXPR_STMT: Self = Self(64);
    pub const RETURN_STMT: Self = Self(65);
    pub const BREAK_STMT: Self = Self(66);
    pub const CONTINUE_STMT: Self = Self(67);
    pub const DISCARD_STMT: Self = Self(68);
    pub const UNSAFE_STMT: Self = Self(69);
    pub const IF_EXPR: Self = Self(70);
    pub const IF_ARM: Self = Self(71);
    pub const ELSE_ARM: Self = Self(72);
    pub const MATCH_EXPR: Self = Self(73);
    pub const MATCH_ARM: Self = Self(74);
    pub const MATCH_GUARD: Self = Self(75);
    pub const FOR_EXPR: Self = Self(76);
    pub const WHILE_EXPR: Self = Self(77);
    pub const LOOP_EXPR: Self = Self(78);
    pub const CALL_EXPR: Self = Self(80);
    pub const INDEX_EXPR: Self = Self(81);
    pub const FIELD_EXPR: Self = Self(82);
    pub const METHOD_CALL: Self = Self(83);
    pub const AWAIT_EXPR: Self = Self(84);
    pub const TRY_EXPR: Self = Self(85);
    pub const UNSAFE_EXPR: Self = Self(86);
    pub const BLOCK_EXPR: Self = Self(87);
    pub const PAREN_EXPR: Self = Self(88);
    pub const BINARY_EXPR: Self = Self(89);
    pub const UNARY_EXPR: Self = Self(90);
    pub const REF_EXPR: Self = Self(91);
    pub const REF_MUT_EXPR: Self = Self(92);
    pub const MOVE_EXPR: Self = Self(93);
    pub const TYPE_EXPR: Self = Self(100);
    pub const PATH_TYPE: Self = Self(101);
    pub const GENERIC_TYPE: Self = Self(102);
    pub const ARRAY_TYPE: Self = Self(103);
    pub const OPTION_TYPE: Self = Self(104);
    pub const RESULT_TYPE: Self = Self(105);
    pub const REF_TYPE: Self = Self(106);
    pub const REF_MUT_TYPE: Self = Self(107);
    pub const TUPLE_TYPE: Self = Self(108);
    pub const FUNCTION_TYPE: Self = Self(109);
    pub const ATTR: Self = Self(110);
    pub const ATTR_ARGS: Self = Self(111);
    pub const STRING_LITERAL: Self = Self(120);
    pub const BYTE_STRING_LITERAL: Self = Self(121);
    pub const RAW_STRING_LITERAL: Self = Self(122);
    pub const TUPLE_EXPR: Self = Self(130);
    pub const ARRAY_EXPR: Self = Self(131);
    pub const STRUCT_LITERAL: Self = Self(132);
    pub const LITERAL_PATTERN: Self = Self(140);
    pub const IDENT_PATTERN: Self = Self(141);
    pub const WILDCARD_PATTERN: Self = Self(142);
    pub const TUPLE_PATTERN: Self = Self(143);
    pub const STRUCT_PATTERN: Self = Self(144);
    pub const ENUM_PATTERN: Self = Self(145);
    pub const REST_PATTERN: Self = Self(146);
    pub const GUARD_PATTERN: Self = Self(147);
    pub const RENAME_PATTERN: Self = Self(148);
    pub const OR_PATTERN: Self = Self(149);

    pub const TRIVIA_WS: Self = Self(200);
    pub const TRIVIA_NEWLINE: Self = Self(201);
    pub const TRIVIA_LINE_COMMENT: Self = Self(202);
    pub const TRIVIA_BLOCK_COMMENT: Self = Self(203);
    pub const TRIVIA_DOC_COMMENT: Self = Self(204);
    pub const TRIVIA_MOD_DOC_COMMENT: Self = Self(205);

    pub const ERROR: Self = Self(u16::MAX);

    pub fn is_trivia(self) -> bool {
        self.0 >= 200 && self.0 < 300
    }
}
