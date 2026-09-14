use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Counter used to make temporary write paths unique across concurrent writers.
static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// Build a per-operation unique temporary path next to `target`.
///
/// Concurrent writers may race on the same `ContentDigest` (e.g. multiple
/// threads caching identical content). Deriving the temp name from the digest
/// alone would make them all collide on the same file and overwrite/remove one
/// another mid-write. A unique suffix keeps each operation's temp file
/// isolated while the final `rename` stays atomic.
fn unique_temp_path(target: &Path) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let tid = std::thread::current().id();
    let suffix = format!("{nanos:x}-{seq:x}-{tid:?}");
    let file_name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "entry".to_string());
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    let unique = format!("{file_name}.tmp.{suffix}");
    parent.join(unique)
}

#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("NEXA-CACHE-0001: Invalid content digest format '{digest}'")]
    InvalidDigest { digest: String },
    #[error("NEXA-CACHE-0002: Cache entry not found for digest '{digest}'")]
    EntryNotFound { digest: String },
    #[error("NEXA-CACHE-0003: Cache directory not accessible at '{path}'")]
    CacheDirectoryNotAccessible { path: String },
    #[error("NEXA-CACHE-0004: Digest mismatch: expected '{expected}', found '{actual}'")]
    DigestMismatch { expected: String, actual: String },
    #[error("NEXA-CACHE-0005: Cache entry corrupted for digest '{digest}'")]
    EntryCorrupted { digest: String },
    #[error("NEXA-CACHE-0006: Cache directory creation failed at '{path}': {reason}")]
    DirectoryCreationFailed { path: String, reason: String },
    #[error("NEXA-CACHE-0007: Atomic write failed for digest '{digest}': {reason}")]
    AtomicWriteFailed { digest: String, reason: String },
    #[error("NEXA-CACHE-0008: Cache entry too large: {size} bytes exceeds maximum {max}")]
    EntryTooLarge { size: u64, max: u64 },
    #[error("NEXA-CACHE-0009: Read/write error at '{path}': {reason}")]
    IoError { path: String, reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContentDigest(pub String);

impl ContentDigest {
    pub fn new(digest: &str) -> Result<Self, CacheError> {
        if digest.len() != 64 {
            return Err(CacheError::InvalidDigest {
                digest: digest.to_string(),
            });
        }
        if !digest.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(CacheError::InvalidDigest {
                digest: digest.to_string(),
            });
        }
        Ok(ContentDigest(digest.to_string()))
    }

    pub fn sha256_hex(bytes: &[u8]) -> Self {
        let mut hasher = Sha256Hasher::new();
        hasher.update(bytes);
        let result = hasher.finalize();
        ContentDigest(hex_encode(&result))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_valid(&self) -> bool {
        self.0.len() == 64 && self.0.chars().all(|c| c.is_ascii_hexdigit())
    }
}

#[derive(Debug)]
pub struct CacheEntry {
    pub digest: ContentDigest,
    pub data: Vec<u8>,
}

pub struct CacheEntryMetadata {
    pub digest: ContentDigest,
    pub size: u64,
    pub cached: bool,
}

pub struct CacheIndex {
    pub entries: Vec<CacheEntryMetadata>,
}

pub struct PackageCache {
    base_path: PathBuf,
}

const MAX_ENTRY_SIZE: u64 = 100 * 1024 * 1024;

impl PackageCache {
    pub fn new(base_path: impl AsRef<Path>) -> Result<Self, CacheError> {
        let path = base_path.as_ref().to_path_buf();
        if path.exists() && !path.is_dir() {
            return Err(CacheError::CacheDirectoryNotAccessible {
                path: path.display().to_string(),
            });
        }
        fs::create_dir_all(&path).map_err(|e| CacheError::DirectoryCreationFailed {
            path: path.display().to_string(),
            reason: e.to_string(),
        })?;
        Ok(PackageCache { base_path: path })
    }

    pub fn default_path() -> PathBuf {
        dirs_home().join(".nexa").join("cache")
    }

    pub fn has(&self, digest: &ContentDigest) -> bool {
        self.digest_path(digest).exists()
    }

    pub fn get(&self, digest: &ContentDigest) -> Result<CacheEntry, CacheError> {
        let path = self.digest_path(digest);
        if !path.exists() {
            return Err(CacheError::EntryNotFound {
                digest: digest.0.clone(),
            });
        }
        let data = fs::read(&path).map_err(|e| CacheError::IoError {
            path: path.display().to_string(),
            reason: e.to_string(),
        })?;
        let computed = ContentDigest::sha256_hex(&data);
        if computed != *digest {
            return Err(CacheError::EntryCorrupted {
                digest: digest.0.clone(),
            });
        }
        Ok(CacheEntry {
            digest: digest.clone(),
            data,
        })
    }

    pub fn put(&self, data: &[u8]) -> Result<ContentDigest, CacheError> {
        if data.len() as u64 > MAX_ENTRY_SIZE {
            return Err(CacheError::EntryTooLarge {
                size: data.len() as u64,
                max: MAX_ENTRY_SIZE,
            });
        }
        let digest = ContentDigest::sha256_hex(data);
        if self.has(&digest) {
            return Ok(digest);
        }
        let target = self.digest_path(&digest);
        let parent = target.parent().unwrap();
        fs::create_dir_all(parent).map_err(|e| CacheError::DirectoryCreationFailed {
            path: parent.display().to_string(),
            reason: e.to_string(),
        })?;
        let temp_path = unique_temp_path(&target);
        fs::write(&temp_path, data).map_err(|e| CacheError::AtomicWriteFailed {
            digest: digest.0.clone(),
            reason: e.to_string(),
        })?;
        let verify_data = fs::read(&temp_path).map_err(|e| CacheError::AtomicWriteFailed {
            digest: digest.0.clone(),
            reason: e.to_string(),
        })?;
        let verify_digest = ContentDigest::sha256_hex(&verify_data);
        if verify_digest != digest {
            let _ = fs::remove_file(&temp_path);
            return Err(CacheError::DigestMismatch {
                expected: digest.0,
                actual: verify_digest.0,
            });
        }
        if let Err(e) = fs::rename(&temp_path, &target) {
            if !target.exists() {
                let _ = fs::remove_file(&temp_path);
                return Err(CacheError::AtomicWriteFailed {
                    digest: digest.0.clone(),
                    reason: e.to_string(),
                });
            }
            let _ = fs::remove_file(&temp_path);
        }
        Ok(digest)
    }

    pub fn store(&self, entry: CacheEntry) -> Result<(), CacheError> {
        if !self.has(&entry.digest) {
            let target = self.digest_path(&entry.digest);
            let parent = target.parent().unwrap();
            fs::create_dir_all(parent).map_err(|e| CacheError::DirectoryCreationFailed {
                path: parent.display().to_string(),
                reason: e.to_string(),
            })?;
            let temp_path = unique_temp_path(&target);
            fs::write(&temp_path, &entry.data).map_err(|e| CacheError::AtomicWriteFailed {
                digest: entry.digest.0.clone(),
                reason: e.to_string(),
            })?;
            fs::rename(&temp_path, &target).map_err(|e| CacheError::AtomicWriteFailed {
                digest: entry.digest.0.clone(),
                reason: e.to_string(),
            })?;
        }
        Ok(())
    }

    pub fn remove(&self, digest: &ContentDigest) -> Result<(), CacheError> {
        let path = self.digest_path(digest);
        if !path.exists() {
            return Err(CacheError::EntryNotFound {
                digest: digest.0.clone(),
            });
        }
        fs::remove_file(&path).map_err(|e| CacheError::IoError {
            path: path.display().to_string(),
            reason: e.to_string(),
        })?;
        Ok(())
    }

    pub fn index(&self) -> Result<CacheIndex, CacheError> {
        let sha_dir = self.base_path.join("sha256");
        let mut entries = Vec::new();
        if !sha_dir.exists() {
            return Ok(CacheIndex { entries });
        }
        let prefix_dirs = fs::read_dir(&sha_dir).map_err(|e| CacheError::IoError {
            path: sha_dir.display().to_string(),
            reason: e.to_string(),
        })?;
        for prefix_entry in prefix_dirs {
            let prefix_entry = prefix_entry.map_err(|e| CacheError::IoError {
                path: sha_dir.display().to_string(),
                reason: e.to_string(),
            })?;
            let prefix_path = prefix_entry.path();
            if !prefix_path.is_dir() {
                continue;
            }
            let files = fs::read_dir(&prefix_path).map_err(|e| CacheError::IoError {
                path: prefix_path.display().to_string(),
                reason: e.to_string(),
            })?;
            for file_entry in files {
                let file_entry = file_entry.map_err(|e| CacheError::IoError {
                    path: prefix_path.display().to_string(),
                    reason: e.to_string(),
                })?;
                let file_path = file_entry.path();
                if file_path.is_file() {
                    let file_name = file_path.file_name().unwrap().to_string_lossy().to_string();
                    let prefix_name = prefix_path
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .to_string();
                    let full_hex = format!("{}{}", prefix_name, file_name);
                    if let Ok(digest) = ContentDigest::new(&full_hex) {
                        let metadata =
                            fs::metadata(&file_path).map_err(|e| CacheError::IoError {
                                path: file_path.display().to_string(),
                                reason: e.to_string(),
                            })?;
                        entries.push(CacheEntryMetadata {
                            digest,
                            size: metadata.len(),
                            cached: true,
                        });
                    }
                }
            }
        }
        Ok(CacheIndex { entries })
    }

    pub fn digest_path(&self, digest: &ContentDigest) -> PathBuf {
        let hex = &digest.0;
        let prefix = &hex[..2];
        let rest = &hex[2..];
        self.base_path.join("sha256").join(prefix).join(rest)
    }

    pub fn verify(&self, digest: &ContentDigest) -> Result<bool, CacheError> {
        let path = self.digest_path(digest);
        if !path.exists() {
            return Ok(false);
        }
        let data = fs::read(&path).map_err(|e| CacheError::IoError {
            path: path.display().to_string(),
            reason: e.to_string(),
        })?;
        let computed = ContentDigest::sha256_hex(&data);
        Ok(computed == *digest)
    }
}

struct Sha256Hasher {
    state: [u32; 8],
    buffer: Vec<u8>,
    total_len: u64,
}

impl Sha256Hasher {
    fn new() -> Self {
        Sha256Hasher {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            buffer: Vec::new(),
            total_len: 0,
        }
    }

    fn update(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
        self.total_len += data.len() as u64;
        while self.buffer.len() >= 64 {
            let block: [u8; 64] = self.buffer[..64].try_into().unwrap();
            self.buffer.drain(..64);
            self.process_block(block);
        }
    }

    fn process_block(&mut self, block: [u8; 64]) {
        let k: [u32; 64] = [
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
            0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
            0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
            0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
            0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
            0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
            0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
            0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
            0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
            0xc67178f2,
        ];

        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(block[i * 4..(i + 1) * 4].try_into().unwrap());
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let mut a = self.state[0];
        let mut b = self.state[1];
        let mut c = self.state[2];
        let mut d = self.state[3];
        let mut e = self.state[4];
        let mut f = self.state[5];
        let mut g = self.state[6];
        let mut h = self.state[7];

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(k[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
        self.state[4] = self.state[4].wrapping_add(e);
        self.state[5] = self.state[5].wrapping_add(f);
        self.state[6] = self.state[6].wrapping_add(g);
        self.state[7] = self.state[7].wrapping_add(h);
    }

    fn finalize(mut self) -> [u8; 32] {
        let total_bits = self.total_len * 8;
        self.buffer.push(0x80);
        while (self.buffer.len() % 64) != 56 {
            self.buffer.push(0);
        }
        self.buffer.extend_from_slice(&total_bits.to_be_bytes());
        while self.buffer.len() >= 64 {
            let block: [u8; 64] = self.buffer[..64].try_into().unwrap();
            self.buffer.drain(..64);
            self.process_block(block);
        }
        let mut result = [0u8; 32];
        for i in 0..8 {
            result[i * 4..(i + 1) * 4].copy_from_slice(&self.state[i].to_be_bytes());
        }
        result
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn dirs_home() -> PathBuf {
    if cfg!(windows) {
        std::env::var("USERPROFILE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
    } else {
        std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_cache() -> PackageCache {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let tid = std::thread::current().id();
        let dir = std::env::temp_dir().join(format!("nexa-cache-test-{}-{:?}", ts, tid));
        PackageCache::new(&dir).unwrap()
    }

    #[test]
    fn content_digest_new_valid() {
        let hex = "a".repeat(64);
        let d = ContentDigest::new(&hex).unwrap();
        assert_eq!(d.as_str(), hex);
    }

    #[test]
    fn content_digest_new_too_short() {
        let result = ContentDigest::new("abc123");
        assert!(result.is_err());
        match result.unwrap_err() {
            CacheError::InvalidDigest { digest } => assert_eq!(digest, "abc123"),
            _ => panic!("expected InvalidDigest"),
        }
    }

    #[test]
    fn content_digest_new_too_long() {
        let hex = "a".repeat(65);
        let result = ContentDigest::new(&hex);
        assert!(result.is_err());
    }

    #[test]
    fn content_digest_new_non_hex() {
        let hex = "g".repeat(64);
        let result = ContentDigest::new(&hex);
        assert!(result.is_err());
    }

    #[test]
    fn content_digest_new_mixed_invalid() {
        let mut hex = "a".repeat(63);
        hex.push('z');
        let result = ContentDigest::new(&hex);
        assert!(result.is_err());
    }

    #[test]
    fn sha256_hex_correct_length() {
        let digest = ContentDigest::sha256_hex(b"hello world");
        assert_eq!(digest.as_str().len(), 64);
    }

    #[test]
    fn sha256_hex_deterministic() {
        let d1 = ContentDigest::sha256_hex(b"test data");
        let d2 = ContentDigest::sha256_hex(b"test data");
        assert_eq!(d1, d2);
    }

    #[test]
    fn sha256_hex_different_for_different_data() {
        let d1 = ContentDigest::sha256_hex(b"hello");
        let d2 = ContentDigest::sha256_hex(b"world");
        assert_ne!(d1, d2);
    }

    #[test]
    fn is_valid_true_for_correct_digest() {
        let d = ContentDigest::sha256_hex(b"test");
        assert!(d.is_valid());
    }

    #[test]
    fn is_valid_false_for_short() {
        let d = ContentDigest("abc".to_string());
        assert!(!d.is_valid());
    }

    #[test]
    fn is_valid_false_for_non_hex() {
        let mut s = "a".repeat(64);
        s.push('x');
        let d = ContentDigest(s);
        assert!(!d.is_valid());
    }

    #[test]
    fn package_cache_new() {
        let cache = temp_cache();
        assert!(cache.base_path.exists());
    }

    #[test]
    fn has_returns_false_for_nonexistent() {
        let cache = temp_cache();
        let digest = ContentDigest::sha256_hex(b"nothing");
        assert!(!cache.has(&digest));
    }

    #[test]
    fn put_stores_and_returns_digest() {
        let cache = temp_cache();
        let data = b"hello nexa";
        let digest = cache.put(data).unwrap();
        assert!(cache.has(&digest));
        assert_eq!(digest, ContentDigest::sha256_hex(data));
    }

    #[test]
    fn get_retrieves_stored_data() {
        let cache = temp_cache();
        let data = b"retrieve me";
        let digest = cache.put(data).unwrap();
        let entry = cache.get(&digest).unwrap();
        assert_eq!(entry.data, data);
        assert_eq!(entry.digest, digest);
    }

    #[test]
    fn get_fails_for_nonexistent() {
        let cache = temp_cache();
        let digest = ContentDigest::sha256_hex(b"ghost");
        let result = cache.get(&digest);
        assert!(result.is_err());
        match result.unwrap_err() {
            CacheError::EntryNotFound { digest: d } => {
                assert_eq!(d, digest.as_str());
            }
            _ => panic!("expected EntryNotFound"),
        }
    }

    #[test]
    fn store_and_get_roundtrip() {
        let cache = temp_cache();
        let data = b"roundtrip data";
        let digest = ContentDigest::sha256_hex(data);
        let entry = CacheEntry {
            digest: digest.clone(),
            data: data.to_vec(),
        };
        cache.store(entry).unwrap();
        let retrieved = cache.get(&digest).unwrap();
        assert_eq!(retrieved.data, data);
    }

    #[test]
    fn remove_deletes_entry() {
        let cache = temp_cache();
        let data = b"remove me";
        let digest = cache.put(data).unwrap();
        assert!(cache.has(&digest));
        cache.remove(&digest).unwrap();
        assert!(!cache.has(&digest));
    }

    #[test]
    fn remove_fails_for_nonexistent() {
        let cache = temp_cache();
        let digest = ContentDigest::sha256_hex(b"nothing here");
        let result = cache.remove(&digest);
        assert!(result.is_err());
    }

    #[test]
    fn verify_confirms_integrity() {
        let cache = temp_cache();
        let data = b"verify this";
        let digest = cache.put(data).unwrap();
        assert!(cache.verify(&digest).unwrap());
    }

    #[test]
    fn verify_detects_corruption() {
        let cache = temp_cache();
        let data = b"corrupt me";
        let digest = cache.put(data).unwrap();
        let path = cache.digest_path(&digest);
        fs::write(&path, b"corrupted data").unwrap();
        assert!(!cache.verify(&digest).unwrap());
    }

    #[test]
    fn verify_returns_false_for_missing() {
        let cache = temp_cache();
        let digest = ContentDigest::sha256_hex(b"missing");
        assert!(!cache.verify(&digest).unwrap());
    }

    #[test]
    fn index_lists_all_entries() {
        let cache = temp_cache();
        let d1 = cache.put(b"one").unwrap();
        let d2 = cache.put(b"two").unwrap();
        let d3 = cache.put(b"three").unwrap();
        let idx = cache.index().unwrap();
        assert_eq!(idx.entries.len(), 3);
        let digests: Vec<_> = idx.entries.iter().map(|e| &e.digest).collect();
        assert!(digests.contains(&&d1));
        assert!(digests.contains(&&d2));
        assert!(digests.contains(&&d3));
    }

    #[test]
    fn index_empty_cache() {
        let cache = temp_cache();
        let idx = cache.index().unwrap();
        assert!(idx.entries.is_empty());
    }

    #[test]
    fn digest_path_structure() {
        let cache = temp_cache();
        let digest = ContentDigest::new(&("aabbccdd".to_string() + &"ee".repeat(28))).unwrap();
        let path = cache.digest_path(&digest);
        let _components: Vec<_> = path.components().collect();
        let path_str = path.to_string_lossy();
        assert!(path_str.contains("sha256"));
        assert!(path_str.contains("aa"));
    }

    #[test]
    fn put_idempotent() {
        let cache = temp_cache();
        let data = b"idempotent content";
        let d1 = cache.put(data).unwrap();
        let d2 = cache.put(data).unwrap();
        assert_eq!(d1, d2);
    }

    #[test]
    fn concurrent_put_same_content() {
        let cache = temp_cache();
        let data = b"concurrent data";
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let path = cache.base_path.clone();
                thread::spawn(move || {
                    let c = PackageCache::new(&path).unwrap();
                    c.put(data).unwrap()
                })
            })
            .collect();
        let digests: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        for d in &digests {
            assert_eq!(*d, digests[0]);
        }
        assert!(cache.has(&digests[0]));
    }

    #[test]
    fn large_data_storage() {
        let cache = temp_cache();
        let data = vec![0xABu8; 1024 * 1024];
        let digest = cache.put(&data).unwrap();
        let retrieved = cache.get(&digest).unwrap();
        assert_eq!(retrieved.data.len(), data.len());
        assert_eq!(retrieved.data, data);
    }

    #[test]
    fn empty_data_storage() {
        let cache = temp_cache();
        let data = b"";
        let digest = cache.put(data).unwrap();
        let retrieved = cache.get(&digest).unwrap();
        assert!(retrieved.data.is_empty());
    }

    #[test]
    fn index_metadata_size() {
        let cache = temp_cache();
        let data = b"size check";
        let _digest = cache.put(data).unwrap();
        let idx = cache.index().unwrap();
        assert_eq!(idx.entries.len(), 1);
        assert_eq!(idx.entries[0].size, data.len() as u64);
        assert!(idx.entries[0].cached);
    }

    #[test]
    fn default_path_contains_nexa() {
        let p = PackageCache::default_path();
        let s = p.to_string_lossy();
        assert!(s.contains(".nexa"));
        assert!(s.contains("cache"));
    }

    #[test]
    fn put_rejects_oversized_entry() {
        let cache = temp_cache();
        let data = vec![0xFFu8; MAX_ENTRY_SIZE as usize + 1];
        let result = cache.put(&data);
        assert!(result.is_err());
        match result.unwrap_err() {
            CacheError::EntryTooLarge { size, max } => {
                assert_eq!(size, MAX_ENTRY_SIZE + 1);
                assert_eq!(max, MAX_ENTRY_SIZE);
            }
            _ => panic!("expected EntryTooLarge"),
        }
    }

    #[test]
    fn store_does_not_overwrite_existing() {
        let cache = temp_cache();
        let data1 = b"original";
        let digest = cache.put(data1).unwrap();
        let data2 = b"replacement";
        let entry = CacheEntry {
            digest: ContentDigest::sha256_hex(data2),
            data: data2.to_vec(),
        };
        cache.store(entry).unwrap();
        let retrieved = cache.get(&digest).unwrap();
        assert_eq!(retrieved.data, data1);
    }
}
