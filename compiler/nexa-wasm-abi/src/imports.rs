use std::collections::HashMap;

pub const CONSOLE_WRITE_UTF8: &str = "console.write_utf8";
pub const RUNTIME_TRAP: &str = "runtime.trap";
pub const RUNTIME_CONTRACT_FAIL: &str = "runtime.contract_fail";
pub const RUNTIME_PANIC: &str = "runtime.panic";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ImportSignature {
    pub module: String,
    pub name: String,
    pub params: Vec<WasmValType>,
    pub results: Vec<WasmValType>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WasmValType {
    I32,
    I64,
    F32,
    F64,
}

impl ImportSignature {
    pub fn new(
        module: &str,
        name: &str,
        params: Vec<WasmValType>,
        results: Vec<WasmValType>,
    ) -> Self {
        Self {
            module: module.to_string(),
            name: name.to_string(),
            params,
            results,
        }
    }
}

pub fn nexa_runtime_imports() -> Vec<ImportSignature> {
    vec![
        ImportSignature::new(
            "nexa",
            CONSOLE_WRITE_UTF8,
            vec![WasmValType::I32, WasmValType::I32],
            vec![],
        ),
        ImportSignature::new(
            "nexa",
            RUNTIME_TRAP,
            vec![WasmValType::I32, WasmValType::I32],
            vec![],
        ),
        ImportSignature::new(
            "nexa",
            RUNTIME_CONTRACT_FAIL,
            vec![WasmValType::I32, WasmValType::I32],
            vec![],
        ),
        ImportSignature::new(
            "nexa",
            RUNTIME_PANIC,
            vec![WasmValType::I32, WasmValType::I32],
            vec![],
        ),
    ]
}

#[derive(Debug, Clone, Default)]
pub struct RuntimeImportRegistry {
    required: HashMap<String, ImportSignature>,
}

impl RuntimeImportRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, sig: ImportSignature) {
        self.required.insert(sig.name.clone(), sig);
    }

    pub fn register_all(&mut self, sigs: Vec<ImportSignature>) {
        for sig in sigs {
            self.register(sig);
        }
    }

    pub fn is_required(&self, name: &str) -> bool {
        self.required.contains_key(name)
    }

    pub fn get(&self, name: &str) -> Option<&ImportSignature> {
        self.required.get(name)
    }

    pub fn required_imports(&self) -> Vec<&ImportSignature> {
        self.required.values().collect()
    }

    pub fn len(&self) -> usize {
        self.required.len()
    }

    pub fn is_empty(&self) -> bool {
        self.required.is_empty()
    }

    pub fn names(&self) -> Vec<&str> {
        self.required.keys().map(|s| s.as_str()).collect()
    }
}

pub fn entrypoint_signature() -> ImportSignature {
    ImportSignature::new("nexa", "__nexa_main", vec![], vec![])
}

pub fn is_known_import(module: &str, name: &str) -> bool {
    if module != "nexa" {
        return false;
    }
    matches!(
        name,
        CONSOLE_WRITE_UTF8 | RUNTIME_TRAP | RUNTIME_CONTRACT_FAIL | RUNTIME_PANIC
    )
}
