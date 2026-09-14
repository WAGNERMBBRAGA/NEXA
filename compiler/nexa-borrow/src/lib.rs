use nexa_ownership::{Place, UseKind};
use nexa_source::SourceSpan;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BorrowId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorrowKind {
    Shared,
    Mutable,
}

#[derive(Debug, Clone)]
pub struct BorrowRegion {
    pub creation_point: u32,
    pub last_use_point: u32,
}

#[derive(Debug, Clone)]
pub struct Borrow {
    pub id: BorrowId,
    pub kind: BorrowKind,
    pub place: Place,
    pub origin: SourceSpan,
    pub region: BorrowRegion,
}

pub struct BorrowChecker {
    active_borrows: Vec<Borrow>,
    next_id: u32,
    diagnostics: Vec<BorrowDiagnostic>,
}

impl Default for BorrowChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl BorrowChecker {
    pub fn new() -> Self {
        Self {
            active_borrows: Vec::new(),
            next_id: 0,
            diagnostics: Vec::new(),
        }
    }

    pub fn create_borrow(
        &mut self,
        kind: BorrowKind,
        place: Place,
        span: SourceSpan,
        creation_point: u32,
    ) -> BorrowId {
        let id = BorrowId(self.next_id);
        self.next_id += 1;

        for existing in &self.active_borrows {
            if !places_overlap(&existing.place, &place) {
                continue;
            }

            match (&existing.kind, &kind) {
                (BorrowKind::Mutable, BorrowKind::Mutable) => {
                    self.diagnostics.push(BorrowDiagnostic {
                        code: BorrowDiagnosticCode::MultipleMutableBorrows,
                        message: "cannot create second mutable borrow while first is active"
                            .to_string(),
                        span,
                    });
                }
                (BorrowKind::Shared, BorrowKind::Mutable)
                | (BorrowKind::Mutable, BorrowKind::Shared) => {
                    self.diagnostics.push(BorrowDiagnostic {
                        code: BorrowDiagnosticCode::ConflictingSharedAndMutableBorrow,
                        message: "cannot create mutable borrow while it is already borrowed"
                            .to_string(),
                        span,
                    });
                }
                _ => {}
            }
        }

        let borrow = Borrow {
            id,
            kind,
            place,
            origin: span,
            region: BorrowRegion {
                creation_point,
                last_use_point: creation_point,
            },
        };
        self.active_borrows.push(borrow);
        id
    }

    pub fn end_borrow(&mut self, id: BorrowId, last_use_point: u32) {
        if let Some(borrow) = self.active_borrows.iter_mut().find(|b| b.id == id) {
            borrow.region.last_use_point = last_use_point;
        }
        self.active_borrows.retain(|b| b.id != id);
    }

    pub fn check_use(&mut self, place: &Place, kind: UseKind, span: SourceSpan) {
        for existing in &self.active_borrows {
            if !places_overlap(&existing.place, place) {
                continue;
            }

            match (&existing.kind, kind) {
                (BorrowKind::Mutable, _) => {
                    self.diagnostics.push(BorrowDiagnostic {
                        code: BorrowDiagnosticCode::MutationWhileSharedBorrowed,
                        message: "cannot use while it is mutably borrowed".to_string(),
                        span,
                    });
                }
                (BorrowKind::Shared, UseKind::Mutate) => {
                    self.diagnostics.push(BorrowDiagnostic {
                        code: BorrowDiagnosticCode::MutationWhileSharedBorrowed,
                        message: "cannot mutate while it is shared borrowed".to_string(),
                        span,
                    });
                }
                (BorrowKind::Shared, UseKind::Move) => {
                    self.diagnostics.push(BorrowDiagnostic {
                        code: BorrowDiagnosticCode::MoveWhileBorrowed,
                        message: "cannot move while it is borrowed".to_string(),
                        span,
                    });
                }
                _ => {}
            }
        }
    }

    pub fn active_borrows(&self) -> &[Borrow] {
        &self.active_borrows
    }

    pub fn has_diagnostics(&self) -> bool {
        !self.diagnostics.is_empty()
    }

    pub fn diagnostics(&self) -> &[BorrowDiagnostic] {
        &self.diagnostics
    }

    pub fn is_place_borrowed(&self, place: &Place) -> bool {
        self.active_borrows
            .iter()
            .any(|b| places_overlap(&b.place, place))
    }

    pub fn has_active_borrows(&self, place: &Place) -> bool {
        self.is_place_borrowed(place)
    }

    pub fn expire_unused_borrows(&mut self, _available_point: u32) {}
}

pub fn places_overlap(a: &Place, b: &Place) -> bool {
    let root_a = a.root();
    let root_b = b.root();
    if root_a != root_b {
        return false;
    }
    // Same root: collect projection chains and compare
    let chain_a = projection_chain(a);
    let chain_b = projection_chain(b);

    // One is a prefix of the other (or equal)
    let min_len = chain_a.len().min(chain_b.len());
    for i in 0..min_len {
        if chain_a[i] != chain_b[i] {
            // Diverged at field level
            if matches!(
                (&chain_a[i], &chain_b[i]),
                (ProjKind::Field(_), ProjKind::Field(_))
            ) {
                return false; // disjoint fields
            }
            // Index vs anything → conservative overlap
            return true;
        }
    }
    // One is a prefix of the other (e.g., x vs x.f, or x.f == x.f)
    true
}

#[derive(PartialEq, Eq)]
enum ProjKind {
    Field(nexa_symbols::SymbolId),
    Index,
    Deref,
}

fn projection_chain(place: &Place) -> Vec<ProjKind> {
    let mut chain = Vec::new();
    let mut current = place;
    loop {
        match current {
            Place::Local(_) => break,
            Place::Field(base, sym) => {
                chain.push(ProjKind::Field(*sym));
                current = base;
            }
            Place::Index(base, _) => {
                chain.push(ProjKind::Index);
                current = base;
            }
            Place::Deref(base) => {
                chain.push(ProjKind::Deref);
                current = base;
            }
        }
    }
    chain.reverse();
    chain
}

pub fn borrow_escapes_boundary(borrow: &Borrow, boundary_point: u32) -> bool {
    borrow.region.last_use_point > boundary_point
}

#[derive(Debug, Clone)]
pub struct BorrowDiagnostic {
    pub code: BorrowDiagnosticCode,
    pub message: String,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorrowDiagnosticCode {
    ConflictingSharedAndMutableBorrow,
    MultipleMutableBorrows,
    MutationWhileSharedBorrowed,
    MoveWhileBorrowed,
    BorrowEscapesOwner,
    BorrowOutlivesOwner,
    InvalidBorrowedReturn,
    BorrowAcrossAwait,
    MutableBorrowAcrossAwait,
    BorrowStoredInOwnedAggregate,
    InvalidBorrowOfTemporary,
    DanglingReference,
    MutableBorrowOfImmutablePlace,
}

impl BorrowDiagnosticCode {
    pub fn as_str(&self) -> &str {
        match self {
            Self::ConflictingSharedAndMutableBorrow => "NEXA-BORROW-0001",
            Self::MultipleMutableBorrows => "NEXA-BORROW-0002",
            Self::MutationWhileSharedBorrowed => "NEXA-BORROW-0003",
            Self::MoveWhileBorrowed => "NEXA-BORROW-0004",
            Self::BorrowEscapesOwner => "NEXA-BORROW-0005",
            Self::BorrowOutlivesOwner => "NEXA-BORROW-0006",
            Self::InvalidBorrowedReturn => "NEXA-BORROW-0007",
            Self::BorrowAcrossAwait => "NEXA-BORROW-0008",
            Self::MutableBorrowAcrossAwait => "NEXA-BORROW-0009",
            Self::BorrowStoredInOwnedAggregate => "NEXA-BORROW-0010",
            Self::InvalidBorrowOfTemporary => "NEXA-BORROW-0011",
            Self::DanglingReference => "NEXA-BORROW-0012",
            Self::MutableBorrowOfImmutablePlace => "NEXA-BORROW-0013",
        }
    }

    pub fn is_error(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexa_ownership::{ExprRef, Place};
    use nexa_source::SourceId;
    use nexa_symbols::SymbolId;

    fn test_span() -> SourceSpan {
        SourceSpan::point(SourceId(0), 0)
    }

    fn local(id: u32) -> Place {
        Place::local(SymbolId(id))
    }

    fn field(base: Place, field_id: u32) -> Place {
        Place::field(base, SymbolId(field_id))
    }

    fn index(base: Place, idx: u32) -> Place {
        Place::index(base, ExprRef(idx))
    }

    #[test]
    fn borrow_id_creation_and_equality() {
        let a = BorrowId(0);
        let b = BorrowId(0);
        let c = BorrowId(1);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn borrow_kind_variants() {
        assert_ne!(BorrowKind::Shared, BorrowKind::Mutable);
        assert_eq!(BorrowKind::Shared, BorrowKind::Shared);
        assert_eq!(BorrowKind::Mutable, BorrowKind::Mutable);
    }

    #[test]
    fn borrow_region_creation() {
        let region = BorrowRegion {
            creation_point: 10,
            last_use_point: 50,
        };
        assert_eq!(region.creation_point, 10);
        assert_eq!(region.last_use_point, 50);
    }

    #[test]
    fn borrow_creation() {
        let mut checker = BorrowChecker::new();
        let id = checker.create_borrow(BorrowKind::Shared, local(0), test_span(), 1);
        assert_eq!(id, BorrowId(0));
        assert_eq!(checker.active_borrows().len(), 1);
        assert_eq!(checker.active_borrows()[0].kind, BorrowKind::Shared);
    }

    #[test]
    fn checker_new_has_no_active_borrows() {
        let checker = BorrowChecker::new();
        assert!(checker.active_borrows().is_empty());
        assert!(!checker.has_diagnostics());
    }

    #[test]
    fn checker_create_shared_borrow() {
        let mut checker = BorrowChecker::new();
        checker.create_borrow(BorrowKind::Shared, local(0), test_span(), 0);
        assert_eq!(checker.active_borrows().len(), 1);
        assert!(!checker.has_diagnostics());
    }

    #[test]
    fn checker_create_mutable_borrow() {
        let mut checker = BorrowChecker::new();
        checker.create_borrow(BorrowKind::Mutable, local(0), test_span(), 0);
        assert_eq!(checker.active_borrows().len(), 1);
        assert!(!checker.has_diagnostics());
    }

    #[test]
    fn shared_plus_shared_allowed() {
        let mut checker = BorrowChecker::new();
        checker.create_borrow(BorrowKind::Shared, local(0), test_span(), 0);
        checker.create_borrow(BorrowKind::Shared, local(0), test_span(), 1);
        assert!(!checker.has_diagnostics());
        assert_eq!(checker.active_borrows().len(), 2);
    }

    #[test]
    fn shared_plus_mutable_rejected() {
        let mut checker = BorrowChecker::new();
        checker.create_borrow(BorrowKind::Shared, local(0), test_span(), 0);
        checker.create_borrow(BorrowKind::Mutable, local(0), test_span(), 1);
        assert!(checker.has_diagnostics());
        assert_eq!(
            checker.diagnostics()[0].code,
            BorrowDiagnosticCode::ConflictingSharedAndMutableBorrow
        );
    }

    #[test]
    fn mutable_plus_mutable_rejected() {
        let mut checker = BorrowChecker::new();
        checker.create_borrow(BorrowKind::Mutable, local(0), test_span(), 0);
        checker.create_borrow(BorrowKind::Mutable, local(0), test_span(), 1);
        assert!(checker.has_diagnostics());
        assert_eq!(
            checker.diagnostics()[0].code,
            BorrowDiagnosticCode::MultipleMutableBorrows
        );
    }

    #[test]
    fn move_while_borrowed_rejected() {
        let mut checker = BorrowChecker::new();
        checker.create_borrow(BorrowKind::Shared, local(0), test_span(), 0);
        checker.check_use(&local(0), UseKind::Move, test_span());
        assert!(checker.has_diagnostics());
        assert_eq!(
            checker.diagnostics()[0].code,
            BorrowDiagnosticCode::MoveWhileBorrowed
        );
    }

    #[test]
    fn places_overlap_same_local() {
        assert!(places_overlap(&local(0), &local(0)));
    }

    #[test]
    fn places_overlap_different_local() {
        assert!(!places_overlap(&local(0), &local(1)));
    }

    #[test]
    fn places_overlap_parent_vs_child() {
        assert!(places_overlap(&local(0), &field(local(0), 1)));
    }

    #[test]
    fn places_overlap_disjoint_fields() {
        assert!(!places_overlap(&field(local(0), 1), &field(local(0), 2)));
    }

    #[test]
    fn places_overlap_same_field() {
        assert!(places_overlap(&field(local(0), 1), &field(local(0), 1)));
    }

    #[test]
    fn places_overlap_index_vs_field() {
        assert!(places_overlap(&index(local(0), 0), &field(local(0), 1)));
    }

    #[test]
    fn diagnostic_code_as_str() {
        assert_eq!(
            BorrowDiagnosticCode::ConflictingSharedAndMutableBorrow.as_str(),
            "NEXA-BORROW-0001"
        );
        assert_eq!(
            BorrowDiagnosticCode::MultipleMutableBorrows.as_str(),
            "NEXA-BORROW-0002"
        );
        assert_eq!(
            BorrowDiagnosticCode::MutationWhileSharedBorrowed.as_str(),
            "NEXA-BORROW-0003"
        );
        assert_eq!(
            BorrowDiagnosticCode::MoveWhileBorrowed.as_str(),
            "NEXA-BORROW-0004"
        );
        assert_eq!(
            BorrowDiagnosticCode::BorrowEscapesOwner.as_str(),
            "NEXA-BORROW-0005"
        );
        assert_eq!(
            BorrowDiagnosticCode::BorrowOutlivesOwner.as_str(),
            "NEXA-BORROW-0006"
        );
        assert_eq!(
            BorrowDiagnosticCode::InvalidBorrowedReturn.as_str(),
            "NEXA-BORROW-0007"
        );
        assert_eq!(
            BorrowDiagnosticCode::BorrowAcrossAwait.as_str(),
            "NEXA-BORROW-0008"
        );
        assert_eq!(
            BorrowDiagnosticCode::MutableBorrowAcrossAwait.as_str(),
            "NEXA-BORROW-0009"
        );
        assert_eq!(
            BorrowDiagnosticCode::BorrowStoredInOwnedAggregate.as_str(),
            "NEXA-BORROW-0010"
        );
        assert_eq!(
            BorrowDiagnosticCode::InvalidBorrowOfTemporary.as_str(),
            "NEXA-BORROW-0011"
        );
        assert_eq!(
            BorrowDiagnosticCode::DanglingReference.as_str(),
            "NEXA-BORROW-0012"
        );
        assert_eq!(
            BorrowDiagnosticCode::MutableBorrowOfImmutablePlace.as_str(),
            "NEXA-BORROW-0013"
        );
    }

    #[test]
    fn borrow_escapes_boundary_true() {
        let borrow = Borrow {
            id: BorrowId(0),
            kind: BorrowKind::Shared,
            place: local(0),
            origin: test_span(),
            region: BorrowRegion {
                creation_point: 5,
                last_use_point: 20,
            },
        };
        assert!(borrow_escapes_boundary(&borrow, 10));
    }

    #[test]
    fn borrow_escapes_boundary_false() {
        let borrow = Borrow {
            id: BorrowId(0),
            kind: BorrowKind::Shared,
            place: local(0),
            origin: test_span(),
            region: BorrowRegion {
                creation_point: 5,
                last_use_point: 10,
            },
        };
        assert!(!borrow_escapes_boundary(&borrow, 20));
    }

    #[test]
    fn end_borrow_removes_active() {
        let mut checker = BorrowChecker::new();
        let id = checker.create_borrow(BorrowKind::Shared, local(0), test_span(), 0);
        assert_eq!(checker.active_borrows().len(), 1);
        checker.end_borrow(id, 10);
        assert!(checker.active_borrows().is_empty());
    }

    #[test]
    fn is_place_borrowed() {
        let mut checker = BorrowChecker::new();
        assert!(!checker.is_place_borrowed(&local(0)));
        checker.create_borrow(BorrowKind::Shared, local(0), test_span(), 0);
        assert!(checker.is_place_borrowed(&local(0)));
        assert!(!checker.is_place_borrowed(&local(1)));
    }

    #[test]
    fn mutation_while_shared_borrowed() {
        let mut checker = BorrowChecker::new();
        checker.create_borrow(BorrowKind::Shared, local(0), test_span(), 0);
        checker.check_use(&local(0), UseKind::Mutate, test_span());
        assert!(checker.has_diagnostics());
        assert_eq!(
            checker.diagnostics()[0].code,
            BorrowDiagnosticCode::MutationWhileSharedBorrowed
        );
    }

    #[test]
    fn no_conflict_different_places() {
        let mut checker = BorrowChecker::new();
        checker.create_borrow(BorrowKind::Mutable, local(0), test_span(), 0);
        checker.create_borrow(BorrowKind::Mutable, local(1), test_span(), 1);
        assert!(!checker.has_diagnostics());
        assert_eq!(checker.active_borrows().len(), 2);
    }

    #[test]
    fn diagnostic_is_error() {
        assert!(BorrowDiagnosticCode::DanglingReference.is_error());
        assert!(BorrowDiagnosticCode::BorrowEscapesOwner.is_error());
    }

    #[test]
    fn shared_read_allowed() {
        let mut checker = BorrowChecker::new();
        checker.create_borrow(BorrowKind::Shared, local(0), test_span(), 0);
        checker.check_use(&local(0), UseKind::Read, test_span());
        assert!(!checker.has_diagnostics());
    }
}
