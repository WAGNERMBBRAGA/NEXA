use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TaskId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskKind {
    Compile,
    Link,
    Package,
    Verify,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    Pending,
    Running,
    Completed,
    Failed,
    Skipped,
}

pub struct BuildTask {
    pub id: TaskId,
    pub kind: TaskKind,
    pub package: String,
    pub state: TaskState,
    pub input_paths: Vec<String>,
    pub output_path: Option<String>,
}

pub struct BuildEdge {
    pub from: TaskId,
    pub to: TaskId,
}

pub struct BuildGraph {
    pub tasks: Vec<BuildTask>,
    pub edges: Vec<BuildEdge>,
}

#[derive(Debug, Clone)]
pub struct BuildFingerprint {
    pub package: String,
    pub version: String,
    pub source_digest: String,
    pub manifest_digest: String,
    pub lockfile_digest: String,
    pub combined: String,
}

pub struct ArtifactCache {
    fingerprints: HashMap<String, BuildFingerprint>,
    outputs: HashMap<String, Vec<u8>>,
}

pub struct BuildArtifact {
    pub package: String,
    pub path: String,
    pub size: u64,
    pub digest: String,
}

pub struct BuildResult {
    pub success: bool,
    pub tasks_completed: u32,
    pub tasks_failed: u32,
    pub artifacts: Vec<BuildArtifact>,
    pub duration_ms: u64,
}

pub struct BuildConfig {
    pub release: bool,
    pub target: String,
    pub output_dir: String,
    pub cache_dir: String,
}

#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error("NEXA-BUILD-0001: Build graph has cycle involving task '{package}'")]
    BuildCycle { package: String },
    #[error("NEXA-BUILD-0002: Missing dependency '{dep}' for task '{package}'")]
    MissingDependency { package: String, dep: String },
    #[error("NEXA-BUILD-0003: Fingerprint mismatch for '{package}': build not reproducible")]
    FingerprintMismatch { package: String },
    #[error("NEXA-BUILD-0004: Build task '{package}' failed: {reason}")]
    TaskFailed { package: String, reason: String },
    #[error("NEXA-BUILD-0005: Output directory '{path}' not writable")]
    OutputNotWritable { path: String },
    #[error("NEXA-BUILD-0006: Source discovery failed for '{package}': {reason}")]
    SourceDiscoveryFailed { package: String, reason: String },
    #[error("NEXA-BUILD-0007: Lockfile required but not found")]
    LockfileRequired,
    #[error("NEXA-BUILD-0008: Package '{package}' not in lockfile")]
    PackageNotInLockfile { package: String },
    #[error("NEXA-BUILD-0009: Cannot build offline: package '{package}' not cached")]
    OfflineBuildFailed { package: String },
    #[error("NEXA-BUILD-0010: Atomic write failed for artifact '{path}': {reason}")]
    AtomicWriteFailed { path: String, reason: String },
}

fn simple_hash(data: &[u8]) -> String {
    let mut h: u64 = 14695981039346656037;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(1099511628211);
    }
    format!("{:016x}", h)
}

fn combine_hashes(parts: &[&str]) -> String {
    let combined = parts.join("|");
    simple_hash(combined.as_bytes())
}

impl BuildGraph {
    pub fn new() -> Self {
        Self {
            tasks: Vec::new(),
            edges: Vec::new(),
        }
    }

    pub fn add_task(&mut self, task: BuildTask) -> Result<(), BuildError> {
        self.tasks.push(task);
        Ok(())
    }

    pub fn add_edge(&mut self, from: TaskId, to: TaskId) -> Result<(), BuildError> {
        self.edges.push(BuildEdge { from, to });
        Ok(())
    }

    pub fn task_count(&self) -> usize {
        self.tasks.len()
    }

    pub fn tasks_for_package(&self, package: &str) -> Vec<&BuildTask> {
        self.tasks.iter().filter(|t| t.package == package).collect()
    }

    pub fn has_cycle(&self) -> bool {
        self.topological_order().is_err()
    }

    fn task_index(&self, id: &TaskId) -> Option<usize> {
        self.tasks.iter().position(|t| t.id == *id)
    }

    pub fn topological_order(&self) -> Result<Vec<TaskId>, BuildError> {
        let n = self.tasks.len();
        let mut in_degree = vec![0u32; n];
        let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];

        for edge in &self.edges {
            if let (Some(from_idx), Some(to_idx)) =
                (self.task_index(&edge.from), self.task_index(&edge.to))
            {
                adj[from_idx].push(to_idx);
                in_degree[to_idx] += 1;
            }
        }

        let mut queue: Vec<usize> = Vec::new();
        for (i, &degree) in in_degree.iter().enumerate() {
            if degree == 0 {
                queue.push(i);
            }
        }

        let mut order = Vec::new();
        let mut visited = 0;

        while let Some(node) = queue.pop() {
            order.push(self.tasks[node].id.clone());
            visited += 1;

            for &neighbor in &adj[node] {
                in_degree[neighbor] -= 1;
                if in_degree[neighbor] == 0 {
                    queue.push(neighbor);
                }
            }
        }

        if visited != n {
            for (i, &degree) in in_degree.iter().enumerate() {
                if degree > 0 {
                    return Err(BuildError::BuildCycle {
                        package: self.tasks[i].package.clone(),
                    });
                }
            }
            return Err(BuildError::BuildCycle {
                package: self.tasks[0].package.clone(),
            });
        }

        Ok(order)
    }

    pub fn build_order(&self) -> Result<Vec<&BuildTask>, BuildError> {
        let order = self.topological_order()?;
        Ok(order
            .into_iter()
            .filter_map(|id| self.tasks.iter().find(|t| t.id == id))
            .collect())
    }
}

impl BuildFingerprint {
    pub fn compute(
        package: &str,
        version: &str,
        source_bytes: &[u8],
        manifest_bytes: &[u8],
        lockfile_bytes: &[u8],
    ) -> Self {
        let source_digest = simple_hash(source_bytes);
        let manifest_digest = simple_hash(manifest_bytes);
        let lockfile_digest = simple_hash(lockfile_bytes);
        let combined = combine_hashes(&[&source_digest, &manifest_digest, &lockfile_digest]);

        Self {
            package: package.to_string(),
            version: version.to_string(),
            source_digest,
            manifest_digest,
            lockfile_digest,
            combined,
        }
    }

    pub fn matches(&self, other: &BuildFingerprint) -> bool {
        self.combined == other.combined
    }

    pub fn as_str(&self) -> &str {
        &self.combined
    }
}

impl ArtifactCache {
    pub fn new() -> Self {
        Self {
            fingerprints: HashMap::new(),
            outputs: HashMap::new(),
        }
    }

    pub fn has_artifact(&self, package: &str) -> bool {
        self.outputs.contains_key(package)
    }

    pub fn get_artifact(&self, package: &str) -> Option<&Vec<u8>> {
        self.outputs.get(package)
    }

    pub fn store_artifact(&mut self, package: &str, data: Vec<u8>, fingerprint: BuildFingerprint) {
        self.fingerprints.insert(package.to_string(), fingerprint);
        self.outputs.insert(package.to_string(), data);
    }

    pub fn is_fresh(&self, package: &str, fingerprint: &BuildFingerprint) -> bool {
        match self.fingerprints.get(package) {
            Some(cached) => cached.matches(fingerprint),
            None => false,
        }
    }

    pub fn invalidate(&mut self, package: &str) {
        self.fingerprints.remove(package);
        self.outputs.remove(package);
    }

    pub fn artifact_count(&self) -> usize {
        self.outputs.len()
    }
}

impl Default for BuildGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for ArtifactCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn make_task(id: u32, package: &str) -> BuildTask {
        BuildTask {
            id: TaskId(id),
            kind: TaskKind::Compile,
            package: package.to_string(),
            state: TaskState::Pending,
            input_paths: Vec::new(),
            output_path: None,
        }
    }

    #[test]
    fn build_graph_creation() {
        let g = BuildGraph::new();
        assert_eq!(g.task_count(), 0);
        assert!(g.tasks.is_empty());
        assert!(g.edges.is_empty());
    }

    #[test]
    fn build_graph_add_task() {
        let mut g = BuildGraph::new();
        let t = make_task(0, "a");
        g.add_task(t).unwrap();
        assert_eq!(g.task_count(), 1);
    }

    #[test]
    fn build_graph_add_edge() {
        let mut g = BuildGraph::new();
        g.add_task(make_task(0, "a")).unwrap();
        g.add_task(make_task(1, "b")).unwrap();
        g.add_edge(TaskId(0), TaskId(1)).unwrap();
        assert_eq!(g.edges.len(), 1);
    }

    #[test]
    fn topological_order_linear() {
        let mut g = BuildGraph::new();
        g.add_task(make_task(0, "a")).unwrap();
        g.add_task(make_task(1, "b")).unwrap();
        g.add_task(make_task(2, "c")).unwrap();
        g.add_edge(TaskId(0), TaskId(1)).unwrap();
        g.add_edge(TaskId(1), TaskId(2)).unwrap();
        let order = g.topological_order().unwrap();
        assert_eq!(order.len(), 3);
        assert_eq!(order[0], TaskId(0));
        assert_eq!(order[1], TaskId(1));
        assert_eq!(order[2], TaskId(2));
    }

    #[test]
    fn topological_order_diamond() {
        let mut g = BuildGraph::new();
        g.add_task(make_task(0, "a")).unwrap();
        g.add_task(make_task(1, "b")).unwrap();
        g.add_task(make_task(2, "c")).unwrap();
        g.add_task(make_task(3, "d")).unwrap();
        g.add_edge(TaskId(0), TaskId(1)).unwrap();
        g.add_edge(TaskId(0), TaskId(2)).unwrap();
        g.add_edge(TaskId(1), TaskId(3)).unwrap();
        g.add_edge(TaskId(2), TaskId(3)).unwrap();
        let order = g.topological_order().unwrap();
        assert_eq!(order.len(), 4);
        let pos_a = order.iter().position(|id| *id == TaskId(0)).unwrap();
        let pos_d = order.iter().position(|id| *id == TaskId(3)).unwrap();
        assert!(pos_a < pos_d);
    }

    #[test]
    fn topological_order_cycle_detection() {
        let mut g = BuildGraph::new();
        g.add_task(make_task(0, "a")).unwrap();
        g.add_task(make_task(1, "b")).unwrap();
        g.add_edge(TaskId(0), TaskId(1)).unwrap();
        g.add_edge(TaskId(1), TaskId(0)).unwrap();
        assert!(g.topological_order().is_err());
    }

    #[test]
    fn has_cycle() {
        let mut g = BuildGraph::new();
        g.add_task(make_task(0, "a")).unwrap();
        g.add_task(make_task(1, "b")).unwrap();
        g.add_edge(TaskId(0), TaskId(1)).unwrap();
        g.add_edge(TaskId(1), TaskId(0)).unwrap();
        assert!(g.has_cycle());
    }

    #[test]
    fn no_cycle_in_acyclic_graph() {
        let mut g = BuildGraph::new();
        g.add_task(make_task(0, "a")).unwrap();
        g.add_task(make_task(1, "b")).unwrap();
        g.add_edge(TaskId(0), TaskId(1)).unwrap();
        assert!(!g.has_cycle());
    }

    #[test]
    fn fingerprint_same_inputs() {
        let f1 = BuildFingerprint::compute("pkg", "1.0", b"src", b"man", b"lock");
        let f2 = BuildFingerprint::compute("pkg", "1.0", b"src", b"man", b"lock");
        assert!(f1.matches(&f2));
        assert_eq!(f1.combined, f2.combined);
    }

    #[test]
    fn fingerprint_different_inputs() {
        let f1 = BuildFingerprint::compute("pkg", "1.0", b"src1", b"man", b"lock");
        let f2 = BuildFingerprint::compute("pkg", "1.0", b"src2", b"man", b"lock");
        assert!(!f1.matches(&f2));
    }

    #[test]
    fn fingerprint_as_str() {
        let f = BuildFingerprint::compute("pkg", "1.0", b"s", b"m", b"l");
        assert_eq!(f.as_str(), &f.combined);
    }

    #[test]
    fn fingerprint_mismatch_error() {
        let f1 = BuildFingerprint::compute("pkg", "1.0", b"a", b"b", b"c");
        let f2 = BuildFingerprint::compute("pkg", "1.0", b"x", b"y", b"z");
        assert!(!f1.matches(&f2));
    }

    #[test]
    fn artifact_cache_store_and_retrieve() {
        let mut cache = ArtifactCache::new();
        let fp = BuildFingerprint::compute("pkg", "1.0", b"s", b"m", b"l");
        cache.store_artifact("pkg", vec![1, 2, 3], fp);
        assert!(cache.has_artifact("pkg"));
        assert_eq!(cache.get_artifact("pkg"), Some(&vec![1, 2, 3]));
    }

    #[test]
    fn artifact_cache_freshness() {
        let mut cache = ArtifactCache::new();
        let fp = BuildFingerprint::compute("pkg", "1.0", b"s", b"m", b"l");
        cache.store_artifact("pkg", vec![1], fp.clone());
        assert!(cache.is_fresh("pkg", &fp));
        let fp2 = BuildFingerprint::compute("pkg", "1.0", b"s2", b"m", b"l");
        assert!(!cache.is_fresh("pkg", &fp2));
    }

    #[test]
    fn artifact_cache_invalidation() {
        let mut cache = ArtifactCache::new();
        let fp = BuildFingerprint::compute("pkg", "1.0", b"s", b"m", b"l");
        cache.store_artifact("pkg", vec![1], fp);
        assert!(cache.has_artifact("pkg"));
        cache.invalidate("pkg");
        assert!(!cache.has_artifact("pkg"));
    }

    #[test]
    fn artifact_cache_not_found() {
        let cache = ArtifactCache::new();
        assert!(!cache.has_artifact("nope"));
        assert!(cache.get_artifact("nope").is_none());
    }

    #[test]
    fn artifact_cache_count() {
        let mut cache = ArtifactCache::new();
        assert_eq!(cache.artifact_count(), 0);
        let fp = BuildFingerprint::compute("a", "1", b"s", b"m", b"l");
        cache.store_artifact("a", vec![1], fp);
        assert_eq!(cache.artifact_count(), 1);
        let fp2 = BuildFingerprint::compute("b", "1", b"s", b"m", b"l");
        cache.store_artifact("b", vec![2], fp2);
        assert_eq!(cache.artifact_count(), 2);
    }

    #[test]
    fn topological_order_multiple_roots() {
        let mut g = BuildGraph::new();
        g.add_task(make_task(0, "a")).unwrap();
        g.add_task(make_task(1, "b")).unwrap();
        g.add_task(make_task(2, "c")).unwrap();
        g.add_edge(TaskId(0), TaskId(2)).unwrap();
        g.add_edge(TaskId(1), TaskId(2)).unwrap();
        let order = g.topological_order().unwrap();
        assert_eq!(order.len(), 3);
        let pos_c = order.iter().position(|id| *id == TaskId(2)).unwrap();
        assert_eq!(pos_c, 2);
    }

    #[test]
    fn empty_build_graph() {
        let g = BuildGraph::new();
        let order = g.topological_order().unwrap();
        assert!(order.is_empty());
    }

    #[test]
    fn task_state_transitions() {
        let mut t = make_task(0, "pkg");
        assert_eq!(t.state, TaskState::Pending);
        t.state = TaskState::Running;
        assert_eq!(t.state, TaskState::Running);
        t.state = TaskState::Completed;
        assert_eq!(t.state, TaskState::Completed);
    }

    #[test]
    fn task_kind_classification() {
        let mut t = make_task(0, "pkg");
        assert_eq!(t.kind, TaskKind::Compile);
        t.kind = TaskKind::Link;
        assert_eq!(t.kind, TaskKind::Link);
        t.kind = TaskKind::Package;
        assert_eq!(t.kind, TaskKind::Package);
        t.kind = TaskKind::Verify;
        assert_eq!(t.kind, TaskKind::Verify);
    }

    #[test]
    fn build_artifact_creation() {
        let a = BuildArtifact {
            package: "pkg".to_string(),
            path: "out/pkg.nexa".to_string(),
            size: 1024,
            digest: "abc123".to_string(),
        };
        assert_eq!(a.package, "pkg");
        assert_eq!(a.size, 1024);
    }

    #[test]
    fn build_result_summary() {
        let r = BuildResult {
            success: true,
            tasks_completed: 10,
            tasks_failed: 0,
            artifacts: Vec::new(),
            duration_ms: 500,
        };
        assert!(r.success);
        assert_eq!(r.tasks_completed, 10);
        assert_eq!(r.tasks_failed, 0);
        assert_eq!(r.duration_ms, 500);
    }

    #[test]
    fn build_order_respects_dependencies() {
        let mut g = BuildGraph::new();
        g.add_task(make_task(0, "base")).unwrap();
        g.add_task(make_task(1, "mid")).unwrap();
        g.add_task(make_task(2, "top")).unwrap();
        g.add_edge(TaskId(0), TaskId(1)).unwrap();
        g.add_edge(TaskId(1), TaskId(2)).unwrap();
        let order = g.build_order().unwrap();
        assert_eq!(order.len(), 3);
        assert_eq!(order[0].package, "base");
        assert_eq!(order[1].package, "mid");
        assert_eq!(order[2].package, "top");
    }

    #[test]
    fn large_build_graph_performance() {
        let start = Instant::now();
        let mut g = BuildGraph::new();
        for i in 0..1000 {
            g.add_task(make_task(i, &format!("pkg{}", i))).unwrap();
        }
        for i in 0..999 {
            g.add_edge(TaskId(i), TaskId(i + 1)).unwrap();
        }
        let order = g.topological_order().unwrap();
        assert_eq!(order.len(), 1000);
        assert!(start.elapsed().as_millis() < 5000);
    }

    #[test]
    fn tasks_for_package() {
        let mut g = BuildGraph::new();
        g.add_task(make_task(0, "a")).unwrap();
        g.add_task(make_task(1, "a")).unwrap();
        g.add_task(make_task(2, "b")).unwrap();
        let found = g.tasks_for_package("a");
        assert_eq!(found.len(), 2);
        let found_b = g.tasks_for_package("b");
        assert_eq!(found_b.len(), 1);
        let found_c = g.tasks_for_package("c");
        assert!(found_c.is_empty());
    }

    #[test]
    fn cache_idempotency() {
        let mut cache = ArtifactCache::new();
        let fp = BuildFingerprint::compute("pkg", "1.0", b"s", b"m", b"l");
        cache.store_artifact("pkg", vec![1, 2], fp.clone());
        cache.store_artifact("pkg", vec![3, 4], fp.clone());
        assert_eq!(cache.artifact_count(), 1);
        assert_eq!(cache.get_artifact("pkg"), Some(&vec![3, 4]));
    }

    #[test]
    fn build_graph_default() {
        let g = BuildGraph::default();
        assert_eq!(g.task_count(), 0);
    }

    #[test]
    fn artifact_cache_default() {
        let c = ArtifactCache::default();
        assert_eq!(c.artifact_count(), 0);
    }

    #[test]
    fn build_config_fields() {
        let cfg = BuildConfig {
            release: true,
            target: "wasm32-unknown-unknown".to_string(),
            output_dir: "target/release".to_string(),
            cache_dir: ".nexa/cache".to_string(),
        };
        assert!(cfg.release);
        assert_eq!(cfg.target, "wasm32-unknown-unknown");
    }
}
