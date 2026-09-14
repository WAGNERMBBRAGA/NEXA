use std::sync::Arc;

use nxr_providers::{
    FilesystemProvider, InMemoryConsole, InMemoryEnvironment, InMemoryFilesystem, InMemoryNetwork,
    InMemoryProcess, InMemoryRandom, InMemorySecrets, NetworkRoute, ProcessExit, VirtualClock,
};
use nxr_resources::ResourceRegistry;
use nxr_security::{CapabilitySet, SecurityContext, SecurityKernel};
use nxr_tasks::TaskScheduler;

// ─── TestRuntimeBuilder ────────────────────────────────────────

pub struct TestRuntimeBuilder {
    console: InMemoryConsole,
    clock: VirtualClock,
    random: InMemoryRandom,
    environment: InMemoryEnvironment,
    filesystem: InMemoryFilesystem,
    network: InMemoryNetwork,
    process: InMemoryProcess,
    secrets: InMemorySecrets,
    max_tasks: u32,
    max_open_handles: u32,
}

impl TestRuntimeBuilder {
    pub fn new() -> Self {
        Self {
            console: InMemoryConsole::new(),
            clock: VirtualClock::new(),
            random: InMemoryRandom::new(0),
            environment: InMemoryEnvironment::new(),
            filesystem: InMemoryFilesystem::new(128),
            network: InMemoryNetwork::new(),
            process: InMemoryProcess::new(),
            secrets: InMemorySecrets::new(),
            max_tasks: 256,
            max_open_handles: 1024,
        }
    }

    pub fn with_console_input(&mut self, data: &[u8]) -> &mut Self {
        self.console.push_input(data);
        self
    }

    pub fn with_env_var(&mut self, name: &str, value: &str) -> &mut Self {
        self.environment.set_var(name, value);
        self
    }

    pub fn with_file(&mut self, path: &str, content: &[u8]) -> &mut Self {
        let handle = self.filesystem.open_file(path, true, true, true).unwrap();
        self.filesystem.write_file(handle, content).unwrap();
        self.filesystem.close(handle).unwrap();
        self
    }

    pub fn with_route(&mut self, host: &str, port: u16, response: &[u8]) -> &mut Self {
        self.network.add_route(NetworkRoute {
            host: host.to_string(),
            port,
            response: response.to_vec(),
        });
        self
    }

    pub fn with_secret(&mut self, id: &str, value: &[u8]) -> &mut Self {
        self.secrets.add_secret(id, value);
        self
    }

    pub fn with_simulated_process(
        &mut self,
        executable: &str,
        exit_code: i32,
        stdout: &[u8],
    ) -> &mut Self {
        self.process.add_exit(
            executable,
            ProcessExit {
                exit_code,
                stdout: stdout.to_vec(),
                stderr: Vec::new(),
            },
        );
        self
    }

    pub fn with_random_seed(&mut self, seed: u64) -> &mut Self {
        self.random = InMemoryRandom::new(seed);
        self
    }

    pub fn with_max_tasks(&mut self, max: u32) -> &mut Self {
        self.max_tasks = max;
        self
    }

    pub fn with_max_open_handles(&mut self, max: u32) -> &mut Self {
        self.max_open_handles = max;
        self
    }

    pub fn advance_clock_ns(&mut self, delta: u64) -> &mut Self {
        self.clock.advance_now_ns(delta);
        self.clock.advance_monotonic_ns(delta);
        self
    }

    pub fn console(&self) -> &InMemoryConsole {
        &self.console
    }

    pub fn clock(&self) -> &VirtualClock {
        &self.clock
    }

    pub fn random(&self) -> &InMemoryRandom {
        &self.random
    }

    pub fn environment(&self) -> &InMemoryEnvironment {
        &self.environment
    }

    pub fn filesystem(&self) -> &InMemoryFilesystem {
        &self.filesystem
    }

    pub fn network(&self) -> &InMemoryNetwork {
        &self.network
    }

    pub fn process(&self) -> &InMemoryProcess {
        &self.process
    }

    pub fn secrets(&self) -> &InMemorySecrets {
        &self.secrets
    }

    pub fn build(self) -> TestRuntime {
        TestRuntime {
            console: Arc::new(self.console),
            clock: Arc::new(self.clock),
            random: Arc::new(self.random),
            environment: Arc::new(self.environment),
            filesystem: Arc::new(self.filesystem),
            network: Arc::new(self.network),
            process: Arc::new(self.process),
            secrets: Arc::new(self.secrets),
            security: SecurityKernel::new(),
            task_scheduler: TaskScheduler::new(self.max_tasks),
            resource_registry: ResourceRegistry::new(self.max_open_handles),
            observability: nxr_observability::ObservabilityRuntime::with_memory_sinks(),
        }
    }
}

impl Default for TestRuntimeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ─── TestRuntime ───────────────────────────────────────────────

pub struct TestRuntime {
    console: Arc<InMemoryConsole>,
    clock: Arc<VirtualClock>,
    random: Arc<InMemoryRandom>,
    environment: Arc<InMemoryEnvironment>,
    filesystem: Arc<InMemoryFilesystem>,
    network: Arc<InMemoryNetwork>,
    process: Arc<InMemoryProcess>,
    secrets: Arc<InMemorySecrets>,
    security: SecurityKernel,
    task_scheduler: TaskScheduler,
    resource_registry: ResourceRegistry,
    observability: nxr_observability::ObservabilityRuntime,
}

impl TestRuntime {
    pub fn console(&self) -> &InMemoryConsole {
        &self.console
    }

    pub fn clock(&self) -> &VirtualClock {
        &self.clock
    }

    pub fn random(&self) -> &InMemoryRandom {
        &self.random
    }

    pub fn environment(&self) -> &InMemoryEnvironment {
        &self.environment
    }

    pub fn filesystem(&self) -> &InMemoryFilesystem {
        &self.filesystem
    }

    pub fn network(&self) -> &InMemoryNetwork {
        &self.network
    }

    pub fn process(&self) -> &InMemoryProcess {
        &self.process
    }

    pub fn secrets(&self) -> &InMemorySecrets {
        &self.secrets
    }

    pub fn security(&self) -> &SecurityKernel {
        &self.security
    }

    pub fn task_scheduler(&self) -> &TaskScheduler {
        &self.task_scheduler
    }

    pub fn resource_registry(&self) -> &ResourceRegistry {
        &self.resource_registry
    }

    pub fn observability(&self) -> &nxr_observability::ObservabilityRuntime {
        &self.observability
    }

    pub fn create_root_context(&mut self, caps: CapabilitySet) -> SecurityContext {
        self.security.create_root_context(caps)
    }
}

// ─── TestHarness ───────────────────────────────────────────────

pub struct TestHarness {
    runtime: TestRuntime,
    test_name: String,
}

impl TestHarness {
    pub fn new(test_name: &str) -> Self {
        let runtime = TestRuntimeBuilder::new().build();
        Self {
            runtime,
            test_name: test_name.to_string(),
        }
    }

    pub fn runtime(&self) -> &TestRuntime {
        &self.runtime
    }

    pub fn runtime_mut(&mut self) -> &mut TestRuntime {
        &mut self.runtime
    }

    pub fn assert_console_output(&self, expected: &str) {
        let output = self.runtime.console().stdout_string().unwrap_or_default();
        assert_eq!(
            output, expected,
            "[{}] console output mismatch: expected {:?}, got {:?}",
            self.test_name, expected, output
        );
    }

    pub fn assert_no_stderr(&self) {
        let output = self.runtime.console().stderr();
        assert!(
            output.is_empty(),
            "[{}] expected empty stderr, got {:?}",
            self.test_name,
            String::from_utf8_lossy(&output)
        );
    }

    pub fn assert_metric(&self, name: &str, expected: u64) {
        use nxr_observability::{MetricEntry, MetricValue};
        let metrics = self.runtime.observability().metrics_snapshot();
        let entry = metrics.iter().find(|m| m.name == name);
        let actual = match entry {
            Some(MetricEntry {
                value: MetricValue::Counter(v),
                ..
            }) => *v,
            _ => 0,
        };
        assert_eq!(
            actual, expected,
            "[{}] metric '{}' expected {}, got {}",
            self.test_name, name, expected, actual
        );
    }

    pub fn assert_audit_event(&self, event_type: nxr_observability::AuditEventType) {
        let sink = self
            .runtime
            .observability()
            .audit_sink()
            .as_any()
            .downcast_ref::<nxr_observability::MemoryAuditSink>()
            .unwrap();
        assert!(
            sink.contains_type(event_type),
            "[{}] expected audit event {:?} not found",
            self.test_name,
            event_type
        );
    }

    pub fn pass(&self) {
        println!("[PASS] {}", self.test_name);
    }

    pub fn fail(&self, reason: &str) {
        println!("[FAIL] {}: {}", self.test_name, reason);
    }
}

// ─── Tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use nxr_providers::{
        ClockProvider, ConsoleProvider, EnvironmentProvider, FilesystemProvider, NetworkProvider,
        ProcessProvider, RandomProvider, SecretsProvider,
    };

    #[test]
    fn test_builder_creates_runtime() {
        let runtime = TestRuntimeBuilder::new().build();
        assert!(runtime.security().context_count() == 0);
        assert_eq!(runtime.task_scheduler().task_count(), 0);
        assert_eq!(runtime.resource_registry().open_count(), 0);
    }

    #[test]
    fn test_builder_with_console_input() {
        let mut builder = TestRuntimeBuilder::new();
        builder.with_console_input(b"hello");
        builder.with_console_input(b"world");
        let runtime = builder.build();

        let line1 = runtime.console().read_line().unwrap();
        let line2 = runtime.console().read_line().unwrap();
        assert_eq!(line1, b"hello");
        assert_eq!(line2, b"world");
    }

    #[test]
    fn test_builder_with_env_var() {
        let mut builder = TestRuntimeBuilder::new();
        builder.with_env_var("HOME", "/root");
        builder.with_env_var("PATH", "/usr/bin");
        let runtime = builder.build();

        assert_eq!(
            runtime.environment().get_var("HOME").unwrap(),
            Some("/root".to_string())
        );
        assert_eq!(
            runtime.environment().get_var("PATH").unwrap(),
            Some("/usr/bin".to_string())
        );
    }

    #[test]
    fn test_builder_with_file() {
        let mut builder = TestRuntimeBuilder::new();
        builder.with_file("/test.txt", b"file content");
        let runtime = builder.build();

        let meta = runtime.filesystem().stat("/test.txt").unwrap();
        assert!(meta.is_file);
        assert_eq!(meta.size, 12);

        let handle = runtime
            .filesystem()
            .open_file("/test.txt", true, false, false)
            .unwrap();
        let mut buf = [0u8; 64];
        let n = runtime.filesystem().read_file(handle, &mut buf).unwrap();
        assert_eq!(&buf[..n], b"file content");
        runtime.filesystem().close(handle).unwrap();
    }

    #[test]
    fn test_builder_with_route() {
        let mut builder = TestRuntimeBuilder::new();
        builder.with_route("example.com", 80, b"HTTP 200 OK");
        let runtime = builder.build();

        let handle = runtime.network().connect("example.com", 80).unwrap();
        let mut buf = [0u8; 64];
        let n = runtime.network().receive(handle, &mut buf).unwrap();
        assert_eq!(&buf[..n], b"HTTP 200 OK");
        runtime.network().close(handle).unwrap();
    }

    #[test]
    fn test_builder_with_secret() {
        let mut builder = TestRuntimeBuilder::new();
        builder.with_secret("api_key", b"secret123");
        let runtime = builder.build();

        let value = runtime.secrets().resolve("api_key").unwrap();
        assert_eq!(value, b"secret123");
    }

    #[test]
    fn test_builder_advance_clock() {
        let mut builder = TestRuntimeBuilder::new();
        assert_eq!(builder.clock().now_ns(), 0);

        builder.advance_clock_ns(1000);
        assert_eq!(builder.clock().now_ns(), 1000);
        assert_eq!(builder.clock().monotonic_ns(), 1000);

        builder.advance_clock_ns(500);
        assert_eq!(builder.clock().now_ns(), 1500);
        assert_eq!(builder.clock().monotonic_ns(), 1500);
    }

    #[test]
    fn test_builder_with_random_seed() {
        let mut builder = TestRuntimeBuilder::new();
        builder.with_random_seed(42);
        let runtime = builder.build();

        let val1 = runtime.random().next_u64().unwrap();
        drop(runtime);

        let mut builder2 = TestRuntimeBuilder::new();
        builder2.with_random_seed(42);
        let runtime2 = builder2.build();
        let val2 = runtime2.random().next_u64().unwrap();

        assert_eq!(val1, val2);
    }

    #[test]
    fn test_builder_with_simulated_process() {
        let mut builder = TestRuntimeBuilder::new();
        builder.with_simulated_process("ls", 0, b"file1\nfile2\n");
        let runtime = builder.build();

        let handle = runtime.process().spawn("ls", &[], &[]).unwrap();
        let exit = runtime.process().wait(handle).unwrap();
        assert_eq!(exit.exit_code, 0);
        assert_eq!(exit.stdout, b"file1\nfile2\n");
    }

    #[test]
    fn test_harness_create() {
        let harness = TestHarness::new("my_test");
        assert_eq!(harness.runtime.security().context_count(), 0);
    }

    #[test]
    fn test_harness_assert_console_output() {
        let mut harness = TestHarness::new("console_test");
        harness
            .runtime_mut()
            .console()
            .write_stdout(b"hello world")
            .unwrap();
        harness.assert_console_output("hello world");
        harness.pass();
    }

    #[test]
    fn test_harness_assert_no_stderr() {
        let harness = TestHarness::new("stderr_test");
        harness.assert_no_stderr();
        harness.pass();
    }

    #[test]
    fn test_harness_pass() {
        let harness = TestHarness::new("pass_test");
        harness.pass();
    }

    #[test]
    fn test_harness_fail() {
        let harness = TestHarness::new("fail_test");
        harness.fail("something went wrong");
    }
}
