//! NEXA CLI — driver de linha de comando (Implementações 01-04).
//!
//! Subcomandos (contrato Impl 01 §26, §456-467; Impl 02 §477-478):
//! - `nexa version [--format human|json]` — versão do toolchain;
//! - `nexa lex <FILE> [--format human|json]` — roda o lexer e imprime o
//!   output lossless (tokens + trivia) + diagnostics;
//! - `nexa parse <FILE> [--format human|json]` — roda o parser e imprime o
//!   AST estrutural + diagnostics (exit 1 em erros de parser, §477);
//! - `nexa resolve <FILE> [--format human|json]` — parser + resolver e
//!   imprime o índice semântico (debug, Impl 03 §458-465);
//! - `nexa check <FILE> [--format human|json]` — parser + resolver + type
//!   checker e imprime o modelo tipado + diagnostics (Impl 04).
//!
//! Exit codes (Impl 01 §172-177, Impl 02 §477):
//! - `0` sucesso / `nexa version`;
//! - `1` erros lexicais/parser/semânticos/de tipo (diagnostics de severidade
//!   `error`);
//! - `2` erro de uso do CLI (opção/subcomando desconhecido, argumento faltante);
//! - `3` erro de entrada/tool (ex.: arquivo não pode ser lido);
//! - `101` ICE (reservado; propaganda na fronteira em fases futuras).

use std::path::PathBuf;
use std::process::ExitCode;

use nexa_compiler::{
    check_output_json, compile_source_to_wasm, human_kind, lex_output_json, location_str,
    parse_output_json, render_diagnostics, resolve_output_json, symbol_kind_label,
    type_severity_label, version, LexResult, LexemeKind, Pipeline, SemanticResult, TypeCheckResult,
    SCHEMA_VERSION,
};

const EXIT_OK: u8 = 0;
const EXIT_LEX_ERROR: u8 = 1;
const EXIT_USAGE: u8 = 2;
const EXIT_IO: u8 = 3;
const EXIT_ICE: u8 = 101;

const USAGE: &str = "\
nexa — NEXA reference compiler (Implementações 01-04)

USAGE:
  nexa version [--format human|json]
  nexa lex <FILE> [--format human|json]
  nexa parse <FILE> [--format human|json]
  nexa resolve <FILE> [--format human|json]
  nexa check <FILE> [--format human|json]
  nexa build <FILE> [-o OUTPUT]
  nexa run <FILE>
  nexa fmt <FILE> [--check]
  nexa verify <FILE.wasm>
  nexa inspect <FILE>
  nexa test <FILE> [--format human|json]
  nexa package resolve [MANIFEST] [--lock PATH]
  nexa restore [LOCKFILE]
  nexa help

COMMANDS:
  version   Print the reference toolchain version and language support profile.
  lex       Lex a source file and print the lossless token stream.
  parse     Parse a source file and print the structural AST + diagnostics.
  resolve   Parse+resolve a source file and print the semantic index (debug, Impl 03 §458-465).
  check     Parse+resolve+typecheck a source file and print the typed model + diagnostics (Impl 04).
  build     Compile a checked application through verified NIR to WebAssembly.
  run       Compile and execute an application through the WASM host boundary.
  fmt       Canonically format a source file, or verify it with --check.
  verify    Validate a wasm32-nexa artifact and its ABI boundary.
  inspect   Print deterministic source/artifact metadata.
  test      Discover and run @test items with deterministic providers.
  package   Resolve a project manifest and write a canonical lockfile.
  restore   Validate a lockfile and restore/verify local package sources.

EXIT CODES:
  0          success
  1          lexical/parser/semantic/type errors reported
  2          invalid CLI usage
  3          input/tool error (e.g. file not readable)
";

struct Options {
    format: String,
}

fn main() -> ExitCode {
    let code = match run() {
        Ok(code) => code,
        Err((code, msg)) => {
            eprintln!("nexa: error: {msg}");
            code
        }
    };
    ExitCode::from(code)
}

fn run() -> Result<u8, (u8, String)> {
    let mut args = std::env::args().skip(1);
    let Some(sub) = args.next() else {
        print!("{USAGE}");
        return Ok(EXIT_OK);
    };
    match sub.as_str() {
        "version" => cmd_version(args),
        "lex" => cmd_lex(args),
        "parse" => cmd_parse(args),
        "resolve" => cmd_resolve(args),
        "check" => cmd_check(args),
        "build" => cmd_build(args),
        "run" => cmd_run(args),
        "fmt" => cmd_fmt(args),
        "verify" => cmd_verify(args),
        "inspect" => cmd_inspect(args),
        "test" => cmd_test(args),
        "package" => cmd_package(args),
        "restore" => cmd_restore(args),
        "help" | "--help" | "-h" => {
            print!("{USAGE}");
            Ok(EXIT_OK)
        }
        other => Err((
            EXIT_USAGE,
            format!("unknown subcommand '{other}'\n\n{USAGE}"),
        )),
    }
}

fn cmd_package(mut args: impl Iterator<Item = String>) -> Result<u8, (u8, String)> {
    let Some(operation) = args.next() else {
        return Err(usage_error(
            "missing package operation (expected 'resolve')",
        ));
    };
    if operation != "resolve" {
        return Err(usage_error(format!(
            "unknown package operation '{operation}'"
        )));
    }
    let mut manifest = PathBuf::from("nexa.project");
    let mut lock_path = PathBuf::from("nexa.lock");
    let mut positional_seen = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--lock" => {
                lock_path = args
                    .next()
                    .map(PathBuf::from)
                    .ok_or_else(|| usage_error("--lock requires a path"))?;
            }
            _ if arg.starts_with('-') => {
                return Err(usage_error(format!(
                    "unknown package resolve argument '{arg}'"
                )));
            }
            _ if !positional_seen => {
                manifest = PathBuf::from(arg);
                positional_seen = true;
            }
            _ => return Err(usage_error("multiple manifest paths supplied")),
        }
    }

    let manifest_text = read_utf8_source(&manifest)?;
    let project = nexa_manifest::ProjectManifest::parse(&manifest_text)
        .map_err(|e| (EXIT_LEX_ERROR, format!("manifest parse failed: {e}")))?;
    project.validate().map_err(|errors| {
        (
            EXIT_LEX_ERROR,
            format!(
                "manifest validation failed: {}",
                errors
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        )
    })?;
    let mut resolver =
        nexa_resolver_pkg::Resolver::new(Box::new(nexa_registry::InMemoryRegistry::new()));
    let graph = resolver
        .resolve(&project)
        .map_err(|e| (EXIT_LEX_ERROR, format!("package resolution failed: {e}")))?;
    let mut lock = lock_from_graph(&graph, manifest_text.as_bytes())?;
    lock.sort_canonical();
    let json = lock
        .to_deterministic_json()
        .map_err(|e| (EXIT_ICE, format!("lockfile serialization failed: {e}")))?;
    std::fs::write(&lock_path, format!("{json}\n")).map_err(|e| {
        (
            EXIT_IO,
            format!("cannot write '{}': {e}", lock_path.display()),
        )
    })?;
    println!(
        "resolved {} package(s) into {}",
        graph.packages.len(),
        lock_path.display()
    );
    Ok(EXIT_OK)
}

fn lock_from_graph(
    graph: &nexa_resolver_pkg::ResolvedDependencyGraph,
    manifest_bytes: &[u8],
) -> Result<nexa_lock::LockFile, (u8, String)> {
    use nexa_lock::{
        LockedDependency, LockedDependencyKind, LockedPackage, LockedPackageId, LockedSource,
        ResolverIdentity,
    };
    let source = |source: &nexa_resolver_pkg::PackageSourceIdentity| match source {
        nexa_resolver_pkg::PackageSourceIdentity::Registry { registry_id } => {
            LockedSource::Registry {
                registry_id: registry_id.clone(),
            }
        }
        nexa_resolver_pkg::PackageSourceIdentity::Path { path } => {
            LockedSource::Path { path: path.clone() }
        }
    };
    let root = LockedPackageId {
        name: graph.root.name.clone(),
        version: graph.root.version.clone(),
        source: source(&graph.root.source),
    };
    let mut lock = nexa_lock::LockFile::new(
        root,
        ResolverIdentity {
            name: "nexa-reference".to_string(),
            version: nexa_compiler::version::TOOLCHAIN_VERSION.to_string(),
        },
    );
    for node in &graph.packages {
        let node_source = source(&node.id.source);
        let digest = if node.id.digest.is_empty() {
            nexa_lock::ContentDigest::sha256_hex(manifest_bytes)
                .as_str()
                .to_string()
        } else {
            node.id.digest.clone()
        };
        let dependencies = node
            .dependencies
            .iter()
            .map(|dependency| {
                let target = graph
                    .edges
                    .iter()
                    .find(|edge| edge.from == node.id && edge.alias.0 == dependency.alias)
                    .map(|edge| &edge.to);
                LockedDependency {
                    alias: dependency.alias.clone(),
                    name: target.map_or_else(|| dependency.alias.clone(), |id| id.name.clone()),
                    version_req: target
                        .map_or_else(|| dependency.version_req.clone(), |id| id.version.clone()),
                    source: source(&dependency.source),
                    kind: match dependency.kind {
                        nexa_resolver_pkg::DependencyKind::Normal => LockedDependencyKind::Normal,
                        nexa_resolver_pkg::DependencyKind::Dev => LockedDependencyKind::Dev,
                    },
                }
            })
            .collect();
        lock.add_package(LockedPackage {
            id: LockedPackageId {
                name: node.id.name.clone(),
                version: node.id.version.clone(),
                source: node_source.clone(),
            },
            source: node_source,
            digest,
            dependencies,
        })
        .map_err(|e| (EXIT_ICE, format!("lockfile construction failed: {e}")))?;
    }
    lock.validate().map_err(|errors| {
        (
            EXIT_ICE,
            format!(
                "generated lockfile is invalid: {}",
                errors
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        )
    })?;
    Ok(lock)
}

fn cmd_restore(mut args: impl Iterator<Item = String>) -> Result<u8, (u8, String)> {
    let lock_path = args
        .next()
        .map_or_else(|| PathBuf::from("nexa.lock"), PathBuf::from);
    if let Some(other) = args.next() {
        return Err(usage_error(format!("unknown restore argument '{other}'")));
    }
    let text = read_utf8_source(&lock_path)?;
    let lock = nexa_lock::LockFile::from_json(&text)
        .map_err(|e| (EXIT_LEX_ERROR, format!("invalid lockfile: {e}")))?;
    lock.validate().map_err(|errors| {
        (
            EXIT_LEX_ERROR,
            format!(
                "lockfile validation failed: {}",
                errors
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        )
    })?;
    let base = lock_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."));
    for package in &lock.packages {
        if let nexa_lock::LockedSource::Path { path } = &package.source {
            let resolved = base.join(path);
            if !resolved.exists() {
                return Err((
                    EXIT_IO,
                    format!(
                        "path package '{}' is missing at {}",
                        package.id.name,
                        resolved.display()
                    ),
                ));
            }
        }
    }
    println!("restored and verified {} package(s)", lock.packages.len());
    Ok(EXIT_OK)
}

fn cmd_test(mut args: impl Iterator<Item = String>) -> Result<u8, (u8, String)> {
    let file = args
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| usage_error("missing input file (usage: nexa test <FILE>)"))?;
    let mut format = "human".to_string();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--format" => {
                format = args
                    .next()
                    .ok_or_else(|| usage_error("--format requires human or json"))?;
            }
            other => return Err(usage_error(format!("unknown test argument '{other}'"))),
        }
    }
    let source = read_utf8_source(&file)?;
    let runner = nexa_test_runner::TestRunner::default();
    let results = runner.run(&file.display().to_string(), &source);
    let report = runner.report(results);
    match format.as_str() {
        "json" => println!("{}", report.to_json()),
        "human" => {
            for result in &report.results {
                println!("{:?} {}", result.status, result.id);
            }
            println!(
                "test result: {}. {} passed; {} failed; {} skipped",
                if report.is_success() { "ok" } else { "FAILED" },
                report.passed,
                report.failed,
                report.skipped
            );
        }
        other => return Err(usage_error(format!("unknown test format '{other}'"))),
    }
    Ok(if report.is_success() {
        EXIT_OK
    } else {
        EXIT_LEX_ERROR
    })
}

fn cmd_fmt(mut args: impl Iterator<Item = String>) -> Result<u8, (u8, String)> {
    let file = args
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| usage_error("missing input file (usage: nexa fmt <FILE> [--check])"))?;
    let mut check = false;
    for arg in args {
        match arg.as_str() {
            "--check" => check = true,
            other => return Err(usage_error(format!("unknown fmt argument '{other}'"))),
        }
    }
    let source = read_utf8_source(&file)?;
    let formatted = nexa_formatter::format_source(&source);
    let verification = nexa_formatter::verify(&source);
    if !verification.semantic_preserved || !verification.idempotent {
        return Err((
            EXIT_ICE,
            "formatter invariant verification failed".to_string(),
        ));
    }
    if check {
        if formatted != source {
            eprintln!("{} is not canonically formatted", file.display());
            return Ok(EXIT_LEX_ERROR);
        }
    } else if formatted != source {
        std::fs::write(&file, formatted.as_bytes())
            .map_err(|e| (EXIT_IO, format!("cannot write '{}': {e}", file.display())))?;
        println!("formatted {}", file.display());
    }
    Ok(EXIT_OK)
}

fn cmd_verify(mut args: impl Iterator<Item = String>) -> Result<u8, (u8, String)> {
    let file = args
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| usage_error("missing artifact (usage: nexa verify <FILE.wasm>)"))?;
    if let Some(other) = args.next() {
        return Err(usage_error(format!("unknown verify argument '{other}'")));
    }
    let bytes = std::fs::read(&file)
        .map_err(|e| (EXIT_IO, format!("cannot read '{}': {e}", file.display())))?;
    nexa_host_wasm::ValidatedWasmArtifact::new(bytes)
        .map_err(|e| (EXIT_LEX_ERROR, format!("artifact validation failed: {e}")))?;
    println!("verified {}", file.display());
    Ok(EXIT_OK)
}

fn cmd_inspect(mut args: impl Iterator<Item = String>) -> Result<u8, (u8, String)> {
    let file = args
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| usage_error("missing input (usage: nexa inspect <FILE>)"))?;
    if let Some(other) = args.next() {
        return Err(usage_error(format!("unknown inspect argument '{other}'")));
    }
    if file
        .extension()
        .is_some_and(|extension| extension == "wasm")
    {
        let bytes = std::fs::read(&file)
            .map_err(|e| (EXIT_IO, format!("cannot read '{}': {e}", file.display())))?;
        let valid = nexa_host_wasm::ValidatedWasmArtifact::new(bytes.clone()).is_ok();
        println!("artifact: {}", file.display());
        println!("bytes: {}", bytes.len());
        println!("wasm32-nexa-valid: {valid}");
    } else {
        let source = read_utf8_source(&file)?;
        let artifact = compile_source_to_wasm(&file.display().to_string(), &source)
            .map_err(|e| (EXIT_LEX_ERROR, e.to_string()))?;
        println!("source: {}", file.display());
        println!("functions: {}", artifact.function_count);
        println!("types: {}", artifact.type_count);
        println!("wasm-bytes: {}", artifact.wasm.len());
    }
    Ok(EXIT_OK)
}

fn cmd_build(mut args: impl Iterator<Item = String>) -> Result<u8, (u8, String)> {
    let file = args
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| usage_error("missing input file (usage: nexa build <FILE> [-o OUTPUT])"))?;
    let mut output = file.with_extension("wasm");
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-o" | "--output" => {
                output = args
                    .next()
                    .map(PathBuf::from)
                    .ok_or_else(|| usage_error("--output requires a path"))?;
            }
            other => return Err(usage_error(format!("unknown build argument '{other}'"))),
        }
    }

    let source = read_utf8_source(&file)?;
    let artifact = compile_source_to_wasm(&file.display().to_string(), &source)
        .map_err(|e| (EXIT_LEX_ERROR, e.to_string()))?;
    std::fs::write(&output, &artifact.wasm)
        .map_err(|e| (EXIT_IO, format!("cannot write '{}': {e}", output.display())))?;
    println!(
        "built {} ({} bytes, {} function(s), {} type(s))",
        output.display(),
        artifact.wasm.len(),
        artifact.function_count,
        artifact.type_count
    );
    Ok(EXIT_OK)
}

fn cmd_run(mut args: impl Iterator<Item = String>) -> Result<u8, (u8, String)> {
    let file = args
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| usage_error("missing input file (usage: nexa run <FILE>)"))?;
    if let Some(other) = args.next() {
        return Err(usage_error(format!("unknown run argument '{other}'")));
    }

    let source = read_utf8_source(&file)?;
    let artifact = compile_source_to_wasm(&file.display().to_string(), &source)
        .map_err(|e| (EXIT_LEX_ERROR, e.to_string()))?;
    let validated = nexa_host_wasm::ValidatedWasmArtifact::new(artifact.wasm)
        .map_err(|e| (EXIT_IO, format!("generated WASM was rejected by host: {e}")))?;
    let context = nexa_host_wasm::WasmExecutionContext::default();
    let result = nexa_host_wasm::run_wasm(&validated, &context)
        .map_err(|e| (EXIT_IO, format!("WASM execution failed: {e}")))?;
    if !result.stdout.is_empty() {
        print!("{}", String::from_utf8_lossy(&result.stdout));
    }
    if !result.stderr.is_empty() {
        eprint!("{}", String::from_utf8_lossy(&result.stderr));
    }
    Ok(result.exit_code.clamp(0, u8::MAX as i32) as u8)
}

fn read_utf8_source(file: &PathBuf) -> Result<String, (u8, String)> {
    let bytes = std::fs::read(file)
        .map_err(|e| (EXIT_IO, format!("cannot read '{}': {e}", file.display())))?;
    String::from_utf8(bytes).map_err(|e| {
        (
            EXIT_LEX_ERROR,
            format!("source '{}' is not valid UTF-8: {e}", file.display()),
        )
    })
}

/// `nexa version [--format human|json]` (Impl 01 §263, §466-467).
fn cmd_version(mut args: impl Iterator<Item = String>) -> Result<u8, (u8, String)> {
    let opts = parse_global_options(&mut args)?;
    match opts.format.as_str() {
        "human" => println!("{}", version::version_human()),
        "json" => {
            let output = serde_json::json!({
                "schemaVersion": SCHEMA_VERSION,
                "toolchainVersion": version::TOOLCHAIN_VERSION,
                "languageProfiles": [version::LANGUAGE_PROFILE],
            });
            let json = serde_json::to_string_pretty(&output)
                .map_err(|e| (EXIT_ICE, format!("json serialization failed: {e}")))?;
            println!("{json}");
        }
        other => return Err((EXIT_USAGE, format!("unknown --format '{other}'"))),
    }
    Ok(EXIT_OK)
}

/// `nexa lex <FILE> [--format human|json]` (Impl 01 §167, §172, §463).
fn usage_error(msg: impl Into<String>) -> (u8, String) {
    (EXIT_USAGE, msg.into())
}

fn cmd_lex(mut args: impl Iterator<Item = String>) -> Result<u8, (u8, String)> {
    let mut file: Option<PathBuf> = None;
    let mut format = String::from("human");
    while let Some(a) = args.next() {
        match a.as_str() {
            "--format" => {
                let value = args
                    .next()
                    .ok_or_else(|| usage_error("--format requires a value (human|json)"))?;
                format = value;
            }
            _ if a.starts_with('-') && a != "-" => {
                return Err(usage_error(format!("unknown option '{a}'")));
            }
            _ => {
                if file.is_some() {
                    return Err(usage_error(format!(
                        "multiple input files are not supported ('{a}')"
                    )));
                }
                file = Some(a.into());
            }
        }
    }

    let file = file.ok_or_else(|| {
        usage_error("missing input file (usage: nexa lex <FILE> [--format human|json])")
    })?;

    let bytes = std::fs::read(&file)
        .map_err(|e| (EXIT_IO, format!("cannot read '{}': {e}", file.display())))?;

    let mut pipeline = Pipeline::new();
    let result = pipeline.lex_bytes(&file, bytes);

    match format.as_str() {
        "human" => print_human(&pipeline, &result),
        "json" => {
            let envelope = lex_output_json(&result);
            let json = serde_json::to_string_pretty(&envelope)
                .map_err(|e| (EXIT_ICE, format!("json serialization failed: {e}")))?;
            println!("{json}");
        }
        other => {
            return Err((
                EXIT_USAGE,
                format!("unknown --format '{other}' (expected 'human' or 'json')"),
            ))
        }
    }

    if result.has_errors() {
        Ok(EXIT_LEX_ERROR)
    } else {
        Ok(EXIT_OK)
    }
}

/// `nexa parse <FILE> [--format human|json]` (Impl 02 §477-478).
///
/// Exit `1` quando há erros de parser; `3` quando o arquivo não pode ser
/// lido; invalid UTF-8 é reportado como erro de fronteira (exit `1`).
fn cmd_parse(mut args: impl Iterator<Item = String>) -> Result<u8, (u8, String)> {
    let mut file: Option<PathBuf> = None;
    let mut format = String::from("human");
    while let Some(a) = args.next() {
        match a.as_str() {
            "--format" => {
                let value = args
                    .next()
                    .ok_or_else(|| usage_error("--format requires a value (human|json)"))?;
                format = value;
            }
            _ if a.starts_with('-') && a != "-" => {
                return Err(usage_error(format!("unknown option '{a}'")));
            }
            _ => {
                if file.is_some() {
                    return Err(usage_error(format!(
                        "multiple input files are not supported ('{a}')"
                    )));
                }
                file = Some(a.into());
            }
        }
    }

    let file = file.ok_or_else(|| {
        usage_error("missing input file (usage: nexa parse <FILE> [--format human|json])")
    })?;

    let bytes = std::fs::read(&file)
        .map_err(|e| (EXIT_IO, format!("cannot read '{}': {e}", file.display())))?;

    let mut pipeline = Pipeline::new();
    let result = match pipeline.parse_bytes(&file, bytes) {
        Ok(r) => r,
        Err(diagnostic) => {
            eprintln!("{}", render_diagnostics(&pipeline.sources, &[diagnostic]));
            return Ok(EXIT_LEX_ERROR);
        }
    };

    match format.as_str() {
        "human" => print_human_parse(&pipeline, &result),
        "json" => {
            let envelope = parse_output_json(&result);
            let json = serde_json::to_string_pretty(&envelope)
                .map_err(|e| (EXIT_ICE, format!("json serialization failed: {e}")))?;
            println!("{json}");
        }
        other => {
            return Err((
                EXIT_USAGE,
                format!("unknown --format '{other}' (expected 'human' or 'json')"),
            ))
        }
    }

    if result.has_errors() {
        Ok(EXIT_LEX_ERROR)
    } else {
        Ok(EXIT_OK)
    }
}

/// `nexa resolve <FILE> [--format human|json]` (Impl 03 §458-465).
///
/// Mode single-module (§459). Exit `1` com erros de parser ou semânticos;
/// output JSON é debug tooling (§464-465), não formato normativo.
fn cmd_resolve(mut args: impl Iterator<Item = String>) -> Result<u8, (u8, String)> {
    let mut file: Option<PathBuf> = None;
    let mut format = String::from("human");
    while let Some(a) = args.next() {
        match a.as_str() {
            "--format" => {
                let value = args
                    .next()
                    .ok_or_else(|| usage_error("--format requires a value (human|json)"))?;
                format = value;
            }
            _ if a.starts_with('-') && a != "-" => {
                return Err(usage_error(format!("unknown option '{a}'")));
            }
            _ => {
                if file.is_some() {
                    return Err(usage_error(format!(
                        "multiple input files are not supported ('{a}')"
                    )));
                }
                file = Some(a.into());
            }
        }
    }

    let file = file.ok_or_else(|| {
        usage_error("missing input file (usage: nexa resolve <FILE> [--format human|json])")
    })?;

    let bytes = std::fs::read(&file)
        .map_err(|e| (EXIT_IO, format!("cannot read '{}': {e}", file.display())))?;

    let mut pipeline = Pipeline::new();
    let result = match pipeline.resolve_bytes(&file, bytes) {
        Ok(r) => r,
        Err(diagnostic) => {
            eprintln!("{}", render_diagnostics(&pipeline.sources, &[diagnostic]));
            return Ok(EXIT_LEX_ERROR);
        }
    };

    match format.as_str() {
        "human" => print_human_resolve(&pipeline, &result),
        "json" => {
            let envelope = resolve_output_json(&result, &pipeline.sources);
            let json = serde_json::to_string_pretty(&envelope)
                .map_err(|e| (EXIT_ICE, format!("json serialization failed: {e}")))?;
            println!("{json}");
        }
        other => {
            return Err((
                EXIT_USAGE,
                format!("unknown --format '{other}' (expected 'human' or 'json')"),
            ))
        }
    }

    if result.has_errors() {
        Ok(EXIT_LEX_ERROR)
    } else {
        Ok(EXIT_OK)
    }
}

/// `nexa check <FILE> [--format human|json]` (Impl 04).
///
/// Mode single-module. Exit `1` com erros de parser, semânticos ou de tipo;
/// output JSON é debug tooling (§464-465), não formato normativo.
fn cmd_check(mut args: impl Iterator<Item = String>) -> Result<u8, (u8, String)> {
    let mut file: Option<PathBuf> = None;
    let mut format = String::from("human");
    while let Some(a) = args.next() {
        match a.as_str() {
            "--format" => {
                let value = args
                    .next()
                    .ok_or_else(|| usage_error("--format requires a value (human|json)"))?;
                format = value;
            }
            _ if a.starts_with('-') && a != "-" => {
                return Err(usage_error(format!("unknown option '{a}'")));
            }
            _ => {
                if file.is_some() {
                    return Err(usage_error(format!(
                        "multiple input files are not supported ('{a}')"
                    )));
                }
                file = Some(a.into());
            }
        }
    }

    let file = file.ok_or_else(|| {
        usage_error("missing input file (usage: nexa check <FILE> [--format human|json])")
    })?;

    let bytes = std::fs::read(&file)
        .map_err(|e| (EXIT_IO, format!("cannot read '{}': {e}", file.display())))?;

    let mut pipeline = Pipeline::new();
    let result = match pipeline.check_bytes(&file, bytes) {
        Ok(r) => r,
        Err(diagnostic) => {
            eprintln!("{}", render_diagnostics(&pipeline.sources, &[diagnostic]));
            return Ok(EXIT_LEX_ERROR);
        }
    };

    match format.as_str() {
        "human" => print_human_check(&pipeline, &result),
        "json" => {
            let envelope = check_output_json(&result, &pipeline.sources);
            let json = serde_json::to_string_pretty(&envelope)
                .map_err(|e| (EXIT_ICE, format!("json serialization failed: {e}")))?;
            println!("{json}");
        }
        other => {
            return Err((
                EXIT_USAGE,
                format!("unknown --format '{other}' (expected 'human' or 'json')"),
            ))
        }
    }

    if result.has_errors() {
        Ok(EXIT_LEX_ERROR)
    } else {
        Ok(EXIT_OK)
    }
}

/// Output humano do type checker: resumo do modelo tipado em stdout e todos
/// os diagnostics (parse, semânticos e de tipo) em stderr, com localização.
fn print_human_check(pipeline: &Pipeline, result: &TypeCheckResult) {
    let symbols = result.typed.index.symbols.count();
    let expressions = result.typed.semantic.expression_info.count();
    println!("checked {symbols} symbols, {expressions} typed expressions");
    println!(
        "type check: {} error(s), {} warning(s)",
        result.error_count(),
        result.warning_count()
    );
    if !result.parse_diagnostics.is_empty() {
        eprintln!(
            "{}",
            render_diagnostics(&pipeline.sources, &result.parse_diagnostics)
        );
    }
    if !result.semantic_diagnostics.is_empty() {
        eprintln!("semantic diagnostics:");
        for d in &result.semantic_diagnostics {
            let loc = location_str(&pipeline.sources, d.span).unwrap_or_default();
            eprintln!("  [{}] {loc}: {}", d.code, d.message);
        }
    }
    if !result.typed.diagnostics.is_empty() {
        eprintln!("type diagnostics:");
        for d in &result.typed.diagnostics {
            let loc = location_str(&pipeline.sources, d.span).unwrap_or_default();
            eprintln!(
                "  {} [{}] {loc}: {}",
                type_severity_label(d.severity),
                d.code.code_str(),
                d.message
            );
        }
    }
    if !result.flow_diagnostics.is_empty() {
        eprintln!("flow diagnostics:");
        for d in &result.flow_diagnostics {
            let loc = match d.span {
                Some(span) => location_str(&pipeline.sources, span).unwrap_or_default(),
                None => String::new(),
            };
            eprintln!(
                "  {} [{}] {loc}: {}",
                if d.code.severity() == nexa_compiler::Severity::Error {
                    "error"
                } else {
                    "warning"
                },
                d.code.as_str(),
                d.message
            );
        }
    }
    if !result.effect_diagnostics.is_empty() {
        eprintln!("effect diagnostics:");
        for d in &result.effect_diagnostics {
            let loc = location_str(&pipeline.sources, d.span).unwrap_or_default();
            eprintln!(
                "  {} [{}] {loc}: {}",
                if d.code.is_error() {
                    "error"
                } else {
                    "warning"
                },
                d.code.code_str(),
                d.message
            );
        }
    }
}

/// Output humano do resolver (debug, amostra §463):
///
/// ```text
/// users::service::load
///   Symbol #12 Function
///
/// reference main.nexa:4:12 `UserId`
///   → Symbol #3 Type UserId
/// ```
///
/// Também imprime os diagnostics parse/semânticos em stderr.
fn print_human_resolve(pipeline: &Pipeline, result: &SemanticResult) {
    for s in result.index.symbols.iter() {
        let name = result.index.interner.resolve(s.name);
        println!("{name}");
        println!("  Symbol #{} {}", s.id.0, symbol_kind_label(s.kind));
        println!();
    }
    for r in result.index.references.all() {
        let loc =
            location_str(&pipeline.sources, r.span).unwrap_or_else(|| "<invalid span>".into());
        let text = pipeline
            .sources
            .source(r.span.source)
            .and_then(|f| f.span_text(r.span))
            .unwrap_or("");
        let target = result.index.symbols.get(r.symbol);
        let target_name = target
            .map(|s| result.index.interner.resolve(s.name))
            .unwrap_or("?");
        let target_kind = target.map(|s| symbol_kind_label(s.kind)).unwrap_or("?");
        println!("reference {loc} `{text}`");
        println!("  → Symbol #{} {target_kind} {target_name}", r.symbol.0);
        println!();
    }
    if !result.parse_diagnostics.is_empty() {
        eprintln!(
            "{}",
            render_diagnostics(&pipeline.sources, &result.parse_diagnostics)
        );
    }
    if !result.semantic_diagnostics.is_empty() {
        eprintln!("semantic diagnostics:");
        for d in &result.semantic_diagnostics {
            let loc = location_str(&pipeline.sources, d.span).unwrap_or_default();
            eprintln!("  [{}] {loc}: {}", d.code, d.message);
        }
    }
}

/// Opções globais compartilhadas (`--format human|json`).
fn parse_global_options(args: &mut impl Iterator<Item = String>) -> Result<Options, (u8, String)> {
    let mut format = String::from("human");
    while let Some(a) = args.next() {
        match a.as_str() {
            "--format" => {
                if format != "human" {
                    return Err(usage_error("duplicate --format"));
                }
                format = args
                    .next()
                    .ok_or_else(|| usage_error("--format requires a value (human|json)"))?;
            }
            other => return Err(usage_error(format!("unknown argument '{other}'"))),
        }
    }
    Ok(Options { format })
}

/// Output humano conforme exemplo do contrato (Impl 01 §463):
/// `startByte..endByte kind`.
fn print_human(pipeline: &Pipeline, result: &LexResult) {
    for lexeme in &result.lexemes {
        let Some(file) = pipeline.sources.source(lexeme.span.source) else {
            continue;
        };
        let kind = match lexeme.kind {
            LexemeKind::Token(k) => human_kind(k),
            LexemeKind::Trivia(t) => format!("trivia:{}", t.as_str()),
        };
        let text = file
            .span_text(lexeme.span)
            .unwrap_or("")
            .escape_debug()
            .to_string();
        println!(
            "{:<7}..{:<6} {:<28} {}",
            lexeme.span.start, lexeme.span.end, kind, text
        );
    }
    if !result.diagnostics.is_empty() {
        eprintln!(
            "{}",
            render_diagnostics(&pipeline.sources, &result.diagnostics)
        );
    }
}

/// Output humano do parser: resumo estrutural por declaração de topo (§477).
fn print_human_parse(pipeline: &Pipeline, result: &nexa_compiler::ParseResult) {
    let dump = parse_output_json(result);
    let module = dump
        .ast_dump
        .get("module")
        .and_then(|m| m.get("name"))
        .and_then(|n| n.as_str());
    match module {
        Some(name) => println!("module {name}"),
        None => println!("module <none>"),
    }
    if let Some(imports) = dump.ast_dump.get("imports").and_then(|v| v.as_array()) {
        for imp in imports {
            let path = imp
                .get("path")
                .and_then(|p| p.as_str())
                .unwrap_or("<invalid>");
            let alias = imp
                .get("alias")
                .and_then(|a| a.as_str())
                .map(|a| format!(" as {a}"))
                .unwrap_or_default();
            println!("import {path}{alias}");
        }
    }
    if let Some(items) = dump.ast_dump.get("items").and_then(|v| v.as_array()) {
        for item in items {
            let kind = item.get("kind").and_then(|k| k.as_str()).unwrap_or("?");
            let name = item.get("name").and_then(|n| n.as_str()).unwrap_or("");
            println!("{kind} {name}");
        }
    }
    if !result.diagnostics.is_empty() {
        eprintln!(
            "{}",
            render_diagnostics(&pipeline.sources, &result.diagnostics)
        );
    }
}
