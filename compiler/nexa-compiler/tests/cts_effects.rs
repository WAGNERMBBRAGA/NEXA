//! CTS-EFFECTS — suíte de conformidade do modelo de efeitos (Implementação 05).
//!
//! Casos single-module resolvidos pela fronteira pública real
//! `Pipeline::check_bytes` (parse → resolve → typecheck → flow → effects):
//!
//! 1. `cts/effects/positive/CTS-EFFECTS-XXXX.nexa` — fontes com zero erros
//!    (00XX; actions com cláusula `effects` válida, inferência de effects);
//! 2. `cts/effects/negative/CTS-EFFECTS-XXXX.nexa` — fontes type-clean com
//!    ≥1 erro do modelo de efeitos (01XX; sem erros de parse/resolve/tipo).
//!
//! Golden = envelope `check_output_json` (inclui `effectDiagnostics` e
//! `summary.effectCallEdges`). O runner regera os goldens com
//! `NEXA_CTS_BLESS=1`.

use std::path::{Path, PathBuf};

use nexa_compiler::{check_output_json, Pipeline, TypeCheckResult};

fn cts_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cts/effects")
}

fn case_id(stem: &str) -> u32 {
    stem["CTS-EFFECTS-".len()..]
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
                        .is_some_and(|s| s.to_string_lossy().starts_with("CTS-EFFECTS-"))
            })
            .collect();
        cases.append(&mut found);
    }
    cases.sort();
    cases
}

fn effect_error_count(result: &TypeCheckResult) -> usize {
    result
        .effect_diagnostics
        .iter()
        .filter(|d| d.code.is_error())
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
fn cts_effects_bless_goldens() {
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
fn cts_effects_suite_present_and_reproducable() {
    let cases = discover_cases();
    assert!(
        cases.len() >= 9,
        "expected at least the effects positive+negative cases, found {}",
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
            "CTS-EFFECTS caso {stem}: envelope divergiu do golden"
        );

        if positive {
            assert!(
                !result.has_errors(),
                "[{stem}] caso positivo não pode ter erros"
            );
            for d in &result.effect_diagnostics {
                codes_seen.insert(d.code.code_str().to_string());
            }
        } else {
            // Classificação effect-only: nenhum erro de parse/resolve/tipo/
            // fluxo; pelo menos um erro do modelo de efeitos.
            let n = effect_error_count(&result);
            assert!(
                n >= 1,
                "[{stem}] caso negativo precisa de ≥1 erro de efeito"
            );
            assert_eq!(
                result.error_count(),
                n,
                "[{stem}] caso effect-negative deve ter somente erros de efeito"
            );
            for d in &result.effect_diagnostics {
                if d.code.is_error() {
                    codes_seen.insert(d.code.code_str().to_string());
                }
            }
        }
    }

    // Cobertura mínima: códigos emitidos pela fronteira pública.
    // NEXA-EFFECT-0004 (interface contract) exige interfaces com effects e
    // NEXA-EFFECT-0006..0010 dependem de features não exercitadas na
    // fronteira atual — não são exigidos aqui.
    for expected in [
        "NEXA-EFFECT-0001",
        "NEXA-EFFECT-0002",
        "NEXA-EFFECT-0003",
        "NEXA-EFFECT-0005",
    ] {
        assert!(
            codes_seen.contains(expected),
            "suíte CTS-EFFECTS deve cobrir {expected}"
        );
    }
}
