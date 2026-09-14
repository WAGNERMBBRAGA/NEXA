use std::collections::BTreeMap;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

// ─── LogLevel ─────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Trace => "TRACE",
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
        }
    }

    pub fn priority(&self) -> u8 {
        match self {
            LogLevel::Trace => 0,
            LogLevel::Debug => 1,
            LogLevel::Info => 2,
            LogLevel::Warn => 3,
            LogLevel::Error => 4,
        }
    }
}

// ─── LogValue ─────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum LogValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Null,
}

impl LogValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            LogValue::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            LogValue::Int(i) => Some(*i),
            _ => None,
        }
    }
}

impl fmt::Display for LogValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LogValue::String(s) => write!(f, "{s}"),
            LogValue::Int(i) => write!(f, "{i}"),
            LogValue::Float(fl) => write!(f, "{fl}"),
            LogValue::Bool(b) => write!(f, "{b}"),
            LogValue::Null => write!(f, "null"),
        }
    }
}

// ─── LogField ─────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct LogField {
    pub key: String,
    pub value: LogValue,
}

// ─── StructuredLog ────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct StructuredLog {
    pub level: LogLevel,
    pub message: String,
    pub fields: Vec<LogField>,
    pub trace_id: Option<String>,
    pub task_id: Option<u64>,
    pub timestamp_ns: u64,
}

// ─── LogSink ──────────────────────────────────────────────────

pub trait LogSink: Send + Sync {
    fn emit(&self, entry: &StructuredLog);
    fn flush(&self);
    fn as_any(&self) -> &dyn std::any::Any;
}

// ─── MemoryLogSink ────────────────────────────────────────────

pub struct MemoryLogSink {
    entries: Mutex<Vec<StructuredLog>>,
}

impl MemoryLogSink {
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(Vec::new()),
        }
    }

    pub fn entries(&self) -> Vec<StructuredLog> {
        self.entries.lock().unwrap().clone()
    }

    pub fn clear(&self) {
        self.entries.lock().unwrap().clear();
    }

    pub fn count(&self) -> usize {
        self.entries.lock().unwrap().len()
    }

    pub fn last(&self) -> Option<StructuredLog> {
        self.entries.lock().unwrap().last().cloned()
    }

    pub fn contains_message(&self, msg: &str) -> bool {
        self.entries
            .lock()
            .unwrap()
            .iter()
            .any(|e| e.message == msg)
    }
}

impl Default for MemoryLogSink {
    fn default() -> Self {
        Self::new()
    }
}

impl LogSink for MemoryLogSink {
    fn emit(&self, entry: &StructuredLog) {
        self.entries.lock().unwrap().push(entry.clone());
    }

    fn flush(&self) {}

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

// ─── MetricType ───────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricType {
    Counter,
    Gauge,
    Histogram,
}

// ─── MetricValue ──────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub enum MetricValue {
    Counter(u64),
    Gauge(i64),
    Histogram { sum: f64, count: u64 },
}

// ─── MetricEntry ──────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct MetricEntry {
    pub name: String,
    pub metric_type: MetricType,
    pub value: MetricValue,
    pub labels: Vec<LogField>,
}

// ─── MetricsCollector ─────────────────────────────────────────

pub struct MetricsCollector {
    metrics: Mutex<BTreeMap<String, MetricEntry>>,
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self {
            metrics: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn increment_counter(&self, name: &str, value: u64) {
        let mut map = self.metrics.lock().unwrap();
        let entry = map.entry(name.to_string()).or_insert_with(|| MetricEntry {
            name: name.to_string(),
            metric_type: MetricType::Counter,
            value: MetricValue::Counter(0),
            labels: Vec::new(),
        });
        if let MetricValue::Counter(ref mut c) = entry.value {
            *c += value;
        }
    }

    pub fn set_gauge(&self, name: &str, value: i64) {
        let mut map = self.metrics.lock().unwrap();
        let entry = map.entry(name.to_string()).or_insert_with(|| MetricEntry {
            name: name.to_string(),
            metric_type: MetricType::Gauge,
            value: MetricValue::Gauge(0),
            labels: Vec::new(),
        });
        entry.value = MetricValue::Gauge(value);
    }

    pub fn record_histogram(&self, name: &str, value: f64) {
        let mut map = self.metrics.lock().unwrap();
        let entry = map.entry(name.to_string()).or_insert_with(|| MetricEntry {
            name: name.to_string(),
            metric_type: MetricType::Histogram,
            value: MetricValue::Histogram { sum: 0.0, count: 0 },
            labels: Vec::new(),
        });
        if let MetricValue::Histogram {
            ref mut sum,
            ref mut count,
        } = entry.value
        {
            *sum += value;
            *count += 1;
        }
    }

    pub fn get(&self, name: &str) -> Option<MetricEntry> {
        self.metrics.lock().unwrap().get(name).cloned()
    }

    pub fn snapshot(&self) -> Vec<MetricEntry> {
        self.metrics.lock().unwrap().values().cloned().collect()
    }

    pub fn clear(&self) {
        self.metrics.lock().unwrap().clear();
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

// ─── TraceContext ─────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceContext {
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
}

static TRACE_COUNTER: AtomicU64 = AtomicU64::new(1);

impl TraceContext {
    pub fn new_root() -> Self {
        let id = TRACE_COUNTER.fetch_add(1, Ordering::Relaxed);
        Self {
            trace_id: format!("trace-{id}"),
            span_id: format!("span-{id}"),
            parent_span_id: None,
        }
    }

    pub fn child(&self) -> TraceContext {
        let id = TRACE_COUNTER.fetch_add(1, Ordering::Relaxed);
        TraceContext {
            trace_id: self.trace_id.clone(),
            span_id: format!("span-{id}"),
            parent_span_id: Some(self.span_id.clone()),
        }
    }
}

// ─── AuditEventType ───────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditEventType {
    CapabilityDenied,
    ApprovalRequired,
    ApprovalConsumed,
    ResourceOpened,
    ResourceRevoked,
    ProcessStarted,
    ShellInvoked,
    SecretMaterialized,
    SecurityPolicyFailure,
}

impl AuditEventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            AuditEventType::CapabilityDenied => "capability_denied",
            AuditEventType::ApprovalRequired => "approval_required",
            AuditEventType::ApprovalConsumed => "approval_consumed",
            AuditEventType::ResourceOpened => "resource_opened",
            AuditEventType::ResourceRevoked => "resource_revoked",
            AuditEventType::ProcessStarted => "process_started",
            AuditEventType::ShellInvoked => "shell_invoked",
            AuditEventType::SecretMaterialized => "secret_materialized",
            AuditEventType::SecurityPolicyFailure => "security_policy_failure",
        }
    }
}

// ─── AuditEvent ───────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AuditEvent {
    pub event_type: AuditEventType,
    pub timestamp_ns: u64,
    pub context_id: Option<u64>,
    pub resource_id: Option<u64>,
    pub detail: String,
    pub redacted: bool,
}

// ─── AuditSink ────────────────────────────────────────────────

pub trait AuditSink: Send + Sync {
    fn record(&self, event: &AuditEvent);
    fn flush(&self);
    fn as_any(&self) -> &dyn std::any::Any;
}

// ─── MemoryAuditSink ──────────────────────────────────────────

pub struct MemoryAuditSink {
    events: Mutex<Vec<AuditEvent>>,
}

impl MemoryAuditSink {
    pub fn new() -> Self {
        Self {
            events: Mutex::new(Vec::new()),
        }
    }

    pub fn events(&self) -> Vec<AuditEvent> {
        self.events.lock().unwrap().clone()
    }

    pub fn count(&self) -> usize {
        self.events.lock().unwrap().len()
    }

    pub fn clear(&self) {
        self.events.lock().unwrap().clear();
    }

    pub fn contains_type(&self, event_type: AuditEventType) -> bool {
        self.events
            .lock()
            .unwrap()
            .iter()
            .any(|e| e.event_type == event_type)
    }
}

impl Default for MemoryAuditSink {
    fn default() -> Self {
        Self::new()
    }
}

impl AuditSink for MemoryAuditSink {
    fn record(&self, event: &AuditEvent) {
        self.events.lock().unwrap().push(event.clone());
    }

    fn flush(&self) {}

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

// ─── SecretRedactor ───────────────────────────────────────────

pub struct SecretRedactor {
    redact_always: bool,
}

impl SecretRedactor {
    pub fn new() -> Self {
        Self {
            redact_always: false,
        }
    }

    pub fn new_always_redact() -> Self {
        Self {
            redact_always: true,
        }
    }

    pub fn redact_string(&self, input: &str) -> String {
        if self.redact_always {
            "[REDACTED]".to_string()
        } else {
            input.to_string()
        }
    }

    pub fn is_redacted(&self) -> bool {
        self.redact_always
    }
}

impl Default for SecretRedactor {
    fn default() -> Self {
        Self::new()
    }
}

// ─── ObservabilityRuntime ─────────────────────────────────────

pub struct ObservabilityRuntime {
    log_sink: Box<dyn LogSink>,
    metrics: MetricsCollector,
    audit_sink: Box<dyn AuditSink>,
    redactor: SecretRedactor,
    trace_counter: AtomicU64,
}

impl ObservabilityRuntime {
    pub fn new(log_sink: Box<dyn LogSink>, audit_sink: Box<dyn AuditSink>) -> Self {
        Self {
            log_sink,
            metrics: MetricsCollector::new(),
            audit_sink,
            redactor: SecretRedactor::new(),
            trace_counter: AtomicU64::new(1),
        }
    }

    pub fn with_memory_sinks() -> Self {
        Self::new(
            Box::new(MemoryLogSink::new()),
            Box::new(MemoryAuditSink::new()),
        )
    }

    fn now_ns() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64
    }

    pub fn log(&self, level: LogLevel, message: &str, fields: Vec<LogField>) {
        self.log_with_context(level, message, fields, None, None);
    }

    pub fn log_with_context(
        &self,
        level: LogLevel,
        message: &str,
        mut fields: Vec<LogField>,
        trace_id: Option<String>,
        task_id: Option<u64>,
    ) {
        for field in &mut fields {
            field.value = match &field.value {
                LogValue::String(s) => LogValue::String(self.redactor.redact_string(s)),
                other => other.clone(),
            };
        }
        let entry = StructuredLog {
            level,
            message: message.to_string(),
            fields,
            trace_id,
            task_id,
            timestamp_ns: Self::now_ns(),
        };
        self.log_sink.emit(&entry);
    }

    pub fn increment_counter(&self, name: &str, value: u64) {
        self.metrics.increment_counter(name, value);
    }

    pub fn set_gauge(&self, name: &str, value: i64) {
        self.metrics.set_gauge(name, value);
    }

    pub fn record_histogram(&self, name: &str, value: f64) {
        self.metrics.record_histogram(name, value);
    }

    pub fn metrics_snapshot(&self) -> Vec<MetricEntry> {
        self.metrics.snapshot()
    }

    pub fn record_audit(&self, event: AuditEvent) {
        self.audit_sink.record(&event);
    }

    pub fn create_trace_context(&self) -> TraceContext {
        let id = self.trace_counter.fetch_add(1, Ordering::Relaxed);
        TraceContext {
            trace_id: format!("trace-{id}"),
            span_id: format!("span-{id}"),
            parent_span_id: None,
        }
    }

    pub fn redact(&self, input: &str) -> String {
        self.redactor.redact_string(input)
    }

    pub fn log_sink(&self) -> &dyn LogSink {
        self.log_sink.as_ref()
    }

    pub fn audit_sink(&self) -> &dyn AuditSink {
        self.audit_sink.as_ref()
    }
}

// ─── Tests ────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_level_ordering() {
        assert!(LogLevel::Trace < LogLevel::Debug);
        assert!(LogLevel::Debug < LogLevel::Info);
        assert!(LogLevel::Info < LogLevel::Warn);
        assert!(LogLevel::Warn < LogLevel::Error);
    }

    #[test]
    fn test_log_level_as_str() {
        assert_eq!(LogLevel::Trace.as_str(), "TRACE");
        assert_eq!(LogLevel::Debug.as_str(), "DEBUG");
        assert_eq!(LogLevel::Info.as_str(), "INFO");
        assert_eq!(LogLevel::Warn.as_str(), "WARN");
        assert_eq!(LogLevel::Error.as_str(), "ERROR");
    }

    #[test]
    fn test_log_value_variants() {
        let s = LogValue::String("hello".to_string());
        assert_eq!(s.as_str(), Some("hello"));
        assert_eq!(s.to_string(), "hello");

        let i = LogValue::Int(42);
        assert_eq!(i.as_i64(), Some(42));
        assert_eq!(i.to_string(), "42");

        let f = LogValue::Float(3.5);
        assert_eq!(f.as_str(), None);
        assert_eq!(f.to_string(), "3.5");

        let b = LogValue::Bool(true);
        assert_eq!(b.to_string(), "true");

        let n = LogValue::Null;
        assert_eq!(n.as_str(), None);
        assert_eq!(n.as_i64(), None);
        assert_eq!(n.to_string(), "null");
    }

    #[test]
    fn test_structured_log_creation() {
        let log = StructuredLog {
            level: LogLevel::Info,
            message: "test message".to_string(),
            fields: vec![LogField {
                key: "key".to_string(),
                value: LogValue::String("value".to_string()),
            }],
            trace_id: Some("trace-1".to_string()),
            task_id: Some(42),
            timestamp_ns: 1234567890,
        };
        assert_eq!(log.level, LogLevel::Info);
        assert_eq!(log.message, "test message");
        assert_eq!(log.fields.len(), 1);
        assert_eq!(log.trace_id, Some("trace-1".to_string()));
        assert_eq!(log.task_id, Some(42));
    }

    #[test]
    fn test_memory_log_sink_emit() {
        let sink = MemoryLogSink::new();
        let entry = StructuredLog {
            level: LogLevel::Info,
            message: "hello".to_string(),
            fields: vec![],
            trace_id: None,
            task_id: None,
            timestamp_ns: 0,
        };
        sink.emit(&entry);
        assert_eq!(sink.count(), 1);
        assert!(sink.contains_message("hello"));
    }

    #[test]
    fn test_memory_log_sink_count() {
        let sink = MemoryLogSink::new();
        assert_eq!(sink.count(), 0);
        for i in 0..5 {
            sink.emit(&StructuredLog {
                level: LogLevel::Info,
                message: format!("msg-{i}"),
                fields: vec![],
                trace_id: None,
                task_id: None,
                timestamp_ns: 0,
            });
        }
        assert_eq!(sink.count(), 5);
    }

    #[test]
    fn test_memory_log_sink_clear() {
        let sink = MemoryLogSink::new();
        sink.emit(&StructuredLog {
            level: LogLevel::Error,
            message: "err".to_string(),
            fields: vec![],
            trace_id: None,
            task_id: None,
            timestamp_ns: 0,
        });
        assert_eq!(sink.count(), 1);
        sink.clear();
        assert_eq!(sink.count(), 0);
    }

    #[test]
    fn test_metric_counter() {
        let mc = MetricsCollector::new();
        mc.increment_counter("requests", 5);
        mc.increment_counter("requests", 3);
        let entry = mc.get("requests").unwrap();
        assert_eq!(entry.metric_type, MetricType::Counter);
        assert!(matches!(entry.value, MetricValue::Counter(8)));
    }

    #[test]
    fn test_metric_gauge() {
        let mc = MetricsCollector::new();
        mc.set_gauge("temperature", 72);
        mc.set_gauge("temperature", 80);
        let entry = mc.get("temperature").unwrap();
        assert_eq!(entry.metric_type, MetricType::Gauge);
        assert!(matches!(entry.value, MetricValue::Gauge(80)));
    }

    #[test]
    fn test_metric_histogram() {
        let mc = MetricsCollector::new();
        mc.record_histogram("latency", 1.5);
        mc.record_histogram("latency", 2.5);
        let entry = mc.get("latency").unwrap();
        assert_eq!(entry.metric_type, MetricType::Histogram);
        match entry.value {
            MetricValue::Histogram { sum, count } => {
                assert!((sum - 4.0).abs() < f64::EPSILON);
                assert_eq!(count, 2);
            }
            _ => panic!("expected Histogram"),
        }
    }

    #[test]
    fn test_metrics_snapshot() {
        let mc = MetricsCollector::new();
        mc.increment_counter("a", 1);
        mc.set_gauge("b", 2);
        mc.record_histogram("c", 3.0);
        let snap = mc.snapshot();
        assert_eq!(snap.len(), 3);
    }

    #[test]
    fn test_trace_context_new_root() {
        let ctx = TraceContext::new_root();
        assert!(!ctx.trace_id.is_empty());
        assert!(!ctx.span_id.is_empty());
        assert!(ctx.parent_span_id.is_none());
    }

    #[test]
    fn test_trace_context_child() {
        let root = TraceContext::new_root();
        let child = root.child();
        assert_eq!(child.trace_id, root.trace_id);
        assert_ne!(child.span_id, root.span_id);
        assert_eq!(child.parent_span_id, Some(root.span_id.clone()));
    }

    #[test]
    fn test_audit_event_types() {
        assert_eq!(
            AuditEventType::CapabilityDenied.as_str(),
            "capability_denied"
        );
        assert_eq!(
            AuditEventType::ApprovalRequired.as_str(),
            "approval_required"
        );
        assert_eq!(
            AuditEventType::ApprovalConsumed.as_str(),
            "approval_consumed"
        );
        assert_eq!(AuditEventType::ResourceOpened.as_str(), "resource_opened");
        assert_eq!(AuditEventType::ResourceRevoked.as_str(), "resource_revoked");
        assert_eq!(AuditEventType::ProcessStarted.as_str(), "process_started");
        assert_eq!(AuditEventType::ShellInvoked.as_str(), "shell_invoked");
        assert_eq!(
            AuditEventType::SecretMaterialized.as_str(),
            "secret_materialized"
        );
        assert_eq!(
            AuditEventType::SecurityPolicyFailure.as_str(),
            "security_policy_failure"
        );
    }

    #[test]
    fn test_memory_audit_sink_record() {
        let sink = MemoryAuditSink::new();
        assert_eq!(sink.count(), 0);
        let event = AuditEvent {
            event_type: AuditEventType::ProcessStarted,
            timestamp_ns: 100,
            context_id: None,
            resource_id: None,
            detail: "started".to_string(),
            redacted: false,
        };
        sink.record(&event);
        assert_eq!(sink.count(), 1);
        assert!(sink.contains_type(AuditEventType::ProcessStarted));
        assert!(!sink.contains_type(AuditEventType::CapabilityDenied));
    }

    #[test]
    fn test_secret_redactor() {
        let redactor = SecretRedactor::new();
        assert!(!redactor.is_redacted());
        assert_eq!(redactor.redact_string("secret"), "secret");

        let always = SecretRedactor::new_always_redact();
        assert!(always.is_redacted());
        assert_eq!(always.redact_string("secret"), "[REDACTED]");
    }

    #[test]
    fn test_observability_log() {
        let obs = ObservabilityRuntime::with_memory_sinks();
        obs.log(
            LogLevel::Info,
            "test message",
            vec![LogField {
                key: "k".to_string(),
                value: LogValue::Int(1),
            }],
        );
        let sink = obs
            .log_sink()
            .as_any()
            .downcast_ref::<MemoryLogSink>()
            .unwrap();
        assert_eq!(sink.count(), 1);
        let entries = sink.entries();
        assert_eq!(entries[0].message, "test message");
        assert_eq!(entries[0].level, LogLevel::Info);
    }

    #[test]
    fn test_observability_with_memory_sinks() {
        let obs = ObservabilityRuntime::with_memory_sinks();
        obs.log(LogLevel::Warn, "warn msg", vec![]);
        obs.increment_counter("cnt", 10);
        obs.record_audit(AuditEvent {
            event_type: AuditEventType::ShellInvoked,
            timestamp_ns: 0,
            context_id: None,
            resource_id: None,
            detail: "shell".to_string(),
            redacted: false,
        });

        let log_sink = obs
            .log_sink()
            .as_any()
            .downcast_ref::<MemoryLogSink>()
            .unwrap();
        assert_eq!(log_sink.count(), 1);

        let audit_sink = obs
            .audit_sink()
            .as_any()
            .downcast_ref::<MemoryAuditSink>()
            .unwrap();
        assert_eq!(audit_sink.count(), 1);

        let snap = obs.metrics_snapshot();
        assert_eq!(snap.len(), 1);
    }

    #[test]
    fn test_observability_audit() {
        let obs = ObservabilityRuntime::with_memory_sinks();
        let event = AuditEvent {
            event_type: AuditEventType::ResourceOpened,
            timestamp_ns: 999,
            context_id: Some(1),
            resource_id: Some(2),
            detail: "opened".to_string(),
            redacted: false,
        };
        obs.record_audit(event.clone());
        let audit_sink = obs
            .audit_sink()
            .as_any()
            .downcast_ref::<MemoryAuditSink>()
            .unwrap();
        assert_eq!(audit_sink.count(), 1);
        let events = audit_sink.events();
        assert_eq!(events[0].event_type, AuditEventType::ResourceOpened);
        assert_eq!(events[0].context_id, Some(1));
        assert_eq!(events[0].resource_id, Some(2));
    }
}
