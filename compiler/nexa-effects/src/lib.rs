pub mod analyzer;
pub mod call_graph;
pub mod diagnostics;
pub mod effect_id;
pub mod effect_registry;
pub mod effect_set;

pub use analyzer::{CallableEffectInfo, EffectAnalyzer, EffectModel, EffectProvenance};
pub use call_graph::{CallEdge, CallGraph, DispatchKind};
pub use diagnostics::{EffectDiagnostic, EffectDiagnosticCode};
pub use effect_id::EffectId;
pub use effect_registry::{EffectEntry, EffectRegistry};
pub use effect_set::EffectSet;

#[cfg(test)]
mod tests {
    use super::*;
    use nexa_source::{SourceId, SourceSpan};

    fn invalid_span() -> SourceSpan {
        SourceSpan::point(SourceId(0), 0)
    }

    #[test]
    fn effect_id_ord() {
        assert!(EffectId(0) < EffectId(1));
        assert!(EffectId(5) > EffectId(3));
    }

    #[test]
    fn effect_set_empty() {
        let set = EffectSet::new();
        assert!(set.is_empty());
        assert_eq!(set.len(), 0);
    }

    #[test]
    fn effect_set_singleton() {
        let set = EffectSet::singleton(EffectId(42));
        assert_eq!(set.len(), 1);
        assert!(set.contains(EffectId(42)));
        assert!(!set.contains(EffectId(0)));
    }

    #[test]
    fn effect_set_union() {
        let mut a = EffectSet::new();
        a.insert(EffectId(1));
        a.insert(EffectId(2));
        let mut b = EffectSet::new();
        b.insert(EffectId(2));
        b.insert(EffectId(3));
        let c = a.union(&b);
        assert_eq!(c.len(), 3);
        assert!(c.contains(EffectId(1)));
        assert!(c.contains(EffectId(2)));
        assert!(c.contains(EffectId(3)));
    }

    #[test]
    fn effect_set_is_subset() {
        let mut a = EffectSet::new();
        a.insert(EffectId(1));
        let mut b = EffectSet::new();
        b.insert(EffectId(1));
        b.insert(EffectId(2));
        assert!(a.is_subset(&b));
        assert!(!b.is_subset(&a));
    }

    #[test]
    fn effect_set_difference() {
        let mut a = EffectSet::new();
        a.insert(EffectId(1));
        a.insert(EffectId(2));
        let mut b = EffectSet::new();
        b.insert(EffectId(2));
        let diff = a.difference(&b);
        assert_eq!(diff.len(), 1);
        assert!(diff.contains(EffectId(1)));
    }

    #[test]
    fn effect_registry_standard_count() {
        let registry = EffectRegistry::new();
        assert_eq!(registry.count(), 22);
    }

    #[test]
    fn effect_registry_resolve_known() {
        let registry = EffectRegistry::new();
        assert!(registry.resolve("console::write").is_some());
        assert!(registry.resolve("network::request").is_some());
        assert!(registry.resolve("database::read").is_some());
        assert!(registry.resolve("unknown::effect").is_none());
    }

    #[test]
    fn effect_registry_resolve_set_all_known() {
        let registry = EffectRegistry::new();
        let paths = vec!["console::write".to_string(), "network::request".to_string()];
        let result = registry.resolve_set(&paths);
        assert!(result.is_ok());
        let set = result.unwrap();
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn effect_registry_resolve_set_unknown() {
        let registry = EffectRegistry::new();
        let paths = vec![
            "console::write".to_string(),
            "nonexistent::effect".to_string(),
        ];
        let result = registry.resolve_set(&paths);
        assert!(result.is_err());
        let unknown = result.unwrap_err();
        assert_eq!(unknown, vec!["nonexistent::effect"]);
    }

    #[test]
    fn effect_registry_custom_effect() {
        let mut registry = EffectRegistry::new();
        let id = registry.register("my::effect".to_string(), "Custom effect".to_string());
        assert!(registry.resolve("my::effect").is_some());
        assert_eq!(registry.resolve("my::effect").unwrap(), id);
        assert_eq!(registry.count(), 23);
    }

    #[test]
    fn effect_registry_entry_lookup() {
        let registry = EffectRegistry::new();
        let entry = registry.entry_by_path("filesystem::read");
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().id, EffectId(1));
    }

    #[test]
    fn call_graph_basic() {
        let mut graph = CallGraph::new();
        graph.add_edge(CallEdge {
            caller: nexa_symbols::SymbolId(0),
            callee: nexa_symbols::SymbolId(1),
            span: invalid_span(),
            dispatch: DispatchKind::Direct,
        });
        assert_eq!(graph.edge_count(), 1);
        assert_eq!(graph.callees_of(nexa_symbols::SymbolId(0)).len(), 1);
        assert_eq!(graph.callers_of(nexa_symbols::SymbolId(1)).len(), 1);
    }

    #[test]
    fn effect_analyzer_creation() {
        let analyzer = EffectAnalyzer::new();
        assert_eq!(analyzer.registry.count(), 22);
        assert_eq!(analyzer.call_graph.edge_count(), 0);
    }

    #[test]
    fn effect_analyzer_register_and_record() {
        let mut analyzer = EffectAnalyzer::new();
        let sym = nexa_symbols::SymbolId(0);
        analyzer.register_callable(sym, None, invalid_span());
        analyzer.record_direct_effect(sym, EffectId(0));
        let info = analyzer.get_effect_info(sym);
        assert!(info.is_some());
        assert!(info.unwrap().direct.contains(EffectId(0)));
    }

    #[test]
    fn effect_analyzer_fixpoint_propagation() {
        let mut analyzer = EffectAnalyzer::new();
        let caller = nexa_symbols::SymbolId(0);
        let callee = nexa_symbols::SymbolId(1);
        analyzer.register_callable(caller, None, invalid_span());
        analyzer.register_callable(
            callee,
            Some(EffectSet::singleton(EffectId(3))),
            invalid_span(),
        );
        analyzer.call_graph.add_edge(CallEdge {
            caller,
            callee,
            span: invalid_span(),
            dispatch: DispatchKind::Direct,
        });
        analyzer.propagate_fixpoint();
        let caller_info = analyzer.get_effect_info(caller).unwrap();
        assert!(caller_info.inferred.contains(EffectId(3)));
    }

    #[test]
    fn effect_analyzer_validate_action_call_from_function() {
        let mut analyzer = EffectAnalyzer::new();
        analyzer.validate_action_call_from_function(
            nexa_symbols::SymbolId(0),
            true,
            true,
            invalid_span(),
        );
        assert!(!analyzer.diagnostics.is_empty());
        assert_eq!(
            analyzer.diagnostics[0].code,
            EffectDiagnosticCode::ActionCallFromFunction
        );
    }

    #[test]
    fn effect_analyzer_validate_exported_action_no_clause() {
        let mut analyzer = EffectAnalyzer::new();
        let sym = nexa_symbols::SymbolId(0);
        analyzer.register_callable(sym, None, invalid_span());
        analyzer.validate_action_declaration(sym, true);
        assert!(!analyzer.diagnostics.is_empty());
        assert_eq!(
            analyzer.diagnostics[0].code,
            EffectDiagnosticCode::MissingPublicEffectDeclaration
        );
    }

    #[test]
    fn effect_analyzer_validate_exported_action_with_clause() {
        let mut analyzer = EffectAnalyzer::new();
        let sym = nexa_symbols::SymbolId(0);
        analyzer.register_callable(sym, Some(EffectSet::empty()), invalid_span());
        analyzer.validate_action_declaration(sym, true);
        assert!(analyzer.diagnostics.is_empty());
    }

    #[test]
    fn effect_analyzer_validate_interface_subset() {
        let mut analyzer = EffectAnalyzer::new();
        let sym = nexa_symbols::SymbolId(0);
        let mut declared = EffectSet::new();
        declared.insert(EffectId(0));
        declared.insert(EffectId(1));
        analyzer.register_callable(sym, Some(declared.clone()), invalid_span());
        analyzer.record_direct_effect(sym, EffectId(0));
        let interface_effects = EffectSet::singleton(EffectId(0));
        analyzer.validate_interface_effect_subset(sym, &interface_effects);
        assert!(!analyzer.diagnostics.is_empty());
        assert_eq!(
            analyzer.diagnostics[0].code,
            EffectDiagnosticCode::EffectContractMismatch
        );
    }

    #[test]
    fn effect_diagnostic_code_str() {
        assert_eq!(
            EffectDiagnosticCode::ActionCallFromFunction.code_str(),
            "NEXA-EFFECT-0001"
        );
        assert_eq!(
            EffectDiagnosticCode::UndeclaredEffect.code_str(),
            "NEXA-EFFECT-0002"
        );
        assert_eq!(
            EffectDiagnosticCode::UnknownEffect.code_str(),
            "NEXA-EFFECT-0005"
        );
    }

    #[test]
    fn effect_diagnostic_severity() {
        assert!(EffectDiagnosticCode::ActionCallFromFunction.is_error());
        assert!(EffectDiagnosticCode::MissingPublicEffectDeclaration.is_error());
        assert!(!EffectDiagnosticCode::DeclaredEffectNotUsed.is_error());
    }
}
