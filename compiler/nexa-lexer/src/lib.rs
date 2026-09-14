//! NEXA — Lexer lossless
//!
//! Pipeline:
//!
//! ```text
//! RAW BYTES → SourceManager → validated SourceFile → Lexer
//!      → Lossless Lexeme Stream (tokens + trivia) + Diagnostics
//! ```
//!
//! Invariantes centrais:
//! - cobertura **lossless** de bytes: todos os bytes do source são cobertos
//!   por lexemes (tokens + trivia); a reconstrução `source[span]` para todos
//!   os lexemes não-EOF reproduz exatamente os bytes originais;
//! - spans ordenados e contíguos; EOF zero-width no fim;
//! - lexer nunca faz `panic` em input hostil (recovery consome ≥ 1 scalar);
//! - operadores por longest-match;
//! - strings normais/multiline usam mode stack com `StringText` +
//!   interpolação (`${...}` com brace depth); raw/byte strings são literais
//!   únicos sem interpolação;
//! - `Newline` (LF/CRLF) é token; whitespace horizontal e comentários são
//!   trivia; comentários de bloco são aninhados e iterativos (O(n)).

pub mod json;
pub mod keyword;
pub mod lexeme;
pub mod lexer;
pub mod string;
pub mod token;
pub mod trivia;

pub use json::{lex_output_json, LexOutputEnvelope, TokenRecord};
pub use keyword::Keyword;
pub use lexeme::{Lexeme, LexemeKind};
pub use lexer::{lex, LexResult};
pub use token::{StringStyle, TokenKind};
pub use trivia::TriviaKind;
