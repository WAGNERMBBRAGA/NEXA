pub mod analyzer;
pub mod contract;
pub mod diagnostics;

pub use analyzer::ContractAnalyzer;
pub use contract::{ContractExpression, ContractInfo, OldCapture, OldCaptureId};
pub use diagnostics::{ContractDiagnostic, ContractDiagnosticCode};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::ContractContext;
    use nexa_source::{SourceId, SourceSpan};
    use nexa_types::prelude::bootstrap_prelude;
    use nexa_types::store::TypeStore;
    use nexa_types::ty::CallableKind;

    fn dummy_span() -> SourceSpan {
        SourceSpan::point(SourceId(0), 0)
    }

    #[test]
    fn contract_info_new_empty() {
        let info = ContractInfo::new();
        assert!(!info.has_contracts());
        assert!(!info.has_require());
        assert!(!info.has_ensure());
        assert!(info.requires.is_empty());
        assert!(info.ensures.is_empty());
    }

    #[test]
    fn contract_info_add_require() {
        let mut store = TypeStore::new();
        let prelude = bootstrap_prelude(&mut store);
        let mut info = ContractInfo::new();
        info.add_require(dummy_span(), prelude.bool);
        assert!(info.has_contracts());
        assert!(info.has_require());
        assert_eq!(info.requires.len(), 1);
    }

    #[test]
    fn contract_info_add_ensure() {
        let mut store = TypeStore::new();
        let prelude = bootstrap_prelude(&mut store);
        let mut info = ContractInfo::new();
        info.add_ensure(dummy_span(), prelude.bool);
        assert!(info.has_contracts());
        assert!(info.has_ensure());
        assert_eq!(info.ensures.len(), 1);
    }

    #[test]
    fn contract_info_old_capture() {
        let mut info = ContractInfo::new();
        let id = info.add_old_capture(dummy_span(), nexa_types::TypeId(0));
        assert_eq!(id, OldCaptureId(0));
        let id2 = info.add_old_capture(dummy_span(), nexa_types::TypeId(1));
        assert_eq!(id2, OldCaptureId(1));
        assert_eq!(info.old_captures.len(), 2);
    }

    #[test]
    fn require_bool_valid() {
        let mut store = TypeStore::new();
        let prelude = bootstrap_prelude(&mut store);
        let mut analyzer = ContractAnalyzer::new(&store);
        analyzer.validate_require_expression(prelude.bool, dummy_span(), ContractContext::Require);
        assert!(analyzer.diagnostics.is_empty());
    }

    #[test]
    fn require_non_bool_rejected() {
        let mut store = TypeStore::new();
        let prelude = bootstrap_prelude(&mut store);
        let mut analyzer = ContractAnalyzer::new(&store);
        analyzer.validate_require_expression(prelude.int, dummy_span(), ContractContext::Require);
        assert_eq!(analyzer.diagnostics.len(), 1);
        assert_eq!(
            analyzer.diagnostics[0].code,
            ContractDiagnosticCode::RequireMustBeBool
        );
    }

    #[test]
    fn ensure_bool_valid() {
        let mut store = TypeStore::new();
        let prelude = bootstrap_prelude(&mut store);
        let mut analyzer = ContractAnalyzer::new(&store);
        analyzer.validate_ensure_expression(prelude.bool, dummy_span());
        assert!(analyzer.diagnostics.is_empty());
    }

    #[test]
    fn ensure_non_bool_rejected() {
        let mut store = TypeStore::new();
        let prelude = bootstrap_prelude(&mut store);
        let mut analyzer = ContractAnalyzer::new(&store);
        analyzer.validate_ensure_expression(prelude.string, dummy_span());
        assert_eq!(analyzer.diagnostics.len(), 1);
        assert_eq!(
            analyzer.diagnostics[0].code,
            ContractDiagnosticCode::EnsureMustBeBool
        );
    }

    #[test]
    fn result_in_require_rejected() {
        let mut store = TypeStore::new();
        let _prelude = bootstrap_prelude(&mut store);
        let mut analyzer = ContractAnalyzer::new(&store);
        analyzer.validate_result_reference(dummy_span(), ContractContext::Require);
        assert_eq!(analyzer.diagnostics.len(), 1);
        assert_eq!(
            analyzer.diagnostics[0].code,
            ContractDiagnosticCode::InvalidResultReference
        );
    }

    #[test]
    fn result_in_ensure_ok() {
        let mut store = TypeStore::new();
        let _prelude = bootstrap_prelude(&mut store);
        let mut analyzer = ContractAnalyzer::new(&store);
        analyzer.validate_result_reference(dummy_span(), ContractContext::Ensure);
        assert!(analyzer.diagnostics.is_empty());
    }

    #[test]
    fn old_in_require_rejected() {
        let mut store = TypeStore::new();
        let _prelude = bootstrap_prelude(&mut store);
        let mut analyzer = ContractAnalyzer::new(&store);
        analyzer.validate_old_reference(dummy_span(), ContractContext::Require);
        assert_eq!(analyzer.diagnostics.len(), 1);
        assert_eq!(
            analyzer.diagnostics[0].code,
            ContractDiagnosticCode::InvalidOldReference
        );
    }

    #[test]
    fn old_in_ensure_ok() {
        let mut store = TypeStore::new();
        let _prelude = bootstrap_prelude(&mut store);
        let mut analyzer = ContractAnalyzer::new(&store);
        analyzer.validate_old_reference(dummy_span(), ContractContext::Ensure);
        assert!(analyzer.diagnostics.is_empty());
    }

    #[test]
    fn old_copy_type_ok() {
        let mut store = TypeStore::new();
        let prelude = bootstrap_prelude(&mut store);
        let mut analyzer = ContractAnalyzer::new(&store);
        analyzer.validate_old_copy_type(prelude.int, dummy_span());
        assert!(analyzer.diagnostics.is_empty());
        analyzer.validate_old_copy_type(prelude.bool, dummy_span());
        assert!(analyzer.diagnostics.is_empty());
    }

    #[test]
    fn old_non_copy_type_rejected() {
        let mut store = TypeStore::new();
        let prelude = bootstrap_prelude(&mut store);
        let mut analyzer = ContractAnalyzer::new(&store);
        analyzer.validate_old_copy_type(prelude.string, dummy_span());
        assert_eq!(analyzer.diagnostics.len(), 1);
        assert_eq!(
            analyzer.diagnostics[0].code,
            ContractDiagnosticCode::InvalidOldReference
        );
    }

    #[test]
    fn ensure_on_never_rejected() {
        let mut store = TypeStore::new();
        let prelude = bootstrap_prelude(&mut store);
        let mut analyzer = ContractAnalyzer::new(&store);
        analyzer.validate_ensure_on_never(prelude.never, dummy_span());
        assert_eq!(analyzer.diagnostics.len(), 1);
        assert_eq!(
            analyzer.diagnostics[0].code,
            ContractDiagnosticCode::InvalidContractContext
        );
    }

    #[test]
    fn ensure_on_non_never_ok() {
        let mut store = TypeStore::new();
        let prelude = bootstrap_prelude(&mut store);
        let mut analyzer = ContractAnalyzer::new(&store);
        analyzer.validate_ensure_on_never(prelude.int, dummy_span());
        assert!(analyzer.diagnostics.is_empty());
    }

    #[test]
    fn contract_purity_function_ok() {
        let mut store = TypeStore::new();
        let _prelude = bootstrap_prelude(&mut store);
        let mut analyzer = ContractAnalyzer::new(&store);
        analyzer.validate_contract_purity(CallableKind::Function, dummy_span());
        assert!(analyzer.diagnostics.is_empty());
    }

    #[test]
    fn contract_purity_action_rejected() {
        let mut store = TypeStore::new();
        let _prelude = bootstrap_prelude(&mut store);
        let mut analyzer = ContractAnalyzer::new(&store);
        analyzer.validate_contract_purity(CallableKind::Action, dummy_span());
        assert_eq!(analyzer.diagnostics.len(), 1);
        assert_eq!(
            analyzer.diagnostics[0].code,
            ContractDiagnosticCode::ContractMustBePure
        );
    }

    #[test]
    fn forbidden_contract_operation() {
        let mut store = TypeStore::new();
        let _prelude = bootstrap_prelude(&mut store);
        let mut analyzer = ContractAnalyzer::new(&store);
        analyzer.validate_forbidden_contract_operation("try", dummy_span());
        assert_eq!(analyzer.diagnostics.len(), 1);
        assert_eq!(
            analyzer.diagnostics[0].code,
            ContractDiagnosticCode::ForbiddenContractOperation
        );
    }

    #[test]
    fn interface_impl_contract_override_rejected() {
        let mut store = TypeStore::new();
        let _prelude = bootstrap_prelude(&mut store);
        let mut analyzer = ContractAnalyzer::new(&store);
        analyzer.validate_interface_implementation_no_contract_override(dummy_span());
        assert_eq!(analyzer.diagnostics.len(), 1);
        assert_eq!(
            analyzer.diagnostics[0].code,
            ContractDiagnosticCode::InterfaceImplementationContractOverride
        );
    }

    #[test]
    fn contract_diagnostic_code_str() {
        assert_eq!(
            ContractDiagnosticCode::RequireMustBeBool.code_str(),
            "NEXA-CONTRACT-0001"
        );
        assert_eq!(
            ContractDiagnosticCode::EnsureMustBeBool.code_str(),
            "NEXA-CONTRACT-0002"
        );
        assert_eq!(
            ContractDiagnosticCode::ForbiddenContractOperation.code_str(),
            "NEXA-CONTRACT-0009"
        );
    }

    #[test]
    fn contract_diagnostic_code_is_error() {
        assert!(ContractDiagnosticCode::RequireMustBeBool.is_error());
        assert!(ContractDiagnosticCode::InvalidOldReference.is_error());
        assert!(ContractDiagnosticCode::InterfaceImplementationContractOverride.is_error());
    }
}
