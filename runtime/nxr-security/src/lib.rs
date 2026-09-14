use std::collections::BTreeMap;
use std::fmt;
use std::fmt::Display;

// ─── RuntimeError ────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum RuntimeError {
    InvalidContext { id: u64 },
    ParentNotFound { id: u64 },
    AuthorityEscalation { detail: String },
    ApprovalExpired,
    ApprovalExhausted,
    InvalidApprovalToken { detail: String },
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidContext { id } => write!(f, "invalid security context id {id}"),
            Self::ParentNotFound { id } => write!(f, "parent context {id} not found"),
            Self::AuthorityEscalation { detail } => {
                write!(f, "authority escalation: {detail}")
            }
            Self::ApprovalExpired => write!(f, "approval token expired"),
            Self::ApprovalExhausted => write!(f, "approval token exhausted"),
            Self::InvalidApprovalToken { detail } => {
                write!(f, "invalid approval token: {detail}")
            }
        }
    }
}

impl std::error::Error for RuntimeError {}

// ─── CapabilityId ────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CapabilityId(pub String);

impl CapabilityId {
    pub fn new(name: &str) -> Self {
        Self(name.to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for CapabilityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for CapabilityId {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

// ─── Pre-defined capability constants ───────────────────────────

pub mod capabilities {
    pub const CONSOLE: &str = "console";
    pub const CONSOLE_WRITE: &str = "console::write";
    pub const CONSOLE_READ: &str = "console::read";
    pub const FILESYSTEM: &str = "filesystem";
    pub const FILESYSTEM_READ: &str = "filesystem::read";
    pub const FILESYSTEM_WRITE: &str = "filesystem::write";
    pub const NETWORK: &str = "network";
    pub const CLOCK: &str = "clock";
    pub const CLOCK_WALL: &str = "clock::wall";
    pub const CLOCK_MONOTONIC: &str = "clock::monotonic";
    pub const RANDOM: &str = "random";
    pub const RANDOM_SECURE: &str = "random::secure";
    pub const ENVIRONMENT: &str = "environment";
    pub const ENVIRONMENT_READ: &str = "environment::read";
    pub const PROCESS: &str = "process";
    pub const PROCESS_SPAWN: &str = "process::spawn";
    pub const PROCESS_SHELL: &str = "process::shell";
    pub const SECRETS: &str = "secrets";
}

// ─── CapabilityScope ─────────────────────────────────────────────

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CapabilityScope {
    pub roots: Vec<String>,
    pub hosts: Vec<String>,
    pub ports: Vec<u16>,
    pub allowed_methods: Vec<String>,
    pub variable_names: Vec<String>,
}

impl CapabilityScope {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.roots.is_empty()
            && self.hosts.is_empty()
            && self.ports.is_empty()
            && self.allowed_methods.is_empty()
            && self.variable_names.is_empty()
    }

    pub fn with_root(mut self, root: impl Into<String>) -> Self {
        self.roots.push(root.into());
        self
    }

    pub fn with_host(mut self, host: impl Into<String>) -> Self {
        self.hosts.push(host.into());
        self
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.ports.push(port);
        self
    }

    pub fn with_method(mut self, method: impl Into<String>) -> Self {
        self.allowed_methods.push(method.into());
        self
    }

    pub fn with_variable(mut self, var: impl Into<String>) -> Self {
        self.variable_names.push(var.into());
        self
    }
}

// ─── CapabilityConstraints ───────────────────────────────────────

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CapabilityConstraints {
    pub read_only: bool,
    pub byte_limit: Option<u64>,
    pub path_restrictions: Vec<String>,
    pub host_restrictions: Vec<String>,
}

impl CapabilityConstraints {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn read_only(mut self) -> Self {
        self.read_only = true;
        self
    }

    pub fn with_read_only(mut self) -> Self {
        self.read_only = true;
        self
    }

    pub fn with_byte_limit(mut self, limit: u64) -> Self {
        self.byte_limit = Some(limit);
        self
    }
}

// ─── CapabilityGrant ─────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CapabilityGrant {
    pub capability: CapabilityId,
    pub scope: CapabilityScope,
    pub constraints: CapabilityConstraints,
}

impl CapabilityGrant {
    pub fn new(capability: CapabilityId) -> Self {
        Self {
            capability,
            scope: CapabilityScope::new(),
            constraints: CapabilityConstraints::new(),
        }
    }

    pub fn with_scope(mut self, scope: CapabilityScope) -> Self {
        self.scope = scope;
        self
    }

    pub fn with_constraints(mut self, c: CapabilityConstraints) -> Self {
        self.constraints = c;
        self
    }

    /// Returns `true` when `child` is a sub-capability of `self`.
    ///
    /// A grant is considered a parent if:
    /// 1. The child capability string equals the parent, OR
    /// 2. The child capability starts with `parent::` (hierarchical), AND
    ///    every non-empty field in child scope is a subset of (or equal to)
    ///    the corresponding field in the parent scope.
    pub fn is_parent_of(&self, child: &CapabilityGrant) -> bool {
        let cap_matches = if self.capability == child.capability {
            true
        } else {
            let prefix = format!("{}::", self.capability);
            child.capability.as_str().starts_with(&prefix)
        };

        if !cap_matches {
            return false;
        }

        scope_is_subset(&child.scope, &self.scope)
    }
}

fn scope_is_subset(child: &CapabilityScope, parent: &CapabilityScope) -> bool {
    all_contained(&child.roots, &parent.roots)
        && all_contained(&child.hosts, &parent.hosts)
        && all_contained(&child.ports, &parent.ports)
        && all_contained(&child.allowed_methods, &parent.allowed_methods)
        && all_contained(&child.variable_names, &parent.variable_names)
}

fn all_contained<T: PartialEq>(children: &[T], parents: &[T]) -> bool {
    if parents.is_empty() {
        return true;
    }
    children.iter().all(|c| parents.contains(c))
}

// ─── CapabilitySet ───────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct CapabilitySet {
    grants: BTreeMap<CapabilityId, CapabilityGrant>,
}

impl CapabilitySet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, grant: CapabilityGrant) {
        self.grants.insert(grant.capability.clone(), grant);
    }

    pub fn has(&self, cap: &CapabilityId) -> bool {
        self.grants.contains_key(cap)
    }

    pub fn get(&self, cap: &CapabilityId) -> Option<&CapabilityGrant> {
        self.grants.get(cap)
    }

    pub fn is_empty(&self) -> bool {
        self.grants.is_empty()
    }

    pub fn len(&self) -> usize {
        self.grants.len()
    }

    pub fn grants(&self) -> impl Iterator<Item = &CapabilityGrant> {
        self.grants.values()
    }

    /// Intersection: for each grant in `parent`, if `self` also has that
    /// capability (or a child of it), take the narrower scope.
    pub fn narrow(&self, parent: &CapabilitySet) -> CapabilitySet {
        let mut result = CapabilitySet::new();
        for pg in parent.grants.values() {
            if let Some(my_grant) = self.grants.get(&pg.capability) {
                if pg.is_parent_of(my_grant) {
                    result.add(my_grant.clone());
                } else {
                    result.add(pg.clone());
                }
            } else if let Some((matching_child, child_grant)) = self.grants.iter().find(|(k, _)| {
                k.as_str()
                    .starts_with(&format!("{}::", pg.capability.as_str()))
            }) {
                let _ = (matching_child, child_grant);
                result.add(child_grant.clone());
            }
        }
        result
    }

    /// Returns `true` when every grant in `self` is a child (or equal) of
    /// the corresponding grant in `other`.
    pub fn subset_of(&self, other: &CapabilitySet) -> bool {
        self.grants.iter().all(|(id, g)| {
            if let Some(parent) = other.grants.get(id) {
                parent.is_parent_of(g)
            } else {
                false
            }
        })
    }
}

// ─── SecurityContextId ───────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SecurityContextId(pub u64);

impl Display for SecurityContextId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SC({})", self.0)
    }
}

// ─── SecurityDecision ────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecurityDecision {
    Allowed,
    Denied { reason: String },
    RequiresApproval { operation: String, reason: String },
}

impl SecurityDecision {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed)
    }

    pub fn is_denied(&self) -> bool {
        matches!(self, Self::Denied { .. })
    }

    pub fn requires_approval(&self) -> bool {
        matches!(self, Self::RequiresApproval { .. })
    }
}

impl Display for SecurityDecision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Allowed => write!(f, "ALLOWED"),
            Self::Denied { reason } => write!(f, "DENIED: {reason}"),
            Self::RequiresApproval { operation, reason } => {
                write!(f, "REQUIRES APPROVAL [{operation}]: {reason}")
            }
        }
    }
}

// ─── SecurityContext ─────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SecurityContext {
    id: SecurityContextId,
    capabilities: CapabilitySet,
    parent_id: Option<SecurityContextId>,
}

impl SecurityContext {
    pub fn new(
        id: SecurityContextId,
        capabilities: CapabilitySet,
        parent_id: Option<SecurityContextId>,
    ) -> Self {
        Self {
            id,
            capabilities,
            parent_id,
        }
    }

    pub fn id(&self) -> SecurityContextId {
        self.id
    }

    pub fn capabilities(&self) -> &CapabilitySet {
        &self.capabilities
    }

    pub fn parent_id(&self) -> Option<SecurityContextId> {
        self.parent_id
    }

    pub fn has_capability(&self, cap: &CapabilityId) -> bool {
        self.capabilities.has(cap)
    }

    pub fn evaluate(&self, cap: &CapabilityId) -> SecurityDecision {
        if self.has_capability(cap) {
            SecurityDecision::Allowed
        } else {
            SecurityDecision::Denied {
                reason: format!("capability '{}' not granted", cap.as_str()),
            }
        }
    }
}

// ─── ApprovalToken ───────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ApprovalToken {
    token_id: u64,
    operation_class: String,
    scope: CapabilityScope,
    _issuer: SecurityContextId,
    expiration: Option<u64>,
    use_count: u32,
    _max_uses: u32,
}

impl ApprovalToken {
    pub fn new(
        token_id: u64,
        operation_class: String,
        scope: CapabilityScope,
        issuer: SecurityContextId,
        expiration: Option<u64>,
        max_uses: u32,
    ) -> Self {
        Self {
            token_id,
            operation_class,
            scope,
            _issuer: issuer,
            expiration,
            use_count: max_uses,
            _max_uses: max_uses,
        }
    }

    pub fn validate(
        &self,
        operation_class: &str,
        scope: &CapabilityScope,
        current_time: u64,
    ) -> bool {
        if self.is_expired(current_time) {
            return false;
        }
        if self.use_count == 0 {
            return false;
        }
        if self.operation_class != operation_class {
            return false;
        }
        scope_is_subset(scope, &self.scope)
    }

    pub fn consume(&mut self) -> bool {
        if self.use_count == 0 {
            return false;
        }
        self.use_count -= 1;
        true
    }

    pub fn is_expired(&self, current_time: u64) -> bool {
        match self.expiration {
            Some(exp) => current_time >= exp,
            None => false,
        }
    }

    pub fn id(&self) -> u64 {
        self.token_id
    }

    pub fn operation_class(&self) -> &str {
        &self.operation_class
    }
}

// ─── SecurityDenial ──────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SecurityDenial {
    pub code: &'static str,
    pub message: String,
    pub context_id: SecurityContextId,
}

impl fmt::Display for SecurityDenial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[NEXA-SEC-{}] {}", self.code, self.message)
    }
}

// ─── DataClassification ──────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DataClassification {
    Untrusted,
    Public,
    Private,
    Sensitive,
    Secret,
}

// ─── SecurityKernel ──────────────────────────────────────────────

pub struct SecurityKernel {
    next_context_id: u64,
    next_token_id: u64,
    contexts: BTreeMap<SecurityContextId, SecurityContext>,
}

impl SecurityKernel {
    pub fn new() -> Self {
        Self {
            next_context_id: 1,
            next_token_id: 1,
            contexts: BTreeMap::new(),
        }
    }

    fn alloc_context_id(&mut self) -> SecurityContextId {
        let id = SecurityContextId(self.next_context_id);
        self.next_context_id += 1;
        id
    }

    fn alloc_token_id(&mut self) -> u64 {
        let id = self.next_token_id;
        self.next_token_id += 1;
        id
    }

    pub fn create_root_context(&mut self, capabilities: CapabilitySet) -> SecurityContext {
        let id = self.alloc_context_id();
        let ctx = SecurityContext::new(id, capabilities, None);
        self.contexts.insert(id, ctx.clone());
        ctx
    }

    pub fn derive_child(
        &mut self,
        parent_id: SecurityContextId,
        child_capabilities: CapabilitySet,
    ) -> Result<SecurityContext, RuntimeError> {
        let parent = self
            .contexts
            .get(&parent_id)
            .ok_or(RuntimeError::ParentNotFound { id: parent_id.0 })?;

        // Verify child doesn't request capabilities beyond parent
        if !child_capabilities.subset_of(parent.capabilities()) {
            return Err(RuntimeError::AuthorityEscalation {
                detail: "child capabilities exceed parent authority".into(),
            });
        }

        let narrowed = child_capabilities.narrow(parent.capabilities());

        let id = self.alloc_context_id();
        let child = SecurityContext::new(id, narrowed, Some(parent_id));
        self.contexts.insert(id, child.clone());
        Ok(child)
    }

    pub fn evaluate(
        &self,
        context_id: SecurityContextId,
        capability: &CapabilityId,
    ) -> SecurityDecision {
        match self.contexts.get(&context_id) {
            Some(ctx) => ctx.evaluate(capability),
            None => SecurityDecision::Denied {
                reason: format!("context {} not found", context_id.0),
            },
        }
    }

    pub fn issue_approval(
        &mut self,
        issuer_id: SecurityContextId,
        operation_class: &str,
        scope: CapabilityScope,
        max_uses: u32,
        expiration: Option<u64>,
    ) -> Result<ApprovalToken, RuntimeError> {
        if !self.contexts.contains_key(&issuer_id) {
            return Err(RuntimeError::InvalidContext { id: issuer_id.0 });
        }
        let token_id = self.alloc_token_id();
        Ok(ApprovalToken::new(
            token_id,
            operation_class.to_string(),
            scope,
            issuer_id,
            expiration,
            max_uses,
        ))
    }

    pub fn validate_approval(
        &self,
        token: &ApprovalToken,
        operation_class: &str,
        scope: &CapabilityScope,
        current_time: u64,
    ) -> bool {
        token.validate(operation_class, scope, current_time)
    }

    pub fn context_count(&self) -> usize {
        self.contexts.len()
    }

    pub fn get_context(&self, id: SecurityContextId) -> Option<&SecurityContext> {
        self.contexts.get(&id)
    }
}

impl Default for SecurityKernel {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_id_creation() {
        let cap = CapabilityId::new("filesystem::read");
        assert_eq!(cap.as_str(), "filesystem::read");
        assert_eq!(format!("{cap}"), "filesystem::read");
    }

    #[test]
    fn test_capability_scope_builder() {
        let scope = CapabilityScope::new()
            .with_root("/tmp")
            .with_host("example.com")
            .with_port(443)
            .with_method("GET")
            .with_variable("HOME");

        assert_eq!(scope.roots, vec!["/tmp"]);
        assert_eq!(scope.hosts, vec!["example.com"]);
        assert_eq!(scope.ports, vec![443]);
        assert_eq!(scope.allowed_methods, vec!["GET"]);
        assert_eq!(scope.variable_names, vec!["HOME"]);
        assert!(!scope.is_empty());

        assert!(CapabilityScope::new().is_empty());
    }

    #[test]
    fn test_capability_constraints_read_only() {
        let c = CapabilityConstraints::new()
            .with_read_only()
            .with_byte_limit(1024);
        assert!(c.read_only);
        assert_eq!(c.byte_limit, Some(1024));
    }

    #[test]
    fn test_capability_grant_is_parent_of() {
        let parent = CapabilityGrant::new(CapabilityId::new("filesystem"));

        let child_exact = CapabilityGrant::new(CapabilityId::new("filesystem"));
        assert!(parent.is_parent_of(&child_exact));

        let child_sub = CapabilityGrant::new(CapabilityId::new("filesystem::read"));
        assert!(parent.is_parent_of(&child_sub));

        let unrelated = CapabilityGrant::new(CapabilityId::new("console"));
        assert!(!parent.is_parent_of(&unrelated));
    }

    #[test]
    fn test_capability_set_add_has() {
        let mut set = CapabilitySet::new();
        assert!(!set.has(&CapabilityId::new("filesystem")));

        set.add(CapabilityGrant::new(CapabilityId::new("filesystem")));
        assert!(set.has(&CapabilityId::new("filesystem")));
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn test_capability_set_is_empty() {
        let set = CapabilitySet::new();
        assert!(set.is_empty());
        assert_eq!(set.len(), 0);
    }

    #[test]
    fn test_capability_set_narrow() {
        let mut parent = CapabilitySet::new();
        parent.add(CapabilityGrant::new(CapabilityId::new("filesystem")));
        parent.add(CapabilityGrant::new(CapabilityId::new("console")));

        let mut child = CapabilitySet::new();
        child.add(CapabilityGrant::new(CapabilityId::new("filesystem")));
        child.add(CapabilityGrant::new(CapabilityId::new("network")));

        let narrowed = child.narrow(&parent);
        assert!(narrowed.has(&CapabilityId::new("filesystem")));
        assert!(!narrowed.has(&CapabilityId::new("network")));
        assert!(!narrowed.has(&CapabilityId::new("console")));
    }

    #[test]
    fn test_capability_set_subset_of() {
        let mut parent = CapabilitySet::new();
        parent.add(CapabilityGrant::new(CapabilityId::new("filesystem")));
        parent.add(CapabilityGrant::new(CapabilityId::new("console")));

        let mut child = CapabilitySet::new();
        child.add(CapabilityGrant::new(CapabilityId::new("filesystem")));
        assert!(child.subset_of(&parent));

        let mut unrelated = CapabilitySet::new();
        unrelated.add(CapabilityGrant::new(CapabilityId::new("network")));
        assert!(!unrelated.subset_of(&parent));
    }

    #[test]
    fn test_security_context_creation() {
        let mut caps = CapabilitySet::new();
        caps.add(CapabilityGrant::new(CapabilityId::new("filesystem")));

        let ctx = SecurityContext::new(SecurityContextId(1), caps, None);
        assert_eq!(ctx.id().0, 1);
        assert!(ctx.parent_id().is_none());
    }

    #[test]
    fn test_security_context_has_capability() {
        let mut caps = CapabilitySet::new();
        caps.add(CapabilityGrant::new(CapabilityId::new("filesystem")));

        let ctx = SecurityContext::new(SecurityContextId(1), caps, None);
        assert!(ctx.has_capability(&CapabilityId::new("filesystem")));
        assert!(!ctx.has_capability(&CapabilityId::new("console")));
    }

    #[test]
    fn test_security_decision_allowed() {
        let d = SecurityDecision::Allowed;
        assert!(d.is_allowed());
        assert!(!d.is_denied());
        assert!(!d.requires_approval());
        assert_eq!(format!("{d}"), "ALLOWED");
    }

    #[test]
    fn test_security_decision_denied() {
        let d = SecurityDecision::Denied {
            reason: "no access".into(),
        };
        assert!(!d.is_allowed());
        assert!(d.is_denied());
        assert!(format!("{d}").contains("DENIED"));
    }

    #[test]
    fn test_security_kernel_create_root() {
        let mut kernel = SecurityKernel::new();
        let mut caps = CapabilitySet::new();
        caps.add(CapabilityGrant::new(CapabilityId::new("filesystem")));

        let ctx = kernel.create_root_context(caps);
        assert_eq!(kernel.context_count(), 1);
        assert!(ctx.has_capability(&CapabilityId::new("filesystem")));
        assert!(ctx.parent_id().is_none());
    }

    #[test]
    fn test_security_kernel_derive_child() {
        let mut kernel = SecurityKernel::new();

        let mut parent_caps = CapabilitySet::new();
        parent_caps.add(CapabilityGrant::new(CapabilityId::new("filesystem")));
        parent_caps.add(CapabilityGrant::new(CapabilityId::new("console")));

        let parent = kernel.create_root_context(parent_caps);

        let mut child_caps = CapabilitySet::new();
        child_caps.add(CapabilityGrant::new(CapabilityId::new("filesystem")));

        let child = kernel.derive_child(parent.id(), child_caps).unwrap();
        assert_eq!(kernel.context_count(), 2);
        assert!(child.has_capability(&CapabilityId::new("filesystem")));
        assert!(!child.has_capability(&CapabilityId::new("console")));
        assert_eq!(child.parent_id(), Some(parent.id()));
    }

    #[test]
    fn test_security_kernel_child_escalation_rejected() {
        let mut kernel = SecurityKernel::new();

        let mut parent_caps = CapabilitySet::new();
        parent_caps.add(CapabilityGrant::new(CapabilityId::new("filesystem")));

        let parent = kernel.create_root_context(parent_caps);

        let mut child_caps = CapabilitySet::new();
        child_caps.add(CapabilityGrant::new(CapabilityId::new("network")));

        let result = kernel.derive_child(parent.id(), child_caps);
        assert!(result.is_err());
        match result.unwrap_err() {
            RuntimeError::AuthorityEscalation { .. } => {}
            other => panic!("expected AuthorityEscalation, got {other:?}"),
        }
    }

    #[test]
    fn test_security_kernel_evaluate() {
        let mut kernel = SecurityKernel::new();
        let mut caps = CapabilitySet::new();
        caps.add(CapabilityGrant::new(CapabilityId::new("filesystem")));

        let ctx = kernel.create_root_context(caps);
        let allowed = kernel.evaluate(ctx.id(), &CapabilityId::new("filesystem"));
        assert!(allowed.is_allowed());

        let denied = kernel.evaluate(ctx.id(), &CapabilityId::new("network"));
        assert!(denied.is_denied());
    }

    #[test]
    fn test_approval_token_validate() {
        let mut kernel = SecurityKernel::new();
        let caps = CapabilitySet::new();
        let issuer = kernel.create_root_context(caps);

        let scope = CapabilityScope::new().with_root("/tmp");
        let token = kernel
            .issue_approval(issuer.id(), "file:write", scope.clone(), 5, None)
            .unwrap();

        assert!(token.validate("file:write", &scope, 100));
        assert!(!token.validate("file:read", &scope, 100));
    }

    #[test]
    fn test_approval_token_consume() {
        let mut kernel = SecurityKernel::new();
        let caps = CapabilitySet::new();
        let issuer = kernel.create_root_context(caps);

        let scope = CapabilityScope::new();
        let mut token = kernel
            .issue_approval(issuer.id(), "op", scope, 2, None)
            .unwrap();

        assert!(token.consume());
        assert!(token.consume());
        assert!(!token.consume());
    }

    #[test]
    fn test_approval_token_exhausted() {
        let mut kernel = SecurityKernel::new();
        let caps = CapabilitySet::new();
        let issuer = kernel.create_root_context(caps);

        let scope = CapabilityScope::new();
        let mut token = kernel
            .issue_approval(issuer.id(), "op", scope.clone(), 1, None)
            .unwrap();

        assert!(token.consume());
        // exhausted – validate should fail
        assert!(!token.validate("op", &scope, 0));
    }

    #[test]
    fn test_approval_token_expired() {
        let mut kernel = SecurityKernel::new();
        let caps = CapabilitySet::new();
        let issuer = kernel.create_root_context(caps);

        let scope = CapabilityScope::new();
        let token = kernel
            .issue_approval(issuer.id(), "op", scope.clone(), 5, Some(100))
            .unwrap();

        assert!(!token.validate("op", &scope, 200));
        assert!(token.is_expired(200));
        assert!(!token.is_expired(50));
    }

    #[test]
    fn test_data_classification_ordering() {
        assert!(DataClassification::Public < DataClassification::Private);
        assert!(DataClassification::Private < DataClassification::Sensitive);
        assert!(DataClassification::Sensitive < DataClassification::Secret);
        assert!(DataClassification::Untrusted < DataClassification::Public);
    }

    #[test]
    fn test_security_denial_display() {
        let denial = SecurityDenial {
            code: "1001",
            message: "capability not granted".into(),
            context_id: SecurityContextId(42),
        };
        let msg = format!("{denial}");
        assert!(msg.contains("NEXA-SEC-"));
        assert!(msg.contains("1001"));
        assert!(msg.contains("capability not granted"));
    }
}
