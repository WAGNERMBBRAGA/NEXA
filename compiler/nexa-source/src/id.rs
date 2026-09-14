//! `SourceId` — identidade interna de uma source unit.
//!
//! O path pode mudar; `SourceId` identifica a source unit durante a
//! compilation session.

use serde::Serialize;
use std::fmt;

/// Identidade interna e estável de uma unidade de source dentro de uma
/// sessão de compilação.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct SourceId(pub u32);

impl fmt::Display for SourceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "source#{}", self.0)
    }
}
