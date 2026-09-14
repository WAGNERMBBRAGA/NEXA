use crate::cfg::{BasicBlockId, ControlFlowGraph, FlowTerminator};
use crate::diagnostics::{FlowDiagnostic, FlowDiagnosticCode};
use nexa_source::SourceSpan;
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, Clone)]
pub struct ReachabilityInfo {
    pub reachable_blocks: HashMap<BasicBlockId, bool>,
    pub all_paths_return: bool,
    pub can_complete_normally: bool,
}

/// Um bloco "completa normalmente" se existe um caminho a partir dele que cai
/// fora do callable sem `return` explícito (Fallthrough).
fn block_completes(
    cfg: &ControlFlowGraph,
    block_id: BasicBlockId,
    entering: &mut HashSet<BasicBlockId>,
) -> bool {
    if !entering.insert(block_id) {
        // Ciclo: o caminho nunca sai; tratar como não-completável aqui.
        return false;
    }
    let result = match cfg.get_block(block_id) {
        Some(block) => match &block.terminator {
            FlowTerminator::Return(_) | FlowTerminator::Unreachable | FlowTerminator::Trap => false,
            FlowTerminator::Fallthrough => true,
            FlowTerminator::Goto(target) => block_completes(cfg, *target, entering),
            FlowTerminator::Break { target } => block_completes(cfg, *target, entering),
            FlowTerminator::Continue { target } => block_completes(cfg, *target, entering),
            FlowTerminator::Branch {
                then_block,
                else_block,
                ..
            } => {
                block_completes(cfg, *then_block, entering)
                    || block_completes(cfg, *else_block, entering)
            }
            FlowTerminator::Match { arms, .. } => {
                arms.iter().any(|arm| block_completes(cfg, *arm, entering))
            }
        },
        None => true,
    };
    entering.remove(&block_id);
    result
}

pub fn analyze_reachability(cfg: &ControlFlowGraph) -> ReachabilityInfo {
    let mut reachable_blocks: HashMap<BasicBlockId, bool> = HashMap::new();
    let mut worklist: VecDeque<BasicBlockId> = VecDeque::new();

    reachable_blocks.insert(cfg.entry, true);
    worklist.push_back(cfg.entry);

    while let Some(block_id) = worklist.pop_front() {
        for successor in cfg.successors(block_id) {
            if let std::collections::hash_map::Entry::Vacant(e) = reachable_blocks.entry(successor)
            {
                e.insert(true);
                worklist.push_back(successor);
            }
        }
    }

    let mut can_complete_normally = false;
    for block in &cfg.blocks {
        if !reachable_blocks.get(&block.id).copied().unwrap_or(false) {
            continue;
        }
        let mut entering = HashSet::new();
        if block_completes(cfg, block.id, &mut entering) {
            can_complete_normally = true;
            break;
        }
    }

    ReachabilityInfo {
        reachable_blocks,
        all_paths_return: !can_complete_normally,
        can_complete_normally,
    }
}

pub fn detect_unreachable_blocks(cfg: &ControlFlowGraph) -> Vec<BasicBlockId> {
    let mut reachable: HashMap<BasicBlockId, bool> = HashMap::new();
    let mut worklist: VecDeque<BasicBlockId> = VecDeque::new();

    reachable.insert(cfg.entry, true);
    worklist.push_back(cfg.entry);

    while let Some(block_id) = worklist.pop_front() {
        for successor in cfg.successors(block_id) {
            if let std::collections::hash_map::Entry::Vacant(e) = reachable.entry(successor) {
                e.insert(true);
                worklist.push_back(successor);
            }
        }
    }

    cfg.blocks
        .iter()
        .filter(|b| !reachable.get(&b.id).copied().unwrap_or(false))
        .map(|b| b.id)
        .collect()
}

pub fn check_missing_return(
    cfg: &ControlFlowGraph,
    info: &ReachabilityInfo,
    callable_has_return_type: bool,
    span: Option<SourceSpan>,
) -> Vec<FlowDiagnostic> {
    let mut diagnostics = Vec::new();

    if callable_has_return_type && info.can_complete_normally {
        let body = cfg.get_block(cfg.entry);
        let diag_span = span.or(body.and_then(|b| b.span));
        let mut d = FlowDiagnostic::new(
            FlowDiagnosticCode::MissingReturn,
            "a reachable path reaches the end of this callable without returning",
        );
        if let Some(s) = diag_span {
            d = d.with_span(s);
        }
        diagnostics.push(d);
    }

    diagnostics
}

/// Diagnósticos de código inalcançável (`NEXA-FLOW-0002`, warning §42).
pub fn unreachable_code_diagnostics(cfg: &ControlFlowGraph) -> Vec<FlowDiagnostic> {
    let mut diagnostics = Vec::new();
    for id in detect_unreachable_blocks(cfg) {
        if let Some(block) = cfg.get_block(id) {
            if let Some(span) = block.span {
                diagnostics.push(
                    FlowDiagnostic::new(
                        FlowDiagnosticCode::UnreachableCode,
                        "this statement is unreachable",
                    )
                    .with_span(span),
                );
            }
        }
    }
    diagnostics
}
