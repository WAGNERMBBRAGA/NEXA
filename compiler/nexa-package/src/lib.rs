use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const NXP1_MAGIC: &[u8; 4] = b"NXP1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageEntryKind {
    RegularFile,
    Directory,
}

#[derive(Debug)]
pub struct PackageEntry {
    pub path: String,
    pub kind: PackageEntryKind,
    pub content: Vec<u8>,
    pub digest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageEntryInfo {
    pub path: String,
    pub size: u64,
    pub digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageDependency {
    pub alias: String,
    pub package: String,
    pub version_req: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentMetadata {
    pub total_size: u64,
    pub entry_count: u32,
    pub package_digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageMetadata {
    pub name: String,
    pub version: String,
    pub format_version: u32,
    pub language_version: String,
    pub public_interface_fingerprint: String,
    pub entries: Vec<PackageEntryInfo>,
    pub dependencies: Vec<PackageDependency>,
    pub content_metadata: ContentMetadata,
}

#[derive(Debug)]
pub struct NxpArchive {
    pub magic: [u8; 4],
    pub entry_count: u32,
    pub manifest_offset: u32,
    pub entries: Vec<PackageEntry>,
    pub manifest: PackageMetadata,
}

#[derive(Debug)]
pub struct NxpIndex {
    pub entries: Vec<PackageEntryInfo>,
}

#[derive(Debug, thiserror::Error)]
pub enum PackageError {
    #[error("NEXA-PACKAGE-0001: Invalid NXP magic bytes: expected 'NXP1', got '{got}'")]
    InvalidMagic { got: String },
    #[error("NEXA-PACKAGE-0002: Package entry path '{path}' is invalid: {reason}")]
    InvalidEntryPath { path: String, reason: String },
    #[error("NEXA-PACKAGE-0003: Duplicate entry path '{path}'")]
    DuplicateEntry { path: String },
    #[error("NEXA-PACKAGE-0004: Package manifest missing required field '{field}'")]
    MissingField { field: String },
    #[error(
        "NEXA-PACKAGE-0005: Package digest mismatch: expected '{expected}', computed '{actual}'"
    )]
    DigestMismatch { expected: String, actual: String },
    #[error("NEXA-PACKAGE-0006: Package exceeds maximum size: {size} > {max}")]
    PackageTooLarge { size: u64, max: u64 },
    #[error("NEXA-PACKAGE-0007: Entry '{path}' size mismatch: header {header}, actual {actual}")]
    EntrySizeMismatch {
        path: String,
        header: u64,
        actual: u64,
    },
    #[error("NEXA-PACKAGE-0008: Package path '{path}' contains forbidden components: {reason}")]
    ForbiddenPath { path: String, reason: String },
    #[error("NEXA-PACKAGE-0009: Path case collision detected: '{path_a}' vs '{path_b}'")]
    CaseCollision { path_a: String, path_b: String },
    #[error("NEXA-PACKAGE-0010: Missing mandatory META/nexa-package.manifest entry")]
    MissingManifest,
    #[error("NEXA-PACKAGE-0011: Invalid package format version {0}")]
    InvalidFormatVersion(u32),
    #[error("NEXA-PACKAGE-0012: Package deserialization failed: {reason}")]
    DeserializationFailed { reason: String },
    #[error("NEXA-PACKAGE-0013: Forbidden entry kind: only regular files are allowed")]
    ForbiddenEntryKind,
    #[error("NEXA-PACKAGE-0014: Package contains empty entry list")]
    EmptyPackage,
}

const MAX_PACKAGE_SIZE: u64 = 256 * 1024 * 1024;

fn validate_path(path: &str) -> Result<(), PackageError> {
    if path.is_empty() {
        return Err(PackageError::InvalidEntryPath {
            path: path.to_string(),
            reason: "empty path".to_string(),
        });
    }

    if path.as_bytes().contains(&b'\\') {
        return Err(PackageError::ForbiddenPath {
            path: path.to_string(),
            reason: "backslash".to_string(),
        });
    }

    if path.starts_with('/') {
        return Err(PackageError::InvalidEntryPath {
            path: path.to_string(),
            reason: "absolute path".to_string(),
        });
    }

    if path.len() >= 2 && path.as_bytes()[1] == b':' {
        return Err(PackageError::ForbiddenPath {
            path: path.to_string(),
            reason: "drive prefix".to_string(),
        });
    }

    for component in path.split('/') {
        if component.is_empty() {
            return Err(PackageError::InvalidEntryPath {
                path: path.to_string(),
                reason: "empty component".to_string(),
            });
        }
        if component == "." {
            return Err(PackageError::InvalidEntryPath {
                path: path.to_string(),
                reason: "'.' component".to_string(),
            });
        }
        if component == ".." {
            return Err(PackageError::InvalidEntryPath {
                path: path.to_string(),
                reason: "'..' component".to_string(),
            });
        }
    }

    Ok(())
}

fn check_case_collisions(paths: &[&str]) -> Result<(), PackageError> {
    let mut seen: HashMap<String, &str> = HashMap::new();
    for &path in paths {
        let lower = path.to_lowercase();
        if let Some(&existing) = seen.get(&lower) {
            return Err(PackageError::CaseCollision {
                path_a: existing.to_string(),
                path_b: path.to_string(),
            });
        }
        seen.insert(lower, path);
    }
    Ok(())
}

fn compute_entry_digest(content: &[u8]) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn compute_entries_digest(entries: &[PackageEntry]) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    let mut sorted_entries: Vec<&PackageEntry> = entries.iter().collect();
    sorted_entries.sort_by_key(|e| e.path.clone());
    for entry in sorted_entries {
        entry.path.hash(&mut hasher);
        entry.content.hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

impl NxpArchive {
    pub fn new(manifest: PackageMetadata) -> Self {
        Self {
            magic: *NXP1_MAGIC,
            entry_count: 0,
            manifest_offset: 0,
            entries: Vec::new(),
            manifest,
        }
    }

    pub fn add_entry(&mut self, path: &str, content: Vec<u8>) -> Result<(), PackageError> {
        validate_path(path)?;

        if self.entries.iter().any(|e| e.path == path) {
            return Err(PackageError::DuplicateEntry {
                path: path.to_string(),
            });
        }

        let digest = compute_entry_digest(&content);

        self.entries.push(PackageEntry {
            path: path.to_string(),
            kind: PackageEntryKind::RegularFile,
            content,
            digest: Some(digest),
        });

        self.entry_count = self.entries.len() as u32;

        Ok(())
    }

    pub fn build(&self) -> Vec<u8> {
        let mut buf = Vec::new();

        buf.extend_from_slice(&self.magic);
        buf.extend_from_slice(&self.entry_count.to_le_bytes());
        let manifest_offset_placeholder = buf.len() as u32;
        buf.extend_from_slice(&0u32.to_le_bytes());

        for entry in &self.entries {
            let path_bytes = entry.path.as_bytes();
            buf.extend_from_slice(&(path_bytes.len() as u32).to_le_bytes());
            buf.extend_from_slice(path_bytes);
            buf.extend_from_slice(&(entry.content.len() as u32).to_le_bytes());
            buf.extend_from_slice(&entry.content);
            let digest_bytes = entry.digest.as_deref().unwrap_or("").as_bytes();
            buf.extend_from_slice(&(digest_bytes.len() as u32).to_le_bytes());
            buf.extend_from_slice(digest_bytes);
        }

        let manifest_offset = buf.len() as u32;
        let manifest_json = serde_json::to_vec(&self.manifest).unwrap();
        buf.extend_from_slice(&manifest_json);

        let manifest_offset_pos = manifest_offset_placeholder as usize;
        buf[manifest_offset_pos..manifest_offset_pos + 4]
            .copy_from_slice(&manifest_offset.to_le_bytes());

        buf
    }

    pub fn parse(data: &[u8]) -> Result<Self, PackageError> {
        if data.len() < 12 {
            return Err(PackageError::DeserializationFailed {
                reason: "data too short for header".to_string(),
            });
        }

        let magic = [data[0], data[1], data[2], data[3]];
        if magic != *NXP1_MAGIC {
            let got = String::from_utf8_lossy(&data[..4.min(data.len())]).to_string();
            return Err(PackageError::InvalidMagic { got });
        }

        let entry_count = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
        let manifest_offset = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);

        let mut cursor = 12usize;
        let mut entries = Vec::new();

        for _ in 0..entry_count {
            if cursor + 4 > data.len() {
                return Err(PackageError::DeserializationFailed {
                    reason: "unexpected end reading path_len".to_string(),
                });
            }
            let path_len = u32::from_le_bytes([
                data[cursor],
                data[cursor + 1],
                data[cursor + 2],
                data[cursor + 3],
            ]) as usize;
            cursor += 4;

            if cursor + path_len > data.len() {
                return Err(PackageError::DeserializationFailed {
                    reason: "unexpected end reading path".to_string(),
                });
            }
            let path =
                String::from_utf8(data[cursor..cursor + path_len].to_vec()).map_err(|e| {
                    PackageError::DeserializationFailed {
                        reason: format!("invalid UTF-8 in path: {}", e),
                    }
                })?;
            cursor += path_len;

            if cursor + 4 > data.len() {
                return Err(PackageError::DeserializationFailed {
                    reason: "unexpected end reading content_len".to_string(),
                });
            }
            let content_len = u32::from_le_bytes([
                data[cursor],
                data[cursor + 1],
                data[cursor + 2],
                data[cursor + 3],
            ]) as usize;
            cursor += 4;

            if cursor + content_len > data.len() {
                return Err(PackageError::DeserializationFailed {
                    reason: "unexpected end reading content".to_string(),
                });
            }
            let content = data[cursor..cursor + content_len].to_vec();
            cursor += content_len;

            if cursor + 4 > data.len() {
                return Err(PackageError::DeserializationFailed {
                    reason: "unexpected end reading digest_len".to_string(),
                });
            }
            let digest_len = u32::from_le_bytes([
                data[cursor],
                data[cursor + 1],
                data[cursor + 2],
                data[cursor + 3],
            ]) as usize;
            cursor += 4;

            if cursor + digest_len > data.len() {
                return Err(PackageError::DeserializationFailed {
                    reason: "unexpected end reading digest".to_string(),
                });
            }
            let digest_str = String::from_utf8(data[cursor..cursor + digest_len].to_vec())
                .map_err(|e| PackageError::DeserializationFailed {
                    reason: format!("invalid UTF-8 in digest: {}", e),
                })?;
            cursor += digest_len;

            let digest = if digest_str.is_empty() {
                None
            } else {
                Some(digest_str)
            };

            entries.push(PackageEntry {
                path,
                kind: PackageEntryKind::RegularFile,
                content,
                digest,
            });
        }

        let manifest_offset_usize = manifest_offset as usize;
        if manifest_offset_usize >= data.len() {
            return Err(PackageError::DeserializationFailed {
                reason: "manifest_offset out of bounds".to_string(),
            });
        }

        let manifest: PackageMetadata = serde_json::from_slice(&data[manifest_offset_usize..])
            .map_err(|e| PackageError::DeserializationFailed {
                reason: format!("manifest JSON parse error: {}", e),
            })?;

        Ok(Self {
            magic,
            entry_count,
            manifest_offset,
            entries,
            manifest,
        })
    }

    pub fn validate(&self) -> Result<(), Vec<PackageError>> {
        let mut errors = Vec::new();

        if self.magic != *NXP1_MAGIC {
            errors.push(PackageError::InvalidMagic {
                got: String::from_utf8_lossy(&self.magic).to_string(),
            });
        }

        if self.entries.is_empty() {
            errors.push(PackageError::EmptyPackage);
        }

        if self.manifest.format_version == 0 {
            errors.push(PackageError::InvalidFormatVersion(0));
        }

        let mut paths: Vec<&str> = Vec::new();
        for entry in &self.entries {
            if let Err(e) = validate_path(&entry.path) {
                errors.push(e);
            }
            paths.push(&entry.path);

            if entry.kind != PackageEntryKind::RegularFile {
                errors.push(PackageError::ForbiddenEntryKind);
            }
        }

        if let Err(e) = check_case_collisions(&paths) {
            errors.push(e);
        }

        let mut seen = std::collections::HashSet::new();
        for entry in &self.entries {
            if !seen.insert(entry.path.clone()) {
                errors.push(PackageError::DuplicateEntry {
                    path: entry.path.clone(),
                });
            }
        }

        let manifest_entry = self
            .entries
            .iter()
            .find(|e| e.path == "META/nexa-package.manifest");
        if manifest_entry.is_none() {
            errors.push(PackageError::MissingManifest);
        }

        let expected_digest = compute_entries_digest(&self.entries);
        if self.manifest.content_metadata.package_digest != expected_digest {
            errors.push(PackageError::DigestMismatch {
                expected: self.manifest.content_metadata.package_digest.clone(),
                actual: expected_digest,
            });
        }

        let total_size: u64 = self.entries.iter().map(|e| e.content.len() as u64).sum();
        if total_size > MAX_PACKAGE_SIZE {
            errors.push(PackageError::PackageTooLarge {
                size: total_size,
                max: MAX_PACKAGE_SIZE,
            });
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    pub fn compute_digest(&self) -> String {
        compute_entries_digest(&self.entries)
    }

    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    pub fn has_entry(&self, path: &str) -> bool {
        self.entries.iter().any(|e| e.path == path)
    }

    pub fn get_entry(&self, path: &str) -> Option<&PackageEntry> {
        self.entries.iter().find(|e| e.path == path)
    }

    pub fn manifest(&self) -> &PackageMetadata {
        &self.manifest
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_manifest(name: &str) -> PackageMetadata {
        PackageMetadata {
            name: name.to_string(),
            version: "0.1.0".to_string(),
            format_version: 1,
            language_version: "nexa-0.1".to_string(),
            public_interface_fingerprint: "abc123".to_string(),
            entries: Vec::new(),
            dependencies: Vec::new(),
            content_metadata: ContentMetadata {
                total_size: 0,
                entry_count: 0,
                package_digest: String::new(),
            },
        }
    }

    #[test]
    fn creation_and_build() {
        let manifest = make_manifest("test-pkg");
        let mut archive = NxpArchive::new(manifest);
        archive
            .add_entry("src/main.nexa", b"fn main() {}".to_vec())
            .unwrap();
        let data = archive.build();
        assert!(data.len() > 12);
        assert_eq!(&data[..4], b"NXP1");
    }

    #[test]
    fn parse_roundtrip() {
        let manifest = make_manifest("roundtrip-pkg");
        let mut archive = NxpArchive::new(manifest);
        archive
            .add_entry("src/main.nexa", b"fn main() {}".to_vec())
            .unwrap();
        archive.add_entry("README.md", b"# Hello".to_vec()).unwrap();
        let data = archive.build();
        let parsed = NxpArchive::parse(&data).unwrap();
        assert_eq!(parsed.entry_count(), 2);
        assert_eq!(parsed.manifest().name, "roundtrip-pkg");
        assert!(parsed.has_entry("src/main.nexa"));
        assert!(parsed.has_entry("README.md"));
    }

    #[test]
    fn add_valid_paths() {
        let manifest = make_manifest("test");
        let mut archive = NxpArchive::new(manifest);
        assert!(archive.add_entry("foo.nexa", b"data".to_vec()).is_ok());
        assert!(archive.add_entry("src/bar.nexa", b"data".to_vec()).is_ok());
        assert!(archive
            .add_entry("a/b/c/file.txt", b"data".to_vec())
            .is_ok());
    }

    #[test]
    fn reject_absolute_path() {
        let manifest = make_manifest("test");
        let mut archive = NxpArchive::new(manifest);
        let result = archive.add_entry("/etc/passwd", b"data".to_vec());
        assert!(result.is_err());
    }

    #[test]
    fn reject_empty_path() {
        let manifest = make_manifest("test");
        let mut archive = NxpArchive::new(manifest);
        let result = archive.add_entry("", b"data".to_vec());
        assert!(result.is_err());
    }

    #[test]
    fn reject_dotdot_path() {
        let manifest = make_manifest("test");
        let mut archive = NxpArchive::new(manifest);
        let result = archive.add_entry("../secret", b"data".to_vec());
        assert!(result.is_err());
    }

    #[test]
    fn reject_dot_path() {
        let manifest = make_manifest("test");
        let mut archive = NxpArchive::new(manifest);
        let result = archive.add_entry(".", b"data".to_vec());
        assert!(result.is_err());
    }

    #[test]
    fn reject_backslash_path() {
        let manifest = make_manifest("test");
        let mut archive = NxpArchive::new(manifest);
        let result = archive.add_entry("src\\main.nexa", b"data".to_vec());
        assert!(result.is_err());
    }

    #[test]
    fn case_collision_detection() {
        let manifest = make_manifest("test");
        let mut archive = NxpArchive::new(manifest);
        archive.add_entry("Foo.nexa", b"data1".to_vec()).unwrap();
        archive.add_entry("foo.nexa", b"data2".to_vec()).unwrap();
        let errors = archive.validate().unwrap_err();
        assert!(errors
            .iter()
            .any(|e| matches!(e, PackageError::CaseCollision { .. })));
    }

    #[test]
    fn duplicate_entry_detection() {
        let manifest = make_manifest("test");
        let mut archive = NxpArchive::new(manifest);
        archive.add_entry("foo.nexa", b"data1".to_vec()).unwrap();
        let result = archive.add_entry("foo.nexa", b"data2".to_vec());
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            PackageError::DuplicateEntry { .. }
        ));
    }

    #[test]
    fn entry_count_correctness() {
        let manifest = make_manifest("test");
        let mut archive = NxpArchive::new(manifest);
        assert_eq!(archive.entry_count(), 0);
        archive.add_entry("a.nexa", b"data".to_vec()).unwrap();
        assert_eq!(archive.entry_count(), 1);
        archive.add_entry("b.nexa", b"data".to_vec()).unwrap();
        assert_eq!(archive.entry_count(), 2);
    }

    #[test]
    fn has_entry_and_get_entry() {
        let manifest = make_manifest("test");
        let mut archive = NxpArchive::new(manifest);
        archive.add_entry("foo.nexa", b"hello".to_vec()).unwrap();
        assert!(archive.has_entry("foo.nexa"));
        assert!(!archive.has_entry("bar.nexa"));
        let entry = archive.get_entry("foo.nexa").unwrap();
        assert_eq!(entry.content, b"hello");
        assert!(archive.get_entry("bar.nexa").is_none());
    }

    #[test]
    fn manifest_presence() {
        let manifest = make_manifest("test");
        let archive = NxpArchive::new(manifest);
        assert_eq!(archive.manifest().name, "test");
    }

    #[test]
    fn missing_manifest_detection() {
        let manifest = make_manifest("test");
        let mut archive = NxpArchive::new(manifest);
        archive
            .add_entry("src/main.nexa", b"data".to_vec())
            .unwrap();
        let errors = archive.validate().unwrap_err();
        assert!(errors
            .iter()
            .any(|e| matches!(e, PackageError::MissingManifest)));
    }

    #[test]
    fn magic_byte_validation() {
        let mut data = vec![0u8; 100];
        data[0..4].copy_from_slice(b"NOT1");
        let result = NxpArchive::parse(&data);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            PackageError::InvalidMagic { .. }
        ));
    }

    #[test]
    fn empty_package_rejection() {
        let manifest = make_manifest("test");
        let archive = NxpArchive::new(manifest);
        let errors = archive.validate().unwrap_err();
        assert!(errors
            .iter()
            .any(|e| matches!(e, PackageError::EmptyPackage)));
    }

    #[test]
    fn large_content_entry() {
        let manifest = make_manifest("test");
        let mut archive = NxpArchive::new(manifest);
        let large_content = vec![42u8; 1024 * 1024];
        archive.add_entry("big.bin", large_content.clone()).unwrap();
        let data = archive.build();
        let parsed = NxpArchive::parse(&data).unwrap();
        assert_eq!(parsed.get_entry("big.bin").unwrap().content, large_content);
    }

    #[test]
    fn package_digest_consistency() {
        let manifest = make_manifest("test");
        let mut archive1 = NxpArchive::new(manifest.clone());
        archive1.add_entry("a.nexa", b"data".to_vec()).unwrap();
        let mut archive2 = NxpArchive::new(manifest);
        archive2.add_entry("a.nexa", b"data".to_vec()).unwrap();
        assert_eq!(archive1.compute_digest(), archive2.compute_digest());
    }

    #[test]
    fn validate_passes_for_valid_archive() {
        let manifest = make_manifest("test");
        let mut archive = NxpArchive::new(manifest);
        archive
            .add_entry("src/main.nexa", b"fn main() {}".to_vec())
            .unwrap();
        archive
            .add_entry("META/nexa-package.manifest", b"{}".to_vec())
            .unwrap();
        let digest = archive.compute_digest();
        archive.manifest.content_metadata.package_digest = digest;
        archive.manifest.content_metadata.total_size =
            archive.entries.iter().map(|e| e.content.len() as u64).sum();
        archive.manifest.content_metadata.entry_count = archive.entries.len() as u32;
        assert!(archive.validate().is_ok());
    }

    #[test]
    fn validate_catches_multiple_issues() {
        let manifest = make_manifest("test");
        let archive = NxpArchive::new(manifest);
        let result = archive.validate();
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.len() >= 2);
    }

    #[test]
    fn content_metadata_calculation() {
        let manifest = make_manifest("test");
        let mut archive = NxpArchive::new(manifest);
        archive.add_entry("a.nexa", b"hello".to_vec()).unwrap();
        archive.add_entry("b.nexa", b"world!".to_vec()).unwrap();
        let digest = archive.compute_digest();
        assert!(!digest.is_empty());
    }

    #[test]
    fn multiple_entries_various_paths() {
        let manifest = make_manifest("test");
        let mut archive = NxpArchive::new(manifest);
        archive.add_entry("a.nexa", b"1".to_vec()).unwrap();
        archive.add_entry("b/c/d.nexa", b"2".to_vec()).unwrap();
        archive
            .add_entry("deeply/nested/path/file.txt", b"3".to_vec())
            .unwrap();
        assert_eq!(archive.entry_count(), 3);
    }

    #[test]
    fn nxp1_magic_bytes_correctness() {
        assert_eq!(NXP1_MAGIC, b"NXP1");
        assert_eq!(NXP1_MAGIC.len(), 4);
    }

    #[test]
    fn parse_too_short_data() {
        let result = NxpArchive::parse(&[0u8; 5]);
        assert!(result.is_err());
    }

    #[test]
    fn build_and_parse_preserves_content() {
        let manifest = make_manifest("test");
        let mut archive = NxpArchive::new(manifest);
        archive
            .add_entry("file.txt", b"exact content here".to_vec())
            .unwrap();
        let data = archive.build();
        let parsed = NxpArchive::parse(&data).unwrap();
        assert_eq!(
            parsed.get_entry("file.txt").unwrap().content,
            b"exact content here"
        );
    }

    #[test]
    fn entry_digest_matches_computed() {
        let manifest = make_manifest("test");
        let mut archive = NxpArchive::new(manifest);
        archive
            .add_entry("file.nexa", b"test content".to_vec())
            .unwrap();
        let entry = archive.get_entry("file.nexa").unwrap();
        let expected = compute_entry_digest(b"test content");
        assert_eq!(entry.digest.as_deref(), Some(expected.as_str()));
    }

    #[test]
    fn reject_root_dotdot() {
        let manifest = make_manifest("test");
        let mut archive = NxpArchive::new(manifest);
        assert!(archive
            .add_entry("src/../../secret", b"data".to_vec())
            .is_err());
    }

    #[test]
    fn parse_empty_entry_count() {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"NXP1");
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&12u32.to_le_bytes());
        buf.extend_from_slice(b"{}");
        let manifest = PackageMetadata {
            name: String::new(),
            version: String::new(),
            format_version: 1,
            language_version: String::new(),
            public_interface_fingerprint: String::new(),
            entries: Vec::new(),
            dependencies: Vec::new(),
            content_metadata: ContentMetadata {
                total_size: 0,
                entry_count: 0,
                package_digest: String::new(),
            },
        };
        let json = serde_json::to_vec(&manifest).unwrap();
        buf = Vec::new();
        buf.extend_from_slice(b"NXP1");
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&(12u32).to_le_bytes());
        buf.extend_from_slice(&json);
        let parsed = NxpArchive::parse(&buf).unwrap();
        assert_eq!(parsed.entry_count(), 0);
    }
}
