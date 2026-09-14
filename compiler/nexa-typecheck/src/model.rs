use nexa_source::SourceSpan;
use nexa_symbols::SymbolId;
use nexa_types::id::TypeId;
use nexa_types::ty::*;
use std::collections::HashMap;

/// Expression type info stored in side table.
#[derive(Debug, Clone)]
pub struct ExprInfo {
    pub ty: TypeId,
    pub category: ValueCategory,
    pub resolved_symbol: Option<SymbolId>,
    pub validity: SemanticValidity,
}

/// Info about a declaration's type.
#[derive(Debug, Clone)]
pub struct DeclarationTypeInfo {
    pub symbol: SymbolId,
    pub type_id: TypeId,
    pub callable_signature: Option<CallableType>,
}

/// Side table mapping expressions to their type info.
pub struct ExprTypeMap {
    map: HashMap<(u32, u32), ExprInfo>, // (start, end) → ExprInfo
}

impl ExprTypeMap {
    pub fn new() -> Self {
        ExprTypeMap {
            map: HashMap::new(),
        }
    }

    pub fn insert(&mut self, span: SourceSpan, info: ExprInfo) {
        self.map.insert((span.start, span.end), info);
    }

    pub fn get(&self, span: SourceSpan) -> Option<&ExprInfo> {
        self.map.get(&(span.start, span.end))
    }

    pub fn count(&self) -> usize {
        self.map.len()
    }
}

impl Default for ExprTypeMap {
    fn default() -> Self {
        Self::new()
    }
}

/// Side table mapping declarations to their types.
pub struct DeclarationTypeMap {
    map: HashMap<SymbolId, DeclarationTypeInfo>,
}

impl DeclarationTypeMap {
    pub fn new() -> Self {
        DeclarationTypeMap {
            map: HashMap::new(),
        }
    }

    pub fn insert(&mut self, info: DeclarationTypeInfo) {
        self.map.insert(info.symbol, info);
    }

    pub fn get(&self, symbol: SymbolId) -> Option<&DeclarationTypeInfo> {
        self.map.get(&symbol)
    }

    pub fn get_mut(&mut self, symbol: SymbolId) -> Option<&mut DeclarationTypeInfo> {
        self.map.get_mut(&symbol)
    }

    pub fn count(&self) -> usize {
        self.map.len()
    }
}

impl Default for DeclarationTypeMap {
    fn default() -> Self {
        Self::new()
    }
}

/// Side table mapping callables to their resolved signatures.
pub struct CallableSignatureMap {
    map: HashMap<SymbolId, CallableType>,
}

impl CallableSignatureMap {
    pub fn new() -> Self {
        CallableSignatureMap {
            map: HashMap::new(),
        }
    }

    pub fn insert(&mut self, symbol: SymbolId, signature: CallableType) {
        self.map.insert(symbol, signature);
    }

    pub fn get(&self, symbol: SymbolId) -> Option<&CallableType> {
        self.map.get(&symbol)
    }

    pub fn count(&self) -> usize {
        self.map.len()
    }
}

impl Default for CallableSignatureMap {
    fn default() -> Self {
        Self::new()
    }
}

/// Side table mapping member accesses to their resolution.
pub struct MemberResolutionMap {
    map: HashMap<(SymbolId, String), MemberResolution>,
}

impl MemberResolutionMap {
    pub fn new() -> Self {
        MemberResolutionMap {
            map: HashMap::new(),
        }
    }

    pub fn insert(&mut self, receiver: SymbolId, member: String, resolution: MemberResolution) {
        self.map.insert((receiver, member), resolution);
    }

    pub fn get(&mut self, receiver: SymbolId, member: &str) -> Option<&MemberResolution> {
        self.map.get(&(receiver, member.to_string()))
    }

    pub fn count(&self) -> usize {
        self.map.len()
    }
}

impl Default for MemberResolutionMap {
    fn default() -> Self {
        Self::new()
    }
}

/// The typed semantic model produced by the type checker.
pub struct TypedSemanticModel {
    pub expression_info: ExprTypeMap,
    pub declaration_types: DeclarationTypeMap,
    pub callable_signatures: CallableSignatureMap,
    pub member_resolutions: MemberResolutionMap,
}

impl TypedSemanticModel {
    pub fn new() -> Self {
        TypedSemanticModel {
            expression_info: ExprTypeMap::new(),
            declaration_types: DeclarationTypeMap::new(),
            callable_signatures: CallableSignatureMap::new(),
            member_resolutions: MemberResolutionMap::new(),
        }
    }

    pub fn summary(&self) -> String {
        format!(
            "TypedSemanticModel: {} expressions, {} declarations, {} signatures, {} member resolutions",
            self.expression_info.count(),
            self.declaration_types.count(),
            self.callable_signatures.count(),
            self.member_resolutions.count(),
        )
    }
}

impl Default for TypedSemanticModel {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of the full type checking pass.
pub struct TypeCheckResult {
    pub semantic: TypedSemanticModel,
    pub diagnostics: Vec<crate::diagnostics::TypeDiagnostic>,
}
