use crate::cfg::ExprRef;
use crate::diagnostics::{FlowDiagnostic, FlowDiagnosticCode};
use nexa_source::SourceSpan;
use nexa_symbols::SymbolId;
use std::collections::{BTreeSet, HashMap};

/// A literal value used in pattern matching.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Literal {
    Bool(bool),
    Int(i64),
    String(String),
}

/// A constructor tag for enum variants.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ConstructorTag {
    pub symbol: SymbolId,
    pub arity: usize,
}

/// A pattern in a match arm.
#[derive(Debug, Clone)]
pub enum Pattern {
    /// Wildcard / catch-all `_`
    Wildcard,
    /// Bind a variable (also a catch-all semantically)
    Binding(SymbolId),
    /// Literal match
    Literal(Literal),
    /// Constructor match with sub-patterns
    Constructor {
        tag: ConstructorTag,
        fields: Vec<Pattern>,
    },
    /// OR-pattern: any of the alternatives match
    Or(Vec<Pattern>),
    /// Range pattern (inclusive)
    Range {
        low: Box<Pattern>,
        high: Box<Pattern>,
    },
}

/// Represents a match arm with its guard and body expression.
#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub guard: Option<ExprRef>,
    pub body: ExprRef,
}

/// Result of exhaustiveness analysis.
#[derive(Debug, Clone)]
pub struct ExhaustivenessResult {
    pub is_exhaustive: bool,
    pub missing_patterns: Vec<MissingPattern>,
    pub unreachable_arms: Vec<usize>,
}

/// A description of a missing pattern for diagnostics.
#[derive(Debug, Clone)]
pub struct MissingPattern {
    pub description: String,
    pub span: SourceSpan,
}

/// Check whether a list of match arms is exhaustive for a given scrutinee type.
///
/// `constructors` lists all known constructors for the type being matched
/// (e.g. all enum variants). An empty list means the type is a primitive
/// with no constructors (bool is treated specially with `true`/`false`).
pub fn check_exhaustiveness(
    arms: &[MatchArm],
    constructors: &[ConstructorTag],
    scrutinee_span: SourceSpan,
) -> ExhaustivenessResult {
    let mut missing_patterns = Vec::new();
    let mut unreachable_arms = Vec::new();

    let covered = compute_covered_set(arms);

    if constructors.is_empty() {
        let has_wildcard = arms.iter().any(|arm| is_wildcard_or_binding(&arm.pattern));
        if !has_wildcard {
            missing_patterns.push(MissingPattern {
                description: "not all cases are covered".to_string(),
                span: scrutinee_span,
            });
        }
    } else {
        let covered_constructors: BTreeSet<&ConstructorTag> = covered
            .iter()
            .filter_map(|p| match p {
                Pattern::Constructor { tag, .. } => Some(tag),
                _ => None,
            })
            .collect();

        for ctor in constructors {
            if !covered_constructors.contains(ctor) {
                missing_patterns.push(MissingPattern {
                    description: format!("constructor `{}` is not covered", ctor.symbol),
                    span: scrutinee_span,
                });
            }
        }

        let has_wildcard = arms.iter().any(|arm| is_wildcard_or_binding(&arm.pattern));
        if has_wildcard {
            missing_patterns.clear();
        }
    }

    // Detect unreachable arms: a wildcard/binding after all constructors are covered makes
    // subsequent arms unreachable.
    let mut seen_constructors = BTreeSet::new();
    let mut seen_wildcard = false;
    for (idx, arm) in arms.iter().enumerate() {
        if seen_wildcard {
            unreachable_arms.push(idx);
            continue;
        }
        match &arm.pattern {
            Pattern::Wildcard | Pattern::Binding(_) => {
                seen_wildcard = true;
            }
            Pattern::Constructor { tag, .. } => {
                seen_constructors.insert(tag.clone());
                if seen_constructors.len() >= constructors.len() && !constructors.is_empty() {
                    seen_wildcard = true;
                }
            }
            _ => {}
        }
    }

    ExhaustivenessResult {
        is_exhaustive: missing_patterns.is_empty(),
        missing_patterns,
        unreachable_arms,
    }
}

/// Convert an exhaustiveness result into diagnostics.
pub fn exhaustiveness_diagnostics(
    result: &ExhaustivenessResult,
    _scrutinee_span: SourceSpan,
) -> Vec<FlowDiagnostic> {
    let mut diagnostics = Vec::new();

    if !result.is_exhaustive {
        let missing_desc: Vec<&str> = result
            .missing_patterns
            .iter()
            .map(|m| m.description.as_str())
            .collect();
        diagnostics.push(FlowDiagnostic::new(
            FlowDiagnosticCode::NonExhaustiveMatch,
            format!(
                "match is not exhaustive; missing patterns: {}",
                missing_desc.join(", ")
            ),
        ));
    }

    for &idx in &result.unreachable_arms {
        diagnostics.push(FlowDiagnostic::new(
            FlowDiagnosticCode::UnreachableMatchArm,
            format!("match arm {} is unreachable", idx + 1),
        ));
    }

    diagnostics
}

fn compute_covered_set(arms: &[MatchArm]) -> Vec<Pattern> {
    let mut covered = Vec::new();
    for arm in arms {
        covered.push(arm.pattern.clone());
    }
    covered
}

fn is_wildcard_or_binding(pattern: &Pattern) -> bool {
    matches!(pattern, Pattern::Wildcard | Pattern::Binding(_))
}

/// Check definite assignment for a set of variables in a CFG.
///
/// Returns a map from `SymbolId` to whether the variable is definitely
/// assigned on every path reaching the exit.
pub fn check_definite_assignment(
    cfg: &crate::cfg::ControlFlowGraph,
    variables: &[SymbolId],
) -> HashMap<SymbolId, bool> {
    use crate::cfg::FlowOperation;
    use crate::dataflow::{run_forward_analysis, ForwardAnalysis};
    use std::collections::HashSet;

    struct DefiniteAssignmentAnalysis;

    impl ForwardAnalysis for DefiniteAssignmentAnalysis {
        type State = HashSet<SymbolId>;

        fn entry_state(&self) -> Self::State {
            HashSet::new()
        }

        fn transfer(
            &self,
            _block: &crate::cfg::ControlFlowGraph,
            block_id: crate::cfg::BasicBlockId,
            state: &mut Self::State,
        ) {
            if let Some(bb) = _block.get_block(block_id) {
                for op in &bb.operations {
                    match op {
                        FlowOperation::WriteLocal(sym)
                        | FlowOperation::Assignment { target: sym, .. } => {
                            state.insert(*sym);
                        }
                        FlowOperation::Call { target, .. } => {
                            state.insert(*target);
                        }
                        _ => {}
                    }
                }
            }
        }

        fn join(&self, into: &mut Self::State, incoming: &Self::State) -> bool {
            let old_len = into.len();
            *into = into.intersection(incoming).copied().collect();
            into.len() != old_len
        }
    }

    let analysis = DefiniteAssignmentAnalysis;
    let states = run_forward_analysis(&analysis, cfg);

    let mut result = HashMap::new();
    for var in variables {
        // A variable is definitely assigned if it appears in ALL paths reaching the exit.
        // Since our join is intersection, the entry state is empty, and we propagate
        // only through forward edges, we check if the variable is in the state at all
        // reachable exit blocks.
        let all_exit_states: Vec<&HashSet<SymbolId>> = cfg
            .blocks
            .iter()
            .filter(|b| {
                matches!(
                    &b.terminator,
                    crate::cfg::FlowTerminator::Return(_)
                        | crate::cfg::FlowTerminator::Unreachable
                        | crate::cfg::FlowTerminator::Trap
                )
            })
            .filter_map(|b| states.get(&b.id))
            .collect();

        if all_exit_states.is_empty() {
            result.insert(*var, false);
        } else {
            let assigned = all_exit_states.iter().all(|s| s.contains(var));
            result.insert(*var, assigned);
        }
    }

    result
}
