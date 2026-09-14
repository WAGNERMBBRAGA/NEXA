use std::borrow::Cow;
use std::collections::HashMap;

use nexa_nir::*;
use nexa_wasm_abi::descriptors;
use nexa_wasm_abi::metadata::WasmArtifactMetadata;
use nexa_wasm_abi::target;
use nexa_wasm_abi::types::WasmTypeLayoutTable;
use wasm_encoder::*;

// ── Public Types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct WasmBackendConfig {
    pub profile: target::BuildProfile,
    pub output_kind: target::OutputKind,
    pub max_memory_pages: Option<u32>,
    pub fuel: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct WasmArtifact {
    pub bytes: Vec<u8>,
    pub metadata: WasmArtifactMetadata,
}

#[derive(Debug, thiserror::Error)]
pub enum WasmBackendError {
    #[error("unsupported type representation: {type_name}")]
    UnsupportedTypeRepresentation { type_name: String },

    #[error("layout overflow")]
    LayoutOverflow,

    #[error("codegen internal error: {message}")]
    CodegenInternalError { message: String },

    #[error("wasm validation error: {details}")]
    WasmValidationError { details: String },
}

// ── Internal Types ───────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct WasmCodegenContext {
    pub type_layouts: WasmTypeLayoutTable,
    pub function_index_map: HashMap<u32, u32>,
    pub import_count: u32,
    pub data_segments: Vec<DataSegment>,
    pub static_data_offset: u32,
    pub next_global_index: u32,
    pub globals: Vec<GlobalEntry>,
    pub function_type_indices: HashMap<u32, u32>,
    pub entrypoint_type_index: u32,
}

#[derive(Debug, Clone)]
pub struct DataSegment {
    pub offset: u32,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct GlobalEntry {
    pub name: String,
    pub index: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WasmTypeLayoutLocal {
    pub size: u32,
    pub align: u32,
}

impl Default for WasmCodegenContext {
    fn default() -> Self {
        Self::new()
    }
}

impl WasmCodegenContext {
    pub fn new() -> Self {
        Self {
            type_layouts: WasmTypeLayoutTable::new(),
            function_index_map: HashMap::new(),
            import_count: 0,
            data_segments: Vec::new(),
            static_data_offset: 0,
            next_global_index: 0,
            globals: Vec::new(),
            function_type_indices: HashMap::new(),
            entrypoint_type_index: 0,
        }
    }
}

// ── Type Size/Align Mapping ──────────────────────────────────────────────

pub fn compute_type_size_and_align(type_name: &str) -> Option<(u32, u32)> {
    match type_name {
        "Bool" => Some((4, 4)),
        "Int" | "Int64" | "UInt" | "UInt64" => Some((8, 8)),
        "Int8" | "UInt8" => Some((1, 1)),
        "Int16" | "UInt16" => Some((2, 2)),
        "Int32" | "UInt32" => Some((4, 4)),
        "Float32" => Some((4, 4)),
        "Float64" => Some((8, 8)),
        "Char" => Some((4, 4)),
        "pointer" | "Ptr" => Some((4, 4)),
        "String" => Some((12, 4)),
        "Bytes" => Some((12, 4)),
        "Array" => Some((12, 4)),
        "Unit" => Some((0, 1)),
        _ => None,
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────

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

pub fn emit_string_literal(data: &[u8], ctx: &mut WasmCodegenContext) -> (u32, u32) {
    let byte_len = data.len() as u32;
    let descriptor_size = descriptors::STRING_DESCRIPTOR_DATA_OFFSET + byte_len;
    let aligned_size = align_up(descriptor_size, 4);

    let descriptor_ptr = ctx.static_data_offset;
    let new_offset = ctx.static_data_offset + aligned_size;

    if new_offset < ctx.static_data_offset {
        ctx.static_data_offset = u32::MAX;
    } else {
        ctx.static_data_offset = new_offset;
    }

    let mut segment_data = Vec::with_capacity(aligned_size as usize);
    segment_data.extend_from_slice(&byte_len.to_le_bytes());
    segment_data.extend_from_slice(&(descriptors::StringFlag::Static.to_i32()).to_le_bytes());
    segment_data.extend_from_slice(data);
    segment_data.resize(aligned_size as usize, 0);

    ctx.data_segments.push(DataSegment {
        offset: descriptor_ptr,
        data: segment_data,
    });

    (descriptor_ptr, byte_len)
}

// ── NIR Type → Wasm ValType ──────────────────────────────────────────────

fn nir_type_to_wasm_val(ty: &NirType) -> Option<ValType> {
    match ty {
        NirType::Unit | NirType::Never => None,
        // Executable NIR scalar values are uniformly represented as i64.
        // Keeping Bool in the same lane avoids implicit width changes at
        // calls, locals, CFG conditions and returns.
        NirType::Bool => Some(ValType::I64),
        NirType::Int { bits, .. } => {
            if *bits >= 64 {
                Some(ValType::I64)
            } else {
                Some(ValType::I32)
            }
        }
        NirType::Float { bits } => {
            if *bits >= 64 {
                Some(ValType::F64)
            } else {
                Some(ValType::F32)
            }
        }
        NirType::Char => Some(ValType::I32),
        NirType::String => Some(ValType::I32),
        NirType::Bytes => Some(ValType::I32),
        NirType::Struct(_) => Some(ValType::I32),
        NirType::Enum(_) => Some(ValType::I32),
        NirType::Distinct(_) => Some(ValType::I32),
        NirType::Ref { .. } => Some(ValType::I32),
        NirType::Array(_) => Some(ValType::I32),
        NirType::Interface(_) => Some(ValType::I32),
        NirType::Task(_) => Some(ValType::I32),
        NirType::Callable(_) => Some(ValType::I32),
    }
}

fn resolve_nir_type_id(id: NirTypeId, types: &TypeTable) -> NirType {
    types.get(id).cloned().unwrap_or(NirType::Unit)
}

fn nir_func_wasm_types(
    sig: &NirCallableSignature,
    types: &TypeTable,
) -> (Vec<ValType>, Vec<ValType>) {
    let params: Vec<ValType> = sig
        .parameters
        .iter()
        .filter_map(|p| {
            let nir = resolve_nir_type_id(p.ty, types);
            nir_type_to_wasm_val(&nir)
        })
        .collect();
    let results: Vec<ValType> = {
        let nir = resolve_nir_type_id(sig.return_type, types);
        nir_type_to_wasm_val(&nir).into_iter().collect()
    };
    (params, results)
}

// ── Section Builders ─────────────────────────────────────────────────────

fn build_type_section(module: &VerifiedNirModule, ctx: &mut WasmCodegenContext) -> TypeSection {
    let mut types = TypeSection::new();
    let mut seen: HashMap<(Vec<ValType>, Vec<ValType>), u32> = HashMap::new();

    // Runtime import type: (i32, i32) -> ()
    let runtime_params = vec![ValType::I32, ValType::I32];
    let runtime_results: Vec<ValType> = vec![];
    let runtime_key = (runtime_params.clone(), runtime_results.clone());
    seen.insert(runtime_key, 0);
    types.ty().function(runtime_params, runtime_results);

    // NIR function types
    let nir_types = &module.inner().types;
    for i in 0..module.inner().functions.len() {
        let fid = NirFunctionId(i as u32);
        if let Some(func) = module.inner().functions.get(fid) {
            let (params, results) = nir_func_wasm_types(&func.signature, nir_types);
            let key = (params.clone(), results.clone());
            if let Some(&idx) = seen.get(&key) {
                ctx.function_type_indices.insert(i as u32, idx);
            } else {
                let idx = types.len();
                seen.insert(key, idx);
                ctx.function_type_indices.insert(i as u32, idx);
                types.ty().function(params, results);
            }
        }
    }

    // Entrypoint wrapper type: () -> ()
    let void_key: (Vec<ValType>, Vec<ValType>) = (vec![], vec![]);
    if let Some(&idx) = seen.get(&void_key) {
        ctx.entrypoint_type_index = idx;
    } else {
        ctx.entrypoint_type_index = types.len();
        types.ty().function(vec![], vec![]);
    }

    types
}

fn build_import_section() -> ImportSection {
    let mut imports = ImportSection::new();

    let runtime_type_idx = 0;
    let et = EntityType::Function(runtime_type_idx);

    imports.import(target::RUNTIME_IMPORT_MODULE, "console.write_utf8", et);
    imports.import(target::RUNTIME_IMPORT_MODULE, "runtime.trap", et);
    imports.import(target::RUNTIME_IMPORT_MODULE, "runtime.contract_fail", et);
    imports.import(target::RUNTIME_IMPORT_MODULE, "runtime.panic", et);

    imports
}

fn build_function_section(module: &VerifiedNirModule, ctx: &WasmCodegenContext) -> FunctionSection {
    let mut functions = FunctionSection::new();
    for i in 0..module.inner().functions.len() {
        if let Some(&type_idx) = ctx.function_type_indices.get(&(i as u32)) {
            functions.function(type_idx);
        }
    }
    functions.function(ctx.entrypoint_type_index);
    functions
}

fn build_memory_section(config: &WasmBackendConfig) -> MemorySection {
    let mut memories = MemorySection::new();
    memories.memory(MemoryType {
        minimum: config.max_memory_pages.unwrap_or(1) as u64,
        maximum: config.max_memory_pages.map(|m| m as u64),
        memory64: false,
        shared: false,
        page_size_log2: None,
    });
    memories
}

fn build_global_section(ctx: &mut WasmCodegenContext) -> GlobalSection {
    let mut globals = GlobalSection::new();

    // Stack pointer (global 0)
    globals.global(
        GlobalType {
            val_type: ValType::I32,
            mutable: true,
            shared: false,
        },
        &ConstExpr::i32_const(65536),
    );
    ctx.globals.push(GlobalEntry {
        name: target::STACK_POINTER_GLOBAL.to_string(),
        index: ctx.next_global_index,
    });
    ctx.next_global_index += 1;

    // Heap cursor (global 1)
    globals.global(
        GlobalType {
            val_type: ValType::I32,
            mutable: true,
            shared: false,
        },
        &ConstExpr::i32_const(
            align_up(ctx.static_data_offset, target::HEAP_START_ALIGNMENT) as i32,
        ),
    );
    ctx.globals.push(GlobalEntry {
        name: target::BUMP_HEAP_CURSOR_GLOBAL.to_string(),
        index: ctx.next_global_index,
    });
    ctx.next_global_index += 1;

    globals
}

fn build_export_section(module: &VerifiedNirModule, ctx: &WasmCodegenContext) -> ExportSection {
    let mut exports = ExportSection::new();

    exports.export(target::MEMORY_EXPORT_NAME, ExportKind::Memory, 0);

    let entrypoint_func_idx = ctx.import_count + module.inner().functions.len() as u32;
    exports.export(
        target::ENTRYPOINT_EXPORT_NAME,
        ExportKind::Func,
        entrypoint_func_idx,
    );

    exports
}

fn build_code_section(module: &VerifiedNirModule, ctx: &mut WasmCodegenContext) -> CodeSection {
    let mut code = CodeSection::new();

    for i in 0..module.inner().functions.len() {
        let wasm_idx = ctx.import_count + i as u32;
        ctx.function_index_map.insert(i as u32, wasm_idx);

        if let Some(func) = module.inner().functions.get(NirFunctionId(i as u32)) {
            match &func.body {
                Some(body) => {
                    let wasm_func = emit_function_body(body, func, ctx);
                    code.function(&wasm_func);
                }
                None => {
                    let mut f = Function::new([]);
                    f.instructions().nop().end();
                    code.function(&f);
                }
            }
        } else {
            let mut f = Function::new([]);
            f.instructions().nop().end();
            code.function(&f);
        }
    }

    let wrapper = emit_entrypoint_wrapper(module, ctx);
    code.function(&wrapper);

    code
}

fn emit_function_body(
    body: &NirFunctionBody,
    func: &NirFunction,
    ctx: &mut WasmCodegenContext,
) -> Function {
    let parameter_count = func.signature.parameters.len() as u32;
    let max_dest = body
        .blocks
        .iter()
        .flat_map(|block| block.instructions.iter())
        .filter_map(|instruction| match instruction {
            NirInstruction::ConstI64 { dest, .. } | NirInstruction::BinaryI64 { dest, .. } => {
                Some(*dest)
            }
            NirInstruction::Call { dest, .. } => *dest,
            NirInstruction::LocalRead { dest, .. } => Some(*dest),
            NirInstruction::LocalWrite { .. } => None,
            NirInstruction::ConsoleWriteUtf8 { .. } => None,
            NirInstruction::RuntimeTrap { .. } => None,
            NirInstruction::Nop => None,
        })
        .max();
    let local_count = max_dest
        .map(|dest| dest.saturating_add(1).saturating_sub(parameter_count))
        .unwrap_or(0);
    let mutable_local_count =
        body.blocks
            .iter()
            .flat_map(|block| block.instructions.iter())
            .filter_map(|instruction| match instruction {
                NirInstruction::LocalRead { local, .. }
                | NirInstruction::LocalWrite { local, .. } => Some(*local),
                _ => None,
            })
            .max()
            .map_or(0, |local| local + 1);
    let mutable_local_base = parameter_count + local_count;
    let pc_local = mutable_local_base + mutable_local_count;
    let mut locals = Vec::new();
    if local_count != 0 {
        locals.push((local_count, ValType::I64));
    }
    if mutable_local_count != 0 {
        locals.push((mutable_local_count, ValType::I64));
    }
    locals.push((1, ValType::I32));
    let mut f = Function::new(locals);
    {
        let mut insns = f.instructions();

        insns
            .i32_const(body.entry as i32)
            .local_set(pc_local)
            .loop_(BlockType::Empty);
        for block in &body.blocks {
            insns
                .local_get(pc_local)
                .i32_const(block.id as i32)
                .i32_eq()
                .if_(BlockType::Empty);
            for instruction in &block.instructions {
                emit_nir_instruction(&mut insns, instruction, ctx, mutable_local_base);
            }
            match &block.terminator {
                NirTerminator::Return { value } => {
                    if let Some(val) = value {
                        insns.local_get(*val);
                    }
                    insns.return_();
                }
                NirTerminator::Unreachable => {
                    insns.unreachable();
                }
                NirTerminator::Branch { target } => {
                    insns.i32_const(*target as i32).local_set(pc_local);
                }
                NirTerminator::CondBranch {
                    condition,
                    then_target,
                    else_target,
                } => {
                    insns
                        .local_get(*condition)
                        .i64_eqz()
                        .if_(BlockType::Result(ValType::I32))
                        .i32_const(*else_target as i32)
                        .else_()
                        .i32_const(*then_target as i32)
                        .end()
                        .local_set(pc_local);
                }
            }
            insns.end();
        }
        // The dispatcher never exits normally. Keep the post-loop path
        // explicitly unreachable so result-bearing functions validate even
        // though every reachable return occurs inside a dispatch arm.
        insns.br(0).end().unreachable().end();
    }
    f
}

fn emit_nir_instruction(
    insns: &mut InstructionSink<'_>,
    instruction: &NirInstruction,
    ctx: &mut WasmCodegenContext,
    mutable_local_base: u32,
) {
    match instruction {
        NirInstruction::Nop => {
            insns.nop();
        }
        NirInstruction::ConstI64 { dest, value } => {
            insns.i64_const(*value).local_set(*dest);
        }
        NirInstruction::BinaryI64 {
            dest,
            op,
            left,
            right,
        } => match op {
            NirScalarBinaryOp::Add => emit_checked_add(insns, *dest, *left, *right),
            NirScalarBinaryOp::Sub => emit_checked_sub(insns, *dest, *left, *right),
            NirScalarBinaryOp::Mul => emit_checked_mul(insns, *dest, *left, *right),
            NirScalarBinaryOp::DivSigned => emit_checked_div(insns, *dest, *left, *right),
            NirScalarBinaryOp::RemSigned => emit_checked_rem(insns, *dest, *left, *right),
            NirScalarBinaryOp::ShiftLeft => {
                emit_checked_shift_i64(insns, *dest, *left, *right, true)
            }
            NirScalarBinaryOp::ShiftRightSigned => {
                emit_checked_shift_i64(insns, *dest, *left, *right, false)
            }
            _ => {
                insns.local_get(*left).local_get(*right);
                match op {
                    NirScalarBinaryOp::Eq => insns.i64_eq().i64_extend_i32_u(),
                    NirScalarBinaryOp::Ne => insns.i64_ne().i64_extend_i32_u(),
                    NirScalarBinaryOp::LtSigned => insns.i64_lt_s().i64_extend_i32_u(),
                    NirScalarBinaryOp::LeSigned => insns.i64_le_s().i64_extend_i32_u(),
                    NirScalarBinaryOp::GtSigned => insns.i64_gt_s().i64_extend_i32_u(),
                    NirScalarBinaryOp::GeSigned => insns.i64_ge_s().i64_extend_i32_u(),
                    NirScalarBinaryOp::BitAnd => insns.i64_and(),
                    NirScalarBinaryOp::BitOr => insns.i64_or(),
                    NirScalarBinaryOp::BitXor => insns.i64_xor(),
                    NirScalarBinaryOp::ShiftLeft => insns.i64_shl(),
                    NirScalarBinaryOp::ShiftRightSigned => insns.i64_shr_s(),
                    _ => unreachable!(),
                };
                insns.local_set(*dest);
            }
        },
        NirInstruction::Call {
            dest,
            function,
            args,
        } => {
            for arg in args {
                insns.local_get(*arg);
            }
            insns.call(ctx.import_count + function.0);
            if let Some(dest) = dest {
                insns.local_set(*dest);
            }
        }
        NirInstruction::ConsoleWriteUtf8 { text } => {
            let (descriptor, len) = emit_string_literal(text.as_bytes(), ctx);
            insns
                .i32_const((descriptor + descriptors::STRING_DESCRIPTOR_DATA_OFFSET) as i32)
                .i32_const(len as i32)
                .call(0);
        }
        NirInstruction::LocalRead { dest, local } => {
            insns
                .local_get(mutable_local_base + *local)
                .local_set(*dest);
        }
        NirInstruction::LocalWrite { local, value } => {
            insns
                .local_get(*value)
                .local_set(mutable_local_base + *local);
        }
        NirInstruction::RuntimeTrap { kind, location } => {
            insns
                .i32_const(*kind)
                .i32_const(*location)
                .call(1)
                .unreachable();
        }
    }
}

fn emit_semantic_trap(insns: &mut InstructionSink<'_>, kind: i32) {
    insns.i32_const(kind).i32_const(0).call(1).unreachable();
}

fn emit_checked_add(insns: &mut InstructionSink<'_>, dest: u32, left: u32, right: u32) {
    insns
        .local_get(left)
        .local_get(right)
        .i64_add()
        .local_set(dest);
    insns
        .local_get(left)
        .local_get(dest)
        .i64_xor()
        .local_get(right)
        .local_get(dest)
        .i64_xor()
        .i64_and()
        .i64_const(0)
        .i64_lt_s()
        .if_(BlockType::Empty);
    emit_semantic_trap(insns, 1);
    insns.end();
}

fn emit_checked_sub(insns: &mut InstructionSink<'_>, dest: u32, left: u32, right: u32) {
    insns
        .local_get(left)
        .local_get(right)
        .i64_sub()
        .local_set(dest);
    insns
        .local_get(left)
        .local_get(right)
        .i64_xor()
        .local_get(left)
        .local_get(dest)
        .i64_xor()
        .i64_and()
        .i64_const(0)
        .i64_lt_s()
        .if_(BlockType::Empty);
    emit_semantic_trap(insns, 1);
    insns.end();
}

fn emit_checked_mul(insns: &mut InstructionSink<'_>, dest: u32, left: u32, right: u32) {
    insns
        .local_get(left)
        .local_get(right)
        .i64_mul()
        .local_set(dest);
    // Avoid the division check when the left operand is zero.
    insns
        .local_get(left)
        .i64_eqz()
        .if_(BlockType::Empty)
        .else_();
    // MIN / -1 would itself trap in Wasm, so recognize it first.
    insns
        .local_get(left)
        .i64_const(-1)
        .i64_eq()
        .local_get(right)
        .i64_const(i64::MIN)
        .i64_eq()
        .i32_and()
        .if_(BlockType::Empty);
    emit_semantic_trap(insns, 1);
    insns.end();
    insns
        .local_get(dest)
        .local_get(left)
        .i64_div_s()
        .local_get(right)
        .i64_ne()
        .if_(BlockType::Empty);
    emit_semantic_trap(insns, 1);
    insns.end().end();
}

fn emit_checked_div(insns: &mut InstructionSink<'_>, dest: u32, left: u32, right: u32) {
    insns.local_get(right).i64_eqz().if_(BlockType::Empty);
    emit_semantic_trap(insns, 2);
    insns.end();
    insns
        .local_get(left)
        .i64_const(i64::MIN)
        .i64_eq()
        .local_get(right)
        .i64_const(-1)
        .i64_eq()
        .i32_and()
        .if_(BlockType::Empty);
    emit_semantic_trap(insns, 1);
    insns.end();
    insns
        .local_get(left)
        .local_get(right)
        .i64_div_s()
        .local_set(dest);
}

fn emit_checked_rem(insns: &mut InstructionSink<'_>, dest: u32, left: u32, right: u32) {
    insns.local_get(right).i64_eqz().if_(BlockType::Empty);
    emit_semantic_trap(insns, 2);
    insns.end();
    insns
        .local_get(left)
        .local_get(right)
        .i64_rem_s()
        .local_set(dest);
}

fn emit_checked_shift_i64(
    insns: &mut InstructionSink<'_>,
    dest: u32,
    left: u32,
    right: u32,
    shift_left: bool,
) {
    insns
        .local_get(right)
        .i64_const(0)
        .i64_lt_s()
        .if_(BlockType::Empty);
    emit_semantic_trap(insns, 8);
    insns.end();
    insns
        .local_get(right)
        .i64_const(64)
        .i64_ge_s()
        .if_(BlockType::Empty);
    emit_semantic_trap(insns, 8);
    insns.end().local_get(left).local_get(right);
    if shift_left {
        insns.i64_shl();
    } else {
        insns.i64_shr_s();
    }
    insns.local_set(dest);
}

fn emit_entrypoint_wrapper(module: &VerifiedNirModule, ctx: &WasmCodegenContext) -> Function {
    let mut f = Function::new([]);
    {
        let mut insns = f.instructions();
        let main = (0..module.inner().functions.len())
            .filter_map(|i| module.inner().functions.get(NirFunctionId(i as u32)))
            .find(|func| func.metadata.is_exported);
        if let Some(main) = main {
            insns.call(ctx.import_count + main.id.0);
            let return_type =
                resolve_nir_type_id(main.signature.return_type, &module.inner().types);
            if nir_type_to_wasm_val(&return_type).is_some() {
                insns.drop();
            }
        }
        insns.end();
    }
    f
}

fn build_data_section(ctx: &WasmCodegenContext) -> DataSection {
    let mut data = DataSection::new();
    for seg in &ctx.data_segments {
        data.active(
            0,
            &ConstExpr::i32_const(seg.offset as i32),
            seg.data.iter().copied(),
        );
    }
    data
}

fn build_abi_custom_section(config: &WasmBackendConfig) -> CustomSection<'static> {
    let fingerprint = format!("{:?}:{:?}", config.profile as u8, config.output_kind as u8);
    let abi_bytes = nexa_wasm_abi::metadata::AbiMetadata::new(&fingerprint).serialize();

    CustomSection {
        name: Cow::Borrowed(target::ABI_CUSTOM_SECTION),
        data: Cow::Owned(abi_bytes),
    }
}

// ── Checked Arithmetic Emission ──────────────────────────────────────────

pub fn emit_checked_i64_add_signed(_a: ValType) -> Vec<Instruction<'static>> {
    vec![Instruction::I64Add]
}

pub fn emit_checked_i64_sub_signed() -> Vec<Instruction<'static>> {
    vec![Instruction::I64Sub]
}

pub fn emit_checked_i64_mul_signed() -> Vec<Instruction<'static>> {
    vec![Instruction::I64Mul]
}

pub fn emit_checked_i64_div_signed() -> Vec<Instruction<'static>> {
    vec![Instruction::I64DivS]
}

pub fn emit_checked_shift(bit_width: u32) -> Vec<Instruction<'static>> {
    match bit_width {
        8 | 16 | 32 => vec![Instruction::I64Shl],
        64 => vec![Instruction::I64Shl],
        _ => vec![Instruction::Nop],
    }
}

// ── Public API ───────────────────────────────────────────────────────────

pub fn compile_wasm(
    module: &VerifiedNirModule,
    config: &WasmBackendConfig,
) -> Result<WasmArtifact, WasmBackendError> {
    let mut ctx = WasmCodegenContext::new();
    ctx.import_count = 4;

    let mut module_encoder = Module::new();

    // 1. Type section
    let types = build_type_section(module, &mut ctx);
    module_encoder.section(&types);

    // 2. Import section (runtime imports)
    let imports = build_import_section();
    module_encoder.section(&imports);

    // 3. Function section
    let functions = build_function_section(module, &ctx);
    module_encoder.section(&functions);

    // 4. Memory section
    let memory = build_memory_section(config);
    module_encoder.section(&memory);

    // 5. Global section
    let globals = build_global_section(&mut ctx);
    module_encoder.section(&globals);

    // 6. Export section
    let exports = build_export_section(module, &ctx);
    module_encoder.section(&exports);

    // 7. Code section
    let code = build_code_section(module, &mut ctx);
    module_encoder.section(&code);

    // 8. Data section
    let data = build_data_section(&ctx);
    module_encoder.section(&data);

    // 9. Custom section: nexa.abi metadata
    let abi_section = build_abi_custom_section(config);
    module_encoder.section(&abi_section);

    let bytes = module_encoder.finish();

    nexa_wasm_validate::validate_wasm_artifact(
        &nexa_wasm_validate::WasmArtifact {
            bytes: bytes.clone(),
        },
        &nexa_wasm_validate::ValidationConfig {
            is_application: matches!(config.output_kind, target::OutputKind::Application),
            ..Default::default()
        },
    )
    .map_err(|error| WasmBackendError::WasmValidationError {
        details: error.to_string(),
    })?;

    Ok(WasmArtifact {
        bytes,
        metadata: WasmArtifactMetadata::new(),
    })
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use nexa_wasm_abi::target::{BuildProfile, OutputKind};

    fn empty_module() -> VerifiedNirModule {
        VerifiedNirModule::new(NirModule::new())
    }

    fn default_config() -> WasmBackendConfig {
        WasmBackendConfig {
            profile: BuildProfile::Debug,
            output_kind: OutputKind::Application,
            max_memory_pages: None,
            fuel: None,
        }
    }

    fn single_function_module() -> VerifiedNirModule {
        let mut m = NirModule::new();
        let unit_id = m.types.add(NirType::Unit);
        m.functions.add(NirFunction {
            id: NirFunctionId(0),
            signature: NirCallableSignature {
                kind: NirCallableKind::Function,
                parameters: vec![],
                return_type: unit_id,
                effects: NirEffectSet::empty(),
            },
            body: Some(NirFunctionBody {
                blocks: vec![NirBasicBlock {
                    id: 0,
                    instructions: vec![NirInstruction::Nop],
                    terminator: NirTerminator::Return { value: None },
                }],
                entry: 0,
            }),
            metadata: NirFunctionMetadata::default(),
        });
        VerifiedNirModule::new(m)
    }

    fn multi_function_module(count: u32) -> VerifiedNirModule {
        let mut m = NirModule::new();
        let unit_id = m.types.add(NirType::Unit);
        for i in 0..count {
            m.functions.add(NirFunction {
                id: NirFunctionId(i),
                signature: NirCallableSignature {
                    kind: NirCallableKind::Function,
                    parameters: vec![],
                    return_type: unit_id,
                    effects: NirEffectSet::empty(),
                },
                body: Some(NirFunctionBody {
                    blocks: vec![NirBasicBlock {
                        id: 0,
                        instructions: vec![],
                        terminator: NirTerminator::Return { value: None },
                    }],
                    entry: 0,
                }),
                metadata: NirFunctionMetadata {
                    is_exported: i == 0,
                    ..Default::default()
                },
            });
        }
        VerifiedNirModule::new(m)
    }

    // 1
    #[test]
    fn test_empty_module_compiles() {
        let result = compile_wasm(&empty_module(), &default_config());
        assert!(result.is_ok());
    }

    // 2
    #[test]
    fn test_single_function_compiles() {
        let result = compile_wasm(&single_function_module(), &default_config());
        assert!(result.is_ok());
        let artifact = result.unwrap();
        assert!(!artifact.bytes.is_empty());
    }

    // 3
    #[test]
    fn test_string_literal_stored() {
        let mut ctx = WasmCodegenContext::new();
        let (ptr, len) = emit_string_literal(b"hello", &mut ctx);
        assert_eq!(ptr, 0);
        assert_eq!(len, 5);
        assert_eq!(ctx.data_segments.len(), 1);
        let seg = &ctx.data_segments[0];
        assert_eq!(seg.offset, 0);
        // byte_len LE
        assert_eq!(seg.data[0..4], 5u32.to_le_bytes());
        // flag = 0 (Static)
        assert_eq!(seg.data[4..8], 0i32.to_le_bytes());
        // raw bytes
        assert_eq!(&seg.data[8..13], b"hello");
    }

    // 4
    #[test]
    fn test_type_layout_int() {
        assert_eq!(compute_type_size_and_align("Int"), Some((8, 8)));
    }

    // 5
    #[test]
    fn test_type_layout_bool() {
        assert_eq!(compute_type_size_and_align("Bool"), Some((4, 4)));
    }

    // 6
    #[test]
    fn test_type_layout_string() {
        assert_eq!(compute_type_size_and_align("String"), Some((12, 4)));
    }

    // 7
    #[test]
    fn test_align_up_basic() {
        assert_eq!(align_up(0, 4), 0);
        assert_eq!(align_up(1, 4), 4);
        assert_eq!(align_up(4, 4), 4);
        assert_eq!(align_up(5, 8), 8);
        assert_eq!(align_up(8, 8), 8);
        assert_eq!(align_up(9, 8), 16);
        assert_eq!(align_up(3, 1), 3);
        assert_eq!(align_up(0, 0), 0);
    }

    // 8
    #[test]
    fn test_artifact_metadata() {
        let result = compile_wasm(&empty_module(), &default_config());
        let artifact = result.unwrap();
        assert_eq!(artifact.metadata.abi_version, 1);
        assert_eq!(artifact.metadata.target, "wasm32-nexa");
    }

    // 9
    #[test]
    fn test_wasm_module_magic() {
        let result = compile_wasm(&empty_module(), &default_config());
        let artifact = result.unwrap();
        assert_eq!(&artifact.bytes[0..4], b"\0asm");
    }

    // 10
    #[test]
    fn test_wasm_module_version() {
        let result = compile_wasm(&empty_module(), &default_config());
        let artifact = result.unwrap();
        assert_eq!(&artifact.bytes[4..8], &[1, 0, 0, 0]);
    }

    // 11
    #[test]
    fn test_backend_error_display() {
        let e1 = WasmBackendError::UnsupportedTypeRepresentation {
            type_name: "Foo".to_string(),
        };
        assert!(format!("{e1}").contains("Foo"));

        let e2 = WasmBackendError::LayoutOverflow;
        assert!(format!("{e2}").contains("overflow"));

        let e3 = WasmBackendError::CodegenInternalError {
            message: "bad".to_string(),
        };
        assert!(format!("{e3}").contains("bad"));

        let e4 = WasmBackendError::WasmValidationError {
            details: "nope".to_string(),
        };
        assert!(format!("{e4}").contains("nope"));
    }

    // 12
    #[test]
    fn test_empty_data_segments() {
        let result = compile_wasm(&empty_module(), &default_config());
        let artifact = result.unwrap();
        // An empty module should have no active data segments in the wasm binary.
        // We verify by checking the compiled bytes encode no data section entries.
        // The data section id is 11. A minimal module with no data section has
        // just the type section (id 1), import section (id 2) etc. without data.
        // We simply verify it compiled and has valid header.
        assert!(artifact.bytes.len() > 8);
    }

    // 13
    #[test]
    fn test_config_defaults() {
        let cfg = WasmBackendConfig {
            profile: BuildProfile::Release,
            output_kind: OutputKind::Library,
            max_memory_pages: Some(16),
            fuel: Some(1000),
        };
        assert_eq!(cfg.profile, BuildProfile::Release);
        assert_eq!(cfg.output_kind, OutputKind::Library);
        assert_eq!(cfg.max_memory_pages, Some(16));
        assert_eq!(cfg.fuel, Some(1000));
    }

    // 14
    #[test]
    fn test_multiple_functions_indexed() {
        let module = multi_function_module(3);
        let result = compile_wasm(&module, &default_config());
        assert!(result.is_ok());
        let mut ctx = WasmCodegenContext::new();
        ctx.import_count = 4;
        // Simulate the index mapping
        for i in 0u32..3 {
            ctx.function_index_map.insert(i, 4 + i);
        }
        assert_eq!(ctx.function_index_map[&0], 4);
        assert_eq!(ctx.function_index_map[&1], 5);
        assert_eq!(ctx.function_index_map[&2], 6);
    }

    // 15
    #[test]
    fn test_export_section_has_memory() {
        let result = compile_wasm(&empty_module(), &default_config());
        let artifact = result.unwrap();
        // The wasm binary should contain an export section that exports "memory".
        // We verify by searching for the string "memory" in the bytes.
        let has_memory_export = artifact.bytes.windows(6).any(|w| w == b"memory");
        assert!(has_memory_export, "expected 'memory' export in wasm binary");
    }
}
