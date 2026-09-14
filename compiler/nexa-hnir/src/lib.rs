#![allow(dead_code)]

use nexa_nir::{IntrinsicId, NirCallableSignature, NirConstantId, NirFunctionId, NirTypeId};
use nexa_symbols::SymbolId;

// ── Core IDs ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HValueId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HPlaceId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HBlockId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HFunctionId(pub u32);

// ── HPlace ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HPlace {
    Local { symbol: SymbolId },
    Parameter { index: u32 },
    Field { base: HPlaceId, field: String },
    Index { base: HPlaceId, index: HValueId },
    Deref { base: HPlaceId },
}

// ── HArgument ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct HArgument {
    pub value: HValueId,
    pub passing: nexa_nir::NirPassingMode,
}

// ── HContractKind ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HContractKind {
    Require,
    Ensure,
}

// ── HInstruction ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum HInstruction {
    Const {
        constant: NirConstantId,
        ty: NirTypeId,
    },
    Copy {
        source: HValueId,
        ty: NirTypeId,
    },
    Move {
        source: HValueId,
        ty: NirTypeId,
    },
    BorrowShared {
        place: HPlaceId,
        ty: NirTypeId,
    },
    BorrowMutable {
        place: HPlaceId,
        ty: NirTypeId,
    },
    Load {
        place: HPlaceId,
        ty: NirTypeId,
    },
    Store {
        place: HPlaceId,
        value: HValueId,
    },
    Call {
        target: NirFunctionId,
        args: Vec<HArgument>,
        ty: NirTypeId,
    },
    InterfaceCall {
        interface: NirTypeId,
        method: NirFunctionId,
        receiver: HValueId,
        args: Vec<HArgument>,
        ty: NirTypeId,
    },
    ConstructStruct {
        ty: NirTypeId,
        fields: Vec<HValueId>,
    },
    ConstructEnum {
        ty: NirTypeId,
        variant_index: u32,
        fields: Vec<HValueId>,
    },
    ExtractField {
        source: HValueId,
        field_index: u32,
        ty: NirTypeId,
    },
    ProjectField {
        place: HPlaceId,
        field: String,
        ty: NirTypeId,
    },
    Index {
        base: HValueId,
        index: HValueId,
        ty: NirTypeId,
    },
    Await {
        task: HValueId,
        ty: NirTypeId,
    },
    TryUnwrap {
        source: HValueId,
        success_value: HValueId,
        ty: NirTypeId,
    },
    Drop {
        value: HValueId,
    },
    ResourceCleanup {
        value: HValueId,
    },
    ContractCheck {
        kind: HContractKind,
        condition: HValueId,
    },
    RuntimeIntrinsic {
        intrinsic: IntrinsicId,
        args: Vec<HValueId>,
        ty: NirTypeId,
    },
    TaskCreate {
        action: NirFunctionId,
        args: Vec<HArgument>,
        ty: NirTypeId,
    },
    TaskAwait {
        task: HValueId,
        ty: NirTypeId,
    },
    TaskTransfer {
        task: HValueId,
    },
}

// ── HTerminator ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum HTerminator {
    Goto(HBlockId),
    Branch {
        condition: HValueId,
        then_block: HBlockId,
        else_block: HBlockId,
    },
    SwitchEnum {
        value: HValueId,
        arms: Vec<HEnumArm>,
        default: Option<HBlockId>,
    },
    Return(Option<HValueId>),
    Panic(HValueId),
    Unreachable,
}

#[derive(Debug, Clone)]
pub struct HEnumArm {
    pub variant_index: u32,
    pub target: HBlockId,
    pub bindings: Vec<HValueId>,
}

// ── HBlock ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct HBlock {
    pub id: HBlockId,
    pub instructions: Vec<(HValueId, HInstruction)>,
    pub terminator: HTerminator,
}

// ── HFunction ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct HFunction {
    pub id: HFunctionId,
    pub signature: NirCallableSignature,
    pub places: Vec<(HPlaceId, HPlace)>,
    pub blocks: Vec<HBlock>,
    pub entry: HBlockId,
}

// ── HNIR Module ─────────────────────────────────────────────────────────

pub struct HNirModule {
    pub functions: Vec<HFunction>,
}

impl Default for HNirModule {
    fn default() -> Self {
        Self::new()
    }
}

impl HNirModule {
    pub fn new() -> Self {
        Self {
            functions: Vec::new(),
        }
    }

    pub fn add_function(&mut self, f: HFunction) -> HFunctionId {
        let id = HFunctionId(self.functions.len() as u32);
        self.functions.push(f);
        id
    }
}

// ── HNIR Verifier ───────────────────────────────────────────────────────

pub struct HNirVerifier;

#[derive(Debug, Clone)]
pub struct HNirError {
    pub message: String,
}

impl HNirVerifier {
    pub fn verify(module: &HNirModule) -> Vec<HNirError> {
        let mut errors = Vec::new();

        for func in &module.functions {
            // Check: entry block exists
            if !func.blocks.iter().any(|b| b.id == func.entry) {
                errors.push(HNirError {
                    message: format!(
                        "function {:?}: entry block {:?} not found",
                        func.id, func.entry
                    ),
                });
            }

            // Check: all block IDs unique per function
            let mut seen_blocks = std::collections::HashSet::new();
            for block in &func.blocks {
                if !seen_blocks.insert(block.id) {
                    errors.push(HNirError {
                        message: format!(
                            "function {:?}: duplicate block ID {:?}",
                            func.id, block.id
                        ),
                    });
                }
            }

            // Check: all value IDs unique per function
            let mut seen_values = std::collections::HashSet::new();
            for block in &func.blocks {
                for (val_id, _) in &block.instructions {
                    if !seen_values.insert(*val_id) {
                        errors.push(HNirError {
                            message: format!(
                                "function {:?}: duplicate value ID {:?}",
                                func.id, val_id
                            ),
                        });
                    }
                }
            }

            // Check: all terminator targets exist
            let block_ids: std::collections::HashSet<HBlockId> =
                func.blocks.iter().map(|b| b.id).collect();
            for block in &func.blocks {
                Self::check_terminator_targets(func.id, &block.terminator, &block_ids, &mut errors);
            }
        }

        errors
    }

    fn check_terminator_targets(
        func_id: HFunctionId,
        term: &HTerminator,
        block_ids: &std::collections::HashSet<HBlockId>,
        errors: &mut Vec<HNirError>,
    ) {
        match term {
            HTerminator::Goto(target) => {
                if !block_ids.contains(target) {
                    errors.push(HNirError {
                        message: format!(
                            "function {:?}: terminator targets non-existent block {:?}",
                            func_id, target
                        ),
                    });
                }
            }
            HTerminator::Branch {
                then_block,
                else_block,
                ..
            } => {
                if !block_ids.contains(then_block) {
                    errors.push(HNirError {
                        message: format!(
                            "function {:?}: branch then-target non-existent block {:?}",
                            func_id, then_block
                        ),
                    });
                }
                if !block_ids.contains(else_block) {
                    errors.push(HNirError {
                        message: format!(
                            "function {:?}: branch else-target non-existent block {:?}",
                            func_id, else_block
                        ),
                    });
                }
            }
            HTerminator::SwitchEnum { arms, default, .. } => {
                for arm in arms {
                    if !block_ids.contains(&arm.target) {
                        errors.push(HNirError {
                            message: format!(
                                "function {:?}: switch arm targets non-existent block {:?}",
                                func_id, arm.target
                            ),
                        });
                    }
                }
                if let Some(def) = default {
                    if !block_ids.contains(def) {
                        errors.push(HNirError {
                            message: format!(
                                "function {:?}: switch default targets non-existent block {:?}",
                                func_id, def
                            ),
                        });
                    }
                }
            }
            HTerminator::Return(_) | HTerminator::Panic(_) | HTerminator::Unreachable => {}
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use nexa_nir::{NirCallableKind, NirEffectSet, NirParameter, NirPassingMode};

    #[test]
    fn test_hvalue_id_creation_and_equality() {
        let a = HValueId(0);
        let b = HValueId(0);
        let c = HValueId(1);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn test_hplace_id_creation_and_equality() {
        let a = HPlaceId(0);
        let b = HPlaceId(0);
        let c = HPlaceId(5);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn test_hblock_id_creation_and_equality() {
        let a = HBlockId(0);
        let b = HBlockId(0);
        let c = HBlockId(3);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn test_hfunction_id_creation_and_equality() {
        let a = HFunctionId(0);
        let b = HFunctionId(0);
        let c = HFunctionId(2);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn test_hplace_local() {
        let p = HPlace::Local {
            symbol: SymbolId(42),
        };
        match &p {
            HPlace::Local { symbol } => assert_eq!(symbol.0, 42),
            _ => panic!("expected Local"),
        }
    }

    #[test]
    fn test_hplace_parameter() {
        let p = HPlace::Parameter { index: 3 };
        match &p {
            HPlace::Parameter { index } => assert_eq!(*index, 3),
            _ => panic!("expected Parameter"),
        }
    }

    #[test]
    fn test_hplace_field() {
        let p = HPlace::Field {
            base: HPlaceId(0),
            field: "x".to_string(),
        };
        match &p {
            HPlace::Field { base, field } => {
                assert_eq!(*base, HPlaceId(0));
                assert_eq!(field, "x");
            }
            _ => panic!("expected Field"),
        }
    }

    #[test]
    fn test_hplace_index() {
        let p = HPlace::Index {
            base: HPlaceId(1),
            index: HValueId(7),
        };
        match &p {
            HPlace::Index { base, index } => {
                assert_eq!(*base, HPlaceId(1));
                assert_eq!(*index, HValueId(7));
            }
            _ => panic!("expected Index"),
        }
    }

    #[test]
    fn test_hplace_deref() {
        let p = HPlace::Deref { base: HPlaceId(2) };
        match &p {
            HPlace::Deref { base } => assert_eq!(*base, HPlaceId(2)),
            _ => panic!("expected Deref"),
        }
    }

    #[test]
    fn test_hinstruction_const() {
        let inst = HInstruction::Const {
            constant: nexa_nir::NirConstantId(5),
            ty: NirTypeId(1),
        };
        match &inst {
            HInstruction::Const { constant, ty } => {
                assert_eq!(constant.0, 5);
                assert_eq!(ty.0, 1);
            }
            _ => panic!("expected Const"),
        }
    }

    #[test]
    fn test_hinstruction_copy_and_move() {
        let copy = HInstruction::Copy {
            source: HValueId(10),
            ty: NirTypeId(2),
        };
        let m = HInstruction::Move {
            source: HValueId(11),
            ty: NirTypeId(3),
        };
        match &copy {
            HInstruction::Copy { source, ty } => {
                assert_eq!(*source, HValueId(10));
                assert_eq!(ty.0, 2);
            }
            _ => panic!("expected Copy"),
        }
        match &m {
            HInstruction::Move { source, ty } => {
                assert_eq!(*source, HValueId(11));
                assert_eq!(ty.0, 3);
            }
            _ => panic!("expected Move"),
        }
    }

    #[test]
    fn test_hinstruction_borrow_shared_and_mutable() {
        let shared = HInstruction::BorrowShared {
            place: HPlaceId(0),
            ty: NirTypeId(4),
        };
        let mutable = HInstruction::BorrowMutable {
            place: HPlaceId(1),
            ty: NirTypeId(5),
        };
        match &shared {
            HInstruction::BorrowShared { place, ty } => {
                assert_eq!(*place, HPlaceId(0));
                assert_eq!(ty.0, 4);
            }
            _ => panic!("expected BorrowShared"),
        }
        match &mutable {
            HInstruction::BorrowMutable { place, ty } => {
                assert_eq!(*place, HPlaceId(1));
                assert_eq!(ty.0, 5);
            }
            _ => panic!("expected BorrowMutable"),
        }
    }

    #[test]
    fn test_hterminator_variants() {
        let goto = HTerminator::Goto(HBlockId(1));
        let branch = HTerminator::Branch {
            condition: HValueId(0),
            then_block: HBlockId(2),
            else_block: HBlockId(3),
        };
        let ret = HTerminator::Return(Some(HValueId(5)));
        let ret_none = HTerminator::Return(None);
        let unreachable = HTerminator::Unreachable;

        match &goto {
            HTerminator::Goto(t) => assert_eq!(*t, HBlockId(1)),
            _ => panic!("expected Goto"),
        }
        match &branch {
            HTerminator::Branch {
                condition,
                then_block,
                else_block,
            } => {
                assert_eq!(*condition, HValueId(0));
                assert_eq!(*then_block, HBlockId(2));
                assert_eq!(*else_block, HBlockId(3));
            }
            _ => panic!("expected Branch"),
        }
        match &ret {
            HTerminator::Return(Some(v)) => assert_eq!(*v, HValueId(5)),
            _ => panic!("expected Return(Some)"),
        }
        match &ret_none {
            HTerminator::Return(None) => {}
            _ => panic!("expected Return(None)"),
        }
        match &unreachable {
            HTerminator::Unreachable => {}
            _ => panic!("expected Unreachable"),
        }
    }

    #[test]
    fn test_hcontract_kind_variants() {
        assert_eq!(HContractKind::Require, HContractKind::Require);
        assert_eq!(HContractKind::Ensure, HContractKind::Ensure);
        assert_ne!(HContractKind::Require, HContractKind::Ensure);
    }

    #[test]
    fn test_hnir_module_new_and_add_function() {
        let mut module = HNirModule::new();
        assert!(module.functions.is_empty());

        let sig = NirCallableSignature {
            kind: NirCallableKind::Function,
            parameters: vec![],
            return_type: NirTypeId(0),
            effects: NirEffectSet::empty(),
        };
        let block = HBlock {
            id: HBlockId(0),
            instructions: vec![],
            terminator: HTerminator::Return(None),
        };
        let func = HFunction {
            id: HFunctionId(0),
            signature: sig,
            places: vec![],
            blocks: vec![block],
            entry: HBlockId(0),
        };
        let fid = module.add_function(func);
        assert_eq!(fid, HFunctionId(0));
        assert_eq!(module.functions.len(), 1);
    }

    #[test]
    fn test_hblock_creation() {
        let block = HBlock {
            id: HBlockId(5),
            instructions: vec![(HValueId(0), HInstruction::Drop { value: HValueId(0) })],
            terminator: HTerminator::Return(Some(HValueId(0))),
        };
        assert_eq!(block.id, HBlockId(5));
        assert_eq!(block.instructions.len(), 1);
    }

    #[test]
    fn test_hfunction_creation() {
        let sig = NirCallableSignature {
            kind: NirCallableKind::Function,
            parameters: vec![NirParameter {
                ty: NirTypeId(0),
                passing: NirPassingMode::Owned,
            }],
            return_type: NirTypeId(1),
            effects: NirEffectSet::empty(),
        };
        let func = HFunction {
            id: HFunctionId(7),
            signature: sig,
            places: vec![(HPlaceId(0), HPlace::Parameter { index: 0 })],
            blocks: vec![HBlock {
                id: HBlockId(0),
                instructions: vec![],
                terminator: HTerminator::Return(None),
            }],
            entry: HBlockId(0),
        };
        assert_eq!(func.id, HFunctionId(7));
        assert_eq!(func.signature.parameters.len(), 1);
        assert_eq!(func.places.len(), 1);
    }

    #[test]
    fn test_hargument_creation() {
        let arg = HArgument {
            value: HValueId(42),
            passing: NirPassingMode::Ref,
        };
        assert_eq!(arg.value, HValueId(42));
        assert_eq!(arg.passing, NirPassingMode::Ref);
    }

    #[test]
    fn test_henum_arm_creation() {
        let arm = HEnumArm {
            variant_index: 2,
            target: HBlockId(5),
            bindings: vec![HValueId(10), HValueId(11)],
        };
        assert_eq!(arm.variant_index, 2);
        assert_eq!(arm.target, HBlockId(5));
        assert_eq!(arm.bindings.len(), 2);
    }

    #[test]
    fn test_verifier_valid_module_passes() {
        let mut module = HNirModule::new();
        let sig = NirCallableSignature {
            kind: NirCallableKind::Function,
            parameters: vec![],
            return_type: NirTypeId(0),
            effects: NirEffectSet::empty(),
        };
        let func = HFunction {
            id: HFunctionId(0),
            signature: sig,
            places: vec![],
            blocks: vec![HBlock {
                id: HBlockId(0),
                instructions: vec![],
                terminator: HTerminator::Return(None),
            }],
            entry: HBlockId(0),
        };
        module.add_function(func);

        let errors = HNirVerifier::verify(&module);
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
    }

    #[test]
    fn test_verifier_duplicate_block_ids_detected() {
        let mut module = HNirModule::new();
        let sig = NirCallableSignature {
            kind: NirCallableKind::Function,
            parameters: vec![],
            return_type: NirTypeId(0),
            effects: NirEffectSet::empty(),
        };
        let func = HFunction {
            id: HFunctionId(0),
            signature: sig,
            places: vec![],
            blocks: vec![
                HBlock {
                    id: HBlockId(0),
                    instructions: vec![],
                    terminator: HTerminator::Return(None),
                },
                HBlock {
                    id: HBlockId(0),
                    instructions: vec![],
                    terminator: HTerminator::Return(None),
                },
            ],
            entry: HBlockId(0),
        };
        module.add_function(func);

        let errors = HNirVerifier::verify(&module);
        assert!(!errors.is_empty());
        assert!(errors
            .iter()
            .any(|e| e.message.contains("duplicate block ID")));
    }

    #[test]
    fn test_verifier_missing_terminator_target_detected() {
        let mut module = HNirModule::new();
        let sig = NirCallableSignature {
            kind: NirCallableKind::Function,
            parameters: vec![],
            return_type: NirTypeId(0),
            effects: NirEffectSet::empty(),
        };
        let func = HFunction {
            id: HFunctionId(0),
            signature: sig,
            places: vec![],
            blocks: vec![HBlock {
                id: HBlockId(0),
                instructions: vec![],
                terminator: HTerminator::Goto(HBlockId(99)),
            }],
            entry: HBlockId(0),
        };
        module.add_function(func);

        let errors = HNirVerifier::verify(&module);
        assert!(!errors.is_empty());
        assert!(errors
            .iter()
            .any(|e| e.message.contains("non-existent block")));
    }

    #[test]
    fn test_verifier_entry_block_must_exist() {
        let mut module = HNirModule::new();
        let sig = NirCallableSignature {
            kind: NirCallableKind::Function,
            parameters: vec![],
            return_type: NirTypeId(0),
            effects: NirEffectSet::empty(),
        };
        let func = HFunction {
            id: HFunctionId(0),
            signature: sig,
            places: vec![],
            blocks: vec![HBlock {
                id: HBlockId(0),
                instructions: vec![],
                terminator: HTerminator::Return(None),
            }],
            entry: HBlockId(5),
        };
        module.add_function(func);

        let errors = HNirVerifier::verify(&module);
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.message.contains("entry block")));
    }

    #[test]
    fn test_verifier_duplicate_value_ids_detected() {
        let mut module = HNirModule::new();
        let sig = NirCallableSignature {
            kind: NirCallableKind::Function,
            parameters: vec![],
            return_type: NirTypeId(0),
            effects: NirEffectSet::empty(),
        };
        let func = HFunction {
            id: HFunctionId(0),
            signature: sig,
            places: vec![],
            blocks: vec![HBlock {
                id: HBlockId(0),
                instructions: vec![
                    (HValueId(0), HInstruction::Drop { value: HValueId(0) }),
                    (HValueId(0), HInstruction::Drop { value: HValueId(0) }),
                ],
                terminator: HTerminator::Return(None),
            }],
            entry: HBlockId(0),
        };
        module.add_function(func);

        let errors = HNirVerifier::verify(&module);
        assert!(!errors.is_empty());
        assert!(errors
            .iter()
            .any(|e| e.message.contains("duplicate value ID")));
    }
}
