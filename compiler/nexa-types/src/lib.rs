pub mod compatibility;
pub mod format;
pub mod id;
pub mod prelude;
pub mod store;
pub mod ty;

pub use compatibility::{argument_compatible, assignable, return_compatible, same_type};
pub use format::format_type;
pub use id::{GenericParamId, ImplementationId, NominalTypeId, TypeId, INVALID_TYPE};
pub use prelude::{
    bootstrap_prelude, bootstrap_prelude_nominal, PreludeNominalBootstrap, PreludeSymbols,
    PreludeTypes,
};
pub use store::TypeStore;
pub use ty::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::bootstrap_prelude;
    use crate::store::TypeStore;

    #[test]
    fn primitives_are_interned() {
        let mut store = TypeStore::new();
        let int1 = store.intern_type(ty::Type::Int);
        let int2 = store.intern_type(ty::Type::Int);
        assert_eq!(int1, int2, "same primitive should intern to same TypeId");
        assert_eq!(store.type_count(), 1, "only one type stored");
    }

    #[test]
    fn different_primitives_distinct() {
        let mut store = TypeStore::new();
        let int = store.intern_type(ty::Type::Int);
        let bool = store.intern_type(ty::Type::Bool);
        assert_ne!(int, bool);
        assert_eq!(store.type_count(), 2);
    }

    #[test]
    fn ref_types_interned() {
        let mut store = TypeStore::new();
        let int = store.intern_type(ty::Type::Int);
        let ref1 = store.intern_type(ty::Type::Ref(int));
        let ref2 = store.intern_type(ty::Type::Ref(int));
        assert_eq!(ref1, ref2, "Ref<Int> should intern to same TypeId");
        let int_ref = store.intern_type(ty::Type::Ref(int));
        let bool = store.intern_type(ty::Type::Bool);
        let bool_ref = store.intern_type(ty::Type::Ref(bool));
        assert_ne!(int_ref, bool_ref, "Ref<Int> ≠ Ref<Bool>");
    }

    #[test]
    fn applied_types_interned() {
        let mut store = TypeStore::new();
        let int = store.intern_type(ty::Type::Int);
        let string = store.intern_type(ty::Type::String);
        let arr_int1 = store.create_applied(int, vec![int]);
        let arr_int2 = store.create_applied(int, vec![int]);
        assert_eq!(arr_int1, arr_int2);
        let arr_str = store.create_applied(int, vec![string]);
        assert_ne!(arr_int1, arr_str);
    }

    #[test]
    fn prelude_bootstrap() {
        let mut store = TypeStore::new();
        let prelude = bootstrap_prelude(&mut store);
        assert_eq!(store.type_count(), 19); // 19 primitive types
        assert!(same_type(&store, prelude.int, prelude.int));
        assert!(!same_type(&store, prelude.int, prelude.bool));
        assert!(!same_type(&store, prelude.int, prelude.string));
    }

    #[test]
    fn same_type_reflexive() {
        let mut store = TypeStore::new();
        let int = store.intern_type(ty::Type::Int);
        assert!(same_type(&store, int, int));
    }

    #[test]
    fn same_type_nested() {
        let mut store = TypeStore::new();
        let int = store.intern_type(ty::Type::Int);
        let ref_int1 = store.intern_type(ty::Type::Ref(int));
        let ref_int2 = store.intern_type(ty::Type::Ref(int));
        assert!(same_type(&store, ref_int1, ref_int2));
    }

    #[test]
    fn assignable_same_type() {
        let mut store = TypeStore::new();
        let int = store.intern_type(ty::Type::Int);
        assert!(assignable(&store, int, int));
    }

    #[test]
    fn assignable_different_type_rejected() {
        let mut store = TypeStore::new();
        let int = store.intern_type(ty::Type::Int);
        let bool = store.intern_type(ty::Type::Bool);
        assert!(!assignable(&store, int, bool));
    }

    #[test]
    fn never_assignable_to_any() {
        let mut store = TypeStore::new();
        let never = store.intern_type(ty::Type::Never);
        let int = store.intern_type(ty::Type::Int);
        let bool = store.intern_type(ty::Type::Bool);
        assert!(assignable(&store, never, int));
        assert!(assignable(&store, never, bool));
    }

    #[test]
    fn error_assignable_to_any() {
        let mut store = TypeStore::new();
        let err = store.intern_type(ty::Type::Error);
        let int = store.intern_type(ty::Type::Int);
        assert!(assignable(&store, err, int));
    }

    #[test]
    fn nominal_type_creation() {
        let mut store = TypeStore::new();
        let nominal = NominalType {
            symbol: nexa_symbols::SymbolId(0),
            kind: NominalTypeKind::Struct,
            generic_params: Vec::new(),
            package: nexa_symbols::PackageInstanceId(0),
            module: nexa_symbols::ModuleId(0),
        };
        let (tid, nid) = store.create_nominal(nominal);
        let retrieved = store.get_nominal(nid);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().kind, NominalTypeKind::Struct);
        let retrieved_type = store.get_type(tid);
        assert!(matches!(retrieved_type, Some(ty::Type::Nominal(_))));
    }

    #[test]
    fn callable_type_interned() {
        let mut store = TypeStore::new();
        let int = store.intern_type(ty::Type::Int);
        let string = store.intern_type(ty::Type::String);
        let ct1 = store.intern_type(ty::Type::Callable(CallableType {
            kind: CallableKind::Function,
            parameters: vec![int, int],
            return_type: int,
            generic_params: Vec::new(),
        }));
        let ct2 = store.intern_type(ty::Type::Callable(CallableType {
            kind: CallableKind::Function,
            parameters: vec![int, int],
            return_type: int,
            generic_params: Vec::new(),
        }));
        assert_eq!(ct1, ct2, "same callable type should intern");
        let ct3 = store.intern_type(ty::Type::Callable(CallableType {
            kind: CallableKind::Function,
            parameters: vec![int, string],
            return_type: int,
            generic_params: Vec::new(),
        }));
        assert_ne!(ct1, ct3, "different callable types should not intern");
    }

    #[test]
    fn is_integer_helpers() {
        let mut store = TypeStore::new();
        let int = store.intern_type(ty::Type::Int);
        let bool = store.intern_type(ty::Type::Bool);
        assert!(compatibility::is_integer(&store, int));
        assert!(!compatibility::is_integer(&store, bool));
    }

    #[test]
    fn is_bool_helpers() {
        let mut store = TypeStore::new();
        let bool = store.intern_type(ty::Type::Bool);
        let int = store.intern_type(ty::Type::Int);
        assert!(compatibility::is_bool(&store, bool));
        assert!(!compatibility::is_bool(&store, int));
    }

    #[test]
    fn is_numeric_helpers() {
        let mut store = TypeStore::new();
        let int = store.intern_type(ty::Type::Int);
        let float = store.intern_type(ty::Type::Float64);
        let bool = store.intern_type(ty::Type::Bool);
        assert!(compatibility::is_numeric(&store, int));
        assert!(compatibility::is_numeric(&store, float));
        assert!(!compatibility::is_numeric(&store, bool));
    }

    #[test]
    fn argument_compatible_same_type() {
        let mut store = TypeStore::new();
        let int = store.intern_type(ty::Type::Int);
        assert!(compatibility::argument_compatible(&store, int, int));
    }

    #[test]
    fn return_compatible_never() {
        let mut store = TypeStore::new();
        let never = store.intern_type(ty::Type::Never);
        let int = store.intern_type(ty::Type::Int);
        assert!(compatibility::return_compatible(&store, never, int));
    }

    // ─── Fase A: prelude nominal, format, aliases, layout cycles ──

    fn dummy_symbols() -> crate::prelude::PreludeSymbols {
        use nexa_symbols::SymbolId;
        crate::prelude::PreludeSymbols {
            optional: SymbolId(1),
            result: SymbolId(2),
            some: SymbolId(3),
            none: SymbolId(4),
            success: SymbolId(5),
            failure: SymbolId(6),
        }
    }

    #[test]
    fn prelude_nominal_bootstrap() {
        let mut store = TypeStore::new();
        let bootstrap = super::bootstrap_prelude_nominal(&mut store, &dummy_symbols());
        let int = store.intern_type(ty::Type::Int);
        let string = store.intern_type(ty::Type::String);
        let opt_int = bootstrap.optional_of(&mut store, int);
        let res = bootstrap.result_of(&mut store, int, string);
        // `Some(int)` e `Failure(string)` devem vertebrar aplicações.
        match store.get_type(opt_int) {
            Some(crate::ty::Type::Applied { arguments, .. }) => assert_eq!(arguments, &[int]),
            other => panic!("expected applied, got {other:?}"),
        }
        match store.get_type(res) {
            Some(crate::ty::Type::Applied { base, arguments }) => {
                assert_eq!(arguments, &[int, string]);
                assert!(matches!(
                    store.get_type(*base),
                    Some(crate::ty::Type::Nominal(n))
                        if *n == bootstrap.result_nid
                ));
            }
            other => panic!("expected applied, got {other:?}"),
        }
    }

    #[test]
    fn format_type_uses_nominal_names() {
        let mut store = TypeStore::new();
        let bootstrap = super::bootstrap_prelude_nominal(&mut store, &dummy_symbols());
        let int = store.intern_type(ty::Type::Int);
        let opt_int = bootstrap.optional_of(&mut store, int);
        assert_eq!(crate::format::format_type(&store, opt_int), "Optional(Int)");
        let int_ref = store.intern_type(ty::Type::Ref(int));
        assert_eq!(crate::format::format_type(&store, int_ref), "Ref(Int)");
    }

    #[test]
    fn generic_substitution() {
        let mut store = TypeStore::new();
        let bootstrap = super::bootstrap_prelude_nominal(&mut store, &dummy_symbols());
        let int = store.intern_type(ty::Type::Int);
        let t_ty = store.intern_type(ty::Type::GenericParameter(bootstrap.optional_t));
        let opt_t = bootstrap.optional_of(&mut store, t_ty);
        let substituted = store.substitute_type(
            opt_t,
            &std::collections::HashMap::from([(bootstrap.optional_t, int)]),
        );
        assert_eq!(
            substitute_of(&store, substituted),
            vec![int],
            "Optional<T> com T→Int deve virar Optional(Int)"
        );
    }

    fn substitute_of(store: &TypeStore, ty: TypeId) -> Vec<TypeId> {
        match store.get_type(ty) {
            Some(crate::ty::Type::Applied { arguments, .. }) => arguments.clone(),
            other => panic!("expected applied, got {other:?}"),
        }
    }

    #[test]
    fn alias_is_error_flag() {
        let mut store = TypeStore::new();
        let int = store.intern_type(ty::Type::Int);
        let sym = nexa_symbols::SymbolId(99);
        store.declare_alias(sym, int);
        assert!(store.is_alias(sym));
        assert_eq!(store.alias_target(sym), Some(int));
        assert!(!store.alias_is_error(sym));
        store.set_alias_error(sym);
        assert!(store.alias_is_error(sym));
    }

    #[test]
    fn layout_cycle_detected() {
        use std::collections::HashSet;
        let mut store = TypeStore::new();
        let int = store.intern_type(ty::Type::Int);
        let (node, node_nid) = store.create_nominal(crate::ty::NominalType {
            symbol: nexa_symbols::SymbolId(200),
            kind: crate::ty::NominalTypeKind::Struct,
            generic_params: Vec::new(),
            package: nexa_symbols::PackageInstanceId(0),
            module: nexa_symbols::ModuleId(0),
        });
        let ref_node_ty = store.intern_type(crate::ty::Type::Ref(node));
        store.register_nominal_definition(
            node_nid,
            crate::ty::TypeDefinition::Struct(crate::ty::StructTypeDefinition {
                name: "Node".to_string(),
                ty: node,
                visibility: nexa_symbols::Visibility::Public,
                symbol: nexa_symbols::SymbolId(200),
                fields: vec![crate::ty::FieldDefinition {
                    name: "next".to_string(),
                    ty: ref_node_ty,
                    exported: false,
                    symbol: nexa_symbols::SymbolId(201),
                }],
            }),
        );
        let mut guard = HashSet::new();
        assert!(
            !store.nominal_contains(node_nid, node_nid, &mut guard),
            "Ref quebra o ciclo de layout"
        );
        // Ciclo owned real: Alpha { child: Beta } e Beta { next: Alpha }.
        let (alpha, alpha_nid) = store.create_nominal(crate::ty::NominalType {
            symbol: nexa_symbols::SymbolId(300),
            kind: crate::ty::NominalTypeKind::Struct,
            generic_params: Vec::new(),
            package: nexa_symbols::PackageInstanceId(0),
            module: nexa_symbols::ModuleId(0),
        });
        let (beta, beta_nid) = store.create_nominal(crate::ty::NominalType {
            symbol: nexa_symbols::SymbolId(301),
            kind: crate::ty::NominalTypeKind::Struct,
            generic_params: Vec::new(),
            package: nexa_symbols::PackageInstanceId(0),
            module: nexa_symbols::ModuleId(0),
        });
        let alpha_ty = store.intern_type(crate::ty::Type::Nominal(alpha_nid));
        let beta_ty = store.intern_type(crate::ty::Type::Nominal(beta_nid));
        store.register_nominal_definition(
            alpha_nid,
            crate::ty::TypeDefinition::Struct(crate::ty::StructTypeDefinition {
                name: "Alpha".to_string(),
                ty: alpha,
                visibility: nexa_symbols::Visibility::Public,
                symbol: nexa_symbols::SymbolId(300),
                fields: vec![crate::ty::FieldDefinition {
                    name: "child".to_string(),
                    ty: beta_ty,
                    exported: false,
                    symbol: nexa_symbols::SymbolId(302),
                }],
            }),
        );
        store.register_nominal_definition(
            beta_nid,
            crate::ty::TypeDefinition::Struct(crate::ty::StructTypeDefinition {
                name: "Beta".to_string(),
                ty: beta,
                visibility: nexa_symbols::Visibility::Public,
                symbol: nexa_symbols::SymbolId(301),
                fields: vec![crate::ty::FieldDefinition {
                    name: "next".to_string(),
                    ty: alpha_ty,
                    exported: false,
                    symbol: nexa_symbols::SymbolId(303),
                }],
            }),
        );
        let mut guard = HashSet::new();
        assert!(
            store.nominal_contains(alpha_nid, alpha_nid, &mut guard),
            "cadeia owned Alpha→Beta→Alpha é ciclo de layout"
        );
        let _ = int;
    }

    #[test]
    fn interface_upcast_via_store() {
        let mut store = TypeStore::new();
        let (iface, _iface_nid) = store.create_nominal(crate::ty::NominalType {
            symbol: nexa_symbols::SymbolId(400),
            kind: crate::ty::NominalTypeKind::Interface,
            generic_params: Vec::new(),
            package: nexa_symbols::PackageInstanceId(0),
            module: nexa_symbols::ModuleId(0),
        });
        let (st, st_nid) = store.create_nominal(crate::ty::NominalType {
            symbol: nexa_symbols::SymbolId(401),
            kind: crate::ty::NominalTypeKind::Struct,
            generic_params: Vec::new(),
            package: nexa_symbols::PackageInstanceId(0),
            module: nexa_symbols::ModuleId(0),
        });
        store.register_nominal_definition(
            st_nid,
            crate::ty::TypeDefinition::Struct(crate::ty::StructTypeDefinition {
                name: "Point".to_string(),
                ty: st,
                visibility: nexa_symbols::Visibility::Public,
                symbol: nexa_symbols::SymbolId(401),
                fields: Vec::new(),
            }),
        );
        store.register_implementation(crate::ty::Implementation {
            id: crate::id::ImplementationId(0),
            interface: Some(iface),
            target: st,
            generic_params: Vec::new(),
            constraints: Vec::new(),
            members: Vec::new(),
        });
        assert!(
            crate::compatibility::assignable(&store, st, iface),
            "Struct que implementa a interface deve upcast"
        );
        let other = st_nid_other(&mut store);
        assert!(
            !crate::compatibility::assignable(&store, st, other),
            "upcast para tipo não-interface deve falhar"
        );
    }

    fn st_nid_other(store: &mut TypeStore) -> TypeId {
        let (b, _) = store.create_nominal(crate::ty::NominalType {
            symbol: nexa_symbols::SymbolId(402),
            kind: crate::ty::NominalTypeKind::Struct,
            generic_params: Vec::new(),
            package: nexa_symbols::PackageInstanceId(0),
            module: nexa_symbols::ModuleId(0),
        });
        b
    }
}
