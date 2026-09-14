//! Prelude: names disponíveis sem qualificação em escopo de module.
//!
//! Implementação 03 (§173-176): o resolver precisa dos símbolos do Prelude
//! antes do stdlib existir. Este módulo define o conjunto **congelado** de
//! nomes do Prelude 1.0 e seus namespaces. Não implementa os tipos — apenas
//! registra identidades simbólicas.

use serde::Serialize;

/// Namespace semântico do nome do Prelude.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum PreludeNameKind {
    Type,
    Value,
}

/// Uma entrada do Prelude: nome + namespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct PreludeName {
    pub name: &'static str,
    pub kind: PreludeNameKind,
}

pub const PRELUDE_TYPE_TYPE: PreludeNameKind = PreludeNameKind::Type;
pub const PRELUDE_VALUE: PreludeNameKind = PreludeNameKind::Value;

/// Conjunto congelado do Prelude 1.0 (Impl 03 §174).
///
/// Tipos fundamentais + interfaces de marcação/vigilância + valores e
/// construtores especiais de `Optional`/`Result`.
pub const PRELUDE_NAMES: &[PreludeName] = &[
    // Tipos fundamentais (§26).
    PreludeName {
        name: "Unit",
        kind: PRELUDE_TYPE_TYPE,
    },
    PreludeName {
        name: "Never",
        kind: PRELUDE_TYPE_TYPE,
    },
    PreludeName {
        name: "Bool",
        kind: PRELUDE_TYPE_TYPE,
    },
    PreludeName {
        name: "Int",
        kind: PRELUDE_TYPE_TYPE,
    },
    PreludeName {
        name: "UInt",
        kind: PRELUDE_TYPE_TYPE,
    },
    PreludeName {
        name: "Int8",
        kind: PRELUDE_TYPE_TYPE,
    },
    PreludeName {
        name: "Int16",
        kind: PRELUDE_TYPE_TYPE,
    },
    PreludeName {
        name: "Int32",
        kind: PRELUDE_TYPE_TYPE,
    },
    PreludeName {
        name: "Int64",
        kind: PRELUDE_TYPE_TYPE,
    },
    PreludeName {
        name: "UInt8",
        kind: PRELUDE_TYPE_TYPE,
    },
    PreludeName {
        name: "UInt16",
        kind: PRELUDE_TYPE_TYPE,
    },
    PreludeName {
        name: "UInt32",
        kind: PRELUDE_TYPE_TYPE,
    },
    PreludeName {
        name: "UInt64",
        kind: PRELUDE_TYPE_TYPE,
    },
    PreludeName {
        name: "Float32",
        kind: PRELUDE_TYPE_TYPE,
    },
    PreludeName {
        name: "Float64",
        kind: PRELUDE_TYPE_TYPE,
    },
    PreludeName {
        name: "Byte",
        kind: PRELUDE_TYPE_TYPE,
    },
    PreludeName {
        name: "Char",
        kind: PRELUDE_TYPE_TYPE,
    },
    PreludeName {
        name: "String",
        kind: PRELUDE_TYPE_TYPE,
    },
    PreludeName {
        name: "Bytes",
        kind: PRELUDE_TYPE_TYPE,
    },
    // Tipos paramétricos / tarefas (§174).
    PreludeName {
        name: "Optional",
        kind: PRELUDE_TYPE_TYPE,
    },
    PreludeName {
        name: "Result",
        kind: PRELUDE_TYPE_TYPE,
    },
    PreludeName {
        name: "Array",
        kind: PRELUDE_TYPE_TYPE,
    },
    PreludeName {
        name: "Map",
        kind: PRELUDE_TYPE_TYPE,
    },
    PreludeName {
        name: "Set",
        kind: PRELUDE_TYPE_TYPE,
    },
    PreludeName {
        name: "Task",
        kind: PRELUDE_TYPE_TYPE,
    },
    PreludeName {
        name: "Future",
        kind: PRELUDE_TYPE_TYPE,
    },
    // Interfaces de marcação/vigilância.
    PreludeName {
        name: "Copy",
        kind: PRELUDE_TYPE_TYPE,
    },
    PreludeName {
        name: "Clone",
        kind: PRELUDE_TYPE_TYPE,
    },
    PreludeName {
        name: "Eq",
        kind: PRELUDE_TYPE_TYPE,
    },
    PreludeName {
        name: "Hash",
        kind: PRELUDE_TYPE_TYPE,
    },
    PreludeName {
        name: "Comparable",
        kind: PRELUDE_TYPE_TYPE,
    },
    PreludeName {
        name: "Ordering",
        kind: PRELUDE_TYPE_TYPE,
    },
    PreludeName {
        name: "Display",
        kind: PRELUDE_TYPE_TYPE,
    },
    PreludeName {
        name: "Send",
        kind: PRELUDE_TYPE_TYPE,
    },
    PreludeName {
        name: "Share",
        kind: PRELUDE_TYPE_TYPE,
    },
    // Standard runtime facade available to the first executable profile.
    PreludeName {
        name: "Console",
        kind: PRELUDE_TYPE_TYPE,
    },
    // Valores do Prelude.
    PreludeName {
        name: "assert",
        kind: PRELUDE_VALUE,
    },
    PreludeName {
        name: "panic",
        kind: PRELUDE_VALUE,
    },
    // Construtores especiais de Optional/Result no value namespace (§197).
    PreludeName {
        name: "Some",
        kind: PRELUDE_VALUE,
    },
    PreludeName {
        name: "None",
        kind: PRELUDE_VALUE,
    },
    PreludeName {
        name: "Success",
        kind: PRELUDE_VALUE,
    },
    PreludeName {
        name: "Failure",
        kind: PRELUDE_VALUE,
    },
];

/// Nomes do Prelude 1.0 (todos) — reservados nos respectivos namespaces
/// no nível de module (§188-192).
pub const PRELUDE_SYMBOLS: &[&str] = &[
    "Unit",
    "Never",
    "Bool",
    "Int",
    "UInt",
    "Int8",
    "Int16",
    "Int32",
    "Int64",
    "UInt8",
    "UInt16",
    "UInt32",
    "UInt64",
    "Float32",
    "Float64",
    "Byte",
    "Char",
    "String",
    "Bytes",
    "Optional",
    "Result",
    "Array",
    "Map",
    "Set",
    "Task",
    "Future",
    "Copy",
    "Clone",
    "Eq",
    "Hash",
    "Comparable",
    "Ordering",
    "Display",
    "Send",
    "Share",
    "Console",
    "assert",
    "panic",
    "Some",
    "None",
    "Success",
    "Failure",
];
