//! Executable conformance slice: checked source → verified NIR → valid WASM.

use nexa_compiler::compile_source_to_wasm;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("compiler crate must be inside workspace/compiler")
        .to_path_buf()
}

#[test]
fn cts_codegen_positive_is_deterministic_and_valid() {
    let directory = workspace_root().join("cts/codegen/positive");
    let mut cases: Vec<PathBuf> = std::fs::read_dir(&directory)
        .expect("cts/codegen/positive must exist")
        .map(|entry| entry.expect("valid directory entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "nexa"))
        .collect();
    cases.sort();
    assert!(!cases.is_empty(), "codegen CTS must contain cases");

    for path in cases {
        let source = std::fs::read_to_string(&path).expect("CTS source must be UTF-8");
        let name = path.display().to_string();
        let first = compile_source_to_wasm(&name, &source).expect("CTS case must compile");
        let second = compile_source_to_wasm(&name, &source).expect("CTS case must recompile");
        assert_eq!(first.wasm, second.wasm, "{} is not reproducible", name);
        assert!(first.wasm.starts_with(b"\0asm"), "{} is not WASM", name);

        let stdout_path = path.with_extension("stdout");
        if stdout_path.exists() {
            let validated = nexa_host_wasm::ValidatedWasmArtifact::new(first.wasm)
                .expect("CTS artifact must load in the reference host");
            let context = nexa_host_wasm::WasmExecutionContext::default();
            let result = nexa_host_wasm::run_wasm(&validated, &context)
                .expect("CTS artifact must execute in the reference host");
            let mut expected = std::fs::read(&stdout_path).expect("stdout golden must be readable");
            if expected.ends_with(b"\r\n") {
                expected.truncate(expected.len() - 2);
            } else if expected.ends_with(b"\n") {
                expected.pop();
            }
            assert_eq!(result.stdout, expected, "{} stdout differs", name);
        }
    }
}

#[test]
fn cts_codegen_semantic_traps_are_deterministic() {
    let directory = workspace_root().join("cts/codegen/traps");
    let mut cases: Vec<PathBuf> = std::fs::read_dir(&directory)
        .expect("cts/codegen/traps must exist")
        .map(|entry| entry.expect("valid directory entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "nexa"))
        .collect();
    cases.sort();
    assert!(!cases.is_empty(), "trap CTS must contain cases");

    for path in cases {
        let source = std::fs::read_to_string(&path).expect("CTS source must be UTF-8");
        let name = path.display().to_string();
        let first = compile_source_to_wasm(&name, &source).expect("trap CTS case must compile");
        let second = compile_source_to_wasm(&name, &source).expect("trap CTS must recompile");
        assert_eq!(first.wasm, second.wasm, "{} is not reproducible", name);

        let validated = nexa_host_wasm::ValidatedWasmArtifact::new(first.wasm)
            .expect("trap CTS artifact must load");
        let error =
            nexa_host_wasm::run_wasm(&validated, &nexa_host_wasm::WasmExecutionContext::default())
                .expect_err("trap CTS case must fail at runtime");
        let expected = std::fs::read_to_string(path.with_extension("trap"))
            .expect("trap golden must be readable");
        assert_eq!(
            error.to_string(),
            expected.trim_end(),
            "{} trap differs",
            name
        );
    }
}
