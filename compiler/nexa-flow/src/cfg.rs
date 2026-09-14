use nexa_source::SourceSpan;
use nexa_symbols::SymbolId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CfgId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BasicBlockId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LoopId(pub u32);

/// Reference to an expression in the typed AST (index into an expression store)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExprRef(pub u32);

#[derive(Debug, Clone)]
pub enum FlowOperation {
    /// Reference to a typed expression
    ExpressionUse(ExprRef),
    /// Local variable read
    ReadLocal { symbol: SymbolId, span: SourceSpan },
    /// Local variable write (assignment)
    WriteLocal(SymbolId),
    /// Function/action call
    Call {
        target: SymbolId,
        args: Vec<ExprRef>,
    },
    /// Await suspension point
    Await,
    /// Try point (for flow tracking)
    TryPoint,
    /// Assignment operation
    Assignment { target: SymbolId, value: ExprRef },
    /// Contract check (require/ensure)
    ContractCheck,
}

#[derive(Debug, Clone)]
pub enum FlowTerminator {
    /// Unconditional jump
    Goto(BasicBlockId),
    /// Conditional branch
    Branch {
        condition: ExprRef,
        then_block: BasicBlockId,
        else_block: BasicBlockId,
    },
    /// Match with scrutinee and arms
    Match {
        scrutinee: ExprRef,
        arms: Vec<BasicBlockId>,
    },
    /// Return from callable
    Return(Option<ExprRef>),
    /// Fim de corpo: cai para fora do callable sem retorno explícito.
    /// (Distingue do `Return(None)` que é um retorno de Unit explícito.)
    Fallthrough,
    /// Break to enclosing loop
    Break { target: BasicBlockId },
    /// Continue to loop header
    Continue { target: BasicBlockId },
    /// Code after this is unreachable
    Unreachable,
    /// Runtime trap/panic
    Trap,
}

#[derive(Debug, Clone)]
pub struct BasicBlock {
    pub id: BasicBlockId,
    pub operations: Vec<FlowOperation>,
    pub terminator: FlowTerminator,
    /// Span do statement inicial deste bloco (para posicionar diagnósticos
    /// como `UnreachableCode`).
    pub span: Option<SourceSpan>,
}

#[derive(Debug, Clone)]
pub struct ControlFlowGraph {
    pub id: CfgId,
    pub callable: SymbolId,
    pub entry: BasicBlockId,
    pub blocks: Vec<BasicBlock>,
    pub loop_stack: Vec<LoopId>,
}

impl ControlFlowGraph {
    pub fn new(id: CfgId, callable: SymbolId, entry: BasicBlockId) -> Self {
        Self {
            id,
            callable,
            entry,
            blocks: Vec::new(),
            loop_stack: Vec::new(),
        }
    }

    pub fn add_block(&mut self, block: BasicBlock) {
        self.blocks.push(block);
    }

    pub fn get_block(&self, id: BasicBlockId) -> Option<&BasicBlock> {
        self.blocks.iter().find(|b| b.id == id)
    }

    pub fn get_block_mut(&mut self, id: BasicBlockId) -> Option<&mut BasicBlock> {
        self.blocks.iter_mut().find(|b| b.id == id)
    }

    pub fn successors(&self, id: BasicBlockId) -> Vec<BasicBlockId> {
        let block = match self.get_block(id) {
            Some(b) => b,
            None => return Vec::new(),
        };
        match &block.terminator {
            FlowTerminator::Goto(target) => vec![*target],
            FlowTerminator::Branch {
                then_block,
                else_block,
                ..
            } => vec![*then_block, *else_block],
            FlowTerminator::Match { arms, .. } => arms.clone(),
            FlowTerminator::Break { target } => vec![*target],
            FlowTerminator::Continue { target } => vec![*target],
            FlowTerminator::Return(_)
            | FlowTerminator::Fallthrough
            | FlowTerminator::Unreachable
            | FlowTerminator::Trap => vec![],
        }
    }

    pub fn predecessors(&self, id: BasicBlockId) -> Vec<BasicBlockId> {
        let mut preds = Vec::new();
        for block in &self.blocks {
            if self.successors(block.id).contains(&id) {
                preds.push(block.id);
            }
        }
        preds
    }

    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    pub fn blocks(&self) -> &[BasicBlock] {
        &self.blocks
    }
}

impl Default for BasicBlock {
    fn default() -> Self {
        Self {
            id: BasicBlockId(0),
            operations: Vec::new(),
            terminator: FlowTerminator::Unreachable,
            span: None,
        }
    }
}

impl BasicBlock {
    /// Cria um bloco vazio (terminator `Unreachable` por padrão).
    pub fn new(id: BasicBlockId, span: Option<SourceSpan>) -> Self {
        Self {
            id,
            operations: Vec::new(),
            terminator: FlowTerminator::Unreachable,
            span,
        }
    }
}
