//! Definite assignment (Implementação 05, FLOW-0005/0006).
//!
//! Análise forward (worklist fixpoint/determinística) sobre o CFG que rastreia
//! para cada local um estado em três valores:
//!
//! ```text
//! Uninit ──(algum caminho sem escrita)──► MaybeUninit ──► Init
//! ```
//!
//! - `NEXA-FLOW-0005 UseBeforeInitialization`: leitura com estado `Uninit`
//!   (nenhum caminho escreveu antes).
//! - `NEXA-FLOW-0006 PossiblyUninitialized`: leitura com estado `MaybeUninit`
//!   (há caminho que não escreve antes).

use crate::cfg::{BasicBlockId, ControlFlowGraph, FlowOperation};
use crate::diagnostics::{FlowDiagnostic, FlowDiagnosticCode};
use nexa_symbols::SymbolId;
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AssignState {
    Uninit,
    MaybeUninit,
    Init,
}

impl AssignState {
    /// Junção (merge) de dois estados de entrada de caminhos distintos.
    fn join(self, other: AssignState) -> AssignState {
        if self == other {
            self
        } else {
            AssignState::MaybeUninit
        }
    }
}

/// Roda a análise e devolve os diagnósticos de leitura antes de inicialiação.
pub fn check_definite_assignment_diagnostics(
    cfg: &ControlFlowGraph,
    locals: &[SymbolId],
) -> Vec<FlowDiagnostic> {
    // `states[id]` é a ENTRADA do bloco (join das saídas dos predecessores).
    let mut states: HashMap<BasicBlockId, HashMap<SymbolId, AssignState>> = HashMap::new();
    let mut emitted: HashSet<(BasicBlockId, SymbolId, &'static str)> = HashSet::new();
    let mut worklist: VecDeque<BasicBlockId> = VecDeque::new();

    let init_state: HashMap<SymbolId, AssignState> =
        locals.iter().map(|s| (*s, AssignState::Uninit)).collect();

    states.insert(cfg.entry, init_state);
    worklist.push_back(cfg.entry);

    let mut diagnostics = Vec::new();

    while let Some(block_id) = worklist.pop_front() {
        let state = states.get(&block_id).cloned().unwrap();
        let mut out = state.clone();

        if let Some(block) = cfg.get_block(block_id) {
            for op in &block.operations {
                match op {
                    FlowOperation::WriteLocal(sym) => {
                        out.insert(*sym, AssignState::Init);
                    }
                    FlowOperation::Assignment { target, .. } => {
                        out.insert(*target, AssignState::Init);
                    }
                    FlowOperation::ReadLocal { symbol, span } => {
                        let st = *out.get(symbol).unwrap_or(&AssignState::Uninit);
                        match st {
                            AssignState::Uninit => {
                                if emitted.insert((block_id, *symbol, "use")) {
                                    diagnostics.push(
                                        FlowDiagnostic::new(
                                            FlowDiagnosticCode::UseBeforeInitialization,
                                            format!(
                                                "variable #{} is used before it is initialized",
                                                symbol.0
                                            ),
                                        )
                                        .with_span(*span),
                                    );
                                }
                            }
                            AssignState::MaybeUninit => {
                                if emitted.insert((block_id, *symbol, "maybe")) {
                                    diagnostics.push(
                                        FlowDiagnostic::new(
                                            FlowDiagnosticCode::PossiblyUninitialized,
                                            format!(
                                                "variable #{} may be used before it is initialized",
                                                symbol.0
                                            ),
                                        )
                                        .with_span(*span),
                                    );
                                }
                            }
                            AssignState::Init => {}
                        }
                    }
                    _ => {}
                }
            }
        }

        for successor in cfg.successors(block_id) {
            // Se já não há contribuição, a saída vira a entrada do successor;
            // caso contrário junta (join) com a entrada corrente.
            let updated = match states.entry(successor) {
                std::collections::hash_map::Entry::Vacant(e) => {
                    e.insert(out.clone());
                    true
                }
                std::collections::hash_map::Entry::Occupied(mut e) => {
                    let mut changed = false;
                    for s in locals {
                        let incoming = out.get(s).copied().unwrap_or(AssignState::Uninit);
                        let current = e.get().get(s).copied().unwrap_or(AssignState::Uninit);
                        let merged = current.join(incoming);
                        if merged != current {
                            e.get_mut().insert(*s, merged);
                            changed = true;
                        }
                    }
                    changed
                }
            };
            if updated && !worklist.contains(&successor) {
                worklist.push_back(successor);
            }
        }
    }

    diagnostics
}
