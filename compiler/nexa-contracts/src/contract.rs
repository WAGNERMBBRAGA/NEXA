use nexa_source::SourceSpan;
use nexa_types::TypeId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OldCaptureId(pub u32);

#[derive(Debug, Clone)]
pub struct ContractExpression {
    pub span: SourceSpan,
    pub ty: TypeId,
}

#[derive(Debug, Clone)]
pub struct OldCapture {
    pub id: OldCaptureId,
    pub expression_span: SourceSpan,
    pub ty: TypeId,
}

#[derive(Debug, Clone)]
pub struct ContractInfo {
    pub requires: Vec<ContractExpression>,
    pub ensures: Vec<ContractExpression>,
    pub old_captures: Vec<OldCapture>,
    pub next_old_capture_id: u32,
}

impl ContractInfo {
    pub fn new() -> Self {
        Self {
            requires: Vec::new(),
            ensures: Vec::new(),
            old_captures: Vec::new(),
            next_old_capture_id: 0,
        }
    }

    pub fn add_require(&mut self, span: SourceSpan, ty: TypeId) {
        self.requires.push(ContractExpression { span, ty });
    }

    pub fn add_ensure(&mut self, span: SourceSpan, ty: TypeId) {
        self.ensures.push(ContractExpression { span, ty });
    }

    pub fn add_old_capture(&mut self, expression_span: SourceSpan, ty: TypeId) -> OldCaptureId {
        let id = OldCaptureId(self.next_old_capture_id);
        self.next_old_capture_id += 1;
        self.old_captures.push(OldCapture {
            id,
            expression_span,
            ty,
        });
        id
    }

    pub fn has_contracts(&self) -> bool {
        !self.requires.is_empty() || !self.ensures.is_empty()
    }

    pub fn has_ensure(&self) -> bool {
        !self.ensures.is_empty()
    }

    pub fn has_require(&self) -> bool {
        !self.requires.is_empty()
    }
}

impl Default for ContractInfo {
    fn default() -> Self {
        Self::new()
    }
}
