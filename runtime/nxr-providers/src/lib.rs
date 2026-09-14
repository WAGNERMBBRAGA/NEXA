use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::string::FromUtf8Error;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

// ─── ProviderError ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum ProviderError {
    NotFound(String),
    PermissionDenied(String),
    AlreadyExists(String),
    InvalidHandle(u64),
    IoError(String),
    LimitExceeded(String),
    Utf8Error(String),
    Unavailable(String),
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(s) => write!(f, "not found: {s}"),
            Self::PermissionDenied(s) => write!(f, "permission denied: {s}"),
            Self::AlreadyExists(s) => write!(f, "already exists: {s}"),
            Self::InvalidHandle(h) => write!(f, "invalid handle: {h}"),
            Self::IoError(s) => write!(f, "I/O error: {s}"),
            Self::LimitExceeded(s) => write!(f, "limit exceeded: {s}"),
            Self::Utf8Error(s) => write!(f, "UTF-8 error: {s}"),
            Self::Unavailable(s) => write!(f, "unavailable: {s}"),
        }
    }
}

impl std::error::Error for ProviderError {}

// ─── ConsoleProvider ──────────────────────────────────────────────────────

pub trait ConsoleProvider: Send + Sync {
    fn write_stdout(&self, data: &[u8]) -> Result<(), ProviderError>;
    fn write_stderr(&self, data: &[u8]) -> Result<(), ProviderError>;
    fn read_line(&self) -> Result<Vec<u8>, ProviderError>;
}

pub struct InMemoryConsole {
    stdout: Mutex<Vec<u8>>,
    stderr: Mutex<Vec<u8>>,
    input: Mutex<Vec<Vec<u8>>>,
}

impl InMemoryConsole {
    pub fn new() -> Self {
        Self {
            stdout: Mutex::new(Vec::new()),
            stderr: Mutex::new(Vec::new()),
            input: Mutex::new(Vec::new()),
        }
    }

    pub fn stdout(&self) -> Vec<u8> {
        self.stdout.lock().unwrap().clone()
    }

    pub fn stderr(&self) -> Vec<u8> {
        self.stderr.lock().unwrap().clone()
    }

    pub fn push_input(&self, data: &[u8]) {
        self.input.lock().unwrap().push(data.to_vec());
    }

    pub fn stdout_string(&self) -> Result<String, FromUtf8Error> {
        String::from_utf8(self.stdout())
    }

    pub fn stderr_string(&self) -> Result<String, FromUtf8Error> {
        String::from_utf8(self.stderr())
    }
}

impl Default for InMemoryConsole {
    fn default() -> Self {
        Self::new()
    }
}

impl ConsoleProvider for InMemoryConsole {
    fn write_stdout(&self, data: &[u8]) -> Result<(), ProviderError> {
        self.stdout.lock().unwrap().extend_from_slice(data);
        Ok(())
    }

    fn write_stderr(&self, data: &[u8]) -> Result<(), ProviderError> {
        self.stderr.lock().unwrap().extend_from_slice(data);
        Ok(())
    }

    fn read_line(&self) -> Result<Vec<u8>, ProviderError> {
        let mut input = self.input.lock().unwrap();
        if input.is_empty() {
            return Err(ProviderError::IoError("no input available".into()));
        }
        Ok(input.remove(0))
    }
}

// ─── ClockProvider ────────────────────────────────────────────────────────

pub trait ClockProvider: Send + Sync {
    fn now_ns(&self) -> u64;
    fn monotonic_ns(&self) -> u64;
}

pub struct SystemClock;

impl ClockProvider for SystemClock {
    fn now_ns(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64
    }

    fn monotonic_ns(&self) -> u64 {
        static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
        let start = *START.get_or_init(Instant::now);
        // Some Windows timers report zero during the first clock tick. A
        // production monotonic clock must still return a positive timestamp;
        // the deterministic VirtualClock remains zero-based for tests.
        (start.elapsed().as_nanos() as u64).max(1)
    }
}

pub struct VirtualClock {
    now_ns: AtomicU64,
    monotonic_ns: AtomicU64,
}

impl VirtualClock {
    pub fn new() -> Self {
        Self {
            now_ns: AtomicU64::new(0),
            monotonic_ns: AtomicU64::new(0),
        }
    }

    pub fn advance_now_ns(&self, delta: u64) {
        self.now_ns.fetch_add(delta, Ordering::SeqCst);
    }

    pub fn advance_monotonic_ns(&self, delta: u64) {
        self.monotonic_ns.fetch_add(delta, Ordering::SeqCst);
    }

    pub fn set_now_ns(&self, value: u64) {
        self.now_ns.store(value, Ordering::SeqCst);
    }

    pub fn set_monotonic_ns(&self, value: u64) {
        self.monotonic_ns.store(value, Ordering::SeqCst);
    }
}

impl Default for VirtualClock {
    fn default() -> Self {
        Self::new()
    }
}

impl ClockProvider for VirtualClock {
    fn now_ns(&self) -> u64 {
        self.now_ns.load(Ordering::SeqCst)
    }

    fn monotonic_ns(&self) -> u64 {
        self.monotonic_ns.load(Ordering::SeqCst)
    }
}

// ─── RandomProvider ───────────────────────────────────────────────────────

pub trait RandomProvider: Send + Sync {
    fn fill_bytes(&self, buf: &mut [u8]) -> Result<(), ProviderError>;
    fn next_u64(&self) -> Result<u64, ProviderError>;
}

pub struct InMemoryRandom {
    seed: AtomicU64,
}

impl InMemoryRandom {
    pub fn new(seed: u64) -> Self {
        Self {
            seed: AtomicU64::new(seed),
        }
    }

    pub fn seed(&self) -> u64 {
        self.seed.load(Ordering::SeqCst)
    }

    fn xorshift64(&self) -> u64 {
        let mut x = self.seed.load(Ordering::SeqCst);
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.seed.store(x, Ordering::SeqCst);
        x
    }
}

impl RandomProvider for InMemoryRandom {
    fn fill_bytes(&self, buf: &mut [u8]) -> Result<(), ProviderError> {
        let mut offset = 0;
        while offset < buf.len() {
            let val = self.xorshift64().to_ne_bytes();
            let remaining = buf.len() - offset;
            let to_copy = remaining.min(8);
            buf[offset..offset + to_copy].copy_from_slice(&val[..to_copy]);
            offset += to_copy;
        }
        Ok(())
    }

    fn next_u64(&self) -> Result<u64, ProviderError> {
        Ok(self.xorshift64())
    }
}

// ─── EnvironmentProvider ──────────────────────────────────────────────────

pub trait EnvironmentProvider: Send + Sync {
    fn get_var(&self, name: &str) -> Result<Option<String>, ProviderError>;
    fn list_vars(&self) -> Result<Vec<String>, ProviderError>;
}

pub struct InMemoryEnvironment {
    vars: Mutex<BTreeMap<String, String>>,
    allowed: Mutex<BTreeSet<String>>,
}

impl InMemoryEnvironment {
    pub fn new() -> Self {
        Self {
            vars: Mutex::new(BTreeMap::new()),
            allowed: Mutex::new(BTreeSet::new()),
        }
    }

    pub fn set_var(&self, name: &str, value: &str) {
        self.vars
            .lock()
            .unwrap()
            .insert(name.to_string(), value.to_string());
    }

    pub fn allow_var(&self, name: &str) {
        self.allowed.lock().unwrap().insert(name.to_string());
    }

    pub fn allow_all(&self) {
        drop(self.allowed.lock().unwrap());
    }

    pub fn is_allow_all(&self) -> bool {
        let allowed = self.allowed.lock().unwrap();
        allowed.is_empty() && self.vars.lock().unwrap().is_empty()
    }
}

impl Default for InMemoryEnvironment {
    fn default() -> Self {
        Self::new()
    }
}

impl EnvironmentProvider for InMemoryEnvironment {
    fn get_var(&self, name: &str) -> Result<Option<String>, ProviderError> {
        let allowed = self.allowed.lock().unwrap();
        if allowed.is_empty() {
            // No restrictions configured — all allowed
            let vars = self.vars.lock().unwrap();
            return Ok(vars.get(name).cloned());
        }
        if !allowed.contains(name) {
            return Err(ProviderError::PermissionDenied(name.to_string()));
        }
        let vars = self.vars.lock().unwrap();
        Ok(vars.get(name).cloned())
    }

    fn list_vars(&self) -> Result<Vec<String>, ProviderError> {
        let allowed = self.allowed.lock().unwrap();
        let vars = self.vars.lock().unwrap();
        if allowed.is_empty() {
            return Ok(vars.keys().cloned().collect());
        }
        Ok(vars
            .keys()
            .filter(|k| allowed.contains(k.as_str()))
            .cloned()
            .collect())
    }
}

// ─── FileMetadata ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct FileMetadata {
    pub size: u64,
    pub is_dir: bool,
    pub is_file: bool,
}

// ─── InMemFsNode ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum InMemFsNode {
    File {
        content: Vec<u8>,
    },
    Directory {
        children: BTreeMap<String, InMemFsNode>,
    },
}

impl InMemFsNode {
    fn as_file(&self) -> Option<&Vec<u8>> {
        match self {
            Self::File { content } => Some(content),
            _ => None,
        }
    }

    fn as_file_mut(&mut self) -> Option<&mut Vec<u8>> {
        match self {
            Self::File { content } => Some(content),
            _ => None,
        }
    }

    fn as_dir(&self) -> Option<&BTreeMap<String, InMemFsNode>> {
        match self {
            Self::Directory { children } => Some(children),
            _ => None,
        }
    }

    fn as_dir_mut(&mut self) -> Option<&mut BTreeMap<String, InMemFsNode>> {
        match self {
            Self::Directory { children } => Some(children),
            _ => None,
        }
    }
}

// ─── InMemFsHandle ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct InMemFsHandle {
    path: Vec<String>,
    read: bool,
    write: bool,
    create: bool,
}

// ─── FilesystemProvider ───────────────────────────────────────────────────

pub trait FilesystemProvider: Send + Sync {
    fn open_file(
        &self,
        path: &str,
        read: bool,
        write: bool,
        create: bool,
    ) -> Result<u64, ProviderError>;
    fn read_file(&self, handle: u64, buf: &mut [u8]) -> Result<usize, ProviderError>;
    fn write_file(&self, handle: u64, data: &[u8]) -> Result<usize, ProviderError>;
    fn close(&self, handle: u64) -> Result<(), ProviderError>;
    fn stat(&self, path: &str) -> Result<FileMetadata, ProviderError>;
    fn create_dir(&self, path: &str) -> Result<(), ProviderError>;
    fn remove(&self, path: &str) -> Result<(), ProviderError>;
    fn list_dir(&self, path: &str) -> Result<Vec<String>, ProviderError>;
}

fn validate_path(path: &str) -> Result<Vec<&str>, ProviderError> {
    if path.contains('\0') {
        return Err(ProviderError::IoError("path contains NUL byte".into()));
    }
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    for part in &parts {
        if *part == ".." {
            return Err(ProviderError::PermissionDenied(
                "path traversal not allowed".into(),
            ));
        }
    }
    Ok(parts)
}

fn navigate_to_parent<'a>(
    root: &'a mut InMemFsNode,
    path_parts: &[&str],
) -> Result<&'a mut InMemFsNode, ProviderError> {
    let mut current = root;
    for part in &path_parts[..path_parts.len().saturating_sub(1)] {
        current = current
            .as_dir_mut()
            .ok_or_else(|| ProviderError::IoError("not a directory".into()))?
            .get_mut(*part)
            .ok_or_else(|| ProviderError::NotFound(part.to_string()))?;
    }
    Ok(current)
}

pub struct InMemoryFilesystem {
    root: Mutex<InMemFsNode>,
    handles: Mutex<BTreeMap<u64, InMemFsHandle>>,
    next_handle: AtomicU64,
    _max_open: u32,
}

impl InMemoryFilesystem {
    pub fn new(max_open: u32) -> Self {
        Self {
            root: Mutex::new(InMemFsNode::Directory {
                children: BTreeMap::new(),
            }),
            handles: Mutex::new(BTreeMap::new()),
            next_handle: AtomicU64::new(1),
            _max_open: max_open,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(128)
    }
}

impl FilesystemProvider for InMemoryFilesystem {
    fn open_file(
        &self,
        path: &str,
        read: bool,
        write: bool,
        create: bool,
    ) -> Result<u64, ProviderError> {
        let parts = validate_path(path)?;
        if parts.is_empty() {
            return Err(ProviderError::IoError("empty path".into()));
        }

        let mut root = self.root.lock().unwrap();
        let parent = navigate_to_parent(&mut root, &parts)?;
        let file_name = *parts.last().unwrap();

        if create {
            let dir_children = parent
                .as_dir_mut()
                .ok_or_else(|| ProviderError::IoError("parent is not a directory".into()))?;
            dir_children
                .entry(file_name.to_string())
                .or_insert_with(|| InMemFsNode::File {
                    content: Vec::new(),
                });
        }

        let parent_children = parent
            .as_dir()
            .ok_or_else(|| ProviderError::IoError("parent is not a directory".into()))?;
        parent_children
            .get(file_name)
            .ok_or_else(|| ProviderError::NotFound(file_name.to_string()))?;

        let handle_id = self.next_handle.fetch_add(1, Ordering::SeqCst);
        let path_parts: Vec<String> = parts.iter().map(|s| s.to_string()).collect();
        self.handles.lock().unwrap().insert(
            handle_id,
            InMemFsHandle {
                path: path_parts,
                read,
                write,
                create,
            },
        );
        Ok(handle_id)
    }

    fn read_file(&self, handle: u64, buf: &mut [u8]) -> Result<usize, ProviderError> {
        let handles = self.handles.lock().unwrap();
        let fs_handle = handles
            .get(&handle)
            .ok_or(ProviderError::InvalidHandle(handle))?;

        if !fs_handle.read {
            return Err(ProviderError::PermissionDenied(
                "file not opened for reading".into(),
            ));
        }

        let root = self.root.lock().unwrap();
        let mut current = &*root;
        for part in &fs_handle.path {
            current = current
                .as_dir()
                .and_then(|d| d.get(part))
                .ok_or_else(|| ProviderError::NotFound(part.clone()))?;
        }
        let content = current
            .as_file()
            .ok_or_else(|| ProviderError::IoError("not a file".into()))?;
        let to_read = buf.len().min(content.len());
        buf[..to_read].copy_from_slice(&content[..to_read]);
        Ok(to_read)
    }

    fn write_file(&self, handle: u64, data: &[u8]) -> Result<usize, ProviderError> {
        let handles = self.handles.lock().unwrap();
        let fs_handle = handles
            .get(&handle)
            .ok_or(ProviderError::InvalidHandle(handle))?;

        if !fs_handle.write {
            return Err(ProviderError::PermissionDenied(
                "file not opened for writing".into(),
            ));
        }

        let mut root = self.root.lock().unwrap();
        let mut current = &mut *root;
        for part in &fs_handle.path {
            current = current
                .as_dir_mut()
                .and_then(|d| d.get_mut(part))
                .ok_or_else(|| ProviderError::NotFound(part.clone()))?;
        }
        let content = current
            .as_file_mut()
            .ok_or_else(|| ProviderError::IoError("not a file".into()))?;
        content.clear();
        content.extend_from_slice(data);
        Ok(data.len())
    }

    fn close(&self, handle: u64) -> Result<(), ProviderError> {
        let mut handles = self.handles.lock().unwrap();
        handles
            .remove(&handle)
            .ok_or(ProviderError::InvalidHandle(handle))?;
        Ok(())
    }

    fn stat(&self, path: &str) -> Result<FileMetadata, ProviderError> {
        let parts = validate_path(path)?;
        if parts.is_empty() {
            return Ok(FileMetadata {
                size: 0,
                is_dir: true,
                is_file: false,
            });
        }

        let root = self.root.lock().unwrap();
        let mut current = &*root;
        for part in &parts {
            current = current
                .as_dir()
                .and_then(|d| d.get(*part))
                .ok_or_else(|| ProviderError::NotFound(part.to_string()))?;
        }
        match current {
            InMemFsNode::File { content } => Ok(FileMetadata {
                size: content.len() as u64,
                is_dir: false,
                is_file: true,
            }),
            InMemFsNode::Directory { .. } => Ok(FileMetadata {
                size: 0,
                is_dir: true,
                is_file: false,
            }),
        }
    }

    fn create_dir(&self, path: &str) -> Result<(), ProviderError> {
        let parts = validate_path(path)?;
        if parts.is_empty() {
            return Err(ProviderError::IoError("empty path".into()));
        }

        let mut root = self.root.lock().unwrap();
        let parent = navigate_to_parent(&mut root, &parts)?;
        let dir_name = *parts.last().unwrap();

        let dir_children = parent
            .as_dir_mut()
            .ok_or_else(|| ProviderError::IoError("parent is not a directory".into()))?;
        if dir_children.contains_key(dir_name) {
            return Err(ProviderError::AlreadyExists(dir_name.to_string()));
        }
        dir_children.insert(
            dir_name.to_string(),
            InMemFsNode::Directory {
                children: BTreeMap::new(),
            },
        );
        Ok(())
    }

    fn remove(&self, path: &str) -> Result<(), ProviderError> {
        let parts = validate_path(path)?;
        if parts.is_empty() {
            return Err(ProviderError::IoError("empty path".into()));
        }

        let mut root = self.root.lock().unwrap();
        let parent = navigate_to_parent(&mut root, &parts)?;
        let file_name = *parts.last().unwrap();

        let dir_children = parent
            .as_dir_mut()
            .ok_or_else(|| ProviderError::IoError("parent is not a directory".into()))?;
        dir_children
            .remove(file_name)
            .ok_or_else(|| ProviderError::NotFound(file_name.to_string()))?;
        Ok(())
    }

    fn list_dir(&self, path: &str) -> Result<Vec<String>, ProviderError> {
        let parts = validate_path(path)?;

        let root = self.root.lock().unwrap();
        let mut current = &*root;
        for part in &parts {
            current = current
                .as_dir()
                .and_then(|d| d.get(*part))
                .ok_or_else(|| ProviderError::NotFound(part.to_string()))?;
        }
        let children = current
            .as_dir()
            .ok_or_else(|| ProviderError::IoError("not a directory".into()))?;
        Ok(children.keys().cloned().collect())
    }
}

// ─── NetworkProvider ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct NetworkRoute {
    pub host: String,
    pub port: u16,
    pub response: Vec<u8>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct NetworkConnection {
    host: String,
    port: u16,
    data: Vec<u8>,
}

pub trait NetworkProvider: Send + Sync {
    fn connect(&self, host: &str, port: u16) -> Result<u64, ProviderError>;
    fn send(&self, handle: u64, data: &[u8]) -> Result<(), ProviderError>;
    fn receive(&self, handle: u64, buf: &mut [u8]) -> Result<usize, ProviderError>;
    fn close(&self, handle: u64) -> Result<(), ProviderError>;
}

pub struct InMemoryNetwork {
    routes: Mutex<Vec<NetworkRoute>>,
    connections: Mutex<BTreeMap<u64, NetworkConnection>>,
    next_handle: AtomicU64,
    allowed_hosts: Mutex<BTreeSet<String>>,
}

impl InMemoryNetwork {
    pub fn new() -> Self {
        Self {
            routes: Mutex::new(Vec::new()),
            connections: Mutex::new(BTreeMap::new()),
            next_handle: AtomicU64::new(1),
            allowed_hosts: Mutex::new(BTreeSet::new()),
        }
    }

    pub fn add_route(&self, route: NetworkRoute) {
        self.routes.lock().unwrap().push(route);
    }

    pub fn allow_host(&self, host: &str) {
        self.allowed_hosts.lock().unwrap().insert(host.to_string());
    }
}

impl Default for InMemoryNetwork {
    fn default() -> Self {
        Self::new()
    }
}

impl NetworkProvider for InMemoryNetwork {
    fn connect(&self, host: &str, port: u16) -> Result<u64, ProviderError> {
        let allowed = self.allowed_hosts.lock().unwrap();
        if !allowed.is_empty() && !allowed.contains(host) {
            return Err(ProviderError::PermissionDenied(format!(
                "host '{host}' not allowed"
            )));
        }

        let routes = self.routes.lock().unwrap();
        let route = routes.iter().find(|r| r.host == host && r.port == port);

        let response_data = match route {
            Some(r) => r.response.clone(),
            None => {
                drop(routes);
                drop(allowed);
                return Err(ProviderError::NotFound(format!(
                    "no route for {host}:{port}"
                )));
            }
        };

        let handle_id = self.next_handle.fetch_add(1, Ordering::SeqCst);
        self.connections.lock().unwrap().insert(
            handle_id,
            NetworkConnection {
                host: host.to_string(),
                port,
                data: response_data,
            },
        );
        Ok(handle_id)
    }

    fn send(&self, handle: u64, _data: &[u8]) -> Result<(), ProviderError> {
        let connections = self.connections.lock().unwrap();
        if !connections.contains_key(&handle) {
            return Err(ProviderError::InvalidHandle(handle));
        }
        Ok(())
    }

    fn receive(&self, handle: u64, buf: &mut [u8]) -> Result<usize, ProviderError> {
        let mut connections = self.connections.lock().unwrap();
        let conn = connections
            .get_mut(&handle)
            .ok_or(ProviderError::InvalidHandle(handle))?;

        let to_read = buf.len().min(conn.data.len());
        buf[..to_read].copy_from_slice(&conn.data[..to_read]);
        conn.data.drain(..to_read);
        Ok(to_read)
    }

    fn close(&self, handle: u64) -> Result<(), ProviderError> {
        let mut connections = self.connections.lock().unwrap();
        connections
            .remove(&handle)
            .ok_or(ProviderError::InvalidHandle(handle))?;
        Ok(())
    }
}

// ─── ProcessProvider ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ProcessExit {
    pub exit_code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct SpawnRecord {
    pub executable: String,
    pub argv: Vec<String>,
    pub env: Vec<(String, String)>,
}

pub trait ProcessProvider: Send + Sync {
    fn spawn(
        &self,
        executable: &str,
        argv: &[String],
        env: &[(String, String)],
    ) -> Result<u64, ProviderError>;
    fn wait(&self, handle: u64) -> Result<ProcessExit, ProviderError>;
    fn kill(&self, handle: u64) -> Result<(), ProviderError>;
}

pub struct InMemoryProcess {
    simulated_exits: Mutex<BTreeMap<String, ProcessExit>>,
    spawned: Mutex<Vec<SpawnRecord>>,
    next_handle: AtomicU64,
}

impl InMemoryProcess {
    pub fn new() -> Self {
        Self {
            simulated_exits: Mutex::new(BTreeMap::new()),
            spawned: Mutex::new(Vec::new()),
            next_handle: AtomicU64::new(1),
        }
    }

    pub fn add_exit(&self, executable: &str, exit: ProcessExit) {
        self.simulated_exits
            .lock()
            .unwrap()
            .insert(executable.to_string(), exit);
    }

    pub fn spawned_records(&self) -> Vec<SpawnRecord> {
        self.spawned.lock().unwrap().clone()
    }
}

impl Default for InMemoryProcess {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessProvider for InMemoryProcess {
    fn spawn(
        &self,
        executable: &str,
        argv: &[String],
        env: &[(String, String)],
    ) -> Result<u64, ProviderError> {
        self.spawned.lock().unwrap().push(SpawnRecord {
            executable: executable.to_string(),
            argv: argv.to_vec(),
            env: env.to_vec(),
        });

        let handle_id = self.next_handle.fetch_add(1, Ordering::SeqCst);
        // Insert a default exit so wait() doesn't return NotFound
        let exits = self.simulated_exits.lock().unwrap();
        if !exits.contains_key(executable) {
            drop(exits);
            self.simulated_exits.lock().unwrap().insert(
                executable.to_string(),
                ProcessExit {
                    exit_code: 0,
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                },
            );
        }
        Ok(handle_id)
    }

    fn wait(&self, handle: u64) -> Result<ProcessExit, ProviderError> {
        // Find the spawn record by handle index (handle starts at 1)
        let records = self.spawned.lock().unwrap();
        let idx = (handle - 1) as usize;
        let record = records
            .get(idx)
            .ok_or(ProviderError::InvalidHandle(handle))?;
        let executable = record.executable.clone();
        drop(records);

        let exits = self.simulated_exits.lock().unwrap();
        let exit = exits.get(&executable).cloned().unwrap_or(ProcessExit {
            exit_code: 0,
            stdout: Vec::new(),
            stderr: Vec::new(),
        });
        Ok(exit)
    }

    fn kill(&self, handle: u64) -> Result<(), ProviderError> {
        let records = self.spawned.lock().unwrap();
        let idx = (handle - 1) as usize;
        if idx >= records.len() {
            return Err(ProviderError::InvalidHandle(handle));
        }
        Ok(())
    }
}

// ─── SecretsProvider ──────────────────────────────────────────────────────

pub trait SecretsProvider: Send + Sync {
    fn resolve(&self, secret_id: &str) -> Result<Vec<u8>, ProviderError>;
    fn list_available(&self) -> Result<Vec<String>, ProviderError>;
}

pub struct InMemorySecrets {
    secrets: Mutex<BTreeMap<String, Vec<u8>>>,
}

impl InMemorySecrets {
    pub fn new() -> Self {
        Self {
            secrets: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn add_secret(&self, id: &str, value: &[u8]) {
        self.secrets
            .lock()
            .unwrap()
            .insert(id.to_string(), value.to_vec());
    }
}

impl Default for InMemorySecrets {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretsProvider for InMemorySecrets {
    fn resolve(&self, secret_id: &str) -> Result<Vec<u8>, ProviderError> {
        let secrets = self.secrets.lock().unwrap();
        secrets
            .get(secret_id)
            .cloned()
            .ok_or_else(|| ProviderError::NotFound(secret_id.to_string()))
    }

    fn list_available(&self) -> Result<Vec<String>, ProviderError> {
        let secrets = self.secrets.lock().unwrap();
        Ok(secrets.keys().cloned().collect())
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_console_write_stdout() {
        let console = InMemoryConsole::new();
        console.write_stdout(b"hello stdout").unwrap();
        assert_eq!(console.stdout(), b"hello stdout");
        assert_eq!(console.stdout_string().unwrap(), "hello stdout");
    }

    #[test]
    fn test_console_write_stderr() {
        let console = InMemoryConsole::new();
        console.write_stderr(b"error msg").unwrap();
        assert_eq!(console.stderr(), b"error msg");
        assert_eq!(console.stderr_string().unwrap(), "error msg");
    }

    #[test]
    fn test_console_read_line() {
        let console = InMemoryConsole::new();
        console.push_input(b"first line");
        console.push_input(b"second line");
        assert_eq!(console.read_line().unwrap(), b"first line");
        assert_eq!(console.read_line().unwrap(), b"second line");
        assert!(console.read_line().is_err());
    }

    #[test]
    fn test_system_clock_now() {
        let clock = SystemClock;
        assert!(clock.now_ns() > 0);
        assert!(clock.monotonic_ns() > 0);
    }

    #[test]
    fn test_virtual_clock_advance() {
        let clock = VirtualClock::new();
        assert_eq!(clock.now_ns(), 0);
        assert_eq!(clock.monotonic_ns(), 0);

        clock.advance_now_ns(100);
        assert_eq!(clock.now_ns(), 100);

        clock.advance_monotonic_ns(200);
        assert_eq!(clock.monotonic_ns(), 200);

        clock.set_now_ns(5000);
        assert_eq!(clock.now_ns(), 5000);

        clock.set_monotonic_ns(7000);
        assert_eq!(clock.monotonic_ns(), 7000);
    }

    #[test]
    fn test_random_fill_bytes() {
        let rng = InMemoryRandom::new(12345);
        let mut buf = [0u8; 32];
        rng.fill_bytes(&mut buf).unwrap();
        // Should have filled the buffer (not all zeros)
        assert_ne!(buf, [0u8; 32]);
    }

    #[test]
    fn test_random_deterministic() {
        let rng1 = InMemoryRandom::new(42);
        let rng2 = InMemoryRandom::new(42);
        let mut buf1 = [0u8; 64];
        let mut buf2 = [0u8; 64];
        rng1.fill_bytes(&mut buf1).unwrap();
        rng2.fill_bytes(&mut buf2).unwrap();
        assert_eq!(buf1, buf2);

        // Different seeds produce different output
        let rng3 = InMemoryRandom::new(99);
        let mut buf3 = [0u8; 64];
        rng3.fill_bytes(&mut buf3).unwrap();
        assert_ne!(buf1, buf3);
    }

    #[test]
    fn test_environment_get_set() {
        let env = InMemoryEnvironment::new();
        env.set_var("HOME", "/root");
        env.set_var("PATH", "/usr/bin");

        assert_eq!(env.get_var("HOME").unwrap(), Some("/root".to_string()));
        assert_eq!(env.get_var("PATH").unwrap(), Some("/usr/bin".to_string()));
        assert_eq!(env.get_var("MISSING").unwrap(), None);
    }

    #[test]
    fn test_environment_allowed_only() {
        let env = InMemoryEnvironment::new();
        env.set_var("SECRET", "hidden");
        env.set_var("PUBLIC", "visible");
        env.allow_var("PUBLIC");

        assert_eq!(env.get_var("PUBLIC").unwrap(), Some("visible".to_string()));
        assert!(env.get_var("SECRET").is_err());

        let vars = env.list_vars().unwrap();
        assert_eq!(vars, vec!["PUBLIC".to_string()]);
    }

    #[test]
    fn test_filesystem_open_close() {
        let fs = InMemoryFilesystem::with_defaults();
        let handle = fs.open_file("/test.txt", true, true, true).unwrap();
        fs.close(handle).unwrap();
        assert!(fs.close(handle).is_err());
    }

    #[test]
    fn test_filesystem_read_write() {
        let fs = InMemoryFilesystem::with_defaults();
        let handle = fs.open_file("/data.txt", true, true, true).unwrap();

        let written = fs.write_file(handle, b"hello filesystem").unwrap();
        assert_eq!(written, 16);

        let mut buf = [0u8; 64];
        let read = fs.read_file(handle, &mut buf).unwrap();
        assert_eq!(read, 16);
        assert_eq!(&buf[..16], b"hello filesystem");

        fs.close(handle).unwrap();
    }

    #[test]
    fn test_filesystem_create_dir() {
        let fs = InMemoryFilesystem::with_defaults();
        fs.create_dir("/apps").unwrap();
        fs.create_dir("/apps/myapp").unwrap();

        let children = fs.list_dir("/").unwrap();
        assert!(children.contains(&"apps".to_string()));

        let sub_children = fs.list_dir("/apps").unwrap();
        assert!(sub_children.contains(&"myapp".to_string()));

        // Duplicate dir
        assert!(fs.create_dir("/apps").is_err());
    }

    #[test]
    fn test_filesystem_list_dir() {
        let fs = InMemoryFilesystem::with_defaults();
        fs.open_file("/a.txt", true, true, true).unwrap();
        fs.open_file("/b.txt", true, true, true).unwrap();
        fs.create_dir("/sub").unwrap();

        let mut children = fs.list_dir("/").unwrap();
        children.sort();
        assert_eq!(
            children,
            vec!["a.txt".to_string(), "b.txt".to_string(), "sub".to_string()]
        );
    }

    #[test]
    fn test_filesystem_path_traversal_rejected() {
        let fs = InMemoryFilesystem::with_defaults();
        assert!(fs.open_file("/../etc/passwd", true, false, false).is_err());
        assert!(fs.create_dir("/..").is_err());
        assert!(fs.remove("/../../secret").is_err());
    }

    #[test]
    fn test_filesystem_stat() {
        let fs = InMemoryFilesystem::with_defaults();
        fs.open_file("/file.txt", true, true, true).unwrap();
        fs.write_file(1, b"hello").unwrap();
        fs.close(1).unwrap();

        let meta = fs.stat("/file.txt").unwrap();
        assert!(meta.is_file);
        assert!(!meta.is_dir);
        assert_eq!(meta.size, 5);

        fs.create_dir("/mydir").unwrap();
        let meta_dir = fs.stat("/mydir").unwrap();
        assert!(!meta_dir.is_file);
        assert!(meta_dir.is_dir);
    }

    #[test]
    fn test_network_connect_send_receive() {
        let net = InMemoryNetwork::new();
        net.allow_host("example.com");
        net.add_route(NetworkRoute {
            host: "example.com".to_string(),
            port: 80,
            response: b"HTTP/1.1 200 OK".to_vec(),
        });

        let h = net.connect("example.com", 80).unwrap();
        net.send(h, b"GET /").unwrap();

        let mut buf = [0u8; 64];
        let n = net.receive(h, &mut buf).unwrap();
        assert_eq!(&buf[..n], b"HTTP/1.1 200 OK");

        net.close(h).unwrap();
        assert!(net.close(h).is_err());
    }

    #[test]
    fn test_network_host_restriction() {
        let net = InMemoryNetwork::new();
        net.allow_host("safe.com");
        net.add_route(NetworkRoute {
            host: "evil.com".to_string(),
            port: 443,
            response: b"bad".to_vec(),
        });

        assert!(net.connect("evil.com", 443).is_err());
    }

    #[test]
    fn test_process_spawn_wait() {
        let proc = InMemoryProcess::new();
        proc.add_exit(
            "ls",
            ProcessExit {
                exit_code: 0,
                stdout: b"file1\nfile2\n".to_vec(),
                stderr: Vec::new(),
            },
        );

        let h = proc
            .spawn(
                "ls",
                &["-l".to_string()],
                &[("PATH".into(), "/usr/bin".into())],
            )
            .unwrap();
        let exit = proc.wait(h).unwrap();
        assert_eq!(exit.exit_code, 0);
        assert_eq!(exit.stdout, b"file1\nfile2\n");

        let records = proc.spawned_records();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].executable, "ls");
    }

    #[test]
    fn test_secrets_resolve() {
        let secrets = InMemorySecrets::new();
        secrets.add_secret("api_key", b"super-secret-123");
        secrets.add_secret("db_pass", b"p4ssw0rd");

        assert_eq!(
            secrets.resolve("api_key").unwrap(),
            b"super-secret-123".to_vec()
        );
        assert_eq!(secrets.resolve("db_pass").unwrap(), b"p4ssw0rd".to_vec());
        assert!(secrets.resolve("missing").is_err());

        let mut available = secrets.list_available().unwrap();
        available.sort();
        assert_eq!(
            available,
            vec!["api_key".to_string(), "db_pass".to_string()]
        );
    }

    #[test]
    fn test_provider_error_display() {
        let cases = vec![
            ProviderError::NotFound("x".into()),
            ProviderError::PermissionDenied("y".into()),
            ProviderError::AlreadyExists("z".into()),
            ProviderError::InvalidHandle(42),
            ProviderError::IoError("disk".into()),
            ProviderError::LimitExceeded("files".into()),
            ProviderError::Utf8Error("bad".into()),
            ProviderError::Unavailable("net".into()),
        ];
        for e in cases {
            let msg = e.to_string();
            assert!(!msg.is_empty());
        }
    }
}
