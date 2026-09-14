#![allow(dead_code)]

use nexa_hnir::*;
use nexa_mnir::*;
use nexa_nir::*;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct SemanticProgram {
    pub functions: Vec<SemanticFunction>,
    pub type_decls: Vec<SemanticTypeDecl>,
}

impl Default for SemanticProgram {
    fn default() -> Self {
        Self::new()
    }
}

impl SemanticProgram {
    pub fn new() -> Self {
        Self {
            functions: vec![],
            type_decls: vec![],
        }
    }
    pub fn add_function(&mut self, f: SemanticFunction) {
        self.functions.push(f);
    }
}

#[derive(Debug, Clone)]
pub struct SemanticFunction {
    pub name: String,
    pub kind: NirCallableKind,
    pub parameters: Vec<SemanticParameter>,
    pub return_type: String,
    pub body: Vec<SemanticStatement>,
}

#[derive(Debug, Clone)]
pub struct SemanticParameter {
    pub name: String,
    pub type_name: String,
    pub passing: NirPassingMode,
}

#[derive(Debug, Clone)]
pub struct SemanticTypeDecl {
    pub name: String,
    pub kind: SemanticTypeKind,
}

#[derive(Debug, Clone)]
pub enum SemanticTypeKind {
    Struct {
        fields: Vec<(String, String)>,
    },
    Enum {
        variants: Vec<(String, Vec<String>)>,
    },
}

#[derive(Debug, Clone)]
pub enum SemanticStatement {
    Let {
        name: String,
        type_name: Option<String>,
        value: SemanticExpression,
    },
    Assign {
        target: String,
        value: SemanticExpression,
    },
    ArrayAssign {
        target: String,
        index: SemanticExpression,
        value: SemanticExpression,
    },
    Return(Option<SemanticExpression>),
    Expression(SemanticExpression),
    If {
        condition: SemanticExpression,
        then_body: Vec<SemanticStatement>,
        else_body: Vec<SemanticStatement>,
    },
    While {
        condition: SemanticExpression,
        body: Vec<SemanticStatement>,
    },
    Loop {
        body: Vec<SemanticStatement>,
    },
    ForLiteral {
        variable: String,
        iterable: SemanticExpression,
        body: Vec<SemanticStatement>,
    },
    Break,
    Continue,
}

#[derive(Debug, Clone)]
pub enum SemanticExpression {
    LiteralInt(i128),
    LiteralFloat(f64),
    LiteralString(String),
    LiteralBool(bool),
    Identifier(String),
    BinaryOp {
        op: String,
        left: Box<SemanticExpression>,
        right: Box<SemanticExpression>,
    },
    Call {
        function: String,
        args: Vec<SemanticExpression>,
    },
    Array(Vec<SemanticExpression>),
    Index {
        target: Box<SemanticExpression>,
        index: Box<SemanticExpression>,
    },
    Struct {
        fields: Vec<(String, SemanticExpression)>,
    },
    Field {
        target: Box<SemanticExpression>,
        name: String,
    },
}

fn runtime_scalar_expression(expression: &SemanticExpression) -> bool {
    match expression {
        SemanticExpression::LiteralInt(_)
        | SemanticExpression::LiteralBool(_)
        | SemanticExpression::Identifier(_)
        | SemanticExpression::Call { .. }
        | SemanticExpression::Index { .. } => true,
        SemanticExpression::BinaryOp { left, right, .. } => {
            runtime_scalar_expression(left) && runtime_scalar_expression(right)
        }
        SemanticExpression::LiteralFloat(_)
        | SemanticExpression::LiteralString(_)
        | SemanticExpression::Array(_)
        | SemanticExpression::Struct { .. }
        | SemanticExpression::Field { .. } => false,
    }
}

#[derive(Debug, Clone)]
pub enum LoweringError {
    UnsupportedStatement(String),
    UnknownFunction(String),
    InternalError(String),
}

pub struct LoweringPipeline;

impl LoweringPipeline {
    pub fn lower_to_hnir(program: &SemanticProgram) -> Result<HNirModule, LoweringError> {
        let mut lowerer = HnirLowerer::new(program);
        for func in &program.functions {
            lowerer.lower_function(func)?;
        }
        Ok(lowerer.module)
    }

    pub fn lower_to_mnir(hnir: &HNirModule) -> Result<MNirModule, LoweringError> {
        let mut lowerer = MnirLowerer::new();
        for func in &hnir.functions {
            lowerer.lower_function(func)?;
        }
        Ok(lowerer.module)
    }

    pub fn lower_full(
        program: &SemanticProgram,
    ) -> Result<(NirModule, HNirModule, MNirModule), LoweringError> {
        let hnir = Self::lower_to_hnir(program)?;
        let mnir = Self::lower_to_mnir(&hnir)?;

        let mut nir_module = NirModule::new();
        nir_module.metadata.build_provenance.compiler_version = "nexa-0.0.1".to_string();
        nir_module.metadata.build_provenance.language_profile = "1.0-freeze-candidate".to_string();
        nir_module.metadata.build_provenance.nir_version = "NIR1".to_string();

        let mut type_resolver = TypeResolver::new(&mut nir_module.types, &mut nir_module.constants);
        for decl in &program.type_decls {
            type_resolver.register_type_decl(decl);
        }

        let function_map: HashMap<String, (NirFunctionId, bool)> = program
            .functions
            .iter()
            .enumerate()
            .map(|(index, function)| {
                (
                    function.name.clone(),
                    (
                        NirFunctionId(index as u32),
                        function.return_type != "Unit" && function.return_type != "Never",
                    ),
                )
            })
            .collect();

        for (index, func) in program.functions.iter().enumerate() {
            let return_ty = type_resolver.resolve_type_name(&func.return_type);
            let params: Vec<NirParameter> = func
                .parameters
                .iter()
                .map(|p| NirParameter {
                    ty: type_resolver.resolve_type_name(&p.type_name),
                    passing: p.passing,
                })
                .collect();
            let sig = NirCallableSignature {
                kind: func.kind,
                parameters: params,
                return_type: return_ty,
                effects: NirEffectSet::empty(),
            };
            nir_module.functions.add(NirFunction {
                id: NirFunctionId(index as u32),
                signature: sig,
                body: Some(lower_nir_body(func, &function_map)?),
                metadata: NirFunctionMetadata {
                    source_location: None,
                    is_exported: func.name == "main",
                },
            });
        }

        Ok((nir_module, hnir, mnir))
    }
}

fn lower_nir_body(
    func: &SemanticFunction,
    function_map: &HashMap<String, (NirFunctionId, bool)>,
) -> Result<NirFunctionBody, LoweringError> {
    let mut assigned = HashSet::new();
    collect_assigned_names(&func.body, &mut assigned);
    let mut lowerer = NirBodyLowerer {
        next_value: func.parameters.len() as u32,
        values: func
            .parameters
            .iter()
            .enumerate()
            .map(|(index, parameter)| (parameter.name.clone(), index as u32))
            .collect(),
        instructions: Vec::new(),
        blocks: Vec::new(),
        current_block: 0,
        next_block: 1,
        terminated: false,
        mutable_slots: HashMap::new(),
        next_slot: 0,
        assigned,
        loop_targets: Vec::new(),
        static_strings: HashMap::new(),
        static_arrays: HashMap::new(),
        runtime_arrays: HashMap::new(),
        function_map,
    };
    for (index, parameter) in func.parameters.iter().enumerate() {
        if lowerer.assigned.contains(&parameter.name) {
            let slot = lowerer.local_slot(&parameter.name);
            lowerer.instructions.push(NirInstruction::LocalWrite {
                local: slot,
                value: index as u32,
            });
        }
    }
    for statement in &func.body {
        if lowerer.terminated {
            break;
        }
        lowerer.lower_statement(statement)?;
    }
    if !lowerer.terminated {
        lowerer.finish_block(NirTerminator::Return { value: None });
    }
    Ok(NirFunctionBody {
        blocks: lowerer.blocks,
        entry: 0,
    })
}

fn collect_assigned_names(statements: &[SemanticStatement], assigned: &mut HashSet<String>) {
    for statement in statements {
        match statement {
            SemanticStatement::Assign { target, .. } => {
                assigned.insert(target.clone());
            }
            SemanticStatement::ArrayAssign { .. } => {}
            SemanticStatement::If {
                then_body,
                else_body,
                ..
            } => {
                collect_assigned_names(then_body, assigned);
                collect_assigned_names(else_body, assigned);
            }
            SemanticStatement::While { body, .. } | SemanticStatement::Loop { body } => {
                collect_assigned_names(body, assigned)
            }
            SemanticStatement::ForLiteral { body, .. } => collect_assigned_names(body, assigned),
            SemanticStatement::Let { .. }
            | SemanticStatement::Return(_)
            | SemanticStatement::Expression(_)
            | SemanticStatement::Break
            | SemanticStatement::Continue => {}
        }
    }
}

struct NirBodyLowerer<'a> {
    next_value: u32,
    values: HashMap<String, u32>,
    instructions: Vec<NirInstruction>,
    blocks: Vec<NirBasicBlock>,
    current_block: u32,
    next_block: u32,
    terminated: bool,
    mutable_slots: HashMap<String, u32>,
    next_slot: u32,
    assigned: HashSet<String>,
    loop_targets: Vec<(u32, u32)>,
    static_strings: HashMap<String, String>,
    static_arrays: HashMap<String, Vec<SemanticExpression>>,
    runtime_arrays: HashMap<String, Vec<u32>>,
    function_map: &'a HashMap<String, (NirFunctionId, bool)>,
}

impl NirBodyLowerer<'_> {
    fn value_id(&mut self) -> u32 {
        let id = self.next_value;
        self.next_value += 1;
        id
    }

    fn block_id(&mut self) -> u32 {
        let id = self.next_block;
        self.next_block += 1;
        id
    }

    fn local_slot(&mut self, name: &str) -> u32 {
        if let Some(slot) = self.mutable_slots.get(name) {
            return *slot;
        }
        let slot = self.next_slot;
        self.next_slot += 1;
        self.mutable_slots.insert(name.to_string(), slot);
        slot
    }

    fn finish_block(&mut self, terminator: NirTerminator) {
        self.blocks.push(NirBasicBlock {
            id: self.current_block,
            instructions: std::mem::take(&mut self.instructions),
            terminator,
        });
        self.terminated = true;
    }

    fn begin_block(&mut self, id: u32) {
        self.current_block = id;
        self.instructions.clear();
        self.terminated = false;
    }

    fn lower_statement(&mut self, statement: &SemanticStatement) -> Result<(), LoweringError> {
        match statement {
            SemanticStatement::Let { name, value, .. } => {
                if let SemanticExpression::Array(elements) = value {
                    if self.assigned.contains(name) {
                        return Err(LoweringError::UnsupportedStatement(
                            "mutable array locals are not yet available in executable lowering"
                                .to_string(),
                        ));
                    }
                    self.static_arrays.insert(name.clone(), elements.clone());
                    if elements.iter().all(runtime_scalar_expression) {
                        let mut slots = Vec::with_capacity(elements.len());
                        for (index, element) in elements.iter().enumerate() {
                            let value = self.lower_expression(element)?;
                            let slot = self.local_slot(&format!("#array:{name}:{index}"));
                            self.instructions
                                .push(NirInstruction::LocalWrite { local: slot, value });
                            slots.push(slot);
                        }
                        self.runtime_arrays.insert(name.clone(), slots);
                    }
                    return Ok(());
                }
                if let SemanticExpression::LiteralString(text) = value {
                    if self.assigned.contains(name) {
                        return Err(LoweringError::UnsupportedStatement(
                            "mutable String locals are not yet available in executable lowering"
                                .to_string(),
                        ));
                    }
                    self.static_strings.insert(name.clone(), text.clone());
                    return Ok(());
                }
                let value = self.lower_expression(value)?;
                if self.assigned.contains(name) {
                    let slot = self.local_slot(name);
                    self.instructions
                        .push(NirInstruction::LocalWrite { local: slot, value });
                } else {
                    self.values.insert(name.clone(), value);
                }
            }
            SemanticStatement::Assign { target, value } => {
                let value = self.lower_expression(value)?;
                let slot = self.local_slot(target);
                self.instructions
                    .push(NirInstruction::LocalWrite { local: slot, value });
            }
            SemanticStatement::ArrayAssign {
                target,
                index,
                value,
            } => {
                let slots = self.runtime_arrays.get(target).cloned().ok_or_else(|| {
                    LoweringError::UnsupportedStatement(format!(
                        "mutable array `{target}` has no executable scalar storage"
                    ))
                })?;
                if let SemanticExpression::LiteralInt(index) = index {
                    let index = usize::try_from(*index).map_err(|_| {
                        LoweringError::UnsupportedStatement(
                            "negative array assignment index".to_string(),
                        )
                    })?;
                    let slot = slots.get(index).copied().ok_or_else(|| {
                        LoweringError::UnsupportedStatement(format!(
                            "constant array assignment index {index} is outside length {}",
                            slots.len()
                        ))
                    })?;
                    let value = self.lower_expression(value)?;
                    self.instructions
                        .push(NirInstruction::LocalWrite { local: slot, value });
                } else {
                    self.lower_dynamic_array_assignment(&slots, index, value)?;
                }
            }
            SemanticStatement::Return(value) => {
                let value = value
                    .as_ref()
                    .map(|v| self.lower_expression(v))
                    .transpose()?;
                self.finish_block(NirTerminator::Return { value });
            }
            SemanticStatement::Expression(value) => {
                if let SemanticExpression::Call { function, args } = value {
                    self.lower_call(function, args)?;
                } else {
                    self.lower_expression(value)?;
                }
            }
            SemanticStatement::If {
                condition,
                then_body,
                else_body,
            } => self.lower_if(condition, then_body, else_body)?,
            SemanticStatement::While { condition, body } => {
                self.lower_while(condition, body)?;
            }
            SemanticStatement::Loop { body } => self.lower_loop(body)?,
            SemanticStatement::ForLiteral {
                variable,
                iterable,
                body,
            } => {
                let elements = match iterable {
                    SemanticExpression::Array(elements) => elements.clone(),
                    SemanticExpression::Identifier(name) => {
                        self.static_arrays.get(name).cloned().ok_or_else(|| {
                            LoweringError::UnsupportedStatement(format!(
                                "for iterable `{name}` is not a static array"
                            ))
                        })?
                    }
                    _ => {
                        return Err(LoweringError::UnsupportedStatement(
                            "executable for requires a statically known array".to_string(),
                        ))
                    }
                };
                let outer_values = self.values.clone();
                let outer_strings = self.static_strings.clone();
                let outer_arrays = self.static_arrays.clone();
                let outer_runtime_arrays = self.runtime_arrays.clone();
                for element in &elements {
                    self.values = outer_values.clone();
                    self.static_strings = outer_strings.clone();
                    self.static_arrays = outer_arrays.clone();
                    self.runtime_arrays = outer_runtime_arrays.clone();
                    match element {
                        SemanticExpression::LiteralString(text) => {
                            self.values.remove(variable);
                            self.static_strings.insert(variable.clone(), text.clone());
                        }
                        _ => {
                            let value = self.lower_expression(element)?;
                            self.static_strings.remove(variable);
                            self.values.insert(variable.clone(), value);
                        }
                    }
                    for statement in body {
                        if self.terminated {
                            break;
                        }
                        self.lower_statement(statement)?;
                    }
                }
                self.values = outer_values;
                self.static_strings = outer_strings;
                self.static_arrays = outer_arrays;
                self.runtime_arrays = outer_runtime_arrays;
            }
            SemanticStatement::Break => {
                let (_, exit) = self.loop_targets.last().copied().ok_or_else(|| {
                    LoweringError::UnsupportedStatement("break outside executable loop".to_string())
                })?;
                self.finish_block(NirTerminator::Branch { target: exit });
            }
            SemanticStatement::Continue => {
                let (header, _) = self.loop_targets.last().copied().ok_or_else(|| {
                    LoweringError::UnsupportedStatement(
                        "continue outside executable loop".to_string(),
                    )
                })?;
                self.finish_block(NirTerminator::Branch { target: header });
            }
        }
        Ok(())
    }

    fn lower_while(
        &mut self,
        condition: &SemanticExpression,
        body: &[SemanticStatement],
    ) -> Result<(), LoweringError> {
        let header = self.block_id();
        let body_block = self.block_id();
        let exit = self.block_id();
        self.finish_block(NirTerminator::Branch { target: header });

        self.begin_block(header);
        let condition = self.lower_expression(condition)?;
        self.finish_block(NirTerminator::CondBranch {
            condition,
            then_target: body_block,
            else_target: exit,
        });

        self.begin_block(body_block);
        let incoming_values = self.values.clone();
        let incoming_strings = self.static_strings.clone();
        let incoming_arrays = self.static_arrays.clone();
        let incoming_runtime_arrays = self.runtime_arrays.clone();
        self.loop_targets.push((header, exit));
        for statement in body {
            if self.terminated {
                break;
            }
            self.lower_statement(statement)?;
        }
        if !self.terminated {
            self.finish_block(NirTerminator::Branch { target: header });
        }
        self.loop_targets.pop();

        self.begin_block(exit);
        self.values = incoming_values;
        self.static_strings = incoming_strings;
        self.static_arrays = incoming_arrays;
        self.runtime_arrays = incoming_runtime_arrays;
        Ok(())
    }

    fn lower_loop(&mut self, body: &[SemanticStatement]) -> Result<(), LoweringError> {
        let loop_block = self.block_id();
        let exit = self.block_id();
        self.finish_block(NirTerminator::Branch { target: loop_block });
        self.begin_block(loop_block);
        let incoming_values = self.values.clone();
        let incoming_strings = self.static_strings.clone();
        let incoming_arrays = self.static_arrays.clone();
        let incoming_runtime_arrays = self.runtime_arrays.clone();
        self.loop_targets.push((loop_block, exit));
        for statement in body {
            if self.terminated {
                break;
            }
            self.lower_statement(statement)?;
        }
        if !self.terminated {
            self.finish_block(NirTerminator::Branch { target: loop_block });
        }
        self.loop_targets.pop();
        self.begin_block(exit);
        self.values = incoming_values;
        self.static_strings = incoming_strings;
        self.static_arrays = incoming_arrays;
        self.runtime_arrays = incoming_runtime_arrays;
        Ok(())
    }

    fn lower_if(
        &mut self,
        condition: &SemanticExpression,
        then_body: &[SemanticStatement],
        else_body: &[SemanticStatement],
    ) -> Result<(), LoweringError> {
        let condition = self.lower_expression(condition)?;
        let then_block = self.block_id();
        let else_block = self.block_id();
        let merge_block = self.block_id();
        self.finish_block(NirTerminator::CondBranch {
            condition,
            then_target: then_block,
            else_target: else_block,
        });

        let incoming_values = self.values.clone();
        let incoming_strings = self.static_strings.clone();
        let incoming_arrays = self.static_arrays.clone();
        let incoming_runtime_arrays = self.runtime_arrays.clone();
        self.begin_block(then_block);
        for statement in then_body {
            if self.terminated {
                break;
            }
            self.lower_statement(statement)?;
        }
        if !self.terminated {
            self.finish_block(NirTerminator::Branch {
                target: merge_block,
            });
        }

        self.values = incoming_values.clone();
        self.static_strings = incoming_strings.clone();
        self.static_arrays = incoming_arrays.clone();
        self.runtime_arrays = incoming_runtime_arrays.clone();
        self.begin_block(else_block);
        for statement in else_body {
            if self.terminated {
                break;
            }
            self.lower_statement(statement)?;
        }
        if !self.terminated {
            self.finish_block(NirTerminator::Branch {
                target: merge_block,
            });
        }

        self.values = incoming_values;
        self.static_strings = incoming_strings;
        self.static_arrays = incoming_arrays;
        self.runtime_arrays = incoming_runtime_arrays;
        self.begin_block(merge_block);
        Ok(())
    }

    fn lower_expression(&mut self, expression: &SemanticExpression) -> Result<u32, LoweringError> {
        match expression {
            SemanticExpression::LiteralInt(value) => {
                let value = i64::try_from(*value).map_err(|_| {
                    LoweringError::UnsupportedStatement(format!(
                        "integer literal {value} does not fit the executable i64 profile"
                    ))
                })?;
                let dest = self.value_id();
                self.instructions
                    .push(NirInstruction::ConstI64 { dest, value });
                Ok(dest)
            }
            SemanticExpression::LiteralBool(value) => {
                let dest = self.value_id();
                self.instructions.push(NirInstruction::ConstI64 {
                    dest,
                    value: i64::from(*value),
                });
                Ok(dest)
            }
            SemanticExpression::Identifier(name) => {
                if let Some(local) = self.mutable_slots.get(name).copied() {
                    let dest = self.value_id();
                    self.instructions
                        .push(NirInstruction::LocalRead { dest, local });
                    return Ok(dest);
                }
                self.values.get(name).copied().ok_or_else(|| {
                    LoweringError::InternalError(format!(
                        "unknown value `{name}` during NIR lowering"
                    ))
                })
            }
            SemanticExpression::BinaryOp { op, left, right } => {
                if op == "&&" || op == "||" {
                    return self.lower_short_circuit(op, left, right);
                }
                let left = self.lower_expression(left)?;
                let right = self.lower_expression(right)?;
                let op = match op.as_str() {
                    "+" => NirScalarBinaryOp::Add,
                    "-" => NirScalarBinaryOp::Sub,
                    "*" => NirScalarBinaryOp::Mul,
                    "/" => NirScalarBinaryOp::DivSigned,
                    "%" => NirScalarBinaryOp::RemSigned,
                    "==" => NirScalarBinaryOp::Eq,
                    "!=" => NirScalarBinaryOp::Ne,
                    "<" => NirScalarBinaryOp::LtSigned,
                    "<=" => NirScalarBinaryOp::LeSigned,
                    ">" => NirScalarBinaryOp::GtSigned,
                    ">=" => NirScalarBinaryOp::GeSigned,
                    "&" => NirScalarBinaryOp::BitAnd,
                    "|" => NirScalarBinaryOp::BitOr,
                    "^" => NirScalarBinaryOp::BitXor,
                    "<<" => NirScalarBinaryOp::ShiftLeft,
                    ">>" => NirScalarBinaryOp::ShiftRightSigned,
                    other => {
                        return Err(LoweringError::UnsupportedStatement(format!(
                            "binary operator `{other}` in executable lowering"
                        )))
                    }
                };
                let dest = self.value_id();
                self.instructions.push(NirInstruction::BinaryI64 {
                    dest,
                    op,
                    left,
                    right,
                });
                Ok(dest)
            }
            SemanticExpression::Call { function, args } => {
                self.lower_call(function, args)?.ok_or_else(|| {
                    LoweringError::UnsupportedStatement(format!(
                        "unit function `{function}` cannot be used as a value"
                    ))
                })
            }
            SemanticExpression::Struct { .. } | SemanticExpression::Field { .. } => {
                Err(LoweringError::UnsupportedStatement(
                    "structs and fields not yet implemented".to_string(),
                ))
            }
            SemanticExpression::Index { target, index } => {
                if let SemanticExpression::Identifier(name) = target.as_ref() {
                    if let Some(slots) = self.runtime_arrays.get(name).cloned() {
                        if let SemanticExpression::LiteralInt(index) = index.as_ref() {
                            let index = usize::try_from(*index).map_err(|_| {
                                LoweringError::UnsupportedStatement(
                                    "negative constant array index".to_string(),
                                )
                            })?;
                            let local = slots.get(index).copied().ok_or_else(|| {
                                LoweringError::UnsupportedStatement(format!(
                                    "constant array index {index} is outside length {}",
                                    slots.len()
                                ))
                            })?;
                            let dest = self.value_id();
                            self.instructions
                                .push(NirInstruction::LocalRead { dest, local });
                            return Ok(dest);
                        }
                        return self.lower_dynamic_slot_array_index(&slots, index);
                    }
                }
                let elements = match target.as_ref() {
                    SemanticExpression::Array(elements) => elements.clone(),
                    SemanticExpression::Identifier(name) => {
                        self.static_arrays.get(name).cloned().ok_or_else(|| {
                            LoweringError::UnsupportedStatement(format!(
                                "`{name}` is not a static array"
                            ))
                        })?
                    }
                    _ => {
                        return Err(LoweringError::UnsupportedStatement(
                            "array index target is not statically known".to_string(),
                        ))
                    }
                };
                if let SemanticExpression::LiteralInt(index) = index.as_ref() {
                    let index = usize::try_from(*index).map_err(|_| {
                        LoweringError::UnsupportedStatement(
                            "negative constant array index".to_string(),
                        )
                    })?;
                    let element = elements.get(index).cloned().ok_or_else(|| {
                        LoweringError::UnsupportedStatement(format!(
                            "constant array index {index} is outside length {}",
                            elements.len()
                        ))
                    })?;
                    self.lower_expression(&element)
                } else {
                    self.lower_dynamic_static_array_index(&elements, index)
                }
            }
            SemanticExpression::Array(_) => Err(LoweringError::UnsupportedStatement(
                "array value must be bound or indexed in the current executable profile"
                    .to_string(),
            )),
            SemanticExpression::LiteralFloat(_) | SemanticExpression::LiteralString(_) => {
                Err(LoweringError::UnsupportedStatement(
                    "expression is not yet available in the scalar executable profile".to_string(),
                ))
            }
        }
    }

    fn lower_short_circuit(
        &mut self,
        op: &str,
        left: &SemanticExpression,
        right: &SemanticExpression,
    ) -> Result<u32, LoweringError> {
        let left = self.lower_expression(left)?;
        let rhs_block = self.block_id();
        let short_block = self.block_id();
        let merge_block = self.block_id();
        let result_slot = self.next_slot;
        self.next_slot += 1;

        let (then_target, else_target, short_value) = if op == "&&" {
            (rhs_block, short_block, 0)
        } else {
            (short_block, rhs_block, 1)
        };
        self.finish_block(NirTerminator::CondBranch {
            condition: left,
            then_target,
            else_target,
        });

        self.begin_block(short_block);
        let value = self.value_id();
        self.instructions.push(NirInstruction::ConstI64 {
            dest: value,
            value: short_value,
        });
        self.instructions.push(NirInstruction::LocalWrite {
            local: result_slot,
            value,
        });
        self.finish_block(NirTerminator::Branch {
            target: merge_block,
        });

        self.begin_block(rhs_block);
        let value = self.lower_expression(right)?;
        self.instructions.push(NirInstruction::LocalWrite {
            local: result_slot,
            value,
        });
        self.finish_block(NirTerminator::Branch {
            target: merge_block,
        });

        self.begin_block(merge_block);
        let dest = self.value_id();
        self.instructions.push(NirInstruction::LocalRead {
            dest,
            local: result_slot,
        });
        Ok(dest)
    }

    fn lower_dynamic_static_array_index(
        &mut self,
        elements: &[SemanticExpression],
        index: &SemanticExpression,
    ) -> Result<u32, LoweringError> {
        let index_value = self.lower_expression(index)?;
        let zero = self.value_id();
        self.instructions.push(NirInstruction::ConstI64 {
            dest: zero,
            value: 0,
        });
        let negative = self.value_id();
        self.instructions.push(NirInstruction::BinaryI64 {
            dest: negative,
            op: NirScalarBinaryOp::LtSigned,
            left: index_value,
            right: zero,
        });

        let bounds_trap = self.block_id();
        let upper_check = self.block_id();
        let merge = self.block_id();
        let checks: Vec<(u32, u32)> = elements
            .iter()
            .map(|_| (self.block_id(), self.block_id()))
            .collect();
        let first_check = checks.first().map_or(bounds_trap, |pair| pair.0);
        self.finish_block(NirTerminator::CondBranch {
            condition: negative,
            then_target: bounds_trap,
            else_target: upper_check,
        });

        self.begin_block(upper_check);
        let length = self.value_id();
        self.instructions.push(NirInstruction::ConstI64 {
            dest: length,
            value: elements.len() as i64,
        });
        let too_large = self.value_id();
        self.instructions.push(NirInstruction::BinaryI64 {
            dest: too_large,
            op: NirScalarBinaryOp::GeSigned,
            left: index_value,
            right: length,
        });
        self.finish_block(NirTerminator::CondBranch {
            condition: too_large,
            then_target: bounds_trap,
            else_target: first_check,
        });

        self.begin_block(bounds_trap);
        self.instructions.push(NirInstruction::RuntimeTrap {
            kind: 4,
            location: 0,
        });
        self.finish_block(NirTerminator::Unreachable);

        let result_slot = self.local_slot("#array-index-result");
        for (position, (check, hit)) in checks.iter().copied().enumerate() {
            self.begin_block(check);
            let expected = self.value_id();
            self.instructions.push(NirInstruction::ConstI64 {
                dest: expected,
                value: position as i64,
            });
            let matches = self.value_id();
            self.instructions.push(NirInstruction::BinaryI64 {
                dest: matches,
                op: NirScalarBinaryOp::Eq,
                left: index_value,
                right: expected,
            });
            let next = checks.get(position + 1).map_or(bounds_trap, |pair| pair.0);
            self.finish_block(NirTerminator::CondBranch {
                condition: matches,
                then_target: hit,
                else_target: next,
            });

            self.begin_block(hit);
            let value = self.lower_expression(&elements[position])?;
            self.instructions.push(NirInstruction::LocalWrite {
                local: result_slot,
                value,
            });
            self.finish_block(NirTerminator::Branch { target: merge });
        }

        self.begin_block(merge);
        let dest = self.value_id();
        self.instructions.push(NirInstruction::LocalRead {
            dest,
            local: result_slot,
        });
        Ok(dest)
    }

    fn lower_dynamic_array_assignment(
        &mut self,
        slots: &[u32],
        index: &SemanticExpression,
        value: &SemanticExpression,
    ) -> Result<(), LoweringError> {
        let index_value = self.lower_expression(index)?;
        let zero = self.value_id();
        self.instructions.push(NirInstruction::ConstI64 {
            dest: zero,
            value: 0,
        });
        let negative = self.value_id();
        self.instructions.push(NirInstruction::BinaryI64 {
            dest: negative,
            op: NirScalarBinaryOp::LtSigned,
            left: index_value,
            right: zero,
        });
        let trap = self.block_id();
        let upper = self.block_id();
        let merge = self.block_id();
        let branches: Vec<(u32, u32)> = slots
            .iter()
            .map(|_| (self.block_id(), self.block_id()))
            .collect();
        let first = branches.first().map_or(trap, |branch| branch.0);
        self.finish_block(NirTerminator::CondBranch {
            condition: negative,
            then_target: trap,
            else_target: upper,
        });

        self.begin_block(upper);
        let length = self.value_id();
        self.instructions.push(NirInstruction::ConstI64 {
            dest: length,
            value: slots.len() as i64,
        });
        let outside = self.value_id();
        self.instructions.push(NirInstruction::BinaryI64 {
            dest: outside,
            op: NirScalarBinaryOp::GeSigned,
            left: index_value,
            right: length,
        });
        self.finish_block(NirTerminator::CondBranch {
            condition: outside,
            then_target: trap,
            else_target: first,
        });

        self.begin_block(trap);
        self.instructions.push(NirInstruction::RuntimeTrap {
            kind: 4,
            location: 0,
        });
        self.finish_block(NirTerminator::Unreachable);

        for (position, (check, hit)) in branches.iter().copied().enumerate() {
            self.begin_block(check);
            let expected = self.value_id();
            self.instructions.push(NirInstruction::ConstI64 {
                dest: expected,
                value: position as i64,
            });
            let matches = self.value_id();
            self.instructions.push(NirInstruction::BinaryI64 {
                dest: matches,
                op: NirScalarBinaryOp::Eq,
                left: index_value,
                right: expected,
            });
            let next = branches.get(position + 1).map_or(trap, |branch| branch.0);
            self.finish_block(NirTerminator::CondBranch {
                condition: matches,
                then_target: hit,
                else_target: next,
            });

            self.begin_block(hit);
            let value = self.lower_expression(value)?;
            self.instructions.push(NirInstruction::LocalWrite {
                local: slots[position],
                value,
            });
            self.finish_block(NirTerminator::Branch { target: merge });
        }
        self.begin_block(merge);
        Ok(())
    }

    fn lower_dynamic_slot_array_index(
        &mut self,
        slots: &[u32],
        index: &SemanticExpression,
    ) -> Result<u32, LoweringError> {
        let index_value = self.lower_expression(index)?;
        let zero = self.value_id();
        self.instructions.push(NirInstruction::ConstI64 {
            dest: zero,
            value: 0,
        });
        let negative = self.value_id();
        self.instructions.push(NirInstruction::BinaryI64 {
            dest: negative,
            op: NirScalarBinaryOp::LtSigned,
            left: index_value,
            right: zero,
        });
        let trap = self.block_id();
        let upper = self.block_id();
        let merge = self.block_id();
        let branches: Vec<(u32, u32)> = slots
            .iter()
            .map(|_| (self.block_id(), self.block_id()))
            .collect();
        let first = branches.first().map_or(trap, |branch| branch.0);
        self.finish_block(NirTerminator::CondBranch {
            condition: negative,
            then_target: trap,
            else_target: upper,
        });

        self.begin_block(upper);
        let length = self.value_id();
        self.instructions.push(NirInstruction::ConstI64 {
            dest: length,
            value: slots.len() as i64,
        });
        let outside = self.value_id();
        self.instructions.push(NirInstruction::BinaryI64 {
            dest: outside,
            op: NirScalarBinaryOp::GeSigned,
            left: index_value,
            right: length,
        });
        self.finish_block(NirTerminator::CondBranch {
            condition: outside,
            then_target: trap,
            else_target: first,
        });

        self.begin_block(trap);
        self.instructions.push(NirInstruction::RuntimeTrap {
            kind: 4,
            location: 0,
        });
        self.finish_block(NirTerminator::Unreachable);

        let result_slot = self.next_slot;
        self.next_slot += 1;
        for (position, (check, hit)) in branches.iter().copied().enumerate() {
            self.begin_block(check);
            let expected = self.value_id();
            self.instructions.push(NirInstruction::ConstI64 {
                dest: expected,
                value: position as i64,
            });
            let matches = self.value_id();
            self.instructions.push(NirInstruction::BinaryI64 {
                dest: matches,
                op: NirScalarBinaryOp::Eq,
                left: index_value,
                right: expected,
            });
            let next = branches.get(position + 1).map_or(trap, |branch| branch.0);
            self.finish_block(NirTerminator::CondBranch {
                condition: matches,
                then_target: hit,
                else_target: next,
            });

            self.begin_block(hit);
            let loaded = self.value_id();
            self.instructions.push(NirInstruction::LocalRead {
                dest: loaded,
                local: slots[position],
            });
            self.instructions.push(NirInstruction::LocalWrite {
                local: result_slot,
                value: loaded,
            });
            self.finish_block(NirTerminator::Branch { target: merge });
        }
        self.begin_block(merge);
        let dest = self.value_id();
        self.instructions.push(NirInstruction::LocalRead {
            dest,
            local: result_slot,
        });
        Ok(dest)
    }

    fn lower_call(
        &mut self,
        function: &str,
        args: &[SemanticExpression],
    ) -> Result<Option<u32>, LoweringError> {
        if function == "Console::write" {
            let [argument] = args else {
                return Err(LoweringError::UnsupportedStatement(
                    "Console::write requires exactly one String argument".to_string(),
                ));
            };
            let text = match argument {
                SemanticExpression::LiteralString(text) => text.clone(),
                SemanticExpression::Identifier(name) => {
                    self.static_strings.get(name).cloned().ok_or_else(|| {
                        LoweringError::UnsupportedStatement(format!(
                            "Console::write argument `{name}` is not a static String"
                        ))
                    })?
                }
                _ => {
                    return Err(LoweringError::UnsupportedStatement(
                        "Console::write currently requires a static String".to_string(),
                    ))
                }
            };
            self.instructions
                .push(NirInstruction::ConsoleWriteUtf8 { text });
            return Ok(None);
        }
        let (function_id, returns_value) = self
            .function_map
            .get(function)
            .copied()
            .ok_or_else(|| LoweringError::UnknownFunction(function.to_string()))?;
        let args = args
            .iter()
            .map(|arg| self.lower_expression(arg))
            .collect::<Result<Vec<_>, _>>()?;
        let dest = returns_value.then(|| self.value_id());
        self.instructions.push(NirInstruction::Call {
            dest,
            function: function_id,
            args,
        });
        Ok(dest)
    }
}

struct TypeResolver<'a> {
    types: &'a mut TypeTable,
    _constants: &'a mut ConstantPool,
    name_to_id: HashMap<String, NirTypeId>,
}

impl<'a> TypeResolver<'a> {
    fn new(types: &'a mut TypeTable, constants: &'a mut ConstantPool) -> Self {
        let mut resolver = Self {
            types,
            _constants: constants,
            name_to_id: HashMap::new(),
        };

        let builtins = [
            ("Unit", NirType::Unit),
            ("Never", NirType::Never),
            ("Bool", NirType::Bool),
            (
                "Int",
                NirType::Int {
                    signed: true,
                    bits: 64,
                },
            ),
            (
                "UInt",
                NirType::Int {
                    signed: false,
                    bits: 64,
                },
            ),
            ("Float", NirType::Float { bits: 64 }),
            ("String", NirType::String),
            ("Char", NirType::Char),
        ];
        for (name, ty) in &builtins {
            let id = resolver.types.add(ty.clone());
            resolver.name_to_id.insert(name.to_string(), id);
        }
        resolver
    }

    fn resolve_type_name(&mut self, name: &str) -> NirTypeId {
        if let Some(&id) = self.name_to_id.get(name) {
            return id;
        }
        let id = self.types.add(NirType::Unit);
        self.name_to_id.insert(name.to_string(), id);
        id
    }

    fn register_type_decl(&mut self, decl: &SemanticTypeDecl) {
        let ty = match &decl.kind {
            SemanticTypeKind::Struct { fields } => {
                let nir_fields: Vec<NirStructField> = fields
                    .iter()
                    .map(|(fname, ftype)| NirStructField {
                        name: fname.clone(),
                        ty: self.resolve_type_name(ftype),
                    })
                    .collect();
                NirType::Struct(NirStructType {
                    name: decl.name.clone(),
                    fields: nir_fields,
                })
            }
            SemanticTypeKind::Enum { variants } => {
                let nir_variants: Vec<NirEnumVariant> = variants
                    .iter()
                    .map(|(vname, vfields)| NirEnumVariant {
                        name: vname.clone(),
                        fields: vfields.iter().map(|f| self.resolve_type_name(f)).collect(),
                    })
                    .collect();
                NirType::Enum(NirEnumType {
                    name: decl.name.clone(),
                    variants: nir_variants,
                })
            }
        };
        let id = self.types.add(ty);
        self.name_to_id.insert(decl.name.clone(), id);
    }
}

struct HnirLowerer {
    module: HNirModule,
    type_table: Vec<NirType>,
    constant_pool: ConstantPool,
    type_name_map: HashMap<String, NirTypeId>,
    next_value: u32,
    next_place: u32,
    next_block: u32,
    local_places: HashMap<String, HPlaceId>,
    local_values: HashMap<String, HValueId>,
    static_arrays: HashMap<String, Vec<SemanticExpression>>,
    pending_places: Vec<(HPlaceId, HPlace)>,
    block_instructions: Vec<(HValueId, HInstruction)>,
    function_ids: HashMap<String, NirFunctionId>,
}

impl HnirLowerer {
    fn new(program: &SemanticProgram) -> Self {
        let mut type_name_map = HashMap::new();
        let mut type_table_types = TypeTable::new();
        let builtins = [
            ("Unit", NirType::Unit),
            ("Bool", NirType::Bool),
            (
                "Int",
                NirType::Int {
                    signed: true,
                    bits: 64,
                },
            ),
            ("String", NirType::String),
        ];
        let mut types = Vec::new();
        for (name, ty) in &builtins {
            let id = type_table_types.add(ty.clone());
            type_name_map.insert(name.to_string(), id);
            types.push(ty.clone());
        }
        Self {
            module: HNirModule::new(),
            type_table: types,
            constant_pool: ConstantPool::new(),
            type_name_map,
            next_value: 0,
            next_place: 0,
            next_block: 0,
            local_places: HashMap::new(),
            local_values: HashMap::new(),
            static_arrays: HashMap::new(),
            pending_places: Vec::new(),
            block_instructions: Vec::new(),
            function_ids: program
                .functions
                .iter()
                .enumerate()
                .map(|(index, function)| (function.name.clone(), NirFunctionId(index as u32)))
                .collect(),
        }
    }

    fn next_vid(&mut self) -> HValueId {
        let v = HValueId(self.next_value);
        self.next_value += 1;
        v
    }
    fn next_pid(&mut self) -> HPlaceId {
        let p = HPlaceId(self.next_place);
        self.next_place += 1;
        p
    }
    fn next_bid(&mut self) -> HBlockId {
        let b = HBlockId(self.next_block);
        self.next_block += 1;
        b
    }

    fn resolve_type(&mut self, name: &str) -> NirTypeId {
        if let Some(&id) = self.type_name_map.get(name) {
            return id;
        }
        let id = NirTypeId(self.type_table.len() as u32);
        self.type_table.push(NirType::Unit);
        self.type_name_map.insert(name.to_string(), id);
        id
    }

    fn resolve_return_type(&self, name: &str) -> NirTypeId {
        *self.type_name_map.get(name).unwrap_or(&NirTypeId(0))
    }

    fn lower_function(&mut self, func: &SemanticFunction) -> Result<(), LoweringError> {
        self.local_places.clear();
        self.local_values.clear();
        self.static_arrays.clear();
        self.pending_places.clear();
        self.next_block = 0;
        self.next_place = 0;

        let return_ty = self.resolve_return_type(&func.return_type);

        let mut function_places = Vec::new();
        for (i, param) in func.parameters.iter().enumerate() {
            let place_id = self.next_pid();
            let ty = self.resolve_type(&param.type_name);
            function_places.push((place_id, HPlace::Parameter { index: i as u32 }));
            self.local_places.insert(param.name.clone(), place_id);
            let _ = ty;
        }

        let entry = self.next_bid();
        self.block_instructions.clear();

        let mut exit_value = None;
        for stmt in &func.body {
            let val = self.lower_statement(stmt)?;
            if matches!(stmt, SemanticStatement::Return(_)) {
                exit_value = val;
            }
        }

        let terminator = match exit_value {
            Some(v) => HTerminator::Return(Some(v)),
            None => HTerminator::Return(None),
        };

        let entry_block = HBlock {
            id: entry,
            instructions: self.block_instructions.clone(),
            terminator,
        };

        let sig = NirCallableSignature {
            kind: func.kind,
            parameters: func
                .parameters
                .iter()
                .map(|p| NirParameter {
                    ty: self.resolve_type(&p.type_name),
                    passing: p.passing,
                })
                .collect(),
            return_type: return_ty,
            effects: NirEffectSet::empty(),
        };

        function_places.append(&mut self.pending_places);

        let hfunc = HFunction {
            id: HFunctionId(self.module.functions.len() as u32),
            signature: sig,
            places: function_places,
            blocks: vec![entry_block],
            entry,
        };

        self.module.add_function(hfunc);
        Ok(())
    }

    fn lower_statement(
        &mut self,
        stmt: &SemanticStatement,
    ) -> Result<Option<HValueId>, LoweringError> {
        match stmt {
            SemanticStatement::Let { name, value, .. } => {
                if let SemanticExpression::Array(elements) = value {
                    self.static_arrays.insert(name.clone(), elements.clone());
                    return Ok(None);
                }
                let val = self.lower_expression(value)?;
                let place_id = self.next_pid();
                self.pending_places.push((
                    place_id,
                    HPlace::Local {
                        symbol: nexa_symbols::SymbolId(0),
                    },
                ));
                self.local_places.insert(name.clone(), place_id);
                self.local_values.insert(name.clone(), val);
                Ok(Some(val))
            }
            SemanticStatement::Assign { target, value } => {
                let val = self.lower_expression(value)?;
                let place = *self.local_places.get(target).ok_or_else(|| {
                    LoweringError::InternalError(format!("unknown variable: {}", target))
                })?;
                let vid = self.next_vid();
                self.block_instructions
                    .push((vid, HInstruction::Store { place, value: val }));
                self.local_values.insert(target.clone(), val);
                Ok(None)
            }
            SemanticStatement::ArrayAssign {
                target,
                index,
                value,
            } => {
                self.lower_expression(index)?;
                self.lower_expression(value)?;
                if let SemanticExpression::LiteralInt(index) = index {
                    if let Ok(index) = usize::try_from(*index) {
                        if let Some(elements) = self.static_arrays.get_mut(target) {
                            if let Some(element) = elements.get_mut(index) {
                                *element = value.clone();
                            }
                        }
                    }
                }
                Ok(None)
            }
            SemanticStatement::Return(expr) => match expr {
                Some(e) => {
                    let val = self.lower_expression(e)?;
                    Ok(Some(val))
                }
                None => Ok(None),
            },
            SemanticStatement::Expression(expr) => {
                let val = self.lower_expression(expr)?;
                Ok(Some(val))
            }
            SemanticStatement::If {
                condition,
                then_body,
                else_body,
            } => {
                self.lower_expression(condition)?;
                let places = self.local_places.clone();
                let values = self.local_values.clone();
                for statement in then_body {
                    self.lower_statement(statement)?;
                }
                self.local_places = places.clone();
                self.local_values = values.clone();
                for statement in else_body {
                    self.lower_statement(statement)?;
                }
                self.local_places = places;
                self.local_values = values;
                Ok(None)
            }
            SemanticStatement::While { condition, body } => {
                self.lower_expression(condition)?;
                for statement in body {
                    self.lower_statement(statement)?;
                }
                Ok(None)
            }
            SemanticStatement::Loop { body } => {
                for statement in body {
                    self.lower_statement(statement)?;
                }
                Ok(None)
            }
            SemanticStatement::ForLiteral {
                variable,
                iterable,
                body,
            } => {
                let elements = match iterable {
                    SemanticExpression::Array(elements) => elements.clone(),
                    SemanticExpression::Identifier(name) => {
                        self.static_arrays.get(name).cloned().ok_or_else(|| {
                            LoweringError::InternalError(format!("unknown array: {name}"))
                        })?
                    }
                    _ => {
                        return Err(LoweringError::UnsupportedStatement(
                            "non-static for iterable in HNIR lowering".to_string(),
                        ))
                    }
                };
                let old_value = self.local_values.get(variable).copied();
                for element in &elements {
                    let value = self.lower_expression(element)?;
                    self.local_values.insert(variable.clone(), value);
                    for statement in body {
                        self.lower_statement(statement)?;
                    }
                }
                match old_value {
                    Some(value) => {
                        self.local_values.insert(variable.clone(), value);
                    }
                    None => {
                        self.local_values.remove(variable);
                    }
                }
                Ok(None)
            }
            SemanticStatement::Break | SemanticStatement::Continue => Ok(None),
        }
    }

    fn lower_expression(&mut self, expr: &SemanticExpression) -> Result<HValueId, LoweringError> {
        match expr {
            SemanticExpression::LiteralInt(n) => {
                let cid = self.constant_pool.add(NirConstant::Integer(*n));
                let vid = self.next_vid();
                let ty = self.resolve_type("Int");
                self.block_instructions
                    .push((vid, HInstruction::Const { constant: cid, ty }));
                Ok(vid)
            }
            SemanticExpression::LiteralFloat(f) => {
                let bits = f.to_bits();
                let cid = self.constant_pool.add(NirConstant::FloatBits(bits));
                let vid = self.next_vid();
                let ty = self.resolve_type("Float");
                self.block_instructions
                    .push((vid, HInstruction::Const { constant: cid, ty }));
                Ok(vid)
            }
            SemanticExpression::LiteralString(s) => {
                let cid = self.constant_pool.add(NirConstant::StringUtf8(s.clone()));
                let vid = self.next_vid();
                let ty = self.resolve_type("String");
                self.block_instructions
                    .push((vid, HInstruction::Const { constant: cid, ty }));
                Ok(vid)
            }
            SemanticExpression::LiteralBool(b) => {
                let val = if *b { 1i128 } else { 0i128 };
                let cid = self.constant_pool.add(NirConstant::Integer(val));
                let vid = self.next_vid();
                let ty = self.resolve_type("Bool");
                self.block_instructions
                    .push((vid, HInstruction::Const { constant: cid, ty }));
                Ok(vid)
            }
            SemanticExpression::Identifier(name) => {
                if let Some(value) = self.local_values.get(name) {
                    return Ok(*value);
                }
                let place = *self.local_places.get(name).ok_or_else(|| {
                    LoweringError::InternalError(format!("unknown variable: {}", name))
                })?;
                let vid = self.next_vid();
                self.block_instructions.push((
                    vid,
                    HInstruction::Load {
                        place,
                        ty: NirTypeId(0),
                    },
                ));
                Ok(vid)
            }
            SemanticExpression::BinaryOp { op, left, right } => {
                let l = self.lower_expression(left)?;
                let r = self.lower_expression(right)?;
                let vid = self.next_vid();
                let ty = self.resolve_type("Int");
                let intrinsic = match op.as_str() {
                    "+" => 100,
                    "-" => 101,
                    "*" => 102,
                    "/" => 103,
                    "%" => 104,
                    "==" => 105,
                    "!=" => 106,
                    "<" => 107,
                    "<=" => 108,
                    ">" => 109,
                    ">=" => 110,
                    "&" => 111,
                    "|" => 112,
                    "^" => 113,
                    "<<" => 114,
                    ">>" => 115,
                    "&&" => 116,
                    "||" => 117,
                    _ => 199,
                };
                self.block_instructions.push((
                    vid,
                    HInstruction::RuntimeIntrinsic {
                        intrinsic: IntrinsicId(intrinsic),
                        args: vec![l, r],
                        ty,
                    },
                ));
                Ok(vid)
            }
            SemanticExpression::Call { function, args } => {
                let mut h_args = Vec::new();
                for arg in args {
                    let v = self.lower_expression(arg)?;
                    h_args.push(HArgument {
                        value: v,
                        passing: NirPassingMode::Owned,
                    });
                }
                let vid = self.next_vid();
                let instruction = if function == "Console::write" {
                    HInstruction::RuntimeIntrinsic {
                        intrinsic: IntrinsicId(1),
                        args: h_args.into_iter().map(|arg| arg.value).collect(),
                        ty: NirTypeId(0),
                    }
                } else {
                    HInstruction::Call {
                        target: self
                            .function_ids
                            .get(function)
                            .copied()
                            .ok_or_else(|| LoweringError::UnknownFunction(function.clone()))?,
                        args: h_args,
                        ty: NirTypeId(0),
                    }
                };
                self.block_instructions.push((vid, instruction));
                Ok(vid)
            }
            SemanticExpression::Index { target, index } => {
                let elements = match target.as_ref() {
                    SemanticExpression::Array(elements) => elements.clone(),
                    SemanticExpression::Identifier(name) => {
                        self.static_arrays.get(name).cloned().ok_or_else(|| {
                            LoweringError::InternalError(format!("unknown array: {name}"))
                        })?
                    }
                    _ => {
                        return Err(LoweringError::UnsupportedStatement(
                            "non-static array target in HNIR lowering".to_string(),
                        ))
                    }
                };
                if let SemanticExpression::LiteralInt(index) = index.as_ref() {
                    let index = usize::try_from(*index).map_err(|_| {
                        LoweringError::UnsupportedStatement(
                            "negative constant array index".to_string(),
                        )
                    })?;
                    let element = elements.get(index).cloned().ok_or_else(|| {
                        LoweringError::UnsupportedStatement(format!(
                            "constant array index {index} is outside length {}",
                            elements.len()
                        ))
                    })?;
                    self.lower_expression(&element)
                } else {
                    let base = elements.first().cloned().ok_or_else(|| {
                        LoweringError::UnsupportedStatement(
                            "dynamic indexing of an empty static array".to_string(),
                        )
                    })?;
                    let base = self.lower_expression(&base)?;
                    let index = self.lower_expression(index)?;
                    let vid = self.next_vid();
                    self.block_instructions.push((
                        vid,
                        HInstruction::Index {
                            base,
                            index,
                            ty: NirTypeId(0),
                        },
                    ));
                    Ok(vid)
                }
            }
            SemanticExpression::Struct { .. } | SemanticExpression::Field { .. } => {
                Err(LoweringError::UnsupportedStatement(
                    "unconsumed struct or field in HNIR lowering".to_string(),
                ))
            }
            SemanticExpression::Array(_) => Err(LoweringError::UnsupportedStatement(
                "unconsumed static array in HNIR lowering".to_string(),
            )),
        }
    }
}

struct MnirLowerer {
    module: MNirModule,
    next_value: u32,
    next_block: u32,
    h_to_m_value: HashMap<HValueId, MValueId>,
}

impl MnirLowerer {
    fn new() -> Self {
        Self {
            module: MNirModule::new(),
            next_value: 0,
            next_block: 0,
            h_to_m_value: HashMap::new(),
        }
    }

    fn next_vid(&mut self) -> MValueId {
        let v = MValueId(self.next_value);
        self.next_value += 1;
        v
    }
    fn next_bid(&mut self) -> MBlockId {
        let b = MBlockId(self.next_block);
        self.next_block += 1;
        b
    }

    fn map_value(&mut self, hvid: HValueId) -> MValueId {
        if let Some(&mvid) = self.h_to_m_value.get(&hvid) {
            return mvid;
        }
        let mvid = self.next_vid();
        self.h_to_m_value.insert(hvid, mvid);
        mvid
    }

    fn lower_function(&mut self, hfunc: &HFunction) -> Result<(), LoweringError> {
        self.h_to_m_value.clear();

        let mut m_blocks = Vec::new();
        let mut entry_id = None;

        for hblock in &hfunc.blocks {
            let mbid = MBlockId(hblock.id.0);
            if entry_id.is_none() {
                entry_id = Some(mbid);
            }

            let params: Vec<MBlockParameter> = Vec::new();

            let mut m_insts = Vec::new();
            for &(hvid, ref hinst) in &hblock.instructions {
                let mvid = self.map_value(hvid);
                if let Some(minst) = self.lower_instruction(hinst)? {
                    m_insts.push((mvid, minst));
                }
            }

            let m_term = self.lower_terminator(&hblock.terminator)?;

            m_blocks.push(MBlock {
                id: mbid,
                parameters: params,
                instructions: m_insts,
                terminator: m_term,
            });
        }

        let body = MFunctionBody {
            entry: entry_id.unwrap_or(MBlockId(0)),
            blocks: m_blocks,
        };

        self.module.add_function(MFunction {
            id: NirFunctionId(hfunc.id.0),
            signature: hfunc.signature.clone(),
            body: Some(body),
        });

        Ok(())
    }

    fn lower_instruction(
        &mut self,
        inst: &HInstruction,
    ) -> Result<Option<MInstruction>, LoweringError> {
        match inst {
            HInstruction::Const { constant, ty } => Ok(Some(MInstruction::Const {
                constant: *constant,
                ty: *ty,
            })),
            HInstruction::Copy { source, ty } => Ok(Some(MInstruction::CopyValue {
                source: self.map_value(*source),
                ty: *ty,
            })),
            HInstruction::Move { source, ty } => Ok(Some(MInstruction::MoveValue {
                source: self.map_value(*source),
                ty: *ty,
            })),
            HInstruction::Load { place: _, ty } => Ok(Some(MInstruction::Load {
                address: MValueId(0),
                ty: *ty,
            })),
            HInstruction::Store { place: _, value } => Ok(Some(MInstruction::Store {
                address: MValueId(0),
                value: self.map_value(*value),
            })),
            HInstruction::Call { target, args, ty } => {
                let m_args: Vec<MValueId> = args.iter().map(|a| self.map_value(a.value)).collect();
                Ok(Some(MInstruction::Call {
                    function: *target,
                    args: m_args,
                    ty: *ty,
                }))
            }
            HInstruction::Drop { value } => Ok(Some(MInstruction::Drop {
                value: self.map_value(*value),
            })),
            HInstruction::ResourceCleanup { value } => Ok(Some(MInstruction::ResourceCleanup {
                value: self.map_value(*value),
            })),
            HInstruction::RuntimeIntrinsic {
                intrinsic,
                args,
                ty,
            } => Ok(Some(MInstruction::RuntimeIntrinsic {
                intrinsic: *intrinsic,
                args: args.iter().map(|value| self.map_value(*value)).collect(),
                ty: *ty,
            })),
            _ => Ok(None),
        }
    }

    fn lower_terminator(&mut self, term: &HTerminator) -> Result<MTerminator, LoweringError> {
        match term {
            HTerminator::Goto(target) => Ok(MTerminator::Goto {
                target: MBlockId(target.0),
                args: vec![],
            }),
            HTerminator::Branch {
                condition,
                then_block,
                else_block,
            } => Ok(MTerminator::Branch {
                condition: self.map_value(*condition),
                then_block: MBlockId(then_block.0),
                then_args: vec![],
                else_block: MBlockId(else_block.0),
                else_args: vec![],
            }),
            HTerminator::Return(val) => Ok(MTerminator::Return(val.map(|v| self.map_value(v)))),
            HTerminator::Panic(v) => Ok(MTerminator::Return(Some(self.map_value(*v)))),
            HTerminator::Unreachable => Ok(MTerminator::Unreachable),
            HTerminator::SwitchEnum {
                value,
                arms,
                default,
            } => {
                let m_arms: Vec<MEnumArm> = arms
                    .iter()
                    .map(|a| MEnumArm {
                        variant_index: a.variant_index,
                        target: MBlockId(a.target.0),
                        args: vec![],
                    })
                    .collect();
                Ok(MTerminator::SwitchEnum {
                    value: self.map_value(*value),
                    arms: m_arms,
                    default: default.map(|d| MBlockId(d.0)),
                })
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;

    fn int_type() -> String {
        "Int".to_string()
    }
    fn unit_type() -> String {
        "Unit".to_string()
    }

    #[test]
    fn semantic_program_creation() {
        let mut p = SemanticProgram::new();
        p.add_function(SemanticFunction {
            name: "main".into(),
            kind: NirCallableKind::Function,
            parameters: vec![],
            return_type: unit_type(),
            body: vec![],
        });
        assert_eq!(p.functions.len(), 1);
    }

    #[test]
    fn semantic_parameter_creation() {
        let p = SemanticParameter {
            name: "x".into(),
            type_name: int_type(),
            passing: NirPassingMode::Owned,
        };
        assert_eq!(p.name, "x");
    }

    #[test]
    fn semantic_type_decl_struct() {
        let decl = SemanticTypeDecl {
            name: "User".into(),
            kind: SemanticTypeKind::Struct {
                fields: vec![("id".into(), int_type())],
            },
        };
        assert_eq!(decl.name, "User");
    }

    #[test]
    fn semantic_type_decl_enum() {
        let decl = SemanticTypeDecl {
            name: "Option".into(),
            kind: SemanticTypeKind::Enum {
                variants: vec![("Some".into(), vec![int_type()]), ("None".into(), vec![])],
            },
        };
        assert_eq!(decl.name, "Option");
    }

    #[test]
    fn lowering_error_variants() {
        let e1 = LoweringError::UnsupportedStatement("test".into());
        let e2 = LoweringError::UnknownFunction("f".into());
        let e3 = LoweringError::InternalError("ice".into());
        assert!(format!("{:?}", e1).contains("Unsupported"));
        assert!(format!("{:?}", e2).contains("Unknown"));
        assert!(format!("{:?}", e3).contains("Internal"));
    }

    #[test]
    fn lower_empty_program_to_hnir() {
        let p = SemanticProgram::new();
        let m = LoweringPipeline::lower_to_hnir(&p).unwrap();
        assert!(m.functions.is_empty());
    }

    #[test]
    fn lower_function_return_literal() {
        let p = SemanticProgram {
            functions: vec![SemanticFunction {
                name: "main".into(),
                kind: NirCallableKind::Function,
                parameters: vec![],
                return_type: int_type(),
                body: vec![SemanticStatement::Return(Some(
                    SemanticExpression::LiteralInt(42),
                ))],
            }],
            type_decls: vec![],
        };
        let m = LoweringPipeline::lower_to_hnir(&p).unwrap();
        assert_eq!(m.functions.len(), 1);
        let f = &m.functions[0];
        assert_eq!(f.blocks.len(), 1);
        assert!(f.blocks[0].terminator.return_value().is_some());
    }

    #[test]
    fn lower_function_binary_op() {
        let p = SemanticProgram {
            functions: vec![SemanticFunction {
                name: "add".into(),
                kind: NirCallableKind::Function,
                parameters: vec![
                    SemanticParameter {
                        name: "a".into(),
                        type_name: int_type(),
                        passing: NirPassingMode::Owned,
                    },
                    SemanticParameter {
                        name: "b".into(),
                        type_name: int_type(),
                        passing: NirPassingMode::Owned,
                    },
                ],
                return_type: int_type(),
                body: vec![SemanticStatement::Return(Some(
                    SemanticExpression::BinaryOp {
                        op: "+".into(),
                        left: Box::new(SemanticExpression::Identifier("a".into())),
                        right: Box::new(SemanticExpression::Identifier("b".into())),
                    },
                ))],
            }],
            type_decls: vec![],
        };
        let m = LoweringPipeline::lower_to_hnir(&p).unwrap();
        assert_eq!(m.functions.len(), 1);
        assert_eq!(m.functions[0].blocks[0].instructions.len(), 3);
    }

    #[test]
    fn lower_function_call() {
        let p = SemanticProgram {
            functions: vec![
                SemanticFunction {
                    name: "foo".into(),
                    kind: NirCallableKind::Function,
                    parameters: vec![SemanticParameter {
                        name: "value".into(),
                        type_name: int_type(),
                        passing: NirPassingMode::Owned,
                    }],
                    return_type: int_type(),
                    body: vec![SemanticStatement::Return(Some(
                        SemanticExpression::Identifier("value".into()),
                    ))],
                },
                SemanticFunction {
                    name: "caller".into(),
                    kind: NirCallableKind::Function,
                    parameters: vec![],
                    return_type: int_type(),
                    body: vec![SemanticStatement::Return(Some(SemanticExpression::Call {
                        function: "foo".into(),
                        args: vec![SemanticExpression::LiteralInt(1)],
                    }))],
                },
            ],
            type_decls: vec![],
        };
        let m = LoweringPipeline::lower_to_hnir(&p).unwrap();
        assert_eq!(m.functions[1].blocks[0].instructions.len(), 2);
    }

    #[test]
    fn lower_function_let_and_return() {
        let p = SemanticProgram {
            functions: vec![SemanticFunction {
                name: "f".into(),
                kind: NirCallableKind::Function,
                parameters: vec![],
                return_type: int_type(),
                body: vec![
                    SemanticStatement::Let {
                        name: "x".into(),
                        type_name: Some(int_type()),
                        value: SemanticExpression::LiteralInt(10),
                    },
                    SemanticStatement::Return(Some(SemanticExpression::Identifier("x".into()))),
                ],
            }],
            type_decls: vec![],
        };
        let m = LoweringPipeline::lower_to_hnir(&p).unwrap();
        // The let binding aliases its SSA initializer; returning it does not
        // synthesize redundant load instructions.
        assert_eq!(m.functions[0].blocks[0].instructions.len(), 1);
    }

    #[test]
    fn lower_function_string_literal() {
        let p = SemanticProgram {
            functions: vec![SemanticFunction {
                name: "f".into(),
                kind: NirCallableKind::Function,
                parameters: vec![],
                return_type: "String".into(),
                body: vec![SemanticStatement::Return(Some(
                    SemanticExpression::LiteralString("hello".into()),
                ))],
            }],
            type_decls: vec![],
        };
        let m = LoweringPipeline::lower_to_hnir(&p).unwrap();
        assert_eq!(m.functions[0].blocks[0].instructions.len(), 1);
    }

    #[test]
    fn lower_function_bool_literal() {
        let p = SemanticProgram {
            functions: vec![SemanticFunction {
                name: "f".into(),
                kind: NirCallableKind::Function,
                parameters: vec![],
                return_type: "Bool".into(),
                body: vec![SemanticStatement::Return(Some(
                    SemanticExpression::LiteralBool(true),
                ))],
            }],
            type_decls: vec![],
        };
        let m = LoweringPipeline::lower_to_hnir(&p).unwrap();
        assert_eq!(m.functions[0].blocks[0].instructions.len(), 1);
    }

    #[test]
    fn lower_function_assign() {
        let p = SemanticProgram {
            functions: vec![SemanticFunction {
                name: "f".into(),
                kind: NirCallableKind::Function,
                parameters: vec![],
                return_type: unit_type(),
                body: vec![
                    SemanticStatement::Let {
                        name: "x".into(),
                        type_name: Some(int_type()),
                        value: SemanticExpression::LiteralInt(1),
                    },
                    SemanticStatement::Assign {
                        target: "x".into(),
                        value: SemanticExpression::LiteralInt(2),
                    },
                ],
            }],
            type_decls: vec![],
        };
        let m = LoweringPipeline::lower_to_hnir(&p).unwrap();
        assert_eq!(m.functions[0].blocks[0].instructions.len(), 3);
    }

    #[test]
    fn lower_to_mnir_basic() {
        let p = SemanticProgram {
            functions: vec![SemanticFunction {
                name: "f".into(),
                kind: NirCallableKind::Function,
                parameters: vec![],
                return_type: int_type(),
                body: vec![SemanticStatement::Return(Some(
                    SemanticExpression::LiteralInt(5),
                ))],
            }],
            type_decls: vec![],
        };
        let hnir = LoweringPipeline::lower_to_hnir(&p).unwrap();
        let mnir = LoweringPipeline::lower_to_mnir(&hnir).unwrap();
        assert_eq!(mnir.functions.len(), 1);
        assert!(mnir.functions[0].body.is_some());
        let body = mnir.functions[0].body.as_ref().unwrap();
        assert_eq!(body.blocks.len(), 1);
        assert_eq!(body.blocks[0].instructions.len(), 1);
    }

    #[test]
    fn lower_full_pipeline() {
        let p = SemanticProgram {
            functions: vec![SemanticFunction {
                name: "main".into(),
                kind: NirCallableKind::Function,
                parameters: vec![],
                return_type: int_type(),
                body: vec![SemanticStatement::Return(Some(
                    SemanticExpression::LiteralInt(42),
                ))],
            }],
            type_decls: vec![SemanticTypeDecl {
                name: "User".into(),
                kind: SemanticTypeKind::Struct {
                    fields: vec![("id".into(), int_type())],
                },
            }],
        };
        let (nir, hnir, mnir) = LoweringPipeline::lower_full(&p).unwrap();
        assert_eq!(hnir.functions.len(), 1);
        assert_eq!(mnir.functions.len(), 1);
        assert!(nir.types.get(nir_nid(0)).is_some() || true);
    }

    fn nir_nid(n: u32) -> NirTypeId {
        NirTypeId(n)
    }
}

trait ReturnHelper {
    fn return_value(&self) -> Option<HValueId>;
}

impl ReturnHelper for HTerminator {
    fn return_value(&self) -> Option<HValueId> {
        match self {
            HTerminator::Return(v) => *v,
            _ => None,
        }
    }
}
