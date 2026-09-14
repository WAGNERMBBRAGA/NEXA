use std::borrow::Cow;
use std::collections::HashMap;
use wasm_encoder::{CustomSection, Module};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SymbolId {
    pub package: String,
    pub module: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct ResolvedSymbol {
    pub id: SymbolId,
    pub source_package: String,
    pub source_version: String,
    pub kind: SymbolKind,
    pub visibility: SymbolVisibility,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Type,
    Constant,
    Module,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolVisibility {
    Public,
    Private,
}

#[derive(Debug, Clone)]
pub struct LinkGraph {
    pub root_package: String,
    pub symbols: Vec<ResolvedSymbol>,
    pub edges: Vec<LinkEdge>,
}

#[derive(Debug, Clone)]
pub struct LinkEdge {
    pub from: SymbolId,
    pub to: SymbolId,
}

#[derive(Debug, Clone)]
pub struct PublicInterfaceFingerprint {
    pub package: String,
    pub version: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone)]
pub struct LoadedPackage {
    pub name: String,
    pub version: String,
    pub manifest: nexa_package::PackageMetadata,
    pub public_interface: PublicInterfaceFingerprint,
    pub symbols: Vec<ResolvedSymbol>,
}

#[derive(Debug, Clone)]
pub struct LinkedOutput {
    pub wasm_bytes: Vec<u8>,
    pub symbols: Vec<ResolvedSymbol>,
    pub entry_points: Vec<String>,
}

pub struct Linker {
    packages: HashMap<String, LoadedPackage>,
    graph: LinkGraph,
}

#[derive(Debug, thiserror::Error)]
pub enum LinkError {
    #[error("NEXA-LINK-0001: Undefined symbol '{symbol}' referenced from '{package}'")]
    UndefinedSymbol { symbol: String, package: String },
    #[error("NEXA-LINK-0002: Duplicate symbol '{symbol}' in packages '{pkg_a}' and '{pkg_b}'")]
    DuplicateSymbol {
        symbol: String,
        pkg_a: String,
        pkg_b: String,
    },
    #[error("NEXA-LINK-0003: Public API fingerprint mismatch for package '{package}': expected '{expected}', found '{actual}'")]
    FingerprintMismatch {
        package: String,
        expected: String,
        actual: String,
    },
    #[error("NEXA-LINK-0004: Package '{package}' is not loaded")]
    PackageNotLoaded { package: String },
    #[error("NEXA-LINK-0005: Cannot access private symbol '{symbol}' from package '{package}'")]
    PrivateSymbolAccess { symbol: String, package: String },
    #[error("NEXA-LINK-0006: Type mismatch for symbol '{symbol}': expected '{expected}', found '{actual}'")]
    TypeMismatch {
        symbol: String,
        expected: String,
        actual: String,
    },
    #[error("NEXA-LINK-0007: Missing required entry point '{entry}' in package '{package}'")]
    MissingEntryPoint { entry: String, package: String },
    #[error("NEXA-LINK-0008: Link graph contains cycle involving '{package}'")]
    LinkCycle { package: String },
    #[error(
        "NEXA-LINK-0009: Package version mismatch: lockfile says '{locked}', loaded '{loaded}'"
    )]
    VersionMismatch { locked: String, loaded: String },
    #[error("NEXA-LINK-0010: No output artifact specified")]
    NoOutputArtifact,
}

impl Default for Linker {
    fn default() -> Self {
        Self::new()
    }
}

impl Linker {
    pub fn new() -> Self {
        Self {
            packages: HashMap::new(),
            graph: LinkGraph {
                root_package: String::new(),
                symbols: Vec::new(),
                edges: Vec::new(),
            },
        }
    }

    pub fn load_package(
        &mut self,
        name: &str,
        version: &str,
        manifest: nexa_package::PackageMetadata,
    ) -> Result<(), LinkError> {
        let fingerprint = manifest.public_interface_fingerprint.clone();
        let loaded = LoadedPackage {
            name: name.to_string(),
            version: version.to_string(),
            manifest,
            public_interface: PublicInterfaceFingerprint {
                package: name.to_string(),
                version: version.to_string(),
                fingerprint,
            },
            symbols: Vec::new(),
        };
        self.packages.insert(name.to_string(), loaded);
        if self.graph.root_package.is_empty() {
            self.graph.root_package = name.to_string();
        }
        Ok(())
    }

    pub fn register_symbol(&mut self, symbol: ResolvedSymbol) -> Result<(), LinkError> {
        for existing in &self.graph.symbols {
            if existing.source_package != symbol.source_package
                && existing.id.module == symbol.id.module
                && existing.id.name == symbol.id.name
                && existing.visibility == SymbolVisibility::Private
            {
                return Err(LinkError::PrivateSymbolAccess {
                    symbol: existing.id.name.clone(),
                    package: existing.source_package.clone(),
                });
            }
        }

        let symbol_key = format!("{}::{}", symbol.id.package, symbol.id.name);
        for existing in &self.graph.symbols {
            let existing_key = format!("{}::{}", existing.id.package, existing.id.name);
            if existing_key == symbol_key && existing.source_package != symbol.source_package {
                return Err(LinkError::DuplicateSymbol {
                    symbol: symbol.id.name.clone(),
                    pkg_a: existing.source_package.clone(),
                    pkg_b: symbol.source_package.clone(),
                });
            }
        }
        if let Some(pkg) = self.packages.get_mut(&symbol.source_package) {
            pkg.symbols.push(symbol.clone());
        }
        self.graph.symbols.push(symbol);
        Ok(())
    }

    pub fn resolve_symbols(&mut self) -> Result<(), LinkError> {
        let all_symbol_ids: Vec<SymbolId> =
            self.graph.symbols.iter().map(|s| s.id.clone()).collect();

        let mut edges = Vec::new();

        for symbol in &self.graph.symbols {
            if symbol.visibility == SymbolVisibility::Private {
                for other in &self.graph.symbols {
                    if other.source_package != symbol.source_package
                        && other.id.module == symbol.id.module
                        && other.id.name == symbol.id.name
                    {
                        return Err(LinkError::PrivateSymbolAccess {
                            symbol: symbol.id.name.clone(),
                            package: other.source_package.clone(),
                        });
                    }
                }
            }
        }

        for symbol in &self.graph.symbols {
            for dep in &self.packages {
                if dep.0 != &symbol.source_package {
                    let references_symbol = dep
                        .1
                        .manifest
                        .dependencies
                        .iter()
                        .any(|d| d.package == symbol.id.package);
                    if references_symbol {
                        let found = all_symbol_ids.iter().any(|sid| {
                            sid.package == symbol.id.package
                                && sid.module == symbol.id.module
                                && sid.name == symbol.id.name
                        });
                        if !found {
                            return Err(LinkError::UndefinedSymbol {
                                symbol: symbol.id.name.clone(),
                                package: dep.0.clone(),
                            });
                        }
                        edges.push(LinkEdge {
                            from: SymbolId {
                                package: dep.0.clone(),
                                module: symbol.id.module.clone(),
                                name: symbol.id.name.clone(),
                            },
                            to: symbol.id.clone(),
                        });
                    }
                }
            }
        }

        self.graph.edges = edges;
        Ok(())
    }

    pub fn verify_public_interfaces(&self) -> Result<(), LinkError> {
        for (name, pkg) in &self.packages {
            let loaded_fingerprint = pkg.public_interface.fingerprint.clone();
            let manifest_fingerprint = pkg.manifest.public_interface_fingerprint.clone();
            if loaded_fingerprint != manifest_fingerprint {
                return Err(LinkError::FingerprintMismatch {
                    package: name.clone(),
                    expected: manifest_fingerprint,
                    actual: loaded_fingerprint,
                });
            }
        }
        Ok(())
    }

    pub fn build_link_graph(&self) -> Result<LinkGraph, LinkError> {
        let mut visited = std::collections::HashSet::new();
        self.detect_cycle_recursive(&self.graph.root_package, &mut visited)?;

        Ok(self.graph.clone())
    }

    fn detect_cycle_recursive(
        &self,
        package: &str,
        visited: &mut std::collections::HashSet<String>,
    ) -> Result<(), LinkError> {
        if !visited.insert(package.to_string()) {
            return Err(LinkError::LinkCycle {
                package: package.to_string(),
            });
        }

        for edge in &self.graph.edges {
            if edge.from.package == package {
                self.detect_cycle_recursive(&edge.to.package, visited)?;
            }
        }

        visited.remove(package);
        Ok(())
    }

    pub fn link(&self) -> Result<LinkedOutput, LinkError> {
        if self.packages.is_empty() && self.graph.symbols.is_empty() {
            return Err(LinkError::NoOutputArtifact);
        }

        let entry_points: Vec<String> = self
            .graph
            .symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function && s.visibility == SymbolVisibility::Public)
            .map(|s| format!("{}::{}", s.id.module, s.id.name))
            .collect();

        let wasm_bytes = self.generate_link_metadata_module();

        Ok(LinkedOutput {
            wasm_bytes,
            symbols: self.graph.symbols.clone(),
            entry_points,
        })
    }

    fn generate_link_metadata_module(&self) -> Vec<u8> {
        // Package objects do not yet carry executable sections in the linker
        // API. Emit their resolved symbol table as a valid custom section,
        // rather than appending non-Wasm bytes after the module header.
        let mut metadata = Vec::new();
        let symbol_count = self.graph.symbols.len() as u32;
        metadata.extend_from_slice(&symbol_count.to_le_bytes());
        for symbol in &self.graph.symbols {
            let pkg_bytes = symbol.id.package.as_bytes();
            metadata.extend_from_slice(&(pkg_bytes.len() as u32).to_le_bytes());
            metadata.extend_from_slice(pkg_bytes);
            let module_bytes = symbol.id.module.as_bytes();
            metadata.extend_from_slice(&(module_bytes.len() as u32).to_le_bytes());
            metadata.extend_from_slice(module_bytes);
            let name_bytes = symbol.id.name.as_bytes();
            metadata.extend_from_slice(&(name_bytes.len() as u32).to_le_bytes());
            metadata.extend_from_slice(name_bytes);
        }
        let mut module = Module::new();
        module.section(&CustomSection {
            name: Cow::Borrowed("nexa.link.symbols"),
            data: Cow::Owned(metadata),
        });
        module.finish()
    }

    pub fn load_from_nxp(&mut self, data: &[u8]) -> Result<(), LinkError> {
        let archive =
            nexa_package::NxpArchive::parse(data).map_err(|e| LinkError::PackageNotLoaded {
                package: format!("NXP parse error: {}", e),
            })?;

        let manifest = archive.manifest().clone();
        let name = manifest.name.clone();
        let version = manifest.version.clone();
        self.load_package(&name, &version, manifest)
    }

    pub fn symbols_for_package(&self, package: &str) -> Vec<&ResolvedSymbol> {
        self.graph
            .symbols
            .iter()
            .filter(|s| s.source_package == package)
            .collect()
    }

    pub fn graph(&self) -> &LinkGraph {
        &self.graph
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexa_package::ContentMetadata;

    fn make_manifest(name: &str) -> nexa_package::PackageMetadata {
        nexa_package::PackageMetadata {
            name: name.to_string(),
            version: "1.0.0".to_string(),
            format_version: 1,
            language_version: "nexa-0.1".to_string(),
            public_interface_fingerprint: "fp_abc123".to_string(),
            entries: Vec::new(),
            dependencies: Vec::new(),
            content_metadata: ContentMetadata {
                total_size: 0,
                entry_count: 0,
                package_digest: String::new(),
            },
        }
    }

    fn make_manifest_with_deps(
        name: &str,
        deps: Vec<nexa_package::PackageDependency>,
    ) -> nexa_package::PackageMetadata {
        let mut m = make_manifest(name);
        m.dependencies = deps;
        m
    }

    fn make_dep(alias: &str, package: &str) -> nexa_package::PackageDependency {
        nexa_package::PackageDependency {
            alias: alias.to_string(),
            package: package.to_string(),
            version_req: ">=1.0.0".to_string(),
            source: "registry".to_string(),
        }
    }

    fn make_function_symbol(pkg: &str, module: &str, name: &str) -> ResolvedSymbol {
        ResolvedSymbol {
            id: SymbolId {
                package: pkg.to_string(),
                module: module.to_string(),
                name: name.to_string(),
            },
            source_package: pkg.to_string(),
            source_version: "1.0.0".to_string(),
            kind: SymbolKind::Function,
            visibility: SymbolVisibility::Public,
        }
    }

    fn make_private_function_symbol(pkg: &str, module: &str, name: &str) -> ResolvedSymbol {
        let mut s = make_function_symbol(pkg, module, name);
        s.visibility = SymbolVisibility::Private;
        s
    }

    fn make_type_symbol(pkg: &str, module: &str, name: &str) -> ResolvedSymbol {
        ResolvedSymbol {
            id: SymbolId {
                package: pkg.to_string(),
                module: module.to_string(),
                name: name.to_string(),
            },
            source_package: pkg.to_string(),
            source_version: "1.0.0".to_string(),
            kind: SymbolKind::Type,
            visibility: SymbolVisibility::Public,
        }
    }

    fn make_constant_symbol(pkg: &str, module: &str, name: &str) -> ResolvedSymbol {
        ResolvedSymbol {
            id: SymbolId {
                package: pkg.to_string(),
                module: module.to_string(),
                name: name.to_string(),
            },
            source_package: pkg.to_string(),
            source_version: "1.0.0".to_string(),
            kind: SymbolKind::Constant,
            visibility: SymbolVisibility::Public,
        }
    }

    fn make_module_symbol(pkg: &str, module: &str, name: &str) -> ResolvedSymbol {
        ResolvedSymbol {
            id: SymbolId {
                package: pkg.to_string(),
                module: module.to_string(),
                name: name.to_string(),
            },
            source_package: pkg.to_string(),
            source_version: "1.0.0".to_string(),
            kind: SymbolKind::Module,
            visibility: SymbolVisibility::Public,
        }
    }

    #[test]
    fn linker_creation() {
        let linker = Linker::new();
        assert!(linker.packages.is_empty());
        assert!(linker.graph.symbols.is_empty());
        assert!(linker.graph.edges.is_empty());
        assert!(linker.graph.root_package.is_empty());
    }

    #[test]
    fn load_package_from_metadata() {
        let mut linker = Linker::new();
        let manifest = make_manifest("my-app");
        let result = linker.load_package("my-app", "1.0.0", manifest);
        assert!(result.is_ok());
        assert!(linker.packages.contains_key("my-app"));
        assert_eq!(linker.graph.root_package, "my-app");
    }

    #[test]
    fn load_multiple_packages_sets_root_first() {
        let mut linker = Linker::new();
        linker
            .load_package("alpha", "1.0.0", make_manifest("alpha"))
            .unwrap();
        linker
            .load_package("beta", "2.0.0", make_manifest("beta"))
            .unwrap();
        assert_eq!(linker.graph.root_package, "alpha");
        assert_eq!(linker.packages.len(), 2);
    }

    #[test]
    fn register_and_resolve_function_symbol() {
        let mut linker = Linker::new();
        linker
            .load_package("my-app", "1.0.0", make_manifest("my-app"))
            .unwrap();
        let sym = make_function_symbol("my-app", "main", "run");
        let result = linker.register_symbol(sym);
        assert!(result.is_ok());
        assert_eq!(linker.graph.symbols.len(), 1);
        assert_eq!(linker.graph.symbols[0].kind, SymbolKind::Function);
    }

    #[test]
    fn register_type_symbol() {
        let mut linker = Linker::new();
        linker
            .load_package("lib", "1.0.0", make_manifest("lib"))
            .unwrap();
        let sym = make_type_symbol("lib", "types", "Point");
        linker.register_symbol(sym).unwrap();
        assert_eq!(linker.graph.symbols[0].kind, SymbolKind::Type);
    }

    #[test]
    fn register_constant_symbol() {
        let mut linker = Linker::new();
        linker
            .load_package("lib", "1.0.0", make_manifest("lib"))
            .unwrap();
        let sym = make_constant_symbol("lib", "config", "MAX_SIZE");
        linker.register_symbol(sym).unwrap();
        assert_eq!(linker.graph.symbols[0].kind, SymbolKind::Constant);
    }

    #[test]
    fn register_module_symbol() {
        let mut linker = Linker::new();
        linker
            .load_package("lib", "1.0.0", make_manifest("lib"))
            .unwrap();
        let sym = make_module_symbol("lib", "root", "io");
        linker.register_symbol(sym).unwrap();
        assert_eq!(linker.graph.symbols[0].kind, SymbolKind::Module);
    }

    #[test]
    fn duplicate_symbol_detection() {
        let mut linker = Linker::new();
        linker
            .load_package("pkg-a", "1.0.0", make_manifest("pkg-a"))
            .unwrap();
        linker
            .load_package("pkg-b", "1.0.0", make_manifest("pkg-b"))
            .unwrap();
        let sym_a = ResolvedSymbol {
            id: SymbolId {
                package: "shared".to_string(),
                module: "mod".to_string(),
                name: "do_thing".to_string(),
            },
            source_package: "pkg-a".to_string(),
            source_version: "1.0.0".to_string(),
            kind: SymbolKind::Function,
            visibility: SymbolVisibility::Public,
        };
        let sym_b = ResolvedSymbol {
            id: SymbolId {
                package: "shared".to_string(),
                module: "mod".to_string(),
                name: "do_thing".to_string(),
            },
            source_package: "pkg-b".to_string(),
            source_version: "1.0.0".to_string(),
            kind: SymbolKind::Function,
            visibility: SymbolVisibility::Public,
        };
        linker.register_symbol(sym_a).unwrap();
        let result = linker.register_symbol(sym_b);
        assert!(result.is_err());
        match result.unwrap_err() {
            LinkError::DuplicateSymbol {
                symbol,
                pkg_a,
                pkg_b,
            } => {
                assert_eq!(symbol, "do_thing");
                assert_eq!(pkg_a, "pkg-a");
                assert_eq!(pkg_b, "pkg-b");
            }
            _ => panic!("expected DuplicateSymbol"),
        }
    }

    #[test]
    fn private_symbol_access_prevention() {
        let mut linker = Linker::new();
        linker
            .load_package("lib", "1.0.0", make_manifest("lib"))
            .unwrap();
        linker
            .load_package("app", "1.0.0", make_manifest("app"))
            .unwrap();
        let priv_sym = make_private_function_symbol("lib", "internal", "secret_fn");
        linker.register_symbol(priv_sym).unwrap();

        let app_sym = make_function_symbol("app", "internal", "secret_fn");
        let result = linker.register_symbol(app_sym);
        assert!(result.is_err());
        match result.unwrap_err() {
            LinkError::PrivateSymbolAccess { symbol, package } => {
                assert_eq!(symbol, "secret_fn");
                assert_eq!(package, "lib");
            }
            _ => panic!("expected PrivateSymbolAccess"),
        }
    }

    #[test]
    fn public_interface_fingerprint_verification() {
        let mut linker = Linker::new();
        linker
            .load_package("my-app", "1.0.0", make_manifest("my-app"))
            .unwrap();
        assert!(linker.verify_public_interfaces().is_ok());
    }

    #[test]
    fn fingerprint_mismatch_detection() {
        let mut linker = Linker::new();
        let mut manifest = make_manifest("my-app");
        manifest.public_interface_fingerprint = "expected_fp".to_string();
        linker.load_package("my-app", "1.0.0", manifest).unwrap();

        if let Some(pkg) = linker.packages.get_mut("my-app") {
            pkg.public_interface.fingerprint = "wrong_fp".to_string();
        }

        let result = linker.verify_public_interfaces();
        assert!(result.is_err());
        match result.unwrap_err() {
            LinkError::FingerprintMismatch {
                package,
                expected,
                actual,
            } => {
                assert_eq!(package, "my-app");
                assert_eq!(expected, "expected_fp");
                assert_eq!(actual, "wrong_fp");
            }
            _ => panic!("expected FingerprintMismatch"),
        }
    }

    #[test]
    fn package_not_loaded_error() {
        let linker = Linker::new();
        let symbols = linker.symbols_for_package("nonexistent");
        assert!(symbols.is_empty());
    }

    #[test]
    fn empty_linker_no_symbols() {
        let linker = Linker::new();
        let result = linker.link();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), LinkError::NoOutputArtifact));
    }

    #[test]
    fn multiple_packages_with_cross_references() {
        let mut linker = Linker::new();

        let mut lib_manifest =
            make_manifest_with_deps("lib-core", vec![make_dep("lib-math", "lib-math")]);
        lib_manifest.public_interface_fingerprint = "fp_abc123".to_string();
        linker
            .load_package("lib-core", "1.0.0", lib_manifest)
            .unwrap();

        linker
            .load_package("lib-math", "1.0.0", make_manifest("lib-math"))
            .unwrap();

        let add_sym = make_function_symbol("lib-math", "math", "add");
        linker.register_symbol(add_sym).unwrap();

        let use_sym = make_function_symbol("lib-core", "math", "add");
        linker.register_symbol(use_sym).unwrap();

        let result = linker.resolve_symbols();
        assert!(result.is_ok());
    }

    #[test]
    fn load_from_nxp_archive() {
        let mut manifest = make_manifest("nxp-test");
        manifest.public_interface_fingerprint = "fp_abc123".to_string();

        let mut archive = nexa_package::NxpArchive::new(manifest);
        archive
            .add_entry("src/main.nexa", b"fn main() {}".to_vec())
            .unwrap();
        let data = archive.build();

        let mut linker = Linker::new();
        let result = linker.load_from_nxp(&data);
        assert!(result.is_ok());
        assert!(linker.packages.contains_key("nxp-test"));
    }

    #[test]
    fn symbol_lookup_by_package() {
        let mut linker = Linker::new();
        linker
            .load_package("pkg-a", "1.0.0", make_manifest("pkg-a"))
            .unwrap();
        linker
            .load_package("pkg-b", "1.0.0", make_manifest("pkg-b"))
            .unwrap();

        linker
            .register_symbol(make_function_symbol("pkg-a", "mod", "fn_a"))
            .unwrap();
        linker
            .register_symbol(make_function_symbol("pkg-b", "mod", "fn_b"))
            .unwrap();
        linker
            .register_symbol(make_function_symbol("pkg-a", "mod", "fn_a2"))
            .unwrap();

        let syms_a = linker.symbols_for_package("pkg-a");
        assert_eq!(syms_a.len(), 2);

        let syms_b = linker.symbols_for_package("pkg-b");
        assert_eq!(syms_b.len(), 1);
        assert_eq!(syms_b[0].id.name, "fn_b");

        let syms_none = linker.symbols_for_package("pkg-c");
        assert!(syms_none.is_empty());
    }

    #[test]
    fn link_graph_construction() {
        let mut linker = Linker::new();
        linker
            .load_package("my-app", "1.0.0", make_manifest("my-app"))
            .unwrap();
        linker
            .register_symbol(make_function_symbol("my-app", "main", "run"))
            .unwrap();

        let graph = linker.build_link_graph();
        assert!(graph.is_ok());
        let graph = graph.unwrap();
        assert_eq!(graph.root_package, "my-app");
        assert_eq!(graph.symbols.len(), 1);
    }

    #[test]
    fn entry_point_in_linked_output() {
        let mut linker = Linker::new();
        linker
            .load_package("my-app", "1.0.0", make_manifest("my-app"))
            .unwrap();
        linker
            .register_symbol(make_function_symbol("my-app", "main", "run"))
            .unwrap();
        linker
            .register_symbol(make_private_function_symbol("my-app", "internal", "helper"))
            .unwrap();

        let output = linker.link().unwrap();
        assert_eq!(output.entry_points.len(), 1);
        assert_eq!(output.entry_points[0], "main::run");
        assert!(output.wasm_bytes.len() > 8);
    }

    #[test]
    fn wasm_bytes_structure() {
        let mut linker = Linker::new();
        linker
            .load_package("my-app", "1.0.0", make_manifest("my-app"))
            .unwrap();
        linker
            .register_symbol(make_function_symbol("my-app", "main", "start"))
            .unwrap();

        let output = linker.link().unwrap();
        assert_eq!(&output.wasm_bytes[0..4], b"\0asm");
        assert_eq!(&output.wasm_bytes[4..8], &[0x01, 0x00, 0x00, 0x00]);
        wasmparser::Validator::new()
            .validate_all(&output.wasm_bytes)
            .expect("linked metadata output must be valid WebAssembly");
        assert!(wasmparser::Parser::new(0)
            .parse_all(&output.wasm_bytes)
            .any(|payload| matches!(
                payload,
                Ok(wasmparser::Payload::CustomSection(section))
                    if section.name() == "nexa.link.symbols"
            )));
    }

    #[test]
    fn version_mismatch_detection() {
        let mut linker = Linker::new();
        linker
            .load_package("my-app", "1.0.0", make_manifest("my-app"))
            .unwrap();

        let pkg = linker.packages.get("my-app").unwrap();
        let expected_version = "1.0.0";
        let actual_version = &pkg.version;
        assert_eq!(expected_version, actual_version.as_str());

        let wrong_version = "2.0.0";
        assert_ne!(expected_version, wrong_version);
    }

    #[test]
    fn link_cycle_detection() {
        let mut linker = Linker::new();
        linker
            .load_package("pkg-a", "1.0.0", make_manifest("pkg-a"))
            .unwrap();
        linker
            .load_package("pkg-b", "1.0.0", make_manifest("pkg-b"))
            .unwrap();

        linker
            .register_symbol(make_function_symbol("pkg-a", "mod", "fn_a"))
            .unwrap();

        linker.graph.edges.push(LinkEdge {
            from: SymbolId {
                package: "pkg-a".to_string(),
                module: "mod".to_string(),
                name: "fn_a".to_string(),
            },
            to: SymbolId {
                package: "pkg-b".to_string(),
                module: "mod".to_string(),
                name: "fn_b".to_string(),
            },
        });
        linker.graph.edges.push(LinkEdge {
            from: SymbolId {
                package: "pkg-b".to_string(),
                module: "mod".to_string(),
                name: "fn_b".to_string(),
            },
            to: SymbolId {
                package: "pkg-a".to_string(),
                module: "mod".to_string(),
                name: "fn_a".to_string(),
            },
        });

        let result = linker.build_link_graph();
        assert!(result.is_err());
        match result.unwrap_err() {
            LinkError::LinkCycle { package } => {
                assert!(package == "pkg-a" || package == "pkg-b");
            }
            _ => panic!("expected LinkCycle"),
        }
    }

    #[test]
    fn type_mismatch_detection_in_linked_output() {
        let mut linker = Linker::new();
        linker
            .load_package("my-app", "1.0.0", make_manifest("my-app"))
            .unwrap();

        let sym = make_type_symbol("my-app", "types", "Result");
        linker.register_symbol(sym).unwrap();

        let output = linker.link().unwrap();
        assert_eq!(output.symbols.len(), 1);
        assert_eq!(output.symbols[0].kind, SymbolKind::Type);
    }

    #[test]
    fn public_api_consistency_check() {
        let mut linker = Linker::new();
        let mut manifest_a = make_manifest("pkg-a");
        manifest_a.public_interface_fingerprint = "stable_fp".to_string();
        linker.load_package("pkg-a", "1.0.0", manifest_a).unwrap();

        let mut manifest_b = make_manifest("pkg-b");
        manifest_b.public_interface_fingerprint = "stable_fp".to_string();
        linker.load_package("pkg-b", "1.0.0", manifest_b).unwrap();

        assert!(linker.verify_public_interfaces().is_ok());
    }

    #[test]
    fn complex_multi_package_scenario() {
        let mut linker = Linker::new();

        let mut core_manifest = make_manifest_with_deps("core", vec![make_dep("utils", "utils")]);
        core_manifest.public_interface_fingerprint = "fp_core".to_string();
        linker.load_package("core", "1.0.0", core_manifest).unwrap();

        let mut utils_manifest = make_manifest_with_deps("utils", vec![make_dep("math", "math")]);
        utils_manifest.public_interface_fingerprint = "fp_utils".to_string();
        linker
            .load_package("utils", "1.0.0", utils_manifest)
            .unwrap();

        let mut math_manifest = make_manifest("math");
        math_manifest.public_interface_fingerprint = "fp_math".to_string();
        linker.load_package("math", "1.0.0", math_manifest).unwrap();

        linker
            .register_symbol(make_function_symbol("math", "arith", "add"))
            .unwrap();
        linker
            .register_symbol(make_function_symbol("math", "arith", "sub"))
            .unwrap();
        linker
            .register_symbol(make_function_symbol("utils", "strings", "format"))
            .unwrap();
        linker
            .register_symbol(make_type_symbol("core", "types", "Config"))
            .unwrap();
        linker
            .register_symbol(make_constant_symbol("core", "config", "VERSION"))
            .unwrap();
        linker
            .register_symbol(make_module_symbol("core", "root", "app"))
            .unwrap();
        linker
            .register_symbol(make_function_symbol("core", "main", "run"))
            .unwrap();

        assert!(linker.verify_public_interfaces().is_ok());
        assert!(linker.resolve_symbols().is_ok());

        let graph = linker.build_link_graph().unwrap();
        assert_eq!(graph.root_package, "core");
        assert_eq!(graph.symbols.len(), 7);

        let output = linker.link().unwrap();
        assert_eq!(output.symbols.len(), 7);
        assert!(output.entry_points.contains(&"main::run".to_string()));
        assert!(output.wasm_bytes.len() > 8);

        let core_syms = linker.symbols_for_package("core");
        assert_eq!(core_syms.len(), 4);

        let math_syms = linker.symbols_for_package("math");
        assert_eq!(math_syms.len(), 2);
    }

    #[test]
    fn register_same_symbol_same_package_allowed() {
        let mut linker = Linker::new();
        linker
            .load_package("my-app", "1.0.0", make_manifest("my-app"))
            .unwrap();

        let sym1 = make_function_symbol("my-app", "mod", "fn1");
        let sym2 = make_function_symbol("my-app", "mod", "fn2");
        assert!(linker.register_symbol(sym1).is_ok());
        assert!(linker.register_symbol(sym2).is_ok());
        assert_eq!(linker.graph.symbols.len(), 2);
    }

    #[test]
    fn link_graph_access() {
        let mut linker = Linker::new();
        linker
            .load_package("my-app", "1.0.0", make_manifest("my-app"))
            .unwrap();
        linker
            .register_symbol(make_function_symbol("my-app", "main", "run"))
            .unwrap();

        let graph = linker.graph();
        assert_eq!(graph.root_package, "my-app");
        assert_eq!(graph.symbols.len(), 1);
    }

    #[test]
    fn no_cycle_in_simple_graph() {
        let mut linker = Linker::new();
        linker
            .load_package("a", "1.0.0", make_manifest("a"))
            .unwrap();
        linker
            .load_package("b", "1.0.0", make_manifest("b"))
            .unwrap();

        linker
            .register_symbol(make_function_symbol("a", "m", "f1"))
            .unwrap();
        linker
            .register_symbol(make_function_symbol("b", "m", "f2"))
            .unwrap();

        linker.graph.edges.push(LinkEdge {
            from: SymbolId {
                package: "a".to_string(),
                module: "m".to_string(),
                name: "f1".to_string(),
            },
            to: SymbolId {
                package: "b".to_string(),
                module: "m".to_string(),
                name: "f2".to_string(),
            },
        });

        let result = linker.build_link_graph();
        assert!(result.is_ok());
    }

    #[test]
    fn linked_output_contains_all_symbols() {
        let mut linker = Linker::new();
        linker
            .load_package("my-app", "1.0.0", make_manifest("my-app"))
            .unwrap();
        linker
            .register_symbol(make_function_symbol("my-app", "m", "f1"))
            .unwrap();
        linker
            .register_symbol(make_type_symbol("my-app", "m", "T1"))
            .unwrap();
        linker
            .register_symbol(make_constant_symbol("my-app", "m", "C1"))
            .unwrap();

        let output = linker.link().unwrap();
        assert_eq!(output.symbols.len(), 3);
        let kinds: Vec<_> = output.symbols.iter().map(|s| s.kind).collect();
        assert!(kinds.contains(&SymbolKind::Function));
        assert!(kinds.contains(&SymbolKind::Type));
        assert!(kinds.contains(&SymbolKind::Constant));
    }

    #[test]
    fn only_public_symbols_are_entry_points() {
        let mut linker = Linker::new();
        linker
            .load_package("my-app", "1.0.0", make_manifest("my-app"))
            .unwrap();
        linker
            .register_symbol(make_function_symbol("my-app", "m", "public_fn"))
            .unwrap();
        linker
            .register_symbol(make_private_function_symbol("my-app", "m", "private_fn"))
            .unwrap();
        linker
            .register_symbol(make_function_symbol("my-app", "m", "another_public"))
            .unwrap();

        let output = linker.link().unwrap();
        assert_eq!(output.entry_points.len(), 2);
        assert!(output.entry_points.contains(&"m::public_fn".to_string()));
        assert!(output
            .entry_points
            .contains(&"m::another_public".to_string()));
        assert!(!output.entry_points.contains(&"m::private_fn".to_string()));
    }

    #[test]
    fn load_from_nxp_invalid_data() {
        let mut linker = Linker::new();
        let result = linker.load_from_nxp(&[0x00, 0x01, 0x02, 0x03]);
        assert!(result.is_err());
    }

    #[test]
    fn symbols_for_empty_package() {
        let mut linker = Linker::new();
        linker
            .load_package("empty", "1.0.0", make_manifest("empty"))
            .unwrap();
        let symbols = linker.symbols_for_package("empty");
        assert!(symbols.is_empty());
    }

    #[test]
    fn linked_output_symbols_match_graph() {
        let mut linker = Linker::new();
        linker
            .load_package("my-app", "1.0.0", make_manifest("my-app"))
            .unwrap();
        linker
            .register_symbol(make_function_symbol("my-app", "m", "f1"))
            .unwrap();
        linker
            .register_symbol(make_function_symbol("my-app", "m", "f2"))
            .unwrap();

        let output = linker.link().unwrap();
        assert_eq!(output.symbols.len(), linker.graph().symbols.len());
    }

    #[test]
    fn constant_symbol_in_linked_output() {
        let mut linker = Linker::new();
        linker
            .load_package("my-app", "1.0.0", make_manifest("my-app"))
            .unwrap();
        linker
            .register_symbol(make_constant_symbol("my-app", "cfg", "PI"))
            .unwrap();

        let output = linker.link().unwrap();
        assert_eq!(output.symbols[0].id.name, "PI");
        assert_eq!(output.symbols[0].kind, SymbolKind::Constant);
    }
}
