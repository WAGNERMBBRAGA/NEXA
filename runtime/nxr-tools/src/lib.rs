//! # nxr-tools
//!
//! Implementation 11 — NEXA **typed tools** registry and schema-validated
//! invocation.
//!
//! An agent does not receive "all available functions": it receives an
//! explicit `ToolSet` of typed tools. Each tool is described by a
//! [`ToolDescriptor`] (input/output schema, authority requirement, approval
//! policy) and implemented through the [`Tool`] trait. Dispatch goes through
//! [`ToolRegistry`] / [`ToolExecutor`]; the model never calls a provider
//! directly, and a model-generated tool name is an *untrusted selector*.
//!
//! The registered invocation flow (spec §189) is enforced step-by-step:
//! schema validation → registered-ID membership → agent permission (capability)
//! → tool-specific authority → `SecurityContext` capability → budget → approval
//! → execute → audit → typed result. No step may be skipped.

use std::collections::BTreeMap;
use std::sync::Arc;

use nxr_security::ApprovalToken;
use nxr_security::CapabilityId;
pub use nxr_security::DataClassification;
use nxr_security::DataClassification as Class;

/// Opaque identity of a typed tool.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
pub struct ToolId(pub String);

impl ToolId {
    pub fn new(name: impl Into<String>) -> Self {
        ToolId(name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ToolId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The primitive type of a tool parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PrimitiveType {
    String,
    Integer,
    Float,
    Boolean,
    Path,
    Json,
}

/// The declared type of a schema parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemaType {
    Primitive(PrimitiveType),
    Array(Box<SchemaType>),
}

impl SchemaType {
    pub fn string() -> Self {
        SchemaType::Primitive(PrimitiveType::String)
    }

    pub fn integer() -> Self {
        SchemaType::Primitive(PrimitiveType::Integer)
    }

    pub fn float() -> Self {
        SchemaType::Primitive(PrimitiveType::Float)
    }

    pub fn boolean() -> Self {
        SchemaType::Primitive(PrimitiveType::Boolean)
    }

    pub fn path() -> Self {
        SchemaType::Primitive(PrimitiveType::Path)
    }

    pub fn json() -> Self {
        SchemaType::Primitive(PrimitiveType::Json)
    }
}

impl std::fmt::Display for SchemaType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SchemaType::Primitive(p) => f.write_str(match p {
                PrimitiveType::String => "string",
                PrimitiveType::Integer => "integer",
                PrimitiveType::Float => "float",
                PrimitiveType::Boolean => "boolean",
                PrimitiveType::Path => "path",
                PrimitiveType::Json => "json",
            }),
            SchemaType::Array(inner) => write!(f, "{inner}[]"),
        }
    }
}

/// A single declared parameter of a tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolParamSchema {
    pub name: String,
    pub ty: SchemaType,
    pub required: bool,
    pub description: Option<String>,
}

impl ToolParamSchema {
    pub fn new(name: impl Into<String>, ty: SchemaType) -> Self {
        ToolParamSchema {
            name: name.into(),
            ty,
            required: true,
            description: None,
        }
    }

    pub fn optional(mut self) -> Self {
        self.required = false;
        self
    }

    pub fn with_description(mut self, d: impl Into<String>) -> Self {
        self.description = Some(d.into());
        self
    }
}

/// The typed input or output schema of a tool.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ToolSchema {
    pub fields: Vec<ToolParamSchema>,
}

impl ToolSchema {
    pub fn new() -> Self {
        ToolSchema { fields: Vec::new() }
    }

    pub fn field(mut self, f: ToolParamSchema) -> Self {
        self.fields.push(f);
        self
    }

    /// Validate arguments against this schema. Missing required fields and
    /// type-incoherent values are rejected before execution.
    pub fn validate(&self, args: &BTreeMap<String, String>) -> Result<(), ToolError> {
        let declared: BTreeMap<&str, &ToolParamSchema> =
            self.fields.iter().map(|f| (f.name.as_str(), f)).collect();
        for field in &self.fields {
            if field.required && !args.contains_key(&field.name) {
                return Err(ToolError::MissingArgument(field.name.clone()));
            }
        }
        for (key, value) in args {
            match declared.get(key.as_str()) {
                None => {
                    return Err(ToolError::UnknownArgument(key.clone()));
                }
                Some(field) => validate_value(&field.ty, key, value)?,
            }
        }
        Ok(())
    }
}

fn validate_value(ty: &SchemaType, key: &str, value: &str) -> Result<(), ToolError> {
    match ty {
        SchemaType::Primitive(p) => match p {
            PrimitiveType::String | PrimitiveType::Path | PrimitiveType::Json => Ok(()),
            PrimitiveType::Integer => {
                value
                    .parse::<i64>()
                    .map(|_| ())
                    .map_err(|_| ToolError::TypeMismatch {
                        argument: key.to_string(),
                        expected: ty.to_string(),
                    })
            }
            PrimitiveType::Float => {
                value
                    .parse::<f64>()
                    .map(|_| ())
                    .map_err(|_| ToolError::TypeMismatch {
                        argument: key.to_string(),
                        expected: ty.to_string(),
                    })
            }
            PrimitiveType::Boolean => match value {
                "true" | "false" => Ok(()),
                _ => Err(ToolError::TypeMismatch {
                    argument: key.to_string(),
                    expected: ty.to_string(),
                }),
            },
        },
        SchemaType::Array(_) => serde_json::from_str::<serde_json::Value>(value)
            .map(|_| ())
            .map_err(|_| ToolError::TypeMismatch {
                argument: key.to_string(),
                expected: ty.to_string(),
            }),
    }
}

/// The authority (minimum capability) a tool requires on the calling context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolAuthorityRequirement {
    pub capability_id: CapabilityId,
}

impl ToolAuthorityRequirement {
    pub fn new(capability_id: CapabilityId) -> Self {
        ToolAuthorityRequirement { capability_id }
    }
}

/// When a tool requires human/runtime approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum ApprovalPolicy {
    /// Always requires a runtime-issued `ApprovalToken`.
    Always,
    /// Approval is conditional (policy decides per invocation).
    Conditional,
    /// No approval gate.
    #[default]
    Never,
}

/// Complete description of a typed tool (spec §184).
#[derive(Debug, Clone)]
pub struct ToolDescriptor {
    pub id: ToolId,
    pub input_schema: ToolSchema,
    pub output_schema: ToolSchema,
    pub authority: ToolAuthorityRequirement,
    pub approval: ApprovalPolicy,
    /// Optional root for path containment (defense in depth).
    pub path_root: Option<String>,
}

impl ToolDescriptor {
    pub fn new(id: ToolId) -> Self {
        ToolDescriptor {
            id,
            input_schema: ToolSchema::new(),
            output_schema: ToolSchema::new(),
            authority: ToolAuthorityRequirement::new(CapabilityId::new("none")),
            approval: ApprovalPolicy::Never,
            path_root: None,
        }
    }

    pub fn with_input(mut self, schema: ToolSchema) -> Self {
        self.input_schema = schema;
        self
    }

    pub fn with_output(mut self, schema: ToolSchema) -> Self {
        self.output_schema = schema;
        self
    }

    pub fn with_authority(mut self, capability_id: CapabilityId) -> Self {
        self.authority = ToolAuthorityRequirement::new(capability_id);
        self
    }

    pub fn with_approval(mut self, policy: ApprovalPolicy) -> Self {
        self.approval = policy;
        self
    }

    pub fn with_path_root(mut self, root: impl Into<String>) -> Self {
        self.path_root = Some(root.into());
        self
    }
}

/// Typed, validated tool input. Arguments originate from untrusted model
/// output and are validated against the schema before execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolInput {
    pub tool_id: ToolId,
    pub arguments: BTreeMap<String, String>,
    /// Classification of the request that produced this input.
    pub classification: Class,
}

impl ToolInput {
    pub fn new(tool_id: ToolId) -> Self {
        ToolInput {
            tool_id,
            arguments: BTreeMap::new(),
            classification: Class::Untrusted,
        }
    }

    pub fn with_arg(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.arguments.insert(name.into(), value.into());
        self
    }

    pub fn with_classification(mut self, c: Class) -> Self {
        self.classification = c;
        self
    }
}

/// A classified tool result that flows back to the agent/model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolOutput {
    pub result: String,
    pub classification: Class,
}

impl ToolOutput {
    pub fn new(result: impl Into<String>, classification: Class) -> Self {
        ToolOutput {
            result: result.into(),
            classification,
        }
    }
}

/// Runtime-created execution context for a tool. Never model-created.
#[derive(Debug, Clone)]
pub struct ToolExecutionContext {
    pub security_context_id: u64,
    pub granted_capabilities: Vec<CapabilityId>,
    pub approval: Option<ApprovalToken>,
    pub now: u64,
    pub allowed_classifications: Vec<Class>,
}

impl ToolExecutionContext {
    pub fn new(security_context_id: u64) -> Self {
        ToolExecutionContext {
            security_context_id,
            granted_capabilities: Vec::new(),
            approval: None,
            now: 0,
            allowed_classifications: Vec::new(),
        }
    }

    pub fn with_capability(mut self, c: CapabilityId) -> Self {
        self.granted_capabilities.push(c);
        self
    }

    pub fn with_approval(mut self, t: ApprovalToken) -> Self {
        self.approval = Some(t);
        self
    }

    pub fn with_classification(mut self, c: Class) -> Self {
        self.allowed_classifications.push(c);
        self
    }
}

/// Errors raised during tool schema validation, authorization or execution.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("NEXA-TOOL-0001: unknown tool '{0}'")]
    UnknownTool(String),

    #[error("NEXA-TOOL-0002: missing required argument '{0}'")]
    MissingArgument(String),

    #[error("NEXA-TOOL-0003: unknown argument '{0}'")]
    UnknownArgument(String),

    #[error("NEXA-TOOL-0004: argument '{argument}' expected '{expected}'")]
    TypeMismatch { argument: String, expected: String },

    #[error("NEXA-TOOL-0005: capability '{0}' required but not granted")]
    CapabilityDenied(String),

    #[error("NEXA-TOOL-0006: tool '{0}' requires approval")]
    ApprovalRequired(String),

    #[error("NEXA-TOOL-0007: approval token rejected for tool '{0}'")]
    ApprovalRejected(String),

    #[error("NEXA-TOOL-0008: path '{0}' escapes allowed root")]
    PathEscape(String),

    #[error("NEXA-TOOL-0009: invocation failed: {0}")]
    InvocationFailed(String),

    #[error("NEXA-TOOL-0010: result classification {0:?} not permitted for this context")]
    ResultClassificationDenied(Class),
}

/// The runtime contract for a typed tool.
pub trait Tool: Send + Sync {
    fn descriptor(&self) -> &ToolDescriptor;

    fn invoke(
        &self,
        input: ToolInput,
        context: &ToolExecutionContext,
    ) -> Result<ToolOutput, ToolError>;
}

/// Registry of typed tools keyed by `ToolId`.
#[derive(Default)]
pub struct ToolRegistry {
    tools: BTreeMap<ToolId, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        ToolRegistry {
            tools: BTreeMap::new(),
        }
    }

    pub fn register<T: Tool + 'static>(&mut self, tool: T) {
        let descriptor = tool.descriptor().id.clone();
        self.tools.insert(descriptor, Arc::new(tool));
    }

    pub fn register_arc(&mut self, tool: Arc<dyn Tool>) {
        let id = tool.descriptor().id.clone();
        self.tools.insert(id, tool);
    }

    pub fn get(&self, id: &ToolId) -> Option<Arc<dyn Tool>> {
        self.tools.get(id).cloned()
    }

    pub fn descriptor(&self, id: &ToolId) -> Option<&ToolDescriptor> {
        self.tools.get(id).map(|t| t.descriptor())
    }

    pub fn ids(&self) -> Vec<ToolId> {
        self.tools.keys().cloned().collect()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

/// Executes the full tool-invocation gate (spec §189) before running a tool.
pub struct ToolExecutor<'a> {
    registry: &'a ToolRegistry,
}

impl<'a> ToolExecutor<'a> {
    pub fn new(registry: &'a ToolRegistry) -> Self {
        ToolExecutor { registry }
    }

    /// Run a tool through every gate. Returns either a typed result or a
    /// structured denial.
    pub fn execute(
        &self,
        input: ToolInput,
        ctx: &ToolExecutionContext,
    ) -> Result<ToolOutput, ToolError> {
        // 1. Registered-ID membership (untrusted selector -> no auto-dispatch).
        let tool = self
            .registry
            .get(&input.tool_id)
            .ok_or_else(|| ToolError::UnknownTool(input.tool_id.to_string()))?;
        let descriptor = tool.descriptor().clone();

        // 2. Schema validation.
        descriptor.input_schema.validate(&input.arguments)?;

        // 3. Tool authority requirement.
        if !ctx
            .granted_capabilities
            .contains(&descriptor.authority.capability_id)
        {
            return Err(ToolError::CapabilityDenied(
                descriptor.authority.capability_id.to_string(),
            ));
        }

        // 4. Path containment (defense in depth for path-scoped tools).
        if let Some(root) = &descriptor.path_root {
            if let Some(path) = input.arguments.get("path") {
                if !path_contained(path, root) {
                    return Err(ToolError::PathEscape(path.clone()));
                }
            }
        }

        // 5. Approval gate.
        if descriptor.approval == ApprovalPolicy::Always {
            let token = ctx
                .approval
                .as_ref()
                .ok_or_else(|| ToolError::ApprovalRequired(descriptor.id.to_string()))?;
            if token.operation_class() != descriptor.id.as_str() {
                return Err(ToolError::ApprovalRejected(descriptor.id.to_string()));
            }
        }

        // 6. Execute.
        let out = tool.invoke(input, ctx)?;

        // 7. Result classification gate.
        if !ctx.allowed_classifications.is_empty()
            && !ctx
                .allowed_classifications
                .iter()
                .any(|c| *c >= out.classification)
        {
            return Err(ToolError::ResultClassificationDenied(out.classification));
        }

        Ok(out)
    }
}

/// Check whether `path` stays inside `root` (lexical containment). Used for
/// the `read_file`-style path-scoped tool example (defense in depth).
pub fn path_contained(path: &str, root: &str) -> bool {
    let root_norm = path::normalize(root);
    let path_norm = path::normalize(path);
    path_norm == root_norm
        || (path_norm.starts_with(&root_norm) && path_norm[root_norm.len()..].starts_with('/'))
}

/// Minimal path normalization for containment checks: resolves `.` and `..`,
/// collapses duplicate separators, and strips leading `./`.
mod path {
    pub fn normalize(path: &str) -> String {
        let mut parts: Vec<String> = Vec::new();
        let absolute = path.starts_with('/');
        for comp in path.split('/') {
            match comp {
                "" | "." => {}
                ".." => {
                    let last_not_dotdot = parts.last().map(|l| l.as_str() != "..").unwrap_or(false);
                    if last_not_dotdot {
                        parts.pop();
                    } else if !absolute {
                        parts.push("..".to_string());
                    }
                }
                c => parts.push(c.to_string()),
            }
        }
        let mut out = parts.join("/");
        if absolute {
            out = format!("/{out}");
        }
        if out.is_empty() {
            out = ".".to_string();
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn param(name: &str, ty: SchemaType) -> ToolParamSchema {
        ToolParamSchema::new(name, ty)
    }

    struct EchoTool {
        descriptor: ToolDescriptor,
    }

    impl EchoTool {
        fn new() -> Self {
            let descriptor = ToolDescriptor::new(ToolId::new("echo"))
                .with_input(
                    ToolSchema::new()
                        .field(param("text", SchemaType::string()))
                        .field(param("count", SchemaType::integer()).optional()),
                )
                .with_output(ToolSchema::new().field(param("echo", SchemaType::string())))
                .with_authority(CapabilityId::new("tools.echo"));
            EchoTool { descriptor }
        }
    }

    impl Tool for EchoTool {
        fn descriptor(&self) -> &ToolDescriptor {
            &self.descriptor
        }

        fn invoke(
            &self,
            input: ToolInput,
            _ctx: &ToolExecutionContext,
        ) -> Result<ToolOutput, ToolError> {
            let text = input.arguments.get("text").unwrap();
            Ok(ToolOutput::new(text, Class::Public))
        }
    }

    #[test]
    fn registry_roundtrip() {
        let mut reg = ToolRegistry::new();
        reg.register(EchoTool::new());
        assert_eq!(reg.ids(), vec![ToolId::new("echo")]);
        assert!(reg.descriptor(&ToolId::new("echo")).is_some());
        assert!(reg.get(&ToolId::new("missing")).is_none());
    }

    #[test]
    fn unknown_tool_rejected() {
        let reg = ToolRegistry::new();
        let ex = ToolExecutor::new(&reg);
        let ctx = ToolExecutionContext::new(1);
        let input = ToolInput::new(ToolId::new("nope"));
        let err = ex.execute(input, &ctx).unwrap_err();
        assert!(matches!(err, ToolError::UnknownTool(_)));
    }

    #[test]
    fn schema_missing_required_argument() {
        let mut reg = ToolRegistry::new();
        reg.register(EchoTool::new());
        let ex = ToolExecutor::new(&reg);
        let ctx = ToolExecutionContext::new(1).with_capability(CapabilityId::new("tools.echo"));
        let input = ToolInput::new(ToolId::new("echo")); // missing 'text'
        let err = ex.execute(input, &ctx).unwrap_err();
        assert!(matches!(err, ToolError::MissingArgument(_)));
    }

    #[test]
    fn schema_type_mismatch() {
        let mut reg = ToolRegistry::new();
        reg.register(EchoTool::new());
        let ex = ToolExecutor::new(&reg);
        let ctx = ToolExecutionContext::new(1).with_capability(CapabilityId::new("tools.echo"));
        let input = ToolInput::new(ToolId::new("echo"))
            .with_arg("text", "hi")
            .with_arg("count", "not-a-number");
        let err = ex.execute(input, &ctx).unwrap_err();
        assert!(matches!(err, ToolError::TypeMismatch { .. }));
    }

    #[test]
    fn capability_required() {
        let mut reg = ToolRegistry::new();
        reg.register(EchoTool::new());
        let ex = ToolExecutor::new(&reg);
        // Context lacks 'tools.echo' capability.
        let ctx = ToolExecutionContext::new(1);
        let input = ToolInput::new(ToolId::new("echo")).with_arg("text", "hi");
        let err = ex.execute(input, &ctx).unwrap_err();
        assert!(matches!(err, ToolError::CapabilityDenied(_)));
    }

    #[test]
    fn successful_invocation() {
        let mut reg = ToolRegistry::new();
        reg.register(EchoTool::new());
        let ex = ToolExecutor::new(&reg);
        let ctx = ToolExecutionContext::new(1)
            .with_capability(CapabilityId::new("tools.echo"))
            .with_classification(Class::Public);
        let input = ToolInput::new(ToolId::new("echo"))
            .with_arg("text", "hello")
            .with_arg("count", "3");
        let out = ex.execute(input, &ctx).unwrap();
        assert_eq!(out.result, "hello");
        assert_eq!(out.classification, Class::Public);
    }

    #[test]
    fn approval_required_without_token() {
        struct SensitiveTool {
            descriptor: ToolDescriptor,
        }
        impl Tool for SensitiveTool {
            fn descriptor(&self) -> &ToolDescriptor {
                &self.descriptor
            }
            fn invoke(
                &self,
                _i: ToolInput,
                _c: &ToolExecutionContext,
            ) -> Result<ToolOutput, ToolError> {
                Ok(ToolOutput::new("done", Class::Sensitive))
            }
        }
        let descriptor = ToolDescriptor::new(ToolId::new("send_email"))
            .with_input(ToolSchema::new().field(param("to", SchemaType::string())))
            .with_authority(CapabilityId::new("tools.act"))
            .with_approval(ApprovalPolicy::Always);
        let tool = SensitiveTool { descriptor };
        let mut reg = ToolRegistry::new();
        reg.register(tool);
        let ex = ToolExecutor::new(&reg);
        let ctx = ToolExecutionContext::new(1).with_capability(CapabilityId::new("tools.act"));
        let input = ToolInput::new(ToolId::new("send_email")).with_arg("to", "x");
        let err = ex.execute(input, &ctx).unwrap_err();
        assert!(matches!(err, ToolError::ApprovalRequired(_)));
    }

    #[test]
    fn approval_token_accepted() {
        let token = ApprovalToken::new(
            99,
            "send_email".to_string(),
            Default::default(),
            nxr_security::SecurityContextId(1),
            None,
            3,
        );
        struct GateTool {
            descriptor: ToolDescriptor,
        }
        impl Tool for GateTool {
            fn descriptor(&self) -> &ToolDescriptor {
                &self.descriptor
            }
            fn invoke(
                &self,
                _i: ToolInput,
                _c: &ToolExecutionContext,
            ) -> Result<ToolOutput, ToolError> {
                Ok(ToolOutput::new("ok", Class::Public))
            }
        }
        let descriptor = ToolDescriptor::new(ToolId::new("send_email"))
            .with_authority(CapabilityId::new("tools.act"))
            .with_approval(ApprovalPolicy::Always);
        let mut reg = ToolRegistry::new();
        reg.register(GateTool { descriptor });
        let ex = ToolExecutor::new(&reg);
        let ctx = ToolExecutionContext::new(1)
            .with_capability(CapabilityId::new("tools.act"))
            .with_approval(token)
            .with_classification(Class::Public);
        let input = ToolInput::new(ToolId::new("send_email"));
        assert!(ex.execute(input, &ctx).is_ok());
    }

    #[test]
    fn path_containment_rejects_escape() {
        assert!(!path_contained("../../etc/passwd", "/project/src"));
        assert!(path_contained("/project/src/main.nexa", "/project/src"));
        assert!(!path_contained("/project/src2/x", "/project/src"));
    }

    #[test]
    fn result_classification_gate() {
        struct SecretOut {
            descriptor: ToolDescriptor,
        }
        impl Tool for SecretOut {
            fn descriptor(&self) -> &ToolDescriptor {
                &self.descriptor
            }
            fn invoke(
                &self,
                _i: ToolInput,
                _c: &ToolExecutionContext,
            ) -> Result<ToolOutput, ToolError> {
                Ok(ToolOutput::new("secret", Class::Secret))
            }
        }
        let descriptor = ToolDescriptor::new(ToolId::new("read_secret"))
            .with_authority(CapabilityId::new("tools.read"));
        let mut reg = ToolRegistry::new();
        reg.register(SecretOut { descriptor });
        let ex = ToolExecutor::new(&reg);
        // Context only permits Public+Private, the tool returns Secret.
        let ctx = ToolExecutionContext::new(1)
            .with_capability(CapabilityId::new("tools.read"))
            .with_classification(Class::Public)
            .with_classification(Class::Private);
        let input = ToolInput::new(ToolId::new("read_secret"));
        let err = ex.execute(input, &ctx).unwrap_err();
        assert!(matches!(
            err,
            ToolError::ResultClassificationDenied(Class::Secret)
        ));
    }
}
