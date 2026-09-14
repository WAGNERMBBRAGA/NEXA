use std::collections::BTreeMap;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};

// ─── TaskId ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TaskId(pub u64);

// ─── RuntimeTaskState ──────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeTaskState {
    Created,
    Runnable,
    Running,
    Suspended,
    Cancelling,
    Completed,
    Cancelled,
    Panicked,
    Trapped,
    HostFailed,
}

impl RuntimeTaskState {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            RuntimeTaskState::Completed
                | RuntimeTaskState::Cancelled
                | RuntimeTaskState::Panicked
                | RuntimeTaskState::Trapped
                | RuntimeTaskState::HostFailed
        )
    }

    pub fn is_active(&self) -> bool {
        !self.is_terminal() && *self != RuntimeTaskState::Cancelling
    }

    pub fn name(&self) -> &'static str {
        match self {
            RuntimeTaskState::Created => "Created",
            RuntimeTaskState::Runnable => "Runnable",
            RuntimeTaskState::Running => "Running",
            RuntimeTaskState::Suspended => "Suspended",
            RuntimeTaskState::Cancelling => "Cancelling",
            RuntimeTaskState::Completed => "Completed",
            RuntimeTaskState::Cancelled => "Cancelled",
            RuntimeTaskState::Panicked => "Panicked",
            RuntimeTaskState::Trapped => "Trapped",
            RuntimeTaskState::HostFailed => "HostFailed",
        }
    }
}

// ─── TaskOutcome ───────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskOutcome<T, E> {
    Completed(T),
    Failed(E),
    Cancelled,
    Panicked,
    Trapped,
    HostFailed,
}

impl<T, E> TaskOutcome<T, E> {
    pub fn is_success(&self) -> bool {
        matches!(self, TaskOutcome::Completed(_))
    }

    pub fn is_failure(&self) -> bool {
        matches!(
            self,
            TaskOutcome::Failed(_)
                | TaskOutcome::Panicked
                | TaskOutcome::Trapped
                | TaskOutcome::HostFailed
        )
    }

    pub fn is_cancelled(&self) -> bool {
        matches!(self, TaskOutcome::Cancelled)
    }

    pub fn value(&self) -> Option<&T> {
        match self {
            TaskOutcome::Completed(v) => Some(v),
            _ => None,
        }
    }
}

// ─── TaskError ─────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum TaskError {
    LimitExceeded { resource: String, limit: u32 },
    InvalidTaskHandle { id: u64 },
    InvalidStateTransition { from: String, to: String },
    BudgetExceeded { category: String },
    DeadlineExceeded { task_id: u64 },
    ParentNotFound { id: u64 },
    CancellationFailed(String),
}

impl fmt::Display for TaskError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskError::LimitExceeded { resource, limit } => {
                write!(
                    f,
                    "[NEXA-RUNTIME-TASK-0001] limit exceeded for {resource}: max {limit}"
                )
            }
            TaskError::InvalidTaskHandle { id } => {
                write!(f, "[NEXA-RUNTIME-TASK-0002] invalid task handle: {id}")
            }
            TaskError::InvalidStateTransition { from, to } => {
                write!(
                    f,
                    "[NEXA-RUNTIME-TASK-0003] invalid state transition from {from} to {to}"
                )
            }
            TaskError::BudgetExceeded { category } => {
                write!(
                    f,
                    "[NEXA-RUNTIME-TASK-0004] budget exceeded for category: {category}"
                )
            }
            TaskError::DeadlineExceeded { task_id } => {
                write!(
                    f,
                    "[NEXA-RUNTIME-TASK-0005] deadline exceeded for task: {task_id}"
                )
            }
            TaskError::ParentNotFound { id } => {
                write!(f, "[NEXA-RUNTIME-TASK-0006] parent not found: {id}")
            }
            TaskError::CancellationFailed(msg) => {
                write!(f, "[NEXA-RUNTIME-TASK-0007] cancellation failed: {msg}")
            }
        }
    }
}

impl std::error::Error for TaskError {}

// ─── RuntimeBudget ─────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeBudget {
    pub max_instructions: u64,
    pub max_memory_bytes: u64,
    pub max_task_count: u32,
    pub max_resource_count: u32,
    pub max_filesystem_bytes: u64,
    pub max_network_bytes: u64,
    pub max_provider_calls: u32,
    pub deadline_ns: Option<u64>,
}

impl RuntimeBudget {
    pub fn unlimited() -> Self {
        Self {
            max_instructions: u64::MAX,
            max_memory_bytes: u64::MAX,
            max_task_count: u32::MAX,
            max_resource_count: u32::MAX,
            max_filesystem_bytes: u64::MAX,
            max_network_bytes: u64::MAX,
            max_provider_calls: u32::MAX,
            deadline_ns: None,
        }
    }

    pub fn with_max_instructions(mut self, v: u64) -> Self {
        self.max_instructions = v;
        self
    }

    pub fn with_max_memory(mut self, v: u64) -> Self {
        self.max_memory_bytes = v;
        self
    }

    pub fn with_max_task_count(mut self, v: u32) -> Self {
        self.max_task_count = v;
        self
    }

    pub fn with_max_resource_count(mut self, v: u32) -> Self {
        self.max_resource_count = v;
        self
    }

    pub fn with_max_filesystem_bytes(mut self, v: u64) -> Self {
        self.max_filesystem_bytes = v;
        self
    }

    pub fn with_max_network_bytes(mut self, v: u64) -> Self {
        self.max_network_bytes = v;
        self
    }

    pub fn with_max_provider_calls(mut self, v: u32) -> Self {
        self.max_provider_calls = v;
        self
    }

    pub fn with_deadline_ns(mut self, v: Option<u64>) -> Self {
        self.deadline_ns = v;
        self
    }

    pub fn consume_instruction(&mut self, amount: u64) -> Result<(), TaskError> {
        let new_val = self.max_instructions.saturating_add(amount);
        if new_val < amount || new_val > self.max_instructions {
            // overflow check: saturating_add already capped at MAX, but if we were
            // counting down, this logic is different. Let's track consumed vs max.
        }
        // Treat max_instructions as a budget cap: we need a consumed field approach.
        // Simpler: re-interpret — budget holds the REMAINING budget, consume subtracts.
        if self.max_instructions < amount {
            return Err(TaskError::BudgetExceeded {
                category: "instructions".to_string(),
            });
        }
        self.max_instructions -= amount;
        Ok(())
    }

    pub fn consume_memory(&mut self, amount: u64) -> Result<(), TaskError> {
        if self.max_memory_bytes < amount {
            return Err(TaskError::BudgetExceeded {
                category: "memory".to_string(),
            });
        }
        self.max_memory_bytes -= amount;
        Ok(())
    }

    pub fn consume_task(&mut self) -> Result<(), TaskError> {
        if self.max_task_count == 0 {
            return Err(TaskError::BudgetExceeded {
                category: "tasks".to_string(),
            });
        }
        self.max_task_count -= 1;
        Ok(())
    }

    pub fn consume_resource(&mut self) -> Result<(), TaskError> {
        if self.max_resource_count == 0 {
            return Err(TaskError::BudgetExceeded {
                category: "resources".to_string(),
            });
        }
        self.max_resource_count -= 1;
        Ok(())
    }

    pub fn consume_filesystem(&mut self, bytes: u64) -> Result<(), TaskError> {
        if self.max_filesystem_bytes < bytes {
            return Err(TaskError::BudgetExceeded {
                category: "filesystem".to_string(),
            });
        }
        self.max_filesystem_bytes -= bytes;
        Ok(())
    }

    pub fn consume_network(&mut self, bytes: u64) -> Result<(), TaskError> {
        if self.max_network_bytes < bytes {
            return Err(TaskError::BudgetExceeded {
                category: "network".to_string(),
            });
        }
        self.max_network_bytes -= bytes;
        Ok(())
    }

    pub fn consume_provider_call(&mut self) -> Result<(), TaskError> {
        if self.max_provider_calls == 0 {
            return Err(TaskError::BudgetExceeded {
                category: "provider_calls".to_string(),
            });
        }
        self.max_provider_calls -= 1;
        Ok(())
    }

    pub fn remaining_instructions(&self) -> u64 {
        self.max_instructions
    }

    pub fn remaining_memory(&self) -> u64 {
        self.max_memory_bytes
    }

    pub fn is_within(&self, parent: &RuntimeBudget) -> bool {
        self.max_instructions <= parent.max_instructions
            && self.max_memory_bytes <= parent.max_memory_bytes
            && self.max_task_count <= parent.max_task_count
            && self.max_resource_count <= parent.max_resource_count
            && self.max_filesystem_bytes <= parent.max_filesystem_bytes
            && self.max_network_bytes <= parent.max_network_bytes
            && self.max_provider_calls <= parent.max_provider_calls
    }
}

// ─── MonotonicDeadline ─────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct MonotonicDeadline {
    pub deadline_ns: u64,
}

impl MonotonicDeadline {
    pub fn new(deadline_ns: u64) -> Self {
        Self { deadline_ns }
    }

    pub fn is_expired(&self, current_ns: u64) -> bool {
        current_ns >= self.deadline_ns
    }

    pub fn remaining_ns(&self, current_ns: u64) -> u64 {
        self.deadline_ns.saturating_sub(current_ns)
    }
}

// ─── CancellationState ─────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancellationState {
    NotCancellable,
    Active { cancelled: bool },
}

impl CancellationState {
    pub fn is_cancelled(&self) -> bool {
        matches!(self, CancellationState::Active { cancelled: true })
    }

    pub fn mark_cancelled(&mut self) {
        *self = CancellationState::Active { cancelled: true };
    }

    pub fn is_cancellable(&self) -> bool {
        matches!(self, CancellationState::Active { .. })
    }
}

// ─── CancellationToken ─────────────────────────────────────────

#[derive(Debug)]
pub struct CancellationToken {
    cancelled: AtomicBool,
}

impl Clone for CancellationToken {
    fn clone(&self) -> Self {
        Self {
            cancelled: AtomicBool::new(self.cancelled.load(Ordering::SeqCst)),
        }
    }
}

impl CancellationToken {
    pub fn new() -> Self {
        Self {
            cancelled: AtomicBool::new(false),
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub fn check(&self) -> Result<(), TaskError> {
        if self.is_cancelled() {
            Err(TaskError::CancellationFailed(
                "task was cancelled".to_string(),
            ))
        } else {
            Ok(())
        }
    }
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

// ─── RuntimeTask ───────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RuntimeTask {
    id: TaskId,
    parent_id: Option<TaskId>,
    state: RuntimeTaskState,
    security_context_id: u64,
    cancellation: CancellationToken,
    deadline: Option<MonotonicDeadline>,
    budget: RuntimeBudget,
    children: Vec<TaskId>,
}

impl RuntimeTask {
    pub fn id(&self) -> TaskId {
        self.id
    }

    pub fn parent_id(&self) -> Option<TaskId> {
        self.parent_id
    }

    pub fn state(&self) -> RuntimeTaskState {
        self.state
    }

    pub fn security_context_id(&self) -> u64 {
        self.security_context_id
    }

    pub fn cancellation(&self) -> &CancellationToken {
        &self.cancellation
    }

    pub fn deadline(&self) -> Option<MonotonicDeadline> {
        self.deadline
    }

    pub fn budget(&self) -> &RuntimeBudget {
        &self.budget
    }

    pub fn children(&self) -> &[TaskId] {
        &self.children
    }

    pub fn is_terminal(&self) -> bool {
        self.state.is_terminal()
    }

    pub fn cancel(&self) {
        self.cancellation.cancel();
    }

    pub fn add_child(&mut self, child_id: TaskId) {
        self.children.push(child_id);
    }
}

// ─── TaskContext ───────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct TaskContext {
    pub task_id: TaskId,
    pub security_context_id: u64,
    pub cancellation_token: CancellationToken,
    pub deadline: Option<MonotonicDeadline>,
    pub budget: RuntimeBudget,
}

// ─── TaskCreationConfig ────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct TaskCreationConfig {
    pub parent_context_id: u64,
    pub budget: RuntimeBudget,
    pub deadline: Option<MonotonicDeadline>,
}

impl TaskCreationConfig {
    pub fn new(parent_context_id: u64) -> Self {
        Self {
            parent_context_id,
            budget: RuntimeBudget::unlimited(),
            deadline: None,
        }
    }

    pub fn with_budget(mut self, b: RuntimeBudget) -> Self {
        self.budget = b;
        self
    }

    pub fn with_deadline(mut self, d: MonotonicDeadline) -> Self {
        self.deadline = Some(d);
        self
    }
}

// ─── TaskScheduler ─────────────────────────────────────────────

pub struct TaskScheduler {
    tasks: std::sync::Mutex<BTreeMap<TaskId, RuntimeTask>>,
    next_id: u64,
    max_tasks: u32,
}

impl TaskScheduler {
    pub fn new(max_tasks: u32) -> Self {
        Self {
            tasks: std::sync::Mutex::new(BTreeMap::new()),
            next_id: 1,
            max_tasks,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(256)
    }

    fn alloc_id(&mut self) -> TaskId {
        let id = TaskId(self.next_id);
        self.next_id += 1;
        id
    }

    pub fn create_task(&mut self, config: TaskCreationConfig) -> Result<RuntimeTask, TaskError> {
        {
            let tasks = self.tasks.lock().unwrap();
            if tasks.len() as u32 >= self.max_tasks {
                return Err(TaskError::LimitExceeded {
                    resource: "tasks".to_string(),
                    limit: self.max_tasks,
                });
            }
        }

        let task_id = self.alloc_id();

        let parent_id = if config.parent_context_id > 0 {
            let parent_tid = TaskId(config.parent_context_id);
            let tasks = self.tasks.lock().unwrap();
            if tasks.contains_key(&parent_tid) {
                Some(parent_tid)
            } else {
                return Err(TaskError::ParentNotFound {
                    id: config.parent_context_id,
                });
            }
        } else {
            None
        };

        let task = RuntimeTask {
            id: task_id,
            parent_id,
            state: RuntimeTaskState::Created,
            security_context_id: config.parent_context_id,
            cancellation: CancellationToken::new(),
            deadline: config.deadline,
            budget: config.budget,
            children: Vec::new(),
        };

        {
            let mut tasks = self.tasks.lock().unwrap();
            if let Some(pid) = parent_id {
                if let Some(parent) = tasks.get_mut(&pid) {
                    parent.add_child(task_id);
                }
            }
            tasks.insert(task_id, task.clone());
        }

        Ok(task)
    }

    pub fn get_task(&self, id: TaskId) -> Option<RuntimeTask> {
        let tasks = self.tasks.lock().unwrap();
        tasks.get(&id).cloned()
    }

    pub fn set_running(&mut self, id: TaskId) -> Result<(), TaskError> {
        let mut tasks = self.tasks.lock().unwrap();
        let task = tasks
            .get_mut(&id)
            .ok_or(TaskError::InvalidTaskHandle { id: id.0 })?;
        if task.state != RuntimeTaskState::Runnable
            && task.state != RuntimeTaskState::Created
            && task.state != RuntimeTaskState::Suspended
        {
            let from = task.state.name().to_string();
            return Err(TaskError::InvalidStateTransition {
                from,
                to: "Running".to_string(),
            });
        }
        task.state = RuntimeTaskState::Running;
        Ok(())
    }

    pub fn set_suspended(&mut self, id: TaskId) -> Result<(), TaskError> {
        let mut tasks = self.tasks.lock().unwrap();
        let task = tasks
            .get_mut(&id)
            .ok_or(TaskError::InvalidTaskHandle { id: id.0 })?;
        if task.state != RuntimeTaskState::Running {
            let from = task.state.name().to_string();
            return Err(TaskError::InvalidStateTransition {
                from,
                to: "Suspended".to_string(),
            });
        }
        task.state = RuntimeTaskState::Suspended;
        Ok(())
    }

    pub fn complete_task(&mut self, id: TaskId) -> Result<(), TaskError> {
        let mut tasks = self.tasks.lock().unwrap();
        let task = tasks
            .get_mut(&id)
            .ok_or(TaskError::InvalidTaskHandle { id: id.0 })?;
        if task.state.is_terminal() {
            let from = task.state.name().to_string();
            return Err(TaskError::InvalidStateTransition {
                from,
                to: "Completed".to_string(),
            });
        }
        task.state = RuntimeTaskState::Completed;
        Ok(())
    }

    pub fn cancel_task(&mut self, id: TaskId) -> Result<(), TaskError> {
        let mut tasks = self.tasks.lock().unwrap();
        let task = tasks
            .get_mut(&id)
            .ok_or(TaskError::InvalidTaskHandle { id: id.0 })?;
        if task.state.is_terminal() {
            return Ok(());
        }
        task.cancellation.cancel();
        task.state = RuntimeTaskState::Cancelling;
        // Immediately move to Cancelled for cooperative model
        task.state = RuntimeTaskState::Cancelled;
        Ok(())
    }

    pub fn propagate_cancellation(&mut self, parent_id: TaskId) -> Result<(), TaskError> {
        let children: Vec<TaskId> = {
            let tasks = self.tasks.lock().unwrap();
            let parent = tasks
                .get(&parent_id)
                .ok_or(TaskError::InvalidTaskHandle { id: parent_id.0 })?;
            parent.children.clone()
        };

        for child_id in children {
            self.cancel_task(child_id)?;
            self.propagate_cancellation(child_id)?;
        }

        Ok(())
    }

    pub fn task_count(&self) -> u32 {
        let tasks = self.tasks.lock().unwrap();
        tasks.len() as u32
    }

    pub fn active_count(&self) -> u32 {
        let tasks = self.tasks.lock().unwrap();
        tasks.values().filter(|t| !t.state.is_terminal()).count() as u32
    }

    pub fn is_terminal(&self, id: TaskId) -> bool {
        let tasks = self.tasks.lock().unwrap();
        tasks.get(&id).is_some_and(|t| t.state.is_terminal())
    }

    pub fn all_task_ids(&self) -> Vec<TaskId> {
        let tasks = self.tasks.lock().unwrap();
        tasks.keys().copied().collect()
    }
}

// ─── Tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_id_creation() {
        let a = TaskId(1);
        let b = TaskId(1);
        let c = TaskId(2);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn test_task_state_terminal() {
        assert!(RuntimeTaskState::Completed.is_terminal());
        assert!(RuntimeTaskState::Cancelled.is_terminal());
        assert!(RuntimeTaskState::Panicked.is_terminal());
        assert!(RuntimeTaskState::Trapped.is_terminal());
        assert!(RuntimeTaskState::HostFailed.is_terminal());
        assert!(!RuntimeTaskState::Created.is_terminal());
        assert!(!RuntimeTaskState::Runnable.is_terminal());
        assert!(!RuntimeTaskState::Running.is_terminal());
        assert!(!RuntimeTaskState::Suspended.is_terminal());
        assert!(!RuntimeTaskState::Cancelling.is_terminal());
    }

    #[test]
    fn test_task_state_active() {
        assert!(RuntimeTaskState::Created.is_active());
        assert!(RuntimeTaskState::Runnable.is_active());
        assert!(RuntimeTaskState::Running.is_active());
        assert!(RuntimeTaskState::Suspended.is_active());
        assert!(!RuntimeTaskState::Cancelling.is_active());
        assert!(!RuntimeTaskState::Completed.is_active());
        assert!(!RuntimeTaskState::Cancelled.is_active());
        assert!(!RuntimeTaskState::Panicked.is_active());
        assert!(!RuntimeTaskState::Trapped.is_active());
        assert!(!RuntimeTaskState::HostFailed.is_active());
    }

    #[test]
    fn test_task_outcome_completed() {
        let outcome: TaskOutcome<i32, String> = TaskOutcome::Completed(42);
        assert!(outcome.is_success());
        assert!(!outcome.is_failure());
        assert!(!outcome.is_cancelled());
    }

    #[test]
    fn test_task_outcome_cancelled() {
        let outcome: TaskOutcome<i32, String> = TaskOutcome::Cancelled;
        assert!(!outcome.is_success());
        assert!(!outcome.is_failure());
        assert!(outcome.is_cancelled());
    }

    #[test]
    fn test_task_outcome_value() {
        let completed: TaskOutcome<i32, String> = TaskOutcome::Completed(42);
        assert_eq!(completed.value(), Some(&42));

        let failed: TaskOutcome<i32, String> = TaskOutcome::Failed("err".to_string());
        assert_eq!(failed.value(), None);

        let cancelled: TaskOutcome<i32, String> = TaskOutcome::Cancelled;
        assert_eq!(cancelled.value(), None);
    }

    #[test]
    fn test_budget_unlimited() {
        let b = RuntimeBudget::unlimited();
        assert_eq!(b.max_instructions, u64::MAX);
        assert_eq!(b.max_memory_bytes, u64::MAX);
        assert_eq!(b.max_task_count, u32::MAX);
        assert_eq!(b.max_resource_count, u32::MAX);
        assert_eq!(b.max_filesystem_bytes, u64::MAX);
        assert_eq!(b.max_network_bytes, u64::MAX);
        assert_eq!(b.max_provider_calls, u32::MAX);
        assert!(b.deadline_ns.is_none());
    }

    #[test]
    fn test_budget_consume_instruction() {
        let mut b = RuntimeBudget::unlimited().with_max_instructions(1000);
        b.consume_instruction(400).unwrap();
        assert_eq!(b.remaining_instructions(), 600);
        b.consume_instruction(600).unwrap();
        assert_eq!(b.remaining_instructions(), 0);
    }

    #[test]
    fn test_budget_limit_exceeded() {
        let mut b = RuntimeBudget::unlimited().with_max_instructions(100);
        assert!(b.consume_instruction(101).is_err());

        let mut b2 = RuntimeBudget::unlimited().with_max_memory(50);
        assert!(b2.consume_memory(51).is_err());

        let mut b3 = RuntimeBudget::unlimited().with_max_task_count(0);
        assert!(b3.consume_task().is_err());

        let mut b4 = RuntimeBudget::unlimited().with_max_resource_count(0);
        assert!(b4.consume_resource().is_err());

        let mut b5 = RuntimeBudget::unlimited().with_max_filesystem_bytes(10);
        assert!(b5.consume_filesystem(11).is_err());

        let mut b6 = RuntimeBudget::unlimited().with_max_network_bytes(10);
        assert!(b6.consume_network(11).is_err());

        let mut b7 = RuntimeBudget::unlimited().with_max_provider_calls(0);
        assert!(b7.consume_provider_call().is_err());
    }

    #[test]
    fn test_budget_is_within() {
        let parent = RuntimeBudget::unlimited()
            .with_max_instructions(1000)
            .with_max_memory(2048);

        let child = RuntimeBudget::unlimited()
            .with_max_instructions(500)
            .with_max_memory(1024);

        assert!(child.is_within(&parent));

        let too_big = RuntimeBudget::unlimited()
            .with_max_instructions(2000)
            .with_max_memory(1024);

        assert!(!too_big.is_within(&parent));
    }

    #[test]
    fn test_deadline_creation() {
        let d = MonotonicDeadline::new(1000);
        assert_eq!(d.deadline_ns, 1000);
    }

    #[test]
    fn test_deadline_expired() {
        let d = MonotonicDeadline::new(100);
        assert!(!d.is_expired(50));
        assert!(d.is_expired(100));
        assert!(d.is_expired(200));
        assert_eq!(d.remaining_ns(50), 50);
        assert_eq!(d.remaining_ns(100), 0);
        assert_eq!(d.remaining_ns(200), 0);
    }

    #[test]
    fn test_cancellation_token() {
        let token = CancellationToken::new();
        assert!(!token.is_cancelled());
        assert!(token.check().is_ok());

        token.cancel();
        assert!(token.is_cancelled());
        assert!(token.check().is_err());
    }

    #[test]
    fn test_scheduler_create_task() {
        let mut scheduler = TaskScheduler::with_defaults();
        let config = TaskCreationConfig::new(0);
        let task = scheduler.create_task(config).unwrap();
        assert_eq!(task.id(), TaskId(1));
        assert_eq!(task.state(), RuntimeTaskState::Created);
        assert_eq!(scheduler.task_count(), 1);
    }

    #[test]
    fn test_scheduler_task_lifecycle() {
        let mut scheduler = TaskScheduler::with_defaults();
        let config = TaskCreationConfig::new(0);
        let task = scheduler.create_task(config).unwrap();
        let id = task.id();

        // Created -> Running (set_running allows Created -> Running)
        scheduler.set_running(id).unwrap();
        assert_eq!(
            scheduler.get_task(id).unwrap().state(),
            RuntimeTaskState::Running
        );

        // Running -> Suspended
        scheduler.set_suspended(id).unwrap();
        assert_eq!(
            scheduler.get_task(id).unwrap().state(),
            RuntimeTaskState::Suspended
        );

        // Suspended -> Running -> Completed
        scheduler.set_running(id).unwrap();
        scheduler.complete_task(id).unwrap();
        assert_eq!(
            scheduler.get_task(id).unwrap().state(),
            RuntimeTaskState::Completed
        );
        assert!(scheduler.is_terminal(id));
    }

    #[test]
    fn test_scheduler_cancel_task() {
        let mut scheduler = TaskScheduler::with_defaults();
        let config = TaskCreationConfig::new(0);
        let task = scheduler.create_task(config).unwrap();
        let id = task.id();

        scheduler.cancel_task(id).unwrap();
        assert_eq!(
            scheduler.get_task(id).unwrap().state(),
            RuntimeTaskState::Cancelled
        );
        assert!(scheduler.is_terminal(id));
        assert!(scheduler
            .get_task(id)
            .unwrap()
            .cancellation()
            .is_cancelled());
    }

    #[test]
    fn test_scheduler_propagate_cancellation() {
        let mut scheduler = TaskScheduler::with_defaults();
        let root_config = TaskCreationConfig::new(0);
        let root = scheduler.create_task(root_config).unwrap();
        let root_id = root.id();

        // Create children
        let child1_config = TaskCreationConfig::new(root_id.0);
        let child1 = scheduler.create_task(child1_config).unwrap();
        let child1_id = child1.id();

        let child2_config = TaskCreationConfig::new(child1_id.0);
        let child2 = scheduler.create_task(child2_config).unwrap();
        let child2_id = child2.id();

        scheduler.propagate_cancellation(root_id).unwrap();

        assert!(scheduler
            .get_task(child1_id)
            .unwrap()
            .cancellation()
            .is_cancelled());
        assert!(scheduler
            .get_task(child2_id)
            .unwrap()
            .cancellation()
            .is_cancelled());
    }

    #[test]
    fn test_scheduler_limit_exceeded() {
        let mut scheduler = TaskScheduler::new(2);
        let config1 = TaskCreationConfig::new(0);
        scheduler.create_task(config1).unwrap();
        let config2 = TaskCreationConfig::new(0);
        scheduler.create_task(config2).unwrap();
        let config3 = TaskCreationConfig::new(0);
        let result = scheduler.create_task(config3);
        assert!(result.is_err());
        match result.unwrap_err() {
            TaskError::LimitExceeded { resource, limit } => {
                assert_eq!(resource, "tasks");
                assert_eq!(limit, 2);
            }
            other => panic!("expected LimitExceeded, got {:?}", other),
        }
    }
}
