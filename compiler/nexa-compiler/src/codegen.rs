//! Verified executable pipeline: AST → semantic lowering → NIR gate → WASM.

use nexa_ast::{
    AssignOp, AssignTarget, BinaryOp, Block, EnumVariantKind, Expr, ExprKind, ItemKind, StmtKind,
    StringPart, Type, TypeKind,
};
use nexa_backend_wasm::{compile_wasm, WasmBackendConfig};
use nexa_lowering::{
    LoweringPipeline, SemanticExpression, SemanticFunction, SemanticParameter, SemanticProgram,
    SemanticStatement, SemanticTypeDecl, SemanticTypeKind,
};
use nexa_nir::{NirCallableKind, NirPassingMode, VerifiedNirModule};
use nexa_nir_verify::NirVerifier;
use nexa_wasm_abi::target::{BuildProfile, OutputKind};

use crate::Pipeline;

#[derive(Debug, Clone)]
pub struct CompileArtifact {
    pub wasm: Vec<u8>,
    pub function_count: usize,
    pub type_count: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum CompileError {
    #[error("source validation failed with {0} error(s)")]
    SourceErrors(usize),
    #[error("unsupported executable construct: {0}")]
    Unsupported(String),
    #[error("lowering failed: {0:?}")]
    Lowering(nexa_lowering::LoweringError),
    #[error("NIR verification failed: {0}")]
    Verification(String),
    #[error("WASM backend failed: {0}")]
    Backend(#[from] nexa_backend_wasm::WasmBackendError),
}

/// Compile the currently executable NEXA subset. The frontend check must pass,
/// and the NIR verifier is an obligatory gate before the backend can run.
pub fn compile_source_to_wasm(
    display_name: &str,
    source: &str,
) -> Result<CompileArtifact, CompileError> {
    let mut pipeline = Pipeline::new();
    let checked = pipeline.check_source(display_name, source);
    if checked.has_errors() {
        return Err(CompileError::SourceErrors(checked.error_count()));
    }

    let parsed = pipeline.parse_source(display_name, source);
    if parsed
        .diagnostics
        .iter()
        .any(|d| d.severity == crate::Severity::Error)
    {
        return Err(CompileError::SourceErrors(
            parsed
                .diagnostics
                .iter()
                .filter(|d| d.severity == crate::Severity::Error)
                .count(),
        ));
    }

    let program = semantic_program(&parsed.ast)?;
    if !program.functions.iter().any(|f| f.name == "main") {
        return Err(CompileError::Unsupported(
            "an application requires a `main` callable".to_string(),
        ));
    }

    let (nir, _hnir, mnir) =
        LoweringPipeline::lower_full(&program).map_err(CompileError::Lowering)?;
    NirVerifier::verify_module(&nir, &mnir).map_err(|errors| {
        CompileError::Verification(
            errors
                .iter()
                .map(|e| format!("{:?}: {}", e.code, e.message))
                .collect::<Vec<_>>()
                .join("; "),
        )
    })?;

    let function_count = nir.functions.len();
    let type_count = nir.types.len();
    let verified = VerifiedNirModule::new(nir);
    let artifact = compile_wasm(
        &verified,
        &WasmBackendConfig {
            profile: BuildProfile::Debug,
            output_kind: OutputKind::Application,
            max_memory_pages: None,
            fuel: None,
        },
    )?;

    Ok(CompileArtifact {
        wasm: artifact.bytes,
        function_count,
        type_count,
    })
}

fn semantic_program(ast: &nexa_ast::SourceUnit) -> Result<SemanticProgram, CompileError> {
    let mut program = SemanticProgram::new();
    for item in &ast.items {
        match &item.kind {
            ItemKind::Function(f) => program.add_function(SemanticFunction {
                name: f.name.name.clone(),
                kind: NirCallableKind::Function,
                parameters: parameters(&f.params),
                return_type: type_name_opt(f.return_type.as_ref()),
                body: statements(&f.body)?,
            }),
            ItemKind::Action(a) => program.add_function(SemanticFunction {
                name: a.name.name.clone(),
                kind: if a.is_async {
                    NirCallableKind::AsyncAction
                } else {
                    NirCallableKind::Action
                },
                parameters: parameters(&a.params),
                return_type: type_name_opt(a.return_type.as_ref()),
                body: statements(&a.body)?,
            }),
            ItemKind::Struct(s) => program.type_decls.push(SemanticTypeDecl {
                name: s.name.name.clone(),
                kind: SemanticTypeKind::Struct {
                    fields: s
                        .fields
                        .iter()
                        .map(|f| (f.name.name.clone(), type_name(&f.ty)))
                        .collect(),
                },
            }),
            ItemKind::Enum(e) => program.type_decls.push(SemanticTypeDecl {
                name: e.name.name.clone(),
                kind: SemanticTypeKind::Enum {
                    variants: e
                        .variants
                        .iter()
                        .map(|v| {
                            let fields = match &v.kind {
                                EnumVariantKind::Unit => Vec::new(),
                                EnumVariantKind::Tuple(types) => {
                                    types.iter().map(type_name).collect()
                                }
                                EnumVariantKind::Struct(fields) => {
                                    fields.iter().map(|f| type_name(&f.ty)).collect()
                                }
                            };
                            (v.name.name.clone(), fields)
                        })
                        .collect(),
                },
            }),
            ItemKind::Interface(_)
            | ItemKind::Implement(_)
            | ItemKind::TypeAlias(_)
            | ItemKind::Const(_) => {}
        }
    }
    Ok(program)
}

fn parameters(params: &[nexa_ast::Param]) -> Vec<SemanticParameter> {
    params
        .iter()
        .map(|p| SemanticParameter {
            name: p.name.name.clone(),
            type_name: type_name(&p.ty),
            passing: NirPassingMode::Owned,
        })
        .collect()
}

fn statements(block: &Block) -> Result<Vec<SemanticStatement>, CompileError> {
    block
        .stmts
        .iter()
        .map(|stmt| match &stmt.kind {
            StmtKind::Let(binding) => Ok(SemanticStatement::Let {
                name: binding.name.name.clone(),
                type_name: binding.ty.as_ref().map(type_name),
                value: expression(&binding.init)?,
            }),
            StmtKind::Const(binding) => Ok(SemanticStatement::Let {
                name: binding.name.name.clone(),
                type_name: binding.ty.as_ref().map(type_name),
                value: expression(&binding.init)?,
            }),
            StmtKind::Var(binding) => binding
                .init
                .as_ref()
                .map(|init| {
                    Ok(SemanticStatement::Let {
                        name: binding.name.name.clone(),
                        type_name: binding.ty.as_ref().map(type_name),
                        value: expression(init)?,
                    })
                })
                .unwrap_or_else(|| {
                    Err(CompileError::Unsupported(format!(
                        "uninitialized variable `{}`",
                        binding.name.name
                    )))
                }),
            StmtKind::Assign(AssignTarget::Ident(name), AssignOp::Equal, value) => {
                Ok(SemanticStatement::Assign {
                    target: name.name.clone(),
                    value: expression(value)?,
                })
            }
            StmtKind::Assign(AssignTarget::Index(target, index), AssignOp::Equal, value) => {
                let ExprKind::Ident(name) = &target.kind else {
                    return Err(CompileError::Unsupported(
                        "array assignment target must be a local identifier".to_string(),
                    ));
                };
                Ok(SemanticStatement::ArrayAssign {
                    target: name.name.clone(),
                    index: expression(index)?,
                    value: expression(value)?,
                })
            }
            StmtKind::Expr(Expr {
                kind: ExprKind::If(if_expr),
                ..
            }) => if_statement(if_expr),
            StmtKind::Expr(Expr {
                kind: ExprKind::While(while_expr),
                ..
            }) => Ok(SemanticStatement::While {
                condition: expression(&while_expr.condition)?,
                body: statements(&while_expr.body)?,
            }),
            StmtKind::Expr(Expr {
                kind: ExprKind::Loop(loop_expr),
                ..
            }) => Ok(SemanticStatement::Loop {
                body: statements(&loop_expr.body)?,
            }),
            StmtKind::Expr(Expr {
                kind: ExprKind::For(for_expr),
                ..
            }) => {
                if for_expr.binding_mode != nexa_ast::ForBindingMode::Value {
                    return Err(CompileError::Unsupported(
                        "only value binding is executable for literal arrays".to_string(),
                    ));
                }
                Ok(SemanticStatement::ForLiteral {
                    variable: for_expr.variable.name.clone(),
                    iterable: expression(&for_expr.iterable)?,
                    body: statements(&for_expr.body)?,
                })
            }
            StmtKind::Expr(Expr {
                kind: ExprKind::Match(match_expr),
                ..
            }) => match_statement(match_expr),
            StmtKind::Expr(expr) | StmtKind::Discard(nexa_ast::DiscardStmt { expr, .. }) => {
                Ok(SemanticStatement::Expression(expression(expr)?))
            }
            StmtKind::Return(ret) => Ok(SemanticStatement::Return(
                ret.value.as_ref().map(expression).transpose()?,
            )),
            StmtKind::Break(brk) if brk.value.is_none() => Ok(SemanticStatement::Break),
            StmtKind::Continue(_) => Ok(SemanticStatement::Continue),
            other => Err(CompileError::Unsupported(format!("statement {other:?}"))),
        })
        .collect()
}

fn if_statement(if_expr: &nexa_ast::IfExpr) -> Result<SemanticStatement, CompileError> {
    let mut else_body = if_expr
        .else_block
        .as_ref()
        .map(statements)
        .transpose()?
        .unwrap_or_default();
    for else_if in if_expr.else_ifs.iter().rev() {
        else_body = vec![SemanticStatement::If {
            condition: expression(&else_if.condition)?,
            then_body: statements(&else_if.block)?,
            else_body,
        }];
    }
    Ok(SemanticStatement::If {
        condition: expression(&if_expr.condition)?,
        then_body: statements(&if_expr.then_block)?,
        else_body,
    })
}

fn match_statement(match_expr: &nexa_ast::MatchExpr) -> Result<SemanticStatement, CompileError> {
    let scrutinee = expression(&match_expr.scrutinee)?;
    let mut fallback = Vec::new();
    for arm in match_expr.arms.iter().rev() {
        if arm.guard.is_some() {
            return Err(CompileError::Unsupported(
                "match guards during executable lowering".to_string(),
            ));
        }
        let body = expression_body(&arm.body)?;
        match &arm.pattern.kind {
            nexa_ast::PatternKind::Wildcard => fallback = body,
            nexa_ast::PatternKind::Literal(literal) => {
                fallback = vec![SemanticStatement::If {
                    condition: SemanticExpression::BinaryOp {
                        op: "==".to_string(),
                        left: Box::new(scrutinee.clone()),
                        right: Box::new(expression(literal)?),
                    },
                    then_body: body,
                    else_body: fallback,
                }];
            }
            other => {
                return Err(CompileError::Unsupported(format!(
                    "match pattern {other:?} during executable lowering"
                )))
            }
        }
    }
    fallback.into_iter().next().ok_or_else(|| {
        CompileError::Unsupported("empty match expression during executable lowering".to_string())
    })
}

fn expression_body(expr: &Expr) -> Result<Vec<SemanticStatement>, CompileError> {
    match &expr.kind {
        ExprKind::Block(block) => statements(block),
        _ => Ok(vec![SemanticStatement::Expression(expression(expr)?)]),
    }
}

fn expression(expr: &Expr) -> Result<SemanticExpression, CompileError> {
    match &expr.kind {
        ExprKind::IntLiteral(v) => Ok(SemanticExpression::LiteralInt(*v as i128)),
        ExprKind::FloatLiteral(v) => Ok(SemanticExpression::LiteralFloat(*v)),
        ExprKind::BoolLiteral(v) => Ok(SemanticExpression::LiteralBool(*v)),
        ExprKind::StringLiteral(value) => {
            let mut text = String::new();
            for part in &value.parts {
                match part {
                    StringPart::Text(value) => text.push_str(value),
                    StringPart::Interpolation { .. } => {
                        return Err(CompileError::Unsupported(
                            "string interpolation during code generation".to_string(),
                        ));
                    }
                }
            }
            Ok(SemanticExpression::LiteralString(text))
        }
        ExprKind::Ident(name) => Ok(SemanticExpression::Identifier(name.name.clone())),
        ExprKind::Path(path) => Ok(SemanticExpression::Identifier(path_name(path))),
        ExprKind::Binary(op, left, right) => Ok(SemanticExpression::BinaryOp {
            op: binary_name(*op).to_string(),
            left: Box::new(expression(left)?),
            right: Box::new(expression(right)?),
        }),
        ExprKind::Unary(op, inner) => {
            let inner = expression(inner)?;
            match op {
                nexa_ast::UnaryOp::Plus => Ok(inner),
                nexa_ast::UnaryOp::Neg => Ok(SemanticExpression::BinaryOp {
                    op: "-".to_string(),
                    left: Box::new(SemanticExpression::LiteralInt(0)),
                    right: Box::new(inner),
                }),
                nexa_ast::UnaryOp::Not => Ok(SemanticExpression::BinaryOp {
                    op: "==".to_string(),
                    left: Box::new(inner),
                    right: Box::new(SemanticExpression::LiteralBool(false)),
                }),
                nexa_ast::UnaryOp::BitNot => Ok(SemanticExpression::BinaryOp {
                    op: "^".to_string(),
                    left: Box::new(inner),
                    right: Box::new(SemanticExpression::LiteralInt(-1)),
                }),
            }
        }
        ExprKind::Call { callee, args } => {
            let function = match &callee.kind {
                ExprKind::Ident(name) => name.name.clone(),
                ExprKind::Path(path) => path_name(path),
                _ => {
                    return Err(CompileError::Unsupported(
                        "indirect callable expression".to_string(),
                    ));
                }
            };
            Ok(SemanticExpression::Call {
                function,
                args: args.iter().map(expression).collect::<Result<_, _>>()?,
            })
        }
        ExprKind::Array(elements) => Ok(SemanticExpression::Array(
            elements.iter().map(expression).collect::<Result<_, _>>()?,
        )),
        ExprKind::Index { target, index } => Ok(SemanticExpression::Index {
            target: Box::new(expression(target)?),
            index: Box::new(expression(index)?),
        }),
        ExprKind::Paren(inner) => expression(inner),
        other => Err(CompileError::Unsupported(format!("expression {other:?}"))),
    }
}

fn type_name_opt(ty: Option<&Type>) -> String {
    ty.map(type_name).unwrap_or_else(|| "Unit".to_string())
}

fn type_name(ty: &Type) -> String {
    match &ty.kind {
        TypeKind::Path(path) => path_name(path),
        TypeKind::Unit => "Unit".to_string(),
        TypeKind::Generic { path, args } => format!(
            "{}[{}]",
            path_name(path),
            args.iter().map(type_name).collect::<Vec<_>>().join(",")
        ),
        TypeKind::Array(inner) => format!("Array[{}]", type_name(inner)),
        TypeKind::Optional(inner) => format!("Optional[{}]", type_name(inner)),
        TypeKind::Result { ok, err } => format!("Result[{},{}]", type_name(ok), type_name(err)),
        TypeKind::Ref(inner) => format!("&{}", type_name(inner)),
        TypeKind::RefMut(inner) => format!("&mut {}", type_name(inner)),
        TypeKind::Tuple(items) => format!(
            "({})",
            items.iter().map(type_name).collect::<Vec<_>>().join(",")
        ),
        TypeKind::Function { params, ret } => format!(
            "function({})->{}",
            params.iter().map(type_name).collect::<Vec<_>>().join(","),
            type_name(ret)
        ),
    }
}

fn path_name(path: &nexa_ast::QualifiedName) -> String {
    path.segments
        .iter()
        .map(|s| s.name.as_str())
        .collect::<Vec<_>>()
        .join("::")
}

fn binary_name(op: BinaryOp) -> &'static str {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_reaches_verified_wasm() {
        let source = "module main\n\naction main() -> Unit {\n    return\n}\n";
        let artifact = compile_source_to_wasm("hello.nexa", source).unwrap();
        assert!(artifact.wasm.starts_with(b"\0asm"));
        assert_eq!(artifact.function_count, 1);
    }

    #[test]
    fn application_requires_main() {
        let source = "module demo\n\nfunction value() -> Int {\n    return 1\n}\n";
        assert!(matches!(
            compile_source_to_wasm("demo.nexa", source),
            Err(CompileError::Unsupported(_))
        ));
    }

    #[test]
    fn scalar_arithmetic_reaches_structurally_valid_wasm() {
        let source = "module main\n\naction main() -> Unit {\n    let answer: Int = 40 + 2\n    answer + 1\n    return\n}\n";
        let artifact = compile_source_to_wasm("arithmetic.nexa", source).unwrap();
        assert!(artifact.wasm.starts_with(b"\0asm"));
        assert!(artifact.wasm.len() > 200);
    }
}
