//! Identificação de versão do compiler.

/// Nome do executável.
pub const COMPILER_NAME: &str = "nexa";

/// Versão dos crates do workspace (do Cargo.toml raiz).
pub const COMPILER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Versão do toolchain de referência (contrato Impl 01 §263, §267):
/// antes de Alpha, `0.0.x-dev`.
pub const TOOLCHAIN_VERSION: &str = "0.0.1-dev";

/// Perfil de linguagem suportado declarado pelo toolchain.
/// A Language Spec está em `1.0 Freeze Candidate`, ainda não é 1.0 final.
pub const LANGUAGE_PROFILE: &str = "1.0-freeze-candidate";

/// `nexa version --format human` (linhas exatas do contrato Impl 01 §263).
pub fn version_human() -> String {
    format!("NEXA Reference Toolchain {TOOLCHAIN_VERSION}\nLanguage support: {LANGUAGE_PROFILE}")
}
