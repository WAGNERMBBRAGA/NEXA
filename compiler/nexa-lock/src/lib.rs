use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LockFormatVersion(pub u32);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResolverIdentity {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LockedPackageId {
    pub name: String,
    pub version: String,
    pub source: LockedSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LockedSource {
    Registry { registry_id: String },
    Path { path: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ContentDigest(pub String);

impl ContentDigest {
    pub fn sha256_hex(bytes: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        let result = hasher.finalize();
        ContentDigest(hex::encode(result))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_valid(&self) -> bool {
        self.0.len() == 64 && self.0.chars().all(|c| c.is_ascii_hexdigit())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LockedDependencyKind {
    Normal,
    Dev,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LockedDependency {
    pub alias: String,
    pub name: String,
    pub version_req: String,
    pub source: LockedSource,
    pub kind: LockedDependencyKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LockedPackage {
    pub id: LockedPackageId,
    pub source: LockedSource,
    pub digest: String,
    pub dependencies: Vec<LockedDependency>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LockFile {
    pub format: u32,
    pub resolver: ResolverIdentity,
    pub root: LockedPackageId,
    pub packages: Vec<LockedPackage>,
}

#[derive(Debug, thiserror::Error)]
pub enum LockError {
    #[error("NEXA-LOCK-0001: Invalid lock format version {0}")]
    InvalidFormatVersion(u32),

    #[error("NEXA-LOCK-0002: Duplicate package '{name}' v{version}")]
    DuplicatePackage { name: String, version: String },

    #[error("NEXA-LOCK-0003: Missing required lockfile field '{field}'")]
    MissingField { field: String },

    #[error("NEXA-LOCK-0004: Lockfile is empty or corrupted")]
    EmptyOrCorrupted,

    #[error("NEXA-LOCK-0005: Lock serialization failed: {reason}")]
    SerializationFailed { reason: String },

    #[error("NEXA-LOCK-0006: Lock deserialization failed: {reason}")]
    DeserializationFailed { reason: String },

    #[error("NEXA-LOCK-0007: Package '{name}' has invalid content digest")]
    InvalidDigest { name: String },

    #[error("NEXA-LOCK-0008: Dependency cycle detected involving '{package}'")]
    DependencyCycle { package: String },

    #[error("NEXA-LOCK-0009: Lockfile version mismatch: expected {expected}, found {found}")]
    VersionMismatch { expected: u32, found: u32 },

    #[error("NEXA-LOCK-0010: Lock ordering is not deterministic")]
    NonDeterministicOrder,
}

const LOCK_FORMAT_VERSION: u32 = 1;

impl LockFile {
    pub fn new(root: LockedPackageId, resolver: ResolverIdentity) -> Self {
        LockFile {
            format: LOCK_FORMAT_VERSION,
            resolver,
            root,
            packages: Vec::new(),
        }
    }

    pub fn add_package(&mut self, package: LockedPackage) -> Result<(), LockError> {
        let exists = self.packages.iter().any(|p| {
            p.id.name == package.id.name
                && p.id.version == package.id.version
                && p.id.source == package.id.source
        });

        if exists {
            return Err(LockError::DuplicatePackage {
                name: package.id.name.clone(),
                version: package.id.version.clone(),
            });
        }

        self.packages.push(package);
        Ok(())
    }

    pub fn validate(&self) -> Result<(), Vec<LockError>> {
        let mut errors = Vec::new();

        if self.format != LOCK_FORMAT_VERSION {
            errors.push(LockError::InvalidFormatVersion(self.format));
        }

        if self.packages.is_empty() {
            errors.push(LockError::EmptyOrCorrupted);
        }

        let mut seen = HashSet::new();
        for pkg in &self.packages {
            let key = (
                pkg.id.name.clone(),
                pkg.id.version.clone(),
                pkg.id.source.clone(),
            );
            if !seen.insert(key.clone()) {
                errors.push(LockError::DuplicatePackage {
                    name: pkg.id.name.clone(),
                    version: pkg.id.version.clone(),
                });
            }

            let digest = ContentDigest(pkg.digest.clone());
            if !digest.is_valid() {
                errors.push(LockError::InvalidDigest {
                    name: pkg.id.name.clone(),
                });
            }
        }

        if !self.has_cycle().is_none() {
            if let Some(cycle_pkg) = self.has_cycle() {
                errors.push(LockError::DependencyCycle { package: cycle_pkg });
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn has_cycle(&self) -> Option<String> {
        for pkg in &self.packages {
            let mut visited = HashSet::new();
            if self.detect_cycle(pkg, &mut visited) {
                return Some(pkg.id.name.clone());
            }
        }
        None
    }

    fn detect_cycle(&self, pkg: &LockedPackage, visited: &mut HashSet<String>) -> bool {
        for dep in &pkg.dependencies {
            if visited.contains(&dep.name) {
                return true;
            }

            if let Some(dep_pkg) = self
                .packages
                .iter()
                .find(|p| p.id.name == dep.name && p.id.version == dep.version_req)
            {
                visited.insert(dep.name.clone());
                if self.detect_cycle(dep_pkg, visited) {
                    return true;
                }
                visited.remove(&dep.name);
            }
        }
        false
    }

    pub fn sort_canonical(&mut self) {
        self.packages.sort_by(|a, b| {
            a.id.name
                .cmp(&b.id.name)
                .then_with(|| {
                    let a_ver: Vec<u32> =
                        a.id.version
                            .split('.')
                            .filter_map(|s| s.parse().ok())
                            .collect();
                    let b_ver: Vec<u32> =
                        b.id.version
                            .split('.')
                            .filter_map(|s| s.parse().ok())
                            .collect();
                    a_ver.cmp(&b_ver)
                })
                .then_with(|| match (&a.id.source, &b.id.source) {
                    (
                        LockedSource::Registry { registry_id: a_reg },
                        LockedSource::Registry { registry_id: b_reg },
                    ) => a_reg.cmp(b_reg),
                    (LockedSource::Path { path: a_path }, LockedSource::Path { path: b_path }) => {
                        a_path.cmp(b_path)
                    }
                    (LockedSource::Registry { .. }, LockedSource::Path { .. }) => {
                        std::cmp::Ordering::Less
                    }
                    (LockedSource::Path { .. }, LockedSource::Registry { .. }) => {
                        std::cmp::Ordering::Greater
                    }
                })
        });

        for pkg in &mut self.packages {
            pkg.dependencies
                .sort_by(|a, b| a.alias.cmp(&b.alias).then_with(|| a.name.cmp(&b.name)));
        }
    }

    pub fn to_deterministic_json(&self) -> Result<String, LockError> {
        let mut lock = self.clone();
        lock.sort_canonical();

        serde_json::to_string_pretty(&lock).map_err(|e| LockError::SerializationFailed {
            reason: e.to_string(),
        })
    }

    pub fn from_json(json: &str) -> Result<Self, LockError> {
        let mut lock: LockFile =
            serde_json::from_str(json).map_err(|e| LockError::DeserializationFailed {
                reason: e.to_string(),
            })?;

        if lock.format != LOCK_FORMAT_VERSION {
            return Err(LockError::VersionMismatch {
                expected: LOCK_FORMAT_VERSION,
                found: lock.format,
            });
        }

        lock.sort_canonical();
        Ok(lock)
    }

    pub fn packages_sorted_by(&self) -> Vec<&LockedPackage> {
        let mut pkgs: Vec<&LockedPackage> = self.packages.iter().collect();
        pkgs.sort_by(|a, b| {
            a.id.name
                .cmp(&b.id.name)
                .then_with(|| {
                    let a_ver: Vec<u32> =
                        a.id.version
                            .split('.')
                            .filter_map(|s| s.parse().ok())
                            .collect();
                    let b_ver: Vec<u32> =
                        b.id.version
                            .split('.')
                            .filter_map(|s| s.parse().ok())
                            .collect();
                    a_ver.cmp(&b_ver)
                })
                .then_with(|| match (&a.id.source, &b.id.source) {
                    (
                        LockedSource::Registry { registry_id: a_reg },
                        LockedSource::Registry { registry_id: b_reg },
                    ) => a_reg.cmp(b_reg),
                    (LockedSource::Path { path: a_path }, LockedSource::Path { path: b_path }) => {
                        a_path.cmp(b_path)
                    }
                    (LockedSource::Registry { .. }, LockedSource::Path { .. }) => {
                        std::cmp::Ordering::Less
                    }
                    (LockedSource::Path { .. }, LockedSource::Registry { .. }) => {
                        std::cmp::Ordering::Greater
                    }
                })
        });
        pkgs
    }

    pub fn has_package(&self, name: &str, version: &str) -> bool {
        self.packages
            .iter()
            .any(|p| p.id.name == name && p.id.version == version)
    }

    pub fn get_package(&self, name: &str, version: &str) -> Option<&LockedPackage> {
        self.packages
            .iter()
            .find(|p| p.id.name == name && p.id.version == version)
    }

    pub fn root_id(&self) -> &LockedPackageId {
        &self.root
    }
}

impl Clone for LockFile {
    fn clone(&self) -> Self {
        LockFile {
            format: self.format,
            resolver: self.resolver.clone(),
            root: self.root.clone(),
            packages: self.packages.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_root() -> LockedPackageId {
        LockedPackageId {
            name: "my-app".to_string(),
            version: "1.0.0".to_string(),
            source: LockedSource::Path {
                path: ".".to_string(),
            },
        }
    }

    fn make_resolver() -> ResolverIdentity {
        ResolverIdentity {
            name: "nexa-resolver".to_string(),
            version: "0.1.0".to_string(),
        }
    }

    fn make_registry_source() -> LockedSource {
        LockedSource::Registry {
            registry_id: "crates-io".to_string(),
        }
    }

    fn make_valid_digest() -> String {
        "a".repeat(64)
    }

    fn make_package(name: &str, version: &str) -> LockedPackage {
        LockedPackage {
            id: LockedPackageId {
                name: name.to_string(),
                version: version.to_string(),
                source: make_registry_source(),
            },
            source: make_registry_source(),
            digest: make_valid_digest(),
            dependencies: Vec::new(),
        }
    }

    fn make_package_with_deps(
        name: &str,
        version: &str,
        deps: Vec<LockedDependency>,
    ) -> LockedPackage {
        LockedPackage {
            id: LockedPackageId {
                name: name.to_string(),
                version: version.to_string(),
                source: make_registry_source(),
            },
            source: make_registry_source(),
            digest: make_valid_digest(),
            dependencies: deps,
        }
    }

    fn make_dep(alias: &str, name: &str, version: &str) -> LockedDependency {
        LockedDependency {
            alias: alias.to_string(),
            name: name.to_string(),
            version_req: version.to_string(),
            source: make_registry_source(),
            kind: LockedDependencyKind::Normal,
        }
    }

    #[test]
    fn test_lockfile_creation_with_root() {
        let lock = LockFile::new(make_root(), make_resolver());
        assert_eq!(lock.format, 1);
        assert_eq!(lock.root.name, "my-app");
        assert_eq!(lock.packages.len(), 0);
    }

    #[test]
    fn test_add_package_valid() {
        let mut lock = LockFile::new(make_root(), make_resolver());
        let pkg = make_package("serde", "1.0.0");
        assert!(lock.add_package(pkg).is_ok());
        assert_eq!(lock.packages.len(), 1);
    }

    #[test]
    fn test_add_package_duplicate() {
        let mut lock = LockFile::new(make_root(), make_resolver());
        let pkg1 = make_package("serde", "1.0.0");
        let pkg2 = make_package("serde", "1.0.0");
        assert!(lock.add_package(pkg1).is_ok());
        let result = lock.add_package(pkg2);
        assert!(matches!(result, Err(LockError::DuplicatePackage { .. })));
    }

    #[test]
    fn test_content_digest_sha256_hex() {
        let data = b"hello world";
        let digest = ContentDigest::sha256_hex(data);
        assert_eq!(digest.as_str().len(), 64);
        assert!(digest.is_valid());
    }

    #[test]
    fn test_content_digest_valid() {
        let digest = ContentDigest("a".repeat(64));
        assert!(digest.is_valid());
    }

    #[test]
    fn test_content_digest_invalid_length() {
        let digest = ContentDigest("abc".to_string());
        assert!(!digest.is_valid());
    }

    #[test]
    fn test_content_digest_invalid_chars() {
        let digest = ContentDigest("g".repeat(64));
        assert!(!digest.is_valid());
    }

    #[test]
    fn test_validate_empty_lockfile() {
        let lock = LockFile::new(make_root(), make_resolver());
        let result = lock.validate();
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| matches!(e, LockError::EmptyOrCorrupted)));
    }

    #[test]
    fn test_validate_valid_lockfile() {
        let mut lock = LockFile::new(make_root(), make_resolver());
        lock.add_package(make_package("serde", "1.0.0")).unwrap();
        assert!(lock.validate().is_ok());
    }

    #[test]
    fn test_validate_with_invalid_digest() {
        let mut lock = LockFile::new(make_root(), make_resolver());
        let pkg = LockedPackage {
            id: LockedPackageId {
                name: "bad".to_string(),
                version: "1.0.0".to_string(),
                source: make_registry_source(),
            },
            source: make_registry_source(),
            digest: "invalid".to_string(),
            dependencies: Vec::new(),
        };
        lock.add_package(pkg).unwrap();
        let result = lock.validate();
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| matches!(e, LockError::InvalidDigest { .. })));
    }

    #[test]
    fn test_sort_canonical_consistency() {
        let mut lock1 = LockFile::new(make_root(), make_resolver());
        lock1.add_package(make_package("zlib", "2.0.0")).unwrap();
        lock1.add_package(make_package("alpha", "1.0.0")).unwrap();
        lock1.add_package(make_package("beta", "1.5.0")).unwrap();

        let mut lock2 = lock1.clone();
        lock2.sort_canonical();

        lock1.sort_canonical();

        assert_eq!(lock1.packages[0].id.name, "alpha");
        assert_eq!(lock1.packages[1].id.name, "beta");
        assert_eq!(lock1.packages[2].id.name, "zlib");
        assert_eq!(lock1.packages, lock2.packages);
    }

    #[test]
    fn test_to_deterministic_json_valid() {
        let mut lock = LockFile::new(make_root(), make_resolver());
        lock.add_package(make_package("serde", "1.0.0")).unwrap();
        let json = lock.to_deterministic_json();
        assert!(json.is_ok());
        let json_str = json.unwrap();
        assert!(json_str.contains("serde"));
    }

    #[test]
    fn test_to_deterministic_json_same_graph_same_bytes() {
        let mut lock1 = LockFile::new(make_root(), make_resolver());
        lock1.add_package(make_package("zlib", "2.0.0")).unwrap();
        lock1.add_package(make_package("alpha", "1.0.0")).unwrap();

        let mut lock2 = LockFile::new(make_root(), make_resolver());
        lock2.add_package(make_package("alpha", "1.0.0")).unwrap();
        lock2.add_package(make_package("zlib", "2.0.0")).unwrap();

        let json1 = lock1.to_deterministic_json().unwrap();
        let json2 = lock2.to_deterministic_json().unwrap();
        assert_eq!(json1, json2);
    }

    #[test]
    fn test_from_json_roundtrip() {
        let mut lock = LockFile::new(make_root(), make_resolver());
        lock.add_package(make_package("serde", "1.0.0")).unwrap();
        let json = lock.to_deterministic_json().unwrap();

        let restored = LockFile::from_json(&json);
        assert!(restored.is_ok());
        let restored = restored.unwrap();
        assert_eq!(restored.format, 1);
        assert_eq!(restored.packages.len(), 1);
        assert_eq!(restored.packages[0].id.name, "serde");
    }

    #[test]
    fn test_from_json_invalid_json() {
        let result = LockFile::from_json("not json");
        assert!(matches!(
            result,
            Err(LockError::DeserializationFailed { .. })
        ));
    }

    #[test]
    fn test_has_package() {
        let mut lock = LockFile::new(make_root(), make_resolver());
        lock.add_package(make_package("serde", "1.0.0")).unwrap();
        assert!(lock.has_package("serde", "1.0.0"));
        assert!(!lock.has_package("serde", "2.0.0"));
        assert!(!lock.has_package("other", "1.0.0"));
    }

    #[test]
    fn test_get_package() {
        let mut lock = LockFile::new(make_root(), make_resolver());
        lock.add_package(make_package("serde", "1.0.0")).unwrap();
        let pkg = lock.get_package("serde", "1.0.0");
        assert!(pkg.is_some());
        assert_eq!(pkg.unwrap().id.name, "serde");

        let missing = lock.get_package("serde", "2.0.0");
        assert!(missing.is_none());
    }

    #[test]
    fn test_root_id() {
        let lock = LockFile::new(make_root(), make_resolver());
        assert_eq!(lock.root_id().name, "my-app");
        assert_eq!(lock.root_id().version, "1.0.0");
    }

    #[test]
    fn test_locked_package_id_comparison() {
        let id1 = LockedPackageId {
            name: "serde".to_string(),
            version: "1.0.0".to_string(),
            source: make_registry_source(),
        };
        let id2 = LockedPackageId {
            name: "serde".to_string(),
            version: "1.0.0".to_string(),
            source: make_registry_source(),
        };
        let id3 = LockedPackageId {
            name: "serde".to_string(),
            version: "2.0.0".to_string(),
            source: make_registry_source(),
        };
        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_locked_source_equality() {
        let reg1 = LockedSource::Registry {
            registry_id: "crates-io".to_string(),
        };
        let reg2 = LockedSource::Registry {
            registry_id: "crates-io".to_string(),
        };
        let path1 = LockedSource::Path {
            path: "./local".to_string(),
        };
        assert_eq!(reg1, reg2);
        assert_ne!(reg1, path1);
    }

    #[test]
    fn test_multiple_packages_with_dependencies() {
        let mut lock = LockFile::new(make_root(), make_resolver());
        let dep = make_dep("serde", "serde", "1.0.0");
        let pkg = make_package_with_deps("my-lib", "1.0.0", vec![dep]);
        lock.add_package(pkg).unwrap();
        lock.add_package(make_package("serde", "1.0.0")).unwrap();
        assert_eq!(lock.packages.len(), 2);
    }

    #[test]
    fn test_edge_sorting_within_packages() {
        let mut lock = LockFile::new(make_root(), make_resolver());
        let deps = vec![
            make_dep("zlib", "zlib", "2.0.0"),
            make_dep("alpha", "alpha", "1.0.0"),
            make_dep("beta", "beta", "1.5.0"),
        ];
        let pkg = make_package_with_deps("my-lib", "1.0.0", deps);
        lock.add_package(pkg).unwrap();
        lock.sort_canonical();

        let pkg = lock.get_package("my-lib", "1.0.0").unwrap();
        assert_eq!(pkg.dependencies[0].alias, "alpha");
        assert_eq!(pkg.dependencies[1].alias, "beta");
        assert_eq!(pkg.dependencies[2].alias, "zlib");
    }

    #[test]
    fn test_version_mismatch_error() {
        let json = r#"{
            "format": 999,
            "resolver": {"name": "test", "version": "1.0.0"},
            "root": {"name": "app", "version": "1.0.0", "source": {"Path": {"path": "."}}},
            "packages": []
        }"#;
        let result = LockFile::from_json(json);
        assert!(matches!(result, Err(LockError::VersionMismatch { .. })));
    }

    #[test]
    fn test_large_lockfile_serialization() {
        let mut lock = LockFile::new(make_root(), make_resolver());
        for i in 0..1000 {
            let name = format!("pkg-{}", i);
            let version = format!("{}.0.0", i % 10);
            lock.add_package(make_package(&name, &version)).unwrap();
        }
        let json = lock.to_deterministic_json();
        assert!(json.is_ok());
        let json_str = json.unwrap();
        assert!(json_str.len() > 10000);

        let restored = LockFile::from_json(&json_str);
        assert!(restored.is_ok());
        assert_eq!(restored.unwrap().packages.len(), 1000);
    }

    #[test]
    fn test_packages_sorted_by() {
        let mut lock = LockFile::new(make_root(), make_resolver());
        lock.add_package(make_package("zlib", "2.0.0")).unwrap();
        lock.add_package(make_package("alpha", "1.0.0")).unwrap();

        let sorted = lock.packages_sorted_by();
        assert_eq!(sorted[0].id.name, "alpha");
        assert_eq!(sorted[1].id.name, "zlib");
    }

    #[test]
    fn test_dependency_cycle_detection() {
        let mut lock = LockFile::new(make_root(), make_resolver());
        let dep_a = make_dep("b", "b", "1.0.0");
        let pkg_a = make_package_with_deps("a", "1.0.0", vec![dep_a]);
        let dep_b = make_dep("a", "a", "1.0.0");
        let pkg_b = make_package_with_deps("b", "1.0.0", vec![dep_b]);

        lock.add_package(pkg_a).unwrap();
        lock.add_package(pkg_b).unwrap();

        let result = lock.validate();
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors
            .iter()
            .any(|e| matches!(e, LockError::DependencyCycle { .. })));
    }

    #[test]
    fn test_path_source_sorting() {
        let mut lock = LockFile::new(make_root(), make_resolver());
        let pkg1 = LockedPackage {
            id: LockedPackageId {
                name: "alpha".to_string(),
                version: "1.0.0".to_string(),
                source: LockedSource::Path {
                    path: "./b".to_string(),
                },
            },
            source: LockedSource::Path {
                path: "./b".to_string(),
            },
            digest: make_valid_digest(),
            dependencies: Vec::new(),
        };
        let pkg2 = LockedPackage {
            id: LockedPackageId {
                name: "alpha".to_string(),
                version: "1.0.0".to_string(),
                source: LockedSource::Path {
                    path: "./a".to_string(),
                },
            },
            source: LockedSource::Path {
                path: "./a".to_string(),
            },
            digest: make_valid_digest(),
            dependencies: Vec::new(),
        };
        lock.add_package(pkg1).unwrap();
        lock.add_package(pkg2).unwrap();
        lock.sort_canonical();

        assert_eq!(
            lock.packages[0].id.source,
            LockedSource::Path {
                path: "./a".to_string()
            }
        );
    }
}
