//! CTS-CHECK — suíte de conformidade do type checker (Implementação 04).
//!
//! Casos single-module resolvidos pela fronteira pública real
//! `Pipeline::check_bytes` (parse → resolve → typecheck, §459):
//!
//! 1. `cts/check/positive/CTS-CHECK-XXXX.nexa` — fontes tipáveis com zero
//!    erros (positivos, 00XX);
//! 2. `cts/check/negative/CTS-CHECK-XXXX.nexa` — fontes com ≥1 erro de tipo
//!    (negativos, 01XX).
//!
//! Golden = envelope `check_output_json` (§464-465). O runner regera os
//! goldens com `NEXA_CTS_BLESS=1`.

use std::path::{Path, PathBuf};

use nexa_compiler::{check_output_json, Pipeline, TypeCheckResult};

fn cts_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cts/check")
}

fn case_id(stem: &str) -> u32 {
    stem["CTS-CHECK-".len()..]
        .parse()
        .unwrap_or_else(|_| panic!("[{stem}] case id inválido"))
}

fn discover_cases() -> Vec<PathBuf> {
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
                        .is_some_and(|s| s.to_string_lossy().starts_with("CTS-CHECK-"))
            })
            .collect();
        cases.append(&mut found);
    }
    cases.sort();
    cases
}

fn type_error_count(result: &TypeCheckResult) -> usize {
    result
        .typed
        .diagnostics
        .iter()
        .filter(|d| d.severity == nexa_typecheck::diagnostics::DiagnosticSeverity::Error)
        .count()
        + result
            .flow_diagnostics
            .iter()
            .filter(|d| d.code.severity() == nexa_diagnostics::Severity::Error)
            .count()
}

/// Roda um caso pela fronteira real `Pipeline::check_bytes`.
fn run_case(input_path: &Path) -> (String, TypeCheckResult, Pipeline) {
    let stem = input_path
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let bytes =
        std::fs::read(input_path).unwrap_or_else(|e| panic!("[{stem}] cannot read input: {e}"));
    let display = PathBuf::from(format!("{stem}.nexa"));
    let mut pipeline = Pipeline::new();
    let result = pipeline
        .check_bytes(&display, bytes)
        .unwrap_or_else(|d| panic!("[{stem}] fronteira retornou diagnóstico: {:?}", d.message));
    (stem, result, pipeline)
}

#[test]
fn cts_check_bless_goldens() {
    if std::env::var_os("NEXA_CTS_BLESS").is_none() {
        return;
    }
    for input_path in discover_cases() {
        let (stem, result, pipeline) = run_case(&input_path);
        let json = serde_json::to_string_pretty(&check_output_json(&result, &pipeline.sources))
            .unwrap_or_else(|e| panic!("[{stem}] serialization failed: {e}"));
        let golden_path = cts_dir().join("golden").join(format!("{stem}.json"));
        std::fs::write(&golden_path, json)
            .unwrap_or_else(|e| panic!("[{stem}] cannot write golden {golden_path:?}: {e}"));
    }
}

#[test]
fn cts_check_suite_present_and_reproducable() {
    let cases = discover_cases();
    assert!(
        cases.len() >= 63,
        "expected at least the type checker positive+negative cases, found {}",
        cases.len()
    );

    let mut codes_seen = std::collections::BTreeSet::new();
    let mut flow_warnings_seen = std::collections::BTreeSet::new();
    for input_path in &cases {
        let (stem, result, pipeline) = run_case(input_path);
        let id = case_id(&stem);
        let positive = id < 100;

        let golden_path = cts_dir().join("golden").join(format!("{stem}.json"));
        let golden_text = std::fs::read_to_string(&golden_path)
            .unwrap_or_else(|e| panic!("[{stem}] cannot read golden {golden_path:?}: {e}"));

        let actual = serde_json::to_value(check_output_json(&result, &pipeline.sources))
            .unwrap_or_else(|e| panic!("[{stem}] serialization failed: {e}"));
        let expected: serde_json::Value = serde_json::from_str(&golden_text)
            .unwrap_or_else(|e| panic!("[{stem}] golden é JSON inválido: {e}"));
        assert_eq!(
            expected, actual,
            "CTS-CHECK caso {stem}: envelope divergiu do golden"
        );

        if positive {
            assert!(
                !result.has_errors(),
                "[{stem}] caso positivo não pode ter erros"
            );
            for d in &result.flow_diagnostics {
                if d.code.severity() == nexa_diagnostics::Severity::Warning {
                    flow_warnings_seen.insert(d.code.as_str().to_string());
                }
            }
        } else {
            let n = type_error_count(&result);
            assert!(n >= 1, "[{stem}] caso negativo precisa de ≥1 erro de tipo");
            for d in &result.typed.diagnostics {
                if d.severity == nexa_typecheck::diagnostics::DiagnosticSeverity::Error {
                    codes_seen.insert(d.code.code_str().to_string());
                }
            }
            for d in &result.flow_diagnostics {
                if d.code.severity() == nexa_diagnostics::Severity::Error {
                    codes_seen.insert(d.code.as_str().to_string());
                }
            }
        }
    }

    // Cobertura mínima de códigos de diagnóstico de tipo nos negativos.
    for expected in [
        "NEXA-TYPE-0001",
        "NEXA-TYPE-0002",
        "NEXA-TYPE-0003",
        "NEXA-TYPE-0004",
        "NEXA-TYPE-0005",
        "NEXA-TYPE-0006",
        "NEXA-TYPE-0008",
        "NEXA-TYPE-0009",
        "NEXA-TYPE-0010",
        "NEXA-TYPE-0011",
        "NEXA-TYPE-0012",
        "NEXA-TYPE-0013",
        "NEXA-TYPE-0015",
        "NEXA-TYPE-0016",
        "NEXA-TYPE-0017",
        "NEXA-TYPE-0018",
        "NEXA-TYPE-0019",
        "NEXA-TYPE-0020",
        "NEXA-TYPE-0021",
        "NEXA-TYPE-0022",
        "NEXA-TYPE-0023",
        "NEXA-TYPE-0024",
        "NEXA-TYPE-0025",
        "NEXA-TYPE-0026",
        "NEXA-TYPE-0027",
        "NEXA-TYPE-0030",
        "NEXA-TYPE-0031",
        "NEXA-TYPE-0032",
        "NEXA-TYPE-0034",
        "NEXA-TYPE-0035",
        "NEXA-TYPE-0036",
        "NEXA-TYPE-0039",
        "NEXA-TYPE-0040",
        "NEXA-TYPE-0041",
        "NEXA-TYPE-0042",
        "NEXA-TYPE-0043",
        "NEXA-TYPE-0044",
        "NEXA-TYPE-0045",
    ] {
        assert!(
            codes_seen.contains(expected),
            "suíte CTS-CHECK deve cobrir {expected}"
        );
    }

    // Cobertura mínima de códigos de fluxo (Implementação 05, fatia 1).
    for expected in ["NEXA-FLOW-0001", "NEXA-FLOW-0005", "NEXA-FLOW-0006"] {
        assert!(
            codes_seen.contains(expected),
            "suíte CTS-CHECK deve cobrir {expected}"
        );
    }
    assert!(
        flow_warnings_seen.contains("NEXA-FLOW-0002"),
        "suíte CTS-CHECK deve cobrir o warning NEXA-FLOW-0002"
    );
}
