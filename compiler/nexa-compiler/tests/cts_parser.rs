//! CTS-PARSE — suíte de conformidade do parser (Implementação 02 §480-489).
//!
//! Cada caso é um par:
//! - `cts/parser/{positive,negative,recovery}/CTS-PARSE-XXXX.nexa` — fonte;
//! - `cts/parser/golden/CTS-PARSE-XXXX.json` — envelope golden
//!   (`{ schemaVersion, diagnostics, astDump }`) = `parse_output_json`
//!   (projeção `{kind, span, children}`; nenhum detalhe de serialização Rust
//!   interna — §482-483).
//!
//! O runner lexa/parsea pela fronteira real `Pipeline::parse_bytes` de cada
//! fixture (reprodução exata de bytes, inclusive CRLF) e verifica:
//! - reconstrução lossless da fonte (§485 `CTS-PARSE-0030`);
//! - envelope completo contra o golden;
//! - positivos (00XX) com zero erros; negativos (01XX) e recovery (02XX)
//!   com ao menos um; recovery ainda preserva a declaração seguinte (§464-465).

use std::path::{Path, PathBuf};

use nexa_ast::ItemKind;
use nexa_compiler::{parse_output_json, Pipeline};
use nexa_diagnostics::Severity;

fn cts_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cts/parser")
}

fn discover_cases() -> Vec<PathBuf> {
    let root = cts_dir();
    let mut cases: Vec<PathBuf> = Vec::new();
    for sub in ["positive", "negative", "recovery"] {
        let dir = root.join(sub);
        let mut found: Vec<PathBuf> = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("cannot read {dir:?}: {e}"))
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.extension().is_some_and(|x| x == "nexa")
                    && p.file_stem()
                        .is_some_and(|s| s.to_string_lossy().starts_with("CTS-PARSE-"))
            })
            .collect();
        cases.append(&mut found);
    }
    cases.sort();
    cases
}

fn case_id(case: &str) -> u32 {
    case["CTS-PARSE-".len()..]
        .parse()
        .unwrap_or_else(|_| panic!("[{case}] case id inválido"))
}

fn error_count(result: &nexa_compiler::ParseResult) -> usize {
    result
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .count()
}

fn item_last_name(result: &nexa_compiler::ParseResult) -> String {
    let Some(item) = result.ast.items.last() else {
        return String::new();
    };
    match &item.kind {
        ItemKind::Function(f) => f.name.name.clone(),
        ItemKind::Action(a) => a.name.name.clone(),
        ItemKind::Struct(s) => s.name.name.clone(),
        ItemKind::Enum(e) => e.name.name.clone(),
        ItemKind::Interface(i) => i.name.name.clone(),
        ItemKind::TypeAlias(t) => t.name.name.clone(),
        _ => String::new(),
    }
}

#[test]
fn cts_parse_bless_goldens() {
    // Regenera cts/parser/golden/*.json a partir da fronteira real do
    // compiler (sem passar pelo console, que mangla não-ASCII da mensagem).
    // Uso: `NEXA_CTS_BLESS=1 cargo test -p nexa-compiler --test cts_parser`.
    if std::env::var_os("NEXA_CTS_BLESS").is_none() {
        return;
    }
    for input_path in discover_cases() {
        let stem = input_path
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let bytes = std::fs::read(&input_path)
            .unwrap_or_else(|e| panic!("[{stem}] cannot read input: {e}"));
        let mut pipeline = Pipeline::new();
        let result = pipeline
            .parse_bytes(&input_path, bytes)
            .unwrap_or_else(|d| panic!("[{stem}] fronteira retornou diagnóstico: {:?}", d.message));
        let json = serde_json::to_string_pretty(&parse_output_json(&result))
            .unwrap_or_else(|e| panic!("[{stem}] serialization failed: {e}"));
        let golden_path = cts_dir().join("golden").join(format!("{stem}.json"));
        std::fs::write(&golden_path, json)
            .unwrap_or_else(|e| panic!("[{stem}] cannot write golden {golden_path:?}: {e}"));
    }
}

#[test]
fn cts_parse_suite_present_and_reproducable() {
    let cases = discover_cases();
    assert!(
        cases.len() >= 30,
        "expected at least 30 CTS-PARSE cases (30 positivos + negativos + recovery), found {}",
        cases.len()
    );
    let positive = cases
        .iter()
        .filter(|p| case_id(&p.file_stem().unwrap().to_string_lossy()) < 100)
        .count();
    assert_eq!(
        positive, 30,
        "CTS-PARSE-00XX deve ter exatamente os 30 positivos de §485"
    );

    for input_path in &cases {
        let stem = input_path
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let id = case_id(&stem);

        // Entrada (bytes exatos; CRLF do 0029 deve sobreviver).
        let bytes =
            std::fs::read(input_path).unwrap_or_else(|e| panic!("[{stem}] cannot read input: {e}"));

        // Golden.
        let golden_path = cts_dir().join("golden").join(format!("{stem}.json"));
        let golden_text = std::fs::read_to_string(&golden_path)
            .unwrap_or_else(|e| panic!("[{stem}] cannot read golden file {golden_path:?}: {e}"));

        // Parseia pela fronteira pública real do compiler.
        let mut pipeline = Pipeline::new();
        let result = pipeline
            .parse_bytes(input_path, bytes.clone())
            .unwrap_or_else(|d| panic!("[{stem}] fronteira retornou diagnóstico: {:?}", d.message));

        // Reconstrução lossless: o texto reconstruído deve ser a fonte exata.
        let expected_text = String::from_utf8(bytes)
            .unwrap_or_else(|e| panic!("[{stem}] fixture não é UTF-8 válido: {e}"));
        assert_eq!(
            result.reconstruction, expected_text,
            "[{stem}] reconstrução deve reproduzir a fonte byte a byte"
        );

        // Envelope golden = projeção normalizada (§482); comparar inteiro.
        let actual =
            serde_json::to_value(parse_output_json(&result)).unwrap_or_else(|e| panic!("{e}"));
        let expected: serde_json::Value = serde_json::from_str(&golden_text)
            .unwrap_or_else(|e| panic!("[{stem}] golden é JSON inválido: {e}"));
        assert_eq!(
            expected, actual,
            "CTS-PARSE caso {stem}: envelope divergiu do golden"
        );

        let n = error_count(&result);
        let kind = if id < 100 {
            "positive"
        } else if id < 200 {
            "negative"
        } else {
            "recovery"
        };
        match kind {
            "positive" => assert_eq!(n, 0, "[{stem}] caso positivo não pode ter erros"),
            _ => assert!(n >= 1, "[{stem}] precisa reportar ao menos um erro"),
        }

        // Recovery (§464-465): declaração/statement seguinte deve ser
        // preservada em vez de engolida.
        if kind == "recovery" {
            match &stem[..] {
                "CTS-PARSE-0201" | "CTS-PARSE-0203" | "CTS-PARSE-0204" | "CTS-PARSE-0205"
                | "CTS-PARSE-0206" => {
                    assert!(
                        result.ast.items.len() >= 2,
                        "[{stem}] recovery deve preservar a função seguinte como item"
                    );
                    assert_eq!(
                        item_last_name(&result),
                        "good",
                        "[{stem}] último item deve ser a função válida `good`"
                    );
                }
                "CTS-PARSE-0202" => {
                    // quebra de expressão seguida de statement válido dentro
                    // do mesmo corpo: o statement seguinte não pode sumir.
                    assert_eq!(result.ast.items.len(), 1, "[{stem}] um item esperado");
                    let ItemKind::Function(f) = &result.ast.items[0].kind else {
                        panic!("[{stem}] esperava função como item único");
                    };
                    assert!(
                        f.body.stmts.len() >= 2,
                        "[{stem}] o statement seguinte à quebra deve ser preservado"
                    );
                }
                _ => {}
            }
        }
    }
}
