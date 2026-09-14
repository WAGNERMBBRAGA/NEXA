use nexa_source::SourceSpan;

#[derive(Debug, Clone)]
pub struct SourceUnit {
    pub module: Option<ModuleDeclaration>,
    pub imports: Vec<ImportDeclaration>,
    pub items: Vec<Item>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct ModuleDeclaration {
    pub name: QualifiedName,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct ImportDeclaration {
    pub path: QualifiedName,
    pub alias: Option<Ident>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct QualifiedName {
    pub segments: Vec<Ident>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct Ident {
    pub name: String,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct Item {
    pub attrs: Vec<Attribute>,
    pub exported: bool,
    pub kind: ItemKind,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub enum ItemKind {
    Function(FunctionDecl),
    Action(ActionDecl),
    Struct(StructDecl),
    Enum(EnumDecl),
    Interface(InterfaceDecl),
    Implement(ImplDecl),
    TypeAlias(TypeAliasDecl),
    Const(ConstDecl),
}

#[derive(Debug, Clone)]
pub struct FunctionDecl {
    pub name: Ident,
    pub generic_params: Vec<GenericParam>,
    pub params: Vec<Param>,
    pub return_type: Option<Type>,
    pub where_clause: Vec<WherePredicate>,
    pub requires: Vec<ContractExpr>,
    pub ensures: Vec<ContractExpr>,
    pub body: Block,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct ActionDecl {
    pub name: Ident,
    pub generic_params: Vec<GenericParam>,
    pub params: Vec<Param>,
    pub return_type: Option<Type>,
    pub is_async: bool,
    pub effects: Option<EffectsClause>,
    pub where_clause: Vec<WherePredicate>,
    pub requires: Vec<ContractExpr>,
    pub ensures: Vec<ContractExpr>,
    pub body: Block,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: Ident,
    pub ty: Type,
    pub mutable: bool,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct GenericParam {
    pub name: Ident,
    pub bounds: Vec<TypeBound>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct TypeBound {
    pub path: QualifiedName,
    pub generic_args: Vec<Type>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct WherePredicate {
    pub type_name: Ident,
    pub bounds: Vec<TypeBound>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct EffectsClause {
    pub effects: Vec<EffectPath>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct EffectPath {
    pub path: QualifiedName,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct ContractExpr {
    pub kind: ContractKind,
    pub expr: Expr,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractKind {
    Require,
    Ensure,
}

#[derive(Debug, Clone)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub enum StmtKind {
    Let(LetBinding),
    Var(VarBinding),
    Const(ConstBinding),
    Assign(AssignTarget, AssignOp, Expr),
    Expr(Expr),
    Return(ReturnStmt),
    Break(BreakStmt),
    Continue(ContinueStmt),
    Discard(DiscardStmt),
}

#[derive(Debug, Clone)]
pub struct LetBinding {
    pub name: Ident,
    pub ty: Option<Type>,
    pub init: Expr,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct VarBinding {
    pub name: Ident,
    pub ty: Option<Type>,
    pub init: Option<Expr>,
    pub span: SourceSpan,
}

/// `const` local: compile-time immutable value (§44-45).
#[derive(Debug, Clone)]
pub struct ConstBinding {
    pub name: Ident,
    pub ty: Option<Type>,
    pub init: Expr,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub enum AssignTarget {
    Ident(Ident),
    Field(Box<Expr>, Ident),
    Index(Box<Expr>, Box<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Equal,
    PlusEqual,
    MinusEqual,
    StarEqual,
    SlashEqual,
    PercentEqual,
    AmpEqual,
    PipeEqual,
    CaretEqual,
    ShiftLeftEqual,
    ShiftRightEqual,
}

#[derive(Debug, Clone)]
pub struct ReturnStmt {
    pub value: Option<Expr>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct BreakStmt {
    pub value: Option<Expr>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct ContinueStmt {
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct DiscardStmt {
    pub expr: Expr,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub enum ExprKind {
    IntLiteral(i64),
    FloatLiteral(f64),
    StringLiteral(StringLiteral),
    ByteStringLiteral(Vec<u8>),
    CharLiteral(char),
    BoolLiteral(bool),
    Ident(Ident),
    Path(QualifiedName),
    Underscore,
    Binary(BinaryOp, Box<Expr>, Box<Expr>),
    Unary(UnaryOp, Box<Expr>),
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    Index {
        target: Box<Expr>,
        index: Box<Expr>,
    },
    Field {
        target: Box<Expr>,
        name: Ident,
    },
    MethodCall {
        target: Box<Expr>,
        name: Ident,
        args: Vec<Expr>,
    },
    Await(Box<Expr>),
    Try(Box<Expr>),
    Ref(Box<Expr>),
    RefMut(Box<Expr>),
    Move(Box<Expr>),
    If(IfExpr),
    Match(MatchExpr),
    For(ForExpr),
    While(WhileExpr),
    Loop(LoopExpr),
    Block(Block),
    Unsafe(Block),
    Tuple(Vec<Expr>),
    Array(Vec<Expr>),
    StructConstruct(StructConstructExpr),
    Paren(Box<Expr>),
    AssignExpr(Box<AssignTarget>, AssignOp, Box<Expr>),
}

#[derive(Debug, Clone)]
pub struct StringLiteral {
    pub parts: Vec<StringPart>,
    pub multiline: bool,
}

#[derive(Debug, Clone)]
pub enum StringPart {
    Text(String),
    Interpolation { expr: Box<Expr>, span: SourceSpan },
}

/// Construção de struct literal (§130-141): `User { id: id, name }`.
#[derive(Debug, Clone)]
pub struct StructConstructExpr {
    pub path: QualifiedName,
    pub fields: Vec<StructLiteralField>,
    pub span: SourceSpan,
}

/// Um campo de struct literal, `name: expr` ou shorthand `name` (== `name: name`).
#[derive(Debug, Clone)]
pub struct StructLiteralField {
    pub name: Ident,
    pub expr: Expr,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct IfExpr {
    pub condition: Box<Expr>,
    pub then_block: Block,
    pub else_ifs: Vec<ElseIf>,
    pub else_block: Option<Block>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct ElseIf {
    pub condition: Box<Expr>,
    pub block: Block,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct MatchExpr {
    pub scrutinee: Box<Expr>,
    pub arms: Vec<MatchArm>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub guard: Option<Box<Expr>>,
    pub body: Expr,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct ForExpr {
    pub variable: Ident,
    pub binding_mode: ForBindingMode,
    pub iterable: Box<Expr>,
    pub body: Block,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForBindingMode {
    Value,
    Ref,
    RefMut,
    Move,
}

#[derive(Debug, Clone)]
pub struct WhileExpr {
    pub condition: Box<Expr>,
    pub body: Block,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct LoopExpr {
    pub body: Block,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct Pattern {
    pub kind: PatternKind,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub enum PatternKind {
    Literal(Expr),
    Ident(Ident),
    Wildcard,
    Rest,
    Tuple(Vec<Pattern>),
    Struct {
        path: QualifiedName,
        fields: Vec<FieldPattern>,
    },
    Enum {
        path: QualifiedName,
        variant: Ident,
        pattern: Option<Box<Pattern>>,
    },
    Or(Vec<Pattern>),
    Rename {
        pattern: Box<Pattern>,
        alias: Ident,
    },
    Guard {
        pattern: Box<Pattern>,
        condition: Box<Expr>,
    },
}

#[derive(Debug, Clone)]
pub struct FieldPattern {
    pub name: Ident,
    pub pattern: Option<Pattern>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Plus,
    Not,
    BitNot,
}

#[derive(Debug, Clone)]
pub struct StructDecl {
    pub name: Ident,
    pub generic_params: Vec<GenericParam>,
    pub fields: Vec<StructField>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct StructField {
    pub name: Ident,
    pub ty: Type,
    pub exported: bool,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct EnumDecl {
    pub name: Ident,
    pub generic_params: Vec<GenericParam>,
    pub variants: Vec<EnumVariant>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct EnumVariant {
    pub name: Ident,
    pub kind: EnumVariantKind,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub enum EnumVariantKind {
    Unit,
    Tuple(Vec<Type>),
    Struct(Vec<StructField>),
}

#[derive(Debug, Clone)]
pub struct InterfaceDecl {
    pub name: Ident,
    pub generic_params: Vec<GenericParam>,
    pub methods: Vec<InterfaceMethod>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct InterfaceMethod {
    pub name: Ident,
    pub receiver: ReceiverKind,
    pub params: Vec<Param>,
    pub return_type: Option<Type>,
    pub effects: Option<EffectsClause>,
    pub requires: Vec<ContractExpr>,
    pub ensures: Vec<ContractExpr>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct ImplDecl {
    pub trait_path: QualifiedName,
    pub for_type: Type,
    pub generic_params: Vec<GenericParam>,
    pub methods: Vec<ImplMethod>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct ImplMethod {
    pub name: Ident,
    pub receiver: ReceiverKind,
    pub params: Vec<Param>,
    pub return_type: Option<Type>,
    pub effects: Option<EffectsClause>,
    pub body: Option<Block>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceiverKind {
    Self_,
    RefSelf,
    RefMutSelf,
}

#[derive(Debug, Clone)]
pub struct TypeAliasDecl {
    pub name: Ident,
    pub generic_params: Vec<GenericParam>,
    pub is_distinct: bool,
    pub ty: Type,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct ConstDecl {
    pub name: Ident,
    pub ty: Option<Type>,
    pub init: Expr,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct Type {
    pub kind: TypeKind,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub enum TypeKind {
    Path(QualifiedName),
    Unit,
    Generic {
        path: QualifiedName,
        args: Vec<Type>,
    },
    Array(Box<Type>),
    Optional(Box<Type>),
    Result {
        ok: Box<Type>,
        err: Box<Type>,
    },
    Ref(Box<Type>),
    RefMut(Box<Type>),
    Tuple(Vec<Type>),
    Function {
        params: Vec<Type>,
        ret: Box<Type>,
    },
}

#[derive(Debug, Clone)]
pub struct Attribute {
    pub name: QualifiedName,
    pub args: Vec<Expr>,
    pub span: SourceSpan,
}
