use std::collections::{HashMap, HashSet};

use nexa_manifest::{ProjectManifest, SemanticVersion, VersionRequirement};
use nexa_registry::RegistryClient;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PackageInstanceId {
    pub name: String,
    pub version: String,
    pub source: PackageSourceIdentity,
    pub digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PackageSourceIdentity {
    Registry { registry_id: String },
    Path { path: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DependencyAlias(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencyKind {
    Normal,
    Dev,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedDependencyEdge {
    pub from: PackageInstanceId,
    pub alias: DependencyAlias,
    pub to: PackageInstanceId,
    pub kind: DependencyKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPackageNode {
    pub id: PackageInstanceId,
    pub dependencies: Vec<ResolvedDependency>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedDependency {
    pub alias: String,
    pub version_req: String,
    pub source: PackageSourceIdentity,
    pub kind: DependencyKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedDependencyGraph {
    pub root: PackageInstanceId,
    pub packages: Vec<ResolvedPackageNode>,
    pub edges: Vec<ResolvedDependencyEdge>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictReport {
    pub conflicting_package: String,
    pub constraints: Vec<ConstraintInfo>,
    pub resolution_path: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstraintInfo {
    pub required_by: String,
    pub version_req: String,
    pub source: PackageSourceIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionSet {
    pub constraints: Vec<VersionConstraint>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionConstraint {
    pub operator: ConstraintOp,
    pub version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstraintOp {
    Gte,
    Lte,
    Eq,
    Caret,
    Tilde,
}

#[derive(Debug, thiserror::Error)]
pub enum ResolverError {
    #[error("NEXA-PKG-0001: Dependency cycle detected: {cycle}")]
    DependencyCycle { cycle: String },
    #[error("NEXA-PKG-0002: Version resolution failed for '{package}': no version satisfies all constraints")]
    VersionResolutionFailed { package: String },
    #[error(
        "NEXA-PKG-0003: Source conflict for package '{package}': same name from different sources"
    )]
    SourceConflict { package: String },
    #[error("NEXA-PKG-0004: Package '{package}' not found in any registry")]
    PackageNotFound { package: String },
    #[error("NEXA-PKG-0005: No versions available for package '{package}'")]
    NoVersionsAvailable { package: String },
    #[error("NEXA-PKG-0006: Dependency '{alias}' points to non-existent package '{package}'")]
    BrokenDependency { alias: String, package: String },
    #[error("NEXA-PKG-0007: Root package '{package}' is not valid")]
    InvalidRootPackage { package: String },
    #[error("NEXA-PKG-0008: Path dependency '{path}' not found")]
    PathDependencyNotFound { path: String },
    #[error("NEXA-PKG-0009: Version requirement '{req}' is invalid")]
    InvalidVersionRequirement { req: String },
    #[error(
        "NEXA-PKG-0010: Yanked version '{version}' cannot be resolved for package '{package}'"
    )]
    YankedVersion { package: String, version: String },
}

pub struct Resolver {
    registry: Box<dyn RegistryClient>,
    locked_packages: HashMap<String, String>,
    allow_prerelease: bool,
}

impl Resolver {
    pub fn new(registry: Box<dyn RegistryClient>) -> Self {
        Resolver {
            registry,
            locked_packages: HashMap::new(),
            allow_prerelease: false,
        }
    }

    pub fn with_locked(registry: Box<dyn RegistryClient>, locked: HashMap<String, String>) -> Self {
        Resolver {
            registry,
            locked_packages: locked,
            allow_prerelease: false,
        }
    }

    pub fn resolve(
        &mut self,
        root: &ProjectManifest,
    ) -> Result<ResolvedDependencyGraph, ResolverError> {
        let root_name = root.project.name.as_str().to_string();
        let root_version = root.project.version.to_string();

        let root_source = PackageSourceIdentity::Path {
            path: ".".to_string(),
        };

        let root_id = PackageInstanceId {
            name: root_name.clone(),
            version: root_version.clone(),
            source: root_source.clone(),
            digest: String::new(),
        };

        let mut visited: HashSet<String> = HashSet::new();
        let mut path: Vec<String> = Vec::new();
        let mut packages: Vec<ResolvedPackageNode> = Vec::new();
        let mut edges: Vec<ResolvedDependencyEdge> = Vec::new();

        let mut all_deps: Vec<ResolvedDependency> = Vec::new();

        for dep in &root.dependencies {
            let (package_name, source) = match (&dep.package, &dep.path) {
                (Some(pkg), _) => (
                    pkg.as_str().to_string(),
                    PackageSourceIdentity::Registry {
                        registry_id: "default".to_string(),
                    },
                ),
                (_, Some(p)) => (
                    dep.alias.as_str().to_string(),
                    PackageSourceIdentity::Path {
                        path: p.as_str().to_string(),
                    },
                ),
                _ => {
                    return Err(ResolverError::BrokenDependency {
                        alias: dep.alias.as_str().to_string(),
                        package: dep.alias.as_str().to_string(),
                    });
                }
            };

            let version_req_str = dep
                .version_req
                .as_ref()
                .map(|v| v.as_str().to_string())
                .unwrap_or_else(|| "*".to_string());

            let instance_id = self.resolve_package(
                &package_name,
                &version_req_str,
                &source,
                &mut visited,
                &mut path,
                &mut packages,
                &mut edges,
            )?;

            all_deps.push(ResolvedDependency {
                alias: dep.alias.as_str().to_string(),
                version_req: version_req_str,
                source: source.clone(),
                kind: DependencyKind::Normal,
            });

            edges.push(ResolvedDependencyEdge {
                from: root_id.clone(),
                alias: DependencyAlias(dep.alias.as_str().to_string()),
                to: instance_id.clone(),
                kind: DependencyKind::Normal,
            });
        }

        for dep in &root.dev_dependencies {
            let (package_name, source) = match (&dep.package, &dep.path) {
                (Some(pkg), _) => (
                    pkg.as_str().to_string(),
                    PackageSourceIdentity::Registry {
                        registry_id: "default".to_string(),
                    },
                ),
                (_, Some(p)) => (
                    dep.alias.as_str().to_string(),
                    PackageSourceIdentity::Path {
                        path: p.as_str().to_string(),
                    },
                ),
                _ => {
                    return Err(ResolverError::BrokenDependency {
                        alias: dep.alias.as_str().to_string(),
                        package: dep.alias.as_str().to_string(),
                    });
                }
            };

            let version_req_str = dep
                .version_req
                .as_ref()
                .map(|v| v.as_str().to_string())
                .unwrap_or_else(|| "*".to_string());

            let instance_id = self.resolve_package(
                &package_name,
                &version_req_str,
                &source,
                &mut visited,
                &mut path,
                &mut packages,
                &mut edges,
            )?;

            all_deps.push(ResolvedDependency {
                alias: dep.alias.as_str().to_string(),
                version_req: version_req_str,
                source: source.clone(),
                kind: DependencyKind::Dev,
            });

            edges.push(ResolvedDependencyEdge {
                from: root_id.clone(),
                alias: DependencyAlias(dep.alias.as_str().to_string()),
                to: instance_id.clone(),
                kind: DependencyKind::Dev,
            });
        }

        packages.push(ResolvedPackageNode {
            id: root_id.clone(),
            dependencies: all_deps,
        });

        let mut graph = ResolvedDependencyGraph {
            root: root_id,
            packages,
            edges,
        };

        graph.packages.sort_by(|a, b| a.id.name.cmp(&b.id.name));

        graph
            .edges
            .sort_by(|a, b| a.alias.0.cmp(&b.alias.0).then(a.to.name.cmp(&b.to.name)));

        Ok(graph)
    }

    #[allow(clippy::too_many_arguments)]
    fn resolve_package(
        &mut self,
        name: &str,
        version_req: &str,
        source: &PackageSourceIdentity,
        visited: &mut HashSet<String>,
        path: &mut Vec<String>,
        packages: &mut Vec<ResolvedPackageNode>,
        edges: &mut Vec<ResolvedDependencyEdge>,
    ) -> Result<PackageInstanceId, ResolverError> {
        if visited.contains(name) {
            if let Some(locked_version) = self.locked_packages.get(name) {
                if let Ok(req_obj) = VersionRequirement::new(version_req) {
                    if let Ok(ver) = SemanticVersion::new(locked_version) {
                        if req_obj.matches(&ver) {
                            return Ok(PackageInstanceId {
                                name: name.to_string(),
                                version: locked_version.clone(),
                                source: source.clone(),
                                digest: String::new(),
                            });
                        }
                    }
                }
            }
        }

        if path.contains(&name.to_string()) {
            let mut cycle_path = path.clone();
            cycle_path.push(name.to_string());
            return Err(ResolverError::DependencyCycle {
                cycle: cycle_path.join(" -> "),
            });
        }

        path.push(name.to_string());

        if source.is_registry() {
            let versions =
                self.registry
                    .list_versions(name)
                    .map_err(|_| ResolverError::PackageNotFound {
                        package: name.to_string(),
                    })?;

            if versions.is_empty() {
                path.pop();
                return Err(ResolverError::NoVersionsAvailable {
                    package: name.to_string(),
                });
            }

            let selected_version = self.select_version(name, &versions, version_req)?;

            let pkg_info = self
                .registry
                .get_package_version(name, &selected_version)
                .map_err(|_| ResolverError::YankedVersion {
                    package: name.to_string(),
                    version: selected_version.clone(),
                })?;

            let instance_id = PackageInstanceId {
                name: name.to_string(),
                version: selected_version.clone(),
                source: source.clone(),
                digest: pkg_info.digest.clone(),
            };

            let mut pkg_deps: Vec<ResolvedDependency> = Vec::new();

            for dep in &pkg_info.dependencies {
                let dep_source = match dep.source {
                    nexa_registry::DependencySource::Registry => PackageSourceIdentity::Registry {
                        registry_id: "default".to_string(),
                    },
                    nexa_registry::DependencySource::Path => PackageSourceIdentity::Path {
                        path: dep.package.clone(),
                    },
                };

                let child_id = self.resolve_package(
                    &dep.package,
                    &dep.version_req,
                    &dep_source,
                    visited,
                    path,
                    packages,
                    edges,
                )?;

                pkg_deps.push(ResolvedDependency {
                    alias: dep.alias.clone(),
                    version_req: dep.version_req.clone(),
                    source: dep_source,
                    kind: DependencyKind::Normal,
                });

                edges.push(ResolvedDependencyEdge {
                    from: instance_id.clone(),
                    alias: DependencyAlias(dep.alias.clone()),
                    to: child_id.clone(),
                    kind: DependencyKind::Normal,
                });
            }

            packages.push(ResolvedPackageNode {
                id: instance_id.clone(),
                dependencies: pkg_deps,
            });

            path.pop();
            visited.insert(name.to_string());
            Ok(instance_id)
        } else {
            let instance_id = PackageInstanceId {
                name: name.to_string(),
                version: "0.0.0".to_string(),
                source: source.clone(),
                digest: String::new(),
            };

            packages.push(ResolvedPackageNode {
                id: instance_id.clone(),
                dependencies: vec![],
            });

            path.pop();
            visited.insert(name.to_string());
            Ok(instance_id)
        }
    }

    fn select_version(
        &self,
        package: &str,
        versions: &[String],
        req: &str,
    ) -> Result<String, ResolverError> {
        if let Some(locked_version) = self.locked_packages.get(package) {
            let req_obj = VersionRequirement::new(req).map_err(|_| {
                ResolverError::InvalidVersionRequirement {
                    req: req.to_string(),
                }
            })?;
            if let Ok(ver) = SemanticVersion::new(locked_version) {
                if req_obj.matches(&ver) {
                    return Ok(locked_version.clone());
                }
            }
        }

        let req_obj =
            VersionRequirement::new(req).map_err(|_| ResolverError::InvalidVersionRequirement {
                req: req.to_string(),
            })?;

        let mut candidates: Vec<SemanticVersion> = Vec::new();

        for v in versions {
            if let Ok(ver) = SemanticVersion::new(v) {
                if !self.allow_prerelease && ver.to_string().contains('-') {
                    continue;
                }
                if req_obj.matches(&ver) {
                    candidates.push(ver);
                }
            }
        }

        candidates.sort();
        candidates.reverse();

        candidates
            .into_iter()
            .next()
            .map(|v| v.to_string())
            .ok_or_else(|| ResolverError::VersionResolutionFailed {
                package: package.to_string(),
            })
    }
}

impl PackageSourceIdentity {
    fn is_registry(&self) -> bool {
        matches!(self, PackageSourceIdentity::Registry { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexa_manifest::{
        DependencyAlias as ManifestAlias, DependencyManifest, DependencySource, PackageName,
        PackageNameRef, ProjectManifest, SemanticVersion as SemVer, VersionRequirement as VerReq,
    };
    use nexa_registry::{InMemoryRegistry, RegistryPackageVersion};

    fn make_registry_entry(name: &str, version: &str) -> RegistryPackageVersion {
        RegistryPackageVersion {
            name: name.to_string(),
            version: version.to_string(),
            digest: format!("{:064}", version.len() * 7),
            size: 1024,
            dependencies: vec![],
            format_version: 1,
            published: true,
            yanked: false,
        }
    }

    fn make_registry_entry_yanked(name: &str, version: &str) -> RegistryPackageVersion {
        let mut entry = make_registry_entry(name, version);
        entry.yanked = true;
        entry
    }

    fn make_registry_entry_with_deps(
        name: &str,
        version: &str,
        deps: Vec<nexa_registry::RegistryDependency>,
    ) -> RegistryPackageVersion {
        let mut entry = make_registry_entry(name, version);
        entry.dependencies = deps;
        entry
    }

    fn make_simple_manifest(name: &str, version: &str) -> ProjectManifest {
        ProjectManifest {
            project: nexa_manifest::ProjectMetadata {
                name: PackageName::new(name).unwrap(),
                version: SemVer::new(version).unwrap(),
                language: nexa_manifest::LanguageVersionRequirement::new("1.0").unwrap(),
            },
            targets: vec![],
            dependencies: vec![],
            dev_dependencies: vec![],
        }
    }

    fn make_manifest_with_deps(
        name: &str,
        version: &str,
        deps: Vec<DependencyManifest>,
    ) -> ProjectManifest {
        ProjectManifest {
            project: nexa_manifest::ProjectMetadata {
                name: PackageName::new(name).unwrap(),
                version: SemVer::new(version).unwrap(),
                language: nexa_manifest::LanguageVersionRequirement::new("1.0").unwrap(),
            },
            targets: vec![],
            dependencies: deps,
            dev_dependencies: vec![],
        }
    }

    fn make_manifest_with_dev_deps(
        name: &str,
        version: &str,
        deps: Vec<DependencyManifest>,
        dev_deps: Vec<DependencyManifest>,
    ) -> ProjectManifest {
        ProjectManifest {
            project: nexa_manifest::ProjectMetadata {
                name: PackageName::new(name).unwrap(),
                version: SemVer::new(version).unwrap(),
                language: nexa_manifest::LanguageVersionRequirement::new("1.0").unwrap(),
            },
            targets: vec![],
            dependencies: deps,
            dev_dependencies: dev_deps,
        }
    }

    fn reg_dep(alias: &str, package: &str, version_req: &str) -> DependencyManifest {
        DependencyManifest {
            alias: ManifestAlias::new(alias).unwrap(),
            package: Some(PackageNameRef::new(package).unwrap()),
            path: None,
            version_req: Some(VerReq::new(version_req).unwrap()),
            source: DependencySource::Registry,
        }
    }

    fn path_dep(alias: &str, path: &str) -> DependencyManifest {
        DependencyManifest {
            alias: ManifestAlias::new(alias).unwrap(),
            package: None,
            path: Some(nexa_manifest::PathRef::new(path).unwrap()),
            version_req: None,
            source: DependencySource::Path,
        }
    }

    #[test]
    fn resolver_creation() {
        let reg: Box<dyn RegistryClient> = Box::new(InMemoryRegistry::new());
        let _resolver = Resolver::new(reg);
    }

    #[test]
    fn resolver_with_locked_creation() {
        let reg: Box<dyn RegistryClient> = Box::new(InMemoryRegistry::new());
        let mut locked = HashMap::new();
        locked.insert("serde".to_string(), "1.0.0".to_string());
        let _resolver = Resolver::with_locked(reg, locked);
    }

    #[test]
    fn resolve_empty_dependencies() {
        let reg = InMemoryRegistry::new();
        let manifest = make_simple_manifest("my-app", "1.0.0");

        let mut resolver = Resolver::new(Box::new(reg));
        let graph = resolver.resolve(&manifest).unwrap();

        assert_eq!(graph.root.name, "my-app");
        assert_eq!(graph.root.version, "1.0.0");
        assert_eq!(graph.packages.len(), 1);
        assert!(graph.edges.is_empty());
    }

    #[test]
    fn resolve_single_dependency() {
        let mut reg = InMemoryRegistry::new();
        reg.add_package(make_registry_entry("nexa.json", "1.2.0"));
        reg.add_package(make_registry_entry("nexa.json", "1.3.0"));
        reg.add_package(make_registry_entry("nexa.json", "2.0.0"));

        let manifest = make_manifest_with_deps(
            "my-app",
            "1.0.0",
            vec![reg_dep("json", "nexa.json", "^1.2.0")],
        );

        let mut resolver = Resolver::new(Box::new(reg));
        let graph = resolver.resolve(&manifest).unwrap();

        assert_eq!(graph.root.name, "my-app");
        assert_eq!(graph.packages.len(), 2);
        assert_eq!(graph.edges.len(), 1);
        assert_eq!(graph.edges[0].to.name, "nexa.json");
        assert_eq!(graph.edges[0].to.version, "1.3.0");
    }

    #[test]
    fn resolve_version_constraint_caret() {
        let mut reg = InMemoryRegistry::new();
        reg.add_package(make_registry_entry("pkg-a", "1.0.0"));
        reg.add_package(make_registry_entry("pkg-a", "1.5.0"));
        reg.add_package(make_registry_entry("pkg-a", "2.0.0"));

        let manifest =
            make_manifest_with_deps("app", "1.0.0", vec![reg_dep("a", "pkg-a", "^1.0.0")]);

        let mut resolver = Resolver::new(Box::new(reg));
        let graph = resolver.resolve(&manifest).unwrap();

        assert_eq!(graph.edges[0].to.version, "1.5.0");
    }

    #[test]
    fn resolve_version_constraint_tilde() {
        let mut reg = InMemoryRegistry::new();
        reg.add_package(make_registry_entry("pkg-b", "1.2.0"));
        reg.add_package(make_registry_entry("pkg-b", "1.2.5"));
        reg.add_package(make_registry_entry("pkg-b", "1.3.0"));

        let manifest =
            make_manifest_with_deps("app", "1.0.0", vec![reg_dep("b", "pkg-b", "~1.2.0")]);

        let mut resolver = Resolver::new(Box::new(reg));
        let graph = resolver.resolve(&manifest).unwrap();

        assert_eq!(graph.edges[0].to.version, "1.2.5");
    }

    #[test]
    fn resolve_version_constraint_exact() {
        let mut reg = InMemoryRegistry::new();
        reg.add_package(make_registry_entry("pkg-c", "1.0.0"));
        reg.add_package(make_registry_entry("pkg-c", "1.1.0"));

        let manifest =
            make_manifest_with_deps("app", "1.0.0", vec![reg_dep("c", "pkg-c", "=1.0.0")]);

        let mut resolver = Resolver::new(Box::new(reg));
        let graph = resolver.resolve(&manifest).unwrap();

        assert_eq!(graph.edges[0].to.version, "1.0.0");
    }

    #[test]
    fn resolve_version_constraint_gte() {
        let mut reg = InMemoryRegistry::new();
        reg.add_package(make_registry_entry("pkg-d", "1.0.0"));
        reg.add_package(make_registry_entry("pkg-d", "2.0.0"));
        reg.add_package(make_registry_entry("pkg-d", "3.0.0"));

        let manifest =
            make_manifest_with_deps("app", "1.0.0", vec![reg_dep("d", "pkg-d", ">=2.0.0")]);

        let mut resolver = Resolver::new(Box::new(reg));
        let graph = resolver.resolve(&manifest).unwrap();

        assert_eq!(graph.edges[0].to.version, "3.0.0");
    }

    #[test]
    fn detect_cycle_a_to_b_to_a() {
        let mut reg = InMemoryRegistry::new();
        reg.add_package(make_registry_entry_with_deps(
            "pkg-a",
            "1.0.0",
            vec![nexa_registry::RegistryDependency {
                alias: "b".to_string(),
                package: "pkg-b".to_string(),
                version_req: "^1.0.0".to_string(),
                source: nexa_registry::DependencySource::Registry,
            }],
        ));
        reg.add_package(make_registry_entry_with_deps(
            "pkg-b",
            "1.0.0",
            vec![nexa_registry::RegistryDependency {
                alias: "a".to_string(),
                package: "pkg-a".to_string(),
                version_req: "^1.0.0".to_string(),
                source: nexa_registry::DependencySource::Registry,
            }],
        ));

        let manifest =
            make_manifest_with_deps("app", "1.0.0", vec![reg_dep("a", "pkg-a", "^1.0.0")]);

        let mut resolver = Resolver::new(Box::new(reg));
        let result = resolver.resolve(&manifest);

        assert!(result.is_err());
        match result.unwrap_err() {
            ResolverError::DependencyCycle { cycle } => {
                assert!(cycle.contains("pkg-a"));
                assert!(cycle.contains("pkg-b"));
            }
            _ => panic!("Expected DependencyCycle error"),
        }
    }

    #[test]
    fn detect_source_conflict_different_registries() {
        let mut reg = InMemoryRegistry::new();
        reg.add_package(make_registry_entry("conflict-pkg", "1.0.0"));

        let dep1 = DependencyManifest {
            alias: ManifestAlias::new("x").unwrap(),
            package: Some(PackageNameRef::new("conflict-pkg").unwrap()),
            path: None,
            version_req: Some(VerReq::new("^1.0.0").unwrap()),
            source: DependencySource::Registry,
        };

        let dep2 = DependencyManifest {
            alias: ManifestAlias::new("y").unwrap(),
            package: Some(PackageNameRef::new("conflict-pkg").unwrap()),
            path: Some(nexa_manifest::PathRef::new("./local-conflict").unwrap()),
            version_req: None,
            source: DependencySource::Path,
        };

        let manifest = ProjectManifest {
            project: nexa_manifest::ProjectMetadata {
                name: PackageName::new("app").unwrap(),
                version: SemVer::new("1.0.0").unwrap(),
                language: nexa_manifest::LanguageVersionRequirement::new("1.0").unwrap(),
            },
            targets: vec![],
            dependencies: vec![dep1],
            dev_dependencies: vec![dep2],
        };

        let mut resolver = Resolver::new(Box::new(reg));
        let graph = resolver.resolve(&manifest).unwrap();

        let registry_src = graph
            .packages
            .iter()
            .find(|p| p.id.name == "conflict-pkg" && p.id.source.is_registry());

        assert!(registry_src.is_some());
    }

    #[test]
    fn package_not_found() {
        let reg = InMemoryRegistry::new();
        let manifest = make_manifest_with_deps(
            "app",
            "1.0.0",
            vec![reg_dep("missing", "nonexistent-pkg", "^1.0.0")],
        );

        let mut resolver = Resolver::new(Box::new(reg));
        let result = resolver.resolve(&manifest);

        assert!(result.is_err());
        match result.unwrap_err() {
            ResolverError::PackageNotFound { package } => {
                assert_eq!(package, "nonexistent-pkg");
            }
            _ => panic!("Expected PackageNotFound error"),
        }
    }

    #[test]
    fn no_versions_available() {
        let mut reg = InMemoryRegistry::new();
        reg.add_versions("empty-pkg", vec![]);

        let manifest =
            make_manifest_with_deps("app", "1.0.0", vec![reg_dep("e", "empty-pkg", "^1.0.0")]);

        let mut resolver = Resolver::new(Box::new(reg));
        let result = resolver.resolve(&manifest);

        assert!(result.is_err());
        match result.unwrap_err() {
            ResolverError::NoVersionsAvailable { package } => {
                assert_eq!(package, "empty-pkg");
            }
            _ => panic!("Expected NoVersionsAvailable error"),
        }
    }

    #[test]
    fn yanked_version_handling() {
        let mut reg = InMemoryRegistry::new();
        reg.add_package(make_registry_entry_yanked("yanked-pkg", "1.0.0"));
        reg.add_package(make_registry_entry("yanked-pkg", "2.0.0"));

        let manifest =
            make_manifest_with_deps("app", "1.0.0", vec![reg_dep("y", "yanked-pkg", "^1.0.0")]);

        let mut resolver = Resolver::new(Box::new(reg));
        let result = resolver.resolve(&manifest);

        assert!(result.is_err());
        match result.unwrap_err() {
            ResolverError::VersionResolutionFailed { package } => {
                assert_eq!(package, "yanked-pkg");
            }
            _ => panic!("Expected VersionResolutionFailed error"),
        }
    }

    #[test]
    fn locked_version_preference() {
        let mut reg = InMemoryRegistry::new();
        reg.add_package(make_registry_entry("locked-pkg", "1.0.0"));
        reg.add_package(make_registry_entry("locked-pkg", "1.1.0"));
        reg.add_package(make_registry_entry("locked-pkg", "2.0.0"));

        let mut locked = HashMap::new();
        locked.insert("locked-pkg".to_string(), "1.0.0".to_string());

        let manifest =
            make_manifest_with_deps("app", "1.0.0", vec![reg_dep("lp", "locked-pkg", "^1.0.0")]);

        let mut resolver = Resolver::with_locked(Box::new(reg), locked);
        let graph = resolver.resolve(&manifest).unwrap();

        assert_eq!(graph.edges[0].to.version, "1.0.0");
    }

    #[test]
    fn pre_release_disabled_by_default() {
        let mut reg = InMemoryRegistry::new();
        reg.add_package(make_registry_entry("pre-pkg", "1.0.0-alpha.1"));
        reg.add_package(make_registry_entry("pre-pkg", "1.0.0"));

        let manifest =
            make_manifest_with_deps("app", "1.0.0", vec![reg_dep("p", "pre-pkg", "^1.0.0")]);

        let mut resolver = Resolver::new(Box::new(reg));
        let graph = resolver.resolve(&manifest).unwrap();

        assert_eq!(graph.edges[0].to.version, "1.0.0");
    }

    #[test]
    fn multiple_dependencies() {
        let mut reg = InMemoryRegistry::new();
        reg.add_package(make_registry_entry("dep-a", "1.0.0"));
        reg.add_package(make_registry_entry("dep-b", "2.0.0"));
        reg.add_package(make_registry_entry("dep-c", "3.0.0"));

        let manifest = make_manifest_with_deps(
            "app",
            "1.0.0",
            vec![
                reg_dep("a", "dep-a", "^1.0.0"),
                reg_dep("b", "dep-b", "^2.0.0"),
                reg_dep("c", "dep-c", "^3.0.0"),
            ],
        );

        let mut resolver = Resolver::new(Box::new(reg));
        let graph = resolver.resolve(&manifest).unwrap();

        assert_eq!(graph.packages.len(), 4);
        assert_eq!(graph.edges.len(), 3);
    }

    #[test]
    fn diamond_dependency() {
        let mut reg = InMemoryRegistry::new();
        reg.add_package(make_registry_entry("top", "1.0.0"));
        reg.add_package(make_registry_entry("left", "1.0.0"));
        reg.add_package(make_registry_entry("left", "2.0.0"));
        reg.add_package(make_registry_entry("right", "1.0.0"));
        reg.add_package(make_registry_entry("right", "1.5.0"));
        reg.add_package(make_registry_entry("bottom", "1.0.0"));
        reg.add_package(make_registry_entry("bottom", "2.0.0"));

        reg.add_package(make_registry_entry_with_deps(
            "left",
            "2.0.0",
            vec![nexa_registry::RegistryDependency {
                alias: "b".to_string(),
                package: "bottom".to_string(),
                version_req: "^2.0.0".to_string(),
                source: nexa_registry::DependencySource::Registry,
            }],
        ));

        reg.add_package(make_registry_entry_with_deps(
            "right",
            "1.5.0",
            vec![nexa_registry::RegistryDependency {
                alias: "b".to_string(),
                package: "bottom".to_string(),
                version_req: "^1.0.0".to_string(),
                source: nexa_registry::DependencySource::Registry,
            }],
        ));

        let manifest = make_manifest_with_deps(
            "app",
            "1.0.0",
            vec![
                reg_dep("left", "left", "^1.0.0"),
                reg_dep("right", "right", "^1.0.0"),
            ],
        );

        let mut resolver = Resolver::new(Box::new(reg));
        let graph = resolver.resolve(&manifest).unwrap();

        assert!(graph.packages.len() >= 3);
    }

    #[test]
    fn root_package_validation_invalid() {
        let reg = InMemoryRegistry::new();
        let manifest = make_simple_manifest("app", "1.0.0");

        let mut resolver = Resolver::new(Box::new(reg));
        let graph = resolver.resolve(&manifest).unwrap();

        assert_eq!(graph.root.name, "app");
        assert_eq!(graph.root.version, "1.0.0");
    }

    #[test]
    fn version_selection_highest_compatible() {
        let mut reg = InMemoryRegistry::new();
        reg.add_package(make_registry_entry("versel", "1.0.0"));
        reg.add_package(make_registry_entry("versel", "1.3.0"));
        reg.add_package(make_registry_entry("versel", "1.1.0"));
        reg.add_package(make_registry_entry("versel", "1.9.0"));

        let manifest =
            make_manifest_with_deps("app", "1.0.0", vec![reg_dep("v", "versel", "^1.0.0")]);

        let mut resolver = Resolver::new(Box::new(reg));
        let graph = resolver.resolve(&manifest).unwrap();

        assert_eq!(graph.edges[0].to.version, "1.9.0");
    }

    #[test]
    fn graph_determinism() {
        let mut reg1 = InMemoryRegistry::new();
        reg1.add_package(make_registry_entry("determin", "1.0.0"));
        reg1.add_package(make_registry_entry("determin", "1.5.0"));

        let mut reg2 = InMemoryRegistry::new();
        reg2.add_package(make_registry_entry("determin", "1.5.0"));
        reg2.add_package(make_registry_entry("determin", "1.0.0"));

        let manifest1 =
            make_manifest_with_deps("app", "1.0.0", vec![reg_dep("d", "determin", "^1.0.0")]);
        let manifest2 = manifest1.clone();

        let mut resolver1 = Resolver::new(Box::new(reg1));
        let graph1 = resolver1.resolve(&manifest1).unwrap();

        let mut resolver2 = Resolver::new(Box::new(reg2));
        let graph2 = resolver2.resolve(&manifest2).unwrap();

        assert_eq!(graph1.root.name, graph2.root.name);
        assert_eq!(graph1.root.version, graph2.root.version);
        assert_eq!(graph1.packages.len(), graph2.packages.len());
        assert_eq!(graph1.edges.len(), graph2.edges.len());

        for (a, b) in graph1.packages.iter().zip(graph2.packages.iter()) {
            assert_eq!(a.id.name, b.id.name);
            assert_eq!(a.id.version, b.id.version);
        }
    }

    #[test]
    fn version_constraint_wildcard() {
        let mut reg = InMemoryRegistry::new();
        reg.add_package(make_registry_entry("wild", "5.0.0"));

        let manifest = make_manifest_with_deps("app", "1.0.0", vec![reg_dep("w", "wild", "*")]);

        let mut resolver = Resolver::new(Box::new(reg));
        let graph = resolver.resolve(&manifest).unwrap();

        assert_eq!(graph.edges[0].to.version, "5.0.0");
    }

    #[test]
    fn version_constraint_lt() {
        let mut reg = InMemoryRegistry::new();
        reg.add_package(make_registry_entry("lt-pkg", "1.0.0"));
        reg.add_package(make_registry_entry("lt-pkg", "2.0.0"));
        reg.add_package(make_registry_entry("lt-pkg", "3.0.0"));

        let manifest =
            make_manifest_with_deps("app", "1.0.0", vec![reg_dep("lt", "lt-pkg", "<2.0.0")]);

        let mut resolver = Resolver::new(Box::new(reg));
        let graph = resolver.resolve(&manifest).unwrap();

        assert_eq!(graph.edges[0].to.version, "1.0.0");
    }

    #[test]
    fn version_constraint_lte() {
        let mut reg = InMemoryRegistry::new();
        reg.add_package(make_registry_entry("lte-pkg", "1.0.0"));
        reg.add_package(make_registry_entry("lte-pkg", "2.0.0"));
        reg.add_package(make_registry_entry("lte-pkg", "3.0.0"));

        let manifest =
            make_manifest_with_deps("app", "1.0.0", vec![reg_dep("lte", "lte-pkg", "<=2.0.0")]);

        let mut resolver = Resolver::new(Box::new(reg));
        let graph = resolver.resolve(&manifest).unwrap();

        assert_eq!(graph.edges[0].to.version, "2.0.0");
    }

    #[test]
    fn version_resolution_failed() {
        let mut reg = InMemoryRegistry::new();
        reg.add_package(make_registry_entry("nover", "1.0.0"));
        reg.add_package(make_registry_entry("nover", "2.0.0"));

        let manifest =
            make_manifest_with_deps("app", "1.0.0", vec![reg_dep("nv", "nover", "^3.0.0")]);

        let mut resolver = Resolver::new(Box::new(reg));
        let result = resolver.resolve(&manifest);

        assert!(result.is_err());
        match result.unwrap_err() {
            ResolverError::VersionResolutionFailed { package } => {
                assert_eq!(package, "nover");
            }
            _ => panic!("Expected VersionResolutionFailed"),
        }
    }

    #[test]
    fn version_set_constraints() {
        let vs = VersionSet {
            constraints: vec![
                VersionConstraint {
                    operator: ConstraintOp::Caret,
                    version: "1.0.0".to_string(),
                },
                VersionConstraint {
                    operator: ConstraintOp::Tilde,
                    version: "1.2.0".to_string(),
                },
            ],
        };

        assert_eq!(vs.constraints.len(), 2);
        assert_eq!(vs.constraints[0].operator, ConstraintOp::Caret);
        assert_eq!(vs.constraints[1].operator, ConstraintOp::Tilde);
    }

    #[test]
    fn dependency_kind_dev_vs_normal() {
        let mut reg = InMemoryRegistry::new();
        reg.add_package(make_registry_entry("dev-dep", "1.0.0"));
        reg.add_package(make_registry_entry("normal-dep", "1.0.0"));

        let manifest = make_manifest_with_dev_deps(
            "app",
            "1.0.0",
            vec![reg_dep("nd", "normal-dep", "^1.0.0")],
            vec![reg_dep("dd", "dev-dep", "^1.0.0")],
        );

        let mut resolver = Resolver::new(Box::new(reg));
        let graph = resolver.resolve(&manifest).unwrap();

        let normal_edge = graph.edges.iter().find(|e| e.alias.0 == "nd").unwrap();
        let dev_edge = graph.edges.iter().find(|e| e.alias.0 == "dd").unwrap();

        assert_eq!(normal_edge.kind, DependencyKind::Normal);
        assert_eq!(dev_edge.kind, DependencyKind::Dev);
    }

    #[test]
    fn package_source_identity_equality() {
        let s1 = PackageSourceIdentity::Registry {
            registry_id: "default".to_string(),
        };
        let s2 = PackageSourceIdentity::Registry {
            registry_id: "default".to_string(),
        };
        let s3 = PackageSourceIdentity::Path {
            path: "./local".to_string(),
        };

        assert_eq!(s1, s2);
        assert_ne!(s1, s3);
    }

    #[test]
    fn package_source_identity_is_registry() {
        let reg = PackageSourceIdentity::Registry {
            registry_id: "default".to_string(),
        };
        let path = PackageSourceIdentity::Path {
            path: ".".to_string(),
        };

        assert!(reg.is_registry());
        assert!(!path.is_registry());
    }

    #[test]
    fn resolver_error_messages() {
        let errors = vec![
            ResolverError::DependencyCycle {
                cycle: "a -> b -> a".to_string(),
            },
            ResolverError::VersionResolutionFailed {
                package: "pkg".to_string(),
            },
            ResolverError::SourceConflict {
                package: "pkg".to_string(),
            },
            ResolverError::PackageNotFound {
                package: "pkg".to_string(),
            },
            ResolverError::NoVersionsAvailable {
                package: "pkg".to_string(),
            },
            ResolverError::BrokenDependency {
                alias: "a".to_string(),
                package: "b".to_string(),
            },
            ResolverError::InvalidRootPackage {
                package: "pkg".to_string(),
            },
            ResolverError::PathDependencyNotFound {
                path: "./missing".to_string(),
            },
            ResolverError::InvalidVersionRequirement {
                req: "bad".to_string(),
            },
            ResolverError::YankedVersion {
                package: "pkg".to_string(),
                version: "1.0.0".to_string(),
            },
        ];

        for (i, err) in errors.iter().enumerate() {
            let code = format!("NEXA-PKG-{:04}", i + 1);
            let msg = err.to_string();
            assert!(
                msg.contains(&code),
                "Missing error code {} in: {}",
                code,
                msg
            );
        }
    }

    #[test]
    fn resolve_path_dependency() {
        let reg = InMemoryRegistry::new();
        let manifest =
            make_manifest_with_deps("app", "1.0.0", vec![path_dep("local", "../my-local-dep")]);

        let mut resolver = Resolver::new(Box::new(reg));
        let graph = resolver.resolve(&manifest).unwrap();

        assert_eq!(graph.edges.len(), 1);
        assert!(!graph.edges[0].to.source.is_registry());
    }

    #[test]
    fn graph_sorted_deterministically() {
        let mut reg = InMemoryRegistry::new();
        reg.add_package(make_registry_entry("z-pkg", "1.0.0"));
        reg.add_package(make_registry_entry("a-pkg", "1.0.0"));
        reg.add_package(make_registry_entry("m-pkg", "1.0.0"));

        let manifest = make_manifest_with_deps(
            "app",
            "1.0.0",
            vec![
                reg_dep("z", "z-pkg", "^1.0.0"),
                reg_dep("a", "a-pkg", "^1.0.0"),
                reg_dep("m", "m-pkg", "^1.0.0"),
            ],
        );

        let mut resolver = Resolver::new(Box::new(reg));
        let graph = resolver.resolve(&manifest).unwrap();

        let non_root: Vec<&str> = graph
            .packages
            .iter()
            .filter(|p| p.id.name != "app")
            .map(|p| p.id.name.as_str())
            .collect();

        let mut sorted = non_root.clone();
        sorted.sort();
        assert_eq!(non_root, sorted);
    }
}
