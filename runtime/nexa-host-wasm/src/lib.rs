use std::collections::HashSet;

use nexa_wasm_abi::imports::{self, is_known_import, WasmValType};
use nexa_wasm_abi::target::{ENTRYPOINT_EXPORT_NAME, MEMORY_EXPORT_NAME};
use wasmparser::{DataKind, ExternalKind, Operator, Parser, Payload, TypeRef};

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("invalid UTF-8 in string")]
    InvalidUtf8,

    #[error("pointer {ptr} with length {len} is outside memory bounds")]
    PointerOutsideMemory { ptr: u32, len: u32 },

    #[error("pointer {ptr} + length {len} overflows u32")]
    PtrLenOverflow { ptr: u32, len: u32 },

    #[error("capability denied: {capability}")]
    CapabilityDenied { capability: String },

    #[error("unexpected trap: {message}")]
    UnexpectedTrap { message: String },

    #[error("fuel limit exceeded")]
    FuelLimitExceeded,

    #[error("memory limit exceeded")]
    MemoryLimitExceeded,

    #[error("missing entrypoint: {0}")]
    MissingEntrypoint(String),

    #[error("invalid WASM module: {details}")]
    InvalidWasmModule { details: String },
}

#[derive(Debug, Clone)]
pub struct WasmExecutionContext {
    pub allowed_effects: HashSet<String>,
    pub max_memory_pages: Option<u32>,
    pub fuel: Option<u64>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl Default for WasmExecutionContext {
    fn default() -> Self {
        Self {
            allowed_effects: ["console::write".to_string()].into_iter().collect(),
            max_memory_pages: None,
            fuel: None,
            stdout: Vec::new(),
            stderr: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionResult {
    pub exit_code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct ValidatedWasmArtifact {
    pub bytes: Vec<u8>,
    pub imports: Vec<ArtifactImport>,
    pub exports: Vec<ArtifactExport>,
    pub has_memory: bool,
    pub has_entrypoint: bool,
}

#[derive(Debug, Clone)]
pub struct ArtifactImport {
    pub module: String,
    pub name: String,
    pub params: Vec<WasmValType>,
    pub results: Vec<WasmValType>,
}

#[derive(Debug, Clone)]
pub struct ArtifactExport {
    pub name: String,
    pub kind: ExportKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportKind {
    Function,
    Global,
    Memory,
    Table,
}

impl ValidatedWasmArtifact {
    pub fn new(wasm_bytes: Vec<u8>) -> Result<Self, RuntimeError> {
        Self::parse_and_validate(&wasm_bytes)
    }

    fn parse_and_validate(bytes: &[u8]) -> Result<Self, RuntimeError> {
        if bytes.len() < 8 {
            return Err(RuntimeError::InvalidWasmModule {
                details: "WASM binary too short".to_string(),
            });
        }

        if &bytes[0..4] != b"\0asm" {
            return Err(RuntimeError::InvalidWasmModule {
                details: "missing WASM magic bytes".to_string(),
            });
        }

        let version = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        if version != 1 {
            return Err(RuntimeError::InvalidWasmModule {
                details: format!("unsupported WASM version: {}", version),
            });
        }

        let mut imports = Vec::new();
        let mut exports = Vec::new();
        let mut has_memory = false;
        let mut has_entrypoint = false;

        let mut pos = 8;
        while pos < bytes.len() {
            if pos >= bytes.len() {
                break;
            }
            let section_id = bytes[pos];
            pos += 1;

            let (section_size, new_pos) = match decode_leb128_u32(bytes, pos) {
                Some(v) => v,
                None => break,
            };
            pos = new_pos;

            let section_end = pos + section_size as usize;
            if section_end > bytes.len() {
                break;
            }

            match section_id {
                2 => {
                    let (imps, end) = parse_import_section(&bytes[pos..section_end])?;
                    imports = imps;
                    pos = section_end;
                    let _ = end;
                }
                7 => {
                    let (exps, end) = parse_export_section(&bytes[pos..section_end])?;
                    for e in &exps {
                        if e.name == MEMORY_EXPORT_NAME {
                            has_memory = true;
                        }
                        if e.name == ENTRYPOINT_EXPORT_NAME && e.kind == ExportKind::Function {
                            has_entrypoint = true;
                        }
                    }
                    exports = exps;
                    pos = section_end;
                    let _ = end;
                }
                _ => {
                    pos = section_end;
                }
            }
        }

        if !has_memory {
            return Err(RuntimeError::InvalidWasmModule {
                details: "no memory export found".to_string(),
            });
        }

        Ok(Self {
            bytes: bytes.to_vec(),
            imports,
            exports,
            has_memory,
            has_entrypoint,
        })
    }
}

fn decode_leb128_u32(bytes: &[u8], start: usize) -> Option<(u32, usize)> {
    let mut result: u32 = 0;
    let mut shift: u32 = 0;
    let mut pos = start;
    loop {
        if pos >= bytes.len() {
            return None;
        }
        let byte = bytes[pos];
        pos += 1;
        result |= ((byte & 0x7F) as u32) << shift;
        if byte & 0x80 == 0 {
            return Some((result, pos));
        }
        shift += 7;
        if shift >= 35 {
            return None;
        }
    }
}

fn parse_import_section(bytes: &[u8]) -> Result<(Vec<ArtifactImport>, usize), RuntimeError> {
    let (count, mut pos) = match decode_leb128_u32(bytes, 0) {
        Some(v) => v,
        None => return Ok((Vec::new(), 0)),
    };

    let mut imports = Vec::new();
    for _ in 0..count {
        let (mod_len, new_pos) = match decode_leb128_u32(bytes, pos) {
            Some(v) => v,
            None => break,
        };
        pos = new_pos;

        if pos + mod_len as usize > bytes.len() {
            break;
        }
        let module = String::from_utf8(bytes[pos..pos + mod_len as usize].to_vec())
            .map_err(|_| RuntimeError::InvalidUtf8)?;
        pos += mod_len as usize;

        let (name_len, new_pos) = match decode_leb128_u32(bytes, pos) {
            Some(v) => v,
            None => break,
        };
        pos = new_pos;

        if pos + name_len as usize > bytes.len() {
            break;
        }
        let name = String::from_utf8(bytes[pos..pos + name_len as usize].to_vec())
            .map_err(|_| RuntimeError::InvalidUtf8)?;
        pos += name_len as usize;

        if pos >= bytes.len() {
            break;
        }
        let kind = bytes[pos];
        pos += 1;

        if kind == 0 {
            // A function import stores a type-section index, not the inline
            // function signature. The baseline NEXA runtime imports all share
            // the canonical `(i32, i32) -> ()` ABI signature.
            let (_type_index, new_pos) = match decode_leb128_u32(bytes, pos) {
                Some(v) => v,
                None => break,
            };
            pos = new_pos;

            imports.push(ArtifactImport {
                module,
                name,
                params: vec![WasmValType::I32, WasmValType::I32],
                results: vec![],
            });
        }
    }

    Ok((imports, pos))
}

fn parse_export_section(bytes: &[u8]) -> Result<(Vec<ArtifactExport>, usize), RuntimeError> {
    let (count, mut pos) = match decode_leb128_u32(bytes, 0) {
        Some(v) => v,
        None => return Ok((Vec::new(), 0)),
    };

    let mut exports = Vec::new();
    for _ in 0..count {
        let (name_len, new_pos) = match decode_leb128_u32(bytes, pos) {
            Some(v) => v,
            None => break,
        };
        pos = new_pos;

        if pos + name_len as usize > bytes.len() {
            break;
        }
        let name = String::from_utf8(bytes[pos..pos + name_len as usize].to_vec())
            .map_err(|_| RuntimeError::InvalidUtf8)?;
        pos += name_len as usize;

        if pos >= bytes.len() {
            break;
        }
        let kind_byte = bytes[pos];
        pos += 1;

        let kind = match kind_byte {
            0 => ExportKind::Function,
            1 => ExportKind::Table,
            2 => ExportKind::Memory,
            3 => ExportKind::Global,
            _ => ExportKind::Function,
        };

        let (_, new_pos) = match decode_leb128_u32(bytes, pos) {
            Some(v) => v,
            None => break,
        };
        pos = new_pos;

        exports.push(ArtifactExport { name, kind });
    }

    Ok((exports, pos))
}

pub fn handle_console_write_utf8(
    ptr: u32,
    len: u32,
    memory: &[u8],
    stdout: &mut Vec<u8>,
) -> Result<(), RuntimeError> {
    let end = ptr
        .checked_add(len)
        .ok_or(RuntimeError::PtrLenOverflow { ptr, len })?;

    if end as usize > memory.len() {
        return Err(RuntimeError::PointerOutsideMemory { ptr, len });
    }

    let bytes = &memory[ptr as usize..end as usize];
    std::str::from_utf8(bytes).map_err(|_| RuntimeError::InvalidUtf8)?;

    stdout.extend_from_slice(bytes);
    Ok(())
}

pub fn handle_runtime_trap(kind: i32, location: i32) -> Result<(), RuntimeError> {
    let message = match kind {
        0 => format!("unreachable at location {}", location),
        1 => format!("integer overflow at location {}", location),
        2 => format!("integer division by zero at location {}", location),
        3 => format!("invalid conversion to integer at location {}", location),
        4 => format!("out of bounds memory access at location {}", location),
        5 => format!("indirect call type mismatch at location {}", location),
        6 => format!("stack overflow at location {}", location),
        7 => format!("stack underflow at location {}", location),
        8 => format!("invalid integer shift count at location {}", location),
        _ => format!("unknown trap (kind={}) at location {}", kind, location),
    };
    Err(RuntimeError::UnexpectedTrap { message })
}

pub fn handle_runtime_panic(msg_ptr: u32, msg_len: u32, memory: &[u8]) -> Result<(), RuntimeError> {
    let end = msg_ptr
        .checked_add(msg_len)
        .ok_or(RuntimeError::PtrLenOverflow {
            ptr: msg_ptr,
            len: msg_len,
        })?;

    if end as usize > memory.len() {
        return Err(RuntimeError::PointerOutsideMemory {
            ptr: msg_ptr,
            len: msg_len,
        });
    }

    let bytes = &memory[msg_ptr as usize..end as usize];
    let message = std::str::from_utf8(bytes)
        .map_err(|_| RuntimeError::InvalidUtf8)?
        .to_string();

    Err(RuntimeError::UnexpectedTrap { message })
}

pub fn run_wasm(
    artifact: &ValidatedWasmArtifact,
    context: &WasmExecutionContext,
) -> Result<ExecutionResult, RuntimeError> {
    if artifact.bytes.is_empty() {
        return Err(RuntimeError::InvalidWasmModule {
            details: "empty WASM module".to_string(),
        });
    }

    if artifact.bytes.len() >= 4 && &artifact.bytes[0..4] != b"\0asm" {
        return Err(RuntimeError::InvalidWasmModule {
            details: "missing WASM magic bytes".to_string(),
        });
    }

    if !artifact.has_memory {
        return Err(RuntimeError::InvalidWasmModule {
            details: "no memory export".to_string(),
        });
    }

    if !artifact.has_entrypoint {
        return Err(RuntimeError::MissingEntrypoint(
            ENTRYPOINT_EXPORT_NAME.to_string(),
        ));
    }

    for import in &artifact.imports {
        if !is_known_import(&import.module, &import.name) {
            return Err(RuntimeError::InvalidWasmModule {
                details: format!("unknown import: {}.{}", import.module, import.name),
            });
        }
        if import.name == imports::CONSOLE_WRITE_UTF8
            && !context.allowed_effects.contains("console::write")
        {
            return Err(RuntimeError::CapabilityDenied {
                capability: "console::write".to_string(),
            });
        }
    }

    for import in &artifact.imports {
        match import.name.as_str() {
            imports::CONSOLE_WRITE_UTF8 => {
                if import.params.len() != 2 {
                    return Err(RuntimeError::InvalidWasmModule {
                        details: format!(
                            "{} expects 2 i32 params, got {}",
                            imports::CONSOLE_WRITE_UTF8,
                            import.params.len()
                        ),
                    });
                }
                if !import.results.is_empty() {
                    return Err(RuntimeError::InvalidWasmModule {
                        details: format!(
                            "{} expects no results, got {}",
                            imports::CONSOLE_WRITE_UTF8,
                            import.results.len()
                        ),
                    });
                }
            }
            #[allow(clippy::collapsible_match)]
            imports::RUNTIME_TRAP | imports::RUNTIME_PANIC | imports::RUNTIME_CONTRACT_FAIL => {
                if import.params.len() != 2 {
                    return Err(RuntimeError::InvalidWasmModule {
                        details: format!(
                            "{} expects 2 i32 params, got {}",
                            import.name,
                            import.params.len()
                        ),
                    });
                }
            }
            _ => {}
        }
    }

    let stdout = execute_supported_wasm(
        &artifact.bytes,
        context.fuel,
        context.max_memory_pages,
        context.stdout.clone(),
    )?;

    Ok(ExecutionResult {
        exit_code: 0,
        stdout,
        stderr: context.stderr.clone(),
    })
}

#[derive(Debug, Clone, Copy)]
struct FunctionSignature {
    params: usize,
    returns_value: bool,
}

#[derive(Debug, Clone)]
struct ExecutableFunction {
    local_count: usize,
    instructions: Vec<ExecutableInstruction>,
}

#[derive(Debug, Clone, Copy)]
enum ExecutableInstruction {
    Nop,
    I64Const(i64),
    LocalGet(u32),
    LocalSet(u32),
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
    ExtendI32Unsigned,
    I32Eq,
    I32And,
    I64Eqz,
    Drop,
    Call(u32),
    Return,
    Loop,
    If,
    Else,
    Br(u32),
    End,
    Unreachable,
}

struct ExecutableModule {
    signatures: Vec<FunctionSignature>,
    functions: Vec<ExecutableFunction>,
    imported_function_count: u32,
    entrypoint: u32,
    imported_function_names: Vec<String>,
    memory: Vec<u8>,
}

fn execute_supported_wasm(
    bytes: &[u8],
    fuel: Option<u64>,
    max_memory_pages: Option<u32>,
    mut stdout: Vec<u8>,
) -> Result<Vec<u8>, RuntimeError> {
    let module = parse_executable_module(bytes)?;
    if let Some(max_pages) = max_memory_pages {
        if module.memory.len() / 65_536 > max_pages as usize {
            return Err(RuntimeError::MemoryLimitExceeded);
        }
    }
    let mut remaining_fuel = fuel;
    execute_function(
        &module,
        module.entrypoint,
        Vec::new(),
        &mut remaining_fuel,
        &mut stdout,
    )?;
    Ok(stdout)
}

fn parse_executable_module(bytes: &[u8]) -> Result<ExecutableModule, RuntimeError> {
    let mut types = Vec::new();
    let mut imported_types = Vec::new();
    let mut defined_types = Vec::new();
    let mut functions = Vec::new();
    let mut entrypoint = None;
    let mut imported_function_names = Vec::new();
    let mut memory = Vec::new();

    for payload in Parser::new(0).parse_all(bytes) {
        let payload = payload.map_err(|error| RuntimeError::InvalidWasmModule {
            details: error.to_string(),
        })?;
        match payload {
            Payload::TypeSection(reader) => {
                for group in reader {
                    let group = group.map_err(|error| RuntimeError::InvalidWasmModule {
                        details: error.to_string(),
                    })?;
                    for subtype in group.types() {
                        let function = subtype.composite_type.unwrap_func();
                        types.push(FunctionSignature {
                            params: function.params().len(),
                            returns_value: !function.results().is_empty(),
                        });
                    }
                }
            }
            Payload::ImportSection(reader) => {
                for import in reader.into_imports() {
                    let import = import.map_err(|error| RuntimeError::InvalidWasmModule {
                        details: error.to_string(),
                    })?;
                    if let TypeRef::Func(index) = import.ty {
                        imported_types.push(index);
                        imported_function_names.push(import.name.to_string());
                    }
                }
            }
            Payload::MemorySection(reader) => {
                for memory_type in reader {
                    let memory_type =
                        memory_type.map_err(|error| RuntimeError::InvalidWasmModule {
                            details: error.to_string(),
                        })?;
                    let bytes = memory_type.initial.saturating_mul(65_536);
                    let bytes =
                        usize::try_from(bytes).map_err(|_| RuntimeError::MemoryLimitExceeded)?;
                    memory.resize(bytes, 0);
                }
            }
            Payload::FunctionSection(reader) => {
                for index in reader {
                    defined_types.push(index.map_err(|error| RuntimeError::InvalidWasmModule {
                        details: error.to_string(),
                    })?);
                }
            }
            Payload::ExportSection(reader) => {
                for export in reader {
                    let export = export.map_err(|error| RuntimeError::InvalidWasmModule {
                        details: error.to_string(),
                    })?;
                    if export.name == ENTRYPOINT_EXPORT_NAME && export.kind == ExternalKind::Func {
                        entrypoint = Some(export.index);
                    }
                }
            }
            Payload::CodeSectionEntry(body) => {
                let mut local_count = 0usize;
                let locals =
                    body.get_locals_reader()
                        .map_err(|error| RuntimeError::InvalidWasmModule {
                            details: error.to_string(),
                        })?;
                for local in locals {
                    let (count, _) = local.map_err(|error| RuntimeError::InvalidWasmModule {
                        details: error.to_string(),
                    })?;
                    local_count = local_count.saturating_add(count as usize);
                }
                let mut instructions = Vec::new();
                let operators = body.get_operators_reader().map_err(|error| {
                    RuntimeError::InvalidWasmModule {
                        details: error.to_string(),
                    }
                })?;
                for operator in operators {
                    let operator = operator.map_err(|error| RuntimeError::InvalidWasmModule {
                        details: error.to_string(),
                    })?;
                    instructions.push(convert_operator(operator)?);
                }
                functions.push(ExecutableFunction {
                    local_count,
                    instructions,
                });
            }
            Payload::DataSection(reader) => {
                for data in reader {
                    let data = data.map_err(|error| RuntimeError::InvalidWasmModule {
                        details: error.to_string(),
                    })?;
                    let offset = match data.kind {
                        DataKind::Active { offset_expr, .. } => {
                            let mut operators = offset_expr.get_operators_reader();
                            match operators.read().map_err(|error| {
                                RuntimeError::InvalidWasmModule {
                                    details: error.to_string(),
                                }
                            })? {
                                Operator::I32Const { value } if value >= 0 => value as usize,
                                other => {
                                    return Err(RuntimeError::InvalidWasmModule {
                                        details: format!(
                                            "unsupported data offset expression: {other:?}"
                                        ),
                                    })
                                }
                            }
                        }
                        DataKind::Passive => continue,
                    };
                    let end = offset
                        .checked_add(data.data.len())
                        .ok_or(RuntimeError::MemoryLimitExceeded)?;
                    if end > memory.len() {
                        return Err(RuntimeError::MemoryLimitExceeded);
                    }
                    memory[offset..end].copy_from_slice(data.data);
                }
            }
            _ => {}
        }
    }

    let signatures = imported_types
        .iter()
        .chain(defined_types.iter())
        .map(|index| {
            types
                .get(*index as usize)
                .copied()
                .ok_or_else(|| RuntimeError::InvalidWasmModule {
                    details: format!("function references unknown type {index}"),
                })
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(ExecutableModule {
        signatures,
        functions,
        imported_function_count: imported_types.len() as u32,
        entrypoint: entrypoint
            .ok_or_else(|| RuntimeError::MissingEntrypoint(ENTRYPOINT_EXPORT_NAME.to_string()))?,
        imported_function_names,
        memory,
    })
}

fn convert_operator(operator: Operator<'_>) -> Result<ExecutableInstruction, RuntimeError> {
    let instruction = match operator {
        Operator::Nop => ExecutableInstruction::Nop,
        Operator::I64Const { value } => ExecutableInstruction::I64Const(value),
        Operator::I32Const { value } => ExecutableInstruction::I64Const(value as i64),
        Operator::LocalGet { local_index } => ExecutableInstruction::LocalGet(local_index),
        Operator::LocalSet { local_index } => ExecutableInstruction::LocalSet(local_index),
        Operator::I64Add => ExecutableInstruction::Add,
        Operator::I64Sub => ExecutableInstruction::Sub,
        Operator::I64Mul => ExecutableInstruction::Mul,
        Operator::I64DivS => ExecutableInstruction::DivSigned,
        Operator::I64RemS => ExecutableInstruction::RemSigned,
        Operator::I64Eq => ExecutableInstruction::Eq,
        Operator::I64Ne => ExecutableInstruction::Ne,
        Operator::I64LtS => ExecutableInstruction::LtSigned,
        Operator::I64LeS => ExecutableInstruction::LeSigned,
        Operator::I64GtS => ExecutableInstruction::GtSigned,
        Operator::I64GeS => ExecutableInstruction::GeSigned,
        Operator::I64And => ExecutableInstruction::BitAnd,
        Operator::I64Or => ExecutableInstruction::BitOr,
        Operator::I64Xor => ExecutableInstruction::BitXor,
        Operator::I64Shl => ExecutableInstruction::ShiftLeft,
        Operator::I64ShrS => ExecutableInstruction::ShiftRightSigned,
        Operator::I64ExtendI32U => ExecutableInstruction::ExtendI32Unsigned,
        Operator::I32Eq => ExecutableInstruction::I32Eq,
        Operator::I32And => ExecutableInstruction::I32And,
        Operator::I64Eqz => ExecutableInstruction::I64Eqz,
        Operator::Drop => ExecutableInstruction::Drop,
        Operator::Call { function_index } => ExecutableInstruction::Call(function_index),
        Operator::Return => ExecutableInstruction::Return,
        Operator::Loop { .. } => ExecutableInstruction::Loop,
        Operator::If { .. } => ExecutableInstruction::If,
        Operator::Else => ExecutableInstruction::Else,
        Operator::Br { relative_depth } => ExecutableInstruction::Br(relative_depth),
        Operator::End => ExecutableInstruction::End,
        Operator::Unreachable => ExecutableInstruction::Unreachable,
        other => {
            return Err(RuntimeError::InvalidWasmModule {
                details: format!("unsupported executable instruction: {other:?}"),
            })
        }
    };
    Ok(instruction)
}

fn execute_function(
    module: &ExecutableModule,
    function_index: u32,
    args: Vec<i64>,
    fuel: &mut Option<u64>,
    stdout: &mut Vec<u8>,
) -> Result<Option<i64>, RuntimeError> {
    if function_index < module.imported_function_count {
        let name = module
            .imported_function_names
            .get(function_index as usize)
            .map(String::as_str)
            .ok_or_else(|| RuntimeError::InvalidWasmModule {
                details: format!("unknown imported function {function_index}"),
            })?;
        return match name {
            imports::CONSOLE_WRITE_UTF8 => {
                let [ptr, len] = args.as_slice() else {
                    return Err(RuntimeError::InvalidWasmModule {
                        details: "console.write_utf8 expects two arguments".to_string(),
                    });
                };
                handle_console_write_utf8(*ptr as u32, *len as u32, &module.memory, stdout)?;
                Ok(None)
            }
            imports::RUNTIME_TRAP => {
                let [kind, location] = args.as_slice() else {
                    return Err(RuntimeError::InvalidWasmModule {
                        details: "runtime.trap expects two arguments".to_string(),
                    });
                };
                handle_runtime_trap(*kind as i32, *location as i32)?;
                Ok(None)
            }
            other => Err(RuntimeError::UnexpectedTrap {
                message: format!("runtime import `{other}` was invoked"),
            }),
        };
    }
    let signature = module
        .signatures
        .get(function_index as usize)
        .ok_or_else(|| RuntimeError::InvalidWasmModule {
            details: format!("unknown function index {function_index}"),
        })?;
    if args.len() != signature.params {
        return Err(RuntimeError::InvalidWasmModule {
            details: format!(
                "function {function_index} expected {} arguments, received {}",
                signature.params,
                args.len()
            ),
        });
    }
    let function = module
        .functions
        .get((function_index - module.imported_function_count) as usize)
        .ok_or_else(|| RuntimeError::InvalidWasmModule {
            details: format!("function {function_index} has no body"),
        })?;
    let mut locals = args;
    locals.resize(locals.len() + function.local_count, 0);
    let mut stack = Vec::new();

    let mut instruction_index = 0usize;
    while instruction_index < function.instructions.len() {
        let instruction = function.instructions[instruction_index];
        consume_fuel(fuel)?;
        match instruction {
            ExecutableInstruction::Nop => {}
            ExecutableInstruction::I64Const(value) => stack.push(value),
            ExecutableInstruction::LocalGet(index) => {
                stack.push(*locals.get(index as usize).ok_or_else(|| {
                    RuntimeError::UnexpectedTrap {
                        message: format!("unknown local {index}"),
                    }
                })?)
            }
            ExecutableInstruction::LocalSet(index) => {
                let value = pop_value(&mut stack)?;
                *locals
                    .get_mut(index as usize)
                    .ok_or_else(|| RuntimeError::UnexpectedTrap {
                        message: format!("unknown local {index}"),
                    })? = value;
            }
            ExecutableInstruction::Add => binary_overflow_checked(&mut stack, i64::checked_add)?,
            ExecutableInstruction::Sub => binary_overflow_checked(&mut stack, i64::checked_sub)?,
            ExecutableInstruction::Mul => binary_overflow_checked(&mut stack, i64::checked_mul)?,
            ExecutableInstruction::DivSigned => binary_div_checked(&mut stack, false)?,
            ExecutableInstruction::RemSigned => binary_div_checked(&mut stack, true)?,
            ExecutableInstruction::Eq => comparison(&mut stack, |a, b| a == b)?,
            ExecutableInstruction::Ne => comparison(&mut stack, |a, b| a != b)?,
            ExecutableInstruction::LtSigned => comparison(&mut stack, |a, b| a < b)?,
            ExecutableInstruction::LeSigned => comparison(&mut stack, |a, b| a <= b)?,
            ExecutableInstruction::GtSigned => comparison(&mut stack, |a, b| a > b)?,
            ExecutableInstruction::GeSigned => comparison(&mut stack, |a, b| a >= b)?,
            ExecutableInstruction::BitAnd => binary(&mut stack, |a, b| a & b)?,
            ExecutableInstruction::BitOr => binary(&mut stack, |a, b| a | b)?,
            ExecutableInstruction::BitXor => binary(&mut stack, |a, b| a ^ b)?,
            ExecutableInstruction::ShiftLeft => shift_checked(&mut stack, true)?,
            ExecutableInstruction::ShiftRightSigned => shift_checked(&mut stack, false)?,
            ExecutableInstruction::ExtendI32Unsigned => {
                let value = pop_value(&mut stack)?;
                stack.push((value as u32) as i64);
            }
            ExecutableInstruction::I32Eq => comparison(&mut stack, |a, b| a as i32 == b as i32)?,
            ExecutableInstruction::I32And => {
                binary(&mut stack, |a, b| i64::from((a as i32) & (b as i32)))?
            }
            ExecutableInstruction::I64Eqz => {
                let value = pop_value(&mut stack)?;
                stack.push(i64::from(value == 0));
            }
            ExecutableInstruction::Drop => {
                pop_value(&mut stack)?;
            }
            ExecutableInstruction::Call(callee) => {
                let callee_signature = module.signatures.get(callee as usize).ok_or_else(|| {
                    RuntimeError::InvalidWasmModule {
                        details: format!("unknown callee {callee}"),
                    }
                })?;
                let mut call_args = Vec::with_capacity(callee_signature.params);
                for _ in 0..callee_signature.params {
                    call_args.push(pop_value(&mut stack)?);
                }
                call_args.reverse();
                if let Some(value) = execute_function(module, callee, call_args, fuel, stdout)? {
                    stack.push(value);
                }
            }
            ExecutableInstruction::Return => break,
            ExecutableInstruction::Loop | ExecutableInstruction::End => {}
            ExecutableInstruction::If => {
                if pop_value(&mut stack)? == 0 {
                    instruction_index =
                        find_else_or_end(&function.instructions, instruction_index)?;
                }
            }
            ExecutableInstruction::Else => {
                instruction_index = find_matching_end(&function.instructions, instruction_index)?;
            }
            ExecutableInstruction::Br(depth) => {
                instruction_index =
                    branch_target(&function.instructions, instruction_index, depth)?;
                continue;
            }
            ExecutableInstruction::Unreachable => {
                return Err(RuntimeError::UnexpectedTrap {
                    message: "unreachable instruction".to_string(),
                })
            }
        }
        instruction_index += 1;
    }
    if signature.returns_value {
        Ok(Some(pop_value(&mut stack)?))
    } else {
        Ok(None)
    }
}

fn find_else_or_end(
    instructions: &[ExecutableInstruction],
    start: usize,
) -> Result<usize, RuntimeError> {
    let mut depth = 0usize;
    for (index, instruction) in instructions.iter().enumerate().skip(start + 1) {
        match instruction {
            ExecutableInstruction::If | ExecutableInstruction::Loop => depth += 1,
            ExecutableInstruction::End if depth == 0 => return Ok(index),
            ExecutableInstruction::End => depth -= 1,
            ExecutableInstruction::Else if depth == 0 => return Ok(index),
            _ => {}
        }
    }
    Err(RuntimeError::InvalidWasmModule {
        details: "unterminated if instruction".to_string(),
    })
}

fn find_matching_end(
    instructions: &[ExecutableInstruction],
    start: usize,
) -> Result<usize, RuntimeError> {
    let mut depth = 0usize;
    for (index, instruction) in instructions.iter().enumerate().skip(start + 1) {
        match instruction {
            ExecutableInstruction::If | ExecutableInstruction::Loop => depth += 1,
            ExecutableInstruction::End if depth == 0 => return Ok(index),
            ExecutableInstruction::End => depth -= 1,
            _ => {}
        }
    }
    Err(RuntimeError::InvalidWasmModule {
        details: "unterminated control instruction".to_string(),
    })
}

fn branch_target(
    instructions: &[ExecutableInstruction],
    current: usize,
    relative_depth: u32,
) -> Result<usize, RuntimeError> {
    let mut controls = Vec::new();
    for (index, instruction) in instructions.iter().enumerate().take(current) {
        match instruction {
            ExecutableInstruction::If | ExecutableInstruction::Loop => controls.push(index),
            ExecutableInstruction::End => {
                controls.pop();
            }
            _ => {}
        }
    }
    let control = controls
        .iter()
        .rev()
        .nth(relative_depth as usize)
        .copied()
        .ok_or_else(|| RuntimeError::InvalidWasmModule {
            details: format!("invalid branch depth {relative_depth}"),
        })?;
    match instructions[control] {
        ExecutableInstruction::Loop => Ok(control + 1),
        ExecutableInstruction::If => Ok(find_matching_end(instructions, control)? + 1),
        _ => unreachable!(),
    }
}

fn consume_fuel(fuel: &mut Option<u64>) -> Result<(), RuntimeError> {
    if let Some(remaining) = fuel {
        if *remaining == 0 {
            return Err(RuntimeError::FuelLimitExceeded);
        }
        *remaining -= 1;
    }
    Ok(())
}

fn pop_value(stack: &mut Vec<i64>) -> Result<i64, RuntimeError> {
    stack.pop().ok_or_else(|| RuntimeError::UnexpectedTrap {
        message: "operand stack underflow".to_string(),
    })
}

fn binary(stack: &mut Vec<i64>, op: impl FnOnce(i64, i64) -> i64) -> Result<(), RuntimeError> {
    let right = pop_value(stack)?;
    let left = pop_value(stack)?;
    stack.push(op(left, right));
    Ok(())
}

fn binary_overflow_checked(
    stack: &mut Vec<i64>,
    op: impl FnOnce(i64, i64) -> Option<i64>,
) -> Result<(), RuntimeError> {
    let right = pop_value(stack)?;
    let left = pop_value(stack)?;
    stack.push(op(left, right).ok_or_else(|| handle_runtime_trap(1, 0).unwrap_err())?);
    Ok(())
}

fn binary_div_checked(stack: &mut Vec<i64>, remainder: bool) -> Result<(), RuntimeError> {
    let right = pop_value(stack)?;
    let left = pop_value(stack)?;
    if right == 0 {
        return handle_runtime_trap(2, 0);
    }
    if !remainder && left == i64::MIN && right == -1 {
        return handle_runtime_trap(1, 0);
    }
    stack.push(if remainder {
        if left == i64::MIN && right == -1 {
            0
        } else {
            left % right
        }
    } else {
        left / right
    });
    Ok(())
}

fn shift_checked(stack: &mut Vec<i64>, left_shift: bool) -> Result<(), RuntimeError> {
    let count = pop_value(stack)?;
    let value = pop_value(stack)?;
    if !(0..64).contains(&count) {
        return handle_runtime_trap(8, 0);
    }
    stack.push(if left_shift {
        value << count
    } else {
        value >> count
    });
    Ok(())
}

fn comparison(stack: &mut Vec<i64>, op: impl FnOnce(i64, i64) -> bool) -> Result<(), RuntimeError> {
    binary(stack, |left, right| i64::from(op(left, right)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use wasm_encoder::{
        CodeSection, EntityType, ExportKind as WasmExportKind, ExportSection, Function,
        FunctionSection, ImportSection, Instruction, MemorySection, MemoryType, Module,
        TypeSection, ValType,
    };

    fn make_default_artifact() -> ValidatedWasmArtifact {
        let mut module = Module::new();
        let mut types = TypeSection::new();
        types.ty().function([ValType::I32, ValType::I32], []);
        types.ty().function([], []);
        module.section(&types);
        let mut import_section = ImportSection::new();
        import_section.import("nexa", imports::CONSOLE_WRITE_UTF8, EntityType::Function(0));
        module.section(&import_section);
        let mut functions = FunctionSection::new();
        functions.function(1);
        module.section(&functions);
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
        exports.export(MEMORY_EXPORT_NAME, WasmExportKind::Memory, 0);
        exports.export(ENTRYPOINT_EXPORT_NAME, WasmExportKind::Func, 1);
        module.section(&exports);
        let mut code = CodeSection::new();
        let mut function = Function::new([]);
        function.instruction(&Instruction::End);
        code.function(&function);
        module.section(&code);
        ValidatedWasmArtifact::new(module.finish()).expect("test module must be valid")
    }

    #[test]
    fn test_console_write_valid_utf8() {
        let memory = b"hello world".to_vec();
        let mut stdout = Vec::new();

        let result = handle_console_write_utf8(0, 5, &memory, &mut stdout);
        assert!(result.is_ok());
        assert_eq!(stdout, b"hello");
    }

    #[test]
    fn test_console_write_pointer_outside_memory() {
        let memory = vec![0u8; 10];
        let mut stdout = Vec::new();

        let result = handle_console_write_utf8(5, 10, &memory, &mut stdout);
        assert!(result.is_err());
        match result.unwrap_err() {
            RuntimeError::PointerOutsideMemory { ptr, len } => {
                assert_eq!(ptr, 5);
                assert_eq!(len, 10);
            }
            other => panic!("expected PointerOutsideMemory, got {:?}", other),
        }
    }

    #[test]
    fn test_console_write_ptr_len_overflow() {
        let memory = vec![0u8; 100];
        let mut stdout = Vec::new();

        let result = handle_console_write_utf8(u32::MAX, 1, &memory, &mut stdout);
        assert!(result.is_err());
        match result.unwrap_err() {
            RuntimeError::PtrLenOverflow { ptr, len } => {
                assert_eq!(ptr, u32::MAX);
                assert_eq!(len, 1);
            }
            other => panic!("expected PtrLenOverflow, got {:?}", other),
        }
    }

    #[test]
    fn test_console_write_invalid_utf8() {
        let memory = vec![0xFF, 0xFE, 0xFD];
        let mut stdout = Vec::new();

        let result = handle_console_write_utf8(0, 3, &memory, &mut stdout);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RuntimeError::InvalidUtf8));
    }

    #[test]
    fn test_console_write_empty() {
        let memory = b"hello".to_vec();
        let mut stdout = Vec::new();

        let result = handle_console_write_utf8(0, 0, &memory, &mut stdout);
        assert!(result.is_ok());
        assert!(stdout.is_empty());
    }

    #[test]
    fn test_runtime_trap_returns_error() {
        let result = handle_runtime_trap(0, 42);
        assert!(result.is_err());
        match result.unwrap_err() {
            RuntimeError::UnexpectedTrap { message } => {
                assert!(message.contains("42"));
                assert!(message.contains("unreachable"));
            }
            other => panic!("expected UnexpectedTrap, got {:?}", other),
        }
    }

    #[test]
    fn test_runtime_panic_extracts_message() {
        let message = b"something went wrong";
        let mut memory = vec![0u8; 64];
        memory[0..message.len()].copy_from_slice(message);

        let result = handle_runtime_panic(0, message.len() as u32, &memory);
        assert!(result.is_err());
        match result.unwrap_err() {
            RuntimeError::UnexpectedTrap { message: msg } => {
                assert_eq!(msg, "something went wrong");
            }
            other => panic!("expected UnexpectedTrap, got {:?}", other),
        }
    }

    #[test]
    fn test_execution_context_default() {
        let ctx = WasmExecutionContext::default();
        assert!(ctx.allowed_effects.contains("console::write"));
        assert_eq!(ctx.allowed_effects.len(), 1);
        assert!(ctx.max_memory_pages.is_none());
        assert!(ctx.fuel.is_none());
        assert!(ctx.stdout.is_empty());
        assert!(ctx.stderr.is_empty());
    }

    #[test]
    fn test_execution_result_fields() {
        let result = ExecutionResult {
            exit_code: 42,
            stdout: b"out".to_vec(),
            stderr: b"err".to_vec(),
        };
        assert_eq!(result.exit_code, 42);
        assert_eq!(result.stdout, b"out");
        assert_eq!(result.stderr, b"err");
    }

    #[test]
    fn test_error_codes_format() {
        let e = RuntimeError::InvalidUtf8;
        assert!(format!("{}", e).contains("UTF-8"));

        let e = RuntimeError::PointerOutsideMemory { ptr: 100, len: 50 };
        let msg = format!("{}", e);
        assert!(msg.contains("100"));
        assert!(msg.contains("50"));

        let e = RuntimeError::PtrLenOverflow {
            ptr: u32::MAX,
            len: 1,
        };
        let msg = format!("{}", e);
        assert!(msg.contains("overflows"));

        let e = RuntimeError::CapabilityDenied {
            capability: "foo::bar".to_string(),
        };
        assert!(format!("{}", e).contains("foo::bar"));

        let e = RuntimeError::UnexpectedTrap {
            message: "oops".to_string(),
        };
        assert!(format!("{}", e).contains("oops"));

        let e = RuntimeError::FuelLimitExceeded;
        assert!(format!("{}", e).contains("fuel"));

        let e = RuntimeError::MemoryLimitExceeded;
        assert!(format!("{}", e).contains("memory"));

        let e = RuntimeError::MissingEntrypoint("main".to_string());
        assert!(format!("{}", e).contains("main"));

        let e = RuntimeError::InvalidWasmModule {
            details: "bad magic".to_string(),
        };
        assert!(format!("{}", e).contains("bad magic"));
    }

    #[test]
    fn test_capability_denial() {
        let artifact = make_default_artifact();
        let mut allowed = HashSet::new();
        allowed.insert("other::capability".to_string());

        let context = WasmExecutionContext {
            allowed_effects: allowed,
            max_memory_pages: None,
            fuel: None,
            stdout: Vec::new(),
            stderr: Vec::new(),
        };

        let result = run_wasm(&artifact, &context);
        assert!(result.is_err());
        match result.unwrap_err() {
            RuntimeError::CapabilityDenied { capability } => {
                assert_eq!(capability, "console::write");
            }
            other => panic!("expected CapabilityDenied, got {:?}", other),
        }
    }

    #[test]
    fn test_empty_stdout_stderr() {
        let ctx = WasmExecutionContext::default();
        assert!(ctx.stdout.is_empty());
        assert!(ctx.stderr.is_empty());
        let result = ExecutionResult {
            exit_code: 0,
            stdout: ctx.stdout,
            stderr: ctx.stderr,
        };
        assert!(result.stdout.is_empty());
        assert!(result.stderr.is_empty());
    }

    #[test]
    fn test_multiple_writes_concat() {
        let mut stdout = Vec::new();

        handle_console_write_utf8(0, 5, b"hello", &mut stdout).unwrap();
        handle_console_write_utf8(0, 1, b" ", &mut stdout).unwrap();
        handle_console_write_utf8(0, 5, b"world", &mut stdout).unwrap();

        assert_eq!(stdout, b"hello world");
    }

    #[test]
    fn test_error_display_variants() {
        let variants: Vec<RuntimeError> = vec![
            RuntimeError::InvalidUtf8,
            RuntimeError::PointerOutsideMemory { ptr: 0, len: 0 },
            RuntimeError::PtrLenOverflow { ptr: 0, len: 0 },
            RuntimeError::CapabilityDenied {
                capability: "x".to_string(),
            },
            RuntimeError::UnexpectedTrap {
                message: "x".to_string(),
            },
            RuntimeError::FuelLimitExceeded,
            RuntimeError::MemoryLimitExceeded,
            RuntimeError::MissingEntrypoint("x".to_string()),
            RuntimeError::InvalidWasmModule {
                details: "x".to_string(),
            },
        ];

        for variant in &variants {
            let msg = format!("{}", variant);
            assert!(
                !msg.is_empty(),
                "error variant produced empty display: {:?}",
                variant
            );
        }
    }

    #[test]
    fn test_valid_wasm_artifact_rejects_non_wasm() {
        let result = ValidatedWasmArtifact::new(b"not wasm".to_vec());
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            RuntimeError::InvalidWasmModule { .. }
        ));
    }

    #[test]
    fn test_valid_wasm_artifact_rejects_bad_version() {
        let mut bytes = vec![0, b'a', b's', b'm'];
        bytes.extend_from_slice(&2u32.to_le_bytes());
        let result = ValidatedWasmArtifact::new(bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_run_wasm_with_no_entrypoint() {
        let mut bytes = vec![0u8; 16];
        bytes[0..4].copy_from_slice(b"\0asm");
        bytes[4..8].copy_from_slice(&1u32.to_le_bytes());
        let artifact = ValidatedWasmArtifact {
            bytes,
            imports: vec![],
            exports: vec![],
            has_memory: true,
            has_entrypoint: false,
        };

        let context = WasmExecutionContext::default();
        let result = run_wasm(&artifact, &context);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            RuntimeError::MissingEntrypoint(_)
        ));
    }

    #[test]
    fn test_run_wasm_valid_module() {
        let artifact = make_default_artifact();
        let mut allowed = HashSet::new();
        allowed.insert("console::write".to_string());

        let context = WasmExecutionContext {
            allowed_effects: allowed,
            max_memory_pages: None,
            fuel: None,
            stdout: Vec::new(),
            stderr: Vec::new(),
        };

        let result = run_wasm(&artifact, &context);
        assert!(result.is_ok());
        let exec = result.unwrap();
        assert_eq!(exec.exit_code, 0);
    }

    #[test]
    fn test_run_wasm_enforces_fuel() {
        let artifact = make_default_artifact();
        let context = WasmExecutionContext {
            allowed_effects: ["console::write".to_string()].into_iter().collect(),
            fuel: Some(0),
            ..WasmExecutionContext::default()
        };

        let result = run_wasm(&artifact, &context);
        assert!(matches!(result, Err(RuntimeError::FuelLimitExceeded)));
    }

    #[test]
    fn test_run_wasm_enforces_memory_limit() {
        let artifact = make_default_artifact();
        let context = WasmExecutionContext {
            allowed_effects: ["console::write".to_string()].into_iter().collect(),
            max_memory_pages: Some(0),
            ..WasmExecutionContext::default()
        };

        let result = run_wasm(&artifact, &context);
        assert!(matches!(result, Err(RuntimeError::MemoryLimitExceeded)));
    }

    #[test]
    fn test_trap_kinds() {
        for kind in 0..8 {
            let result = handle_runtime_trap(kind, 0);
            assert!(result.is_err());
        }
    }

    #[test]
    fn test_runtime_panic_ptr_overflow() {
        let memory = vec![0u8; 10];
        let result = handle_runtime_panic(u32::MAX, 1, &memory);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            RuntimeError::PtrLenOverflow { .. }
        ));
    }

    #[test]
    fn test_runtime_panic_out_of_bounds() {
        let memory = vec![0u8; 10];
        let result = handle_runtime_panic(5, 10, &memory);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            RuntimeError::PointerOutsideMemory { .. }
        ));
    }
}
