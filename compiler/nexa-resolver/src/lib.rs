pub mod associated;
pub mod index;
pub mod module_index;
pub mod name_interner;
pub mod reference_index;
pub mod resolution;
pub mod resolver;
pub mod scope;
pub mod symbol;

pub use index::SemanticIndex;
pub use name_interner::NameInterner;
pub use resolver::{resolve, ResolveResult};
pub use scope::{NamespaceChoice, ScopeGraph, ScopeKind};
pub use symbol::{SymbolData, SymbolTable};

#[cfg(test)]
mod tests {
    use crate::resolver::Resolver;
    use crate::scope::{NamespaceChoice, ScopeKind};
    use nexa_source::{SourceId, SourceSpan};
    use nexa_symbols::{SymbolKind, Visibility};

    fn dummy_span() -> SourceSpan {
        SourceSpan::new(SourceId(0), 0, 0)
    }

    #[test]
    fn phase_a_collect_struct_and_function() {
        let mut resolver = Resolver::new();
        let source_id = SourceId(0);
        let mod_id = resolver.register_root_module("test", source_id, dummy_span());
        let root_scope = resolver.index().scopes.module_scope(mod_id).unwrap();

        let struct_sym = resolver.collect_header(
            "MyStruct",
            SymbolKind::Struct,
            mod_id,
            root_scope,
            Visibility::ModulePrivate,
            dummy_span(),
        );
        let fn_sym = resolver.collect_header(
            "main_func",
            SymbolKind::Function,
            mod_id,
            root_scope,
            Visibility::ModulePrivate,
            dummy_span(),
        );

        assert_ne!(struct_sym, fn_sym);
        assert!(
            resolver.index().symbols.count() >= 2,
            "prelude + declarations present"
        );

        // Verify the struct symbol data.
        let data = resolver.index().symbols.get(struct_sym).unwrap();
        assert_eq!(data.kind, SymbolKind::Struct);
        assert_eq!(data.module, mod_id);

        // Verify the function symbol data.
        let data = resolver.index().symbols.get(fn_sym).unwrap();
        assert_eq!(data.kind, SymbolKind::Function);
    }

    #[test]
    fn name_resolution_finds_function_in_scope() {
        let mut resolver = Resolver::new();
        let source_id = SourceId(0);
        let mod_id = resolver.register_root_module("test", source_id, dummy_span());
        let root_scope = resolver.index().scopes.module_scope(mod_id).unwrap();

        resolver.collect_header(
            "my_func",
            SymbolKind::Function,
            mod_id,
            root_scope,
            Visibility::ModulePrivate,
            dummy_span(),
        );

        let resolved = resolver.resolve_name("my_func", root_scope, NamespaceChoice::Value);
        assert!(
            resolved.is_some(),
            "should resolve my_func in value namespace"
        );

        // Should not resolve in type namespace.
        let not_resolved = resolver.resolve_name("my_func", root_scope, NamespaceChoice::Type);
        assert!(
            not_resolved.is_none(),
            "function should not be in type namespace"
        );
    }

    #[test]
    fn name_resolution_walks_scope_chain() {
        let mut resolver = Resolver::new();
        let source_id = SourceId(0);
        let mod_id = resolver.register_root_module("test", source_id, dummy_span());
        let root_scope = resolver.index().scopes.module_scope(mod_id).unwrap();

        resolver.collect_header(
            "global_func",
            SymbolKind::Function,
            mod_id,
            root_scope,
            Visibility::ModulePrivate,
            dummy_span(),
        );

        // Create a child scope.
        let child_scope = resolver.index_mut().scopes.add_scope(
            ScopeKind::Block,
            Some(root_scope),
            mod_id,
            dummy_span(),
        );

        // Should find parent's function from child scope.
        let resolved = resolver.resolve_name("global_func", child_scope, NamespaceChoice::Value);
        assert!(resolved.is_some(), "should resolve through scope chain");
    }

    #[test]
    fn shadowing_replaces_in_same_scope() {
        let mut resolver = Resolver::new();
        let source_id = SourceId(0);
        let mod_id = resolver.register_root_module("test", source_id, dummy_span());
        let root_scope = resolver.index().scopes.module_scope(mod_id).unwrap();

        let sym1 = resolver.collect_header(
            "x",
            SymbolKind::Const,
            mod_id,
            root_scope,
            Visibility::ModulePrivate,
            dummy_span(),
        );
        let sym2 = resolver.collect_header(
            "x",
            SymbolKind::LocalLet,
            mod_id,
            root_scope,
            Visibility::ModulePrivate,
            dummy_span(),
        );

        // Política §360-364: duplicates são error com keep-first; a primeira
        // declaração válida permanece.
        assert_ne!(sym1, sym2);
        let resolved = resolver
            .resolve_name("x", root_scope, NamespaceChoice::Value)
            .unwrap();
        assert_eq!(
            resolved, sym1,
            "duplicate declaration keeps the first declaration"
        );

        // Should have a diagnostic for redeclaration.
        assert!(
            !resolver.diagnostics().is_empty(),
            "should emit redeclaration diagnostic"
        );
        assert_eq!(resolver.diagnostics()[0].code, "NEXA-SEM-0002");
    }

    #[test]
    fn visibility_module_private_blocks_cross_module_access() {
        let mut resolver = Resolver::new();
        let source_id = SourceId(0);
        let mod_a = resolver.register_root_module("mod_a", source_id, dummy_span());
        let mod_b = resolver.register_root_module("mod_b", source_id, dummy_span());
        let scope_a = resolver.index().scopes.module_scope(mod_a).unwrap();
        let scope_b = resolver.index().scopes.module_scope(mod_b).unwrap();

        resolver.collect_header(
            "private_fn",
            SymbolKind::Function,
            mod_a,
            scope_a,
            Visibility::ModulePrivate,
            dummy_span(),
        );

        // From mod_a, should resolve.
        let resolved_a = resolver.resolve_name("private_fn", scope_a, NamespaceChoice::Value);
        assert!(resolved_a.is_some(), "should resolve within same module");

        // From mod_b, should NOT resolve (module-private).
        let resolved_b = resolver.resolve_name("private_fn", scope_b, NamespaceChoice::Value);
        assert!(
            resolved_b.is_none(),
            "should not resolve across modules when private"
        );
    }

    #[test]
    fn public_visibility_accessible_cross_module() {
        let mut resolver = Resolver::new();
        let source_id = SourceId(0);
        let mod_a = resolver.register_root_module("mod_a", source_id, dummy_span());
        let mod_b = resolver.register_root_module("mod_b", source_id, dummy_span());
        let scope_a = resolver.index().scopes.module_scope(mod_a).unwrap();
        let scope_b = resolver.index().scopes.module_scope(mod_b).unwrap();

        resolver.collect_header(
            "public_fn",
            SymbolKind::Function,
            mod_a,
            scope_a,
            Visibility::Public,
            dummy_span(),
        );

        // Direct name from mod_b won't resolve (not in scope chain).
        let resolved_direct = resolver.resolve_name("public_fn", scope_b, NamespaceChoice::Value);
        assert!(
            resolved_direct.is_none(),
            "direct name should not cross module boundaries"
        );

        // Resolve from within mod_a (same module).
        let resolved_same = resolver.resolve_name("public_fn", scope_a, NamespaceChoice::Value);
        assert!(resolved_same.is_some(), "should resolve within same module");
    }

    #[test]
    fn qualified_path_resolution() {
        let mut resolver = Resolver::new();
        let source_id = SourceId(0);
        let root_mod = resolver.register_root_module("root", source_id, dummy_span());
        let root_scope = resolver.index().scopes.module_scope(root_mod).unwrap();

        // Create a child module "io".
        let (io_sym, io_mod) =
            resolver.collect_module_header("io", root_mod, Visibility::Public, dummy_span());
        let io_scope = resolver.index().scopes.module_scope(io_mod).unwrap();

        // Add a function inside "io".
        resolver.collect_header(
            "read",
            SymbolKind::Function,
            io_mod,
            io_scope,
            Visibility::Public,
            dummy_span(),
        );

        // Resolve "io::read" from root.
        let resolved = resolver.resolve_qualified(&["io", "read"], root_scope);
        assert!(
            resolved.is_some(),
            "should resolve io::read through qualified path"
        );

        // Resolve just "io" from root.
        let resolved_io = resolver.resolve_name("io", root_scope, NamespaceChoice::Module);
        assert!(resolved_io.is_some(), "should resolve io module");
        assert_eq!(resolved_io.unwrap(), io_sym);
    }

    #[test]
    fn body_scope_and_parameters() {
        let mut resolver = Resolver::new();
        let source_id = SourceId(0);
        let mod_id = resolver.register_root_module("test", source_id, dummy_span());
        let root_scope = resolver.index().scopes.module_scope(mod_id).unwrap();

        let fn_sym = resolver.collect_header(
            "my_fn",
            SymbolKind::Function,
            mod_id,
            root_scope,
            Visibility::ModulePrivate,
            dummy_span(),
        );

        // Create a body scope.
        let body_scope = resolver.create_body_scope(root_scope, mod_id, dummy_span());

        // Add parameters.
        let param_sym =
            resolver.collect_local("x", SymbolKind::Parameter, mod_id, body_scope, dummy_span());

        // Mark the function with body scope.
        if let Some(sym_data) = resolver.index_mut().symbols.get_mut(fn_sym) {
            sym_data.body_scope = Some(body_scope);
            sym_data.body_resolved = true;
        }

        // Resolve parameter from body scope.
        let resolved = resolver.resolve_name("x", body_scope, NamespaceChoice::Value);
        assert!(
            resolved.is_some(),
            "should resolve parameter x in body scope"
        );
        assert_eq!(resolved.unwrap(), param_sym);

        // Should NOT resolve parameter from root scope.
        let not_resolved = resolver.resolve_name("x", root_scope, NamespaceChoice::Value);
        assert!(
            not_resolved.is_none(),
            "parameter should not be visible at module scope"
        );
    }

    #[test]
    fn unresolved_name_emits_diagnostic() {
        let mut resolver = Resolver::new();
        let source_id = SourceId(0);
        let mod_id = resolver.register_root_module("test", source_id, dummy_span());
        let root_scope = resolver.index().scopes.module_scope(mod_id).unwrap();

        let resolved = resolver.resolve_name("nonexistent", root_scope, NamespaceChoice::Value);
        assert!(resolved.is_none());
    }

    #[test]
    fn module_hierarchy() {
        let mut resolver = Resolver::new();
        let source_id = SourceId(0);
        let root = resolver.register_root_module("root", source_id, dummy_span());

        let (_a_sym, a_mod) =
            resolver.collect_module_header("a", root, Visibility::Public, dummy_span());
        let (_b_sym, b_mod) =
            resolver.collect_module_header("b", a_mod, Visibility::Public, dummy_span());
        let b_scope = resolver.index().scopes.module_scope(b_mod).unwrap();

        resolver.collect_header(
            "helper",
            SymbolKind::Function,
            b_mod,
            b_scope,
            Visibility::Public,
            dummy_span(),
        );

        let root_scope = resolver.index().scopes.module_scope(root).unwrap();

        // Resolve "a::b::helper" from root.
        let resolved = resolver.resolve_qualified(&["a", "b", "helper"], root_scope);
        assert!(resolved.is_some(), "should resolve a::b::helper");

        // Verify parent-child relationships.
        let root_entry = resolver.index().modules.get(root).unwrap();
        assert_eq!(root_entry.children.len(), 1);
        assert_eq!(root_entry.children[0], a_mod);

        let a_entry = resolver.index().modules.get(a_mod).unwrap();
        assert_eq!(a_entry.children.len(), 1);
        assert_eq!(a_entry.children[0], b_mod);
        assert_eq!(a_entry.parent, Some(root));
    }

    #[test]
    fn type_namespace_separation() {
        let mut resolver = Resolver::new();
        let source_id = SourceId(0);
        let mod_id = resolver.register_root_module("test", source_id, dummy_span());
        let root_scope = resolver.index().scopes.module_scope(mod_id).unwrap();

        // Struct in type namespace.
        resolver.collect_header(
            "MyType",
            SymbolKind::Struct,
            mod_id,
            root_scope,
            Visibility::ModulePrivate,
            dummy_span(),
        );

        // Function with same name in value namespace.
        resolver.collect_header(
            "MyType",
            SymbolKind::Function,
            mod_id,
            root_scope,
            Visibility::ModulePrivate,
            dummy_span(),
        );

        // Both should resolve in their respective namespaces.
        let type_sym = resolver.resolve_name("MyType", root_scope, NamespaceChoice::Type);
        let val_sym = resolver.resolve_name("MyType", root_scope, NamespaceChoice::Value);
        assert!(type_sym.is_some(), "MyType should resolve as type");
        assert!(val_sym.is_some(), "MyType should resolve as value");
        assert_ne!(
            type_sym, val_sym,
            "type and value namespaces should have different symbols"
        );
    }
}
