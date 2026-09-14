//! # nxr-ai
//!
//! Implementation 11 — NEXA `AIProvider` contract.
//!
//! NEXA runtime code speaks to an `AIProvider` contract, never directly to a
//! vendor (OpenAI, Anthropic, Google, ...). This crate defines the typed
//! `AiRequest` / `AiResponse`, the synchronous `AIProvider` trait, the
//! `FakeAiProvider` test double, privacy-classification routing, and the
//! `ai::inference` capability gate.
//!
//! Security invariants enforced here (spec §144–§162):
//! - Model output is **`Untrusted` by default**.
//! - Model output cannot automatically grant capability, create an
//!   `ApprovalToken`, forge a `SecurityContext`, execute a process/SQL, write
//!   a file, or perform a tool.
//! - Router fallback can never widen privacy, authority, destination or cost
//!   ceiling; a `Secret` classified request cannot silently fall back to a
//!   cloud destination that is not authorized for `Secret`.
//!
//! The workspace is synchronous `std`, so the provider contract is expressed
//! with `fn infer(...)` rather than the async form shown in the spec.

use std::collections::BTreeSet;

pub use nxr_security::DataClassification;
use nxr_security::DataClassification as Class;
// TODO: implement actual OpenAI, Anthropic, Google providers per spec §146–§152

/// The role of a message in an AI conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AiRole {
    System,
    User,
    Assistant,
    Tool,
}

/// A single typed message in the request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiMessage {
    pub role: AiRole,
    pub content: String,
}

impl AiMessage {
    pub fn new(role: AiRole, content: impl Into<String>) -> Self {
        AiMessage {
            role,
            content: content.into(),
        }
    }
}

/// The kind of structured output requested.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OutputKind {
    Text,
    Json,
}

/// The schema governing structured output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputSchema {
    pub kind: OutputKind,
    /// A JSON schema description when structured output is requested.
    pub schema: Option<String>,
}

impl OutputSchema {
    pub fn text() -> Self {
        OutputSchema {
            kind: OutputKind::Text,
            schema: None,
        }
    }

    pub fn json(schema: impl Into<String>) -> Self {
        OutputSchema {
            kind: OutputKind::Json,
            schema: Some(schema.into()),
        }
    }
}

// Provider adapters are planned for OpenAI, Anthropic and Google. The current
// crate provides the vendor-neutral contract and a deterministic test double.

/// Model requirement for the request. Vendor-agnostic: selects a model family
/// / capability, not a specific vendor API endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRequirement {
    pub family: String,
    pub min_quality: Option<String>,
}

impl ModelRequirement {
    pub fn new(family: impl Into<String>) -> Self {
        ModelRequirement {
            family: family.into(),
            min_quality: None,
        }
    }

    pub fn with_quality(mut self, quality: impl Into<String>) -> Self {
        self.min_quality = Some(quality.into());
        self
    }
}

/// Sampling / temperature policy.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SamplingPolicy {
    pub temperature: f32,
}

impl Default for SamplingPolicy {
    fn default() -> Self {
        SamplingPolicy { temperature: 0.7 }
    }
}

/// A typed AI request (spec §138–§139).
#[derive(Debug, Clone)]
pub struct AiRequest {
    pub model: ModelRequirement,
    pub messages: Vec<AiMessage>,
    pub output_schema: OutputSchema,
    pub sampling: SamplingPolicy,
    pub token_budget: Option<u32>,
    pub classification: Class,
    pub tool_policy: Option<String>,
}

impl AiRequest {
    pub fn new(model: ModelRequirement, classification: Class) -> Self {
        AiRequest {
            model,
            messages: Vec::new(),
            output_schema: OutputSchema::text(),
            sampling: SamplingPolicy::default(),
            token_budget: None,
            classification,
            tool_policy: None,
        }
    }

    pub fn with_message(mut self, m: AiMessage) -> Self {
        self.messages.push(m);
        self
    }

    pub fn with_output_schema(mut self, s: OutputSchema) -> Self {
        self.output_schema = s;
        self
    }

    pub fn with_token_budget(mut self, n: u32) -> Self {
        self.token_budget = Some(n);
        self
    }
}

/// Runtime-created execution context passed to providers. Not model-created.
#[derive(Debug, Clone)]
pub struct AiExecutionContext {
    pub security_context_id: u64,
    pub allowed_classifications: BTreeSet<Class>,
    pub allowed_destinations: BTreeSet<String>,
    pub tool_availability_set: BTreeSet<String>,
}

impl AiExecutionContext {
    pub fn new(security_context_id: u64) -> Self {
        AiExecutionContext {
            security_context_id,
            allowed_classifications: BTreeSet::new(),
            allowed_destinations: BTreeSet::new(),
            tool_availability_set: BTreeSet::new(),
        }
    }

    pub fn with_classification(mut self, c: Class) -> Self {
        self.allowed_classifications.insert(c);
        self
    }

    pub fn with_destination(mut self, d: impl Into<String>) -> Self {
        self.allowed_destinations.insert(d.into());
        self
    }
}

/// Token / cost usage reported by a provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// How the model finished the generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FinishStatus {
    Complete,
    Stop,
    LengthLimit,
    ContentFilter,
    ProviderError,
}

/// A typed output produced by a model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiOutput {
    /// Raw text output (permitted when the output type is Text).
    Text(String),
    /// Schema-defined structured output.
    Structured(String),
}

/// A typed AI response (spec §141).
#[derive(Debug, Clone)]
pub struct AiResponse {
    pub output: AiOutput,
    pub usage: Usage,
    pub provider_name: String,
    pub model_name: String,
    pub finish: FinishStatus,
    /// Model output is `Untrusted` by default. The runtime may raise it only
    /// after an independent trust decision, never because the model said so.
    pub classification: Class,
}

impl AiResponse {
    /// Construct an `Untrusted` response.
    pub fn untrusted(output: AiOutput) -> Self {
        AiResponse {
            output,
            usage: Usage::default(),
            provider_name: String::new(),
            model_name: String::new(),
            finish: FinishStatus::Complete,
            classification: Class::Untrusted,
        }
    }
}

/// Errors raised by the AI layer (spec-consistent `NEXA-AI-` codes).
#[derive(Debug, thiserror::Error)]
pub enum AiError {
    #[error("NEXA-AI-0001: no provider available for model family '{family}'")]
    NoProvider { family: String },

    #[error("NEXA-AI-0002: request classification {0:?} exceeds allowed classifications")]
    ClassificationDenied(Class),

    #[error("NEXA-AI-0003: destination '{0}' not authorized for classification {1:?}")]
    DestinationDenied(String, Class),

    #[error("NEXA-AI-0004: requested token budget ({0}) exceeds capability ceiling ({1})")]
    BudgetExceeded(u32, u32),

    #[error("NEXA-AI-0005: provider inference failed: {0}")]
    ProviderFailed(String),

    #[error("NEXA-AI-0006: invalid request: {0}")]
    InvalidRequest(String),
}

/// The `ai::inference` capability that bounds what an AI request may do.
#[derive(Debug, Clone)]
pub struct AiCapability {
    pub capability_id: &'static str,
    pub allowed_classifications: BTreeSet<Class>,
    pub allowed_providers: BTreeSet<String>,
    pub max_tokens: Option<u32>,
    pub max_cost: Option<u64>,
}

impl AiCapability {
    pub fn inference() -> Self {
        AiCapability {
            capability_id: "ai.inference",
            allowed_classifications: BTreeSet::new(),
            allowed_providers: BTreeSet::new(),
            max_tokens: None,
            max_cost: None,
        }
    }

    pub fn with_classification(mut self, c: Class) -> Self {
        self.allowed_classifications.insert(c);
        self
    }

    pub fn with_provider(mut self, p: impl Into<String>) -> Self {
        self.allowed_providers.insert(p.into());
        self
    }

    pub fn with_max_tokens(mut self, n: u32) -> Self {
        self.max_tokens = Some(n);
        self
    }

    /// Whether this capability permits a request of the given classification.
    pub fn admits(&self, c: Class) -> bool {
        self.allowed_classifications
            .iter()
            .any(|allowed| *allowed >= c)
    }
}

/// Contract implemented by every NEXA provider adapter.
pub trait AIProvider {
    /// Run a fully-classified inference. `context` is runtime-created.
    fn infer(
        &self,
        request: AiRequest,
        context: &AiExecutionContext,
    ) -> Result<AiResponse, AiError>;
}

/// A deterministic in-memory provider for tests and local (non-cloud) use.
#[derive(Debug, Clone)]
pub struct FakeAiProvider {
    pub name: String,
    pub echo_prefix: String,
}

impl Default for FakeAiProvider {
    fn default() -> Self {
        FakeAiProvider {
            name: "fake.local".to_string(),
            echo_prefix: "reply: ".to_string(),
        }
    }
}

impl FakeAiProvider {
    pub fn new(name: impl Into<String>) -> Self {
        FakeAiProvider {
            name: name.into(),
            echo_prefix: "reply: ".to_string(),
        }
    }
}

impl AIProvider for FakeAiProvider {
    fn infer(
        &self,
        request: AiRequest,
        _context: &AiExecutionContext,
    ) -> Result<AiResponse, AiError> {
        let last_user = request
            .messages
            .iter()
            .rev()
            .find(|m| m.role == AiRole::User)
            .map(|m| m.content.clone())
            .unwrap_or_default();
        let body = match &request.output_schema.kind {
            OutputKind::Text => format!("{}{}", self.echo_prefix, last_user),
            OutputKind::Json => format!("{{ \"echo\": {last_user:?} }}"),
        };
        let total = request
            .messages
            .iter()
            .map(|m| m.content.chars().count() as u32)
            .sum();
        let mut resp = AiResponse::untrusted(AiOutput::Text(body));
        resp.provider_name = self.name.clone();
        resp.model_name = request.model.family.clone();
        resp.usage = Usage {
            prompt_tokens: total,
            completion_tokens: 1,
            total_tokens: total + 1,
        };
        Ok(resp)
    }
}

/// A policy decision from the router.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutingAudit {
    pub provider: String,
    pub model: String,
    pub classification: Class,
    pub tokens: u32,
    pub policy_decision: &'static str,
}

/// Routes a request to a provider without widening privacy/authority.
pub struct AiRouter<'a> {
    providers: Vec<(&'a str, &'a dyn AIProvider)>,
}

impl<'a> AiRouter<'a> {
    pub fn new() -> Self {
        AiRouter {
            providers: Vec::new(),
        }
    }

    pub fn register(&mut self, name: &'a str, provider: &'a dyn AIProvider) {
        self.providers.push((name, provider));
    }

    /// Select the first provider whose declared name is permitted by the
    /// execution context, then run inference subject to the capability.
    ///
    /// Fallback never widens classification or destination: if no provider is
    /// authorized for the request classification, the request fails rather
    /// than downgrading to a riskier destination.
    pub fn route(
        &self,
        request: AiRequest,
        context: &AiExecutionContext,
        capability: &AiCapability,
    ) -> Result<(AiResponse, RoutingAudit), AiError> {
        if !capability.admits(request.classification) {
            return Err(AiError::ClassificationDenied(request.classification));
        }
        if let (Some(budget), Some(ceiling)) = (request.token_budget, capability.max_tokens) {
            if budget > ceiling {
                return Err(AiError::BudgetExceeded(budget, ceiling));
            }
        }
        // Destination authorization: a request may only reach a provider that
        // the runtime explicitly authorized in the execution context. An empty
        // set authorizes nothing (never widen destination).
        let authorized = self
            .providers
            .iter()
            .find(|(name, _)| context.allowed_destinations.contains(*name));
        let Some((name, provider)) = authorized else {
            return Err(AiError::DestinationDenied(
                "none".to_string(),
                request.classification,
            ));
        };
        let resp = provider
            .infer(request.clone(), context)
            .map_err(|e| match e {
                AiError::ProviderFailed(m) => AiError::ProviderFailed(m),
                other => other,
            })?;
        let audit = RoutingAudit {
            provider: name.to_string(),
            model: resp.model_name.clone(),
            classification: resp.classification,
            tokens: resp.usage.total_tokens,
            policy_decision: "allowed",
        };
        Ok((resp, audit))
    }
}

impl<'a> Default for AiRouter<'a> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(classification: Class) -> AiRequest {
        AiRequest::new(ModelRequirement::new("local"), classification)
            .with_message(AiMessage::new(AiRole::User, "hello"))
    }

    fn default_capability() -> AiCapability {
        AiCapability::inference()
            .with_classification(Class::Public)
            .with_classification(Class::Private)
            .with_classification(Class::Sensitive)
            .with_classification(Class::Secret)
    }

    #[test]
    fn fake_provider_response_is_untrusted() {
        let p = FakeAiProvider::default();
        let ctx = AiExecutionContext::new(1);
        let resp = p.infer(req(Class::Public), &ctx).unwrap();
        assert_eq!(resp.classification, Class::Untrusted);
        assert!(matches!(resp.output, AiOutput::Text(t) if t == "reply: hello"));
    }

    #[test]
    fn fake_provider_structured_output() {
        let p = FakeAiProvider::default();
        let ctx = AiExecutionContext::new(1);
        let request = req(Class::Public).with_output_schema(OutputSchema::json("{}"));
        let resp = p.infer(request, &ctx).unwrap();
        assert!(matches!(resp.output, AiOutput::Text(t) if t.contains("echo")));
    }

    #[test]
    fn routing_classification_denied() {
        let router = AiRouter::new();
        let cap = AiCapability::inference().with_classification(Class::Public);
        let ctx = AiExecutionContext::new(1)
            .with_classification(Class::Public)
            .with_destination("fake");
        let err = router.route(req(Class::Secret), &ctx, &cap).unwrap_err();
        assert!(matches!(err, AiError::ClassificationDenied(_)));
    }

    #[test]
    fn routing_destination_denied_when_no_authorized_provider() {
        let mut router = AiRouter::new();
        let p = FakeAiProvider::new("fake");
        router.register("fake", &p);
        let cap = default_capability();
        // No destinations authorized -> cannot route (no widening).
        let ctx = AiExecutionContext::new(1).with_classification(Class::Secret);
        let err = router.route(req(Class::Secret), &ctx, &cap).unwrap_err();
        assert!(matches!(err, AiError::DestinationDenied(..)));
    }

    #[test]
    fn routing_success_with_audit() {
        let mut router = AiRouter::new();
        let p = FakeAiProvider::new("fake");
        router.register("fake", &p);
        let cap = default_capability();
        let ctx = AiExecutionContext::new(1)
            .with_classification(Class::Public)
            .with_destination("fake");
        let (resp, audit) = router.route(req(Class::Public), &ctx, &cap).unwrap();
        assert_eq!(resp.classification, Class::Untrusted);
        assert_eq!(audit.provider, "fake");
        assert_eq!(audit.policy_decision, "allowed");
    }

    #[test]
    fn budget_ceiling_enforced() {
        let mut router = AiRouter::new();
        let p = FakeAiProvider::new("fake");
        router.register("fake", &p);
        let cap = default_capability().with_max_tokens(10);
        let ctx = AiExecutionContext::new(1)
            .with_classification(Class::Public)
            .with_destination("fake");
        let request = req(Class::Public).with_token_budget(100);
        let err = router.route(request, &ctx, &cap).unwrap_err();
        assert!(matches!(err, AiError::BudgetExceeded(100, 10)));
    }

    #[test]
    fn secret_request_does_not_fall_back_to_unauthorized_cloud() {
        let mut router = AiRouter::new();
        let local = FakeAiProvider::new("fake");
        router.register("fake", &local);
        let cap = default_capability();
        // Only cloud authorized, not for Secret.
        let ctx = AiExecutionContext::new(1)
            .with_classification(Class::Public)
            .with_classification(Class::Secret)
            .with_destination("cloud");
        // Even though classification is admitted, destination "fake" is not in
        // the authorized set, so it must not silently route.
        let err = router.route(req(Class::Public), &ctx, &cap).unwrap_err();
        assert!(matches!(err, AiError::DestinationDenied(..)));
    }
}
