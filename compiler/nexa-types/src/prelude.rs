use crate::id::{GenericParamId, NominalTypeId, TypeId};
use crate::store::TypeStore;
use crate::ty::*;
use nexa_symbols::SymbolId;

/// SymbolIds do Prelude necessários para o bootstrap nominal de
/// `Optional`/`Result` (§418-424). O type checker consulta
/// `SemanticIndex::prelude_symbol` para obtê-los.
#[derive(Debug, Clone, Copy)]
pub struct PreludeSymbols {
    pub optional: SymbolId,
    pub result: SymbolId,
    pub some: SymbolId,
    pub none: SymbolId,
    pub success: SymbolId,
    pub failure: SymbolId,
}

/// Bootstrap all primitive and Prelude types into a TypeStore.
/// Returns the TypeIds of key primitive types.
pub struct PreludeTypes {
    pub unit: TypeId,
    pub never: TypeId,
    pub bool: TypeId,
    pub int: TypeId,
    pub uint: TypeId,
    pub int8: TypeId,
    pub int16: TypeId,
    pub int32: TypeId,
    pub int64: TypeId,
    pub uint8: TypeId,
    pub uint16: TypeId,
    pub uint32: TypeId,
    pub uint64: TypeId,
    pub float32: TypeId,
    pub float64: TypeId,
    pub byte: TypeId,
    pub char: TypeId,
    pub string: TypeId,
    pub bytes: TypeId,
}

pub fn bootstrap_prelude(store: &mut TypeStore) -> PreludeTypes {
    let unit = store.intern_type(Type::Unit);
    let never = store.intern_type(Type::Never);
    let bool = store.intern_type(Type::Bool);
    let int = store.intern_type(Type::Int);
    let uint = store.intern_type(Type::UInt);
    let int8 = store.intern_type(Type::Int8);
    let int16 = store.intern_type(Type::Int16);
    let int32 = store.intern_type(Type::Int32);
    let int64 = store.intern_type(Type::Int64);
    let uint8 = store.intern_type(Type::UInt8);
    let uint16 = store.intern_type(Type::UInt16);
    let uint32 = store.intern_type(Type::UInt32);
    let uint64 = store.intern_type(Type::UInt64);
    let float32 = store.intern_type(Type::Float32);
    let float64 = store.intern_type(Type::Float64);
    let byte = store.intern_type(Type::Byte);
    let char = store.intern_type(Type::Char);
    let string = store.intern_type(Type::String);
    let bytes = store.intern_type(Type::Bytes);

    PreludeTypes {
        unit,
        never,
        bool,
        int,
        uint,
        int8,
        int16,
        int32,
        int64,
        uint8,
        uint16,
        uint32,
        uint64,
        float32,
        float64,
        byte,
        char,
        string,
        bytes,
    }
}

/// Nominal structurais do Prelude bootstrapados pelo compilador (§418-424):
/// `Optional<T>` e `Result<T,E>` como enums compiler-known com variantes
/// `Some/None` e `Success/Failure`. Os SymbolIds vêm do resolver (o prelude
/// já registra esses nomes), garantindo identidade nominal correta.
#[derive(Debug, Clone, Copy)]
pub struct PreludeNominalBootstrap {
    /// TypeId do nominal `Optional` (enum, generic T).
    pub optional: TypeId,
    pub optional_nid: NominalTypeId,
    pub optional_t: GenericParamId,
    /// SymbolId do enum `Optional` no prelude do resolver.
    pub optional_symbol: SymbolId,
    /// TypeId do nominal `Result` (enum, generic T, E).
    pub result: TypeId,
    pub result_nid: NominalTypeId,
    pub result_t: GenericParamId,
    pub result_e: GenericParamId,
    /// SymbolId do enum `Result` no prelude do resolver.
    pub result_symbol: SymbolId,
    /// SymbolIds das variantes (usados para reconhecer construtores).
    pub some_symbol: SymbolId,
    pub none_symbol: SymbolId,
    pub success_symbol: SymbolId,
    pub failure_symbol: SymbolId,
}

impl PreludeNominalBootstrap {
    /// `Optional<T>` como tipo aplicado.
    pub fn optional_of(&self, store: &mut TypeStore, t: TypeId) -> TypeId {
        store.create_applied(self.optional, vec![t])
    }

    /// `Result<T,E>` como tipo aplicado.
    pub fn result_of(&self, store: &mut TypeStore, t: TypeId, e: TypeId) -> TypeId {
        store.create_applied(self.result, vec![t, e])
    }
}

/// Registra `Optional<T>` e `Result<T,E>` como enums nominais compiler-known
/// no store (§418-424). As variantes (`Some/None/Success/Failure`) recebem os
/// `SymbolId`s do prelude do resolver, garantindo identidade nominal correta.
pub fn bootstrap_prelude_nominal(
    store: &mut TypeStore,
    symbols: &PreludeSymbols,
) -> PreludeNominalBootstrap {
    // Generic params podem faltar antes do resolver registrar o prelude;
    // bebemos os símbolos de forma conservadora.
    let optional_t = store.create_generic_param(GenericParameterInfo {
        symbol: symbols.optional,
        constraints: Vec::new(),
    });
    let t_ty = store.intern_type(Type::GenericParameter(optional_t));

    let result_t = store.create_generic_param(GenericParameterInfo {
        symbol: symbols.result,
        constraints: Vec::new(),
    });
    let result_e = store.create_generic_param(GenericParameterInfo {
        symbol: symbols.result,
        constraints: Vec::new(),
    });
    let rt_ty = store.intern_type(Type::GenericParameter(result_t));
    let re_ty = store.intern_type(Type::GenericParameter(result_e));

    let (optional, optional_nid) = store.create_nominal(NominalType {
        symbol: symbols.optional,
        kind: NominalTypeKind::Enum,
        generic_params: vec![optional_t],
        package: nexa_symbols::PackageInstanceId(0),
        module: nexa_symbols::ModuleId(0),
    });
    let (result, result_nid) = store.create_nominal(NominalType {
        symbol: symbols.result,
        kind: NominalTypeKind::Enum,
        generic_params: vec![result_t, result_e],
        package: nexa_symbols::PackageInstanceId(0),
        module: nexa_symbols::ModuleId(0),
    });

    let optional_def = TypeDefinition::Enum(EnumTypeDefinition {
        name: "Optional".to_string(),
        ty: optional,
        visibility: nexa_symbols::Visibility::Public,
        symbol: symbols.optional,
        variants: vec![
            VariantDefinition {
                name: "Some".to_string(),
                symbol: symbols.some,
                kind: VariantKind::Tuple(vec![t_ty]),
            },
            VariantDefinition {
                name: "None".to_string(),
                symbol: symbols.none,
                kind: VariantKind::Unit,
            },
        ],
    });
    store.register_nominal_definition(optional_nid, optional_def);

    let result_def = TypeDefinition::Enum(EnumTypeDefinition {
        name: "Result".to_string(),
        ty: result,
        visibility: nexa_symbols::Visibility::Public,
        symbol: symbols.result,
        variants: vec![
            VariantDefinition {
                name: "Success".to_string(),
                symbol: symbols.success,
                kind: VariantKind::Tuple(vec![rt_ty]),
            },
            VariantDefinition {
                name: "Failure".to_string(),
                symbol: symbols.failure,
                kind: VariantKind::Tuple(vec![re_ty]),
            },
        ],
    });
    store.register_nominal_definition(result_nid, result_def);

    PreludeNominalBootstrap {
        optional,
        optional_nid,
        optional_t,
        optional_symbol: symbols.optional,
        result,
        result_nid,
        result_t,
        result_e,
        result_symbol: symbols.result,
        some_symbol: symbols.some,
        none_symbol: symbols.none,
        success_symbol: symbols.success,
        failure_symbol: symbols.failure,
    }
}
