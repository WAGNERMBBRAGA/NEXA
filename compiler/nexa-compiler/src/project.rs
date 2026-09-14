//! Projection do AST para tooling e CTS (Implementação 02 §479-483, §696-700).
//!
//! Formato normalizado e estável (NÃO a serialização Rust interna):
//! `{ kind, span, ... estrutura de filhos importante }`. Os nomes `kind` são
//! protocol/linguagem, não nomes de enum do implementador (§699, §483).
//!
//! ```json
//! { "kind": "FunctionDeclaration", "name": "add",
//!   "parameters": [ {"name": "a", "type": "Int"}, {"name": "b", "type": "Int"} ] }
//! ```
//!
//! `parserProjectionSchemaVersion = 1` (§700).

use nexa_ast::{
    AssignOp, AssignTarget, BinaryOp, Block, ContractKind, ElseIf, EnumVariantKind, Expr, ExprKind,
    Item, ItemKind, Pattern, PatternKind, ReceiverKind, SourceUnit, Stmt, StmtKind, StringLiteral,
    Type, TypeKind, UnaryOp,
};
use serde_json::{json, Value};

/// Versão do schema de projeção (§700).
pub const PARSER_PROJECTION_SCHEMA_VERSION: u32 = 1;

/// Projeção estrutural completa de uma unidade de compilação.
pub fn ast_projection(unit: &SourceUnit) -> Value {
    json!({
        "kind": "SourceUnit",
        "span": span_value(unit.span),
        "module": unit.module.as_ref().map(module_projection),
        "imports": unit.imports.iter().map(import_projection).collect::<Vec<_>>(),
        "items": unit.items.iter().map(item_projection).collect::<Vec<_>>(),
    })
}

// ---------------------------------------------------------------------------
// declarações de topo
// ---------------------------------------------------------------------------

fn module_projection(m: &nexa_ast::ModuleDeclaration) -> Value {
    json!({
        "kind": "ModuleDeclaration",
        "name": name_value(&m.name),
        "span": span_value(m.span),
    })
}

fn import_projection(i: &nexa_ast::ImportDeclaration) -> Value {
    json!({
        "kind": "ImportDeclaration",
        "path": name_value(&i.path),
        "alias": i.alias.as_ref().map(|a| json!(a.name)),
        "span": span_value(i.span),
    })
}

fn item_projection(item: &Item) -> Value {
    let mut base = serde_json::Map::new();
    base.insert("kind".into(), json!(item_kind_name(&item.kind)));
    base.insert("exported".into(), json!(item.exported));
    base.insert("span".into(), span_value(item.span));
    match &item.kind {
        ItemKind::Function(f) => {
            base.insert("name".into(), json!(f.name.name));
            base.insert(
                "genericParams".into(),
                json!(generic_params(&f.generic_params)),
            );
            base.insert("parameters".into(), json!(params(&f.params)));
            base.insert(
                "returnType".into(),
                json!(f.return_type.as_ref().map(type_name)),
            );
            base.insert("whereClause".into(), json!(where_clause(&f.where_clause)));
            base.insert("requires".into(), json!(contracts(&f.requires)));
            base.insert("ensures".into(), json!(contracts(&f.ensures)));
            base.insert("body".into(), block_projection(&f.body));
        }
        ItemKind::Action(a) => {
            base.insert("name".into(), json!(a.name.name));
            base.insert(
                "genericParams".into(),
                json!(generic_params(&a.generic_params)),
            );
            base.insert("isAsync".into(), json!(a.is_async));
            base.insert("parameters".into(), json!(params(&a.params)));
            base.insert(
                "returnType".into(),
                json!(a.return_type.as_ref().map(type_name)),
            );
            base.insert(
                "effects".into(),
                json!(a
                    .effects
                    .as_ref()
                    .map(|e| effect_list(&e.effects))
                    .unwrap_or_default()),
            );
            base.insert("whereClause".into(), json!(where_clause(&a.where_clause)));
            base.insert("requires".into(), json!(contracts(&a.requires)));
            base.insert("ensures".into(), json!(contracts(&a.ensures)));
            base.insert("body".into(), block_projection(&a.body));
        }
        ItemKind::Struct(s) => {
            base.insert("name".into(), json!(s.name.name));
            base.insert(
                "genericParams".into(),
                json!(generic_params(&s.generic_params)),
            );
            base.insert(
                "fields".into(),
                json!(s
                    .fields
                    .iter()
                    .map(|f| {
                        json!({
                            "name": f.name.name,
                            "type": type_name(&f.ty),
                            "exported": f.exported,
                        })
                    })
                    .collect::<Vec<_>>()),
            );
        }
        ItemKind::Enum(e) => {
            base.insert("name".into(), json!(e.name.name));
            base.insert(
                "genericParams".into(),
                json!(generic_params(&e.generic_params)),
            );
            base.insert(
                "variants".into(),
                json!(e
                    .variants
                    .iter()
                    .map(|v| {
                        let (kind, detail) = match &v.kind {
                            EnumVariantKind::Unit => ("Unit", json!(null)),
                            EnumVariantKind::Tuple(types) => (
                                "Tuple",
                                json!(types.iter().map(type_name).collect::<Vec<_>>()),
                            ),
                            EnumVariantKind::Struct(fields) => (
                                "Struct",
                                json!(fields
                                    .iter()
                                    .map(|f| json!({"name": f.name.name, "type": type_name(&f.ty)}))
                                    .collect::<Vec<_>>()),
                            ),
                        };
                        json!({"name": v.name.name, "kind": kind, "payload": detail})
                    })
                    .collect::<Vec<_>>()),
            );
        }
        ItemKind::Interface(i) => {
            base.insert("name".into(), json!(i.name.name));
            base.insert(
                "genericParams".into(),
                json!(generic_params(&i.generic_params)),
            );
            base.insert(
                "methods".into(),
                json!(i.methods
                    .iter()
                    .map(|m| {
                        json!({
                            "name": m.name.name,
                            "receiver": receiver_name(m.receiver),
                            "parameters": params(&m.params),
                            "returnType": m.return_type.as_ref().map(type_name),
                            "effects": m.effects.as_ref().map(|e| effect_list(&e.effects)).unwrap_or_default(),
                        })
                    })
                    .collect::<Vec<_>>()),
            );
        }
        ItemKind::Implement(im) => {
            base.insert("trait".into(), json!(name_value(&im.trait_path)));
            base.insert("type".into(), json!(type_name(&im.for_type)));
            base.insert(
                "genericParams".into(),
                json!(generic_params(&im.generic_params)),
            );
            base.insert(
                "methods".into(),
                json!(im.methods
                    .iter()
                    .map(|m| {
                        json!({
                            "name": m.name.name,
                            "receiver": receiver_name(m.receiver),
                            "parameters": params(&m.params),
                            "returnType": m.return_type.as_ref().map(type_name),
                            "effects": m.effects.as_ref().map(|e| effect_list(&e.effects)).unwrap_or_default(),
                            "hasBody": m.body.is_some(),
                        })
                    })
                    .collect::<Vec<_>>()),
            );
        }
        ItemKind::TypeAlias(t) => {
            base.insert("name".into(), json!(t.name.name));
            base.insert(
                "genericParams".into(),
                json!(generic_params(&t.generic_params)),
            );
            base.insert("distinct".into(), json!(t.is_distinct));
            base.insert("type".into(), json!(type_name(&t.ty)));
        }
        ItemKind::Const(c) => {
            base.insert("name".into(), json!(c.name.name));
            base.insert("type".into(), json!(c.ty.as_ref().map(type_name)));
            base.insert("init".into(), expr_projection(&c.init));
        }
    }
    Value::Object(base)
}

fn item_kind_name(kind: &ItemKind) -> &'static str {
    match kind {
        ItemKind::Function(_) => "FunctionDeclaration",
        ItemKind::Action(_) => "ActionDeclaration",
        ItemKind::Struct(_) => "StructDeclaration",
        ItemKind::Enum(_) => "EnumDeclaration",
        ItemKind::Interface(_) => "InterfaceDeclaration",
        ItemKind::Implement(_) => "ImplementDeclaration",
        ItemKind::TypeAlias(_) => "TypeAliasDeclaration",
        ItemKind::Const(_) => "ConstDeclaration",
    }
}

// ---------------------------------------------------------------------------
// tipos & assinaturas
// ---------------------------------------------------------------------------

fn generic_params(ps: &[nexa_ast::GenericParam]) -> Vec<Value> {
    ps.iter()
        .map(|p| json!({"name": p.name.name, "bounds": type_bounds(&p.bounds)}))
        .collect()
}

fn type_bounds(bs: &[nexa_ast::TypeBound]) -> Vec<Value> {
    bs.iter()
        .map(|b| {
            let mut out = name_value(&b.path);
            if !b.generic_args.is_empty() {
                out = format!(
                    "{}<{}>",
                    out,
                    b.generic_args
                        .iter()
                        .map(type_name)
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            json!(out)
        })
        .collect()
}

fn params(ps: &[nexa_ast::Param]) -> Vec<Value> {
    ps.iter()
        .map(|p| json!({"name": p.name.name, "type": type_name(&p.ty)}))
        .collect()
}

fn receiver_name(r: ReceiverKind) -> &'static str {
    match r {
        ReceiverKind::Self_ => "self",
        ReceiverKind::RefSelf => "ref self",
        ReceiverKind::RefMutSelf => "ref mut self",
    }
}

fn where_clause(ws: &[nexa_ast::WherePredicate]) -> Vec<Value> {
    ws.iter()
        .map(|w| json!({"typeName": w.type_name.name, "bounds": type_bounds(&w.bounds)}))
        .collect()
}

fn contracts(cs: &[nexa_ast::ContractExpr]) -> Vec<Value> {
    cs.iter()
        .map(|c| {
            json!({
                "kind": match c.kind { ContractKind::Require => "Require", ContractKind::Ensure => "Ensure" },
                "expression": expr_projection(&c.expr),
            })
        })
        .collect()
}

fn effect_list(effects: &[nexa_ast::EffectPath]) -> Vec<Value> {
    effects.iter().map(|e| json!(name_value(&e.path))).collect()
}

/// Nome renderizado canônico de um tipo (string normalizada §697).
fn type_name(t: &Type) -> String {
    match &t.kind {
        TypeKind::Path(p) => name_value(p),
        TypeKind::Unit => "Unit".to_string(),
        TypeKind::Generic { path, args } => {
            let inner: Vec<String> = args.iter().map(type_name).collect();
            format!("{}<{}>", name_value(path), inner.join(", "))
        }
        TypeKind::Array(inner) => format!("[{}]", type_name(inner)),
        TypeKind::Optional(inner) => format!("Optional<{}>", type_name(inner)),
        TypeKind::Result { ok, err } => {
            format!("Result<{}, {}>", type_name(ok), type_name(err))
        }
        TypeKind::Ref(inner) => format!("ref {}", type_name(inner)),
        TypeKind::RefMut(inner) => format!("ref mut {}", type_name(inner)),
        TypeKind::Tuple(elems) => {
            let inner: Vec<String> = elems.iter().map(type_name).collect();
            format!("({})", inner.join(", "))
        }
        TypeKind::Function { params, ret } => {
            let inner: Vec<String> = params.iter().map(type_name).collect();
            format!("fn({}) -> {}", inner.join(", "), type_name(ret))
        }
    }
}

fn name_value(q: &nexa_ast::QualifiedName) -> String {
    q.segments
        .iter()
        .map(|s| s.name.as_str())
        .collect::<Vec<_>>()
        .join("::")
}

fn span_value(span: nexa_source::SourceSpan) -> Value {
    json!({"startByte": span.start, "endByte": span.end})
}

// ---------------------------------------------------------------------------
// corpos (statements / expressões / padrões)
// ---------------------------------------------------------------------------

fn block_projection(b: &Block) -> Value {
    json!({
        "kind": "Block",
        "span": span_value(b.span),
        "statements": b.stmts.iter().map(statement_projection).collect::<Vec<_>>(),
    })
}

fn statement_projection(s: &Stmt) -> Value {
    let mut base = serde_json::Map::new();
    base.insert("span".into(), span_value(s.span));
    match &s.kind {
        StmtKind::Let(l) => {
            base.insert("kind".into(), json!("LetStatement"));
            base.insert("name".into(), json!(l.name.name));
            base.insert("type".into(), json!(l.ty.as_ref().map(type_name)));
            base.insert("init".into(), expr_projection(&l.init));
        }
        StmtKind::Var(v) => {
            base.insert("kind".into(), json!("VarStatement"));
            base.insert("name".into(), json!(v.name.name));
            base.insert("type".into(), json!(v.ty.as_ref().map(type_name)));
            base.insert(
                "init".into(),
                v.init.as_ref().map(expr_projection).unwrap_or(Value::Null),
            );
        }
        StmtKind::Const(c) => {
            base.insert("kind".into(), json!("ConstStatement"));
            base.insert("name".into(), json!(c.name.name));
            base.insert("type".into(), json!(c.ty.as_ref().map(type_name)));
            base.insert("init".into(), expr_projection(&c.init));
        }
        StmtKind::Assign(target, op, value) => {
            base.insert("kind".into(), json!("AssignStatement"));
            base.insert("target".into(), assign_target_projection(target));
            base.insert("operator".into(), json!(assign_op(op)));
            base.insert("value".into(), expr_projection(value));
        }
        StmtKind::Expr(e) => {
            base.insert("kind".into(), json!("ExpressionStatement"));
            base.insert("expression".into(), expr_projection(e));
        }
        StmtKind::Return(r) => {
            base.insert("kind".into(), json!("ReturnStatement"));
            base.insert(
                "value".into(),
                r.value.as_ref().map(expr_projection).unwrap_or(Value::Null),
            );
        }
        StmtKind::Break(b) => {
            base.insert("kind".into(), json!("BreakStatement"));
            base.insert(
                "value".into(),
                b.value.as_ref().map(expr_projection).unwrap_or(Value::Null),
            );
        }
        StmtKind::Continue(_) => {
            base.insert("kind".into(), json!("ContinueStatement"));
        }
        StmtKind::Discard(d) => {
            base.insert("kind".into(), json!("DiscardStatement"));
            base.insert("value".into(), expr_projection(&d.expr));
        }
    }
    Value::Object(base)
}

fn assign_target_projection(t: &AssignTarget) -> Value {
    match t {
        AssignTarget::Ident(i) => json!({"kind": "Identifier", "name": i.name}),
        AssignTarget::Field(target, name) => json!({
            "kind": "MemberAccess",
            "target": expr_projection(target),
            "member": name.name,
        }),
        AssignTarget::Index(target, index) => json!({
            "kind": "IndexExpression",
            "target": expr_projection(target),
            "index": expr_projection(index),
        }),
    }
}

fn assign_op(op: &AssignOp) -> &'static str {
    match op {
        AssignOp::Equal => "=",
        AssignOp::PlusEqual => "+=",
        AssignOp::MinusEqual => "-=",
        AssignOp::StarEqual => "*=",
        AssignOp::SlashEqual => "/=",
        AssignOp::PercentEqual => "%=",
        AssignOp::AmpEqual => "&=",
        AssignOp::PipeEqual => "|=",
        AssignOp::CaretEqual => "^=",
        AssignOp::ShiftLeftEqual => "<<=",
        AssignOp::ShiftRightEqual => ">>=",
    }
}

fn expr_projection(e: &Expr) -> Value {
    let mut base = serde_json::Map::new();
    base.insert("span".into(), span_value(e.span));
    match &e.kind {
        ExprKind::IntLiteral(v) => {
            base.insert("kind".into(), json!("IntLiteral"));
            base.insert("value".into(), json!(v));
        }
        ExprKind::FloatLiteral(v) => {
            base.insert("kind".into(), json!("FloatLiteral"));
            base.insert("value".into(), json!(v));
        }
        ExprKind::CharLiteral(c) => {
            base.insert("kind".into(), json!("CharLiteral"));
            base.insert("value".into(), json!(c.to_string()));
        }
        ExprKind::BoolLiteral(b) => {
            base.insert("kind".into(), json!("BoolLiteral"));
            base.insert("value".into(), json!(b));
        }
        ExprKind::Ident(i) => {
            base.insert("kind".into(), json!("Identifier"));
            base.insert("name".into(), json!(i.name));
        }
        ExprKind::Path(p) => {
            base.insert("kind".into(), json!("Path"));
            base.insert("name".into(), json!(name_value(p)));
        }
        ExprKind::Underscore => {
            base.insert("kind".into(), json!("Underscore"));
        }
        ExprKind::StringLiteral(s) => {
            base.insert("kind".into(), json!("StringLiteral"));
            base.insert("multiline".into(), json!(s.multiline));
            base.insert("parts".into(), json!(string_parts(s)));
        }
        ExprKind::ByteStringLiteral(b) => {
            base.insert("kind".into(), json!("ByteStringLiteral"));
            base.insert("value".into(), json!(b));
        }
        ExprKind::Binary(op, l, r) => {
            base.insert("kind".into(), json!("BinaryExpression"));
            base.insert("operator".into(), json!(binary_op(op)));
            base.insert("left".into(), expr_projection(l));
            base.insert("right".into(), expr_projection(r));
        }
        ExprKind::Unary(op, operand) => {
            base.insert("kind".into(), json!("UnaryExpression"));
            base.insert("operator".into(), json!(unary_op(op)));
            base.insert("operand".into(), expr_projection(operand));
        }
        ExprKind::Call { callee, args } => {
            base.insert("kind".into(), json!("CallExpression"));
            base.insert("callee".into(), expr_projection(callee));
            base.insert(
                "arguments".into(),
                json!(args.iter().map(expr_projection).collect::<Vec<_>>()),
            );
        }
        ExprKind::Index { target, index } => {
            base.insert("kind".into(), json!("IndexExpression"));
            base.insert("target".into(), expr_projection(target));
            base.insert("index".into(), expr_projection(index));
        }
        ExprKind::Field { target, name } => {
            base.insert("kind".into(), json!("MemberAccess"));
            base.insert("target".into(), expr_projection(target));
            base.insert("member".into(), json!(name.name));
        }
        ExprKind::MethodCall { target, name, args } => {
            base.insert("kind".into(), json!("MethodCall"));
            base.insert("target".into(), expr_projection(target));
            base.insert("method".into(), json!(name.name));
            base.insert(
                "arguments".into(),
                json!(args.iter().map(expr_projection).collect::<Vec<_>>()),
            );
        }
        ExprKind::Await(inner) => {
            base.insert("kind".into(), json!("Await"));
            base.insert("operand".into(), expr_projection(inner));
        }
        ExprKind::Try(inner) => {
            base.insert("kind".into(), json!("Try"));
            base.insert("operand".into(), expr_projection(inner));
        }
        ExprKind::Ref(inner) => {
            base.insert("kind".into(), json!("Ref"));
            base.insert("operand".into(), expr_projection(inner));
        }
        ExprKind::RefMut(inner) => {
            base.insert("kind".into(), json!("RefMut"));
            base.insert("operand".into(), expr_projection(inner));
        }
        ExprKind::Move(inner) => {
            base.insert("kind".into(), json!("Move"));
            base.insert("operand".into(), expr_projection(inner));
        }
        ExprKind::Paren(inner) => {
            base.insert("kind".into(), json!("Parenthesized"));
            base.insert("expression".into(), expr_projection(inner));
        }
        ExprKind::Array(elems) => {
            base.insert("kind".into(), json!("ArrayLiteral"));
            base.insert(
                "elements".into(),
                json!(elems.iter().map(expr_projection).collect::<Vec<_>>()),
            );
        }
        ExprKind::Tuple(elems) => {
            base.insert("kind".into(), json!("Unit"));
            base.insert(
                "elements".into(),
                json!(elems.iter().map(expr_projection).collect::<Vec<_>>()),
            );
        }
        ExprKind::StructConstruct(sc) => {
            base.insert("kind".into(), json!("StructConstruct"));
            base.insert("target".into(), json!(name_value(&sc.path)));
            base.insert(
                "fields".into(),
                json!(sc
                    .fields
                    .iter()
                    .map(|f| {
                        json!({
                            "name": f.name.name,
                            "value": expr_projection(&f.expr),
                        })
                    })
                    .collect::<Vec<_>>()),
            );
        }
        ExprKind::Block(b) => {
            base.insert("kind".into(), json!("BlockExpression"));
            base.insert(
                "statements".into(),
                json!(b.stmts.iter().map(statement_projection).collect::<Vec<_>>()),
            );
        }
        ExprKind::Unsafe(b) => {
            base.insert("kind".into(), json!("UnsafeExpression"));
            base.insert(
                "statements".into(),
                json!(b.stmts.iter().map(statement_projection).collect::<Vec<_>>()),
            );
        }
        ExprKind::If(if_expr) => {
            base.insert("kind".into(), json!("IfExpression"));
            base.insert("condition".into(), expr_projection(&if_expr.condition));
            base.insert("then".into(), block_into_value(&if_expr.then_block));
            base.insert(
                "elseIfs".into(),
                json!(if_expr
                    .else_ifs
                    .iter()
                    .map(else_if_projection)
                    .collect::<Vec<_>>()),
            );
            base.insert(
                "else".into(),
                if_expr
                    .else_block
                    .as_ref()
                    .map(block_into_value)
                    .unwrap_or(Value::Null),
            );
        }
        ExprKind::Match(m) => {
            base.insert("kind".into(), json!("MatchExpression"));
            base.insert("scrutinee".into(), expr_projection(&m.scrutinee));
            base.insert(
                "arms".into(),
                json!(m.arms
                    .iter()
                    .map(|arm| {
                        json!({
                            "pattern": pattern_projection(&arm.pattern),
                            "guard": arm.guard.as_ref().map(|g| expr_projection(g)).unwrap_or(Value::Null),
                            "body": expr_projection(&arm.body),
                        })
                    })
                    .collect::<Vec<_>>()),
            );
        }
        ExprKind::For(f) => {
            base.insert("kind".into(), json!("ForExpression"));
            base.insert("variable".into(), json!(f.variable.name));
            base.insert("bindingMode".into(), json!(for_binding(&f.binding_mode)));
            base.insert("iterable".into(), expr_projection(&f.iterable));
            base.insert("body".into(), block_into_value(&f.body));
        }
        ExprKind::While(w) => {
            base.insert("kind".into(), json!("WhileExpression"));
            base.insert("condition".into(), expr_projection(&w.condition));
            base.insert("body".into(), block_into_value(&w.body));
        }
        ExprKind::Loop(l) => {
            base.insert("kind".into(), json!("LoopExpression"));
            base.insert("body".into(), block_into_value(&l.body));
        }
        ExprKind::AssignExpr(target, op, value) => {
            base.insert("kind".into(), json!("AssignExpression"));
            base.insert("target".into(), assign_target_projection(target));
            base.insert("operator".into(), json!(assign_op(op)));
            base.insert("value".into(), expr_projection(value));
        }
    }
    Value::Object(base)
}

fn block_into_value(b: &Block) -> Value {
    json!({
        "kind": "Block",
        "span": span_value(b.span),
        "statements": b.stmts.iter().map(statement_projection).collect::<Vec<_>>(),
    })
}

fn else_if_projection(e: &ElseIf) -> Value {
    json!({
        "condition": expr_projection(&e.condition),
        "body": block_into_value(&e.block),
    })
}

fn string_parts(s: &StringLiteral) -> Vec<Value> {
    s.parts
        .iter()
        .map(|part| match part {
            nexa_ast::StringPart::Text(t) => json!({"kind": "Text", "text": t}),
            nexa_ast::StringPart::Interpolation { expr, .. } => {
                json!({"kind": "Interpolation", "expression": expr_projection(expr)})
            }
        })
        .collect()
}

fn for_binding(m: &nexa_ast::ForBindingMode) -> &'static str {
    match m {
        nexa_ast::ForBindingMode::Value => "value",
        nexa_ast::ForBindingMode::Ref => "ref",
        nexa_ast::ForBindingMode::RefMut => "ref mut",
        nexa_ast::ForBindingMode::Move => "move",
    }
}

fn binary_op(op: &BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::Rem => "%",
        BinaryOp::Eq => "==",
        BinaryOp::Ne => "!=",
        BinaryOp::Lt => "<",
        BinaryOp::Le => "<=",
        BinaryOp::Gt => ">",
        BinaryOp::Ge => ">=",
        BinaryOp::And => "&&",
        BinaryOp::Or => "||",
        BinaryOp::BitAnd => "&",
        BinaryOp::BitOr => "|",
        BinaryOp::BitXor => "^",
        BinaryOp::Shl => "<<",
        BinaryOp::Shr => ">>",
    }
}

fn unary_op(op: &UnaryOp) -> &'static str {
    match op {
        UnaryOp::Neg => "-",
        UnaryOp::Plus => "+",
        UnaryOp::Not => "!",
        UnaryOp::BitNot => "~",
    }
}

fn pattern_projection(p: &Pattern) -> Value {
    let mut base = serde_json::Map::new();
    base.insert("span".into(), span_value(p.span));
    match &p.kind {
        PatternKind::Literal(e) => {
            base.insert("kind".into(), json!("LiteralPattern"));
            base.insert("value".into(), expr_projection(e));
        }
        PatternKind::Ident(i) => {
            base.insert("kind".into(), json!("IdentifierPattern"));
            base.insert("name".into(), json!(i.name));
        }
        PatternKind::Wildcard => {
            base.insert("kind".into(), json!("WildcardPattern"));
        }
        PatternKind::Rest => {
            base.insert("kind".into(), json!("RestPattern"));
        }
        PatternKind::Tuple(elems) => {
            base.insert("kind".into(), json!("TuplePattern"));
            base.insert(
                "elements".into(),
                json!(elems.iter().map(pattern_projection).collect::<Vec<_>>()),
            );
        }
        PatternKind::Struct { path, fields } => {
            base.insert("kind".into(), json!("StructPattern"));
            base.insert("path".into(), json!(name_value(path)));
            base.insert(
                "fields".into(),
                json!(fields
                    .iter()
                    .map(|f| {
                        json!({
                            "name": f.name.name,
                            "pattern": f.pattern.as_ref().map(pattern_projection).unwrap_or(Value::Null),
                        })
                    })
                    .collect::<Vec<_>>()),
            );
        }
        PatternKind::Enum {
            path,
            variant,
            pattern,
        } => {
            base.insert("kind".into(), json!("EnumPattern"));
            base.insert("path".into(), json!(name_value(path)));
            base.insert("variant".into(), json!(variant.name));
            base.insert(
                "pattern".into(),
                pattern
                    .as_deref()
                    .map(pattern_projection)
                    .unwrap_or(Value::Null),
            );
        }
        PatternKind::Or(alternatives) => {
            base.insert("kind".into(), json!("OrPattern"));
            base.insert(
                "alternatives".into(),
                json!(alternatives
                    .iter()
                    .map(pattern_projection)
                    .collect::<Vec<_>>()),
            );
        }
        PatternKind::Rename { pattern, alias } => {
            base.insert("kind".into(), json!("RenamePattern"));
            base.insert("pattern".into(), pattern_projection(pattern));
            base.insert("alias".into(), json!(alias.name));
        }
        PatternKind::Guard { pattern, condition } => {
            base.insert("kind".into(), json!("GuardedPattern"));
            base.insert("pattern".into(), pattern_projection(pattern));
            base.insert("condition".into(), expr_projection(condition));
        }
    }
    Value::Object(base)
}
