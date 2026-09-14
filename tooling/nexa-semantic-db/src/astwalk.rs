use nexa_ast::*;

/// Collect every `Ident` node in the tree with its span (used for go-to-definition,
/// references and semantic tokens).
pub fn walk_idents(unit: &SourceUnit, f: &mut impl FnMut(&Ident)) {
    if let Some(m) = &unit.module {
        for seg in &m.name.segments {
            f(seg);
        }
    }
    for imp in &unit.imports {
        for seg in &imp.path.segments {
            f(seg);
        }
        if let Some(a) = &imp.alias {
            f(a);
        }
    }
    for item in &unit.items {
        walk_item_idents(item, f);
    }
}

fn walk_item_idents(item: &Item, f: &mut impl FnMut(&Ident)) {
    for a in &item.attrs {
        for seg in &a.name.segments {
            f(seg);
        }
        for e in &a.args {
            walk_expr_idents(e, f);
        }
    }
    match &item.kind {
        ItemKind::Function(d) => {
            f(&d.name);
            for g in &d.generic_params {
                f(&g.name);
                for b in &g.bounds {
                    for seg in &b.path.segments {
                        f(seg);
                    }
                }
            }
            for p in &d.params {
                f(&p.name);
                walk_type_idents(&p.ty, f);
            }
            if let Some(rt) = &d.return_type {
                walk_type_idents(rt, f);
            }
            for w in &d.where_clause {
                f(&w.type_name);
            }
            for c in &d.requires {
                walk_expr_idents(&c.expr, f);
            }
            for c in &d.ensures {
                walk_expr_idents(&c.expr, f);
            }
            walk_block_idents(&d.body, f);
        }
        ItemKind::Action(d) => {
            f(&d.name);
            for g in &d.generic_params {
                f(&g.name);
            }
            for p in &d.params {
                f(&p.name);
                walk_type_idents(&p.ty, f);
            }
            if let Some(rt) = &d.return_type {
                walk_type_idents(rt, f);
            }
            if let Some(ef) = &d.effects {
                for ep in &ef.effects {
                    for seg in &ep.path.segments {
                        f(seg);
                    }
                }
            }
            for c in &d.requires {
                walk_expr_idents(&c.expr, f);
            }
            for c in &d.ensures {
                walk_expr_idents(&c.expr, f);
            }
            walk_block_idents(&d.body, f);
        }
        ItemKind::Struct(d) => {
            f(&d.name);
            for g in &d.generic_params {
                f(&g.name);
            }
            for field in &d.fields {
                f(&field.name);
                walk_type_idents(&field.ty, f);
            }
        }
        ItemKind::Enum(d) => {
            f(&d.name);
            for g in &d.generic_params {
                f(&g.name);
            }
            for v in &d.variants {
                f(&v.name);
                match &v.kind {
                    EnumVariantKind::Unit => {}
                    EnumVariantKind::Tuple(ts) => {
                        for t in ts {
                            walk_type_idents(t, f);
                        }
                    }
                    EnumVariantKind::Struct(fields) => {
                        for field in fields {
                            f(&field.name);
                            walk_type_idents(&field.ty, f);
                        }
                    }
                }
            }
        }
        ItemKind::Interface(d) => {
            f(&d.name);
            for g in &d.generic_params {
                f(&g.name);
            }
            for m in &d.methods {
                f(&m.name);
                for p in &m.params {
                    f(&p.name);
                    walk_type_idents(&p.ty, f);
                }
                if let Some(rt) = &m.return_type {
                    walk_type_idents(rt, f);
                }
            }
        }
        ItemKind::Implement(d) => {
            walk_qualified_idents(&d.trait_path, f);
            walk_type_idents(&d.for_type, f);
            for m in &d.methods {
                f(&m.name);
                for p in &m.params {
                    f(&p.name);
                    walk_type_idents(&p.ty, f);
                }
                if let Some(rt) = &m.return_type {
                    walk_type_idents(rt, f);
                }
                if let Some(b) = &m.body {
                    walk_block_idents(b, f);
                }
            }
        }
        ItemKind::TypeAlias(d) => {
            f(&d.name);
            walk_type_idents(&d.ty, f);
        }
        ItemKind::Const(d) => {
            f(&d.name);
            if let Some(t) = &d.ty {
                walk_type_idents(t, f);
            }
            walk_expr_idents(&d.init, f);
        }
    }
}

fn walk_qualified_idents(q: &QualifiedName, f: &mut impl FnMut(&Ident)) {
    for seg in &q.segments {
        f(seg);
    }
}

fn walk_type_idents(ty: &Type, f: &mut impl FnMut(&Ident)) {
    match &ty.kind {
        TypeKind::Path(q) => walk_qualified_idents(q, f),
        TypeKind::Unit => {}
        TypeKind::Generic { path, args } => {
            walk_qualified_idents(path, f);
            for a in args {
                walk_type_idents(a, f);
            }
        }
        TypeKind::Array(inner) => walk_type_idents(inner, f),
        TypeKind::Optional(inner) => walk_type_idents(inner, f),
        TypeKind::Result { ok, err } => {
            walk_type_idents(ok, f);
            walk_type_idents(err, f);
        }
        TypeKind::Ref(inner) => walk_type_idents(inner, f),
        TypeKind::RefMut(inner) => walk_type_idents(inner, f),
        TypeKind::Tuple(items) => {
            for i in items {
                walk_type_idents(i, f);
            }
        }
        TypeKind::Function { params, ret } => {
            for p in params {
                walk_type_idents(p, f);
            }
            walk_type_idents(ret, f);
        }
    }
}

fn walk_block_idents(b: &Block, f: &mut impl FnMut(&Ident)) {
    for stmt in &b.stmts {
        walk_stmt_idents(stmt, f);
    }
}

fn walk_stmt_idents(stmt: &Stmt, f: &mut impl FnMut(&Ident)) {
    match &stmt.kind {
        StmtKind::Let(l) => {
            f(&l.name);
            if let Some(t) = &l.ty {
                walk_type_idents(t, f);
            }
            walk_expr_idents(&l.init, f);
        }
        StmtKind::Var(v) => {
            f(&v.name);
            if let Some(t) = &v.ty {
                walk_type_idents(t, f);
            }
            if let Some(i) = &v.init {
                walk_expr_idents(i, f);
            }
        }
        StmtKind::Const(c) => {
            f(&c.name);
            if let Some(t) = &c.ty {
                walk_type_idents(t, f);
            }
            walk_expr_idents(&c.init, f);
        }
        StmtKind::Assign(target, _op, expr) => {
            walk_assign_target_idents(target, f);
            walk_expr_idents(expr, f);
        }
        StmtKind::Expr(e) => walk_expr_idents(e, f),
        StmtKind::Return(r) => {
            if let Some(v) = &r.value {
                walk_expr_idents(v, f);
            }
        }
        StmtKind::Break(b) => {
            if let Some(v) = &b.value {
                walk_expr_idents(v, f);
            }
        }
        StmtKind::Continue(_) => {}
        StmtKind::Discard(d) => walk_expr_idents(&d.expr, f),
    }
}

fn walk_assign_target_idents(t: &AssignTarget, f: &mut impl FnMut(&Ident)) {
    match t {
        AssignTarget::Ident(id) => f(id),
        AssignTarget::Field(target, name) => {
            walk_expr_idents(target, f);
            f(name);
        }
        AssignTarget::Index(target, index) => {
            walk_expr_idents(target, f);
            walk_expr_idents(index, f);
        }
    }
}

fn walk_expr_idents(e: &Expr, f: &mut impl FnMut(&Ident)) {
    match &e.kind {
        ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Underscore
        | ExprKind::CharLiteral(_)
        | ExprKind::ByteStringLiteral(_) => {}
        ExprKind::StringLiteral(_) => {}
        ExprKind::Ident(id) => f(id),
        ExprKind::Path(q) => walk_qualified_idents(q, f),
        ExprKind::Binary(_op, a, b) => {
            walk_expr_idents(a, f);
            walk_expr_idents(b, f);
        }
        ExprKind::Unary(_, a) => walk_expr_idents(a, f),
        ExprKind::Call { callee, args } => {
            walk_expr_idents(callee, f);
            for a in args {
                walk_expr_idents(a, f);
            }
        }
        ExprKind::Index { target, index } => {
            walk_expr_idents(target, f);
            walk_expr_idents(index, f);
        }
        ExprKind::Field { target, name } => {
            walk_expr_idents(target, f);
            f(name);
        }
        ExprKind::MethodCall { target, name, args } => {
            walk_expr_idents(target, f);
            f(name);
            for a in args {
                walk_expr_idents(a, f);
            }
        }
        ExprKind::Await(a)
        | ExprKind::Try(a)
        | ExprKind::Ref(a)
        | ExprKind::RefMut(a)
        | ExprKind::Move(a) => walk_expr_idents(a, f),
        ExprKind::If(ifex) => {
            walk_expr_idents(&ifex.condition, f);
            walk_block_idents(&ifex.then_block, f);
            for ei in &ifex.else_ifs {
                walk_expr_idents(&ei.condition, f);
                walk_block_idents(&ei.block, f);
            }
            if let Some(eb) = &ifex.else_block {
                walk_block_idents(eb, f);
            }
        }
        ExprKind::Match(m) => {
            walk_expr_idents(&m.scrutinee, f);
            for arm in &m.arms {
                walk_pattern_idents(&arm.pattern, f);
                if let Some(g) = &arm.guard {
                    walk_expr_idents(g, f);
                }
                walk_expr_idents(&arm.body, f);
            }
        }
        ExprKind::For(fe) => {
            f(&fe.variable);
            walk_expr_idents(&fe.iterable, f);
            walk_block_idents(&fe.body, f);
        }
        ExprKind::While(we) => {
            walk_expr_idents(&we.condition, f);
            walk_block_idents(&we.body, f);
        }
        ExprKind::Loop(le) => walk_block_idents(&le.body, f),
        ExprKind::Block(b) | ExprKind::Unsafe(b) => walk_block_idents(b, f),
        ExprKind::Tuple(items) => {
            for i in items {
                walk_expr_idents(i, f);
            }
        }
        ExprKind::StructConstruct(sc) => {
            for fp in &sc.fields {
                walk_expr_idents(&fp.expr, f);
            }
        }
        ExprKind::Array(items) => {
            for i in items {
                walk_expr_idents(i, f);
            }
        }
        ExprKind::Paren(inner) => walk_expr_idents(inner, f),
        ExprKind::AssignExpr(target, _op, expr) => {
            walk_assign_target_idents(target, f);
            walk_expr_idents(expr, f);
        }
    }
}

fn walk_pattern_idents(p: &Pattern, f: &mut impl FnMut(&Ident)) {
    match &p.kind {
        PatternKind::Literal(e) => walk_expr_idents(e, f),
        PatternKind::Ident(id) => f(id),
        PatternKind::Wildcard | PatternKind::Rest => {}
        PatternKind::Tuple(ps) | PatternKind::Or(ps) => {
            for pp in ps {
                walk_pattern_idents(pp, f);
            }
        }
        PatternKind::Struct { path, fields } => {
            walk_qualified_idents(path, f);
            for fp in fields {
                f(&fp.name);
                if let Some(pp) = &fp.pattern {
                    walk_pattern_idents(pp, f);
                }
            }
        }
        PatternKind::Enum {
            path,
            variant,
            pattern,
        } => {
            walk_qualified_idents(path, f);
            f(variant);
            if let Some(pp) = pattern {
                walk_pattern_idents(pp, f);
            }
        }
        PatternKind::Rename { pattern, alias } => {
            walk_pattern_idents(pattern, f);
            f(alias);
        }
        PatternKind::Guard { pattern, condition } => {
            walk_pattern_idents(pattern, f);
            walk_expr_idents(condition, f);
        }
    }
}

/// Find the `Ident` whose span contains `offset`, preferring the deepest (smallest) span.
pub fn ident_at(unit: &SourceUnit, offset: u32) -> Option<Ident> {
    let mut best: Option<Ident> = None;
    let mut best_len = u32::MAX;
    walk_idents(unit, &mut |id| {
        if id.span.start <= offset && offset < id.span.end {
            let len = id.span.end - id.span.start;
            if len < best_len {
                best_len = len;
                best = Some(id.clone());
            }
        }
    });
    best
}
