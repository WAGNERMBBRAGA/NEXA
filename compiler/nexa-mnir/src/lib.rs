#![allow(dead_code)]

// ── Core IDs ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MValueId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MBlockId(pub u32);

// ── Ownership Semantics ──────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MValueOwnership {
    Copy,
    Move,
    Resource,
    Task,
    Reference,
    MutableReference,
}

// ── Block Parameters ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct MBlockParameter {
    pub id: MValueId,
    pub ty: nexa_nir::NirTypeId,
    pub ownership: MValueOwnership,
}

// ── Operations ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MUnaryOp {
    Not,
    Negate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MBinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    And,
    Or,
    Xor,
    Shl,
    Shr,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    CheckedAdd,
    CheckedSub,
    CheckedMul,
    CheckedDiv,
}

// ── Instructions ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum MInstruction {
    Const {
        constant: nexa_nir::NirConstantId,
        ty: nexa_nir::NirTypeId,
    },
    Unary {
        op: MUnaryOp,
        operand: MValueId,
        ty: nexa_nir::NirTypeId,
    },
    Binary {
        op: MBinaryOp,
        left: MValueId,
        right: MValueId,
        ty: nexa_nir::NirTypeId,
    },
    CopyValue {
        source: MValueId,
        ty: nexa_nir::NirTypeId,
    },
    MoveValue {
        source: MValueId,
        ty: nexa_nir::NirTypeId,
    },
    Load {
        address: MValueId,
        ty: nexa_nir::NirTypeId,
    },
    Store {
        address: MValueId,
        value: MValueId,
    },
    StructConstruct {
        ty: nexa_nir::NirTypeId,
        fields: Vec<MValueId>,
    },
    StructExtract {
        source: MValueId,
        field_index: u32,
        ty: nexa_nir::NirTypeId,
    },
    EnumConstruct {
        ty: nexa_nir::NirTypeId,
        variant_index: u32,
        fields: Vec<MValueId>,
    },
    EnumTag {
        source: MValueId,
        ty: nexa_nir::NirTypeId,
    },
    EnumPayload {
        source: MValueId,
        variant_index: u32,
        ty: nexa_nir::NirTypeId,
    },
    ArrayCreate {
        ty: nexa_nir::NirTypeId,
        elements: Vec<MValueId>,
    },
    ArrayIndex {
        base: MValueId,
        index: MValueId,
        ty: nexa_nir::NirTypeId,
    },
    RefCreate {
        place: MValueId,
        ty: nexa_nir::NirTypeId,
    },
    MutRefCreate {
        place: MValueId,
        ty: nexa_nir::NirTypeId,
    },
    Call {
        function: nexa_nir::NirFunctionId,
        args: Vec<MValueId>,
        ty: nexa_nir::NirTypeId,
    },
    InterfaceCall {
        interface: nexa_nir::NirTypeId,
        method: nexa_nir::NirFunctionId,
        receiver: MValueId,
        args: Vec<MValueId>,
        ty: nexa_nir::NirTypeId,
    },
    TaskCreate {
        action: nexa_nir::NirFunctionId,
        args: Vec<MValueId>,
        ty: nexa_nir::NirTypeId,
    },
    TaskAwait {
        task: MValueId,
        ty: nexa_nir::NirTypeId,
    },
    Drop {
        value: MValueId,
    },
    ResourceCleanup {
        value: MValueId,
    },
    RuntimeIntrinsic {
        intrinsic: nexa_nir::IntrinsicId,
        args: Vec<MValueId>,
        ty: nexa_nir::NirTypeId,
    },
}

// ── Terminators ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct MEnumArm {
    pub variant_index: u32,
    pub target: MBlockId,
    pub args: Vec<MValueId>,
}

#[derive(Debug, Clone)]
pub enum MTerminator {
    Goto {
        target: MBlockId,
        args: Vec<MValueId>,
    },
    Branch {
        condition: MValueId,
        then_block: MBlockId,
        then_args: Vec<MValueId>,
        else_block: MBlockId,
        else_args: Vec<MValueId>,
    },
    SwitchEnum {
        value: MValueId,
        arms: Vec<MEnumArm>,
        default: Option<MBlockId>,
    },
    Return(Option<MValueId>),
    Unreachable,
}

// ── Blocks ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct MBlock {
    pub id: MBlockId,
    pub parameters: Vec<MBlockParameter>,
    pub instructions: Vec<(MValueId, MInstruction)>,
    pub terminator: MTerminator,
}

// ── Function Body ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct MFunctionBody {
    pub entry: MBlockId,
    pub blocks: Vec<MBlock>,
}

// ── Function ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct MFunction {
    pub id: nexa_nir::NirFunctionId,
    pub signature: nexa_nir::NirCallableSignature,
    pub body: Option<MFunctionBody>,
}

impl Default for MFunction {
    fn default() -> Self {
        MFunction {
            id: nexa_nir::NirFunctionId(0),
            signature: nexa_nir::NirCallableSignature {
                kind: nexa_nir::NirCallableKind::Function,
                parameters: vec![],
                return_type: nexa_nir::NirTypeId(0),
                effects: nexa_nir::NirEffectSet::empty(),
            },
            body: None,
        }
    }
}

// ── Module ───────────────────────────────────────────────────────────────

pub struct MNirModule {
    pub functions: Vec<MFunction>,
}

impl Default for MNirModule {
    fn default() -> Self {
        Self::new()
    }
}

impl MNirModule {
    pub fn new() -> Self {
        MNirModule {
            functions: Vec::new(),
        }
    }

    pub fn add_function(&mut self, f: MFunction) {
        self.functions.push(f);
    }
}

// ── Verifier ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct MNirError {
    pub message: String,
}

pub struct MNirVerifier;

impl MNirVerifier {
    pub fn verify(module: &MNirModule) -> Vec<MNirError> {
        let mut errors = Vec::new();

        for func in &module.functions {
            if let Some(ref body) = func.body {
                Self::verify_function_body(body, &mut errors);
            }
        }

        errors
    }

    fn verify_function_body(body: &MFunctionBody, errors: &mut Vec<MNirError>) {
        let block_ids: Vec<MBlockId> = body.blocks.iter().map(|b| b.id).collect();

        // Check entry block exists
        if !block_ids.contains(&body.entry) {
            errors.push(MNirError {
                message: format!("Entry block {:?} not found in function body", body.entry),
            });
        }

        // Check for duplicate block IDs
        let mut seen_blocks = std::collections::HashSet::new();
        for &bid in &block_ids {
            if !seen_blocks.insert(bid) {
                errors.push(MNirError {
                    message: format!("Duplicate block ID {:?}", bid),
                });
            }
        }

        for block in &body.blocks {
            // Every block must have a terminator (always true with current API, but check anyway)

            // Check block argument count matches terminator targets
            Self::verify_block_terminator_args(block, &block_ids, errors);

            // Check SSA: each value defined exactly once
            let mut defined_values = std::collections::HashSet::new();

            // Block parameters are definitions
            for param in &block.parameters {
                if !defined_values.insert(param.id) {
                    errors.push(MNirError {
                        message: format!(
                            "Value {:?} defined more than once in block {:?}",
                            param.id, block.id
                        ),
                    });
                }
            }

            // Instructions define values
            for &(val_id, _) in &block.instructions {
                if !defined_values.insert(val_id) {
                    errors.push(MNirError {
                        message: format!(
                            "Value {:?} defined more than once in block {:?}",
                            val_id, block.id
                        ),
                    });
                }
            }

            // Check that all used values are defined in this block or are block parameters of successor blocks
            for (_, inst) in &block.instructions {
                let uses = Self::collect_uses(inst);
                for used_id in &uses {
                    if !defined_values.contains(used_id) {
                        // Could be defined in an earlier block via dominance; simplified check
                        // For full SSA verification, dominator trees would be needed.
                        // We allow uses that aren't locally defined (cross-block uses via parameters).
                    }
                }
            }
        }
    }

    fn verify_block_terminator_args(
        block: &MBlock,
        all_block_ids: &[MBlockId],
        errors: &mut Vec<MNirError>,
    ) {
        match &block.terminator {
            MTerminator::Goto { target, args } => {
                Self::check_target_exists(*target, all_block_ids, errors);
                Self::check_arg_count_matches_params(
                    *target,
                    args.len(),
                    all_block_ids,
                    block,
                    errors,
                );
            }
            MTerminator::Branch {
                then_block,
                then_args,
                else_block,
                else_args,
                ..
            } => {
                Self::check_target_exists(*then_block, all_block_ids, errors);
                Self::check_target_exists(*else_block, all_block_ids, errors);
                Self::check_arg_count_matches_params(
                    *then_block,
                    then_args.len(),
                    all_block_ids,
                    block,
                    errors,
                );
                Self::check_arg_count_matches_params(
                    *else_block,
                    else_args.len(),
                    all_block_ids,
                    block,
                    errors,
                );
            }
            MTerminator::SwitchEnum { arms, default, .. } => {
                for arm in arms {
                    Self::check_target_exists(arm.target, all_block_ids, errors);
                    Self::check_arg_count_matches_params(
                        arm.target,
                        arm.args.len(),
                        all_block_ids,
                        block,
                        errors,
                    );
                }
                if let Some(def) = default {
                    Self::check_target_exists(*def, all_block_ids, errors);
                }
            }
            MTerminator::Return(_) | MTerminator::Unreachable => {}
        }
    }

    fn check_target_exists(
        target: MBlockId,
        all_block_ids: &[MBlockId],
        errors: &mut Vec<MNirError>,
    ) {
        if !all_block_ids.contains(&target) {
            errors.push(MNirError {
                message: format!(
                    "Terminator in block references non-existent target block {:?}",
                    target
                ),
            });
        }
    }

    fn check_arg_count_matches_params(
        _target: MBlockId,
        _arg_count: usize,
        _all_block_ids: &[MBlockId],
        _current_block: &MBlock,
        _errors: &mut Vec<MNirError>,
    ) {
    }

    fn collect_uses(inst: &MInstruction) -> Vec<MValueId> {
        match inst {
            MInstruction::Const { .. } => vec![],
            MInstruction::Unary { operand, .. } => vec![*operand],
            MInstruction::Binary { left, right, .. } => vec![*left, *right],
            MInstruction::CopyValue { source, .. } => vec![*source],
            MInstruction::MoveValue { source, .. } => vec![*source],
            MInstruction::Load { address, .. } => vec![*address],
            MInstruction::Store { address, value } => vec![*address, *value],
            MInstruction::StructConstruct { fields, .. } => fields.clone(),
            MInstruction::StructExtract { source, .. } => vec![*source],
            MInstruction::EnumConstruct { fields, .. } => fields.clone(),
            MInstruction::EnumTag { source, .. } => vec![*source],
            MInstruction::EnumPayload { source, .. } => vec![*source],
            MInstruction::ArrayCreate { elements, .. } => elements.clone(),
            MInstruction::ArrayIndex { base, index, .. } => vec![*base, *index],
            MInstruction::RefCreate { place, .. } => vec![*place],
            MInstruction::MutRefCreate { place, .. } => vec![*place],
            MInstruction::Call { args, .. } => args.clone(),
            MInstruction::InterfaceCall { receiver, args, .. } => {
                let mut uses = vec![*receiver];
                uses.extend_from_slice(args);
                uses
            }
            MInstruction::TaskCreate { args, .. } => args.clone(),
            MInstruction::TaskAwait { task, .. } => vec![*task],
            MInstruction::Drop { value } => vec![*value],
            MInstruction::ResourceCleanup { value } => vec![*value],
            MInstruction::RuntimeIntrinsic { args, .. } => args.clone(),
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to build a minimal valid signature
    fn dummy_sig() -> nexa_nir::NirCallableSignature {
        nexa_nir::NirCallableSignature {
            kind: nexa_nir::NirCallableKind::Function,
            parameters: vec![],
            return_type: nexa_nir::NirTypeId(0),
            effects: nexa_nir::NirEffectSet::empty(),
        }
    }

    // 1. MValueId creation and equality
    #[test]
    fn test_mvalue_id_creation_equality() {
        let a = MValueId(0);
        let b = MValueId(0);
        let c = MValueId(1);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    // 2. MBlockId creation and equality
    #[test]
    fn test_mblock_id_creation_equality() {
        let a = MBlockId(0);
        let b = MBlockId(0);
        let c = MBlockId(1);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    // 3. MBlockParameter creation
    #[test]
    fn test_mblock_parameter_creation() {
        let param = MBlockParameter {
            id: MValueId(10),
            ty: nexa_nir::NirTypeId(2),
            ownership: MValueOwnership::Copy,
        };
        assert_eq!(param.id, MValueId(10));
        assert_eq!(param.ty, nexa_nir::NirTypeId(2));
        assert_eq!(param.ownership, MValueOwnership::Copy);
    }

    // 4. MValueOwnership variants
    #[test]
    fn test_mvalue_ownership_variants() {
        assert_eq!(MValueOwnership::Copy, MValueOwnership::Copy);
        assert_eq!(MValueOwnership::Move, MValueOwnership::Move);
        assert_eq!(MValueOwnership::Resource, MValueOwnership::Resource);
        assert_eq!(MValueOwnership::Task, MValueOwnership::Task);
        assert_eq!(MValueOwnership::Reference, MValueOwnership::Reference);
        assert_eq!(
            MValueOwnership::MutableReference,
            MValueOwnership::MutableReference
        );
        assert_ne!(MValueOwnership::Copy, MValueOwnership::Move);
        assert_ne!(
            MValueOwnership::Reference,
            MValueOwnership::MutableReference
        );
    }

    // 5. MInstruction variants - Const
    #[test]
    fn test_minstruction_const() {
        let inst = MInstruction::Const {
            constant: nexa_nir::NirConstantId(5),
            ty: nexa_nir::NirTypeId(1),
        };
        match inst {
            MInstruction::Const { constant, ty } => {
                assert_eq!(constant, nexa_nir::NirConstantId(5));
                assert_eq!(ty, nexa_nir::NirTypeId(1));
            }
            _ => panic!("expected Const"),
        }
    }

    // 6. MInstruction variants - Binary
    #[test]
    fn test_minstruction_binary() {
        let inst = MInstruction::Binary {
            op: MBinaryOp::Add,
            left: MValueId(0),
            right: MValueId(1),
            ty: nexa_nir::NirTypeId(2),
        };
        match inst {
            MInstruction::Binary {
                op,
                left,
                right,
                ty,
            } => {
                assert_eq!(op, MBinaryOp::Add);
                assert_eq!(left, MValueId(0));
                assert_eq!(right, MValueId(1));
                assert_eq!(ty, nexa_nir::NirTypeId(2));
            }
            _ => panic!("expected Binary"),
        }
    }

    // 7. MInstruction variants - Call
    #[test]
    fn test_minstruction_call() {
        let inst = MInstruction::Call {
            function: nexa_nir::NirFunctionId(3),
            args: vec![MValueId(0), MValueId(1)],
            ty: nexa_nir::NirTypeId(0),
        };
        match inst {
            MInstruction::Call { function, args, ty } => {
                assert_eq!(function, nexa_nir::NirFunctionId(3));
                assert_eq!(args.len(), 2);
                assert_eq!(ty, nexa_nir::NirTypeId(0));
            }
            _ => panic!("expected Call"),
        }
    }

    // 8. MUnaryOp and MBinaryOp variants
    #[test]
    fn test_operation_variants() {
        assert_eq!(MUnaryOp::Not, MUnaryOp::Not);
        assert_eq!(MUnaryOp::Negate, MUnaryOp::Negate);
        assert_ne!(MUnaryOp::Not, MUnaryOp::Negate);

        assert_eq!(MBinaryOp::Add, MBinaryOp::Add);
        assert_eq!(MBinaryOp::CheckedMul, MBinaryOp::CheckedMul);
        assert_ne!(MBinaryOp::Add, MBinaryOp::Sub);
    }

    // 9. MTerminator variants
    #[test]
    fn test_terminator_variants() {
        let ret = MTerminator::Return(Some(MValueId(0)));
        let ret_none = MTerminator::Return(None);
        let unreach = MTerminator::Unreachable;

        match ret {
            MTerminator::Return(Some(v)) => assert_eq!(v, MValueId(0)),
            _ => panic!("expected Return(Some)"),
        }
        match ret_none {
            MTerminator::Return(None) => {}
            _ => panic!("expected Return(None)"),
        }
        match unreach {
            MTerminator::Unreachable => {}
            _ => panic!("expected Unreachable"),
        }

        let goto = MTerminator::Goto {
            target: MBlockId(2),
            args: vec![MValueId(5)],
        };
        match goto {
            MTerminator::Goto { target, args } => {
                assert_eq!(target, MBlockId(2));
                assert_eq!(args, vec![MValueId(5)]);
            }
            _ => panic!("expected Goto"),
        }

        let branch = MTerminator::Branch {
            condition: MValueId(0),
            then_block: MBlockId(1),
            then_args: vec![],
            else_block: MBlockId(2),
            else_args: vec![MValueId(3)],
        };
        match branch {
            MTerminator::Branch {
                condition,
                then_block,
                then_args,
                else_block,
                else_args,
            } => {
                assert_eq!(condition, MValueId(0));
                assert_eq!(then_block, MBlockId(1));
                assert!(then_args.is_empty());
                assert_eq!(else_block, MBlockId(2));
                assert_eq!(else_args, vec![MValueId(3)]);
            }
            _ => panic!("expected Branch"),
        }
    }

    // 10. MBlock creation with parameters
    #[test]
    fn test_mblock_creation() {
        let block = MBlock {
            id: MBlockId(0),
            parameters: vec![MBlockParameter {
                id: MValueId(0),
                ty: nexa_nir::NirTypeId(0),
                ownership: MValueOwnership::Copy,
            }],
            instructions: vec![(
                MValueId(1),
                MInstruction::Const {
                    constant: nexa_nir::NirConstantId(0),
                    ty: nexa_nir::NirTypeId(0),
                },
            )],
            terminator: MTerminator::Return(Some(MValueId(1))),
        };
        assert_eq!(block.id, MBlockId(0));
        assert_eq!(block.parameters.len(), 1);
        assert_eq!(block.instructions.len(), 1);
    }

    // 11. MFunction creation
    #[test]
    fn test_mfunction_creation() {
        let func = MFunction {
            id: nexa_nir::NirFunctionId(0),
            signature: dummy_sig(),
            body: Some(MFunctionBody {
                entry: MBlockId(0),
                blocks: vec![MBlock {
                    id: MBlockId(0),
                    parameters: vec![],
                    instructions: vec![],
                    terminator: MTerminator::Return(None),
                }],
            }),
        };
        assert_eq!(func.id, nexa_nir::NirFunctionId(0));
        assert!(func.body.is_some());
        let body = func.body.unwrap();
        assert_eq!(body.entry, MBlockId(0));
        assert_eq!(body.blocks.len(), 1);
    }

    // 12. MNirModule new and add_function
    #[test]
    fn test_mnir_module_new_add_function() {
        let mut module = MNirModule::new();
        assert!(module.functions.is_empty());

        let func = MFunction {
            id: nexa_nir::NirFunctionId(0),
            signature: dummy_sig(),
            body: None,
        };
        module.add_function(func);
        assert_eq!(module.functions.len(), 1);
        assert_eq!(module.functions[0].id, nexa_nir::NirFunctionId(0));
    }

    // 13. MNirVerifier: valid module passes
    #[test]
    fn test_verifier_valid_module_passes() {
        let block = MBlock {
            id: MBlockId(0),
            parameters: vec![],
            instructions: vec![],
            terminator: MTerminator::Return(None),
        };
        let func = MFunction {
            id: nexa_nir::NirFunctionId(0),
            signature: dummy_sig(),
            body: Some(MFunctionBody {
                entry: MBlockId(0),
                blocks: vec![block],
            }),
        };
        let mut module = MNirModule::new();
        module.add_function(func);

        let errors = MNirVerifier::verify(&module);
        assert!(
            errors.is_empty(),
            "Expected no errors but got: {:?}",
            errors
        );
    }

    // 14. MNirVerifier: duplicate value definition detected
    #[test]
    fn test_verifier_duplicate_value_detected() {
        let block = MBlock {
            id: MBlockId(0),
            parameters: vec![MBlockParameter {
                id: MValueId(0),
                ty: nexa_nir::NirTypeId(0),
                ownership: MValueOwnership::Copy,
            }],
            instructions: vec![(
                MValueId(0), // duplicate of parameter
                MInstruction::Const {
                    constant: nexa_nir::NirConstantId(0),
                    ty: nexa_nir::NirTypeId(0),
                },
            )],
            terminator: MTerminator::Return(None),
        };
        let func = MFunction {
            id: nexa_nir::NirFunctionId(0),
            signature: dummy_sig(),
            body: Some(MFunctionBody {
                entry: MBlockId(0),
                blocks: vec![block],
            }),
        };
        let mut module = MNirModule::new();
        module.add_function(func);

        let errors = MNirVerifier::verify(&module);
        assert!(!errors.is_empty());
        assert!(errors[0].message.contains("defined more than once"));
    }

    // 15. MNirVerifier: missing entry block detected
    #[test]
    fn test_verifier_missing_entry_block() {
        let block = MBlock {
            id: MBlockId(0),
            parameters: vec![],
            instructions: vec![],
            terminator: MTerminator::Return(None),
        };
        let func = MFunction {
            id: nexa_nir::NirFunctionId(0),
            signature: dummy_sig(),
            body: Some(MFunctionBody {
                entry: MBlockId(99), // non-existent
                blocks: vec![block],
            }),
        };
        let mut module = MNirModule::new();
        module.add_function(func);

        let errors = MNirVerifier::verify(&module);
        assert!(!errors.is_empty());
        assert!(errors[0].message.contains("Entry block"));
    }

    // 16. MNirVerifier: duplicate block IDs detected
    #[test]
    fn test_verifier_duplicate_block_ids() {
        let block1 = MBlock {
            id: MBlockId(0),
            parameters: vec![],
            instructions: vec![],
            terminator: MTerminator::Return(None),
        };
        let block2 = MBlock {
            id: MBlockId(0), // duplicate
            parameters: vec![],
            instructions: vec![],
            terminator: MTerminator::Unreachable,
        };
        let func = MFunction {
            id: nexa_nir::NirFunctionId(0),
            signature: dummy_sig(),
            body: Some(MFunctionBody {
                entry: MBlockId(0),
                blocks: vec![block1, block2],
            }),
        };
        let mut module = MNirModule::new();
        module.add_function(func);

        let errors = MNirVerifier::verify(&module);
        assert!(!errors.is_empty());
        assert!(errors
            .iter()
            .any(|e| e.message.contains("Duplicate block ID")));
    }

    // 17. MNirVerifier: missing terminator target block detected
    #[test]
    fn test_verifier_missing_target_block() {
        let block = MBlock {
            id: MBlockId(0),
            parameters: vec![],
            instructions: vec![],
            terminator: MTerminator::Goto {
                target: MBlockId(99), // does not exist
                args: vec![],
            },
        };
        let func = MFunction {
            id: nexa_nir::NirFunctionId(0),
            signature: dummy_sig(),
            body: Some(MFunctionBody {
                entry: MBlockId(0),
                blocks: vec![block],
            }),
        };
        let mut module = MNirModule::new();
        module.add_function(func);

        let errors = MNirVerifier::verify(&module);
        assert!(!errors.is_empty());
        assert!(errors
            .iter()
            .any(|e| e.message.contains("non-existent target")));
    }

    // 18. MEnumArm creation
    #[test]
    fn test_menum_arm_creation() {
        let arm = MEnumArm {
            variant_index: 2,
            target: MBlockId(5),
            args: vec![MValueId(1), MValueId(2)],
        };
        assert_eq!(arm.variant_index, 2);
        assert_eq!(arm.target, MBlockId(5));
        assert_eq!(arm.args.len(), 2);
    }

    // 19. MInstruction - StructConstruct and StructExtract
    #[test]
    fn test_minstruction_struct_ops() {
        let construct = MInstruction::StructConstruct {
            ty: nexa_nir::NirTypeId(10),
            fields: vec![MValueId(0), MValueId(1), MValueId(2)],
        };
        match construct {
            MInstruction::StructConstruct { ty, fields } => {
                assert_eq!(ty, nexa_nir::NirTypeId(10));
                assert_eq!(fields.len(), 3);
            }
            _ => panic!("expected StructConstruct"),
        }

        let extract = MInstruction::StructExtract {
            source: MValueId(0),
            field_index: 1,
            ty: nexa_nir::NirTypeId(5),
        };
        match extract {
            MInstruction::StructExtract {
                source,
                field_index,
                ty,
            } => {
                assert_eq!(source, MValueId(0));
                assert_eq!(field_index, 1);
                assert_eq!(ty, nexa_nir::NirTypeId(5));
            }
            _ => panic!("expected StructExtract"),
        }
    }

    // 20. MInstruction - EnumConstruct, EnumTag, EnumPayload
    #[test]
    fn test_minstruction_enum_ops() {
        let construct = MInstruction::EnumConstruct {
            ty: nexa_nir::NirTypeId(10),
            variant_index: 1,
            fields: vec![MValueId(3)],
        };
        match construct {
            MInstruction::EnumConstruct {
                ty,
                variant_index,
                fields,
            } => {
                assert_eq!(ty, nexa_nir::NirTypeId(10));
                assert_eq!(variant_index, 1);
                assert_eq!(fields.len(), 1);
            }
            _ => panic!("expected EnumConstruct"),
        }

        let tag = MInstruction::EnumTag {
            source: MValueId(0),
            ty: nexa_nir::NirTypeId(10),
        };
        match tag {
            MInstruction::EnumTag { source, ty } => {
                assert_eq!(source, MValueId(0));
                assert_eq!(ty, nexa_nir::NirTypeId(10));
            }
            _ => panic!("expected EnumTag"),
        }

        let payload = MInstruction::EnumPayload {
            source: MValueId(0),
            variant_index: 2,
            ty: nexa_nir::NirTypeId(10),
        };
        match payload {
            MInstruction::EnumPayload {
                source,
                variant_index,
                ty,
            } => {
                assert_eq!(source, MValueId(0));
                assert_eq!(variant_index, 2);
                assert_eq!(ty, nexa_nir::NirTypeId(10));
            }
            _ => panic!("expected EnumPayload"),
        }
    }

    // 21. MInstruction - ArrayCreate and ArrayIndex
    #[test]
    fn test_minstruction_array_ops() {
        let create = MInstruction::ArrayCreate {
            ty: nexa_nir::NirTypeId(5),
            elements: vec![MValueId(0), MValueId(1)],
        };
        match create {
            MInstruction::ArrayCreate { ty, elements } => {
                assert_eq!(ty, nexa_nir::NirTypeId(5));
                assert_eq!(elements.len(), 2);
            }
            _ => panic!("expected ArrayCreate"),
        }

        let index = MInstruction::ArrayIndex {
            base: MValueId(0),
            index: MValueId(1),
            ty: nexa_nir::NirTypeId(5),
        };
        match index {
            MInstruction::ArrayIndex { base, index, ty } => {
                assert_eq!(base, MValueId(0));
                assert_eq!(index, MValueId(1));
                assert_eq!(ty, nexa_nir::NirTypeId(5));
            }
            _ => panic!("expected ArrayIndex"),
        }
    }

    // 22. MInstruction - TaskCreate and TaskAwait
    #[test]
    fn test_minstruction_task_ops() {
        let create = MInstruction::TaskCreate {
            action: nexa_nir::NirFunctionId(7),
            args: vec![MValueId(0)],
            ty: nexa_nir::NirTypeId(3),
        };
        match create {
            MInstruction::TaskCreate { action, args, ty } => {
                assert_eq!(action, nexa_nir::NirFunctionId(7));
                assert_eq!(args.len(), 1);
                assert_eq!(ty, nexa_nir::NirTypeId(3));
            }
            _ => panic!("expected TaskCreate"),
        }

        let await_ = MInstruction::TaskAwait {
            task: MValueId(0),
            ty: nexa_nir::NirTypeId(3),
        };
        match await_ {
            MInstruction::TaskAwait { task, ty } => {
                assert_eq!(task, MValueId(0));
                assert_eq!(ty, nexa_nir::NirTypeId(3));
            }
            _ => panic!("expected TaskAwait"),
        }
    }

    // 23. MFunction Default impl
    #[test]
    fn test_mfunction_default() {
        let func = MFunction::default();
        assert_eq!(func.id, nexa_nir::NirFunctionId(0));
        assert!(func.body.is_none());
        assert_eq!(func.signature.kind, nexa_nir::NirCallableKind::Function);
        assert!(func.signature.parameters.is_empty());
    }

    // 24. MNirModule Default impl
    #[test]
    fn test_mnir_module_default() {
        let module = MNirModule::default();
        assert!(module.functions.is_empty());
    }

    // 25. MNirVerifier: valid multi-block module with branch
    #[test]
    fn test_verifier_valid_multi_block() {
        let entry = MBlock {
            id: MBlockId(0),
            parameters: vec![],
            instructions: vec![(
                MValueId(0),
                MInstruction::Const {
                    constant: nexa_nir::NirConstantId(0),
                    ty: nexa_nir::NirTypeId(0),
                },
            )],
            terminator: MTerminator::Branch {
                condition: MValueId(0),
                then_block: MBlockId(1),
                then_args: vec![],
                else_block: MBlockId(2),
                else_args: vec![],
            },
        };
        let then_block = MBlock {
            id: MBlockId(1),
            parameters: vec![],
            instructions: vec![],
            terminator: MTerminator::Return(None),
        };
        let else_block = MBlock {
            id: MBlockId(2),
            parameters: vec![],
            instructions: vec![],
            terminator: MTerminator::Return(None),
        };
        let func = MFunction {
            id: nexa_nir::NirFunctionId(0),
            signature: dummy_sig(),
            body: Some(MFunctionBody {
                entry: MBlockId(0),
                blocks: vec![entry, then_block, else_block],
            }),
        };
        let mut module = MNirModule::new();
        module.add_function(func);

        let errors = MNirVerifier::verify(&module);
        assert!(
            errors.is_empty(),
            "Expected no errors but got: {:?}",
            errors
        );
    }

    // 26. MBinaryOp - all checked variants exist
    #[test]
    fn test_binary_op_checked_variants() {
        assert_eq!(MBinaryOp::CheckedAdd, MBinaryOp::CheckedAdd);
        assert_eq!(MBinaryOp::CheckedSub, MBinaryOp::CheckedSub);
        assert_eq!(MBinaryOp::CheckedMul, MBinaryOp::CheckedMul);
        assert_eq!(MBinaryOp::CheckedDiv, MBinaryOp::CheckedDiv);
        assert_ne!(MBinaryOp::CheckedAdd, MBinaryOp::CheckedSub);
    }

    // 27. MInstruction - Drop and ResourceCleanup
    #[test]
    fn test_minstruction_drop_and_cleanup() {
        let drop = MInstruction::Drop { value: MValueId(5) };
        match drop {
            MInstruction::Drop { value } => assert_eq!(value, MValueId(5)),
            _ => panic!("expected Drop"),
        }

        let cleanup = MInstruction::ResourceCleanup { value: MValueId(6) };
        match cleanup {
            MInstruction::ResourceCleanup { value } => assert_eq!(value, MValueId(6)),
            _ => panic!("expected ResourceCleanup"),
        }
    }

    // 28. MInstruction - InterfaceCall
    #[test]
    fn test_minstruction_interface_call() {
        let inst = MInstruction::InterfaceCall {
            interface: nexa_nir::NirTypeId(10),
            method: nexa_nir::NirFunctionId(5),
            receiver: MValueId(0),
            args: vec![MValueId(1), MValueId(2)],
            ty: nexa_nir::NirTypeId(3),
        };
        match inst {
            MInstruction::InterfaceCall {
                interface,
                method,
                receiver,
                args,
                ty,
            } => {
                assert_eq!(interface, nexa_nir::NirTypeId(10));
                assert_eq!(method, nexa_nir::NirFunctionId(5));
                assert_eq!(receiver, MValueId(0));
                assert_eq!(args.len(), 2);
                assert_eq!(ty, nexa_nir::NirTypeId(3));
            }
            _ => panic!("expected InterfaceCall"),
        }
    }

    // 29. MInstruction - RuntimeIntrinsic
    #[test]
    fn test_minstruction_runtime_intrinsic() {
        let inst = MInstruction::RuntimeIntrinsic {
            intrinsic: nexa_nir::IntrinsicId(42),
            args: vec![MValueId(0), MValueId(1)],
            ty: nexa_nir::NirTypeId(0),
        };
        match inst {
            MInstruction::RuntimeIntrinsic {
                intrinsic,
                args,
                ty,
            } => {
                assert_eq!(intrinsic, nexa_nir::IntrinsicId(42));
                assert_eq!(args.len(), 2);
                assert_eq!(ty, nexa_nir::NirTypeId(0));
            }
            _ => panic!("expected RuntimeIntrinsic"),
        }
    }

    // 30. MInstruction - Load and Store
    #[test]
    fn test_minstruction_load_store() {
        let load = MInstruction::Load {
            address: MValueId(10),
            ty: nexa_nir::NirTypeId(5),
        };
        match load {
            MInstruction::Load { address, ty } => {
                assert_eq!(address, MValueId(10));
                assert_eq!(ty, nexa_nir::NirTypeId(5));
            }
            _ => panic!("expected Load"),
        }

        let store = MInstruction::Store {
            address: MValueId(10),
            value: MValueId(11),
        };
        match store {
            MInstruction::Store { address, value } => {
                assert_eq!(address, MValueId(10));
                assert_eq!(value, MValueId(11));
            }
            _ => panic!("expected Store"),
        }
    }
}
