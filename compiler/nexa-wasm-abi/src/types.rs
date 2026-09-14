use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WasmScalarType {
    I32,
    I64,
    F32,
    F64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WasmValueType {
    Scalar(WasmScalarType),
    Unit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AbiTypeMapping {
    Direct(WasmScalarType),
    DescriptorPointer,
    Unit,
    Never,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrimitiveAlignment {
    pub align: u32,
}

impl PrimitiveAlignment {
    pub fn for_i8() -> Self {
        Self { align: 1 }
    }
    pub fn for_u8() -> Self {
        Self { align: 1 }
    }
    pub fn for_i16() -> Self {
        Self { align: 2 }
    }
    pub fn for_u16() -> Self {
        Self { align: 2 }
    }
    pub fn for_i32() -> Self {
        Self { align: 4 }
    }
    pub fn for_u32() -> Self {
        Self { align: 4 }
    }
    pub fn for_i64() -> Self {
        Self { align: 8 }
    }
    pub fn for_u64() -> Self {
        Self { align: 8 }
    }
    pub fn for_f32() -> Self {
        Self { align: 4 }
    }
    pub fn for_f64() -> Self {
        Self { align: 8 }
    }
    pub fn for_pointer() -> Self {
        Self { align: 4 }
    }
    pub fn for_bool() -> Self {
        Self { align: 4 }
    }
    pub fn for_char() -> Self {
        Self { align: 4 }
    }
}

pub fn resolve_abi_type(nexa_type_name: &str) -> AbiTypeMapping {
    match nexa_type_name {
        "Bool" | "bool" => AbiTypeMapping::Direct(WasmScalarType::I32),
        "Int" | "i64" | "Int64" => AbiTypeMapping::Direct(WasmScalarType::I64),
        "UInt" | "u64" | "UInt64" => AbiTypeMapping::Direct(WasmScalarType::I64),
        "Int8" | "i8" => AbiTypeMapping::Direct(WasmScalarType::I32),
        "UInt8" | "u8" => AbiTypeMapping::Direct(WasmScalarType::I32),
        "Int16" | "i16" => AbiTypeMapping::Direct(WasmScalarType::I32),
        "UInt16" | "u16" => AbiTypeMapping::Direct(WasmScalarType::I32),
        "Int32" | "i32" => AbiTypeMapping::Direct(WasmScalarType::I32),
        "UInt32" | "u32" => AbiTypeMapping::Direct(WasmScalarType::I32),
        "Float32" | "f32" => AbiTypeMapping::Direct(WasmScalarType::F32),
        "Float64" | "f64" => AbiTypeMapping::Direct(WasmScalarType::F64),
        "Char" | "char" => AbiTypeMapping::Direct(WasmScalarType::I32),
        "Unit" | "()" => AbiTypeMapping::Unit,
        "Never" | "!" => AbiTypeMapping::Never,
        "String" => AbiTypeMapping::DescriptorPointer,
        "Bytes" => AbiTypeMapping::DescriptorPointer,
        _ => AbiTypeMapping::DescriptorPointer,
    }
}

pub fn align_up(offset: u32, alignment: u32) -> u32 {
    if alignment == 0 {
        return offset;
    }
    let remainder = offset % alignment;
    if remainder == 0 {
        offset
    } else {
        offset + (alignment - remainder)
    }
}

#[derive(Debug, Clone, Default)]
pub struct WasmTypeLayoutTable {
    layouts: HashMap<String, WasmTypeLayout>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmTypeLayout {
    pub size: u32,
    pub align: u32,
    pub kind: WasmTypeLayoutKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WasmTypeLayoutKind {
    Primitive,
    Struct { fields: Vec<FieldLayout> },
    Enum { tag_size: u32, payload_size: u32 },
    Array,
    String,
    Bytes,
    Pointer,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldLayout {
    pub offset: u32,
    pub size: u32,
    pub align: u32,
}

impl WasmTypeLayoutTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, name: String, layout: WasmTypeLayout) {
        self.layouts.insert(name, layout);
    }

    pub fn get(&self, name: &str) -> Option<&WasmTypeLayout> {
        self.layouts.get(name)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.layouts.contains_key(name)
    }

    pub fn len(&self) -> usize {
        self.layouts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.layouts.is_empty()
    }
}
