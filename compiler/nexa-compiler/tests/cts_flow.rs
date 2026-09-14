//! CTS-FLOW — suíte de conformidade da análise de fluxo (Implementação 05).
//!
//! Casos single-module resolvidos pela fronteira pública real
//! `Pipeline::check_bytes` (parse → resolve → typecheck → flow):
//!
//! 1. `cts/flow/positive/CTS-FLOW-XXXX.nexa` — fontes com zero erros
//!    (00XX; podem conter warnings de fluxo, ex.: NEXA-FLOW-0002);
//! 2. `cts/flow/negative/CTS-FLOW-XXXX.nexa` — fontes type-clean com ≥1
//!    erro de análise de fluxo (01XX; sem erros de parse/resolve/tipo).
//!
//! Golden = envelope `check_output_json` (inclui `flowDiagnostics` e
//! `summary.flowCfgs`). O runner regera os goldens com `NEXA_CTS_BLESS=1`.

use std::path::{Path, PathBuf};

use nexa_compiler::{check_output_json, Pipeline, TypeCheckResult};

fn cts_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cts/flow")
}

fn case_id(stem: &str) -> u32 {
    stem["CTS-FLOW-".len()..]
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
                        .is_some_and(|s| s.to_string_lossy().starts_with("CTS-FLOW-"))
            })
            .collect();
        cases.append(&mut found);
    }
    cases.sort();
    cases
}

fn flow_error_count(result: &TypeCheckResult) -> usize {
    result
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
fn cts_flow_bless_goldens() {
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
fn cts_flow_suite_present_and_reproducable() {
    let cases = discover_cases();
    assert!(
        cases.len() >= 14,
        "expected at least the flow positive+negative cases, found {}",
        cases.len()
    );

    let mut codes_seen = std::collections::BTreeSet::new();
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
            "CTS-FLOW caso {stem}: envelope divergiu do golden"
        );

        if positive {
            assert!(
                !result.has_errors(),
                "[{stem}] caso positivo não pode ter erros"
            );
            for d in &result.flow_diagnostics {
                codes_seen.insert(d.code.as_str().to_string());
            }
        } else {
            // Classificação flow-only: nenhum erro de parse/resolve/tipo;
            // pelo menos um erro de análise de fluxo.
            let n = flow_error_count(&result);
            assert!(n >= 1, "[{stem}] caso negativo precisa de ≥1 erro de fluxo");
            assert_eq!(
                result.error_count(),
                n,
                "[{stem}] caso flow-negative deve ter somente erros de fluxo"
            );
            for d in &result.flow_diagnostics {
                if d.code.severity() == nexa_diagnostics::Severity::Error {
                    codes_seen.insert(d.code.as_str().to_string());
                }
            }
        }
    }

    // Cobertura mínima: todos os códigos efetivamente emitidos pela análise
    // de fluxo. NEXA-FLOW-0007 (InvalidControlFlow) é reservado e não é
    // emitido por nenhum caminho atual da fronteira.
    for expected in [
        "NEXA-FLOW-0001",
        "NEXA-FLOW-0002",
        "NEXA-FLOW-0003",
        "NEXA-FLOW-0004",
        "NEXA-FLOW-0005",
        "NEXA-FLOW-0006",
        "NEXA-FLOW-0008",
        "NEXA-FLOW-0009",
    ] {
        assert!(
            codes_seen.contains(expected),
            "suíte CTS-FLOW deve cobrir {expected}"
        );
    }
}
