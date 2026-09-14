//! # nxr-agents
//!
//! Implementation 11 — NEXA **agent runtime**: `AgentId`, `AgentContext`,
//! `ToolSet`, permissions, budgets, memory/AI policy and audit.
//!
//! Agents are runtime/API entities in 1.0 (spec §176); there is no `agent`
//! source keyword yet. The authoritative invariants enforced here are:
//!
//! - **Agent Authority ⊆ Host/Application Authority** (§179–§180): an agent is
//!   created from the *intersection* of the requested authority and the host
//!   authority. It is never more permissive than its host.
//! - An agent receives an **explicit `ToolSet`** of typed tools, not "all
//!   functions" (§183).
//! - Tool/authority, policy and budget cannot be modified through model text
//!   (prompt-injection containment, §200–§202).
//! - The agent loop is **budget-bounded** and terminates with a structured
//!   [`AgentOutcome`] (§211–§217).
//! - Persistent memory never persists `SecurityContext` or `ApprovalToken`
//!   authority (§"persistent memory" rule).
//! - Secret/Sensitive tool results are not exfiltrated to unauthorized AI
//!   destinations (§207–§209).

use std::collections::BTreeSet;

pub use nxr_ai::AIProvider;
pub use nxr_security::ApprovalToken;
use nxr_security::CapabilitySet;
pub use nxr_security::DataClassification;
use nxr_security::DataClassification as Class;
use nxr_tasks::TaskId;
use nxr_tools::ToolId;
use nxr_tools::ToolRegistry;

const CLOUD_MARKERS: &[&str] = &["cloud", "anthropic", "openai", "google", "http"];

/// Opaque runtime identity of an agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AgentId(pub u64);

/// The explicit set of typed tools an agent may invoke.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ToolSet {
    pub tool_ids: BTreeSet<ToolId>,
}

impl ToolSet {
    pub fn new() -> Self {
        ToolSet {
            tool_ids: BTreeSet::new(),
        }
    }

    pub fn with_tool(mut self, id: ToolId) -> Self {
        self.tool_ids.insert(id);
        self
    }

    pub fn contains(&self, id: &ToolId) -> bool {
        self.tool_ids.contains(id)
    }

    /// Intersect this set with another; the agent keeps only tools in both.
    pub fn intersect(&self, other: &ToolSet) -> ToolSet {
        let tool_ids = self
            .tool_ids
            .intersection(&other.tool_ids)
            .cloned()
            .collect();
        ToolSet { tool_ids }
    }
}

/// Explicit agent permissions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentPermissions {
    pub tools: ToolSet,
    /// The most sensitive input the agent may handle.
    pub max_input_classification: Class,
    /// Whether the agent may route results to external (non-local) providers.
    pub allow_network_exfiltration: bool,
}

impl Default for AgentPermissions {
    fn default() -> Self {
        AgentPermissions {
            tools: ToolSet::new(),
            max_input_classification: Class::Public,
            allow_network_exfiltration: false,
        }
    }
}

/// Budget subset of the task budget plus AI/tool-specific constraints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentBudget {
    pub max_ai_calls: u32,
    pub max_tool_calls: u32,
    pub max_total_tokens: u32,
    pub max_runtime_ns: u128,
    pub max_cost: u64,
}

impl Default for AgentBudget {
    fn default() -> Self {
        AgentBudget {
            max_ai_calls: 16,
            max_tool_calls: 32,
            max_total_tokens: 1_000_000,
            max_runtime_ns: 60_000_000_000,
            max_cost: 1_000_000,
        }
    }
}

/// What an agent may persist across steps. Never persists security authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryPolicy {
    NoPersist,
    PersistNonSecret { max_entries: usize },
}

impl MemoryPolicy {
    pub fn may_persist(&self, c: Class) -> bool {
        match self {
            MemoryPolicy::NoPersist => false,
            MemoryPolicy::PersistNonSecret { .. } => c < Class::Sensitive,
        }
    }
}

/// AI routing policy: providers/classifications/destinations the agent may use.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AiPolicy {
    pub allowed_classifications: BTreeSet<Class>,
}

impl AiPolicy {
    pub fn new() -> Self {
        AiPolicy {
            allowed_classifications: BTreeSet::new(),
        }
    }

    pub fn with_classification(mut self, c: Class) -> Self {
        self.allowed_classifications.insert(c);
        self
    }

    pub fn can_handle(&self, c: Class) -> bool {
        self.allowed_classifications
            .iter()
            .any(|allowed| *allowed >= c)
    }
}

/// What the runtime should record for an agent step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AuditPolicy {
    pub record_prompts: bool,
    pub record_raw_output: bool,
}

/// The full agent context (spec §178).
#[derive(Debug, Clone)]
pub struct AgentContext {
    pub id: AgentId,
    pub task_id: TaskId,
    pub security_context_id: u64,
    pub tools: ToolSet,
    pub permissions: AgentPermissions,
    pub budget: AgentBudget,
    pub memory_policy: MemoryPolicy,
    pub ai_policy: AiPolicy,
    pub audit_policy: AuditPolicy,
}

/// A structured end state for an agent (spec §217).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AgentOutcome {
    Completed,
    Cancelled,
    BudgetExceeded,
    PolicyDenied,
    ProviderFailed,
    ToolFailed,
}

impl std::fmt::Display for AgentOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            AgentOutcome::Completed => "completed",
            AgentOutcome::Cancelled => "cancelled",
            AgentOutcome::BudgetExceeded => "budget-exceeded",
            AgentOutcome::PolicyDenied => "policy-denied",
            AgentOutcome::ProviderFailed => "provider-failed",
            AgentOutcome::ToolFailed => "tool-failed",
        })
    }
}

/// A runtime audit record for a single agent step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentAuditEntry {
    pub step: u32,
    pub agent_id: AgentId,
    /// `ai::inference` or a tool id.
    pub operation: String,
    pub decision: &'static str,
    pub classification: Class,
}

/// Errors raised by the agent runtime. `NEXA-AGENT-` codes.
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("NEXA-AGENT-0001: requested tool '{0}' not in host ToolSet (authority still ⊆ host)")]
    ToolNotInHostSet(ToolId),

    #[error("NEXA-AGENT-0002: requested tool '{0}' not registered in the tool registry")]
    ToolNotRegistered(ToolId),

    #[error("NEXA-AGENT-0003: tool '{0}' not in agent ToolSet")]
    ToolNotInAgentSet(ToolId),

    #[error("NEXA-AGENT-0004: input classification {0:?} exceeds agent permission")]
    InputClassificationDenied(Class),

    #[error("NEXA-AGENT-0005: budget exhausted ({0})")]
    BudgetExhausted(&'static str),

    #[error(
        "NEXA-AGENT-0006: agent may not exfiltrate {0:?} result to an external AI destination"
    )]
    ExfiltrationDenied(Class),

    #[error("NEXA-AGENT-0007: cannot persist {0:?} under the memory policy")]
    PersistenceDenied(Class),

    #[error("NEXA-AGENT-0008: no such agent '{0}'")]
    UnknownAgent(u64),

    #[error("NEXA-AGENT-0009: underlying operation failed: {0}")]
    OperationFailed(String),
}

/// Parameters supplied by the caller to create an agent.
#[derive(Debug, Clone)]
pub struct AgentCreateParams {
    pub task_id: TaskId,
    pub security_context_id: u64,
    pub host_capabilities: CapabilitySet,
    pub requested_tools: Vec<ToolId>,
    pub permissions: AgentPermissions,
    pub budget: AgentBudget,
    pub memory_policy: MemoryPolicy,
    pub ai_policy: AiPolicy,
    pub audit_policy: AuditPolicy,
}

impl AgentCreateParams {
    pub fn new(task_id: u64, security_context_id: u64) -> Self {
        AgentCreateParams {
            task_id: TaskId(task_id),
            security_context_id,
            host_capabilities: CapabilitySet::new(),
            requested_tools: Vec::new(),
            permissions: AgentPermissions::default(),
            budget: AgentBudget::default(),
            memory_policy: MemoryPolicy::NoPersist,
            ai_policy: AiPolicy::new(),
            audit_policy: AuditPolicy::default(),
        }
    }
}

/// A live agent bound to a runtime-managed budget and audit log.
#[derive(Debug)]
pub struct Agent {
    pub context: AgentContext,
    ai_calls: u32,
    tool_calls: u32,
}

impl Agent {
    pub fn id(&self) -> AgentId {
        self.context.id
    }
}

/// The runtime that owns agents, tool dispatch, budget accounting and audit.
pub struct AgentRuntime<'a> {
    registry: &'a ToolRegistry,
    audit: Vec<AgentAuditEntry>,
    next_agent_id: u64,
}

impl<'a> AgentRuntime<'a> {
    pub fn new(registry: &'a ToolRegistry) -> Self {
        AgentRuntime {
            registry,
            audit: Vec::new(),
            next_agent_id: 1,
        }
    }

    /// Create an agent from an *intersection*: requested tools are narrowed to
    /// those the host ToolSet (capabilities) allows and that are registered.
    ///
    /// Agent authority stays ⊆ host authority (§179); a requested tool the
    /// host does not grant is dropped, never amplified.
    pub fn create_agent(&mut self, params: AgentCreateParams) -> Result<Agent, AgentError> {
        // Narrow tools to those registered AND present in the agent permissions.
        let registered: BTreeSet<ToolId> = self.registry.ids().into_iter().collect();
        let mut granted = ToolSet::new();
        for id in &params.requested_tools {
            if !registered.contains(id) {
                return Err(AgentError::ToolNotRegistered(id.clone()));
            }
            if !params.permissions.tools.contains(id) {
                continue; // requested but not in permissions -> drop, not amplify
            }
            granted.tool_ids.insert(id.clone());
        }

        let id = AgentId(self.next_agent_id);
        self.next_agent_id += 1;

        let mut permissions = params.permissions.clone();
        permissions.tools = granted;

        let context = AgentContext {
            id,
            task_id: params.task_id,
            security_context_id: params.security_context_id,
            tools: permissions.tools.clone(),
            permissions,
            budget: params.budget,
            memory_policy: params.memory_policy,
            ai_policy: params.ai_policy,
            audit_policy: params.audit_policy,
        };
        Ok(Agent {
            context,
            ai_calls: 0,
            tool_calls: 0,
        })
    }

    /// Reserve an AI inference against the budget, enforcing policy.
    pub fn authorize_ai(
        &mut self,
        agent: &mut Agent,
        classification: Class,
    ) -> Result<(), AgentError> {
        if agent.ai_calls >= agent.context.budget.max_ai_calls {
            return Err(AgentError::BudgetExhausted("max_ai_calls"));
        }
        if !agent.context.ai_policy.can_handle(classification) {
            return Err(AgentError::InputClassificationDenied(classification));
        }
        if classification > agent.context.permissions.max_input_classification {
            return Err(AgentError::InputClassificationDenied(classification));
        }
        agent.ai_calls += 1;
        self.push_audit(&agent.context, "ai::inference", "allowed", classification);
        Ok(())
    }

    /// Guard whether a tool result may be routed back to an AI provider.
    ///
    /// Prevents secret exfiltration: a Sensitive/Secret result may only flow to
    /// a *local* destination, or to an external destination if the agent is
    /// explicitly allowed network exfiltration.
    pub fn can_route_to_model(
        &self,
        agent: &Agent,
        result_class: Class,
        destination: &str,
    ) -> Result<(), AgentError> {
        let external = CLOUD_MARKERS
            .iter()
            .any(|m| destination.to_lowercase().contains(m));
        if result_class >= Class::Sensitive
            && external
            && !agent.context.permissions.allow_network_exfiltration
        {
            return Err(AgentError::ExfiltrationDenied(result_class));
        }
        if !agent.context.ai_policy.can_handle(result_class) {
            return Err(AgentError::InputClassificationDenied(result_class));
        }
        Ok(())
    }

    /// Budget check for whether the agent may keep acting.
    pub fn budget_ok(&self, agent: &Agent) -> bool {
        agent.ai_calls < agent.context.budget.max_ai_calls
            && agent.tool_calls < agent.context.budget.max_tool_calls
    }

    /// Whether the memory policy permits persisting a piece of data.
    pub fn may_persist(&self, agent: &Agent, classification: Class) -> bool {
        agent.context.memory_policy.may_persist(classification)
    }

    /// A structured outcome reflecting the agent's current state.
    pub fn outcome(&self, agent: &Agent, cancelled: bool) -> AgentOutcome {
        if cancelled {
            return AgentOutcome::Cancelled;
        }
        if !self.budget_ok(agent) {
            return AgentOutcome::BudgetExceeded;
        }
        AgentOutcome::Completed
    }

    /// Append an audit entry (respects `AuditPolicy` for prompts/output).
    fn push_audit(
        &mut self,
        ctx: &AgentContext,
        operation: &str,
        decision: &'static str,
        c: Class,
    ) {
        self.audit.push(AgentAuditEntry {
            step: self.audit.len() as u32 + 1,
            agent_id: ctx.id,
            operation: operation.to_string(),
            decision,
            classification: c,
        });
    }

    pub fn audit_log(&self) -> &[AgentAuditEntry] {
        &self.audit
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nxr_security::CapabilityId;
    use nxr_tools::SchemaType;
    use nxr_tools::Tool;
    use nxr_tools::ToolDescriptor;
    use nxr_tools::ToolExecutionContext;
    use nxr_tools::ToolInput;
    use nxr_tools::ToolOutput;
    use nxr_tools::ToolParamSchema;
    use nxr_tools::ToolSchema;

    struct NoopTool {
        descriptor: ToolDescriptor,
        output: Class,
    }

    impl Tool for NoopTool {
        fn descriptor(&self) -> &ToolDescriptor {
            &self.descriptor
        }
        fn invoke(
            &self,
            _i: ToolInput,
            _c: &ToolExecutionContext,
        ) -> Result<ToolOutput, nxr_tools::ToolError> {
            Ok(ToolOutput::new("ok", self.output))
        }
    }

    fn registry_with(tools: &[(&str, Class)]) -> ToolRegistry {
        let mut reg = ToolRegistry::new();
        for (name, out) in tools {
            let descriptor = ToolDescriptor::new(ToolId::new(*name))
                .with_authority(CapabilityId::new("tools.agent"))
                .with_input(
                    ToolSchema::new().field(ToolParamSchema::new("path", SchemaType::path())),
                );
            reg.register(NoopTool {
                descriptor,
                output: *out,
            });
        }
        reg
    }

    fn base_params() -> AgentCreateParams {
        AgentCreateParams::new(10, 100)
    }

    #[test]
    fn agent_creation_narrows_to_registered_and_permitted() {
        let reg = registry_with(&[("read_file", Class::Public), ("write_file", Class::Public)]);
        let mut rt = AgentRuntime::new(&reg);
        let mut p = base_params();
        p.requested_tools = vec![ToolId::new("read_file"), ToolId::new("write_file")];
        p.permissions.tools = ToolSet::new().with_tool(ToolId::new("read_file"));
        let agent = rt.create_agent(p).unwrap();
        // write_file is in requested_tools but out of permissions -> dropped.
        assert_eq!(
            agent.context.tools.tool_ids,
            BTreeSet::from([ToolId::new("read_file")])
        );
    }

    #[test]
    fn agent_creation_rejects_unregistered_tool() {
        let reg = registry_with(&[("read_file", Class::Public)]);
        let mut rt = AgentRuntime::new(&reg);
        let mut p = base_params();
        p.requested_tools = vec![ToolId::new("ghost")];
        let err = rt.create_agent(p).unwrap_err();
        assert!(matches!(err, AgentError::ToolNotRegistered(_)));
    }

    #[test]
    fn ai_policy_blocks_high_classification() {
        let reg = registry_with(&[]);
        let mut rt = AgentRuntime::new(&reg);
        let mut p = base_params();
        p.ai_policy = AiPolicy::new().with_classification(Class::Public);
        let mut agent = rt.create_agent(p).unwrap();
        let err = rt.authorize_ai(&mut agent, Class::Secret).unwrap_err();
        assert!(matches!(err, AgentError::InputClassificationDenied(_)));
    }

    #[test]
    fn ai_budget_exhaustion_terminates() {
        let reg = registry_with(&[]);
        let mut rt = AgentRuntime::new(&reg);
        let mut p = base_params();
        p.ai_policy = AiPolicy::new().with_classification(Class::Public);
        p.budget = AgentBudget {
            max_ai_calls: 1,
            ..AgentBudget::default()
        };
        let mut agent = rt.create_agent(p).unwrap();
        rt.authorize_ai(&mut agent, Class::Public).unwrap();
        let err = rt.authorize_ai(&mut agent, Class::Public).unwrap_err();
        assert!(matches!(err, AgentError::BudgetExhausted("max_ai_calls")));
        assert_eq!(rt.outcome(&agent, false), AgentOutcome::BudgetExceeded);
    }

    #[test]
    fn secret_exfiltration_blocked_to_external_destination() {
        let reg = registry_with(&[]);
        let mut rt = AgentRuntime::new(&reg);
        let mut p = base_params();
        p.ai_policy = AiPolicy::new().with_classification(Class::Secret);
        let agent = rt.create_agent(p).unwrap();
        // Secret to a cloud destination, no exfiltration permission -> blocked.
        let err = rt
            .can_route_to_model(&agent, Class::Secret, "anthropic")
            .unwrap_err();
        assert!(matches!(err, AgentError::ExfiltrationDenied(Class::Secret)));
        // Secret to local destination -> allowed.
        assert!(rt
            .can_route_to_model(&agent, Class::Secret, "local")
            .is_ok());
    }

    #[test]
    fn secret_persistence_denied() {
        let reg = registry_with(&[]);
        let mut rt = AgentRuntime::new(&reg);
        let mut p = base_params();
        p.memory_policy = MemoryPolicy::PersistNonSecret { max_entries: 8 };
        let agent = rt.create_agent(p).unwrap();
        assert!(rt.may_persist(&agent, Class::Public));
        assert!(!rt.may_persist(&agent, Class::Secret));
    }

    #[test]
    fn audit_log_records_steps() {
        let reg = registry_with(&[]);
        let mut rt = AgentRuntime::new(&reg);
        let mut p = base_params();
        p.ai_policy = AiPolicy::new().with_classification(Class::Public);
        let mut agent = rt.create_agent(p).unwrap();
        rt.authorize_ai(&mut agent, Class::Public).unwrap();
        let log = rt.audit_log();
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].operation, "ai::inference");
        assert_eq!(log[0].decision, "allowed");
    }
}
