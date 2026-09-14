use nexa_source::SourceSpan;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractDiagnosticCode {
    RequireMustBeBool,
    EnsureMustBeBool,
    ContractMustBePure,
    InvalidResultReference,
    InvalidOldReference,
    ContractEvaluationCycle,
    InvalidContractContext,
    InterfaceImplementationContractOverride,
    ForbiddenContractOperation,
}

impl ContractDiagnosticCode {
    pub fn code_str(&self) -> &'static str {
        match self {
            Self::RequireMustBeBool => "NEXA-CONTRACT-0001",
            Self::EnsureMustBeBool => "NEXA-CONTRACT-0002",
            Self::ContractMustBePure => "NEXA-CONTRACT-0003",
            Self::InvalidResultReference => "NEXA-CONTRACT-0004",
            Self::InvalidOldReference => "NEXA-CONTRACT-0005",
            Self::ContractEvaluationCycle => "NEXA-CONTRACT-0006",
            Self::InvalidContractContext => "NEXA-CONTRACT-0007",
            Self::InterfaceImplementationContractOverride => "NEXA-CONTRACT-0008",
            Self::ForbiddenContractOperation => "NEXA-CONTRACT-0009",
        }
    }

    pub fn is_error(&self) -> bool {
        match self {
            Self::RequireMustBeBool => true,
            Self::EnsureMustBeBool => true,
            Self::ContractMustBePure => true,
            Self::InvalidResultReference => true,
            Self::InvalidOldReference => true,
            Self::ContractEvaluationCycle => true,
            Self::InvalidContractContext => true,
            Self::InterfaceImplementationContractOverride => true,
            Self::ForbiddenContractOperation => true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ContractDiagnostic {
    pub code: ContractDiagnosticCode,
    pub message: String,
    pub span: SourceSpan,
}

impl ContractDiagnostic {
    pub fn new(code: ContractDiagnosticCode, message: String, span: SourceSpan) -> Self {
        Self {
            code,
            message,
            span,
        }
    }
}
