//! CTS-LEX — suíte de conformidade do lexer (Implementação 01 §181-197).
//!
//! Cada caso é um par:
//! - `cts/lexer/CTS-LEX-XXXX.nexa` — fonte de entrada (bytes exatos);
//! - `cts/lexer/golden/CTS-LEX-XXXX.json` — envelope JSON Golden
//!   (`{ schemaVersion, tokens, diagnostics }`), reprodução exata de
//!   `lex_output_json`.
//!
//! O runner lexa via a fronteira real `Pipeline::lex_bytes` (validando a API
//! pública e a fronteira UTF-8) e compara semanticamente o envelope contra o
//! golden. Casos positivos (`CTS-LEX-00XX`) devem ter zero diagnostics de
//! erro; negativos (`CTS-LEX-01XX`) devem ter ao menos um.

use std::path::{Path, PathBuf};

use nexa_compiler::{lex_output_json, Pipeline};
use nexa_diagnostics::Severity;

fn cts_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cts/lexer")
}

fn discover_cases() -> Vec<PathBuf> {
    let dir = cts_dir();
    let mut cases: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read cts/lexer: {e}"))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension().is_some_and(|x| x == "nexa")
                && p.file_stem()
                    .is_some_and(|s| s.to_string_lossy().starts_with("CTS-LEX-"))
        })
        .collect();
    cases.sort();
    cases
}

#[test]
fn cts_lex_suite_present_and_reproducable() {
    let cases = discover_cases();
    assert!(
        cases.len() >= 19,
        "expected at least the 11 positive + 8 negative CTS-LEX cases, found {}",
        cases.len()
    );

    for input_path in &cases {
        let stem = input_path.file_stem().unwrap().to_string_lossy();
        let case = stem.as_ref();

        // Entrada (bytes exatos; BOM e CRLF devem sobreviver à leitura).
        let bytes =
            std::fs::read(input_path).unwrap_or_else(|e| panic!("[{case}] cannot read input: {e}"));

        // Golden.
        let golden_path = cts_dir().join("golden").join(format!("{case}.json"));
        let golden_text = std::fs::read_to_string(&golden_path)
            .unwrap_or_else(|e| panic!("[{case}] cannot read golden file {golden_path:?}: {e}"));

        // Lexa pela fronteira pública real do compiler.
        let mut pipeline = Pipeline::new();
        let result = pipeline.lex_bytes(input_path, bytes);

        // Compara envelope completo (tokens kind/start/end + diagnostics).
        let actual = serde_json::to_value(lex_output_json(&result))
            .unwrap_or_else(|e| panic!("[{case}] serialization failed: {e}"));
        let expected: serde_json::Value = serde_json::from_str(&golden_text)
            .unwrap_or_else(|e| panic!("[{case}] golden is invalid JSON: {e}"));

        assert_eq!(
            expected, actual,
            "CTS-LEX case {case}: envelope divergiu do golden"
        );

        // Contratos derivados: positivos (CTS-LEX-00XX) zero erros;
        // negativos (CTS-LEX-01XX) ≥1 erro.
        let num: u32 = case["CTS-LEX-".len()..]
            .parse()
            .unwrap_or_else(|_| panic!("[{case}] case id inválido"));
        let is_positive = num < 100;
        let error_count = result
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .count();
        if is_positive {
            assert_eq!(
                error_count, 0,
                "[{case}] caso positivo não deve ter diagnostics de erro (golden pode conter para diagnóstico específico)"
            );
        } else {
            assert!(
                error_count >= 1,
                "[{case}] caso negativo deve reportar ao menos um erro"
            );
        }
    }
}
