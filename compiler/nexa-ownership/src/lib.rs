use std::collections::HashMap;

use nexa_source::SourceSpan;
use nexa_symbols::SymbolId;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Place {
    Local(SymbolId),
    Field(Box<Place>, SymbolId),
    Index(Box<Place>, ExprRef),
    Deref(Box<Place>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExprRef(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UseKind {
    Read,
    Copy,
    Move,
    BorrowShared,
    BorrowMutable,
    Mutate,
    Consume,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnershipState {
    Available,
    Moved,
    MaybeMoved,
    PartiallyMoved,
    MaybePartiallyMoved,
    Consumed,
    Error,
}

#[derive(Debug, Clone)]
pub struct TypeProperties {
    pub is_copy: bool,
    pub is_clone: bool,
    pub is_resource: bool,
    pub needs_cleanup: bool,
    pub is_send: bool,
    pub is_share: bool,
}

impl Place {
    pub fn local(sym: SymbolId) -> Self {
        Place::Local(sym)
    }

    pub fn field(base: Place, field: SymbolId) -> Self {
        Place::Field(Box::new(base), field)
    }

    pub fn index(base: Place, index: ExprRef) -> Self {
        Place::Index(Box::new(base), index)
    }

    pub fn deref(base: Place) -> Self {
        Place::Deref(Box::new(base))
    }

    pub fn root(&self) -> SymbolId {
        match self {
            Place::Local(sym) => *sym,
            Place::Field(base, _) | Place::Index(base, _) | Place::Deref(base) => base.root(),
        }
    }
}

impl TypeProperties {
    pub fn copy_type() -> Self {
        Self {
            is_copy: true,
            is_clone: true,
            is_resource: false,
            needs_cleanup: false,
            is_send: true,
            is_share: true,
        }
    }

    pub fn move_type() -> Self {
        Self {
            is_copy: false,
            is_clone: true,
            is_resource: false,
            needs_cleanup: false,
            is_send: true,
            is_share: false,
        }
    }

    pub fn resource_type() -> Self {
        Self {
            is_copy: false,
            is_clone: false,
            is_resource: true,
            needs_cleanup: true,
            is_send: false,
            is_share: false,
        }
    }
}

pub fn validate_type_properties(props: &TypeProperties) -> Vec<String> {
    let mut errors = Vec::new();
    if props.is_resource && props.is_copy {
        errors.push("Resource types cannot implement Copy".to_string());
    }
    errors
}

#[derive(Debug, Clone)]
pub struct OwnershipDiagnostic {
    pub code: OwnershipDiagnosticCode,
    pub message: String,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnershipDiagnosticCode {
    UseAfterMove,
    DoubleMove,
    PossiblyMovedValue,
    MoveWhileBorrowed,
    InvalidPartialMove,
    UseOfPartiallyMovedValue,
    CannotCopyMoveOnlyType,
    InvalidClone,
    InvalidConsume,
    MoveFromBorrowedValue,
    CannotMoveOutOfSharedReference,
    CannotMoveOutOfIndexedPlace,
    SelfMoveAssignment,
}

impl OwnershipDiagnosticCode {
    pub fn as_str(&self) -> &str {
        match self {
            OwnershipDiagnosticCode::UseAfterMove => "NEXA-OWN-0001",
            OwnershipDiagnosticCode::DoubleMove => "NEXA-OWN-0002",
            OwnershipDiagnosticCode::PossiblyMovedValue => "NEXA-OWN-0003",
            OwnershipDiagnosticCode::MoveWhileBorrowed => "NEXA-OWN-0004",
            OwnershipDiagnosticCode::InvalidPartialMove => "NEXA-OWN-0005",
            OwnershipDiagnosticCode::UseOfPartiallyMovedValue => "NEXA-OWN-0006",
            OwnershipDiagnosticCode::CannotCopyMoveOnlyType => "NEXA-OWN-0007",
            OwnershipDiagnosticCode::InvalidClone => "NEXA-OWN-0008",
            OwnershipDiagnosticCode::InvalidConsume => "NEXA-OWN-0009",
            OwnershipDiagnosticCode::MoveFromBorrowedValue => "NEXA-OWN-0010",
            OwnershipDiagnosticCode::CannotMoveOutOfSharedReference => "NEXA-OWN-0011",
            OwnershipDiagnosticCode::CannotMoveOutOfIndexedPlace => "NEXA-OWN-0012",
            OwnershipDiagnosticCode::SelfMoveAssignment => "NEXA-OWN-0013",
        }
    }

    pub fn is_error(&self) -> bool {
        true
    }
}

pub struct OwnershipAnalyzer {
    place_states: HashMap<Place, OwnershipState>,
    diagnostics: Vec<OwnershipDiagnostic>,
}

impl Default for OwnershipAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl OwnershipAnalyzer {
    pub fn new() -> Self {
        Self {
            place_states: HashMap::new(),
            diagnostics: Vec::new(),
        }
    }

    pub fn get_state(&self, place: &Place) -> OwnershipState {
        self.place_states
            .get(place)
            .copied()
            .unwrap_or(OwnershipState::Available)
    }

    pub fn record_use(&mut self, place: Place, kind: UseKind, span: SourceSpan) {
        let current = self.get_state(&place);

        match (current, kind) {
            // Available + Move → Moved
            (OwnershipState::Available, UseKind::Move) => {
                self.place_states.insert(place, OwnershipState::Moved);
            }

            // Moved + any use requiring value → UseAfterMove
            (OwnershipState::Moved, UseKind::Move) => {
                self.diagnostics.push(OwnershipDiagnostic {
                    code: OwnershipDiagnosticCode::DoubleMove,
                    message: "value moved twice".to_string(),
                    span,
                });
                self.place_states.insert(place, OwnershipState::Error);
            }
            (OwnershipState::Moved, UseKind::Read) => {
                self.diagnostics.push(OwnershipDiagnostic {
                    code: OwnershipDiagnosticCode::UseAfterMove,
                    message: "use of moved value".to_string(),
                    span,
                });
            }
            (OwnershipState::Moved, UseKind::Copy) => {
                self.diagnostics.push(OwnershipDiagnostic {
                    code: OwnershipDiagnosticCode::UseAfterMove,
                    message: "use of moved value".to_string(),
                    span,
                });
            }
            (OwnershipState::Moved, UseKind::Consume) => {
                self.diagnostics.push(OwnershipDiagnostic {
                    code: OwnershipDiagnosticCode::UseAfterMove,
                    message: "use of moved value".to_string(),
                    span,
                });
            }
            (OwnershipState::Moved, UseKind::Mutate) => {
                self.diagnostics.push(OwnershipDiagnostic {
                    code: OwnershipDiagnosticCode::UseAfterMove,
                    message: "use of moved value".to_string(),
                    span,
                });
            }

            // Any + BorrowShared/BorrowMutable → Available
            (_, UseKind::BorrowShared) | (_, UseKind::BorrowMutable) => {
                self.place_states.insert(place, OwnershipState::Available);
            }

            // Available + Copy → stays Available
            (OwnershipState::Available, UseKind::Copy) => {}
            (OwnershipState::Available, UseKind::Read) => {}
            (OwnershipState::Available, UseKind::Consume) => {
                self.place_states.insert(place, OwnershipState::Consumed);
            }

            // Any + Mutate → Available
            (_, UseKind::Mutate) => {
                self.place_states.insert(place, OwnershipState::Available);
            }

            // Error stays
            (OwnershipState::Error, _) => {}

            // MaybeMoved + value uses → PossiblyMovedValue diagnostic
            (OwnershipState::MaybeMoved, UseKind::Read)
            | (OwnershipState::MaybeMoved, UseKind::Copy)
            | (OwnershipState::MaybeMoved, UseKind::Move)
            | (OwnershipState::MaybeMoved, UseKind::Consume) => {
                self.diagnostics.push(OwnershipDiagnostic {
                    code: OwnershipDiagnosticCode::PossiblyMovedValue,
                    message: "value may have been moved".to_string(),
                    span,
                });
            }

            // Consumed + value uses → UseAfterMove
            (OwnershipState::Consumed, UseKind::Read)
            | (OwnershipState::Consumed, UseKind::Copy)
            | (OwnershipState::Consumed, UseKind::Move)
            | (OwnershipState::Consumed, UseKind::Consume) => {
                self.diagnostics.push(OwnershipDiagnostic {
                    code: OwnershipDiagnosticCode::UseAfterMove,
                    message: "use of consumed value".to_string(),
                    span,
                });
            }

            _ => {}
        }
    }

    pub fn has_diagnostics(&self) -> bool {
        !self.diagnostics.is_empty()
    }

    pub fn diagnostics(&self) -> &[OwnershipDiagnostic] {
        &self.diagnostics
    }

    pub fn place_states(&self) -> &HashMap<Place, OwnershipState> {
        &self.place_states
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexa_source::SourceId;

    fn span() -> SourceSpan {
        SourceSpan::point(SourceId(0), 0)
    }

    #[test]
    fn place_local_creation() {
        let p = Place::local(SymbolId(1));
        assert!(matches!(p, Place::Local(_)));
    }

    #[test]
    fn place_field_creation() {
        let p = Place::field(Place::local(SymbolId(0)), SymbolId(1));
        assert!(matches!(p, Place::Field(_, _)));
    }

    #[test]
    fn place_index_creation() {
        let p = Place::index(Place::local(SymbolId(0)), ExprRef(42));
        assert!(matches!(p, Place::Index(_, _)));
    }

    #[test]
    fn place_deref_creation() {
        let p = Place::deref(Place::local(SymbolId(0)));
        assert!(matches!(p, Place::Deref(_)));
    }

    #[test]
    fn place_root_local() {
        let p = Place::local(SymbolId(5));
        assert_eq!(p.root(), SymbolId(5));
    }

    #[test]
    fn place_root_nested() {
        let p = Place::deref(Place::field(Place::local(SymbolId(7)), SymbolId(8)));
        assert_eq!(p.root(), SymbolId(7));
    }

    #[test]
    fn ownership_state_default_available() {
        let analyzer = OwnershipAnalyzer::new();
        let place = Place::local(SymbolId(0));
        assert_eq!(analyzer.get_state(&place), OwnershipState::Available);
    }

    #[test]
    fn use_kind_read_exists() {
        assert_eq!(UseKind::Read, UseKind::Read);
    }

    #[test]
    fn use_kind_copy_exists() {
        assert_eq!(UseKind::Copy, UseKind::Copy);
    }

    #[test]
    fn type_properties_copy_type() {
        let props = TypeProperties::copy_type();
        assert!(props.is_copy);
        assert!(!props.needs_cleanup);
    }

    #[test]
    fn type_properties_move_type() {
        let props = TypeProperties::move_type();
        assert!(!props.is_copy);
        assert!(props.is_clone);
    }

    #[test]
    fn type_properties_resource_type() {
        let props = TypeProperties::resource_type();
        assert!(props.is_resource);
        assert!(props.needs_cleanup);
        assert!(!props.is_copy);
    }

    #[test]
    fn validate_resource_copy_rejected() {
        let props = TypeProperties {
            is_copy: true,
            is_clone: false,
            is_resource: true,
            needs_cleanup: true,
            is_send: false,
            is_share: false,
        };
        let errors = validate_type_properties(&props);
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn validate_valid_combo() {
        let errors = validate_type_properties(&TypeProperties::copy_type());
        assert!(errors.is_empty());
    }

    #[test]
    fn analyzer_move_transitions() {
        let mut analyzer = OwnershipAnalyzer::new();
        let place = Place::local(SymbolId(0));
        analyzer.record_use(place.clone(), UseKind::Move, span());
        assert_eq!(analyzer.get_state(&place), OwnershipState::Moved);
    }

    #[test]
    fn analyzer_copy_stays_available() {
        let mut analyzer = OwnershipAnalyzer::new();
        let place = Place::local(SymbolId(0));
        analyzer.record_use(place.clone(), UseKind::Copy, span());
        assert_eq!(analyzer.get_state(&place), OwnershipState::Available);
        assert!(!analyzer.has_diagnostics());
    }

    #[test]
    fn analyzer_read_stays_available() {
        let mut analyzer = OwnershipAnalyzer::new();
        let place = Place::local(SymbolId(0));
        analyzer.record_use(place.clone(), UseKind::Read, span());
        assert_eq!(analyzer.get_state(&place), OwnershipState::Available);
        assert!(!analyzer.has_diagnostics());
    }

    #[test]
    fn analyzer_borrow_does_not_consume() {
        let mut analyzer = OwnershipAnalyzer::new();
        let place = Place::local(SymbolId(0));
        analyzer.record_use(place.clone(), UseKind::BorrowShared, span());
        assert_eq!(analyzer.get_state(&place), OwnershipState::Available);
        assert!(!analyzer.has_diagnostics());
    }

    #[test]
    fn analyzer_use_after_move_detected() {
        let mut analyzer = OwnershipAnalyzer::new();
        let place = Place::local(SymbolId(0));
        analyzer.record_use(place.clone(), UseKind::Move, span());
        analyzer.record_use(place.clone(), UseKind::Read, span());
        assert!(analyzer.has_diagnostics());
        assert_eq!(
            analyzer.diagnostics()[0].code,
            OwnershipDiagnosticCode::UseAfterMove
        );
    }

    #[test]
    fn diagnostic_code_as_str() {
        assert_eq!(
            OwnershipDiagnosticCode::UseAfterMove.as_str(),
            "NEXA-OWN-0001"
        );
        assert_eq!(
            OwnershipDiagnosticCode::DoubleMove.as_str(),
            "NEXA-OWN-0002"
        );
        assert_eq!(
            OwnershipDiagnosticCode::PossiblyMovedValue.as_str(),
            "NEXA-OWN-0003"
        );
        assert_eq!(
            OwnershipDiagnosticCode::MoveWhileBorrowed.as_str(),
            "NEXA-OWN-0004"
        );
        assert_eq!(
            OwnershipDiagnosticCode::InvalidPartialMove.as_str(),
            "NEXA-OWN-0005"
        );
        assert_eq!(
            OwnershipDiagnosticCode::UseOfPartiallyMovedValue.as_str(),
            "NEXA-OWN-0006"
        );
        assert_eq!(
            OwnershipDiagnosticCode::CannotCopyMoveOnlyType.as_str(),
            "NEXA-OWN-0007"
        );
        assert_eq!(
            OwnershipDiagnosticCode::InvalidClone.as_str(),
            "NEXA-OWN-0008"
        );
        assert_eq!(
            OwnershipDiagnosticCode::InvalidConsume.as_str(),
            "NEXA-OWN-0009"
        );
        assert_eq!(
            OwnershipDiagnosticCode::MoveFromBorrowedValue.as_str(),
            "NEXA-OWN-0010"
        );
        assert_eq!(
            OwnershipDiagnosticCode::CannotMoveOutOfSharedReference.as_str(),
            "NEXA-OWN-0011"
        );
        assert_eq!(
            OwnershipDiagnosticCode::CannotMoveOutOfIndexedPlace.as_str(),
            "NEXA-OWN-0012"
        );
        assert_eq!(
            OwnershipDiagnosticCode::SelfMoveAssignment.as_str(),
            "NEXA-OWN-0013"
        );
    }

    #[test]
    fn diagnostic_code_is_error() {
        let codes = [
            OwnershipDiagnosticCode::UseAfterMove,
            OwnershipDiagnosticCode::DoubleMove,
            OwnershipDiagnosticCode::PossiblyMovedValue,
            OwnershipDiagnosticCode::MoveWhileBorrowed,
            OwnershipDiagnosticCode::InvalidPartialMove,
            OwnershipDiagnosticCode::UseOfPartiallyMovedValue,
            OwnershipDiagnosticCode::CannotCopyMoveOnlyType,
            OwnershipDiagnosticCode::InvalidClone,
            OwnershipDiagnosticCode::InvalidConsume,
            OwnershipDiagnosticCode::MoveFromBorrowedValue,
            OwnershipDiagnosticCode::CannotMoveOutOfSharedReference,
            OwnershipDiagnosticCode::CannotMoveOutOfIndexedPlace,
            OwnershipDiagnosticCode::SelfMoveAssignment,
        ];
        for code in codes {
            assert!(code.is_error());
        }
    }
}
