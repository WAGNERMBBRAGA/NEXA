use crate::cfg::{BasicBlockId, ControlFlowGraph};
use std::collections::{HashMap, VecDeque};

pub trait ForwardAnalysis {
    type State: Clone;
    fn entry_state(&self) -> Self::State;
    fn transfer(&self, block: &ControlFlowGraph, block_id: BasicBlockId, state: &mut Self::State);
    fn join(&self, into: &mut Self::State, incoming: &Self::State) -> bool;
}

pub fn run_forward_analysis<A: ForwardAnalysis>(
    analysis: &A,
    cfg: &ControlFlowGraph,
) -> HashMap<BasicBlockId, A::State> {
    let mut block_states: HashMap<BasicBlockId, A::State> = HashMap::new();
    let mut worklist: VecDeque<BasicBlockId> = VecDeque::new();

    let entry_state = analysis.entry_state();
    block_states.insert(cfg.entry, entry_state);
    worklist.push_back(cfg.entry);

    while let Some(block_id) = worklist.pop_front() {
        let state = block_states.get(&block_id).cloned().unwrap();
        let mut local_state = state;

        analysis.transfer(cfg, block_id, &mut local_state);

        if let Some(stored) = block_states.get_mut(&block_id) {
            *stored = local_state.clone();
        }

        for successor in cfg.successors(block_id) {
            let dominated = local_state.clone();
            let mut entry = block_states.entry(successor);
            let need_update = match entry {
                std::collections::hash_map::Entry::Occupied(ref mut e) => {
                    analysis.join(e.get_mut(), &dominated)
                }
                std::collections::hash_map::Entry::Vacant(e) => {
                    e.insert(dominated);
                    true
                }
            };
            if need_update && !worklist.contains(&successor) {
                worklist.push_back(successor);
            }
        }
    }

    block_states
}
