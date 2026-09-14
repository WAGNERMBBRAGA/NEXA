use nexa_source::SourceSpan;
use nexa_symbols::SymbolId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExprRef(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    Pending,
    Awaited,
    Moved,
    MaybeConsumed,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsyncKind {
    Sync,
    Async,
}

#[derive(Debug, Clone)]
pub struct TaskObligation {
    pub symbol: SymbolId,
    pub state: TaskState,
    pub span: SourceSpan,
    pub is_structured: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferProperties {
    pub is_send: bool,
    pub is_share: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MustUseKind {
    None,
    MustUse,
    MustConsume,
}

#[derive(Debug, Clone)]
pub struct TaskDiagnostic {
    pub code: TaskDiagnosticCode,
    pub message: String,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskDiagnosticCode {
    UnusedTask,
    TaskEscapesStructuredScope,
    TaskAlreadyAwaited,
    TaskPossiblyUnresolved,
    InvalidTaskMove,
    BorrowAcrossAwait,
    DetachedTaskNotAllowed,
    InvalidAwaitContext,
    TaskScopeViolation,
    NonSendTaskTransfer,
}

impl TaskDiagnosticCode {
    pub fn as_str(&self) -> &str {
        match self {
            TaskDiagnosticCode::UnusedTask => "NEXA-TASK-0001",
            TaskDiagnosticCode::TaskEscapesStructuredScope => "NEXA-TASK-0002",
            TaskDiagnosticCode::TaskAlreadyAwaited => "NEXA-TASK-0003",
            TaskDiagnosticCode::TaskPossiblyUnresolved => "NEXA-TASK-0004",
            TaskDiagnosticCode::InvalidTaskMove => "NEXA-TASK-0005",
            TaskDiagnosticCode::BorrowAcrossAwait => "NEXA-TASK-0006",
            TaskDiagnosticCode::DetachedTaskNotAllowed => "NEXA-TASK-0007",
            TaskDiagnosticCode::InvalidAwaitContext => "NEXA-TASK-0008",
            TaskDiagnosticCode::TaskScopeViolation => "NEXA-TASK-0009",
            TaskDiagnosticCode::NonSendTaskTransfer => "NEXA-TASK-0010",
        }
    }

    pub fn is_error(&self) -> bool {
        true
    }
}

pub struct TaskAnalyzer {
    obligations: Vec<TaskObligation>,
    must_use_obligations: Vec<(SymbolId, MustUseKind, SourceSpan)>,
    diagnostics: Vec<TaskDiagnostic>,
    current_context: AsyncKind,
}

impl Default for TaskAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskAnalyzer {
    pub fn new() -> Self {
        Self {
            obligations: Vec::new(),
            must_use_obligations: Vec::new(),
            diagnostics: Vec::new(),
            current_context: AsyncKind::Sync,
        }
    }

    pub fn set_context(&mut self, ctx: AsyncKind) {
        self.current_context = ctx;
    }

    pub fn register_task(&mut self, symbol: SymbolId, span: SourceSpan) -> TaskState {
        self.obligations.push(TaskObligation {
            symbol,
            state: TaskState::Pending,
            span,
            is_structured: true,
        });
        TaskState::Pending
    }

    pub fn await_task(&mut self, symbol: SymbolId, span: SourceSpan) -> TaskState {
        if let Some(obligation) = self.obligations.iter_mut().find(|o| o.symbol == symbol) {
            match obligation.state {
                TaskState::Pending => {
                    obligation.state = TaskState::Awaited;
                    TaskState::Awaited
                }
                TaskState::Awaited => {
                    self.diagnostics.push(TaskDiagnostic {
                        code: TaskDiagnosticCode::TaskAlreadyAwaited,
                        message: "task has already been awaited".to_string(),
                        span,
                    });
                    TaskState::Error
                }
                other => other,
            }
        } else {
            TaskState::Pending
        }
    }

    pub fn transfer_task(&mut self, symbol: SymbolId, span: SourceSpan) -> TaskState {
        if let Some(obligation) = self.obligations.iter_mut().find(|o| o.symbol == symbol) {
            match obligation.state {
                TaskState::Pending => {
                    obligation.state = TaskState::Moved;
                    TaskState::Moved
                }
                other => other,
            }
        } else {
            let _ = span;
            TaskState::Pending
        }
    }

    pub fn check_scope_exit(&mut self, span: SourceSpan) {
        for obligation in &self.obligations {
            if obligation.state == TaskState::Pending {
                self.diagnostics.push(TaskDiagnostic {
                    code: TaskDiagnosticCode::TaskPossiblyUnresolved,
                    message: "task may be unresolved at scope exit".to_string(),
                    span,
                });
            }
        }
    }

    pub fn register_must_use(&mut self, symbol: SymbolId, kind: MustUseKind, span: SourceSpan) {
        self.must_use_obligations.push((symbol, kind, span));
    }

    pub fn check_unused_must_use(&mut self, span: SourceSpan) {
        for &(symbol, kind, span) in &self.must_use_obligations {
            if kind != MustUseKind::None {
                let consumed = self.obligations.iter().any(|o| {
                    o.symbol == symbol
                        && (o.state == TaskState::Awaited || o.state == TaskState::Moved)
                });
                if !consumed {
                    self.diagnostics.push(TaskDiagnostic {
                        code: TaskDiagnosticCode::UnusedTask,
                        message: "unused task value".to_string(),
                        span,
                    });
                }
            }
        }
        let _ = span;
    }

    pub fn check_structured_scope(
        &mut self,
        symbol: SymbolId,
        is_child_scope: bool,
        span: SourceSpan,
    ) {
        if is_child_scope {
            if let Some(obligation) = self.obligations.iter().find(|o| o.symbol == symbol) {
                if obligation.state == TaskState::Pending && obligation.is_structured {
                    self.diagnostics.push(TaskDiagnostic {
                        code: TaskDiagnosticCode::TaskEscapesStructuredScope,
                        message: "task created in child scope escapes".to_string(),
                        span,
                    });
                }
            }
        }
    }

    pub fn check_await_context(&mut self, span: SourceSpan) {
        if self.current_context == AsyncKind::Sync {
            self.diagnostics.push(TaskDiagnostic {
                code: TaskDiagnosticCode::InvalidAwaitContext,
                message: "await used in synchronous context".to_string(),
                span,
            });
        }
    }

    pub fn check_send_across_await(&mut self, properties: TransferProperties, span: SourceSpan) {
        if !properties.is_send {
            self.diagnostics.push(TaskDiagnostic {
                code: TaskDiagnosticCode::NonSendTaskTransfer,
                message: "non-Send type crosses await boundary".to_string(),
                span,
            });
        }
    }

    pub fn check_borrow_across_await(&mut self, span: SourceSpan) {
        self.diagnostics.push(TaskDiagnostic {
            code: TaskDiagnosticCode::BorrowAcrossAwait,
            message: "borrow active across await point".to_string(),
            span,
        });
    }

    pub fn has_diagnostics(&self) -> bool {
        !self.diagnostics.is_empty()
    }

    pub fn diagnostics(&self) -> &[TaskDiagnostic] {
        &self.diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span() -> SourceSpan {
        SourceSpan::point(nexa_source::SourceId(0), 0)
    }

    #[test]
    fn task_state_variants_exist() {
        let _ = TaskState::Pending;
        let _ = TaskState::Awaited;
        let _ = TaskState::Moved;
        let _ = TaskState::MaybeConsumed;
        let _ = TaskState::Error;
    }

    #[test]
    fn async_kind_variants_exist() {
        let _ = AsyncKind::Sync;
        let _ = AsyncKind::Async;
    }

    #[test]
    fn must_use_kind_variants_exist() {
        let _ = MustUseKind::None;
        let _ = MustUseKind::MustUse;
        let _ = MustUseKind::MustConsume;
    }

    #[test]
    fn transfer_properties_creation() {
        let tp = TransferProperties {
            is_send: true,
            is_share: false,
        };
        assert!(tp.is_send);
        assert!(!tp.is_share);
    }

    #[test]
    fn task_obligation_creation() {
        let o = TaskObligation {
            symbol: SymbolId(1),
            state: TaskState::Pending,
            span: span(),
            is_structured: true,
        };
        assert_eq!(o.symbol, SymbolId(1));
        assert_eq!(o.state, TaskState::Pending);
    }

    #[test]
    fn analyzer_new_has_no_obligations() {
        let analyzer = TaskAnalyzer::new();
        assert!(analyzer.obligations.is_empty());
        assert!(analyzer.diagnostics.is_empty());
    }

    #[test]
    fn analyzer_register_task_creates_pending() {
        let mut analyzer = TaskAnalyzer::new();
        let s = span();
        let state = analyzer.register_task(SymbolId(1), s);
        assert_eq!(state, TaskState::Pending);
        assert_eq!(analyzer.obligations.len(), 1);
        assert_eq!(analyzer.obligations[0].state, TaskState::Pending);
    }

    #[test]
    fn analyzer_await_task_transitions_pending_to_awaited() {
        let mut analyzer = TaskAnalyzer::new();
        let s = span();
        analyzer.register_task(SymbolId(1), s);
        let state = analyzer.await_task(SymbolId(1), s);
        assert_eq!(state, TaskState::Awaited);
    }

    #[test]
    fn analyzer_double_await_rejected() {
        let mut analyzer = TaskAnalyzer::new();
        let s = span();
        analyzer.register_task(SymbolId(1), s);
        analyzer.await_task(SymbolId(1), s);
        let state = analyzer.await_task(SymbolId(1), s);
        assert_eq!(state, TaskState::Error);
        assert!(analyzer.has_diagnostics());
        assert_eq!(
            analyzer.diagnostics()[0].code,
            TaskDiagnosticCode::TaskAlreadyAwaited
        );
    }

    #[test]
    fn analyzer_transfer_task_transitions_pending_to_moved() {
        let mut analyzer = TaskAnalyzer::new();
        let s = span();
        analyzer.register_task(SymbolId(1), s);
        let state = analyzer.transfer_task(SymbolId(1), s);
        assert_eq!(state, TaskState::Moved);
    }

    #[test]
    fn analyzer_check_scope_exit_with_pending_errors() {
        let mut analyzer = TaskAnalyzer::new();
        let s = span();
        analyzer.register_task(SymbolId(1), s);
        analyzer.check_scope_exit(s);
        assert!(analyzer.has_diagnostics());
        assert_eq!(
            analyzer.diagnostics()[0].code,
            TaskDiagnosticCode::TaskPossiblyUnresolved
        );
    }

    #[test]
    fn analyzer_check_scope_exit_with_no_pending_ok() {
        let mut analyzer = TaskAnalyzer::new();
        let s = span();
        analyzer.register_task(SymbolId(1), s);
        analyzer.await_task(SymbolId(1), s);
        analyzer.check_scope_exit(s);
        assert!(!analyzer.has_diagnostics());
    }

    #[test]
    fn analyzer_check_unused_must_use_detects_unused() {
        let mut analyzer = TaskAnalyzer::new();
        let s = span();
        analyzer.register_must_use(SymbolId(1), MustUseKind::MustUse, s);
        analyzer.check_unused_must_use(s);
        assert!(analyzer.has_diagnostics());
        assert_eq!(
            analyzer.diagnostics()[0].code,
            TaskDiagnosticCode::UnusedTask
        );
    }

    #[test]
    fn analyzer_check_structured_scope_detects_escape() {
        let mut analyzer = TaskAnalyzer::new();
        let s = span();
        analyzer.register_task(SymbolId(1), s);
        analyzer.check_structured_scope(SymbolId(1), true, s);
        assert!(analyzer.has_diagnostics());
        assert_eq!(
            analyzer.diagnostics()[0].code,
            TaskDiagnosticCode::TaskEscapesStructuredScope
        );
    }

    #[test]
    fn analyzer_check_await_context_in_sync_errors() {
        let mut analyzer = TaskAnalyzer::new();
        let s = span();
        assert_eq!(analyzer.current_context, AsyncKind::Sync);
        analyzer.check_await_context(s);
        assert!(analyzer.has_diagnostics());
        assert_eq!(
            analyzer.diagnostics()[0].code,
            TaskDiagnosticCode::InvalidAwaitContext
        );
    }

    #[test]
    fn analyzer_check_send_across_await_non_send_errors() {
        let mut analyzer = TaskAnalyzer::new();
        let s = span();
        let props = TransferProperties {
            is_send: false,
            is_share: false,
        };
        analyzer.check_send_across_await(props, s);
        assert!(analyzer.has_diagnostics());
        assert_eq!(
            analyzer.diagnostics()[0].code,
            TaskDiagnosticCode::NonSendTaskTransfer
        );
    }

    #[test]
    fn diagnostic_code_all_as_str() {
        assert_eq!(TaskDiagnosticCode::UnusedTask.as_str(), "NEXA-TASK-0001");
        assert_eq!(
            TaskDiagnosticCode::TaskEscapesStructuredScope.as_str(),
            "NEXA-TASK-0002"
        );
        assert_eq!(
            TaskDiagnosticCode::TaskAlreadyAwaited.as_str(),
            "NEXA-TASK-0003"
        );
        assert_eq!(
            TaskDiagnosticCode::TaskPossiblyUnresolved.as_str(),
            "NEXA-TASK-0004"
        );
        assert_eq!(
            TaskDiagnosticCode::InvalidTaskMove.as_str(),
            "NEXA-TASK-0005"
        );
        assert_eq!(
            TaskDiagnosticCode::BorrowAcrossAwait.as_str(),
            "NEXA-TASK-0006"
        );
        assert_eq!(
            TaskDiagnosticCode::DetachedTaskNotAllowed.as_str(),
            "NEXA-TASK-0007"
        );
        assert_eq!(
            TaskDiagnosticCode::InvalidAwaitContext.as_str(),
            "NEXA-TASK-0008"
        );
        assert_eq!(
            TaskDiagnosticCode::TaskScopeViolation.as_str(),
            "NEXA-TASK-0009"
        );
        assert_eq!(
            TaskDiagnosticCode::NonSendTaskTransfer.as_str(),
            "NEXA-TASK-0010"
        );
    }

    #[test]
    fn diagnostic_code_all_is_error() {
        let codes = [
            TaskDiagnosticCode::UnusedTask,
            TaskDiagnosticCode::TaskEscapesStructuredScope,
            TaskDiagnosticCode::TaskAlreadyAwaited,
            TaskDiagnosticCode::TaskPossiblyUnresolved,
            TaskDiagnosticCode::InvalidTaskMove,
            TaskDiagnosticCode::BorrowAcrossAwait,
            TaskDiagnosticCode::DetachedTaskNotAllowed,
            TaskDiagnosticCode::InvalidAwaitContext,
            TaskDiagnosticCode::TaskScopeViolation,
            TaskDiagnosticCode::NonSendTaskTransfer,
        ];
        for code in codes {
            assert!(code.is_error());
        }
    }
}
