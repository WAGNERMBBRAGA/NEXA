use serde::Serialize;
use std::fmt;

/// Session-local identity of a semantic type representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct TypeId(pub u32);

impl fmt::Display for TypeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "type#{}", self.0)
    }
}

/// Index into the nominal type metadata table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct NominalTypeId(pub u32);

impl fmt::Display for NominalTypeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "nominal#{}", self.0)
    }
}

/// Unique per-declaration generic parameter identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct GenericParamId(pub u32);

impl fmt::Display for GenericParamId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "generic#{}", self.0)
    }
}

/// References an exact interface implementation record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct ImplementationId(pub u32);

impl fmt::Display for ImplementationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "impl#{}", self.0)
    }
}

/// Sentinel: invalid/unresolved type.
pub const INVALID_TYPE: TypeId = TypeId(u32::MAX);
