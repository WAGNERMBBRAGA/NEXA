use nexa_source::SourceSpan;

/// Type diagnostic codes (45 codes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum TypeDiagnosticCode {
    UnknownType = 1,
    TypeMismatch = 2,
    InvalidImplicitConversion = 3,
    GenericConstraintNotSatisfied = 4,
    CannotInferType = 5,
    InvalidDistinctTypeUse = 6,
    InvalidCallableType = 7,
    NumericLiteralOutOfRange = 8,
    InvalidOperatorOperands = 9,
    InvalidReturnType = 10,
    InvalidArgumentType = 11,
    ArgumentCountMismatch = 12,
    UnknownMember = 13,
    MemberNotAccessible = 14,
    NotCallable = 15,
    InvalidIndexType = 16,
    NotIndexable = 17,
    InvalidStructConstruction = 18,
    MissingStructField = 19,
    UnknownStructField = 20,
    DuplicateStructField = 21,
    InvalidEnumVariantPayload = 22,
    MatchArmTypeMismatch = 23,
    InvalidInterfaceImplementation = 24,
    MissingInterfaceMember = 25,
    IncompatibleInterfaceMember = 26,
    DuplicateImplementation = 27,
    CoherenceViolation = 28,
    InvalidSelfType = 29,
    RecursiveTypeCycle = 30,
    GenericArgumentCountMismatch = 31,
    ConstraintMustBeInterface = 32,
    ConflictingGenericInference = 33,
    AmbiguousMember = 34,
    AwaitRequiresTask = 35,
    TryRequiresResultContext = 36,
    TryErrorTypeMismatch = 37,
    BorrowRequiresPlace = 38,
    InvalidPatternType = 39,
    MatchGuardMustBeBool = 40,
    NotIterable = 41,
    OverlappingImplementation = 42,
    PublicConstRequiresExplicitType = 43,
    PublicApiExposesNonPublicType = 44,
    UnexpectedInterfaceImplementationMember = 45,
}

impl TypeDiagnosticCode {
    pub fn code_str(self) -> &'static str {
        match self {
            Self::UnknownType => "NEXA-TYPE-0001",
            Self::TypeMismatch => "NEXA-TYPE-0002",
            Self::InvalidImplicitConversion => "NEXA-TYPE-0003",
            Self::GenericConstraintNotSatisfied => "NEXA-TYPE-0004",
            Self::CannotInferType => "NEXA-TYPE-0005",
            Self::InvalidDistinctTypeUse => "NEXA-TYPE-0006",
            Self::InvalidCallableType => "NEXA-TYPE-0007",
            Self::NumericLiteralOutOfRange => "NEXA-TYPE-0008",
            Self::InvalidOperatorOperands => "NEXA-TYPE-0009",
            Self::InvalidReturnType => "NEXA-TYPE-0010",
            Self::InvalidArgumentType => "NEXA-TYPE-0011",
            Self::ArgumentCountMismatch => "NEXA-TYPE-0012",
            Self::UnknownMember => "NEXA-TYPE-0013",
            Self::MemberNotAccessible => "NEXA-TYPE-0014",
            Self::NotCallable => "NEXA-TYPE-0015",
            Self::InvalidIndexType => "NEXA-TYPE-0016",
            Self::NotIndexable => "NEXA-TYPE-0017",
            Self::InvalidStructConstruction => "NEXA-TYPE-0018",
            Self::MissingStructField => "NEXA-TYPE-0019",
            Self::UnknownStructField => "NEXA-TYPE-0020",
            Self::DuplicateStructField => "NEXA-TYPE-0021",
            Self::InvalidEnumVariantPayload => "NEXA-TYPE-0022",
            Self::MatchArmTypeMismatch => "NEXA-TYPE-0023",
            Self::InvalidInterfaceImplementation => "NEXA-TYPE-0024",
            Self::MissingInterfaceMember => "NEXA-TYPE-0025",
            Self::IncompatibleInterfaceMember => "NEXA-TYPE-0026",
            Self::DuplicateImplementation => "NEXA-TYPE-0027",
            Self::CoherenceViolation => "NEXA-TYPE-0028",
            Self::InvalidSelfType => "NEXA-TYPE-0029",
            Self::RecursiveTypeCycle => "NEXA-TYPE-0030",
            Self::GenericArgumentCountMismatch => "NEXA-TYPE-0031",
            Self::ConstraintMustBeInterface => "NEXA-TYPE-0032",
            Self::ConflictingGenericInference => "NEXA-TYPE-0033",
            Self::AmbiguousMember => "NEXA-TYPE-0034",
            Self::AwaitRequiresTask => "NEXA-TYPE-0035",
            Self::TryRequiresResultContext => "NEXA-TYPE-0036",
            Self::TryErrorTypeMismatch => "NEXA-TYPE-0037",
            Self::BorrowRequiresPlace => "NEXA-TYPE-0038",
            Self::InvalidPatternType => "NEXA-TYPE-0039",
            Self::MatchGuardMustBeBool => "NEXA-TYPE-0040",
            Self::NotIterable => "NEXA-TYPE-0041",
            Self::OverlappingImplementation => "NEXA-TYPE-0042",
            Self::PublicConstRequiresExplicitType => "NEXA-TYPE-0043",
            Self::PublicApiExposesNonPublicType => "NEXA-TYPE-0044",
            Self::UnexpectedInterfaceImplementationMember => "NEXA-TYPE-0045",
        }
    }

    pub fn severity(self) -> DiagnosticSeverity {
        DiagnosticSeverity::Error
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Note,
}

/// A type diagnostic produced during checking.
#[derive(Debug, Clone)]
pub struct TypeDiagnostic {
    pub code: TypeDiagnosticCode,
    pub message: String,
    pub span: SourceSpan,
    pub severity: DiagnosticSeverity,
    pub context: Option<String>,
}

impl TypeDiagnostic {
    pub fn new(code: TypeDiagnosticCode, message: String, span: SourceSpan) -> Self {
        TypeDiagnostic {
            code,
            message,
            span,
            severity: code.severity(),
            context: None,
        }
    }

    pub fn with_context(mut self, ctx: String) -> Self {
        self.context = Some(ctx);
        self
    }
}
