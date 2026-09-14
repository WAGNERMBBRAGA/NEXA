// ── Core NIR IDs ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NirTypeId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NirFunctionId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NirConstantId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NirGlobalId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IntrinsicId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceLocationId(pub u32);

// ── NIR Type system ─────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NirType {
    Unit,
    Never,
    Bool,
    Int { signed: bool, bits: u16 },
    Float { bits: u16 },
    Char,
    String,
    Bytes,
    Struct(NirStructType),
    Enum(NirEnumType),
    Distinct(NirDistinctType),
    Ref { mutable: bool, target: NirTypeId },
    Array(NirTypeId),
    Interface(NirInterfaceType),
    Task(NirTypeId),
    Callable(NirCallableType),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NirStructType {
    pub name: String,
    pub fields: Vec<NirStructField>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NirStructField {
    pub name: String,
    pub ty: NirTypeId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NirEnumType {
    pub name: String,
    pub variants: Vec<NirEnumVariant>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NirEnumVariant {
    pub name: String,
    pub fields: Vec<NirTypeId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NirDistinctType {
    pub name: String,
    pub underlying: NirTypeId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NirInterfaceType {
    pub name: String,
    pub methods: Vec<NirCallableSignature>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NirCallableType {
    pub kind: NirCallableKind,
    pub parameters: Vec<NirParameter>,
    pub return_type: NirTypeId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NirCallableKind {
    Function,
    Action,
    AsyncAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NirParameter {
    pub ty: NirTypeId,
    pub passing: NirPassingMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NirPassingMode {
    Owned,
    Ref,
    MutRef,
}

// ── Effect representation ───────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NirEffectSet {
    pub effects: Vec<String>,
}

impl NirEffectSet {
    pub fn empty() -> Self {
        Self { effects: vec![] }
    }

    pub fn is_empty(&self) -> bool {
        self.effects.is_empty()
    }
}

// ── Callable Signature ──────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NirCallableSignature {
    pub kind: NirCallableKind,
    pub parameters: Vec<NirParameter>,
    pub return_type: NirTypeId,
    pub effects: NirEffectSet,
}

// ── Constant Pool ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum NirConstant {
    Integer(i128),
    FloatBits(u64),
    StringUtf8(String),
    Bytes(Vec<u8>),
    Char(char),
}

#[derive(Debug, Clone, Default)]
pub struct ConstantPool {
    constants: Vec<NirConstant>,
}

impl ConstantPool {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, c: NirConstant) -> NirConstantId {
        let id = NirConstantId(self.constants.len() as u32);
        self.constants.push(c);
        id
    }

    pub fn get(&self, id: NirConstantId) -> Option<&NirConstant> {
        self.constants.get(id.0 as usize)
    }

    pub fn len(&self) -> usize {
        self.constants.len()
    }

    pub fn is_empty(&self) -> bool {
        self.constants.is_empty()
    }
}

// ── Type Table ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct TypeTable {
    types: Vec<NirType>,
}

impl TypeTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, t: NirType) -> NirTypeId {
        let id = NirTypeId(self.types.len() as u32);
        self.types.push(t);
        id
    }

    pub fn get(&self, id: NirTypeId) -> Option<&NirType> {
        self.types.get(id.0 as usize)
    }

    pub fn len(&self) -> usize {
        self.types.len()
    }

    pub fn is_empty(&self) -> bool {
        self.types.is_empty()
    }
}

// ── Function Record ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct NirFunction {
    pub id: NirFunctionId,
    pub signature: NirCallableSignature,
    pub body: Option<NirFunctionBody>,
    pub metadata: NirFunctionMetadata,
}

#[derive(Debug, Clone, Default)]
pub struct NirFunctionMetadata {
    pub source_location: Option<SourceLocationId>,
    pub is_exported: bool,
}

#[derive(Debug, Clone)]
pub struct NirFunctionBody {
    pub blocks: Vec<NirBasicBlock>,
    pub entry: u32,
}

#[derive(Debug, Clone)]
pub struct NirBasicBlock {
    pub id: u32,
    pub instructions: Vec<NirInstruction>,
    pub terminator: NirTerminator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NirScalarBinaryOp {
    Add,
    Sub,
    Mul,
    DivSigned,
    RemSigned,
    Eq,
    Ne,
    LtSigned,
    LeSigned,
    GtSigned,
    GeSigned,
    BitAnd,
    BitOr,
    BitXor,
    ShiftLeft,
    ShiftRightSigned,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NirInstruction {
    Nop,
    ConstI64 {
        dest: u32,
        value: i64,
    },
    BinaryI64 {
        dest: u32,
        op: NirScalarBinaryOp,
        left: u32,
        right: u32,
    },
    Call {
        dest: Option<u32>,
        function: NirFunctionId,
        args: Vec<u32>,
    },
    ConsoleWriteUtf8 {
        text: String,
    },
    LocalRead {
        dest: u32,
        local: u32,
    },
    LocalWrite {
        local: u32,
        value: u32,
    },
    RuntimeTrap {
        kind: i32,
        location: i32,
    },
}

#[derive(Debug, Clone)]
pub enum NirTerminator {
    Return {
        value: Option<u32>,
    },
    Branch {
        target: u32,
    },
    CondBranch {
        condition: u32,
        then_target: u32,
        else_target: u32,
    },
    Unreachable,
}

// ── Function Table ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct FunctionTable {
    functions: Vec<NirFunction>,
}

impl FunctionTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, f: NirFunction) -> NirFunctionId {
        let id = f.id;
        self.functions.push(f);
        id
    }

    pub fn get(&self, id: NirFunctionId) -> Option<&NirFunction> {
        self.functions.iter().find(|f| f.id == id)
    }

    pub fn len(&self) -> usize {
        self.functions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.functions.is_empty()
    }
}

// ── Global Table ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct NirGlobal {
    pub id: NirGlobalId,
    pub ty: NirTypeId,
    pub constant: Option<NirConstantId>,
}

#[derive(Debug, Clone, Default)]
pub struct GlobalTable {
    globals: Vec<NirGlobal>,
}

impl GlobalTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, g: NirGlobal) -> NirGlobalId {
        let id = g.id;
        self.globals.push(g);
        id
    }

    pub fn get(&self, id: NirGlobalId) -> Option<&NirGlobal> {
        self.globals.iter().find(|g| g.id == id)
    }

    pub fn len(&self) -> usize {
        self.globals.len()
    }

    pub fn is_empty(&self) -> bool {
        self.globals.is_empty()
    }
}

// ── Runtime Requirements ────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeCapability {
    Memory,
    ConsoleWrite,
    FileSystem,
    Network,
    TaskRuntime,
    ContractRuntime,
}

#[derive(Debug, Clone, Default)]
pub struct RuntimeRequirements {
    pub capabilities: Vec<RuntimeCapability>,
}

impl RuntimeRequirements {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, cap: RuntimeCapability) {
        if !self.capabilities.contains(&cap) {
            self.capabilities.push(cap);
        }
    }

    pub fn has(&self, cap: &RuntimeCapability) -> bool {
        self.capabilities.contains(cap)
    }

    pub fn is_empty(&self) -> bool {
        self.capabilities.is_empty()
    }
}

// ── Build Provenance ────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct BuildProvenance {
    pub compiler_version: String,
    pub language_profile: String,
    pub nir_version: String,
}

// ── Module Metadata ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct ModuleMetadata {
    pub runtime_requirements: RuntimeRequirements,
    pub build_provenance: BuildProvenance,
}

// ── NirHeader ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NirFormatVersion(pub String);

impl Default for NirFormatVersion {
    fn default() -> Self {
        NirFormatVersion("NIR1".to_string())
    }
}

#[derive(Debug, Clone, Default)]
pub struct NirHeader {
    pub format_version: NirFormatVersion,
}

// ── NirModule ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct NirModule {
    pub header: NirHeader,
    pub types: TypeTable,
    pub constants: ConstantPool,
    pub functions: FunctionTable,
    pub globals: GlobalTable,
    pub metadata: ModuleMetadata,
}

impl NirModule {
    pub fn new() -> Self {
        Self::default()
    }
}

// ── Public Interface ────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct PublicInterface {
    pub exported_types: Vec<String>,
    pub exported_functions: Vec<String>,
}

// ── VerifiedNirModule ───────────────────────────────────────────────────

/// A NIR module that has passed verification. Only this type should be accepted by backends.
#[derive(Debug)]
pub struct VerifiedNirModule(NirModule);

impl VerifiedNirModule {
    /// Wrap a verified module. The caller must ensure verification has passed.
    pub fn new(module: NirModule) -> Self {
        Self(module)
    }

    pub fn inner(&self) -> &NirModule {
        &self.0
    }

    pub fn into_inner(self) -> NirModule {
        self.0
    }
}

// ── Serialization ───────────────────────────────────────────────────────

const MAGIC: &[u8; 4] = b"NIR1";

const SECTION_TYPES: u8 = 0;
const SECTION_CONSTANTS: u8 = 1;
const SECTION_FUNCTIONS: u8 = 2;
const SECTION_GLOBALS: u8 = 3;
const SECTION_METADATA: u8 = 4;

fn write_u16_le(buf: &mut Vec<u8>, v: u16) {
    buf.extend_from_slice(&v.to_le_bytes());
}

fn write_u32_le(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_le_bytes());
}

fn write_u64_le(buf: &mut Vec<u8>, v: u64) {
    buf.extend_from_slice(&v.to_le_bytes());
}

fn write_u128_le(buf: &mut Vec<u8>, v: u128) {
    buf.extend_from_slice(&v.to_le_bytes());
}

fn write_string(buf: &mut Vec<u8>, s: &str) {
    write_u32_le(buf, s.len() as u32);
    buf.extend_from_slice(s.as_bytes());
}

fn read_u16_le(data: &[u8], offset: &mut usize) -> Result<u16, NirDeserError> {
    if *offset + 2 > data.len() {
        return Err(NirDeserError::TruncatedData);
    }
    let v = u16::from_le_bytes([data[*offset], data[*offset + 1]]);
    *offset += 2;
    Ok(v)
}

fn read_u32_le(data: &[u8], offset: &mut usize) -> Result<u32, NirDeserError> {
    if *offset + 4 > data.len() {
        return Err(NirDeserError::TruncatedData);
    }
    let v = u32::from_le_bytes([
        data[*offset],
        data[*offset + 1],
        data[*offset + 2],
        data[*offset + 3],
    ]);
    *offset += 4;
    Ok(v)
}

fn read_u64_le(data: &[u8], offset: &mut usize) -> Result<u64, NirDeserError> {
    if *offset + 8 > data.len() {
        return Err(NirDeserError::TruncatedData);
    }
    let v = u64::from_le_bytes([
        data[*offset],
        data[*offset + 1],
        data[*offset + 2],
        data[*offset + 3],
        data[*offset + 4],
        data[*offset + 5],
        data[*offset + 6],
        data[*offset + 7],
    ]);
    *offset += 8;
    Ok(v)
}

fn read_u128_le(data: &[u8], offset: &mut usize) -> Result<u128, NirDeserError> {
    if *offset + 16 > data.len() {
        return Err(NirDeserError::TruncatedData);
    }
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&data[*offset..*offset + 16]);
    *offset += 16;
    Ok(u128::from_le_bytes(bytes))
}

fn read_string(data: &[u8], offset: &mut usize) -> Result<String, NirDeserError> {
    let len = read_u32_le(data, offset)? as usize;
    if *offset + len > data.len() {
        return Err(NirDeserError::TruncatedData);
    }
    let s = std::str::from_utf8(&data[*offset..*offset + len])
        .map_err(|e| NirDeserError::MalformedData(e.to_string()))?;
    *offset += len;
    Ok(s.to_string())
}

fn serialize_type_id(buf: &mut Vec<u8>, id: NirTypeId) {
    write_u32_le(buf, id.0);
}

fn deserialize_type_id(data: &[u8], offset: &mut usize) -> Result<NirTypeId, NirDeserError> {
    Ok(NirTypeId(read_u32_le(data, offset)?))
}

fn serialize_calable_kind(buf: &mut Vec<u8>, kind: NirCallableKind) {
    let tag: u8 = match kind {
        NirCallableKind::Function => 0,
        NirCallableKind::Action => 1,
        NirCallableKind::AsyncAction => 2,
    };
    buf.push(tag);
}

fn deserialize_callable_kind(
    data: &[u8],
    offset: &mut usize,
) -> Result<NirCallableKind, NirDeserError> {
    if *offset >= data.len() {
        return Err(NirDeserError::TruncatedData);
    }
    let tag = data[*offset];
    *offset += 1;
    match tag {
        0 => Ok(NirCallableKind::Function),
        1 => Ok(NirCallableKind::Action),
        2 => Ok(NirCallableKind::AsyncAction),
        _ => Err(NirDeserError::MalformedData(format!(
            "unknown callable kind tag: {tag}"
        ))),
    }
}

fn serialize_passing_mode(buf: &mut Vec<u8>, mode: NirPassingMode) {
    let tag: u8 = match mode {
        NirPassingMode::Owned => 0,
        NirPassingMode::Ref => 1,
        NirPassingMode::MutRef => 2,
    };
    buf.push(tag);
}

fn deserialize_passing_mode(
    data: &[u8],
    offset: &mut usize,
) -> Result<NirPassingMode, NirDeserError> {
    if *offset >= data.len() {
        return Err(NirDeserError::TruncatedData);
    }
    let tag = data[*offset];
    *offset += 1;
    match tag {
        0 => Ok(NirPassingMode::Owned),
        1 => Ok(NirPassingMode::Ref),
        2 => Ok(NirPassingMode::MutRef),
        _ => Err(NirDeserError::MalformedData(format!(
            "unknown passing mode tag: {tag}"
        ))),
    }
}

fn serialize_nir_type(buf: &mut Vec<u8>, ty: &NirType) {
    let tag: u8 = match ty {
        NirType::Unit => 0,
        NirType::Never => 1,
        NirType::Bool => 2,
        NirType::Int { .. } => 3,
        NirType::Float { .. } => 4,
        NirType::Char => 5,
        NirType::String => 6,
        NirType::Bytes => 7,
        NirType::Struct(_) => 8,
        NirType::Enum(_) => 9,
        NirType::Distinct(_) => 10,
        NirType::Ref { .. } => 11,
        NirType::Array(_) => 12,
        NirType::Interface(_) => 13,
        NirType::Task(_) => 14,
        NirType::Callable(_) => 15,
    };
    buf.push(tag);
    match ty {
        NirType::Unit
        | NirType::Never
        | NirType::Bool
        | NirType::Char
        | NirType::String
        | NirType::Bytes => {}
        NirType::Int { signed, bits } => {
            buf.push(if *signed { 1 } else { 0 });
            write_u16_le(buf, *bits);
        }
        NirType::Float { bits } => {
            write_u16_le(buf, *bits);
        }
        NirType::Struct(st) => {
            write_string(buf, &st.name);
            write_u32_le(buf, st.fields.len() as u32);
            for f in &st.fields {
                write_string(buf, &f.name);
                serialize_type_id(buf, f.ty);
            }
        }
        NirType::Enum(en) => {
            write_string(buf, &en.name);
            write_u32_le(buf, en.variants.len() as u32);
            for v in &en.variants {
                write_string(buf, &v.name);
                write_u32_le(buf, v.fields.len() as u32);
                for fid in &v.fields {
                    serialize_type_id(buf, *fid);
                }
            }
        }
        NirType::Distinct(d) => {
            write_string(buf, &d.name);
            serialize_type_id(buf, d.underlying);
        }
        NirType::Ref { mutable, target } => {
            buf.push(if *mutable { 1 } else { 0 });
            serialize_type_id(buf, *target);
        }
        NirType::Array(inner) => {
            serialize_type_id(buf, *inner);
        }
        NirType::Interface(iface) => {
            write_string(buf, &iface.name);
            write_u32_le(buf, iface.methods.len() as u32);
            for m in &iface.methods {
                serialize_callable_signature(buf, m);
            }
        }
        NirType::Task(inner) => {
            serialize_type_id(buf, *inner);
        }
        NirType::Callable(ct) => {
            serialize_calable_kind(buf, ct.kind);
            write_u32_le(buf, ct.parameters.len() as u32);
            for p in &ct.parameters {
                serialize_type_id(buf, p.ty);
                serialize_passing_mode(buf, p.passing);
            }
            serialize_type_id(buf, ct.return_type);
        }
    }
}

fn deserialize_nir_type(data: &[u8], offset: &mut usize) -> Result<NirType, NirDeserError> {
    if *offset >= data.len() {
        return Err(NirDeserError::TruncatedData);
    }
    let tag = data[*offset];
    *offset += 1;
    match tag {
        0 => Ok(NirType::Unit),
        1 => Ok(NirType::Never),
        2 => Ok(NirType::Bool),
        3 => {
            let signed = read_u8(data, offset)? != 0;
            let bits = read_u16_le(data, offset)?;
            Ok(NirType::Int { signed, bits })
        }
        4 => {
            let bits = read_u16_le(data, offset)?;
            Ok(NirType::Float { bits })
        }
        5 => Ok(NirType::Char),
        6 => Ok(NirType::String),
        7 => Ok(NirType::Bytes),
        8 => {
            let name = read_string(data, offset)?;
            let field_count = read_u32_le(data, offset)? as usize;
            let mut fields = Vec::with_capacity(field_count);
            for _ in 0..field_count {
                let fname = read_string(data, offset)?;
                let fty = deserialize_type_id(data, offset)?;
                fields.push(NirStructField {
                    name: fname,
                    ty: fty,
                });
            }
            Ok(NirType::Struct(NirStructType { name, fields }))
        }
        9 => {
            let name = read_string(data, offset)?;
            let variant_count = read_u32_le(data, offset)? as usize;
            let mut variants = Vec::with_capacity(variant_count);
            for _ in 0..variant_count {
                let vname = read_string(data, offset)?;
                let field_count = read_u32_le(data, offset)? as usize;
                let mut fields = Vec::with_capacity(field_count);
                for _ in 0..field_count {
                    fields.push(deserialize_type_id(data, offset)?);
                }
                variants.push(NirEnumVariant {
                    name: vname,
                    fields,
                });
            }
            Ok(NirType::Enum(NirEnumType { name, variants }))
        }
        10 => {
            let name = read_string(data, offset)?;
            let underlying = deserialize_type_id(data, offset)?;
            Ok(NirType::Distinct(NirDistinctType { name, underlying }))
        }
        11 => {
            let mutable = read_u8(data, offset)? != 0;
            let target = deserialize_type_id(data, offset)?;
            Ok(NirType::Ref { mutable, target })
        }
        12 => {
            let inner = deserialize_type_id(data, offset)?;
            Ok(NirType::Array(inner))
        }
        13 => {
            let name = read_string(data, offset)?;
            let method_count = read_u32_le(data, offset)? as usize;
            let mut methods = Vec::with_capacity(method_count);
            for _ in 0..method_count {
                methods.push(deserialize_callable_signature(data, offset)?);
            }
            Ok(NirType::Interface(NirInterfaceType { name, methods }))
        }
        14 => {
            let inner = deserialize_type_id(data, offset)?;
            Ok(NirType::Task(inner))
        }
        15 => {
            let kind = deserialize_callable_kind(data, offset)?;
            let param_count = read_u32_le(data, offset)? as usize;
            let mut parameters = Vec::with_capacity(param_count);
            for _ in 0..param_count {
                let ty = deserialize_type_id(data, offset)?;
                let passing = deserialize_passing_mode(data, offset)?;
                parameters.push(NirParameter { ty, passing });
            }
            let return_type = deserialize_type_id(data, offset)?;
            Ok(NirType::Callable(NirCallableType {
                kind,
                parameters,
                return_type,
            }))
        }
        _ => Err(NirDeserError::MalformedData(format!(
            "unknown type tag: {tag}"
        ))),
    }
}

fn read_u8(data: &[u8], offset: &mut usize) -> Result<u8, NirDeserError> {
    if *offset >= data.len() {
        return Err(NirDeserError::TruncatedData);
    }
    let v = data[*offset];
    *offset += 1;
    Ok(v)
}

fn serialize_callable_signature(buf: &mut Vec<u8>, sig: &NirCallableSignature) {
    serialize_calable_kind(buf, sig.kind);
    write_u32_le(buf, sig.parameters.len() as u32);
    for p in &sig.parameters {
        serialize_type_id(buf, p.ty);
        serialize_passing_mode(buf, p.passing);
    }
    serialize_type_id(buf, sig.return_type);
    write_u32_le(buf, sig.effects.effects.len() as u32);
    for e in &sig.effects.effects {
        write_string(buf, e);
    }
}

fn deserialize_callable_signature(
    data: &[u8],
    offset: &mut usize,
) -> Result<NirCallableSignature, NirDeserError> {
    let kind = deserialize_callable_kind(data, offset)?;
    let param_count = read_u32_le(data, offset)? as usize;
    let mut parameters = Vec::with_capacity(param_count);
    for _ in 0..param_count {
        let ty = deserialize_type_id(data, offset)?;
        let passing = deserialize_passing_mode(data, offset)?;
        parameters.push(NirParameter { ty, passing });
    }
    let return_type = deserialize_type_id(data, offset)?;
    let effect_count = read_u32_le(data, offset)? as usize;
    let mut effects = Vec::with_capacity(effect_count);
    for _ in 0..effect_count {
        effects.push(read_string(data, offset)?);
    }
    Ok(NirCallableSignature {
        kind,
        parameters,
        return_type,
        effects: NirEffectSet { effects },
    })
}

fn serialize_constant(buf: &mut Vec<u8>, c: &NirConstant) {
    let tag: u8 = match c {
        NirConstant::Integer(_) => 0,
        NirConstant::FloatBits(_) => 1,
        NirConstant::StringUtf8(_) => 2,
        NirConstant::Bytes(_) => 3,
        NirConstant::Char(_) => 4,
    };
    buf.push(tag);
    match c {
        NirConstant::Integer(v) => {
            write_u128_le(buf, *v as u128);
        }
        NirConstant::FloatBits(v) => {
            write_u64_le(buf, *v);
        }
        NirConstant::StringUtf8(s) => {
            write_string(buf, s);
        }
        NirConstant::Bytes(b) => {
            write_u32_le(buf, b.len() as u32);
            buf.extend_from_slice(b);
        }
        NirConstant::Char(ch) => {
            let mut tmp = [0u8; 4];
            let s = ch.encode_utf8(&mut tmp);
            write_string(buf, s);
        }
    }
}

fn deserialize_constant(data: &[u8], offset: &mut usize) -> Result<NirConstant, NirDeserError> {
    let tag = read_u8(data, offset)?;
    match tag {
        0 => {
            let v = read_u128_le(data, offset)?;
            Ok(NirConstant::Integer(v as i128))
        }
        1 => {
            let v = read_u64_le(data, offset)?;
            Ok(NirConstant::FloatBits(v))
        }
        2 => {
            let s = read_string(data, offset)?;
            Ok(NirConstant::StringUtf8(s))
        }
        3 => {
            let len = read_u32_le(data, offset)? as usize;
            if *offset + len > data.len() {
                return Err(NirDeserError::TruncatedData);
            }
            let b = data[*offset..*offset + len].to_vec();
            *offset += len;
            Ok(NirConstant::Bytes(b))
        }
        4 => {
            let s = read_string(data, offset)?;
            let ch = s
                .chars()
                .next()
                .ok_or_else(|| NirDeserError::MalformedData("empty char constant".to_string()))?;
            Ok(NirConstant::Char(ch))
        }
        _ => Err(NirDeserError::MalformedData(format!(
            "unknown constant tag: {tag}"
        ))),
    }
}

#[allow(dead_code)]
fn serialize_nir_parameter(buf: &mut Vec<u8>, p: &NirParameter) {
    serialize_type_id(buf, p.ty);
    serialize_passing_mode(buf, p.passing);
}

#[allow(dead_code)]
fn deserialize_nir_parameter(
    data: &[u8],
    offset: &mut usize,
) -> Result<NirParameter, NirDeserError> {
    let ty = deserialize_type_id(data, offset)?;
    let passing = deserialize_passing_mode(data, offset)?;
    Ok(NirParameter { ty, passing })
}

fn serialize_instruction(buf: &mut Vec<u8>, inst: &NirInstruction) {
    match inst {
        NirInstruction::Nop => buf.push(0),
        NirInstruction::ConstI64 { dest, value } => {
            buf.push(1);
            write_u32_le(buf, *dest);
            write_u64_le(buf, *value as u64);
        }
        NirInstruction::BinaryI64 {
            dest,
            op,
            left,
            right,
        } => {
            buf.push(2);
            write_u32_le(buf, *dest);
            buf.push(*op as u8);
            write_u32_le(buf, *left);
            write_u32_le(buf, *right);
        }
        NirInstruction::Call {
            dest,
            function,
            args,
        } => {
            buf.push(3);
            match dest {
                Some(dest) => {
                    buf.push(1);
                    write_u32_le(buf, *dest);
                }
                None => buf.push(0),
            }
            write_u32_le(buf, function.0);
            write_u32_le(buf, args.len() as u32);
            for arg in args {
                write_u32_le(buf, *arg);
            }
        }
        NirInstruction::ConsoleWriteUtf8 { text } => {
            buf.push(4);
            write_string(buf, text);
        }
        NirInstruction::LocalRead { dest, local } => {
            buf.push(5);
            write_u32_le(buf, *dest);
            write_u32_le(buf, *local);
        }
        NirInstruction::LocalWrite { local, value } => {
            buf.push(6);
            write_u32_le(buf, *local);
            write_u32_le(buf, *value);
        }
        NirInstruction::RuntimeTrap { kind, location } => {
            buf.push(7);
            write_u32_le(buf, *kind as u32);
            write_u32_le(buf, *location as u32);
        }
    }
}

fn deserialize_instruction(
    data: &[u8],
    offset: &mut usize,
) -> Result<NirInstruction, NirDeserError> {
    let tag = read_u8(data, offset)?;
    match tag {
        0 => Ok(NirInstruction::Nop),
        1 => Ok(NirInstruction::ConstI64 {
            dest: read_u32_le(data, offset)?,
            value: read_u64_le(data, offset)? as i64,
        }),
        2 => {
            let dest = read_u32_le(data, offset)?;
            let op = match read_u8(data, offset)? {
                0 => NirScalarBinaryOp::Add,
                1 => NirScalarBinaryOp::Sub,
                2 => NirScalarBinaryOp::Mul,
                3 => NirScalarBinaryOp::DivSigned,
                4 => NirScalarBinaryOp::RemSigned,
                5 => NirScalarBinaryOp::Eq,
                6 => NirScalarBinaryOp::Ne,
                7 => NirScalarBinaryOp::LtSigned,
                8 => NirScalarBinaryOp::LeSigned,
                9 => NirScalarBinaryOp::GtSigned,
                10 => NirScalarBinaryOp::GeSigned,
                11 => NirScalarBinaryOp::BitAnd,
                12 => NirScalarBinaryOp::BitOr,
                13 => NirScalarBinaryOp::BitXor,
                14 => NirScalarBinaryOp::ShiftLeft,
                15 => NirScalarBinaryOp::ShiftRightSigned,
                tag => {
                    return Err(NirDeserError::MalformedData(format!(
                        "unknown scalar binary op tag: {tag}"
                    )))
                }
            };
            Ok(NirInstruction::BinaryI64 {
                dest,
                op,
                left: read_u32_le(data, offset)?,
                right: read_u32_le(data, offset)?,
            })
        }
        3 => {
            let dest = match read_u8(data, offset)? {
                0 => None,
                1 => Some(read_u32_le(data, offset)?),
                tag => {
                    return Err(NirDeserError::MalformedData(format!(
                        "unknown call destination tag: {tag}"
                    )))
                }
            };
            let function = NirFunctionId(read_u32_le(data, offset)?);
            let arg_count = read_u32_le(data, offset)? as usize;
            let mut args = Vec::with_capacity(arg_count);
            for _ in 0..arg_count {
                args.push(read_u32_le(data, offset)?);
            }
            Ok(NirInstruction::Call {
                dest,
                function,
                args,
            })
        }
        4 => Ok(NirInstruction::ConsoleWriteUtf8 {
            text: read_string(data, offset)?,
        }),
        5 => Ok(NirInstruction::LocalRead {
            dest: read_u32_le(data, offset)?,
            local: read_u32_le(data, offset)?,
        }),
        6 => Ok(NirInstruction::LocalWrite {
            local: read_u32_le(data, offset)?,
            value: read_u32_le(data, offset)?,
        }),
        7 => Ok(NirInstruction::RuntimeTrap {
            kind: read_u32_le(data, offset)? as i32,
            location: read_u32_le(data, offset)? as i32,
        }),
        _ => Err(NirDeserError::MalformedData(format!(
            "unknown instruction tag: {tag}"
        ))),
    }
}

fn serialize_terminator(buf: &mut Vec<u8>, term: &NirTerminator) {
    match term {
        NirTerminator::Return { value } => {
            buf.push(0);
            match value {
                Some(v) => {
                    buf.push(1);
                    write_u32_le(buf, *v);
                }
                None => {
                    buf.push(0);
                }
            }
        }
        NirTerminator::Unreachable => {
            buf.push(1);
        }
        NirTerminator::Branch { target } => {
            buf.push(2);
            write_u32_le(buf, *target);
        }
        NirTerminator::CondBranch {
            condition,
            then_target,
            else_target,
        } => {
            buf.push(3);
            write_u32_le(buf, *condition);
            write_u32_le(buf, *then_target);
            write_u32_le(buf, *else_target);
        }
    }
}

fn deserialize_terminator(data: &[u8], offset: &mut usize) -> Result<NirTerminator, NirDeserError> {
    let tag = read_u8(data, offset)?;
    match tag {
        0 => {
            let has_val = read_u8(data, offset)?;
            if has_val != 0 {
                let v = read_u32_le(data, offset)?;
                Ok(NirTerminator::Return { value: Some(v) })
            } else {
                Ok(NirTerminator::Return { value: None })
            }
        }
        1 => Ok(NirTerminator::Unreachable),
        2 => Ok(NirTerminator::Branch {
            target: read_u32_le(data, offset)?,
        }),
        3 => Ok(NirTerminator::CondBranch {
            condition: read_u32_le(data, offset)?,
            then_target: read_u32_le(data, offset)?,
            else_target: read_u32_le(data, offset)?,
        }),
        _ => Err(NirDeserError::MalformedData(format!(
            "unknown terminator tag: {tag}"
        ))),
    }
}

fn serialize_basic_block(buf: &mut Vec<u8>, bb: &NirBasicBlock) {
    write_u32_le(buf, bb.id);
    write_u32_le(buf, bb.instructions.len() as u32);
    for inst in &bb.instructions {
        serialize_instruction(buf, inst);
    }
    serialize_terminator(buf, &bb.terminator);
}

fn deserialize_basic_block(
    data: &[u8],
    offset: &mut usize,
) -> Result<NirBasicBlock, NirDeserError> {
    let id = read_u32_le(data, offset)?;
    let inst_count = read_u32_le(data, offset)? as usize;
    let mut instructions = Vec::with_capacity(inst_count);
    for _ in 0..inst_count {
        instructions.push(deserialize_instruction(data, offset)?);
    }
    let terminator = deserialize_terminator(data, offset)?;
    Ok(NirBasicBlock {
        id,
        instructions,
        terminator,
    })
}

fn serialize_function(buf: &mut Vec<u8>, func: &NirFunction) {
    write_u32_le(buf, func.id.0);
    serialize_callable_signature(buf, &func.signature);
    match &func.body {
        Some(body) => {
            buf.push(1);
            write_u32_le(buf, body.blocks.len() as u32);
            for bb in &body.blocks {
                serialize_basic_block(buf, bb);
            }
            write_u32_le(buf, body.entry);
        }
        None => {
            buf.push(0);
        }
    }
    // metadata
    match func.metadata.source_location {
        Some(loc) => {
            buf.push(1);
            write_u32_le(buf, loc.0);
        }
        None => {
            buf.push(0);
        }
    }
    buf.push(if func.metadata.is_exported { 1 } else { 0 });
}

fn deserialize_function(data: &[u8], offset: &mut usize) -> Result<NirFunction, NirDeserError> {
    let id = NirFunctionId(read_u32_le(data, offset)?);
    let signature = deserialize_callable_signature(data, offset)?;
    let has_body = read_u8(data, offset)?;
    let body = if has_body != 0 {
        let block_count = read_u32_le(data, offset)? as usize;
        let mut blocks = Vec::with_capacity(block_count);
        for _ in 0..block_count {
            blocks.push(deserialize_basic_block(data, offset)?);
        }
        let entry = read_u32_le(data, offset)?;
        Some(NirFunctionBody { blocks, entry })
    } else {
        None
    };
    let has_loc = read_u8(data, offset)?;
    let source_location = if has_loc != 0 {
        Some(SourceLocationId(read_u32_le(data, offset)?))
    } else {
        None
    };
    let is_exported = read_u8(data, offset)? != 0;
    Ok(NirFunction {
        id,
        signature,
        body,
        metadata: NirFunctionMetadata {
            source_location,
            is_exported,
        },
    })
}

fn serialize_global(buf: &mut Vec<u8>, g: &NirGlobal) {
    write_u32_le(buf, g.id.0);
    serialize_type_id(buf, g.ty);
    match g.constant {
        Some(cid) => {
            buf.push(1);
            write_u32_le(buf, cid.0);
        }
        None => {
            buf.push(0);
        }
    }
}

fn deserialize_global(data: &[u8], offset: &mut usize) -> Result<NirGlobal, NirDeserError> {
    let id = NirGlobalId(read_u32_le(data, offset)?);
    let ty = deserialize_type_id(data, offset)?;
    let has_const = read_u8(data, offset)?;
    let constant = if has_const != 0 {
        Some(NirConstantId(read_u32_le(data, offset)?))
    } else {
        None
    };
    Ok(NirGlobal { id, ty, constant })
}

fn serialize_runtime_capability(buf: &mut Vec<u8>, cap: &RuntimeCapability) {
    let tag: u8 = match cap {
        RuntimeCapability::Memory => 0,
        RuntimeCapability::ConsoleWrite => 1,
        RuntimeCapability::FileSystem => 2,
        RuntimeCapability::Network => 3,
        RuntimeCapability::TaskRuntime => 4,
        RuntimeCapability::ContractRuntime => 5,
    };
    buf.push(tag);
}

fn deserialize_runtime_capability(
    data: &[u8],
    offset: &mut usize,
) -> Result<RuntimeCapability, NirDeserError> {
    let tag = read_u8(data, offset)?;
    match tag {
        0 => Ok(RuntimeCapability::Memory),
        1 => Ok(RuntimeCapability::ConsoleWrite),
        2 => Ok(RuntimeCapability::FileSystem),
        3 => Ok(RuntimeCapability::Network),
        4 => Ok(RuntimeCapability::TaskRuntime),
        5 => Ok(RuntimeCapability::ContractRuntime),
        _ => Err(NirDeserError::MalformedData(format!(
            "unknown runtime capability tag: {tag}"
        ))),
    }
}

fn serialize_metadata(buf: &mut Vec<u8>, meta: &ModuleMetadata) {
    write_u32_le(buf, meta.runtime_requirements.capabilities.len() as u32);
    for cap in &meta.runtime_requirements.capabilities {
        serialize_runtime_capability(buf, cap);
    }
    write_string(buf, &meta.build_provenance.compiler_version);
    write_string(buf, &meta.build_provenance.language_profile);
    write_string(buf, &meta.build_provenance.nir_version);
}

fn deserialize_metadata(data: &[u8], offset: &mut usize) -> Result<ModuleMetadata, NirDeserError> {
    let cap_count = read_u32_le(data, offset)? as usize;
    let mut capabilities = Vec::with_capacity(cap_count);
    for _ in 0..cap_count {
        capabilities.push(deserialize_runtime_capability(data, offset)?);
    }
    let compiler_version = read_string(data, offset)?;
    let language_profile = read_string(data, offset)?;
    let nir_version = read_string(data, offset)?;
    Ok(ModuleMetadata {
        runtime_requirements: RuntimeRequirements { capabilities },
        build_provenance: BuildProvenance {
            compiler_version,
            language_profile,
            nir_version,
        },
    })
}

pub struct NirSerializer;

impl NirSerializer {
    pub fn serialize(module: &NirModule) -> Vec<u8> {
        let mut buf: Vec<u8> = Vec::new();

        // Magic
        buf.extend_from_slice(MAGIC);

        // Section count: 5
        write_u32_le(&mut buf, 5);

        // Section 0: Types
        let mut sec = Vec::new();
        write_u32_le(&mut sec, module.types.len() as u32);
        for i in 0..module.types.len() {
            if let Some(t) = module.types.get(NirTypeId(i as u32)) {
                serialize_nir_type(&mut sec, t);
            }
        }
        buf.push(SECTION_TYPES);
        write_u32_le(&mut buf, sec.len() as u32);
        buf.extend_from_slice(&sec);

        // Section 1: Constants
        let mut sec = Vec::new();
        write_u32_le(&mut sec, module.constants.len() as u32);
        for i in 0..module.constants.len() {
            if let Some(c) = module.constants.get(NirConstantId(i as u32)) {
                serialize_constant(&mut sec, c);
            }
        }
        buf.push(SECTION_CONSTANTS);
        write_u32_le(&mut buf, sec.len() as u32);
        buf.extend_from_slice(&sec);

        // Section 2: Functions
        let mut sec = Vec::new();
        write_u32_le(&mut sec, module.functions.len() as u32);
        for i in 0..module.functions.len() {
            if let Some(f) = module.functions.get(NirFunctionId(i as u32)) {
                serialize_function(&mut sec, f);
            }
        }
        buf.push(SECTION_FUNCTIONS);
        write_u32_le(&mut buf, sec.len() as u32);
        buf.extend_from_slice(&sec);

        // Section 3: Globals
        let mut sec = Vec::new();
        write_u32_le(&mut sec, module.globals.len() as u32);
        for i in 0..module.globals.len() {
            if let Some(g) = module.globals.get(NirGlobalId(i as u32)) {
                serialize_global(&mut sec, g);
            }
        }
        buf.push(SECTION_GLOBALS);
        write_u32_le(&mut buf, sec.len() as u32);
        buf.extend_from_slice(&sec);

        // Section 4: Metadata
        let mut sec = Vec::new();
        serialize_metadata(&mut sec, &module.metadata);
        buf.push(SECTION_METADATA);
        write_u32_le(&mut buf, sec.len() as u32);
        buf.extend_from_slice(&sec);

        buf
    }
}

pub struct NirDeserializer;

impl NirDeserializer {
    pub fn deserialize(data: &[u8]) -> Result<NirModule, NirDeserError> {
        if data.len() < 8 {
            return Err(NirDeserError::TruncatedData);
        }

        // Magic check
        if &data[0..4] != MAGIC {
            return Err(NirDeserError::InvalidMagic);
        }

        let mut offset = 4;

        // Section count
        let section_count = read_u32_le(data, &mut offset)? as usize;

        let mut types_table = TypeTable::new();
        let mut constants_pool = ConstantPool::new();
        let mut functions_table = FunctionTable::new();
        let mut globals_table = GlobalTable::new();
        let mut metadata = ModuleMetadata::default();

        for _ in 0..section_count {
            if offset >= data.len() {
                return Err(NirDeserError::TruncatedData);
            }
            let section_tag = data[offset];
            offset += 1;
            let section_len = read_u32_le(data, &mut offset)? as usize;
            let section_end = offset + section_len;
            if section_end > data.len() {
                return Err(NirDeserError::TruncatedData);
            }
            let section_data = &data[offset..section_end];

            match section_tag {
                SECTION_TYPES => {
                    let mut soff = 0;
                    let count = read_u32_le(section_data, &mut soff)? as usize;
                    for _ in 0..count {
                        let ty = deserialize_nir_type(section_data, &mut soff)?;
                        types_table.add(ty);
                    }
                }
                SECTION_CONSTANTS => {
                    let mut soff = 0;
                    let count = read_u32_le(section_data, &mut soff)? as usize;
                    for _ in 0..count {
                        let c = deserialize_constant(section_data, &mut soff)?;
                        constants_pool.add(c);
                    }
                }
                SECTION_FUNCTIONS => {
                    let mut soff = 0;
                    let count = read_u32_le(section_data, &mut soff)? as usize;
                    for _ in 0..count {
                        let f = deserialize_function(section_data, &mut soff)?;
                        functions_table.add(f);
                    }
                }
                SECTION_GLOBALS => {
                    let mut soff = 0;
                    let count = read_u32_le(section_data, &mut soff)? as usize;
                    for _ in 0..count {
                        let g = deserialize_global(section_data, &mut soff)?;
                        globals_table.add(g);
                    }
                }
                SECTION_METADATA => {
                    let mut soff = 0;
                    metadata = deserialize_metadata(section_data, &mut soff)?;
                }
                _ => {
                    return Err(NirDeserError::UnknownCriticalSection(section_tag));
                }
            }

            offset = section_end;
        }

        Ok(NirModule {
            header: NirHeader::default(),
            types: types_table,
            constants: constants_pool,
            functions: functions_table,
            globals: globals_table,
            metadata,
        })
    }
}

#[derive(Debug, Clone)]
pub enum NirDeserError {
    InvalidMagic,
    UnsupportedVersion(String),
    TruncatedData,
    UnknownCriticalSection(u8),
    MalformedData(String),
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // 1. NirTypeId creation/equality
    #[test]
    fn test_nir_type_id_creation_equality() {
        let a = NirTypeId(0);
        let b = NirTypeId(0);
        let c = NirTypeId(1);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    // 2. NirType variants — Unit
    #[test]
    fn test_nir_type_unit() {
        let mut tt = TypeTable::new();
        let id = tt.add(NirType::Unit);
        assert_eq!(tt.get(id), Some(&NirType::Unit));
    }

    // 3. NirType variants — Never
    #[test]
    fn test_nir_type_never() {
        let mut tt = TypeTable::new();
        let id = tt.add(NirType::Never);
        assert_eq!(tt.get(id), Some(&NirType::Never));
    }

    // 4. NirType variants — Bool
    #[test]
    fn test_nir_type_bool() {
        let mut tt = TypeTable::new();
        let id = tt.add(NirType::Bool);
        assert_eq!(tt.get(id), Some(&NirType::Bool));
    }

    // 5. NirType variants — Int
    #[test]
    fn test_nir_type_int() {
        let mut tt = TypeTable::new();
        let id = tt.add(NirType::Int {
            signed: true,
            bits: 32,
        });
        assert_eq!(
            tt.get(id),
            Some(&NirType::Int {
                signed: true,
                bits: 32
            })
        );
    }

    // 6. NirType variants — Float
    #[test]
    fn test_nir_type_float() {
        let mut tt = TypeTable::new();
        let id = tt.add(NirType::Float { bits: 64 });
        assert_eq!(tt.get(id), Some(&NirType::Float { bits: 64 }));
    }

    // 7. NirType variants — Char
    #[test]
    fn test_nir_type_char() {
        let mut tt = TypeTable::new();
        let id = tt.add(NirType::Char);
        assert_eq!(tt.get(id), Some(&NirType::Char));
    }

    // 8. NirType variants — String
    #[test]
    fn test_nir_type_string() {
        let mut tt = TypeTable::new();
        let id = tt.add(NirType::String);
        assert_eq!(tt.get(id), Some(&NirType::String));
    }

    // 9. NirType variants — Struct
    #[test]
    fn test_nir_type_struct() {
        let mut tt = TypeTable::new();
        let bool_id = tt.add(NirType::Bool);
        let st = NirStructType {
            name: "Point".to_string(),
            fields: vec![NirStructField {
                name: "x".to_string(),
                ty: bool_id,
            }],
        };
        let id = tt.add(NirType::Struct(st));
        match tt.get(id) {
            Some(NirType::Struct(s)) => {
                assert_eq!(s.name, "Point");
                assert_eq!(s.fields.len(), 1);
                assert_eq!(s.fields[0].name, "x");
            }
            _ => panic!("expected Struct"),
        }
    }

    // 10. NirType variants — Enum
    #[test]
    fn test_nir_type_enum() {
        let mut tt = TypeTable::new();
        let unit_id = tt.add(NirType::Unit);
        let en = NirEnumType {
            name: "Color".to_string(),
            variants: vec![
                NirEnumVariant {
                    name: "Red".to_string(),
                    fields: vec![],
                },
                NirEnumVariant {
                    name: "Custom".to_string(),
                    fields: vec![unit_id],
                },
            ],
        };
        let id = tt.add(NirType::Enum(en));
        match tt.get(id) {
            Some(NirType::Enum(e)) => {
                assert_eq!(e.name, "Color");
                assert_eq!(e.variants.len(), 2);
            }
            _ => panic!("expected Enum"),
        }
    }

    // 11. NirType variants — Distinct
    #[test]
    fn test_nir_type_distinct() {
        let mut tt = TypeTable::new();
        let int_id = tt.add(NirType::Int {
            signed: false,
            bits: 64,
        });
        let d = NirDistinctType {
            name: "UserId".to_string(),
            underlying: int_id,
        };
        let id = tt.add(NirType::Distinct(d));
        match tt.get(id) {
            Some(NirType::Distinct(d)) => {
                assert_eq!(d.name, "UserId");
                assert_eq!(d.underlying, int_id);
            }
            _ => panic!("expected Distinct"),
        }
    }

    // 12. NirType variants — Ref
    #[test]
    fn test_nir_type_ref() {
        let mut tt = TypeTable::new();
        let target = tt.add(NirType::Bool);
        let id = tt.add(NirType::Ref {
            mutable: true,
            target,
        });
        assert_eq!(
            tt.get(id),
            Some(&NirType::Ref {
                mutable: true,
                target
            })
        );
    }

    // 13. NirType variants — Array, Task, Callable
    #[test]
    fn test_nir_type_array_task_callable() {
        let mut tt = TypeTable::new();
        let inner = tt.add(NirType::Bool);
        let arr_id = tt.add(NirType::Array(inner));
        assert_eq!(tt.get(arr_id), Some(&NirType::Array(inner)));

        let task_id = tt.add(NirType::Task(inner));
        assert_eq!(tt.get(task_id), Some(&NirType::Task(inner)));

        let ct = NirCallableType {
            kind: NirCallableKind::Function,
            parameters: vec![],
            return_type: inner,
        };
        let callable_id = tt.add(NirType::Callable(ct));
        match tt.get(callable_id) {
            Some(NirType::Callable(c)) => assert_eq!(c.kind, NirCallableKind::Function),
            _ => panic!("expected Callable"),
        }
    }

    // 14. NirPassingMode variants
    #[test]
    fn test_nir_passing_mode_variants() {
        assert_eq!(NirPassingMode::Owned, NirPassingMode::Owned);
        assert_eq!(NirPassingMode::Ref, NirPassingMode::Ref);
        assert_eq!(NirPassingMode::MutRef, NirPassingMode::MutRef);
        assert_ne!(NirPassingMode::Owned, NirPassingMode::Ref);
    }

    // 15. NirCallableKind variants
    #[test]
    fn test_nir_callable_kind_variants() {
        assert_eq!(NirCallableKind::Function, NirCallableKind::Function);
        assert_eq!(NirCallableKind::Action, NirCallableKind::Action);
        assert_eq!(NirCallableKind::AsyncAction, NirCallableKind::AsyncAction);
        assert_ne!(NirCallableKind::Function, NirCallableKind::Action);
    }

    // 16. TypeTable: add and get
    #[test]
    fn test_type_table_add_get() {
        let mut tt = TypeTable::new();
        assert!(tt.is_empty());
        let id1 = tt.add(NirType::Unit);
        let id2 = tt.add(NirType::Never);
        assert_eq!(id1, NirTypeId(0));
        assert_eq!(id2, NirTypeId(1));
        assert_eq!(tt.len(), 2);
        assert!(!tt.is_empty());
    }

    // 17. ConstantPool: add and get integer, float, string
    #[test]
    fn test_constant_pool_integer_float_string() {
        let mut pool = ConstantPool::new();
        assert!(pool.is_empty());
        let int_id = pool.add(NirConstant::Integer(42));
        let float_id = pool.add(NirConstant::FloatBits(0x400921FB54442D18));
        let str_id = pool.add(NirConstant::StringUtf8("hello".to_string()));
        assert_eq!(pool.len(), 3);
        assert!(!pool.is_empty());
        assert_eq!(pool.get(int_id), Some(&NirConstant::Integer(42)));
        assert_eq!(
            pool.get(float_id),
            Some(&NirConstant::FloatBits(0x400921FB54442D18))
        );
        assert_eq!(
            pool.get(str_id),
            Some(&NirConstant::StringUtf8("hello".to_string()))
        );
    }

    // 18. FunctionTable: add and get
    #[test]
    fn test_function_table_add_get() {
        let mut ft = FunctionTable::new();
        assert!(ft.is_empty());
        let func = NirFunction {
            id: NirFunctionId(0),
            signature: NirCallableSignature {
                kind: NirCallableKind::Function,
                parameters: vec![],
                return_type: NirTypeId(0),
                effects: NirEffectSet::empty(),
            },
            body: None,
            metadata: NirFunctionMetadata::default(),
        };
        ft.add(func);
        assert_eq!(ft.len(), 1);
        assert!(!ft.is_empty());
        assert!(ft.get(NirFunctionId(0)).is_some());
        assert!(ft.get(NirFunctionId(1)).is_none());
    }

    // 19. GlobalTable: add and get
    #[test]
    fn test_global_table_add_get() {
        let mut gt = GlobalTable::new();
        assert!(gt.is_empty());
        let g = NirGlobal {
            id: NirGlobalId(0),
            ty: NirTypeId(0),
            constant: Some(NirConstantId(0)),
        };
        gt.add(g);
        assert_eq!(gt.len(), 1);
        assert!(!gt.is_empty());
        assert!(gt.get(NirGlobalId(0)).is_some());
        assert!(gt.get(NirGlobalId(1)).is_none());
    }

    // 20. NirModule creation
    #[test]
    fn test_nir_module_creation() {
        let m = NirModule::new();
        assert!(m.types.is_empty());
        assert!(m.constants.is_empty());
        assert!(m.functions.is_empty());
        assert!(m.globals.is_empty());
    }

    // 21. NirHeader default is NIR1
    #[test]
    fn test_nir_header_default() {
        let h = NirHeader::default();
        assert_eq!(h.format_version, NirFormatVersion("NIR1".to_string()));
    }

    // 22. NirEffectSet empty and non-empty
    #[test]
    fn test_nir_effect_set() {
        let empty = NirEffectSet::empty();
        assert!(empty.is_empty());
        let mut non_empty = NirEffectSet {
            effects: vec!["io".to_string()],
        };
        assert!(!non_empty.is_empty());
        non_empty.effects.push("network".to_string());
        assert_eq!(non_empty.effects.len(), 2);
    }

    // 23. NirCallableSignature creation
    #[test]
    fn test_nir_callable_signature() {
        let sig = NirCallableSignature {
            kind: NirCallableKind::Action,
            parameters: vec![NirParameter {
                ty: NirTypeId(0),
                passing: NirPassingMode::Ref,
            }],
            return_type: NirTypeId(1),
            effects: NirEffectSet {
                effects: vec!["io".to_string()],
            },
        };
        assert_eq!(sig.kind, NirCallableKind::Action);
        assert_eq!(sig.parameters.len(), 1);
        assert_eq!(sig.parameters[0].passing, NirPassingMode::Ref);
        assert!(!sig.effects.is_empty());
    }

    // 24. RuntimeRequirements: add and has
    #[test]
    fn test_runtime_requirements() {
        let mut rr = RuntimeRequirements::new();
        assert!(rr.is_empty());
        rr.add(RuntimeCapability::Memory);
        rr.add(RuntimeCapability::ConsoleWrite);
        rr.add(RuntimeCapability::Memory); // duplicate
        assert!(rr.has(&RuntimeCapability::Memory));
        assert!(rr.has(&RuntimeCapability::ConsoleWrite));
        assert!(!rr.has(&RuntimeCapability::Network));
        assert_eq!(rr.capabilities.len(), 2);
    }

    // 25. NirSerializer round-trip — empty module
    #[test]
    fn test_serializer_roundtrip_empty() {
        let m = NirModule::new();
        let data = NirSerializer::serialize(&m);
        let m2 = NirDeserializer::deserialize(&data).unwrap();
        assert!(m2.types.is_empty());
        assert!(m2.constants.is_empty());
        assert!(m2.functions.is_empty());
        assert!(m2.globals.is_empty());
    }

    // 26. NirSerializer round-trip — module with types and constants
    #[test]
    fn test_serializer_roundtrip_types_constants() {
        let mut m = NirModule::new();
        m.types.add(NirType::Unit);
        m.types.add(NirType::Bool);
        m.types.add(NirType::Int {
            signed: true,
            bits: 32,
        });
        m.types.add(NirType::Float { bits: 64 });
        m.types.add(NirType::Char);
        m.types.add(NirType::String);
        m.types.add(NirType::Bytes);
        m.constants.add(NirConstant::Integer(123));
        m.constants.add(NirConstant::FloatBits(0x3FF0000000000000));
        m.constants.add(NirConstant::StringUtf8("test".to_string()));
        m.constants.add(NirConstant::Bytes(vec![1, 2, 3]));
        m.constants.add(NirConstant::Char('A'));

        let data = NirSerializer::serialize(&m);
        let m2 = NirDeserializer::deserialize(&data).unwrap();
        assert_eq!(m2.types.len(), 7);
        assert_eq!(m2.constants.len(), 5);
        assert_eq!(m2.types.get(NirTypeId(0)), Some(&NirType::Unit));
        assert_eq!(
            m2.types.get(NirTypeId(3)),
            Some(&NirType::Float { bits: 64 })
        );
        assert_eq!(
            m2.constants.get(NirConstantId(0)),
            Some(&NirConstant::Integer(123))
        );
        assert_eq!(
            m2.constants.get(NirConstantId(2)),
            Some(&NirConstant::StringUtf8("test".to_string()))
        );
        assert_eq!(
            m2.constants.get(NirConstantId(4)),
            Some(&NirConstant::Char('A'))
        );
    }

    // 27. NirSerializer round-trip — functions with body
    #[test]
    fn test_serializer_roundtrip_functions() {
        let mut m = NirModule::new();
        m.types.add(NirType::Unit);
        let func = NirFunction {
            id: NirFunctionId(0),
            signature: NirCallableSignature {
                kind: NirCallableKind::Function,
                parameters: vec![
                    NirParameter {
                        ty: NirTypeId(0),
                        passing: NirPassingMode::Owned,
                    },
                    NirParameter {
                        ty: NirTypeId(0),
                        passing: NirPassingMode::MutRef,
                    },
                ],
                return_type: NirTypeId(0),
                effects: NirEffectSet {
                    effects: vec!["io".to_string()],
                },
            },
            body: Some(NirFunctionBody {
                blocks: vec![NirBasicBlock {
                    id: 0,
                    instructions: vec![
                        NirInstruction::ConstI64 { dest: 0, value: 40 },
                        NirInstruction::ConstI64 { dest: 1, value: 2 },
                        NirInstruction::BinaryI64 {
                            dest: 2,
                            op: NirScalarBinaryOp::Add,
                            left: 0,
                            right: 1,
                        },
                        NirInstruction::Call {
                            dest: Some(3),
                            function: NirFunctionId(0),
                            args: vec![2],
                        },
                        NirInstruction::RuntimeTrap {
                            kind: 4,
                            location: 99,
                        },
                    ],
                    terminator: NirTerminator::Return { value: Some(2) },
                }],
                entry: 0,
            }),
            metadata: NirFunctionMetadata {
                source_location: Some(SourceLocationId(42)),
                is_exported: true,
            },
        };
        m.functions.add(func);

        let data = NirSerializer::serialize(&m);
        let m2 = NirDeserializer::deserialize(&data).unwrap();
        assert_eq!(m2.functions.len(), 1);
        let f2 = m2.functions.get(NirFunctionId(0)).unwrap();
        assert_eq!(f2.signature.kind, NirCallableKind::Function);
        assert_eq!(f2.signature.parameters.len(), 2);
        assert_eq!(f2.signature.parameters[1].passing, NirPassingMode::MutRef);
        assert_eq!(f2.signature.effects.effects, vec!["io".to_string()]);
        let body = f2.body.as_ref().unwrap();
        assert_eq!(body.blocks.len(), 1);
        assert_eq!(body.entry, 0);
        assert_eq!(
            body.blocks[0].instructions[2],
            NirInstruction::BinaryI64 {
                dest: 2,
                op: NirScalarBinaryOp::Add,
                left: 0,
                right: 1,
            }
        );
        assert_eq!(
            body.blocks[0].instructions[3],
            NirInstruction::Call {
                dest: Some(3),
                function: NirFunctionId(0),
                args: vec![2],
            }
        );
        assert_eq!(
            body.blocks[0].instructions[4],
            NirInstruction::RuntimeTrap {
                kind: 4,
                location: 99,
            }
        );
        assert_eq!(f2.metadata.source_location, Some(SourceLocationId(42)));
        assert!(f2.metadata.is_exported);
    }

    // 28. NirSerializer round-trip — globals
    #[test]
    fn test_serializer_roundtrip_globals() {
        let mut m = NirModule::new();
        m.types.add(NirType::Bool);
        m.constants.add(NirConstant::Integer(1));
        m.globals.add(NirGlobal {
            id: NirGlobalId(0),
            ty: NirTypeId(0),
            constant: Some(NirConstantId(0)),
        });
        m.globals.add(NirGlobal {
            id: NirGlobalId(1),
            ty: NirTypeId(0),
            constant: None,
        });

        let data = NirSerializer::serialize(&m);
        let m2 = NirDeserializer::deserialize(&data).unwrap();
        assert_eq!(m2.globals.len(), 2);
        let g0 = m2.globals.get(NirGlobalId(0)).unwrap();
        assert_eq!(g0.constant, Some(NirConstantId(0)));
        let g1 = m2.globals.get(NirGlobalId(1)).unwrap();
        assert_eq!(g1.constant, None);
    }

    // 29. NirDeserializer: invalid magic rejected
    #[test]
    fn test_deserializer_invalid_magic() {
        let data = b"BADEXPECTEDNIR1";
        let result = NirDeserializer::deserialize(data);
        assert!(matches!(result, Err(NirDeserError::InvalidMagic)));
    }

    // 30. NirDeserializer: truncated data rejected
    #[test]
    fn test_deserializer_truncated() {
        let result = NirDeserializer::deserialize(&[0u8; 4]);
        assert!(matches!(result, Err(NirDeserError::TruncatedData)));

        let result2 = NirDeserializer::deserialize(&[]);
        assert!(matches!(result2, Err(NirDeserError::TruncatedData)));
    }

    // 31. VerifiedNirModule wrap and unwrap
    #[test]
    fn test_verified_nir_module() {
        let m = NirModule::new();
        let vm = VerifiedNirModule::new(m);
        assert!(vm.inner().types.is_empty());
        let inner = vm.into_inner();
        assert!(inner.functions.is_empty());
    }

    // 32. PublicInterface default
    #[test]
    fn test_public_interface_default() {
        let pi = PublicInterface::default();
        assert!(pi.exported_types.is_empty());
        assert!(pi.exported_functions.is_empty());
    }

    // ── Serialization Helpers ─────────────────────────────────────────────

    #[allow(dead_code)]
    fn serialize_metadata(buf: &mut Vec<u8>, meta: &ModuleMetadata) {
        write_u32_le(buf, meta.runtime_requirements.capabilities.len() as u32);
        for cap in &meta.runtime_requirements.capabilities {
            serialize_runtime_capability(buf, cap);
        }
        write_string(buf, &meta.build_provenance.compiler_version);
        write_string(buf, &meta.build_provenance.language_profile);
        write_string(buf, &meta.build_provenance.nir_version);
    }

    #[allow(dead_code)]
    fn serialize_global(buf: &mut Vec<u8>, g: &NirGlobal) {
        write_u32_le(buf, g.id.0);
        serialize_type_id(buf, g.ty);
        match g.constant {
            Some(cid) => {
                buf.push(1); // Has constant
                write_u32_le(buf, cid.0);
            }
            None => {
                buf.push(0); // No constant
            }
        }
    }

    #[allow(dead_code)]
    fn serialize_function(buf: &mut Vec<u8>, f: &NirFunction) {
        write_u32_le(buf, f.id.0);
        serialize_callable_signature(buf, &f.signature);
        match &f.body {
            Some(body) => {
                buf.push(1); // Has body
                write_u32_le(buf, body.blocks.len() as u32);
                for bb in &body.blocks {
                    serialize_basic_block(buf, bb);
                }
                write_u32_le(buf, body.entry);
            }
            None => {
                buf.push(0); // No body
            }
        }
        match f.metadata.source_location {
            Some(loc) => {
                buf.push(1); // Has location
                write_u32_le(buf, loc.0);
            }
            None => {
                buf.push(0); // No location
            }
        }
        buf.push(if f.metadata.is_exported { 1 } else { 0 });
    }

    #[allow(dead_code)]
    fn serialize_callable_signature(buf: &mut Vec<u8>, sig: &NirCallableSignature) {
        match sig.kind {
            NirCallableKind::Function => buf.push(0),
            NirCallableKind::Action => buf.push(1),
            NirCallableKind::AsyncAction => buf.push(2),
        }
        write_u32_le(buf, sig.parameters.len() as u32);
        for param in &sig.parameters {
            serialize_type_id(buf, param.ty);
            match param.passing {
                NirPassingMode::Owned => buf.push(0),
                NirPassingMode::Ref => buf.push(1),
                NirPassingMode::MutRef => buf.push(2),
            }
        }
        serialize_type_id(buf, sig.return_type);
        match &sig.effects {
            NirEffectSet { effects } if !effects.is_empty() => {
                buf.push(1); // Has effects
                for eff in effects {
                    write_string(buf, eff);
                }
            }
            _ => {
                buf.push(0); // No effects
            }
        }
    }

    #[test]
    fn test_serializer_roundtrip_complex_types() {
        let mut m = NirModule::new();

        let bool_id = m.types.add(NirType::Bool);
        let int_id = m.types.add(NirType::Int {
            signed: false,
            bits: 64,
        });

        let struct_id = m.types.add(NirType::Struct(NirStructType {
            name: "User".to_string(),
            fields: vec![
                NirStructField {
                    name: "id".to_string(),
                    ty: int_id,
                },
                NirStructField {
                    name: "active".to_string(),
                    ty: bool_id,
                },
            ],
        }));

        let enum_id = m.types.add(NirType::Enum(NirEnumType {
            name: "Status".to_string(),
            variants: vec![
                NirEnumVariant {
                    name: "Active".to_string(),
                    fields: vec![],
                },
                NirEnumVariant {
                    name: "Inactive".to_string(),
                    fields: vec![bool_id],
                },
            ],
        }));

        let _distinct_id = m.types.add(NirType::Distinct(NirDistinctType {
            name: "Email".to_string(),
            underlying: struct_id,
        }));

        let ref_id = m.types.add(NirType::Ref {
            mutable: false,
            target: struct_id,
        });

        let array_id = m.types.add(NirType::Array(bool_id));

        let _task_id = m.types.add(NirType::Task(enum_id));

        let callable_id = m.types.add(NirType::Callable(NirCallableType {
            kind: NirCallableKind::AsyncAction,
            parameters: vec![
                NirParameter {
                    ty: int_id,
                    passing: NirPassingMode::Owned,
                },
                NirParameter {
                    ty: ref_id,
                    passing: NirPassingMode::MutRef,
                },
            ],
            return_type: bool_id,
        }));

        let _iface_id = m.types.add(NirType::Interface(NirInterfaceType {
            name: "Reader".to_string(),
            methods: vec![NirCallableSignature {
                kind: NirCallableKind::Function,
                parameters: vec![NirParameter {
                    ty: int_id,
                    passing: NirPassingMode::Ref,
                }],
                return_type: callable_id,
                effects: NirEffectSet {
                    effects: vec!["io".to_string()],
                },
            }],
        }));

        let data = NirSerializer::serialize(&m);
        let m2 = NirDeserializer::deserialize(&data).unwrap();
        assert_eq!(m2.types.len(), 10);

        match m2.types.get(struct_id) {
            Some(NirType::Struct(s)) => {
                assert_eq!(s.name, "User");
                assert_eq!(s.fields.len(), 2);
                assert_eq!(s.fields[0].name, "id");
            }
            _ => panic!("expected Struct"),
        }

        match m2.types.get(enum_id) {
            Some(NirType::Enum(e)) => {
                assert_eq!(e.name, "Status");
                assert_eq!(e.variants.len(), 2);
            }
            _ => panic!("expected Enum"),
        }

        match m2.types.get(array_id) {
            Some(NirType::Array(inner)) => assert_eq!(*inner, bool_id),
            _ => panic!("expected Array"),
        }

        match m2.types.get(callable_id) {
            Some(NirType::Callable(c)) => {
                assert_eq!(c.kind, NirCallableKind::AsyncAction);
                assert_eq!(c.parameters.len(), 2);
            }
            _ => panic!("expected Callable"),
        }
    }

    // 34. Runtime capabilities round-trip through serialize
    #[test]
    fn test_runtime_capabilities_metadata_roundtrip() {
        let mut m = NirModule::new();
        m.metadata
            .runtime_requirements
            .add(RuntimeCapability::Memory);
        m.metadata
            .runtime_requirements
            .add(RuntimeCapability::Network);
        m.metadata.build_provenance.compiler_version = "1.0.0".to_string();
        m.metadata.build_provenance.language_profile = "standard".to_string();
        m.metadata.build_provenance.nir_version = "NIR1".to_string();

        let data = NirSerializer::serialize(&m);
        let m2 = NirDeserializer::deserialize(&data).unwrap();
        assert!(m2
            .metadata
            .runtime_requirements
            .has(&RuntimeCapability::Memory));
        assert!(m2
            .metadata
            .runtime_requirements
            .has(&RuntimeCapability::Network));
        assert!(!m2
            .metadata
            .runtime_requirements
            .has(&RuntimeCapability::FileSystem));
        assert_eq!(m2.metadata.build_provenance.compiler_version, "1.0.0");
        assert_eq!(m2.metadata.build_provenance.nir_version, "NIR1");
    }

    // 35. NirFunctionId, NirConstantId, NirGlobalId, IntrinsicId, SourceLocationId
    #[test]
    fn test_other_ids() {
        let fid = NirFunctionId(5);
        assert_eq!(fid, NirFunctionId(5));
        assert_ne!(fid, NirFunctionId(6));

        let cid = NirConstantId(10);
        assert_eq!(cid, NirConstantId(10));

        let gid = NirGlobalId(3);
        assert_eq!(gid, NirGlobalId(3));

        let iid = IntrinsicId(7);
        assert_eq!(iid, IntrinsicId(7));

        let sid = SourceLocationId(99);
        assert_eq!(sid, SourceLocationId(99));
    }
}
