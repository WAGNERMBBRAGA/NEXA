use crate::id::{GenericParamId, ImplementationId, NominalTypeId, TypeId};
use nexa_symbols::{ModuleId, PackageInstanceId, SymbolId};

/// The core type representation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    Error,
    Unit,
    Never,
    Bool,
    Int,
    UInt,
    Int8,
    Int16,
    Int32,
    Int64,
    UInt8,
    UInt16,
    UInt32,
    UInt64,
    Float32,
    Float64,
    Byte,
    Char,
    String,
    Bytes,
    Nominal(NominalTypeId),
    Ref(TypeId),
    MutRef(TypeId),
    Array(TypeId),
    GenericParameter(GenericParamId),
    Applied {
        base: TypeId,
        arguments: Vec<TypeId>,
    },
    Callable(CallableType),
    Task(TypeId),
}

/// Metadata for a nominal type (struct, enum, interface, distinct).
#[derive(Debug, Clone)]
pub struct NominalType {
    pub symbol: SymbolId,
    pub kind: NominalTypeKind,
    pub generic_params: Vec<GenericParamId>,
    pub package: PackageInstanceId,
    pub module: ModuleId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NominalTypeKind {
    Struct,
    Enum,
    Interface,
    Distinct,
}

/// Representation of a generic parameter.
#[derive(Debug, Clone)]
pub struct GenericParameterInfo {
    pub symbol: SymbolId,
    pub constraints: Vec<Constraint>,
}

/// A single type constraint on a generic parameter.
#[derive(Debug, Clone)]
pub struct Constraint {
    pub parameter: GenericParamId,
    pub interface: TypeId,
}

/// Type of a callable (function/action/async action).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CallableType {
    pub kind: CallableKind,
    pub parameters: Vec<TypeId>,
    pub return_type: TypeId,
    pub generic_params: Vec<GenericParamId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CallableKind {
    Function,
    Action,
    AsyncAction,
}

/// Value category for expressions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ValueCategory {
    Value,
    Place,
}

/// Mutability of a place expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlaceMutability {
    Immutable,
    Mutable,
}

/// Information about a place expression.
#[derive(Debug, Clone)]
pub struct PlaceInfo {
    pub ty: TypeId,
    pub mutability: PlaceMutability,
}

/// Semantic validity of an expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SemanticValidity {
    Valid,
    Poisoned,
}

/// Type coercion that may be applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Coercion {
    None,
    NeverToAny,
    InterfaceUpcast { implementation: ImplementationId },
    LiteralToInteger(TypeId),
    LiteralToFloat(TypeId),
}

/// How a call was resolved.
#[derive(Debug, Clone)]
pub enum CallResolution {
    Direct(SymbolId),
    InherentMethod {
        method: SymbolId,
    },
    InterfaceMethod {
        interface: TypeId,
        method: SymbolId,
        implementation: Option<ImplementationId>,
    },
    CallableValue,
    VariantConstructor(SymbolId),
}

/// How a member access was resolved.
#[derive(Debug, Clone)]
pub enum MemberResolution {
    Field(SymbolId),
    Method(CallTargetCandidate),
}

/// A candidate for method call resolution.
#[derive(Debug, Clone)]
pub struct CallTargetCandidate {
    pub symbol: SymbolId,
    pub receiver_type: TypeId,
    pub kind: CallableKind,
}

/// Type definition for downstream compilation without AST.
#[derive(Debug, Clone)]
pub enum TypeDefinition {
    Struct(StructTypeDefinition),
    Enum(EnumTypeDefinition),
    Interface(InterfaceDefinition),
    Distinct(DistinctTypeDefinition),
}

#[derive(Debug, Clone)]
pub struct StructTypeDefinition {
    pub name: String,
    pub ty: TypeId,
    pub visibility: nexa_symbols::Visibility,
    pub symbol: SymbolId,
    pub fields: Vec<FieldDefinition>,
}

#[derive(Debug, Clone)]
pub struct FieldDefinition {
    pub name: String,
    pub ty: TypeId,
    pub exported: bool,
    pub symbol: SymbolId,
}

#[derive(Debug, Clone)]
pub struct EnumTypeDefinition {
    pub name: String,
    pub ty: TypeId,
    pub visibility: nexa_symbols::Visibility,
    pub symbol: SymbolId,
    pub variants: Vec<VariantDefinition>,
}

#[derive(Debug, Clone)]
pub struct VariantDefinition {
    pub name: String,
    pub symbol: SymbolId,
    pub kind: VariantKind,
}

#[derive(Debug, Clone)]
pub enum VariantKind {
    Unit,
    Tuple(Vec<TypeId>),
    Struct(Vec<FieldDefinition>),
}

#[derive(Debug, Clone)]
pub struct InterfaceDefinition {
    pub name: String,
    pub ty: TypeId,
    pub visibility: nexa_symbols::Visibility,
    pub symbol: SymbolId,
    pub members: Vec<InterfaceMemberSignature>,
}

#[derive(Debug, Clone)]
pub struct InterfaceMemberSignature {
    pub symbol: SymbolId,
    pub callable: CallableType,
}

#[derive(Debug, Clone)]
pub struct DistinctTypeDefinition {
    pub name: String,
    pub ty: TypeId,
    pub base: TypeId,
    pub visibility: nexa_symbols::Visibility,
    pub symbol: SymbolId,
}

/// An interface implementation record.
#[derive(Debug, Clone)]
pub struct Implementation {
    pub id: ImplementationId,
    pub interface: Option<TypeId>,
    pub target: TypeId,
    pub generic_params: Vec<GenericParamId>,
    pub constraints: Vec<Constraint>,
    pub members: Vec<SymbolId>,
}

/// Alias resolution state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AliasResolutionState {
    Unresolved,
    Resolving,
    Resolved,
    Error,
}

/// Type store key for interning composite types.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeKey {
    Ref(TypeId),
    MutRef(TypeId),
    Array(TypeId),
    Task(TypeId),
    Applied { base: TypeId, args: Vec<TypeId> },
    Callable(CallableType),
    GenericParameter(GenericParamId),
}
