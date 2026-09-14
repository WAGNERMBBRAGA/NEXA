use std::collections::HashMap;

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("NEXA-REGISTRY-0001: Package '{package}' not found in registry '{registry}'")]
    PackageNotFound { package: String, registry: String },
    #[error("NEXA-REGISTRY-0002: Version '{version}' not found for package '{package}'")]
    VersionNotFound { package: String, version: String },
    #[error("NEXA-REGISTRY-0003: Registry '{registry}' is not reachable")]
    RegistryUnavailable { registry: String },
    #[error("NEXA-REGISTRY-0004: Authentication failed for registry '{registry}'")]
    AuthenticationFailed { registry: String },
    #[error("NEXA-REGISTRY-0005: Network error: {message}")]
    NetworkError { message: String },
    #[error("NEXA-REGISTRY-0006: Invalid registry URL '{url}'")]
    InvalidUrl { url: String },
    #[error("NEXA-REGISTRY-0007: Download size {actual} exceeds limit {limit}")]
    DownloadTooLarge { actual: u64, limit: u64 },
    #[error("NEXA-REGISTRY-0008: Invalid registry id '{id}'")]
    InvalidRegistryId { id: String },
    #[error("NEXA-REGISTRY-0009: Package '{package}' is yanked at version '{version}'")]
    PackageYanked { package: String, version: String },
    #[error("NEXA-REGISTRY-0010: Integrity check failed for package '{package}' v{version}")]
    IntegrityMismatch { package: String, version: String },
}

#[derive(Debug, Clone)]
pub struct RegistryId(String);

impl RegistryId {
    pub fn new(id: &str) -> Result<Self, RegistryError> {
        if id.is_empty() || id.contains(char::is_whitespace) {
            return Err(RegistryError::InvalidRegistryId { id: id.to_string() });
        }
        Ok(Self(id.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_default(&self) -> bool {
        self.0 == "default"
    }
}

pub fn default_registry() -> RegistryId {
    RegistryId("default".to_string())
}

#[derive(Debug, Clone)]
pub struct RegistryPackageVersion {
    pub name: String,
    pub version: String,
    pub digest: String,
    pub size: u64,
    pub dependencies: Vec<RegistryDependency>,
    pub format_version: u32,
    pub published: bool,
    pub yanked: bool,
}

#[derive(Debug, Clone)]
pub struct RegistryDependency {
    pub alias: String,
    pub package: String,
    pub version_req: String,
    pub source: DependencySource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencySource {
    Registry,
    Path,
}

#[derive(Debug, Clone)]
pub struct RegistryPackageMetadata {
    pub name: String,
    pub versions: Vec<String>,
    pub latest: Option<String>,
    pub description: Option<String>,
}

pub trait RegistryClient: Send + Sync {
    fn list_versions(&self, package: &str) -> Result<Vec<String>, RegistryError>;
    fn get_metadata(&self, package: &str) -> Result<RegistryPackageMetadata, RegistryError>;
    fn get_package_version(
        &self,
        package: &str,
        version: &str,
    ) -> Result<RegistryPackageVersion, RegistryError>;
    fn download_artifact(&self, package: &str, version: &str) -> Result<Vec<u8>, RegistryError>;
}

pub struct InMemoryRegistry {
    packages: HashMap<String, Vec<RegistryPackageVersion>>,
}

impl InMemoryRegistry {
    pub fn new() -> Self {
        Self {
            packages: HashMap::new(),
        }
    }

    pub fn add_package(&mut self, entry: RegistryPackageVersion) {
        self.packages
            .entry(entry.name.clone())
            .or_default()
            .push(entry);
    }

    pub fn add_versions(&mut self, name: &str, versions: Vec<RegistryPackageVersion>) {
        self.packages
            .entry(name.to_string())
            .or_default()
            .extend(versions);
    }
}

impl Default for InMemoryRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl RegistryClient for InMemoryRegistry {
    fn list_versions(&self, package: &str) -> Result<Vec<String>, RegistryError> {
        self.packages
            .get(package)
            .map(|versions| {
                versions
                    .iter()
                    .filter(|v| !v.yanked)
                    .map(|v| v.version.clone())
                    .collect()
            })
            .ok_or_else(|| RegistryError::PackageNotFound {
                package: package.to_string(),
                registry: "in-memory".to_string(),
            })
    }

    fn get_metadata(&self, package: &str) -> Result<RegistryPackageMetadata, RegistryError> {
        let versions =
            self.packages
                .get(package)
                .ok_or_else(|| RegistryError::PackageNotFound {
                    package: package.to_string(),
                    registry: "in-memory".to_string(),
                })?;

        let version_strings: Vec<String> = versions.iter().map(|v| v.version.clone()).collect();
        let latest = versions
            .iter()
            .filter(|v| v.published && !v.yanked)
            .max_by_key(|v| v.version.clone())
            .map(|v| v.version.clone());

        Ok(RegistryPackageMetadata {
            name: package.to_string(),
            versions: version_strings,
            latest,
            description: None,
        })
    }

    fn get_package_version(
        &self,
        package: &str,
        version: &str,
    ) -> Result<RegistryPackageVersion, RegistryError> {
        let entries = self
            .packages
            .get(package)
            .ok_or_else(|| RegistryError::PackageNotFound {
                package: package.to_string(),
                registry: "in-memory".to_string(),
            })?;

        let entry = entries
            .iter()
            .find(|v| v.version == version)
            .ok_or_else(|| RegistryError::VersionNotFound {
                package: package.to_string(),
                version: version.to_string(),
            })?;

        if entry.yanked {
            return Err(RegistryError::PackageYanked {
                package: package.to_string(),
                version: version.to_string(),
            });
        }

        Ok(entry.clone())
    }

    fn download_artifact(&self, package: &str, version: &str) -> Result<Vec<u8>, RegistryError> {
        let entry = self.get_package_version(package, version)?;
        Ok(vec![0u8; entry.size as usize])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_version(name: &str, version: &str) -> RegistryPackageVersion {
        RegistryPackageVersion {
            name: name.to_string(),
            version: version.to_string(),
            digest: format!("sha256:{}", version),
            size: 1024,
            dependencies: vec![],
            format_version: 1,
            published: true,
            yanked: false,
        }
    }

    fn yanked_version(name: &str, version: &str) -> RegistryPackageVersion {
        let mut v = sample_version(name, version);
        v.yanked = true;
        v
    }

    #[test]
    fn registry_id_valid() {
        let id = RegistryId::new("my-registry").unwrap();
        assert_eq!(id.as_str(), "my-registry");
        assert!(!id.is_default());
    }

    #[test]
    fn registry_id_empty_fails() {
        assert!(RegistryId::new("").is_err());
    }

    #[test]
    fn registry_id_whitespace_fails() {
        assert!(RegistryId::new("my registry").is_err());
        assert!(RegistryId::new(" registry").is_err());
        assert!(RegistryId::new("registry ").is_err());
    }

    #[test]
    fn registry_id_default() {
        let id = RegistryId::new("default").unwrap();
        assert!(id.is_default());
    }

    #[test]
    fn default_registry_returns_default() {
        let id = default_registry();
        assert_eq!(id.as_str(), "default");
        assert!(id.is_default());
    }

    #[test]
    fn registry_id_with_special_chars() {
        let id = RegistryId::new("https://registry.example.com/").unwrap();
        assert_eq!(id.as_str(), "https://registry.example.com/");
    }

    #[test]
    fn in_memory_list_versions_found() {
        let mut reg = InMemoryRegistry::new();
        reg.add_package(sample_version("foo", "1.0.0"));
        reg.add_package(sample_version("foo", "1.1.0"));

        let versions = reg.list_versions("foo").unwrap();
        assert_eq!(versions, vec!["1.0.0", "1.1.0"]);
    }

    #[test]
    fn in_memory_list_versions_not_found() {
        let reg = InMemoryRegistry::new();
        let err = reg.list_versions("missing").unwrap_err();
        assert!(matches!(err, RegistryError::PackageNotFound { .. }));
    }

    #[test]
    fn in_memory_get_metadata_found() {
        let mut reg = InMemoryRegistry::new();
        reg.add_package(sample_version("bar", "0.1.0"));
        reg.add_package(sample_version("bar", "0.2.0"));

        let meta = reg.get_metadata("bar").unwrap();
        assert_eq!(meta.name, "bar");
        assert_eq!(meta.versions, vec!["0.1.0", "0.2.0"]);
        assert!(meta.latest.is_some());
    }

    #[test]
    fn in_memory_get_metadata_not_found() {
        let reg = InMemoryRegistry::new();
        let err = reg.get_metadata("nope").unwrap_err();
        assert!(matches!(err, RegistryError::PackageNotFound { .. }));
    }

    #[test]
    fn in_memory_get_metadata_yanked_version_excluded_from_latest() {
        let mut reg = InMemoryRegistry::new();
        reg.add_package(sample_version("baz", "1.0.0"));
        reg.add_package(yanked_version("baz", "2.0.0"));

        let meta = reg.get_metadata("baz").unwrap();
        assert_eq!(meta.latest.as_deref(), Some("1.0.0"));
    }

    #[test]
    fn in_memory_get_package_version_found() {
        let mut reg = InMemoryRegistry::new();
        reg.add_package(sample_version("pkg", "3.0.0"));

        let v = reg.get_package_version("pkg", "3.0.0").unwrap();
        assert_eq!(v.name, "pkg");
        assert_eq!(v.version, "3.0.0");
    }

    #[test]
    fn in_memory_get_package_version_not_found() {
        let mut reg = InMemoryRegistry::new();
        reg.add_package(sample_version("pkg", "1.0.0"));

        let err = reg.get_package_version("pkg", "9.0.0").unwrap_err();
        assert!(matches!(err, RegistryError::VersionNotFound { .. }));
    }

    #[test]
    fn in_memory_get_package_version_package_not_found() {
        let reg = InMemoryRegistry::new();
        let err = reg.get_package_version("ghost", "1.0.0").unwrap_err();
        assert!(matches!(err, RegistryError::PackageNotFound { .. }));
    }

    #[test]
    fn in_memory_get_package_version_yanked() {
        let mut reg = InMemoryRegistry::new();
        reg.add_package(yanked_version("yanked", "1.0.0"));

        let err = reg.get_package_version("yanked", "1.0.0").unwrap_err();
        assert!(matches!(err, RegistryError::PackageYanked { .. }));
    }

    #[test]
    fn in_memory_download_artifact_found() {
        let mut reg = InMemoryRegistry::new();
        let mut v = sample_version("dl", "1.0.0");
        v.size = 5;
        reg.add_package(v);

        let data = reg.download_artifact("dl", "1.0.0").unwrap();
        assert_eq!(data.len(), 5);
    }

    #[test]
    fn in_memory_download_artifact_not_found() {
        let reg = InMemoryRegistry::new();
        let err = reg.download_artifact("dl", "1.0.0").unwrap_err();
        assert!(matches!(err, RegistryError::PackageNotFound { .. }));
    }

    #[test]
    fn in_memory_download_artifact_yanked() {
        let mut reg = InMemoryRegistry::new();
        reg.add_package(yanked_version("dl2", "1.0.0"));

        let err = reg.download_artifact("dl2", "1.0.0").unwrap_err();
        assert!(matches!(err, RegistryError::PackageYanked { .. }));
    }

    #[test]
    fn package_yanked_error_message() {
        let err = RegistryError::PackageYanked {
            package: "foo".to_string(),
            version: "1.0.0".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("NEXA-REGISTRY-0009"));
        assert!(msg.contains("foo"));
        assert!(msg.contains("1.0.0"));
    }

    #[test]
    fn multiple_versions_ordering_preserved() {
        let mut reg = InMemoryRegistry::new();
        reg.add_package(sample_version("ord", "3.0.0"));
        reg.add_package(sample_version("ord", "1.0.0"));
        reg.add_package(sample_version("ord", "2.0.0"));

        let versions = reg.list_versions("ord").unwrap();
        assert_eq!(versions, vec!["3.0.0", "1.0.0", "2.0.0"]);
    }

    #[test]
    fn registry_metadata_empty_versions() {
        let mut reg = InMemoryRegistry::new();
        reg.add_versions("empty-pkg", vec![]);

        let meta = reg.get_metadata("empty-pkg").unwrap();
        assert!(meta.versions.is_empty());
        assert!(meta.latest.is_none());
    }

    #[test]
    fn empty_registry_lookup() {
        let reg = InMemoryRegistry::new();
        assert!(reg.list_versions("anything").is_err());
        assert!(reg.get_metadata("anything").is_err());
        assert!(reg.get_package_version("anything", "1.0.0").is_err());
        assert!(reg.download_artifact("anything", "1.0.0").is_err());
    }

    #[test]
    fn case_sensitive_package_lookup() {
        let mut reg = InMemoryRegistry::new();
        reg.add_package(sample_version("MyPackage", "1.0.0"));

        assert!(reg.list_versions("MyPackage").is_ok());
        assert!(reg.list_versions("mypackage").is_err());
        assert!(reg.list_versions("MYPACKAGE").is_err());
    }

    #[test]
    fn add_versions_populates_multiple() {
        let mut reg = InMemoryRegistry::new();
        reg.add_versions(
            "multi",
            vec![
                sample_version("multi", "1.0.0"),
                sample_version("multi", "2.0.0"),
                sample_version("multi", "3.0.0"),
            ],
        );

        let versions = reg.list_versions("multi").unwrap();
        assert_eq!(versions.len(), 3);
    }

    #[test]
    fn all_published_yanked_versions_show_none_latest() {
        let mut reg = InMemoryRegistry::new();
        reg.add_package(yanked_version("all-yanked", "1.0.0"));
        reg.add_package(yanked_version("all-yanked", "2.0.0"));

        let meta = reg.get_metadata("all-yanked").unwrap();
        assert!(meta.latest.is_none());
    }

    #[test]
    fn registry_error_messages_cover_all_codes() {
        let errors: Vec<String> = vec![
            RegistryError::PackageNotFound {
                package: "p".into(),
                registry: "r".into(),
            },
            RegistryError::VersionNotFound {
                package: "p".into(),
                version: "v".into(),
            },
            RegistryError::RegistryUnavailable {
                registry: "r".into(),
            },
            RegistryError::AuthenticationFailed {
                registry: "r".into(),
            },
            RegistryError::NetworkError {
                message: "m".into(),
            },
            RegistryError::InvalidUrl { url: "u".into() },
            RegistryError::DownloadTooLarge {
                actual: 100,
                limit: 50,
            },
            RegistryError::InvalidRegistryId { id: "i".into() },
            RegistryError::PackageYanked {
                package: "p".into(),
                version: "v".into(),
            },
            RegistryError::IntegrityMismatch {
                package: "p".into(),
                version: "v".into(),
            },
        ]
        .into_iter()
        .map(|e| e.to_string())
        .collect();

        for i in 1..=10 {
            let code = format!("NEXA-REGISTRY-{:04}", i);
            assert!(
                errors.iter().any(|e| e.contains(&code)),
                "Missing error code {}",
                code
            );
        }
    }

    #[test]
    fn dependency_source_equality() {
        assert_eq!(DependencySource::Registry, DependencySource::Registry);
        assert_eq!(DependencySource::Path, DependencySource::Path);
        assert_ne!(DependencySource::Registry, DependencySource::Path);
    }

    #[test]
    fn registry_id_with_non_ascii() {
        let id = RegistryId::new("café-registry").unwrap();
        assert_eq!(id.as_str(), "café-registry");
        assert!(!id.is_default());
    }
}
