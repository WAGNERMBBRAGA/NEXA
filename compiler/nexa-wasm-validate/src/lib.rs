use nexa_wasm_abi::imports::is_known_import;
use nexa_wasm_abi::metadata::AbiMetadata;
use nexa_wasm_abi::target::{
    ABI_CUSTOM_SECTION, ABI_VERSION, ENTRYPOINT_EXPORT_NAME, MEMORY_EXPORT_NAME, TARGET_IDENTITY,
};
use wasmparser::{ExternalKind, Parser, Payload};

#[derive(Debug, Clone)]
pub struct WasmArtifact {
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct ValidatedWasmArtifact {
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct ValidationConfig {
    pub target: &'static str,
    pub abi_version: u32,
    pub is_application: bool,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            target: TARGET_IDENTITY,
            abi_version: ABI_VERSION,
            is_application: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WasmValidationError {
    MissingAbiMetadata,
    DuplicateAbiMetadata,
    UnsupportedTarget {
        target: String,
    },
    UnsupportedAbiVersion {
        version: u32,
    },
    UnknownImport {
        module: String,
        name: String,
    },
    InvalidImportSignature {
        module: String,
        name: String,
        details: String,
    },
    MissingMemoryExport,
    InvalidMemoryExport,
    MissingEntrypoint,
    InvalidEntrypointSignature {
        details: String,
    },
    MultipleMemoriesNotAllowed,
    StructuralWasmValidationFailed {
        details: String,
    },
    MalformedExternalArtifact,
}

impl WasmValidationError {
    pub fn error_code(&self) -> &'static str {
        match self {
            WasmValidationError::MissingAbiMetadata => "NEXA-HOST-0002",
            WasmValidationError::DuplicateAbiMetadata => "NEXA-HOST-0003",
            WasmValidationError::UnsupportedTarget { .. } => "NEXA-HOST-0004",
            WasmValidationError::UnsupportedAbiVersion { .. } => "NEXA-HOST-0005",
            WasmValidationError::UnknownImport { .. } => "NEXA-HOST-0006",
            WasmValidationError::InvalidImportSignature { .. } => "NEXA-HOST-0008",
            WasmValidationError::MissingMemoryExport => "NEXA-HOST-0010",
            WasmValidationError::InvalidMemoryExport => "NEXA-HOST-0011",
            WasmValidationError::MissingEntrypoint => "NEXA-HOST-0012",
            WasmValidationError::InvalidEntrypointSignature { .. } => "NEXA-HOST-0013",
            WasmValidationError::MultipleMemoriesNotAllowed => "NEXA-HOST-0014",
            WasmValidationError::StructuralWasmValidationFailed { .. } => "NEXA-HOST-0015",
            WasmValidationError::MalformedExternalArtifact => "NEXA-HOST-0016",
        }
    }

    pub fn is_error(&self) -> bool {
        true
    }
}

impl std::fmt::Display for WasmValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WasmValidationError::MissingAbiMetadata => {
                write!(f, "[NEXA-HOST-0002] Missing NEXA ABI metadata section")
            }
            WasmValidationError::DuplicateAbiMetadata => {
                write!(
                    f,
                    "[NEXA-HOST-0003] Duplicate NEXA ABI metadata sections found"
                )
            }
            WasmValidationError::UnsupportedTarget { target } => {
                write!(f, "[NEXA-HOST-0004] Unsupported target: {}", target)
            }
            WasmValidationError::UnsupportedAbiVersion { version } => {
                write!(f, "[NEXA-HOST-0005] Unsupported ABI version: {}", version)
            }
            WasmValidationError::UnknownImport { module, name } => {
                write!(f, "[NEXA-HOST-0006] Unknown import: {}.{}", module, name)
            }
            WasmValidationError::InvalidImportSignature {
                module,
                name,
                details,
            } => {
                write!(
                    f,
                    "[NEXA-HOST-0008] Invalid import signature for {}.{}: {}",
                    module, name, details
                )
            }
            WasmValidationError::MissingMemoryExport => {
                write!(f, "[NEXA-HOST-0010] Missing memory export")
            }
            WasmValidationError::InvalidMemoryExport => {
                write!(f, "[NEXA-HOST-0011] Invalid memory export")
            }
            WasmValidationError::MissingEntrypoint => {
                write!(f, "[NEXA-HOST-0012] Missing entrypoint export")
            }
            WasmValidationError::InvalidEntrypointSignature { details } => {
                write!(
                    f,
                    "[NEXA-HOST-0013] Invalid entrypoint signature: {}",
                    details
                )
            }
            WasmValidationError::MultipleMemoriesNotAllowed => {
                write!(f, "Multiple memories are not allowed")
            }
            WasmValidationError::StructuralWasmValidationFailed { details } => {
                write!(f, "Structural WASM validation failed: {}", details)
            }
            WasmValidationError::MalformedExternalArtifact => {
                write!(f, "Malformed external artifact")
            }
        }
    }
}

impl std::error::Error for WasmValidationError {}

pub fn validate_wasm_artifact(
    artifact: &WasmArtifact,
    config: &ValidationConfig,
) -> Result<ValidatedWasmArtifact, WasmValidationError> {
    wasmparser::validate(&artifact.bytes).map_err(|e| {
        WasmValidationError::StructuralWasmValidationFailed {
            details: e.to_string(),
        }
    })?;

    let mut abi_section_count: u32 = 0;
    let mut memory_count: u32 = 0;
    let mut memory_export_found = false;
    let mut memory_export_invalid = false;
    let mut entrypoint_found = false;
    let mut entrypoint_invalid = false;
    let mut custom_sections: Vec<Vec<u8>> = Vec::new();

    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        let payload = payload.map_err(|e| WasmValidationError::StructuralWasmValidationFailed {
            details: e.to_string(),
        })?;

        match payload {
            Payload::CustomSection(section) => {
                if section.name() == ABI_CUSTOM_SECTION {
                    abi_section_count += 1;
                    custom_sections.push(section.data().to_vec());
                }
            }
            Payload::MemorySection(reader) => {
                for _ in reader {
                    memory_count += 1;
                }
            }
            Payload::ExportSection(reader) => {
                for export in reader {
                    let export = export.map_err(|e| {
                        WasmValidationError::StructuralWasmValidationFailed {
                            details: e.to_string(),
                        }
                    })?;
                    match export.name {
                        name if name == MEMORY_EXPORT_NAME => {
                            if export.kind == ExternalKind::Memory {
                                memory_export_found = true;
                            } else {
                                memory_export_invalid = true;
                            }
                        }
                        name if name == ENTRYPOINT_EXPORT_NAME => {
                            entrypoint_found = true;
                            if export.kind != ExternalKind::Func {
                                entrypoint_invalid = true;
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    if memory_count > 1 {
        return Err(WasmValidationError::MultipleMemoriesNotAllowed);
    }

    if !memory_export_found {
        if memory_export_invalid {
            return Err(WasmValidationError::InvalidMemoryExport);
        }
        return Err(WasmValidationError::MissingMemoryExport);
    }

    if config.is_application && !entrypoint_found {
        return Err(WasmValidationError::MissingEntrypoint);
    }

    if entrypoint_found && entrypoint_invalid {
        return Err(WasmValidationError::InvalidEntrypointSignature {
            details: "Entry point must be a function export".to_string(),
        });
    }

    if config.is_application && entrypoint_found {
        validate_entrypoint_signature(&artifact.bytes)?;
    }

    if abi_section_count == 0 {
        return Err(WasmValidationError::MissingAbiMetadata);
    }
    if abi_section_count > 1 {
        return Err(WasmValidationError::DuplicateAbiMetadata);
    }

    let abi_data = &custom_sections[0];
    let metadata =
        AbiMetadata::deserialize(abi_data).ok_or(WasmValidationError::MalformedExternalArtifact)?;

    if metadata.target != config.target {
        return Err(WasmValidationError::UnsupportedTarget {
            target: metadata.target,
        });
    }

    if metadata.abi_version != config.abi_version {
        return Err(WasmValidationError::UnsupportedAbiVersion {
            version: metadata.abi_version,
        });
    }

    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        let payload = payload.map_err(|e| WasmValidationError::StructuralWasmValidationFailed {
            details: e.to_string(),
        })?;

        if let Payload::ImportSection(reader) = payload {
            for import_result in reader.into_imports() {
                let import = import_result.map_err(|e| {
                    WasmValidationError::StructuralWasmValidationFailed {
                        details: e.to_string(),
                    }
                })?;

                if !is_known_import(import.module, import.name) {
                    return Err(WasmValidationError::UnknownImport {
                        module: import.module.to_string(),
                        name: import.name.to_string(),
                    });
                }
            }
        }
    }

    Ok(ValidatedWasmArtifact {
        bytes: artifact.bytes.clone(),
    })
}

fn validate_entrypoint_signature(bytes: &[u8]) -> Result<(), WasmValidationError> {
    let mut type_section_types: Vec<(Vec<wasmparser::ValType>, Vec<wasmparser::ValType>)> =
        Vec::new();
    let mut func_type_indices: Vec<u32> = Vec::new();
    let mut entrypoint_func_index: Option<u32> = None;

    for payload in Parser::new(0).parse_all(bytes) {
        let payload = payload.map_err(|e| WasmValidationError::StructuralWasmValidationFailed {
            details: e.to_string(),
        })?;

        match payload {
            Payload::TypeSection(reader) => {
                for rec_group_result in reader {
                    let rec_group = rec_group_result.map_err(|e| {
                        WasmValidationError::StructuralWasmValidationFailed {
                            details: e.to_string(),
                        }
                    })?;
                    for sub_type in rec_group.types() {
                        if let Ok(func_type) =
                            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                sub_type.composite_type.unwrap_func()
                            }))
                        {
                            let ft = func_type;
                            type_section_types.push((ft.params().to_vec(), ft.results().to_vec()));
                        } else {
                            type_section_types.push((vec![], vec![]));
                        }
                    }
                }
            }
            Payload::FunctionSection(reader) => {
                for func in reader {
                    let func =
                        func.map_err(|e| WasmValidationError::StructuralWasmValidationFailed {
                            details: e.to_string(),
                        })?;
                    func_type_indices.push(func);
                }
            }
            Payload::ImportSection(reader) => {
                for import in reader.into_imports() {
                    let import = import.map_err(|e| {
                        WasmValidationError::StructuralWasmValidationFailed {
                            details: e.to_string(),
                        }
                    })?;
                    if let wasmparser::TypeRef::Func(type_index) = import.ty {
                        func_type_indices.push(type_index);
                    }
                }
            }
            Payload::ExportSection(reader) => {
                for export in reader {
                    let export = export.map_err(|e| {
                        WasmValidationError::StructuralWasmValidationFailed {
                            details: e.to_string(),
                        }
                    })?;
                    if export.name == ENTRYPOINT_EXPORT_NAME && export.kind == ExternalKind::Func {
                        entrypoint_func_index = Some(export.index);
                    }
                }
            }
            _ => {}
        }
    }

    if let Some(func_idx) = entrypoint_func_index {
        if (func_idx as usize) < func_type_indices.len() {
            let type_idx = func_type_indices[func_idx as usize] as usize;
            if type_idx < type_section_types.len() {
                let (params, results) = &type_section_types[type_idx];
                if !params.is_empty() || !results.is_empty() {
                    return Err(WasmValidationError::InvalidEntrypointSignature {
                        details: format!(
                            "Expected () -> (), got ({}) -> ({})",
                            params
                                .iter()
                                .map(|p| format!("{:?}", p))
                                .collect::<Vec<_>>()
                                .join(", "),
                            results
                                .iter()
                                .map(|r| format!("{:?}", r))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                    });
                }
                return Ok(());
            }
        }
        return Err(WasmValidationError::InvalidEntrypointSignature {
            details: "Could not resolve entrypoint type".to_string(),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;
    use wasm_encoder::{
        CodeSection, CustomSection, ExportKind, ExportSection, Function, FunctionSection,
        ImportSection, MemorySection, MemoryType, Module, TypeSection, ValType,
    };

    fn make_abi_section() -> Vec<u8> {
        let metadata = AbiMetadata::new("test_fp");
        metadata.serialize()
    }

    fn abi_custom_section<'a>(bytes: &'a [u8]) -> CustomSection<'a> {
        CustomSection {
            name: Cow::Borrowed(ABI_CUSTOM_SECTION),
            data: Cow::Borrowed(bytes),
        }
    }

    fn build_module(builder: impl FnOnce(&mut Module)) -> Vec<u8> {
        let mut module = Module::new();
        builder(&mut module);
        module.finish()
    }

    fn empty_function() -> Function {
        let mut f = Function::new([]);
        f.instructions().end();
        f
    }

    fn build_minimal_module_with_abi(
        has_memory_export: bool,
        has_entrypoint: bool,
        entrypoint_type_idx: Option<u32>,
    ) -> Vec<u8> {
        build_module(|module| {
            let mut types = TypeSection::new();
            types.ty().function([], []);
            if let Some(1) = entrypoint_type_idx {
                types.ty().function([ValType::I32, ValType::I32], []);
            }
            module.section(&types);

            let mut funcs = FunctionSection::new();
            funcs.function(0);
            if let Some(1) = entrypoint_type_idx {
                funcs.function(1);
            }
            module.section(&funcs);

            let mut memories = MemorySection::new();
            memories.memory(MemoryType {
                minimum: 1,
                maximum: None,
                memory64: false,
                shared: false,
                page_size_log2: None,
            });
            module.section(&memories);

            let mut exports = ExportSection::new();
            if has_memory_export {
                exports.export("memory", ExportKind::Memory, 0);
            }
            if has_entrypoint {
                let idx = entrypoint_type_idx.unwrap_or(0);
                exports.export("__nexa_main", ExportKind::Func, idx);
            }
            module.section(&exports);

            let mut code = CodeSection::new();
            code.function(&empty_function());
            if let Some(1) = entrypoint_type_idx {
                code.function(&empty_function());
            }
            module.section(&code);

            let abi_bytes = make_abi_section();
            module.section(&abi_custom_section(&abi_bytes));
        })
    }

    fn build_valid_application_wasm() -> Vec<u8> {
        build_minimal_module_with_abi(true, true, None)
    }

    fn build_valid_library_wasm() -> Vec<u8> {
        build_minimal_module_with_abi(true, false, None)
    }

    fn default_config() -> ValidationConfig {
        ValidationConfig::default()
    }

    #[test]
    fn test_valid_empty_module_passes() {
        let bytes = build_valid_library_wasm();
        let artifact = WasmArtifact { bytes };
        let config = ValidationConfig {
            is_application: false,
            ..default_config()
        };
        let result = validate_wasm_artifact(&artifact, &config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_valid_application_module_passes() {
        let bytes = build_valid_application_wasm();
        let artifact = WasmArtifact { bytes };
        let result = validate_wasm_artifact(&artifact, &default_config());
        assert!(result.is_ok());
    }

    #[test]
    fn test_missing_abi_metadata_rejected() {
        let bytes = build_module(|module| {
            let mut types = TypeSection::new();
            types.ty().function([], []);
            module.section(&types);

            let mut funcs = FunctionSection::new();
            funcs.function(0);
            module.section(&funcs);

            let mut memories = MemorySection::new();
            memories.memory(MemoryType {
                minimum: 1,
                maximum: None,
                memory64: false,
                shared: false,
                page_size_log2: None,
            });
            module.section(&memories);

            let mut exports = ExportSection::new();
            exports.export("memory", ExportKind::Memory, 0);
            exports.export("__nexa_main", ExportKind::Func, 0);
            module.section(&exports);

            let mut code = CodeSection::new();
            code.function(&empty_function());
            module.section(&code);
        });

        let artifact = WasmArtifact { bytes };
        let result = validate_wasm_artifact(&artifact, &default_config());
        assert!(matches!(
            result,
            Err(WasmValidationError::MissingAbiMetadata)
        ));
    }

    #[test]
    fn test_duplicate_abi_metadata_rejected() {
        let bytes = build_module(|module| {
            let mut types = TypeSection::new();
            types.ty().function([], []);
            module.section(&types);

            let mut funcs = FunctionSection::new();
            funcs.function(0);
            module.section(&funcs);

            let mut memories = MemorySection::new();
            memories.memory(MemoryType {
                minimum: 1,
                maximum: None,
                memory64: false,
                shared: false,
                page_size_log2: None,
            });
            module.section(&memories);

            let mut exports = ExportSection::new();
            exports.export("memory", ExportKind::Memory, 0);
            exports.export("__nexa_main", ExportKind::Func, 0);
            module.section(&exports);

            let mut code = CodeSection::new();
            code.function(&empty_function());
            module.section(&code);

            let abi_bytes = make_abi_section();
            module.section(&abi_custom_section(&abi_bytes));
            module.section(&abi_custom_section(&abi_bytes));
        });

        let artifact = WasmArtifact { bytes };
        let result = validate_wasm_artifact(&artifact, &default_config());
        assert!(matches!(
            result,
            Err(WasmValidationError::DuplicateAbiMetadata)
        ));
    }

    #[test]
    fn test_wrong_target_rejected() {
        let bytes = build_module(|module| {
            let mut types = TypeSection::new();
            types.ty().function([], []);
            module.section(&types);

            let mut funcs = FunctionSection::new();
            funcs.function(0);
            module.section(&funcs);

            let mut memories = MemorySection::new();
            memories.memory(MemoryType {
                minimum: 1,
                maximum: None,
                memory64: false,
                shared: false,
                page_size_log2: None,
            });
            module.section(&memories);

            let mut exports = ExportSection::new();
            exports.export("memory", ExportKind::Memory, 0);
            exports.export("__nexa_main", ExportKind::Func, 0);
            module.section(&exports);

            let mut code = CodeSection::new();
            code.function(&empty_function());
            module.section(&code);

            let mut metadata = AbiMetadata::new("test_fp");
            metadata.target = "wrong-target".to_string();
            let abi_bytes = metadata.serialize();
            module.section(&abi_custom_section(&abi_bytes));
        });

        let artifact = WasmArtifact { bytes };
        let result = validate_wasm_artifact(&artifact, &default_config());
        assert!(matches!(
            result,
            Err(WasmValidationError::UnsupportedTarget { .. })
        ));
    }

    #[test]
    fn test_unsupported_abi_version_rejected() {
        let bytes = build_module(|module| {
            let mut types = TypeSection::new();
            types.ty().function([], []);
            module.section(&types);

            let mut funcs = FunctionSection::new();
            funcs.function(0);
            module.section(&funcs);

            let mut memories = MemorySection::new();
            memories.memory(MemoryType {
                minimum: 1,
                maximum: None,
                memory64: false,
                shared: false,
                page_size_log2: None,
            });
            module.section(&memories);

            let mut exports = ExportSection::new();
            exports.export("memory", ExportKind::Memory, 0);
            exports.export("__nexa_main", ExportKind::Func, 0);
            module.section(&exports);

            let mut code = CodeSection::new();
            code.function(&empty_function());
            module.section(&code);

            let mut metadata = AbiMetadata::new("test_fp");
            metadata.abi_version = 999;
            let abi_bytes = metadata.serialize();
            module.section(&abi_custom_section(&abi_bytes));
        });

        let artifact = WasmArtifact { bytes };
        let result = validate_wasm_artifact(&artifact, &default_config());
        assert!(matches!(
            result,
            Err(WasmValidationError::UnsupportedAbiVersion { version: 999 })
        ));
    }

    #[test]
    fn test_unknown_import_rejected() {
        let bytes = build_module(|module| {
            let mut types = TypeSection::new();
            types.ty().function([], []);
            module.section(&types);

            let mut imports = ImportSection::new();
            imports.import(
                "unknown_module",
                "unknown_fn",
                wasm_encoder::EntityType::Function(0),
            );
            module.section(&imports);

            let mut funcs = FunctionSection::new();
            funcs.function(0);
            module.section(&funcs);

            let mut memories = MemorySection::new();
            memories.memory(MemoryType {
                minimum: 1,
                maximum: None,
                memory64: false,
                shared: false,
                page_size_log2: None,
            });
            module.section(&memories);

            let mut exports = ExportSection::new();
            exports.export("memory", ExportKind::Memory, 0);
            exports.export("__nexa_main", ExportKind::Func, 0);
            module.section(&exports);

            let mut code = CodeSection::new();
            code.function(&empty_function());
            module.section(&code);

            let abi_bytes = make_abi_section();
            module.section(&abi_custom_section(&abi_bytes));
        });

        let artifact = WasmArtifact { bytes };
        let result = validate_wasm_artifact(&artifact, &default_config());
        assert!(matches!(
            result,
            Err(WasmValidationError::UnknownImport { ref module, ref name }) if module == "unknown_module" && name == "unknown_fn"
        ));
    }

    #[test]
    fn test_valid_imports_pass() {
        let bytes = build_module(|module| {
            let mut types = TypeSection::new();
            types.ty().function([], []);
            types.ty().function([ValType::I32, ValType::I32], []);
            module.section(&types);

            let mut imports = ImportSection::new();
            imports.import(
                "nexa",
                "console.write_utf8",
                wasm_encoder::EntityType::Function(1),
            );
            module.section(&imports);

            let mut funcs = FunctionSection::new();
            funcs.function(0);
            module.section(&funcs);

            let mut memories = MemorySection::new();
            memories.memory(MemoryType {
                minimum: 1,
                maximum: None,
                memory64: false,
                shared: false,
                page_size_log2: None,
            });
            module.section(&memories);

            let mut exports = ExportSection::new();
            exports.export("memory", ExportKind::Memory, 0);
            // Function index 0 is the imported console function; the local
            // `() -> ()` entrypoint is index 1.
            exports.export("__nexa_main", ExportKind::Func, 1);
            module.section(&exports);

            let mut code = CodeSection::new();
            code.function(&empty_function());
            module.section(&code);

            let abi_bytes = make_abi_section();
            module.section(&abi_custom_section(&abi_bytes));
        });

        let artifact = WasmArtifact { bytes };
        let result = validate_wasm_artifact(&artifact, &default_config());
        assert!(result.is_ok());
    }

    #[test]
    fn test_missing_memory_rejected() {
        let bytes = build_module(|module| {
            let mut types = TypeSection::new();
            types.ty().function([], []);
            module.section(&types);

            let mut funcs = FunctionSection::new();
            funcs.function(0);
            module.section(&funcs);

            let mut memories = MemorySection::new();
            memories.memory(MemoryType {
                minimum: 1,
                maximum: None,
                memory64: false,
                shared: false,
                page_size_log2: None,
            });
            module.section(&memories);

            let mut exports = ExportSection::new();
            exports.export("__nexa_main", ExportKind::Func, 0);
            module.section(&exports);

            let mut code = CodeSection::new();
            code.function(&empty_function());
            module.section(&code);

            let abi_bytes = make_abi_section();
            module.section(&abi_custom_section(&abi_bytes));
        });

        let artifact = WasmArtifact { bytes };
        let result = validate_wasm_artifact(&artifact, &default_config());
        assert!(matches!(
            result,
            Err(WasmValidationError::MissingMemoryExport)
        ));
    }

    #[test]
    fn test_invalid_memory_rejected() {
        let bytes = build_module(|module| {
            let mut types = TypeSection::new();
            types.ty().function([], []);
            module.section(&types);

            let mut funcs = FunctionSection::new();
            funcs.function(0);
            module.section(&funcs);

            let mut memories = MemorySection::new();
            memories.memory(MemoryType {
                minimum: 1,
                maximum: None,
                memory64: false,
                shared: false,
                page_size_log2: None,
            });
            module.section(&memories);

            let mut exports = ExportSection::new();
            exports.export("memory", ExportKind::Func, 0);
            exports.export("__nexa_main", ExportKind::Func, 0);
            module.section(&exports);

            let mut code = CodeSection::new();
            code.function(&empty_function());
            module.section(&code);

            let abi_bytes = make_abi_section();
            module.section(&abi_custom_section(&abi_bytes));
        });

        let artifact = WasmArtifact { bytes };
        let result = validate_wasm_artifact(&artifact, &default_config());
        assert!(matches!(
            result,
            Err(WasmValidationError::InvalidMemoryExport)
        ));
    }

    #[test]
    fn test_missing_entrypoint_rejected() {
        let bytes = build_minimal_module_with_abi(true, false, None);
        let artifact = WasmArtifact { bytes };
        let result = validate_wasm_artifact(&artifact, &default_config());
        assert!(matches!(
            result,
            Err(WasmValidationError::MissingEntrypoint)
        ));
    }

    #[test]
    fn test_invalid_entrypoint_signature_rejected() {
        let bytes = build_minimal_module_with_abi(true, true, Some(1));
        let artifact = WasmArtifact { bytes };
        let result = validate_wasm_artifact(&artifact, &default_config());
        assert!(matches!(
            result,
            Err(WasmValidationError::InvalidEntrypointSignature { .. })
        ));
    }

    #[test]
    fn test_multiple_memories_rejected() {
        let bytes = build_module(|module| {
            let mut types = TypeSection::new();
            types.ty().function([], []);
            module.section(&types);

            let mut funcs = FunctionSection::new();
            funcs.function(0);
            module.section(&funcs);

            let mut memories = MemorySection::new();
            memories.memory(MemoryType {
                minimum: 1,
                maximum: None,
                memory64: false,
                shared: false,
                page_size_log2: None,
            });
            memories.memory(MemoryType {
                minimum: 1,
                maximum: None,
                memory64: false,
                shared: false,
                page_size_log2: None,
            });
            module.section(&memories);

            let mut exports = ExportSection::new();
            exports.export("memory", ExportKind::Memory, 0);
            exports.export("__nexa_main", ExportKind::Func, 0);
            module.section(&exports);

            let mut code = CodeSection::new();
            code.function(&empty_function());
            module.section(&code);

            let abi_bytes = make_abi_section();
            module.section(&abi_custom_section(&abi_bytes));
        });

        let artifact = WasmArtifact { bytes };
        let result = validate_wasm_artifact(&artifact, &default_config());
        assert!(matches!(
            result,
            Err(WasmValidationError::MultipleMemoriesNotAllowed)
        ));
    }

    #[test]
    fn test_malformed_wasm_rejected() {
        let artifact = WasmArtifact {
            bytes: vec![0xDE, 0xAD, 0xBE, 0xEF],
        };
        let result = validate_wasm_artifact(&artifact, &default_config());
        assert!(matches!(
            result,
            Err(WasmValidationError::StructuralWasmValidationFailed { .. })
        ));
    }

    #[test]
    fn test_empty_config_defaults() {
        let config = ValidationConfig::default();
        assert_eq!(config.target, TARGET_IDENTITY);
        assert_eq!(config.abi_version, ABI_VERSION);
        assert!(config.is_application);
    }

    #[test]
    fn test_error_codes_all_valid() {
        let errors = vec![
            WasmValidationError::MissingAbiMetadata,
            WasmValidationError::DuplicateAbiMetadata,
            WasmValidationError::UnsupportedTarget {
                target: "test".to_string(),
            },
            WasmValidationError::UnsupportedAbiVersion { version: 1 },
            WasmValidationError::UnknownImport {
                module: "a".to_string(),
                name: "b".to_string(),
            },
            WasmValidationError::InvalidImportSignature {
                module: "a".to_string(),
                name: "b".to_string(),
                details: "c".to_string(),
            },
            WasmValidationError::MissingMemoryExport,
            WasmValidationError::InvalidMemoryExport,
            WasmValidationError::MissingEntrypoint,
            WasmValidationError::InvalidEntrypointSignature {
                details: "d".to_string(),
            },
            WasmValidationError::MultipleMemoriesNotAllowed,
            WasmValidationError::StructuralWasmValidationFailed {
                details: "e".to_string(),
            },
            WasmValidationError::MalformedExternalArtifact,
        ];

        for err in &errors {
            assert!(
                err.error_code().starts_with("NEXA-"),
                "Error code {} does not start with NEXA-",
                err.error_code()
            );
            assert!(err.is_error());
        }
    }

    #[test]
    fn test_validated_artifact_preserves_bytes() {
        let bytes = build_valid_application_wasm();
        let original = bytes.clone();
        let artifact = WasmArtifact { bytes };
        let validated = validate_wasm_artifact(&artifact, &default_config()).unwrap();
        assert_eq!(validated.bytes, original);
    }
}
