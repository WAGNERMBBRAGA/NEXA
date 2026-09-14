use crate::diagnostics::{ContractDiagnostic, ContractDiagnosticCode};
use nexa_source::SourceSpan;
use nexa_types::{
    compatibility,
    ty::{self, CallableKind},
    TypeId, TypeStore,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractContext {
    Require,
    Ensure,
}

pub struct ContractAnalyzer<'a> {
    pub store: &'a TypeStore,
    pub diagnostics: Vec<ContractDiagnostic>,
}

impl<'a> ContractAnalyzer<'a> {
    pub fn new(store: &'a TypeStore) -> Self {
        Self {
            store,
            diagnostics: Vec::new(),
        }
    }

    pub fn validate_require_expression(
        &mut self,
        expr_ty: TypeId,
        span: SourceSpan,
        _context: ContractContext,
    ) {
        if !compatibility::is_bool(self.store, expr_ty) {
            self.diagnostics.push(ContractDiagnostic::new(
                ContractDiagnosticCode::RequireMustBeBool,
                "require expression must be Bool".to_string(),
                span,
            ));
        }
    }

    pub fn validate_ensure_expression(&mut self, expr_ty: TypeId, span: SourceSpan) {
        if !compatibility::is_bool(self.store, expr_ty) {
            self.diagnostics.push(ContractDiagnostic::new(
                ContractDiagnosticCode::EnsureMustBeBool,
                "ensure expression must be Bool".to_string(),
                span,
            ));
        }
    }

    pub fn validate_result_reference(&mut self, span: SourceSpan, context: ContractContext) {
        if context == ContractContext::Require {
            self.diagnostics.push(ContractDiagnostic::new(
                ContractDiagnosticCode::InvalidResultReference,
                "'result' is not available in require expressions".to_string(),
                span,
            ));
        }
    }

    pub fn validate_old_reference(&mut self, span: SourceSpan, context: ContractContext) {
        if context == ContractContext::Require {
            self.diagnostics.push(ContractDiagnostic::new(
                ContractDiagnosticCode::InvalidOldReference,
                "'old' is only available in ensure expressions".to_string(),
                span,
            ));
        }
    }

    pub fn validate_old_copy_type(&mut self, ty: TypeId, span: SourceSpan) {
        if !self.is_copy_type(ty) {
            self.diagnostics.push(ContractDiagnostic::new(
                ContractDiagnosticCode::InvalidOldReference,
                "old() expression must have Copy type in Core 1.0".to_string(),
                span,
            ));
        }
    }

    pub fn validate_ensure_on_never(&mut self, return_type: TypeId, span: SourceSpan) {
        if matches!(self.store.get_type(return_type), Some(ty::Type::Never)) {
            self.diagnostics.push(ContractDiagnostic::new(
                ContractDiagnosticCode::InvalidContractContext,
                "ensure is not valid on Never-returning callable".to_string(),
                span,
            ));
        }
    }

    pub fn validate_contract_purity(&mut self, callable_kind: CallableKind, span: SourceSpan) {
        if callable_kind != CallableKind::Function {
            self.diagnostics.push(ContractDiagnostic::new(
                ContractDiagnosticCode::ContractMustBePure,
                "contract expressions cannot contain action calls".to_string(),
                span,
            ));
        }
    }

    pub fn validate_forbidden_contract_operation(&mut self, operation: &str, span: SourceSpan) {
        self.diagnostics.push(ContractDiagnostic::new(
            ContractDiagnosticCode::ForbiddenContractOperation,
            format!("forbidden operation in contract: {}", operation),
            span,
        ));
    }

    pub fn validate_interface_implementation_no_contract_override(&mut self, span: SourceSpan) {
        self.diagnostics.push(ContractDiagnostic::new(
            ContractDiagnosticCode::InterfaceImplementationContractOverride,
            "interface implementation members must not add or replace require/ensure clauses"
                .to_string(),
            span,
        ));
    }

    fn is_copy_type(&self, ty: TypeId) -> bool {
        matches!(
            self.store.get_type(ty),
            Some(ty::Type::Int)
                | Some(ty::Type::Int8)
                | Some(ty::Type::Int16)
                | Some(ty::Type::Int32)
                | Some(ty::Type::Int64)
                | Some(ty::Type::UInt)
                | Some(ty::Type::UInt8)
                | Some(ty::Type::UInt16)
                | Some(ty::Type::UInt32)
                | Some(ty::Type::UInt64)
                | Some(ty::Type::Float32)
                | Some(ty::Type::Float64)
                | Some(ty::Type::Bool)
                | Some(ty::Type::Char)
                | Some(ty::Type::Byte)
                | Some(ty::Type::Unit)
        )
    }
}
