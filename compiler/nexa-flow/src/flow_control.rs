//! Flow Control Primitives
//! Represents let, if, match, while, for blocks and their semantics

use nexa_source::SourceSpan;
use nexa_symbols::SymbolId;

/// Represents a single operation in flow control
#[derive(Debug, Clone)]
pub enum FlowOp {
    /// Let binding: `let x = e`
    Let { binding: SymbolId, expr: Expr },
    /// If expression: `if cond then block else block`
    If {
        cond: Expr,
        if_block: Block,
        else_block: Option<Block>,
    },
    /// Match expression with arms
    Match {
        subject: Expr,
        arms: Vec<MatchArm>,
        tail: Option<Expr>,
    },
    /// While loop: `while cond { body }`
    While { cond: Expr, body: Block },
    /// For loop: `for i in range { body }` or `for item in collection { body }`
    For {
        iter_expr: Expr,
        init_expr: Option<Expr>,
        update_expr: Option<Expr>,
        body: Block,
    },
    /// Yield expression (generators)
    Yield { expr: Expr },
    /// Spawn (async tasks)
    Spawn { task: Box<Block> },
}

/// Expression type for flow control
#[derive(Debug, Clone)]
pub enum Expr {
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Call {
        func: SymbolId,
        args: Vec<Expr>,
    },
    Literal(Literal),
}

/// Binary operation type
#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Modulo,
    Eq,
    Neq,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    NotEq,
    BitAnd,
    BitOr,
    ShiftL,
    ShiftR,
}

/// Literal types
#[derive(Debug, Clone)]
pub enum Literal {
    /// Integer literal (simplified to i64)
    Int(i64),
    /// Float literal (f64)
    Float(f64),
    /// Boolean literal
    Bool(bool),
    /// String literal
    String(String),
    /// Character literal
    Char(char),
    /// None value
    None,
    /// Some value (for Option type)
    Some,
}

/// Match arm for pattern matching
#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub guard: Option<Expr>,
    pub body: Expr,
}

/// Patterns used in match expressions
#[derive(Debug, Clone)]
pub enum Pattern {
    /// Literal pattern for a specific value
    Literal(Literal),
    /// Wildcard pattern (matches anything)
    Wildcard,
    /// Variable binding pattern `x` or `(x, y)`
    Binding(SymbolId),
}

/// Represents the flow control body of an expression
#[derive(Debug, Clone)]
pub struct Block {
    pub id: u32,
    pub span: Option<SourceSpan>,
    pub ops: Vec<FlowOp>,
    pub terminator: Terminator,
}

/// Termination type for a block (return, panic, fallthrough, etc.)
#[derive(Debug, Clone)]
pub enum Terminator {
    /// Return value or implicit return None
    Return(Option<Expr>),
    /// Panic with optional message expression
    Panic(Option<Expr>),
    /// Fall through to the next statement (if/while)
    FallThrough,
    /// Break from loop (for/while)
    Break(BlockId),
    /// Continue in loop (for/while)
    Continue(BlockId),
}

/// Identifier for a block
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockId(u32);

impl BlockId {
    pub fn new(id: u32) -> Self {
        Self(id)
    }

    pub fn get(self) -> u32 {
        self.0
    }
}
