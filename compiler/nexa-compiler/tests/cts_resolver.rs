//! CTS-RESOLVE — suíte de conformidade do resolver (Implementação 03 §466-472).
//!
//! Três formas de caso, todas resolvidas pelas fronteiras públicas reais:
//!
//! 1. `cts/resolver/positive/CTS-RESOLVE-XXXX.nexa` — fonte única (single-module
//!    mode, §459) com envelope golden de `resolve_output_json`. Positivos (00XX)
//!    com zero erros (parse + semânticos).
//! 2. `cts/resolver/negative/CTS-RESOLVE-XXXX.nexa` — idem; negativos (01XX)
//!    com ≥1 erro semântico.
//! 3. `cts/resolver/{modules,visibility}/CTS-RESOLVE-XXXX/` — caso multi-módulo
//!    com descriptor `case.json` (topologia de packages/módulos + aliases de
//!    dependência) resolvido via harness de biblioteca `resolve_project`
//!    (§460-462, §795). Golden = envelope de todo o projeto.
//!
//! O runner regera os goldens com `NEXA_CTS_BLESS=1`.

use std::path::{Path, PathBuf};

use nexa_compiler::{resolve_output_json, symbol_kind_label, Pipeline, SCHEMA_VERSION};
use nexa_diagnostics::Severity;
use nexa_parser::{parse, ParseMode};
use nexa_project::{
    DependencyAlias, ModulePath, PackageInstance, ParsedModule, ParsedPackage, ParsedProject,
    ResolverConfig,
};
use nexa_resolver::reference_index::ReferenceKind;
use nexa_resolver::resolver::{
    resolve_project, DiagnosticSeverity, ResolveDiagnostic, ResolveResult,
};
use nexa_resolver::SemanticIndex;
use nexa_source::SourceManager;
use nexa_symbols::PackageInstanceId;

fn cts_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cts/resolver")
}

fn case_id(stem: &str) -> u32 {
    stem["CTS-RESOLVE-".len()..]
        .parse()
        .unwrap_or_else(|_| panic!("[{stem}] case id inválido"))
}

fn semantic_error_count(result: &ResolveResult) -> usize {
    result
        .diagnostics
        .iter()
        .filter(|d| d.severity == DiagnosticSeverity::Error)
        .count()
}

// ---------------------------------------------------------------------------
// 1 + 2. Casos single-module (positive/negative).
// ---------------------------------------------------------------------------

fn discover_single_cases() -> Vec<PathBuf> {
    let root = cts_dir();
    let mut cases: Vec<PathBuf> = Vec::new();
    for sub in ["positive", "negative"] {
        let dir = root.join(sub);
        let mut found: Vec<PathBuf> = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("cannot read {dir:?}: {e}"))
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.extension().is_some_and(|x| x == "nexa")
                    && p.file_stem()
                        .is_some_and(|s| s.to_string_lossy().starts_with("CTS-RESOLVE-"))
            })
            .collect();
        cases.append(&mut found);
    }
    cases.sort();
    cases
}

/// Roda um caso single-module pela fronteira real `Pipeline::resolve_bytes`.
fn run_single_case(input_path: &Path) -> (String, nexa_compiler::SemanticResult, Pipeline) {
    let stem = input_path
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let bytes =
        std::fs::read(input_path).unwrap_or_else(|e| panic!("[{stem}] cannot read input: {e}"));
    // Display path determinístico (só o nome do arquivo) para goldens estáveis.
    let display = PathBuf::from(format!("{stem}.nexa"));
    let mut pipeline = Pipeline::new();
    let result = pipeline
        .resolve_bytes(&display, bytes)
        .unwrap_or_else(|d| panic!("[{stem}] fronteira retornou diagnóstico: {:?}", d.message));
    (stem, result, pipeline)
}

#[test]
fn cts_resolve_bless_single_goldens() {
    if std::env::var_os("NEXA_CTS_BLESS").is_none() {
        return;
    }
    for input_path in discover_single_cases() {
        let (stem, result, pipeline) = run_single_case(&input_path);
        let json = serde_json::to_string_pretty(&resolve_output_json(&result, &pipeline.sources))
            .unwrap_or_else(|e| panic!("[{stem}] serialization failed: {e}"));
        let golden_path = cts_dir().join("golden").join(format!("{stem}.json"));
        std::fs::write(&golden_path, json)
            .unwrap_or_else(|e| panic!("[{stem}] cannot write golden {golden_path:?}: {e}"));
    }
}

#[test]
fn cts_resolve_suite_present_and_reproducable() {
    let cases = discover_single_cases();
    assert!(
        cases.len() >= 22,
        "expected at least the resolver positive+negative single-module cases, found {}",
        cases.len()
    );

    for input_path in &cases {
        let (stem, result, pipeline) = run_single_case(input_path);
        let id = case_id(&stem);
        let positive = id < 100;

        let golden_path = cts_dir().join("golden").join(format!("{stem}.json"));
        let golden_text = std::fs::read_to_string(&golden_path)
            .unwrap_or_else(|e| panic!("[{stem}] cannot read golden {golden_path:?}: {e}"));

        let actual = serde_json::to_value(resolve_output_json(&result, &pipeline.sources))
            .unwrap_or_else(|e| panic!("[{stem}] serialization failed: {e}"));
        let expected: serde_json::Value = serde_json::from_str(&golden_text)
            .unwrap_or_else(|e| panic!("[{stem}] golden é JSON inválido: {e}"));
        assert_eq!(
            expected, actual,
            "CTS-RESOLVE caso {stem}: envelope divergiu do golden"
        );

        if positive {
            assert!(
                !result.has_errors(),
                "[{stem}] caso positivo não pode ter erros"
            );
        } else {
            let n = result
                .semantic_diagnostics
                .iter()
                .filter(|d| d.severity == DiagnosticSeverity::Error)
                .count();
            assert!(
                n >= 1,
                "[{stem}] caso negativo precisa de ≥1 erro semântico"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 3. Casos multi-module (modules/visibility) via resolve_project.
// ---------------------------------------------------------------------------

fn discover_multi_cases() -> Vec<PathBuf> {
    let root = cts_dir();
    let mut cases: Vec<PathBuf> = Vec::new();
    for sub in ["modules", "visibility"] {
        let dir = root.join(sub);
        for entry in std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("cannot read {dir:?}: {e}"))
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_dir()
                && entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("CTS-RESOLVE-")
            {
                cases.push(path);
            }
        }
    }
    cases.sort();
    cases
}

/// Descriptor interno do harness (§795): topologia do projeto.
struct CaseProject {
    current_package: String,
    aliases: Vec<(String, String)>,
    packages: Vec<Pkg>,
}

struct Pkg {
    name: String,
    modules: Vec<Mod>,
}

struct Mod {
    file: String,
    path: String,
    public: bool,
}

fn load_descriptor(case_dir: &Path) -> CaseProject {
    let path = case_dir.join("case.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read descriptor {path:?}: {e}"));
    let v: serde_json::Value = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("descriptor {path:?} é JSON inválido: {e}"));
    let current = v["currentPackage"]
        .as_str()
        .expect("currentPackage")
        .to_string();
    let mut aliases = Vec::new();
    if let Some(a) = v["dependencyAliases"].as_array() {
        for e in a {
            aliases.push((
                e["alias"].as_str().expect("alias").to_string(),
                e["package"].as_str().expect("package").to_string(),
            ));
        }
    }
    let mut packages = Vec::new();
    for p in v["packages"].as_array().expect("packages") {
        let mut mods = Vec::new();
        for m in p["modules"].as_array().expect("modules") {
            mods.push(Mod {
                file: m["file"].as_str().expect("file").to_string(),
                path: m["path"].as_str().expect("path").to_string(),
                public: m["public"].as_bool().unwrap_or(true),
            });
        }
        packages.push(Pkg {
            name: p["name"].as_str().expect("name").to_string(),
            modules: mods,
        });
    }
    CaseProject {
        current_package: current,
        aliases,
        packages,
    }
}

/// Resolve um caso multi-module pela API de biblioteca (§462).
fn run_multi_case(case_dir: &Path) -> (String, ResolveResult) {
    let stem = case_dir.file_name().unwrap().to_string_lossy().to_string();
    let desc = load_descriptor(case_dir);

    // Atribuição determinística de PackageInstanceId: current = 0; demais por nome.
    let mut names: Vec<&str> = desc.packages.iter().map(|p| p.name.as_str()).collect();
    names.sort_unstable();
    let mut order: Vec<String> = Vec::new();
    order.push(desc.current_package.clone());
    for n in names {
        if n != desc.current_package && !order.iter().any(|x| x == n) {
            order.push(n.to_string());
        }
    }
    let mut id_of: std::collections::HashMap<String, PackageInstanceId> =
        std::collections::HashMap::new();
    for (i, name) in order.iter().enumerate() {
        id_of.insert(name.clone(), PackageInstanceId(i as u32));
    }

    // Carrega fontes e parseia cada módulo (SingleFile), em ordem determinística
    // (packages por id; módulos por path).
    let mut sources = SourceManager::new();
    let mut pkg_order: Vec<usize> = (0..desc.packages.len()).collect();
    pkg_order.sort_by_key(|&pi| id_of[&desc.packages[pi].name].0);

    let mut parsed_modules: Vec<ParsedModule> = Vec::new();
    for pi in pkg_order {
        let pkg = &desc.packages[pi];
        let mut mods: Vec<&Mod> = pkg.modules.iter().collect();
        mods.sort_by(|a, b| a.path.cmp(&b.path));
        for m in mods {
            let bytes = std::fs::read(case_dir.join(&m.file))
                .unwrap_or_else(|e| panic!("[{stem}] cannot read {}: {e}", m.file));
            let source_id = sources
                .load_bytes(PathBuf::from(m.file.clone()), bytes)
                .unwrap_or_else(|e| panic!("[{stem}] load_bytes failed for {}: {:?}", m.file, e));
            let source = sources.source(source_id).expect("just loaded source");
            let parse_result = parse(source, ParseMode::SingleFile);
            for d in &parse_result.diagnostics {
                assert_eq!(
                    d.severity,
                    Severity::Warning,
                    "[{stem}] módulo {} não pode ter erros de parse: {}",
                    m.file,
                    d.message
                );
            }
            parsed_modules.push(ParsedModule {
                package: id_of[&pkg.name],
                module_path: ModulePath::new(m.path.split("::").map(|s| s.to_string()).collect()),
                source_id,
                ast: parse_result.ast,
                is_public: m.public,
            });
        }
    }

    let mut packages: Vec<ParsedPackage> = Vec::new();
    for pkg in &desc.packages {
        let id = id_of[&pkg.name];
        let modules = parsed_modules
            .iter()
            .filter(|m| m.package == id)
            .cloned()
            .collect();
        packages.push(ParsedPackage {
            instance: PackageInstance::new(id, pkg.name.clone()),
            modules,
        });
    }
    let project = ParsedProject {
        current_package: id_of[&desc.current_package],
        packages,
    };
    let config = ResolverConfig {
        dependency_aliases: desc
            .aliases
            .iter()
            .map(|(alias, pkg)| DependencyAlias {
                alias: alias.clone(),
                package: id_of[pkg],
            })
            .collect(),
    };
    let result = resolve_project(&project, &config);
    (stem, result)
}

fn diagnostic_value(d: &ResolveDiagnostic) -> serde_json::Value {
    serde_json::json!({
        "code": d.code,
        "severity": match d.severity {
            DiagnosticSeverity::Error => "error",
            DiagnosticSeverity::Warning => "warning",
            DiagnosticSeverity::Note => "note",
        },
        "message": d.message,
        "span": {
            "source": d.span.source.0,
            "start": d.span.start,
            "end": d.span.end,
        },
        "context": d.context,
    })
}

fn reference_kind_label(kind: ReferenceKind) -> &'static str {
    use ReferenceKind::*;
    match kind {
        Read => "Read",
        Write => "Write",
        Type => "Type",
        Call => "Call",
        Import => "Import",
    }
}

/// Envelope JSON de todo o projeto (debug tooling; mesmo shape por elemento de
/// `resolve_output_json`, §463-465).
fn project_envelope(index: &SemanticIndex, diagnostics: &[ResolveDiagnostic]) -> serde_json::Value {
    let mut modules = Vec::new();
    for mid in 0..index.modules.count() {
        if let Some(m) = index.modules.get(nexa_symbols::ModuleId(mid as u32)) {
            let sym = m.symbol.and_then(|s| index.symbols.get(s));
            modules.push(serde_json::json!({
                "id": mid,
                "name": m.name,
                "package": m.package.0,
                "path": m.path.display(),
                "public": m.is_public,
                "source": m.source_id.map(|s| s.0),
                "span": { "start": m.span.start, "end": m.span.end },
                "symbol": m.symbol.map(|s| s.0),
                "rootScope": m.root_scope.0,
                "symbolName": sym.map(|s| index.interner.resolve(s.name)),
            }));
        }
    }
    let mut symbols = Vec::new();
    for s in index.symbols.iter() {
        symbols.push(serde_json::json!({
            "id": s.id.0,
            "name": index.interner.resolve(s.name),
            "kind": symbol_kind_label(s.kind),
            "module": s.module.0,
            "scope": s.scope.0,
            "visibility": s.visibility.as_str(),
            "owner": s.owner.map(|o| o.0),
            "nameSpan": s.name_span.map(|sp| [sp.start, sp.end]),
            "bodyScope": s.body_scope.map(|b| b.0),
        }));
    }
    let mut references = Vec::new();
    for r in index.references.all() {
        let target = index
            .symbols
            .get(r.symbol)
            .map(|s| index.interner.resolve(s.name).to_string());
        references.push(serde_json::json!({
            "span": { "source": r.span.source.0, "start": r.span.start, "end": r.span.end },
            "symbol": r.symbol.0,
            "symbolName": target,
            "kind": reference_kind_label(r.kind),
            "scope": r.scope.0,
        }));
    }
    serde_json::json!({
        "schemaVersion": SCHEMA_VERSION,
        "semanticDiagnostics": diagnostics.iter().map(diagnostic_value).collect::<Vec<_>>(),
        "modules": modules,
        "symbols": symbols,
        "references": references,
    })
}

#[test]
fn cts_resolve_suite_multi_module() {
    let cases = discover_multi_cases();
    assert!(
        cases.len() >= 7,
        "expected at least the modules+visibility multi-module cases, found {}",
        cases.len()
    );

    for case_dir in &cases {
        let (stem, result) = run_multi_case(case_dir);
        let id = case_id(&stem);
        let positive = id < 100;

        let golden_path = cts_dir().join("golden").join(format!("{stem}.json"));
        if std::env::var_os("NEXA_CTS_BLESS").is_some() {
            let json =
                serde_json::to_string_pretty(&project_envelope(&result.index, &result.diagnostics))
                    .unwrap_or_else(|e| panic!("[{stem}] serialization failed: {e}"));
            std::fs::write(&golden_path, json)
                .unwrap_or_else(|e| panic!("[{stem}] cannot write golden {golden_path:?}: {e}"));
            continue;
        }

        let golden_text = std::fs::read_to_string(&golden_path)
            .unwrap_or_else(|e| panic!("[{stem}] cannot read golden {golden_path:?}: {e}"));
        let actual = project_envelope(&result.index, &result.diagnostics);
        let expected: serde_json::Value = serde_json::from_str(&golden_text)
            .unwrap_or_else(|e| panic!("[{stem}] golden é JSON inválido: {e}"));
        assert_eq!(
            expected, actual,
            "CTS-RESOLVE caso {stem}: envelope divergiu do golden"
        );

        let n = semantic_error_count(&result);
        if positive {
            assert_eq!(
                n, 0,
                "[{stem}] caso positivo multi-module não pode ter erros"
            );
        } else {
            assert!(
                n >= 1,
                "[{stem}] caso negativo multi-module precisa de ≥1 erro"
            );
        }
    }
}
