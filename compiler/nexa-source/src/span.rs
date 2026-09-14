//! `SourceSpan` — região exata de um source.

use crate::id::SourceId;
use serde::Serialize;

/// Região exata `[start, end)` de um arquivo de source.
///
/// - `start`/`end` são **byte offsets UTF-8**;
/// - `start <= end` (spans zero-width são permitidos, ex.: EOF);
/// - `end` é exclusivo;
/// - linha/coluna nunca são armazenadas aqui (são derivadas).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct SourceSpan {
    pub source: SourceId,
    pub start: u32,
    pub end: u32,
}

impl SourceSpan {
    pub fn new(source: SourceId, start: u32, end: u32) -> Self {
        debug_assert!(start <= end, "SourceSpan: start > end");
        Self { source, start, end }
    }

    /// Span zero-width em `offset` (ex.: EOF).
    pub fn point(source: SourceId, offset: u32) -> Self {
        Self {
            source,
            start: offset,
            end: offset,
        }
    }

    pub fn is_zero_width(&self) -> bool {
        self.start == self.end
    }

    /// Union contígua de dois spans do **mesmo** source.
    /// Retorna `None` se os sources diferirem ou se os spans não forem
    /// adjacentes/sobrepostos.
    pub fn cover(a: SourceSpan, b: SourceSpan) -> Option<SourceSpan> {
        if a.source != b.source {
            return None;
        }
        if a.end < b.start || b.end < a.start {
            return None;
        }
        Some(SourceSpan::new(
            a.source,
            a.start.min(b.start),
            a.end.max(b.end),
        ))
    }
}
