use crate::id::TypeId;
use crate::store::TypeStore;
use crate::ty::*;

/// Exact type equality (structural, not nominal for aliases).
pub fn same_type(store: &TypeStore, a: TypeId, b: TypeId) -> bool {
    if a == b {
        return true;
    }
    let ta = match store.get_type(a) {
        Some(t) => t,
        None => return false,
    };
    let tb = match store.get_type(b) {
        Some(t) => t,
        None => return false,
    };
    match (ta, tb) {
        (Type::Error, _) | (_, Type::Error) => true,
        (Type::Ref(a_inner), Type::Ref(b_inner)) => same_type(store, *a_inner, *b_inner),
        (Type::MutRef(a_inner), Type::MutRef(b_inner)) => same_type(store, *a_inner, *b_inner),
        (Type::Array(a_inner), Type::Array(b_inner)) => same_type(store, *a_inner, *b_inner),
        (Type::Task(a_inner), Type::Task(b_inner)) => same_type(store, *a_inner, *b_inner),
        (
            Type::Applied {
                base: ab,
                arguments: aa,
            },
            Type::Applied {
                base: bb,
                arguments: ba,
            },
        ) => {
            same_type(store, *ab, *bb)
                && aa.len() == ba.len()
                && aa
                    .iter()
                    .zip(ba.iter())
                    .all(|(a, b)| same_type(store, *a, *b))
        }
        (Type::Callable(ca), Type::Callable(cb)) => {
            ca.kind == cb.kind
                && ca.parameters.len() == cb.parameters.len()
                && ca
                    .parameters
                    .iter()
                    .zip(cb.parameters.iter())
                    .all(|(a, b)| same_type(store, *a, *b))
                && same_type(store, ca.return_type, cb.return_type)
                && ca.generic_params.len() == cb.generic_params.len()
        }
        // Interning garante que tipos estruturalmente iguais têm o mesmo
        // TypeId; ids distintos ⇒ tipos distintos (nunca equacionar por
        // discriminant apenas — dois nominals diferentes não são iguais).
        _ => false,
    }
}

/// Assignment compatibility: can `from` be assigned to `to`?
/// Includes interface upcast.
pub fn assignable(store: &TypeStore, from: TypeId, to: TypeId) -> bool {
    if same_type(store, from, to) {
        return true;
    }
    // Never coerces to anything.
    if let Some(Type::Never) = store.get_type(from) {
        return true;
    }
    // Interface upcast: T → I if T implements I.
    if is_interface_upcast(store, from, to) {
        return true;
    }
    false
}

/// Call argument compatibility: can `arg_type` be passed as `param_type`?
pub fn argument_compatible(store: &TypeStore, arg_type: TypeId, param_type: TypeId) -> bool {
    assignable(store, arg_type, param_type)
}

/// Return expression compatibility: can `expr_type` be returned as `return_type`?
pub fn return_compatible(store: &TypeStore, expr_type: TypeId, return_type: TypeId) -> bool {
    if same_type(store, expr_type, return_type) {
        return true;
    }
    if let Some(Type::Never) = store.get_type(expr_type) {
        return true;
    }
    assignable(store, expr_type, return_type)
}

/// Compute coercion needed to convert `from` to `to`.
pub fn interface_coercible(store: &TypeStore, from: TypeId, to: TypeId) -> Coercion {
    if same_type(store, from, to) {
        return Coercion::None;
    }
    if let Some(Type::Never) = store.get_type(from) {
        return Coercion::NeverToAny;
    }
    if let Some(implementation) = store.has_interface(from, to) {
        return Coercion::InterfaceUpcast { implementation };
    }
    Coercion::None
}

/// `true` se `from` pode subir para a interface `to` (§264-266, §724).
fn is_interface_upcast(store: &TypeStore, from: TypeId, to: TypeId) -> bool {
    if !is_interface_type(store, to) {
        return false;
    }
    store.has_interface(from, to).is_some()
}

/// O tipo `ty` é (ou é aplicação de) uma interface nominal?
pub fn is_interface_type(store: &TypeStore, ty: TypeId) -> bool {
    match store.get_type(ty) {
        Some(Type::Nominal(nid)) => store
            .get_nominal(*nid)
            .is_some_and(|n| n.kind == NominalTypeKind::Interface),
        Some(Type::Applied { base, .. }) => match store.get_type(*base) {
            Some(Type::Nominal(nid)) => store
                .get_nominal(*nid)
                .is_some_and(|n| n.kind == NominalTypeKind::Interface),
            _ => false,
        },
        _ => false,
    }
}

/// Check if a type is an integer type.
pub fn is_integer(store: &TypeStore, id: TypeId) -> bool {
    matches!(
        store.get_type(id),
        Some(
            Type::Int
                | Type::UInt
                | Type::Int8
                | Type::Int16
                | Type::Int32
                | Type::Int64
                | Type::UInt8
                | Type::UInt16
                | Type::UInt32
                | Type::UInt64
                | Type::Byte
        )
    )
}

/// Check if a type is a signed integer type.
pub fn is_signed_integer(store: &TypeStore, id: TypeId) -> bool {
    matches!(
        store.get_type(id),
        Some(Type::Int | Type::Int8 | Type::Int16 | Type::Int32 | Type::Int64)
    )
}

/// Check if a type is a float type.
pub fn is_float(store: &TypeStore, id: TypeId) -> bool {
    matches!(store.get_type(id), Some(Type::Float32 | Type::Float64))
}

/// Check if a type is numeric (integer or float).
pub fn is_numeric(store: &TypeStore, id: TypeId) -> bool {
    is_integer(store, id) || is_float(store, id)
}

/// Check if a type is comparable (supports <, <=, >, >=).
pub fn is_comparable(store: &TypeStore, id: TypeId) -> bool {
    is_numeric(store, id) || matches!(store.get_type(id), Some(Type::Char | Type::String))
}

/// Check if a type supports equality (==, !=).
pub fn is_eq_capable(store: &TypeStore, id: TypeId) -> bool {
    is_numeric(store, id)
        || matches!(
            store.get_type(id),
            Some(Type::Bool | Type::Char | Type::String)
        )
}

/// Check if a type is Bool.
pub fn is_bool(store: &TypeStore, id: TypeId) -> bool {
    matches!(store.get_type(id), Some(Type::Bool))
}

/// Check if a type is the Unit type.
pub fn is_unit(store: &TypeStore, id: TypeId) -> bool {
    matches!(store.get_type(id), Some(Type::Unit))
}
