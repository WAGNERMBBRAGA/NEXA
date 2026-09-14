use nexa_ownership::Place;
use nexa_source::SourceSpan;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExprRef(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupKind {
    Drop,
    ResourceClose,
    CompositeCleanup,
}

#[derive(Debug, Clone)]
pub struct CleanupAction {
    pub place: Place,
    pub kind: CleanupKind,
    pub order_index: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeExitKind {
    Return,
    TryPropagation,
    Break,
    Continue,
    NormalScopeEnd,
    PanicExit,
    LanguageTrapExit,
    Cancellation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceObligationStatus {
    Active,
    Transferred,
    Consumed,
    Cleaned,
}

#[derive(Debug, Clone)]
pub struct ResourceObligation {
    pub place: Place,
    pub status: ResourceObligationStatus,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct CleanupPlan {
    pub actions: Vec<CleanupAction>,
    pub exit_kind: ScopeExitKind,
}

impl CleanupPlan {
    pub fn new(exit_kind: ScopeExitKind) -> Self {
        Self {
            actions: Vec::new(),
            exit_kind,
        }
    }

    pub fn add_action(&mut self, action: CleanupAction) {
        self.actions.push(action);
    }

    pub fn actions(&self) -> &[CleanupAction] {
        &self.actions
    }

    pub fn ordered_actions(&self) -> Vec<&CleanupAction> {
        let mut refs: Vec<&CleanupAction> = self.actions.iter().collect();
        refs.sort_by_key(|a| std::cmp::Reverse(a.order_index));
        refs
    }
}

pub struct ResourceAnalyzer {
    obligations: Vec<ResourceObligation>,
    #[allow(dead_code)]
    cleanup_plans: Vec<CleanupPlan>,
    diagnostics: Vec<ResourceDiagnostic>,
    next_cleanup_id: u32,
}

impl Default for ResourceAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl ResourceAnalyzer {
    pub fn new() -> Self {
        Self {
            obligations: Vec::new(),
            cleanup_plans: Vec::new(),
            diagnostics: Vec::new(),
            next_cleanup_id: 0,
        }
    }

    pub fn register_obligation(&mut self, place: Place, span: SourceSpan) {
        self.obligations.push(ResourceObligation {
            place,
            status: ResourceObligationStatus::Active,
            span,
        });
        self.next_cleanup_id += 1;
    }

    pub fn transfer_obligation(&mut self, from: &Place, to: &Place, span: SourceSpan) {
        if let Some(obligation) = self
            .obligations
            .iter_mut()
            .find(|o| o.place == *from && o.status == ResourceObligationStatus::Active)
        {
            obligation.status = ResourceObligationStatus::Transferred;
        }
        self.register_obligation(to.clone(), span);
    }

    pub fn consume_obligation(&mut self, place: &Place, _span: SourceSpan) {
        if let Some(obligation) = self
            .obligations
            .iter_mut()
            .find(|o| o.place == *place && o.status == ResourceObligationStatus::Active)
        {
            obligation.status = ResourceObligationStatus::Consumed;
        }
    }

    pub fn build_cleanup_plan(&self, exit_kind: ScopeExitKind, _scope_depth: u32) -> CleanupPlan {
        let mut plan = CleanupPlan::new(exit_kind);
        for (i, obligation) in self.obligations.iter().enumerate() {
            if obligation.status == ResourceObligationStatus::Active {
                plan.add_action(CleanupAction {
                    place: obligation.place.clone(),
                    kind: CleanupKind::ResourceClose,
                    order_index: i as u32,
                });
            }
        }
        plan.actions
            .sort_by_key(|a| std::cmp::Reverse(a.order_index));
        plan
    }

    pub fn validate_scope_exit(&mut self, exit_kind: ScopeExitKind, span: SourceSpan) {
        let _ = exit_kind;
        for obligation in &self.obligations {
            if obligation.status == ResourceObligationStatus::Active {
                self.diagnostics.push(ResourceDiagnostic {
                    code: ResourceDiagnosticCode::ResourceNotClosedBeforeScopeExit,
                    message: format!(
                        "resource at {:?} has not been closed before scope exit",
                        obligation.place
                    ),
                    span,
                });
            }
        }
    }

    pub fn obligations(&self) -> &[ResourceObligation] {
        &self.obligations
    }

    pub fn has_diagnostics(&self) -> bool {
        !self.diagnostics.is_empty()
    }

    pub fn diagnostics(&self) -> &[ResourceDiagnostic] {
        &self.diagnostics
    }

    pub fn build_return_cleanup(&self) -> CleanupPlan {
        self.build_cleanup_plan(ScopeExitKind::Return, 0)
    }

    pub fn build_try_cleanup(&self) -> CleanupPlan {
        self.build_cleanup_plan(ScopeExitKind::TryPropagation, 0)
    }

    pub fn build_break_cleanup(&self) -> CleanupPlan {
        self.build_cleanup_plan(ScopeExitKind::Break, 0)
    }
}

#[derive(Debug, Clone)]
pub struct ResourceDiagnostic {
    pub code: ResourceDiagnosticCode,
    pub message: String,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceDiagnosticCode {
    ResourceLeak,
    DoubleCleanup,
    CleanupAfterConsume,
    ResourceNotClosedBeforeScopeExit,
    InvalidResourceTransfer,
    CleanupFailureInTry,
    ResourceInSharedReference,
    DetachedResourceObligation,
}

impl ResourceDiagnosticCode {
    pub fn as_str(&self) -> &str {
        match self {
            ResourceDiagnosticCode::ResourceLeak => "NEXA-RES-0001",
            ResourceDiagnosticCode::DoubleCleanup => "NEXA-RES-0002",
            ResourceDiagnosticCode::CleanupAfterConsume => "NEXA-RES-0003",
            ResourceDiagnosticCode::ResourceNotClosedBeforeScopeExit => "NEXA-RES-0004",
            ResourceDiagnosticCode::InvalidResourceTransfer => "NEXA-RES-0005",
            ResourceDiagnosticCode::CleanupFailureInTry => "NEXA-RES-0006",
            ResourceDiagnosticCode::ResourceInSharedReference => "NEXA-RES-0007",
            ResourceDiagnosticCode::DetachedResourceObligation => "NEXA-RES-0008",
        }
    }

    pub fn is_error(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexa_source::SourceId;
    use nexa_symbols::SymbolId;

    fn span() -> SourceSpan {
        SourceSpan::point(SourceId(0), 0)
    }

    #[test]
    fn cleanup_kind_variants_exist() {
        let _drop = CleanupKind::Drop;
        let _close = CleanupKind::ResourceClose;
        let _composite = CleanupKind::CompositeCleanup;
    }

    #[test]
    fn cleanup_action_creation_and_ordering() {
        let place = Place::local(SymbolId(1));
        let action = CleanupAction {
            place: place.clone(),
            kind: CleanupKind::ResourceClose,
            order_index: 2,
        };
        assert_eq!(action.place, place);
        assert_eq!(action.kind, CleanupKind::ResourceClose);
        assert_eq!(action.order_index, 2);
    }

    #[test]
    fn scope_exit_kind_variants_exist() {
        let _ = ScopeExitKind::Return;
        let _ = ScopeExitKind::TryPropagation;
        let _ = ScopeExitKind::Break;
        let _ = ScopeExitKind::Continue;
        let _ = ScopeExitKind::NormalScopeEnd;
        let _ = ScopeExitKind::PanicExit;
        let _ = ScopeExitKind::LanguageTrapExit;
        let _ = ScopeExitKind::Cancellation;
    }

    #[test]
    fn resource_obligation_status_variants_exist() {
        let _ = ResourceObligationStatus::Active;
        let _ = ResourceObligationStatus::Transferred;
        let _ = ResourceObligationStatus::Consumed;
        let _ = ResourceObligationStatus::Cleaned;
    }

    #[test]
    fn analyzer_new_has_no_obligations() {
        let analyzer = ResourceAnalyzer::new();
        assert!(analyzer.obligations().is_empty());
    }

    #[test]
    fn analyzer_register_obligation_tracked() {
        let mut analyzer = ResourceAnalyzer::new();
        let place = Place::local(SymbolId(1));
        analyzer.register_obligation(place.clone(), span());
        assert_eq!(analyzer.obligations().len(), 1);
        assert_eq!(analyzer.obligations()[0].place, place);
        assert_eq!(
            analyzer.obligations()[0].status,
            ResourceObligationStatus::Active
        );
    }

    #[test]
    fn analyzer_transfer_obligation_changes_owner() {
        let mut analyzer = ResourceAnalyzer::new();
        let from = Place::local(SymbolId(1));
        let to = Place::local(SymbolId(2));
        analyzer.register_obligation(from.clone(), span());
        analyzer.transfer_obligation(&from, &to, span());
        assert_eq!(analyzer.obligations().len(), 2);
        assert_eq!(
            analyzer.obligations()[0].status,
            ResourceObligationStatus::Transferred
        );
        assert_eq!(
            analyzer.obligations()[1].status,
            ResourceObligationStatus::Active
        );
        assert_eq!(analyzer.obligations()[1].place, to);
    }

    #[test]
    fn analyzer_consume_obligation_marks_consumed() {
        let mut analyzer = ResourceAnalyzer::new();
        let place = Place::local(SymbolId(1));
        analyzer.register_obligation(place.clone(), span());
        analyzer.consume_obligation(&place, span());
        assert_eq!(
            analyzer.obligations()[0].status,
            ResourceObligationStatus::Consumed
        );
    }

    #[test]
    fn analyzer_build_return_cleanup_creates_plan() {
        let mut analyzer = ResourceAnalyzer::new();
        let place = Place::local(SymbolId(1));
        analyzer.register_obligation(place, span());
        let plan = analyzer.build_return_cleanup();
        assert_eq!(plan.exit_kind, ScopeExitKind::Return);
        assert_eq!(plan.actions().len(), 1);
    }

    #[test]
    fn cleanup_plan_ordered_by_reverse_declaration() {
        let mut analyzer = ResourceAnalyzer::new();
        analyzer.register_obligation(Place::local(SymbolId(1)), span());
        analyzer.register_obligation(Place::local(SymbolId(2)), span());
        analyzer.register_obligation(Place::local(SymbolId(3)), span());
        let plan = analyzer.build_return_cleanup();
        let ordered = plan.ordered_actions();
        assert_eq!(ordered.len(), 3);
        assert_eq!(ordered[0].order_index, 2);
        assert_eq!(ordered[1].order_index, 1);
        assert_eq!(ordered[2].order_index, 0);
    }

    #[test]
    fn consumed_obligations_not_cleaned_again() {
        let mut analyzer = ResourceAnalyzer::new();
        let place = Place::local(SymbolId(1));
        analyzer.register_obligation(place.clone(), span());
        analyzer.consume_obligation(&place, span());
        let plan = analyzer.build_return_cleanup();
        assert!(plan.actions().is_empty());
    }

    #[test]
    fn validate_scope_exit_with_no_obligations_is_ok() {
        let mut analyzer = ResourceAnalyzer::new();
        analyzer.validate_scope_exit(ScopeExitKind::Return, span());
        assert!(!analyzer.has_diagnostics());
    }

    #[test]
    fn validate_scope_exit_with_active_obligation_is_error() {
        let mut analyzer = ResourceAnalyzer::new();
        analyzer.register_obligation(Place::local(SymbolId(1)), span());
        analyzer.validate_scope_exit(ScopeExitKind::Return, span());
        assert!(analyzer.has_diagnostics());
        assert_eq!(
            analyzer.diagnostics()[0].code,
            ResourceDiagnosticCode::ResourceNotClosedBeforeScopeExit
        );
    }

    #[test]
    fn cleanup_plan_new_is_empty() {
        let plan = CleanupPlan::new(ScopeExitKind::Return);
        assert!(plan.actions().is_empty());
        assert_eq!(plan.exit_kind, ScopeExitKind::Return);
    }

    #[test]
    fn diagnostic_code_all_as_str_correct() {
        assert_eq!(
            ResourceDiagnosticCode::ResourceLeak.as_str(),
            "NEXA-RES-0001"
        );
        assert_eq!(
            ResourceDiagnosticCode::DoubleCleanup.as_str(),
            "NEXA-RES-0002"
        );
        assert_eq!(
            ResourceDiagnosticCode::CleanupAfterConsume.as_str(),
            "NEXA-RES-0003"
        );
        assert_eq!(
            ResourceDiagnosticCode::ResourceNotClosedBeforeScopeExit.as_str(),
            "NEXA-RES-0004"
        );
        assert_eq!(
            ResourceDiagnosticCode::InvalidResourceTransfer.as_str(),
            "NEXA-RES-0005"
        );
        assert_eq!(
            ResourceDiagnosticCode::CleanupFailureInTry.as_str(),
            "NEXA-RES-0006"
        );
        assert_eq!(
            ResourceDiagnosticCode::ResourceInSharedReference.as_str(),
            "NEXA-RES-0007"
        );
        assert_eq!(
            ResourceDiagnosticCode::DetachedResourceObligation.as_str(),
            "NEXA-RES-0008"
        );
    }
}
