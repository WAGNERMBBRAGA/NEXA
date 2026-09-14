//! NEXA — Source Model
//!
//! Tipos fundamentais que representam código-fonte NEXA carregado:
//! `SourceId`, `SourceSpan`, `SourceLocation`, `SourceFile` e `SourceManager`.
//!
//! Regras (Implementação 01):
//! - `start`/`end` são **byte offsets UTF-8**;
//! - spans são `[start, end)`, `end` exclusivo;
//! - linha/coluna são **derivadas** (via line index), nunca identidade;
//! - offsets são `u32` (limite teórico de ~4 GiB por source, suficiente);
//! - o path do arquivo **não** é identidade interna (`SourceId` é).

pub mod error;
pub mod id;
pub mod location;
pub mod manager;
pub mod source_file;
pub mod span;

pub use error::SourceLoadError;
pub use id::SourceId;
pub use location::SourceLocation;
pub use manager::SourceManager;
pub use source_file::SourceFile;
pub use span::SourceSpan;
