//! Formatação de tipos para diagnostics (§515-519).
//!
//! Apresentação source-like: `Int`, `Array(String)`, `Ref(Int)`,
//! `Optional(User)`, `Result<Int,String>(payload)` etc.

use crate::id::TypeId;
use crate::store::TypeStore;
use crate::ty::*;

/// Formata um TypeId de forma determinística.
pub fn format_type(store: &TypeStore, ty: TypeId) -> String {
    match store.get_type(ty) {
        None => "<unresolved>".to_string(),
        Some(Type::Error) => "Error".to_string(),
        Some(Type::Unit) => "Unit".to_string(),
        Some(Type::Never) => "Never".to_string(),
        Some(Type::Bool) => "Bool".to_string(),
        Some(Type::Int) => "Int".to_string(),
        Some(Type::UInt) => "UInt".to_string(),
        Some(Type::Int8) => "Int8".to_string(),
        Some(Type::Int16) => "Int16".to_string(),
        Some(Type::Int32) => "Int32".to_string(),
        Some(Type::Int64) => "Int64".to_string(),
        Some(Type::UInt8) => "UInt8".to_string(),
        Some(Type::UInt16) => "UInt16".to_string(),
        Some(Type::UInt32) => "UInt32".to_string(),
        Some(Type::UInt64) => "UInt64".to_string(),
        Some(Type::Float32) => "Float32".to_string(),
        Some(Type::Float64) => "Float64".to_string(),
        Some(Type::Byte) => "Byte".to_string(),
        Some(Type::Char) => "Char".to_string(),
        Some(Type::String) => "String".to_string(),
        Some(Type::Bytes) => "Bytes".to_string(),
        Some(Type::Ref(inner)) => format!("Ref({})", format_type(store, *inner)),
        Some(Type::MutRef(inner)) => format!("MutRef({})", format_type(store, *inner)),
        Some(Type::Array(inner)) => format!("Array({})", format_type(store, *inner)),
        Some(Type::Task(inner)) => format!("Task({})", format_type(store, *inner)),
        Some(Type::GenericParameter(gp)) => {
            format!("T{}", gp.0)
        }
        Some(Type::Applied { base, arguments }) => {
            let base_name = nominal_name(store, *base);
            if base_name == "Optional" && arguments.len() == 1 {
                format!("Optional({})", format_type(store, arguments[0]))
            } else if base_name == "Result" && arguments.len() == 2 {
                format!(
                    "Result({},{})",
                    format_type(store, arguments[0]),
                    format_type(store, arguments[1])
                )
            } else {
                let args: Vec<String> = arguments.iter().map(|a| format_type(store, *a)).collect();
                format!("{base_name}({})", args.join(","))
            }
        }
        Some(Type::Nominal(_nid)) => nominal_name(store, ty).to_string(),
        Some(Type::Callable(c)) => {
            let ret = format_type(store, c.return_type);
            match c.kind {
                CallableKind::Function | CallableKind::Action | CallableKind::AsyncAction => {
                    let kind = match c.kind {
                        CallableKind::Function => "fn",
                        CallableKind::Action => "action",
                        CallableKind::AsyncAction => "async action",
                    };
                    let params: Vec<String> = c
                        .parameters
                        .iter()
                        .map(|p| format_type(store, *p))
                        .collect();
                    format!("{kind}({}) -> {ret}", params.join(", "))
                }
            }
        }
    }
}

fn nominal_name(store: &TypeStore, ty: TypeId) -> String {
    match store.get_type(ty) {
        Some(Type::Nominal(nid)) => store
            .get_nominal_definition(*nid)
            .map(|d| match d {
                TypeDefinition::Struct(s) => s.name.clone(),
                TypeDefinition::Enum(e) => e.name.clone(),
                TypeDefinition::Interface(i) => i.name.clone(),
                TypeDefinition::Distinct(d) => d.name.clone(),
            })
            .unwrap_or_else(|| "<nominal>".to_string()),
        _ => "<nominal>".to_string(),
    }
}
