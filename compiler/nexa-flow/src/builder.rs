//! Construtor de CFG a partir do AST tipado (Implementação 05 §311-408).
//!
//! Traduz o corpo (bloco de statements) de um callable num `ControlFlowGraph`
//! (NEXA-FLOW-0001/0002 baseline). O `FlowBuilder` também modela leituras e
//! escritas de locals (para definite assignment, FLOW-0005/0006).
//!
//! Notas da fatia atual:
//! - `if`/`match`/`while`/`loop` como *statement expressions* geram branch
//!   real no CFG (then/else, arms, backedges).
//! - `if`/`match` como sub-expressão (`let x = if c {...}`) ainda é tratado
//!   de forma linear/conservadora (limitação da fatia).

use crate::cfg::{
    BasicBlock, BasicBlockId, CfgId, ControlFlowGraph, ExprRef, FlowOperation, FlowTerminator,
};
use crate::diagnostics::{FlowDiagnostic, FlowDiagnosticCode};
use nexa_ast::{AssignTarget, Block, ElseIf, Expr, ExprKind, IfExpr, Stmt, StmtKind};
use nexa_source::SourceSpan;
use nexa_symbols::SymbolId;
use nexa_types::id::TypeId;

/// Identificador de um laço no stack do builder.
struct LoopCtx {
    /// Bloco de condição (target do `continue` / backedge).
    header: BasicBlockId,
    /// Bloco após o laço (target do `break`).
    after: BasicBlockId,
}

/// Resultado da construção de um CFG para um callable.
#[derive(Debug)]
pub struct BuiltFlow {
    pub cfg: ControlFlowGraph,
    /// Symbols de locals (params + bindings) encontrados no corpo.
    pub locals: Vec<SymbolId>,
    /// Diagnósticos estruturais emitidos durante a construção
    /// (`InvalidBreak`/`InvalidContinue`, FLOW-0003/0004).
    pub build_diagnostics: Vec<FlowDiagnostic>,
}

/// Cria o CFG de um corpo de callable.
///
/// * `params` — symbols dos parâmetros (e `self`) já inicializados na entrada.
/// * `resolve_span` — resolução span→symbol (leia o `SemanticIndex`).
/// * `expr_type` — tipo (TypeId) de cada expressão pelo span.
/// * `never_ty` — `TypeId` de `Never` (usado para detectar terminação).
pub fn build_callable_cfg(
    cfg_id: CfgId,
    callable: SymbolId,
    body: &Block,
    params: &[SymbolId],
    resolve_span: &dyn Fn(SourceSpan) -> Option<SymbolId>,
    expr_type: &dyn Fn(SourceSpan) -> Option<TypeId>,
    never_ty: TypeId,
) -> BuiltFlow {
    let mut b = FlowBuilder::new(cfg_id, callable, resolve_span, expr_type, never_ty);
    for &p in params {
        if !b.locals.contains(&p) {
            b.locals.push(p);
        }
    }
    let entry = b.fresh_block(Some(body.span));
    b.cfg.entry = entry;
    // Parâmetros já inicializados na entrada.
    for &p in params {
        b.op(entry, FlowOperation::WriteLocal(p));
    }
    let ends = b.build_block(body, entry);
    for end in ends {
        b.set_terminator(end, FlowTerminator::Fallthrough);
    }
    BuiltFlow {
        cfg: b.cfg,
        locals: b.locals,
        build_diagnostics: b.build_diagnostics,
    }
}

struct FlowBuilder<'c> {
    cfg: ControlFlowGraph,
    next_block: u32,
    locals: Vec<SymbolId>,
    loop_stack: Vec<LoopCtx>,
    resolve_span: &'c dyn Fn(SourceSpan) -> Option<SymbolId>,
    expr_type: &'c dyn Fn(SourceSpan) -> Option<TypeId>,
    never_ty: TypeId,
    build_diagnostics: Vec<FlowDiagnostic>,
}

impl<'c> FlowBuilder<'c> {
    fn new(
        cfg_id: CfgId,
        callable: SymbolId,
        resolve_span: &'c dyn Fn(SourceSpan) -> Option<SymbolId>,
        expr_type: &'c dyn Fn(SourceSpan) -> Option<TypeId>,
        never_ty: TypeId,
    ) -> Self {
        Self {
            cfg: ControlFlowGraph::new(cfg_id, callable, BasicBlockId(0)),
            next_block: 0,
            locals: Vec::new(),
            loop_stack: Vec::new(),
            resolve_span,
            expr_type,
            never_ty,
            build_diagnostics: Vec::new(),
        }
    }

    fn fresh_block(&mut self, span: Option<SourceSpan>) -> BasicBlockId {
        let id = BasicBlockId(self.next_block);
        self.next_block += 1;
        self.cfg.add_block(BasicBlock::new(id, span));
        id
    }

    fn op(&mut self, block: BasicBlockId, op: FlowOperation) {
        if let Some(bb) = self.cfg.get_block_mut(block) {
            bb.operations.push(op);
        }
    }

    fn set_terminator(&mut self, block: BasicBlockId, term: FlowTerminator) {
        if let Some(bb) = self.cfg.get_block_mut(block) {
            bb.terminator = term;
        }
    }

    fn is_never(&self, expr: &Expr) -> bool {
        (self.expr_type)(expr.span) == Some(self.never_ty)
    }

    fn sym_at(&self, span: SourceSpan) -> Option<SymbolId> {
        (self.resolve_span)(span)
    }

    fn is_local(&self, sym: SymbolId) -> bool {
        self.locals.contains(&sym)
    }

    /// Constrói o CFG de um bloco a partir do bloco inicial `entry`;
    /// devolve os blocos "abertos" (que caem para o próximo statement).
    fn build_block(&mut self, block: &Block, entry: BasicBlockId) -> Vec<BasicBlockId> {
        let mut frontier = vec![entry];
        for stmt in &block.stmts {
            if frontier.is_empty() {
                // Fluxo morto: o próximo statement vive num bloco inalcançável
                // (detectado por `analyze_reachability`/`UnreachableCode`).
                frontier.push(self.fresh_block(Some(stmt.span)));
            }
            frontier = self.stmt(frontier, stmt);
        }
        frontier
    }

    fn stmt(&mut self, frontier: Vec<BasicBlockId>, stmt: &Stmt) -> Vec<BasicBlockId> {
        match &stmt.kind {
            StmtKind::Let(binding) => {
                self.bind_with_init(frontier, binding.name.span, &binding.init)
            }
            StmtKind::Const(binding) => {
                self.bind_with_init(frontier, binding.name.span, &binding.init)
            }
            StmtKind::Var(binding) => {
                if let Some(init) = &binding.init {
                    self.bind_with_init(frontier, binding.name.span, init)
                } else {
                    if let Some(sym) = self.sym_at(binding.name.span) {
                        if !self.locals.contains(&sym) {
                            self.locals.push(sym);
                        }
                    }
                    // `var x: T` sem init: declara o local sem escritas; fica
                    // Uninit até o primeiro assignment (FLOW-0005/0006).
                    frontier
                }
            }
            StmtKind::Expr(expr) => self.stmt_expr(frontier, expr, stmt.span),
            StmtKind::Return(ret) => {
                if let Some(value) = &ret.value {
                    self.attach_reads(frontier.clone(), value);
                }
                for b in frontier {
                    self.set_terminator(b, FlowTerminator::Return(None));
                }
                Vec::new()
            }
            StmtKind::Break(brk) => {
                if let Some(value) = &brk.value {
                    self.attach_reads(frontier.clone(), value);
                }
                match self.loop_stack.last().map(|c| (c.after, c.header)) {
                    Some((after, _header)) => {
                        for b in frontier {
                            self.set_terminator(b, FlowTerminator::Break { target: after });
                        }
                    }
                    None => {
                        self.build_diagnostics.push(
                            FlowDiagnostic::new(
                                FlowDiagnosticCode::InvalidBreak,
                                "break outside of a loop (while, loop or for)",
                            )
                            .with_span(stmt.span),
                        );
                        for b in frontier {
                            self.set_terminator(b, FlowTerminator::Unreachable);
                        }
                    }
                }
                Vec::new()
            }
            StmtKind::Continue(_) => {
                match self.loop_stack.last().map(|c| (c.after, c.header)) {
                    Some((_after, header)) => {
                        for b in frontier {
                            self.set_terminator(b, FlowTerminator::Continue { target: header });
                        }
                    }
                    None => {
                        self.build_diagnostics.push(
                            FlowDiagnostic::new(
                                FlowDiagnosticCode::InvalidContinue,
                                "continue outside of a loop (while, loop or for)",
                            )
                            .with_span(stmt.span),
                        );
                        for b in frontier {
                            self.set_terminator(b, FlowTerminator::Unreachable);
                        }
                    }
                }
                Vec::new()
            }
            StmtKind::Assign(target, _op, value) => {
                self.attach_reads(frontier.clone(), value);
                if let AssignTarget::Ident(ident) = target {
                    if let Some(sym) = self.sym_at(ident.span) {
                        if self.is_local(sym) {
                            for b in &frontier {
                                self.op(*b, FlowOperation::WriteLocal(sym));
                            }
                        }
                    }
                }
                frontier
            }
            StmtKind::Discard(d) => {
                self.attach_reads(frontier.clone(), &d.expr);
                for b in &frontier {
                    self.op(*b, FlowOperation::ExpressionUse(ExprRef(0)));
                }
                if self.is_never(&d.expr) {
                    Vec::new()
                } else {
                    frontier
                }
            }
        }
    }

    fn bind_with_init(
        &mut self,
        frontier: Vec<BasicBlockId>,
        name_span: SourceSpan,
        init: &Expr,
    ) -> Vec<BasicBlockId> {
        self.attach_reads(frontier.clone(), init);
        if let Some(sym) = self.sym_at(name_span) {
            if !self.locals.contains(&sym) {
                self.locals.push(sym);
            }
            for b in &frontier {
                self.op(*b, FlowOperation::WriteLocal(sym));
            }
        }
        if self.is_never(init) {
            Vec::new()
        } else {
            frontier
        }
    }

    /// Emite as leituras de locals de `expr` em todos os blocos da frontier.
    fn attach_reads(&mut self, frontier: Vec<BasicBlockId>, expr: &Expr) {
        let reads = collect_local_reads(expr, &self.resolve_span, &self.locals);
        for b in &frontier {
            for (sym, span) in &reads {
                self.op(
                    *b,
                    FlowOperation::ReadLocal {
                        symbol: *sym,
                        span: *span,
                    },
                );
            }
        }
    }

    /// Trata um statement `Expr` — que pode ser uma expressão de controle
    /// (`if`, `match`, `while`, `loop`, `for`, bloco) ou uma expressão simples.
    fn stmt_expr(
        &mut self,
        frontier: Vec<BasicBlockId>,
        expr: &Expr,
        stmt_span: SourceSpan,
    ) -> Vec<BasicBlockId> {
        match &expr.kind {
            ExprKind::If(_) => self.build_if(frontier, expr, stmt_span),
            ExprKind::Match(_) => self.build_match(frontier, expr),
            ExprKind::While(_) | ExprKind::Loop(_) | ExprKind::For(_) => {
                self.build_loop(frontier, expr)
            }
            ExprKind::Block(block) => {
                let entry = self.fresh_block(Some(stmt_span));
                for b in frontier {
                    self.set_terminator(b, FlowTerminator::Goto(entry));
                }
                self.build_block(block, entry)
            }
            ExprKind::Paren(inner) => self.stmt_expr(frontier, inner, stmt_span),
            _ => {
                self.attach_reads(frontier.clone(), expr);
                for b in &frontier {
                    self.op(*b, FlowOperation::ExpressionUse(ExprRef(0)));
                }
                if self.is_never(expr) {
                    Vec::new()
                } else {
                    frontier
                }
            }
        }
    }

    fn build_if(
        &mut self,
        frontier: Vec<BasicBlockId>,
        expr: &Expr,
        _stmt_span: SourceSpan,
    ) -> Vec<BasicBlockId> {
        let ExprKind::If(ifx) = &expr.kind else {
            return frontier;
        };
        let then_id = self.fresh_block(None);
        let else_id = self.fresh_block(None);
        self.attach_reads(frontier.clone(), &ifx.condition);
        for b in frontier {
            self.op(b, FlowOperation::ExpressionUse(ExprRef(0)));
            self.set_terminator(
                b,
                FlowTerminator::Branch {
                    condition: ExprRef(0),
                    then_block: then_id,
                    else_block: else_id,
                },
            );
        }
        let then_ends = self.build_block(&ifx.then_block, then_id);
        let mut else_ends = self.build_else(&ifx.else_ifs, ifx.else_block.as_ref(), else_id);
        let mut combined = then_ends;
        combined.append(&mut else_ends);
        if combined.is_empty() {
            // Todos os ramos terminam (return/break/continue/never).
            return Vec::new();
        }
        let merge = self.fresh_block(Some(expr.span));
        for e in combined {
            self.set_terminator(e, FlowTerminator::Goto(merge));
        }
        vec![merge]
    }

    /// Constrói o fluxo de `else if` encadeados + bloco `else` final;
    /// devolve os ends abertos dos ramos.
    fn build_else(
        &mut self,
        else_ifs: &[ElseIf],
        else_block: Option<&Block>,
        entry: BasicBlockId,
    ) -> Vec<BasicBlockId> {
        if !else_ifs.is_empty() {
            let mut ends = Vec::new();
            let mut cur = entry;
            for (i, ei) in else_ifs.iter().enumerate() {
                let then_id = self.fresh_block(None);
                self.attach_reads(vec![cur], &ei.condition);
                self.op(cur, FlowOperation::ExpressionUse(ExprRef(0)));
                let next_else = if i + 1 < else_ifs.len() || else_block.is_some() {
                    self.fresh_block(None)
                } else {
                    cur
                };
                self.set_terminator(
                    cur,
                    FlowTerminator::Branch {
                        condition: ExprRef(0),
                        then_block: then_id,
                        else_block: next_else,
                    },
                );
                ends.extend(self.build_block(&ei.block, then_id));
                cur = next_else;
            }
            match else_block {
                Some(b) => ends.extend(self.build_block(b, cur)),
                None => ends.push(cur),
            }
            ends
        } else {
            match else_block {
                Some(b) => self.build_block(b, entry),
                None => vec![entry],
            }
        }
    }

    fn build_match(&mut self, frontier: Vec<BasicBlockId>, expr: &Expr) -> Vec<BasicBlockId> {
        let ExprKind::Match(mx) = &expr.kind else {
            return frontier;
        };
        let arm_ids: Vec<BasicBlockId> = mx
            .arms
            .iter()
            .map(|arm| self.fresh_block(Some(arm.span)))
            .collect();
        self.attach_reads(frontier.clone(), &mx.scrutinee);
        for b in frontier {
            self.op(b, FlowOperation::ExpressionUse(ExprRef(0)));
            self.set_terminator(
                b,
                FlowTerminator::Match {
                    scrutinee: ExprRef(0),
                    arms: arm_ids.clone(),
                },
            );
        }
        let merge = self.fresh_block(Some(expr.span));
        let mut any_completes = false;
        for (i, arm) in mx.arms.iter().enumerate() {
            let entry = arm_ids[i];
            self.attach_reads(vec![entry], &arm.body);
            self.op(entry, FlowOperation::ExpressionUse(ExprRef(0)));
            if self.is_never(&arm.body) {
                self.set_terminator(entry, FlowTerminator::Unreachable);
            } else {
                any_completes = true;
                self.set_terminator(entry, FlowTerminator::Goto(merge));
            }
        }
        if mx.arms.is_empty() {
            return vec![merge];
        }
        if any_completes {
            vec![merge]
        } else {
            Vec::new()
        }
    }

    fn build_loop(&mut self, frontier: Vec<BasicBlockId>, expr: &Expr) -> Vec<BasicBlockId> {
        match &expr.kind {
            ExprKind::While(w) => {
                let header = self.fresh_block(None);
                let body_id = self.fresh_block(None);
                let after = self.fresh_block(Some(expr.span));
                for b in frontier {
                    self.set_terminator(b, FlowTerminator::Goto(header));
                }
                self.attach_reads(vec![header], &w.condition);
                self.op(header, FlowOperation::ExpressionUse(ExprRef(0)));
                self.set_terminator(
                    header,
                    FlowTerminator::Branch {
                        condition: ExprRef(0),
                        then_block: body_id,
                        else_block: after,
                    },
                );
                self.loop_stack.push(LoopCtx { header, after });
                let ends = self.build_block(&w.body, body_id);
                self.loop_stack.pop();
                for e in ends {
                    self.set_terminator(e, FlowTerminator::Goto(header));
                }
                vec![after]
            }
            ExprKind::Loop(l) => {
                let body_id = self.fresh_block(None);
                let after = self.fresh_block(Some(expr.span));
                for b in frontier {
                    self.set_terminator(b, FlowTerminator::Goto(body_id));
                }
                self.loop_stack.push(LoopCtx {
                    header: body_id,
                    after,
                });
                let ends = self.build_block(&l.body, body_id);
                self.loop_stack.pop();
                for e in ends {
                    self.set_terminator(e, FlowTerminator::Goto(body_id));
                }
                vec![after]
            }
            ExprKind::For(f) => {
                // Fatia atual: tratado como loop com condição verdadeira única
                // (backedge); leituras da variável do laço não são modeladas.
                let body_id = self.fresh_block(None);
                let after = self.fresh_block(Some(expr.span));
                for b in frontier {
                    self.set_terminator(b, FlowTerminator::Goto(body_id));
                }
                let _ = &f.iterable;
                self.loop_stack.push(LoopCtx {
                    header: body_id,
                    after,
                });
                let ends = self.build_block(&f.body, body_id);
                self.loop_stack.pop();
                for e in ends {
                    self.set_terminator(e, FlowTerminator::Goto(body_id));
                }
                vec![after]
            }
            _ => frontier,
        }
    }
}

/// Coleta os (symbol, span) de leituras de locals em `expr`.
fn collect_local_reads(
    expr: &Expr,
    resolve_span: &dyn Fn(SourceSpan) -> Option<SymbolId>,
    locals: &[SymbolId],
) -> Vec<(SymbolId, SourceSpan)> {
    fn walk(
        expr: &Expr,
        resolve_span: &dyn Fn(SourceSpan) -> Option<SymbolId>,
        locals: &[SymbolId],
        out: &mut Vec<(SymbolId, SourceSpan)>,
    ) {
        match &expr.kind {
            ExprKind::Ident(_) => {
                if let Some(sym) = resolve_span(expr.span) {
                    if locals.contains(&sym) {
                        out.push((sym, expr.span));
                    }
                }
            }
            ExprKind::Binary(_, l, r) => {
                walk(l, resolve_span, locals, out);
                walk(r, resolve_span, locals, out);
            }
            ExprKind::Unary(_, e) => walk(e, resolve_span, locals, out),
            ExprKind::Call { callee, args } => {
                walk(callee, resolve_span, locals, out);
                for a in args {
                    walk(a, resolve_span, locals, out);
                }
            }
            ExprKind::MethodCall { target, args, .. } => {
                walk(target, resolve_span, locals, out);
                for a in args {
                    walk(a, resolve_span, locals, out);
                }
            }
            ExprKind::Index { target, index } => {
                walk(target, resolve_span, locals, out);
                walk(index, resolve_span, locals, out);
            }
            ExprKind::Field { target, .. } => walk(target, resolve_span, locals, out),
            ExprKind::Await(e)
            | ExprKind::Try(e)
            | ExprKind::Ref(e)
            | ExprKind::RefMut(e)
            | ExprKind::Move(e) => walk(e, resolve_span, locals, out),
            ExprKind::Paren(e) => walk(e, resolve_span, locals, out),
            ExprKind::Tuple(es) | ExprKind::Array(es) => {
                for e in es {
                    walk(e, resolve_span, locals, out);
                }
            }
            ExprKind::StructConstruct(sc) => {
                for f in &sc.fields {
                    walk(&f.expr, resolve_span, locals, out);
                }
            }
            ExprKind::AssignExpr(_, _, value) => walk(value, resolve_span, locals, out),
            ExprKind::Block(b) => {
                for s in &b.stmts {
                    walk_stmt(s, resolve_span, locals, out);
                }
            }
            ExprKind::If(ifx) => walk_if(ifx, resolve_span, locals, out),
            ExprKind::Match(mx) => {
                walk(&mx.scrutinee, resolve_span, locals, out);
                for arm in &mx.arms {
                    walk(&arm.body, resolve_span, locals, out);
                }
            }
            ExprKind::While(w) => {
                walk(&w.condition, resolve_span, locals, out);
                for s in &w.body.stmts {
                    walk_stmt(s, resolve_span, locals, out);
                }
            }
            ExprKind::Loop(l) => {
                for s in &l.body.stmts {
                    walk_stmt(s, resolve_span, locals, out);
                }
            }
            ExprKind::For(f) => {
                walk(&f.iterable, resolve_span, locals, out);
                for s in &f.body.stmts {
                    walk_stmt(s, resolve_span, locals, out);
                }
            }
            _ => {}
        }
    }

    fn walk_stmt(
        stmt: &Stmt,
        resolve_span: &dyn Fn(SourceSpan) -> Option<SymbolId>,
        locals: &[SymbolId],
        out: &mut Vec<(SymbolId, SourceSpan)>,
    ) {
        match &stmt.kind {
            StmtKind::Let(b) => walk(&b.init, resolve_span, locals, out),
            StmtKind::Const(b) => walk(&b.init, resolve_span, locals, out),
            StmtKind::Var(b) => {
                if let Some(i) = &b.init {
                    walk(i, resolve_span, locals, out);
                }
            }
            StmtKind::Expr(e) => walk(e, resolve_span, locals, out),
            StmtKind::Return(r) => {
                if let Some(v) = &r.value {
                    walk(v, resolve_span, locals, out);
                }
            }
            StmtKind::Assign(_, _, v) => walk(v, resolve_span, locals, out),
            StmtKind::Discard(d) => walk(&d.expr, resolve_span, locals, out),
            StmtKind::Break(b) => {
                if let Some(v) = &b.value {
                    walk(v, resolve_span, locals, out);
                }
            }
            StmtKind::Continue(_) => {}
        }
    }

    fn walk_if(
        ifx: &IfExpr,
        resolve_span: &dyn Fn(SourceSpan) -> Option<SymbolId>,
        locals: &[SymbolId],
        out: &mut Vec<(SymbolId, SourceSpan)>,
    ) {
        walk(&ifx.condition, resolve_span, locals, out);
        for s in &ifx.then_block.stmts {
            walk_stmt(s, resolve_span, locals, out);
        }
        for ei in &ifx.else_ifs {
            walk(&ei.condition, resolve_span, locals, out);
            for s in &ei.block.stmts {
                walk_stmt(s, resolve_span, locals, out);
            }
        }
        if let Some(else_b) = &ifx.else_block {
            for s in &else_b.stmts {
                walk_stmt(s, resolve_span, locals, out);
            }
        }
    }

    let mut out = Vec::new();
    walk(expr, resolve_span, locals, &mut out);
    out
}
