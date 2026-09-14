use crate::call_graph::CallGraph;
use crate::diagnostics::{EffectDiagnostic, EffectDiagnosticCode};
use crate::effect_id::EffectId;
use crate::effect_registry::EffectRegistry;
use crate::effect_set::EffectSet;
use nexa_source::{SourceId, SourceSpan};
use nexa_symbols::SymbolId;
use std::collections::{BTreeMap, HashMap};

fn invalid_span() -> SourceSpan {
    SourceSpan::point(SourceId(0), 0)
}

#[derive(Debug, Clone)]
pub struct CallableEffectInfo {
    pub callable: SymbolId,
    pub declared: Option<EffectSet>,
    pub inferred: EffectSet,
    pub direct: EffectSet,
}

#[derive(Debug, Clone)]
pub struct EffectProvenance {
    pub source: SymbolId,
    pub effect: EffectId,
    pub path: Vec<SymbolId>,
}

#[derive(Debug, Clone)]
pub struct EffectModel {
    pub callable_effects: BTreeMap<SymbolId, CallableEffectInfo>,
    pub provenance: Vec<EffectProvenance>,
}

pub struct EffectAnalyzer {
    pub registry: EffectRegistry,
    pub call_graph: CallGraph,
    pub model: EffectModel,
    pub diagnostics: Vec<EffectDiagnostic>,
    /// Span do nome de cada callable, usado nos diagnostics de validação.
    pub callable_spans: HashMap<SymbolId, SourceSpan>,
}

impl EffectAnalyzer {
    pub fn new() -> Self {
        Self {
            registry: EffectRegistry::new(),
            call_graph: CallGraph::new(),
            model: EffectModel {
                callable_effects: BTreeMap::new(),
                provenance: Vec::new(),
            },
            diagnostics: Vec::new(),
            callable_spans: HashMap::new(),
        }
    }

    pub fn register_callable(
        &mut self,
        symbol: SymbolId,
        declared: Option<EffectSet>,
        span: SourceSpan,
    ) {
        self.model.callable_effects.insert(
            symbol,
            CallableEffectInfo {
                callable: symbol,
                declared,
                inferred: EffectSet::new(),
                direct: EffectSet::new(),
            },
        );
        self.callable_spans.insert(symbol, span);
    }

    pub fn record_direct_effect(&mut self, caller: SymbolId, effect: EffectId) {
        if let Some(info) = self.model.callable_effects.get_mut(&caller) {
            info.direct.insert(effect);
        }
    }

    pub fn propagate_fixpoint(&mut self) {
        let symbols: Vec<SymbolId> = self.model.callable_effects.keys().copied().collect();
        let mut changed = true;
        let mut iterations = 0;
        let max_iterations = 100;

        while changed && iterations < max_iterations {
            changed = false;
            iterations += 1;

            for symbol in &symbols {
                let direct = self.model.callable_effects[symbol].direct.clone();

                let mut callee_effects = EffectSet::new();
                for edge in self.call_graph.callees_of(*symbol) {
                    if let Some(callee_info) = self.model.callable_effects.get(&edge.callee) {
                        let callee_set = if let Some(ref declared) = callee_info.declared {
                            declared.clone()
                        } else {
                            callee_info.inferred.clone()
                        };
                        callee_effects = callee_effects.union(&callee_set);
                    }
                }

                let new_inferred = direct.union(&callee_effects);

                if let Some(info) = self.model.callable_effects.get_mut(symbol) {
                    if info.inferred != new_inferred {
                        info.inferred = new_inferred;
                        changed = true;
                    }
                }
            }
        }
    }

    pub fn validate_function_purity(&mut self, symbol: SymbolId) {
        let is_pure = match self.model.callable_effects.get(&symbol) {
            Some(info) => info.direct.is_empty() && info.inferred.is_empty(),
            None => true,
        };

        if !is_pure {
            let span = self
                .callable_spans
                .get(&symbol)
                .copied()
                .unwrap_or_else(invalid_span);
            self.diagnostics.push(EffectDiagnostic::new(
                EffectDiagnosticCode::ActionCallFromFunction,
                "function must be pure but has effects".to_string(),
                span,
            ));
        }
    }

    pub fn validate_action_declaration(&mut self, symbol: SymbolId, is_exported: bool) {
        if let Some(info) = self.model.callable_effects.get(&symbol) {
            let span = self
                .callable_spans
                .get(&symbol)
                .copied()
                .unwrap_or_else(invalid_span);
            if is_exported && info.declared.is_none() {
                self.diagnostics.push(EffectDiagnostic::new(
                    EffectDiagnosticCode::MissingPublicEffectDeclaration,
                    "exported action must have an explicit effects clause".to_string(),
                    span,
                ));
            }

            if let Some(ref declared) = info.declared {
                if !info.inferred.is_subset(declared) {
                    let extra = info.inferred.difference(declared);
                    self.diagnostics.push(EffectDiagnostic::new(
                        EffectDiagnosticCode::UndeclaredEffect,
                        format!(
                            "inferred effects exceed declared effects (extra: {} effects)",
                            extra.len()
                        ),
                        span,
                    ));
                }
            }
        }
    }

    pub fn validate_interface_effect_subset(
        &mut self,
        impl_symbol: SymbolId,
        interface_effects: &EffectSet,
    ) {
        if let Some(info) = self.model.callable_effects.get(&impl_symbol) {
            let actual = if let Some(ref declared) = info.declared {
                declared.clone()
            } else {
                info.inferred.clone()
            };

            if !actual.is_subset(interface_effects) {
                self.diagnostics.push(EffectDiagnostic::new(
                    EffectDiagnosticCode::EffectContractMismatch,
                    "implementation effects exceed interface contract".to_string(),
                    invalid_span(),
                ));
            }
        }
    }

    pub fn validate_action_call_from_function(
        &mut self,
        _caller: SymbolId,
        caller_is_function: bool,
        callee_is_action: bool,
        span: SourceSpan,
    ) {
        if caller_is_function && callee_is_action {
            self.diagnostics.push(EffectDiagnostic::new(
                EffectDiagnosticCode::ActionCallFromFunction,
                "functions cannot call actions".to_string(),
                span,
            ));
        }
    }

    pub fn get_effect_info(&self, symbol: SymbolId) -> Option<&CallableEffectInfo> {
        self.model.callable_effects.get(&symbol)
    }

    pub fn provenance_for(&self, symbol: SymbolId, effect: EffectId) -> Option<&EffectProvenance> {
        self.model
            .provenance
            .iter()
            .find(|p| p.source == symbol && p.effect == effect)
    }
}

impl Default for EffectAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}
