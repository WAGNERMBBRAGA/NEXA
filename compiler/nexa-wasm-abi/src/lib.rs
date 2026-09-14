pub mod descriptors;
pub mod imports;
pub mod metadata;
pub mod target;
pub mod types;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_target_constants() {
        assert_eq!(target::TARGET_IDENTITY, "wasm32-nexa");
        assert_eq!(target::ABI_VERSION, 1);
        assert_eq!(target::LANGUAGE_PROFILE_VERSION, "1.0");
        assert_eq!(target::ABI_CUSTOM_SECTION, "nexa.abi");
        assert_eq!(target::SOURCE_MAP_CUSTOM_SECTION, "nexa.source_map");
        assert_eq!(target::MEMORY_EXPORT_NAME, "memory");
        assert_eq!(target::ENTRYPOINT_EXPORT_NAME, "__nexa_main");
        assert_eq!(target::MAGIC, b"NEXA");
    }

    #[test]
    fn test_build_profile_variants() {
        assert_ne!(target::BuildProfile::Debug, target::BuildProfile::Release);
    }

    #[test]
    fn test_output_kind_variants() {
        assert_ne!(target::OutputKind::Application, target::OutputKind::Library);
    }

    #[test]
    fn test_align_up() {
        assert_eq!(types::align_up(0, 4), 0);
        assert_eq!(types::align_up(1, 4), 4);
        assert_eq!(types::align_up(4, 4), 4);
        assert_eq!(types::align_up(5, 8), 8);
        assert_eq!(types::align_up(8, 8), 8);
        assert_eq!(types::align_up(9, 8), 16);
        assert_eq!(types::align_up(3, 1), 3);
    }

    #[test]
    fn test_resolve_abi_types() {
        assert_eq!(
            types::resolve_abi_type("Bool"),
            types::AbiTypeMapping::Direct(types::WasmScalarType::I32)
        );
        assert_eq!(
            types::resolve_abi_type("Int"),
            types::AbiTypeMapping::Direct(types::WasmScalarType::I64)
        );
        assert_eq!(
            types::resolve_abi_type("Float32"),
            types::AbiTypeMapping::Direct(types::WasmScalarType::F32)
        );
        assert_eq!(
            types::resolve_abi_type("Float64"),
            types::AbiTypeMapping::Direct(types::WasmScalarType::F64)
        );
        assert_eq!(
            types::resolve_abi_type("String"),
            types::AbiTypeMapping::DescriptorPointer
        );
        assert_eq!(
            types::resolve_abi_type("Bytes"),
            types::AbiTypeMapping::DescriptorPointer
        );
        assert_eq!(types::resolve_abi_type("Unit"), types::AbiTypeMapping::Unit);
        assert_eq!(
            types::resolve_abi_type("Never"),
            types::AbiTypeMapping::Never
        );
        assert_eq!(
            types::resolve_abi_type("Int8"),
            types::AbiTypeMapping::Direct(types::WasmScalarType::I32)
        );
        assert_eq!(
            types::resolve_abi_type("UInt8"),
            types::AbiTypeMapping::Direct(types::WasmScalarType::I32)
        );
        assert_eq!(
            types::resolve_abi_type("Int32"),
            types::AbiTypeMapping::Direct(types::WasmScalarType::I32)
        );
        assert_eq!(
            types::resolve_abi_type("UInt32"),
            types::AbiTypeMapping::Direct(types::WasmScalarType::I32)
        );
        assert_eq!(
            types::resolve_abi_type("Int64"),
            types::AbiTypeMapping::Direct(types::WasmScalarType::I64)
        );
    }

    #[test]
    fn test_type_layout_table() {
        let mut table = types::WasmTypeLayoutTable::new();
        assert!(table.is_empty());
        table.insert(
            "Int".into(),
            types::WasmTypeLayout {
                size: 8,
                align: 8,
                kind: types::WasmTypeLayoutKind::Primitive,
            },
        );
        assert_eq!(table.len(), 1);
        assert!(table.contains("Int"));
        assert!(!table.contains("String"));
        let layout = table.get("Int").unwrap();
        assert_eq!(layout.size, 8);
        assert_eq!(layout.align, 8);
    }

    #[test]
    fn test_string_flag() {
        assert_eq!(descriptors::StringFlag::Static.to_i32(), 0);
        assert_eq!(descriptors::StringFlag::Owned.to_i32(), 1);
        assert_eq!(
            descriptors::StringFlag::from_i32(0),
            Some(descriptors::StringFlag::Static)
        );
        assert_eq!(
            descriptors::StringFlag::from_i32(1),
            Some(descriptors::StringFlag::Owned)
        );
        assert_eq!(descriptors::StringFlag::from_i32(2), None);
    }

    #[test]
    fn test_string_descriptor() {
        let d = descriptors::StringDescriptor::new_static(5);
        assert_eq!(d.byte_len, 5);
        assert_eq!(d.flag, descriptors::StringFlag::Static);
        assert_eq!(
            d.total_size(),
            descriptors::STRING_DESCRIPTOR_DATA_OFFSET + 5
        );

        let d2 = descriptors::StringDescriptor::new_owned(10);
        assert_eq!(d2.flag, descriptors::StringFlag::Owned);
        assert!(descriptors::is_static_string(d.flag));
        assert!(!descriptors::is_owned_string(d.flag));
        assert!(descriptors::is_owned_string(d2.flag));
    }

    #[test]
    fn test_array_descriptor() {
        let d = descriptors::ArrayDescriptor::new(100, 5, 10);
        assert_eq!(d.data_ptr, 100);
        assert_eq!(d.len, 5);
        assert_eq!(d.capacity, 10);
        assert_eq!(descriptors::ArrayDescriptor::total_size(), 12);
    }

    #[test]
    fn test_string_data_ptr() {
        assert_eq!(descriptors::string_data_ptr(0), 8);
        assert_eq!(descriptors::string_data_ptr(100), 108);
    }

    #[test]
    fn test_string_byte_len_from_memory() {
        let mut mem = vec![0u8; 64];
        let ptr = 16u32;
        let len_bytes = 42u32.to_le_bytes();
        mem[ptr as usize..ptr as usize + 4].copy_from_slice(&len_bytes);
        assert_eq!(descriptors::string_byte_len(ptr, &mem), Some(42));
    }

    #[test]
    fn test_string_flag_from_memory() {
        let mut mem = vec![0u8; 64];
        let ptr = 16u32;
        let flag_bytes = 1i32.to_le_bytes();
        mem[(ptr + descriptors::STRING_DESCRIPTOR_FLAGS_OFFSET) as usize
            ..(ptr + descriptors::STRING_DESCRIPTOR_FLAGS_OFFSET) as usize + 4]
            .copy_from_slice(&flag_bytes);
        assert_eq!(
            descriptors::string_flag_from_memory(ptr, &mem),
            Some(descriptors::StringFlag::Owned)
        );
    }

    #[test]
    fn test_string_byte_len_out_of_bounds() {
        let mem = vec![0u8; 4];
        assert_eq!(descriptors::string_byte_len(100, &mem), None);
    }

    #[test]
    fn test_runtime_imports() {
        let imports = imports::nexa_runtime_imports();
        assert_eq!(imports.len(), 4);
        assert!(imports.iter().all(|i| i.module == "nexa"));
        assert!(imports
            .iter()
            .any(|i| i.name == imports::CONSOLE_WRITE_UTF8));
        assert!(imports.iter().any(|i| i.name == imports::RUNTIME_TRAP));
        assert!(imports
            .iter()
            .any(|i| i.name == imports::RUNTIME_CONTRACT_FAIL));
        assert!(imports.iter().any(|i| i.name == imports::RUNTIME_PANIC));
    }

    #[test]
    fn test_import_registry() {
        let mut reg = imports::RuntimeImportRegistry::new();
        assert!(reg.is_empty());
        let sig = imports::ImportSignature::new("nexa", "test_fn", vec![], vec![]);
        reg.register(sig);
        assert_eq!(reg.len(), 1);
        assert!(reg.is_required("test_fn"));
        assert!(!reg.is_required("other_fn"));
        assert!(reg.get("test_fn").is_some());
    }

    #[test]
    fn test_is_known_import() {
        assert!(imports::is_known_import(
            "nexa",
            imports::CONSOLE_WRITE_UTF8
        ));
        assert!(imports::is_known_import("nexa", imports::RUNTIME_TRAP));
        assert!(!imports::is_known_import("nexa", "unknown_function"));
        assert!(!imports::is_known_import(
            "other",
            imports::CONSOLE_WRITE_UTF8
        ));
    }

    #[test]
    fn test_entrypoint_signature() {
        let ep = imports::entrypoint_signature();
        assert_eq!(ep.module, "nexa");
        assert_eq!(ep.name, "__nexa_main");
        assert!(ep.params.is_empty());
        assert!(ep.results.is_empty());
    }

    #[test]
    fn test_abi_metadata_roundtrip() {
        let m = metadata::AbiMetadata::new("fp_abc123");
        let bytes = m.serialize();
        let d = metadata::AbiMetadata::deserialize(&bytes).unwrap();
        assert_eq!(d.magic, *target::MAGIC);
        assert_eq!(d.abi_version, 1);
        assert_eq!(d.target, "wasm32-nexa");
        assert_eq!(d.language_profile, "1.0");
        assert_eq!(d.runtime_requirements_fingerprint, "fp_abc123");
    }

    #[test]
    fn test_abi_metadata_deserialize_bad_magic() {
        let data = b"XXXX";
        assert!(metadata::AbiMetadata::deserialize(data).is_none());
    }

    #[test]
    fn test_abi_metadata_deserialize_truncated() {
        let data = b"NEXA";
        assert!(metadata::AbiMetadata::deserialize(data).is_none());
    }

    #[test]
    fn test_artifact_metadata_default() {
        let m = metadata::WasmArtifactMetadata::new();
        assert_eq!(m.abi_version, 1);
        assert_eq!(m.target, "wasm32-nexa");
    }

    #[test]
    fn test_capability_set() {
        let b = metadata::CapabilitySet::baseline();
        assert!(b.console_write);
        assert!(!b.runtime_tasks);
        let f = metadata::CapabilitySet::full();
        assert!(f.console_write);
        assert!(f.runtime_tasks);
    }
}
