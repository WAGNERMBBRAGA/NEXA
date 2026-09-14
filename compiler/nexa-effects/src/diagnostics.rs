use nexa_source::SourceSpan;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectDiagnosticCode {
    ActionCallFromFunction,
    UndeclaredEffect,
    MissingPublicEffectDeclaration,
    EffectContractMismatch,
    UnknownEffect,
    DuplicateEffect,
    EffectNotAllowedInFunction,
    DeclaredEffectNotUsed,
    EffectfulMatchGuard,
    ActionValueRequiresExplicitEffects,
}

impl EffectDiagnosticCode {
    pub fn code_str(&self) -> &'static str {
        match self {
            Self::ActionCallFromFunction => "NEXA-EFFECT-0001",
            Self::UndeclaredEffect => "NEXA-EFFECT-0002",
            Self::MissingPublicEffectDeclaration => "NEXA-EFFECT-0003",
            Self::EffectContractMismatch => "NEXA-EFFECT-0004",
            Self::UnknownEffect => "NEXA-EFFECT-0005",
            Self::DuplicateEffect => "NEXA-EFFECT-0006",
            Self::EffectNotAllowedInFunction => "NEXA-EFFECT-0007",
            Self::DeclaredEffectNotUsed => "NEXA-EFFECT-0008",
            Self::EffectfulMatchGuard => "NEXA-EFFECT-0010",
            Self::ActionValueRequiresExplicitEffects => "NEXA-EFFECT-R0009",
        }
    }

    pub fn is_error(&self) -> bool {
        match self {
            Self::ActionCallFromFunction => true,
            Self::UndeclaredEffect => true,
            Self::MissingPublicEffectDeclaration => true,
            Self::EffectContractMismatch => true,
            Self::UnknownEffect => true,
            Self::DuplicateEffect => true,
            Self::EffectNotAllowedInFunction => true,
            Self::DeclaredEffectNotUsed => false,
            Self::EffectfulMatchGuard => true,
            Self::ActionValueRequiresExplicitEffects => true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EffectDiagnostic {
    pub code: EffectDiagnosticCode,
    pub message: String,
    pub span: SourceSpan,
}

impl EffectDiagnostic {
    pub fn new(code: EffectDiagnosticCode, message: String, span: SourceSpan) -> Self {
        Self {
            code,
            message,
            span,
        }
    }
}
