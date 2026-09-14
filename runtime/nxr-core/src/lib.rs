use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

// ─── ProviderId ───────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProviderId(pub String);

impl ProviderId {
    pub fn new(name: &str) -> Self {
        Self(name.to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProviderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for ProviderId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

// ─── ProviderVersion ──────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProviderVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl ProviderVersion {
    pub fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    pub fn is_compatible_with(&self, required: &ProviderVersion) -> bool {
        self.major == required.major && self.minor >= required.minor
    }
}

// ─── ProviderFeatureId ────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProviderFeatureId(pub String);

impl ProviderFeatureId {
    pub fn new(name: &str) -> Self {
        Self(name.to_string())
    }
}

// ─── ProviderDescriptor ───────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ProviderDescriptor {
    pub id: ProviderId,
    pub version: ProviderVersion,
    pub features: BTreeSet<ProviderFeatureId>,
}

impl ProviderDescriptor {
    pub fn new(id: ProviderId, version: ProviderVersion) -> Self {
        Self {
            id,
            version,
            features: BTreeSet::new(),
        }
    }

    pub fn has_feature(&self, feature: &ProviderFeatureId) -> bool {
        self.features.contains(feature)
    }

    pub fn add_feature(&mut self, feature: ProviderFeatureId) {
        self.features.insert(feature);
    }
}

// ─── NxrProvider trait ────────────────────────────────────────

pub trait NxrProvider: Send + Sync {
    fn descriptor(&self) -> &ProviderDescriptor;

    fn name(&self) -> &str {
        self.descriptor().id.as_str()
    }
}

// ─── ProviderRegistry ─────────────────────────────────────────

pub struct ProviderRegistry {
    providers: BTreeMap<ProviderId, Arc<dyn NxrProvider>>,
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: BTreeMap::new(),
        }
    }

    pub fn register(&mut self, provider: Arc<dyn NxrProvider>) -> Result<(), RuntimeError> {
        let id = provider.descriptor().id.clone();
        if self.providers.contains_key(&id) {
            return Err(RuntimeError::ProviderInitializationFailure {
                provider: id.to_string(),
                details: "provider already registered".to_string(),
            });
        }
        self.providers.insert(id, provider);
        Ok(())
    }

    pub fn get(&self, id: &ProviderId) -> Option<&Arc<dyn NxrProvider>> {
        self.providers.get(id)
    }

    pub fn has(&self, id: &ProviderId) -> bool {
        self.providers.contains_key(id)
    }

    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }

    pub fn len(&self) -> usize {
        self.providers.len()
    }

    pub fn ids(&self) -> Vec<&ProviderId> {
        self.providers.keys().collect()
    }

    pub fn feature_ids(&self) -> Vec<&ProviderFeatureId> {
        let mut features = BTreeSet::new();
        for provider in self.providers.values() {
            for f in &provider.descriptor().features {
                features.insert(f);
            }
        }
        features.into_iter().collect()
    }

    pub fn supports_feature(&self, feature: &ProviderFeatureId) -> bool {
        self.providers
            .values()
            .any(|p| p.descriptor().has_feature(feature))
    }

    pub fn remove(&mut self, id: &ProviderId) -> Option<Arc<dyn NxrProvider>> {
        self.providers.remove(id)
    }

    pub fn clear(&mut self) {
        self.providers.clear();
    }

    pub fn descriptors(&self) -> Vec<&ProviderDescriptor> {
        self.providers.values().map(|p| p.descriptor()).collect()
    }
}

// ─── RuntimeProfile ───────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeProfile {
    Minimal,
    Standard,
}

impl RuntimeProfile {
    pub fn name(&self) -> &'static str {
        match self {
            RuntimeProfile::Minimal => "minimal",
            RuntimeProfile::Standard => "standard",
        }
    }

    pub fn required_features(&self) -> Vec<&'static str> {
        match self {
            RuntimeProfile::Minimal => vec!["console", "clock::monotonic"],
            RuntimeProfile::Standard => vec![
                "console",
                "clock::monotonic",
                "filesystem",
                "network",
                "clock::wall",
                "random",
                "environment",
                "process",
                "secrets",
            ],
        }
    }
}

// ─── AssuranceLevel ───────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssuranceLevel {
    Managed,
    ConstrainedNative,
    UnrestrictedNative,
}

// ─── RuntimeError ─────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum RuntimeError {
    UnsupportedRuntimeProfile(String),
    MissingRequiredProvider { provider: String },
    ProviderInitializationFailure { provider: String, details: String },
    InvalidRuntimeRequirement(String),
    RuntimePolicyFailure(String),
    InvalidRuntimeHandle { handle: u64 },
    StaleRuntimeHandle { handle: u64 },
    ResourceLimitExceeded { resource: String, limit: u32 },
    TaskInfrastructureFailure(String),
    AuditInfrastructureFailure(String),
    ExpectedFailure(String),
    Cancellation(String),
    Trap(String),
    Panic(String),
    HostFailure(String),
    Fatal(String),
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuntimeError::UnsupportedRuntimeProfile(p) => {
                write!(f, "[NEXA-NXR-0001] unsupported runtime profile: {p}")
            }
            RuntimeError::MissingRequiredProvider { provider } => {
                write!(f, "[NEXA-NXR-0002] missing required provider: {provider}")
            }
            RuntimeError::ProviderInitializationFailure { provider, details } => {
                write!(
                    f,
                    "[NEXA-NXR-0003] provider initialization failure for {provider}: {details}"
                )
            }
            RuntimeError::InvalidRuntimeRequirement(r) => {
                write!(f, "[NEXA-NXR-0004] invalid runtime requirement: {r}")
            }
            RuntimeError::RuntimePolicyFailure(m) => {
                write!(f, "[NEXA-NXR-0005] runtime policy failure: {m}")
            }
            RuntimeError::InvalidRuntimeHandle { handle } => {
                write!(f, "[NEXA-NXR-0006] invalid runtime handle: {handle}")
            }
            RuntimeError::StaleRuntimeHandle { handle } => {
                write!(f, "[NEXA-NXR-0007] stale runtime handle: {handle}")
            }
            RuntimeError::ResourceLimitExceeded { resource, limit } => {
                write!(
                    f,
                    "[NEXA-NXR-0008] resource limit exceeded for {resource}: max {limit}"
                )
            }
            RuntimeError::TaskInfrastructureFailure(m) => {
                write!(f, "[NEXA-NXR-0009] task infrastructure failure: {m}")
            }
            RuntimeError::AuditInfrastructureFailure(m) => {
                write!(f, "[NEXA-NXR-0010] audit infrastructure failure: {m}")
            }
            RuntimeError::ExpectedFailure(m) => {
                write!(f, "[NEXA-NXR-0011] expected failure: {m}")
            }
            RuntimeError::Cancellation(m) => {
                write!(f, "[NEXA-NXR-0012] cancellation: {m}")
            }
            RuntimeError::Trap(m) => {
                write!(f, "[NEXA-NXR-0013] trap: {m}")
            }
            RuntimeError::Panic(m) => {
                write!(f, "[NEXA-NXR-0014] panic: {m}")
            }
            RuntimeError::HostFailure(m) => {
                write!(f, "[NEXA-NXR-0015] host failure: {m}")
            }
            RuntimeError::Fatal(m) => {
                write!(f, "[NEXA-NXR-0016] fatal: {m}")
            }
        }
    }
}

impl std::error::Error for RuntimeError {}

// ─── RuntimeRequirements ──────────────────────────────────────

#[derive(Debug)]
pub struct RuntimeRequirements {
    pub providers: Vec<ProviderId>,
    pub features: BTreeSet<ProviderFeatureId>,
    pub profile: RuntimeProfile,
}

impl RuntimeRequirements {
    pub fn new(profile: RuntimeProfile) -> Self {
        Self {
            providers: Vec::new(),
            features: BTreeSet::new(),
            profile,
        }
    }

    pub fn add_provider(&mut self, id: ProviderId) {
        self.providers.push(id);
    }

    pub fn add_feature(&mut self, f: ProviderFeatureId) {
        self.features.insert(f);
    }

    pub fn is_satisfied_by(&self, registry: &ProviderRegistry) -> bool {
        for required in &self.providers {
            if !registry.has(required) {
                return false;
            }
        }
        for required in &self.features {
            if !registry.supports_feature(required) {
                return false;
            }
        }
        true
    }
}

// ─── ExecutionContext ──────────────────────────────────────────

#[derive(Debug)]
pub struct ExecutionContext {
    pub profile: RuntimeProfile,
    pub assurance_level: AssuranceLevel,
    pub requirements: RuntimeRequirements,
}

// ─── NxrRuntimeConfig ─────────────────────────────────────────

pub struct NxrRuntimeConfig {
    pub profile: RuntimeProfile,
    pub assurance_level: AssuranceLevel,
    pub max_open_handles: u32,
    pub max_open_files: u32,
    pub max_open_sockets: u32,
    pub max_task_count: u32,
    pub max_memory_bytes: u64,
}

impl Default for NxrRuntimeConfig {
    fn default() -> Self {
        Self {
            profile: RuntimeProfile::Standard,
            assurance_level: AssuranceLevel::Managed,
            max_open_handles: 1024,
            max_open_files: 128,
            max_open_sockets: 64,
            max_task_count: 256,
            max_memory_bytes: 256 * 1024 * 1024,
        }
    }
}

// ─── NxrRuntime ───────────────────────────────────────────────

pub struct NxrRuntime {
    config: NxrRuntimeConfig,
    providers: ProviderRegistry,
    initialized: bool,
}

impl NxrRuntime {
    pub fn new(config: NxrRuntimeConfig) -> Result<Self, RuntimeError> {
        Ok(Self {
            config,
            providers: ProviderRegistry::new(),
            initialized: false,
        })
    }

    pub fn register_provider(
        &mut self,
        provider: Arc<dyn NxrProvider>,
    ) -> Result<(), RuntimeError> {
        self.providers.register(provider)
    }

    pub fn load_artifact(
        &self,
        requirements: &RuntimeRequirements,
    ) -> Result<ExecutionContext, RuntimeError> {
        if !requirements.is_satisfied_by(&self.providers) {
            for required in &requirements.providers {
                if !self.providers.has(required) {
                    return Err(RuntimeError::MissingRequiredProvider {
                        provider: required.to_string(),
                    });
                }
            }
            for required in &requirements.features {
                if !self.providers.supports_feature(required) {
                    return Err(RuntimeError::InvalidRuntimeRequirement(format!(
                        "missing required feature: {}",
                        required.0
                    )));
                }
            }
            return Err(RuntimeError::InvalidRuntimeRequirement(
                "unsatisfied requirements".to_string(),
            ));
        }
        Ok(ExecutionContext {
            profile: self.config.profile,
            assurance_level: self.config.assurance_level,
            requirements: RuntimeRequirements {
                providers: requirements.providers.clone(),
                features: requirements.features.clone(),
                profile: requirements.profile,
            },
        })
    }

    pub fn execute<F, T>(&self, entrypoint: F) -> Result<T, RuntimeError>
    where
        F: FnOnce() -> Result<T, RuntimeError>,
    {
        if !self.initialized {
            return Err(RuntimeError::RuntimePolicyFailure(
                "runtime not initialized".to_string(),
            ));
        }
        entrypoint()
    }

    pub fn shutdown(&mut self) {
        self.initialized = false;
        self.providers.clear();
    }

    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    pub fn profile(&self) -> RuntimeProfile {
        self.config.profile
    }

    pub fn providers(&self) -> &ProviderRegistry {
        &self.providers
    }

    pub fn config(&self) -> &NxrRuntimeConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StubProvider {
        descriptor: ProviderDescriptor,
    }

    impl StubProvider {
        fn new(name: &str, major: u32, minor: u32, patch: u32) -> Self {
            Self {
                descriptor: ProviderDescriptor::new(
                    ProviderId::new(name),
                    ProviderVersion::new(major, minor, patch),
                ),
            }
        }
    }

    impl NxrProvider for StubProvider {
        fn descriptor(&self) -> &ProviderDescriptor {
            &self.descriptor
        }
    }

    #[test]
    fn test_provider_id_creation() {
        let id = ProviderId::new("nexa.console");
        assert_eq!(id.as_str(), "nexa.console");
        assert_eq!(id.to_string(), "nexa.console");

        let from_string: ProviderId = String::from("nexa.fs").into();
        assert_eq!(from_string.as_str(), "nexa.fs");
    }

    #[test]
    fn test_provider_version_compatible() {
        let v = ProviderVersion::new(1, 5, 0);
        let required = ProviderVersion::new(1, 3, 0);
        assert!(v.is_compatible_with(&required));

        let same = ProviderVersion::new(1, 5, 0);
        assert!(v.is_compatible_with(&same));
    }

    #[test]
    fn test_provider_version_incompatible() {
        let v = ProviderVersion::new(2, 0, 0);
        let required = ProviderVersion::new(1, 0, 0);
        assert!(!v.is_compatible_with(&required));

        let v2 = ProviderVersion::new(1, 2, 0);
        let required2 = ProviderVersion::new(1, 5, 0);
        assert!(!v2.is_compatible_with(&required2));
    }

    #[test]
    fn test_provider_descriptor_features() {
        let mut desc =
            ProviderDescriptor::new(ProviderId::new("nexa.test"), ProviderVersion::new(1, 0, 0));
        let feat = ProviderFeatureId::new("my_feature");
        assert!(!desc.has_feature(&feat));
        desc.add_feature(feat.clone());
        assert!(desc.has_feature(&feat));
    }

    #[test]
    fn test_provider_registry_register_and_get() {
        let mut reg = ProviderRegistry::new();
        let provider: Arc<dyn NxrProvider> = Arc::new(StubProvider::new("nexa.a", 1, 0, 0));
        reg.register(provider).unwrap();

        let id = ProviderId::new("nexa.a");
        assert!(reg.get(&id).is_some());
        assert_eq!(reg.len(), 1);
        assert!(!reg.is_empty());
    }

    #[test]
    fn test_provider_registry_has() {
        let mut reg = ProviderRegistry::new();
        let id = ProviderId::new("nexa.missing");
        assert!(!reg.has(&id));

        let provider: Arc<dyn NxrProvider> = Arc::new(StubProvider::new("nexa.present", 1, 0, 0));
        reg.register(provider).unwrap();
        assert!(reg.has(&ProviderId::new("nexa.present")));
    }

    #[test]
    fn test_provider_registry_feature_support() {
        let mut provider = StubProvider::new("nexa.svc", 1, 0, 0);
        provider
            .descriptor
            .add_feature(ProviderFeatureId::new("alpha"));
        provider
            .descriptor
            .add_feature(ProviderFeatureId::new("beta"));

        let mut reg = ProviderRegistry::new();
        reg.register(Arc::new(provider)).unwrap();

        assert!(reg.supports_feature(&ProviderFeatureId::new("alpha")));
        assert!(reg.supports_feature(&ProviderFeatureId::new("beta")));
        assert!(!reg.supports_feature(&ProviderFeatureId::new("gamma")));

        let features = reg.feature_ids();
        assert_eq!(features.len(), 2);
    }

    #[test]
    fn test_runtime_profile_names() {
        assert_eq!(RuntimeProfile::Minimal.name(), "minimal");
        assert_eq!(RuntimeProfile::Standard.name(), "standard");
    }

    #[test]
    fn test_runtime_profile_required_features() {
        let minimal = RuntimeProfile::Minimal.required_features();
        assert!(minimal.contains(&"console"));
        assert!(minimal.contains(&"clock::monotonic"));

        let standard = RuntimeProfile::Standard.required_features();
        assert!(standard.contains(&"console"));
        assert!(standard.contains(&"filesystem"));
        assert!(standard.contains(&"network"));
        assert!(standard.len() > minimal.len());
    }

    #[test]
    fn test_runtime_requirements_satisfied() {
        let mut reg = ProviderRegistry::new();
        let mut p = StubProvider::new("nexa.console", 1, 0, 0);
        p.descriptor.add_feature(ProviderFeatureId::new("console"));
        reg.register(Arc::new(p)).unwrap();

        let mut reqs = RuntimeRequirements::new(RuntimeProfile::Minimal);
        reqs.add_provider(ProviderId::new("nexa.console"));
        reqs.add_feature(ProviderFeatureId::new("console"));
        assert!(reqs.is_satisfied_by(&reg));
    }

    #[test]
    fn test_runtime_requirements_unsatisfied() {
        let reg = ProviderRegistry::new();
        let mut reqs = RuntimeRequirements::new(RuntimeProfile::Minimal);
        reqs.add_provider(ProviderId::new("nexa.missing"));
        assert!(!reqs.is_satisfied_by(&reg));
    }

    #[test]
    fn test_nxr_runtime_creation() {
        let rt = NxrRuntime::new(NxrRuntimeConfig::default());
        assert!(rt.is_ok());
        let rt = rt.unwrap();
        assert!(!rt.is_initialized());
        assert_eq!(rt.profile(), RuntimeProfile::Standard);
    }

    #[test]
    fn test_nxr_runtime_provider_registration() {
        let mut rt = NxrRuntime::new(NxrRuntimeConfig::default()).unwrap();
        let provider: Arc<dyn NxrProvider> = Arc::new(StubProvider::new("nexa.x", 1, 0, 0));
        assert!(rt.register_provider(provider).is_ok());
        assert!(rt.providers().has(&ProviderId::new("nexa.x")));
        assert_eq!(rt.providers().len(), 1);

        let dup: Arc<dyn NxrProvider> = Arc::new(StubProvider::new("nexa.x", 1, 1, 0));
        assert!(rt.register_provider(dup).is_err());
    }

    #[test]
    fn test_nxr_runtime_execute() {
        let rt = NxrRuntime::new(NxrRuntimeConfig::default()).unwrap();
        let result = rt.execute(|| Ok(42i32));
        assert!(result.is_err());

        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("NEXA-NXR-0005"));
    }

    #[test]
    fn test_nxr_runtime_shutdown() {
        let mut rt = NxrRuntime::new(NxrRuntimeConfig::default()).unwrap();
        let provider: Arc<dyn NxrProvider> = Arc::new(StubProvider::new("nexa.s", 1, 0, 0));
        rt.register_provider(provider).unwrap();
        assert!(rt.providers().len() == 1);

        rt.shutdown();
        assert!(!rt.is_initialized());
        assert!(rt.providers().is_empty());
    }

    #[test]
    fn test_runtime_config_defaults() {
        let cfg = NxrRuntimeConfig::default();
        assert_eq!(cfg.profile, RuntimeProfile::Standard);
        assert_eq!(cfg.assurance_level, AssuranceLevel::Managed);
        assert_eq!(cfg.max_open_handles, 1024);
        assert_eq!(cfg.max_open_files, 128);
        assert_eq!(cfg.max_open_sockets, 64);
        assert_eq!(cfg.max_task_count, 256);
        assert_eq!(cfg.max_memory_bytes, 256 * 1024 * 1024);
    }

    #[test]
    fn test_assurance_level_variants() {
        assert_eq!(AssuranceLevel::Managed, AssuranceLevel::Managed);
        assert_ne!(AssuranceLevel::Managed, AssuranceLevel::ConstrainedNative);
        assert_ne!(
            AssuranceLevel::ConstrainedNative,
            AssuranceLevel::UnrestrictedNative
        );
    }

    #[test]
    fn test_runtime_error_display() {
        let e = RuntimeError::Fatal("boom".to_string());
        let s = e.to_string();
        assert!(s.contains("NEXA-NXR-0016"));
        assert!(s.contains("boom"));

        let e2 = RuntimeError::MissingRequiredProvider {
            provider: "nexa.x".to_string(),
        };
        let s2 = e2.to_string();
        assert!(s2.contains("NEXA-NXR-0002"));
        assert!(s2.contains("nexa.x"));
    }

    #[test]
    fn test_runtime_error_is_std_error() {
        let e: Box<dyn std::error::Error> = Box::new(RuntimeError::Trap("oops".to_string()));
        assert!(e.to_string().contains("NEXA-NXR-0013"));
    }

    #[test]
    fn test_provider_registry_remove_and_clear() {
        let mut reg = ProviderRegistry::new();
        let p1: Arc<dyn NxrProvider> = Arc::new(StubProvider::new("nexa.a", 1, 0, 0));
        let p2: Arc<dyn NxrProvider> = Arc::new(StubProvider::new("nexa.b", 1, 0, 0));
        reg.register(p1).unwrap();
        reg.register(p2).unwrap();
        assert_eq!(reg.len(), 2);

        let removed = reg.remove(&ProviderId::new("nexa.a"));
        assert!(removed.is_some());
        assert_eq!(reg.len(), 1);

        reg.clear();
        assert!(reg.is_empty());
    }

    #[test]
    fn test_provider_registry_descriptors() {
        let mut reg = ProviderRegistry::new();
        let p: Arc<dyn NxrProvider> = Arc::new(StubProvider::new("nexa.d", 2, 3, 1));
        reg.register(p).unwrap();

        let descs = reg.descriptors();
        assert_eq!(descs.len(), 1);
        assert_eq!(descs[0].id.as_str(), "nexa.d");
        assert_eq!(descs[0].version.major, 2);
    }

    #[test]
    fn test_load_artifact_satisfied() {
        let mut rt = NxrRuntime::new(NxrRuntimeConfig::default()).unwrap();
        let mut p = StubProvider::new("nexa.console", 1, 0, 0);
        p.descriptor.add_feature(ProviderFeatureId::new("console"));
        rt.register_provider(Arc::new(p)).unwrap();

        let mut reqs = RuntimeRequirements::new(RuntimeProfile::Minimal);
        reqs.add_provider(ProviderId::new("nexa.console"));
        reqs.add_feature(ProviderFeatureId::new("console"));

        let ctx = rt.load_artifact(&reqs);
        assert!(ctx.is_ok());
        let ctx = ctx.unwrap();
        assert_eq!(ctx.profile, RuntimeProfile::Standard);
        assert_eq!(ctx.assurance_level, AssuranceLevel::Managed);
    }

    #[test]
    fn test_load_artifact_unsatisfied_provider() {
        let rt = NxrRuntime::new(NxrRuntimeConfig::default()).unwrap();
        let mut reqs = RuntimeRequirements::new(RuntimeProfile::Minimal);
        reqs.add_provider(ProviderId::new("nexa.nothing"));

        let err = rt.load_artifact(&reqs).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("NEXA-NXR-0002"));
    }
}
