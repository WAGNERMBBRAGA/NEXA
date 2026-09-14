use crate::id::{GenericParamId, ImplementationId, NominalTypeId, TypeId};
use crate::ty::*;
use crate::TypeKey;
use nexa_symbols::SymbolId;
use std::collections::HashMap;

/// Central store for all types in a compilation session.
/// Types are interned: the same composite type always yields the same TypeId.
pub struct TypeStore {
    types: Vec<Type>,
    interned: HashMap<TypeKey, TypeId>,
    nominals: Vec<NominalType>,
    /// Maps NominalTypeId → TypeId (the TypeId of the Type::Nominal(nid) entry).
    nominal_to_type: HashMap<NominalTypeId, TypeId>,
    generic_params: Vec<GenericParameterInfo>,
    implementations: Vec<Implementation>,
    /// Definições nominais completas (campos, variantes, métodos) — sem AST,
    /// prontas para downstream (§767-770).
    definitions: HashMap<NominalTypeId, TypeDefinition>,
    /// Type aliases por symbol. `raw` é o type resolvido sem expandir aliases.
    aliases: HashMap<nexa_symbols::SymbolId, AliasEntry>,
    /// Profundidade estrutural máxima permitida para tipos compostos (§651).
    max_type_depth: u32,
}

#[derive(Debug, Clone)]
struct AliasEntry {
    raw: TypeId,
    state: AliasResolutionState,
}

impl TypeStore {
    pub fn new() -> Self {
        TypeStore {
            types: Vec::new(),
            interned: HashMap::new(),
            nominals: Vec::new(),
            nominal_to_type: HashMap::new(),
            generic_params: Vec::new(),
            implementations: Vec::new(),
            definitions: HashMap::new(),
            aliases: HashMap::new(),
            max_type_depth: 512,
        }
    }

    /// Limite de profundidade estrutural de tipos compostos (§651).
    pub fn set_max_type_depth(&mut self, depth: u32) {
        self.max_type_depth = depth;
    }

    pub fn max_type_depth(&self) -> u32 {
        self.max_type_depth
    }

    // ─── Definitions (sem AST, para downstream) ─────────────────────

    pub fn register_nominal_definition(&mut self, nid: NominalTypeId, def: TypeDefinition) {
        self.definitions.insert(nid, def);
    }

    pub fn get_nominal_definition(&self, nid: NominalTypeId) -> Option<&TypeDefinition> {
        self.definitions.get(&nid)
    }

    // ─── Aliases / Distinct ─────────────────────────────────────────

    /// Declara `symbol` como alias (ou distinct) de `target`.
    ///
    /// A expansão de aliases-DEH aliases e a detecção de ciclo são
    /// responsabilidade do type checker (pilha de visitados durante a
    /// resolução de type syntax, §98, §331). O store guarda o alvo.
    pub fn declare_alias(&mut self, symbol: nexa_symbols::SymbolId, target: TypeId) {
        self.aliases.insert(
            symbol,
            AliasEntry {
                raw: target,
                state: AliasResolutionState::Resolved,
            },
        );
    }

    /// Alvo (já expandido pelo type checker) de um alias symbol.
    pub fn alias_target(&self, symbol: nexa_symbols::SymbolId) -> Option<TypeId> {
        self.aliases.get(&symbol).map(|e| e.raw)
    }

    pub fn is_alias(&self, symbol: nexa_symbols::SymbolId) -> bool {
        self.aliases.contains_key(&symbol)
    }

    /// Marca um alias como em estado de erro (ciclo detectado no type checker).
    pub fn set_alias_error(&mut self, symbol: nexa_symbols::SymbolId) {
        if let Some(e) = self.aliases.get_mut(&symbol) {
            e.state = AliasResolutionState::Error;
        }
    }

    pub fn alias_is_error(&self, symbol: nexa_symbols::SymbolId) -> bool {
        self.aliases
            .get(&symbol)
            .map(|e| e.state == AliasResolutionState::Error)
            .unwrap_or(false)
    }

    // ─── Generic substitution (§109) ────────────────────────────────

    /// Substitui `GenericParameter` por tipos concretos no `map`.
    pub fn substitute_type(
        &mut self,
        ty: TypeId,
        map: &std::collections::HashMap<GenericParamId, TypeId>,
    ) -> TypeId {
        if map.is_empty() {
            return ty;
        }
        let substituted = match self.get_type(ty).cloned() {
            None => return ty,
            Some(Type::GenericParameter(gp)) => match map.get(&gp) {
                Some(&t) => return t,
                None => return ty,
            },
            Some(Type::Ref(inner)) => Type::Ref(self.substitute_type(inner, map)),
            Some(Type::MutRef(inner)) => Type::MutRef(self.substitute_type(inner, map)),
            Some(Type::Array(inner)) => Type::Array(self.substitute_type(inner, map)),
            Some(Type::Task(inner)) => Type::Task(self.substitute_type(inner, map)),
            Some(Type::Applied { base, arguments }) => {
                let args: Vec<TypeId> = arguments
                    .iter()
                    .map(|a| self.substitute_type(*a, map))
                    .collect();
                Type::Applied {
                    base,
                    arguments: args,
                }
            }
            Some(Type::Callable(ct)) => Type::Callable(CallableType {
                kind: ct.kind,
                parameters: ct
                    .parameters
                    .iter()
                    .map(|p| self.substitute_type(*p, map))
                    .collect(),
                return_type: self.substitute_type(ct.return_type, map),
                generic_params: ct.generic_params.clone(),
            }),
            Some(other) => other,
        };
        self.intern_type(substituted)
    }

    // ─── Interface lookup ───────────────────────────────────────────

    /// Retorna o `ImplementationId` cujo target é `ty` (ou nominal de `ty`) e
    /// cuja interface é `interface` (comparando applied vs nominal).
    pub fn has_interface(&self, ty: TypeId, interface: TypeId) -> Option<ImplementationId> {
        for (id, impl_record) in self.implementations.iter().enumerate() {
            let target_matches = self.types_match_for_impl(impl_record.target, ty);
            if !target_matches {
                continue;
            }
            if let Some(ifc) = impl_record.interface {
                if self.interface_matches(ifc, interface) {
                    return Some(ImplementationId(id as u32));
                }
            }
        }
        None
    }

    /// Par exato: `true` se já existe um `implement` para exatamente o mesmo
    /// (interface, target) — mesmos `TypeId`s, sem normalização nominal.
    /// Distingue a re-declaração idêntica (§379, NEXA-TYPE-0027) da sobreposição
    /// de padrões não-idênticos (§381-384, NEXA-TYPE-0042).
    pub fn has_exact_implementation(&self, target: TypeId, interface: TypeId) -> bool {
        self.implementations
            .iter()
            .any(|r| r.interface == Some(interface) && r.target == target)
    }

    fn types_match_for_impl(&self, a: TypeId, b: TypeId) -> bool {
        if a == b {
            return true;
        }
        // Nominal vs Applied<Nominal, []>: considera "mesmo alvo" se mesmo nominal.
        let nominal_of = |t: TypeId| -> Option<NominalTypeId> {
            match self.get_type(t) {
                Some(Type::Nominal(nid)) => Some(*nid),
                Some(Type::Applied { base, .. }) => match self.get_type(*base) {
                    Some(Type::Nominal(nid)) => Some(*nid),
                    _ => None,
                },
                _ => None,
            }
        };
        nominal_of(a) == nominal_of(b)
    }

    fn interface_matches(&self, a: TypeId, b: TypeId) -> bool {
        // Igualdade exata, ou aplicados do mesmo nominal com mesmos args
        // (interface genérica instanciada de forma idêntica).
        if a == b {
            return true;
        }
        match (self.get_type(a), self.get_type(b)) {
            (
                Some(Type::Applied {
                    base: ab,
                    arguments: aa,
                }),
                Some(Type::Applied {
                    base: bb,
                    arguments: ba,
                }),
            ) => ab == bb && aa == ba,
            _ => false,
        }
    }

    /// Encontra TODAS as interfaces satisfeitas por `ty`, para busca de métodos.
    pub fn interfaces_of(&self, ty: TypeId) -> Vec<(TypeId, ImplementationId)> {
        let mut out = Vec::new();
        for (id, impl_record) in self.implementations.iter().enumerate() {
            if self.types_match_for_impl(impl_record.target, ty) {
                if let Some(ifc) = impl_record.interface {
                    out.push((ifc, ImplementationId(id as u32)));
                }
            }
        }
        out
    }

    /// Intern a type. For primitives, deduplicates by discriminant.
    /// For composites (Ref, Applied, etc.), deduplicates by TypeKey.
    pub fn intern_type(&mut self, ty: Type) -> TypeId {
        match &ty {
            Type::Error
            | Type::Unit
            | Type::Never
            | Type::Bool
            | Type::Int
            | Type::UInt
            | Type::Int8
            | Type::Int16
            | Type::Int32
            | Type::Int64
            | Type::UInt8
            | Type::UInt16
            | Type::UInt32
            | Type::UInt64
            | Type::Float32
            | Type::Float64
            | Type::Byte
            | Type::Char
            | Type::String
            | Type::Bytes => self.find_or_insert_primitive(&ty),
            Type::Nominal(_nid) => {
                // Nominals are created via create_nominal, not interned directly.
                let id = self.types.len() as u32;
                self.types.push(ty);
                TypeId(id)
            }
            Type::Ref(inner) => {
                let key = TypeKey::Ref(*inner);
                self.intern_with_key(key, ty)
            }
            Type::MutRef(inner) => {
                let key = TypeKey::MutRef(*inner);
                self.intern_with_key(key, ty)
            }
            Type::Array(inner) => {
                let key = TypeKey::Array(*inner);
                self.intern_with_key(key, ty)
            }
            Type::Task(inner) => {
                let key = TypeKey::Task(*inner);
                self.intern_with_key(key, ty)
            }
            Type::Applied { base, arguments } => {
                let key = TypeKey::Applied {
                    base: *base,
                    args: arguments.clone(),
                };
                self.intern_with_key(key, ty)
            }
            Type::Callable(ct) => {
                let key = TypeKey::Callable(ct.clone());
                self.intern_with_key(key, ty)
            }
            Type::GenericParameter(gpid) => {
                let key = TypeKey::GenericParameter(*gpid);
                self.intern_with_key(key, ty)
            }
        }
    }

    fn intern_with_key(&mut self, key: TypeKey, ty: Type) -> TypeId {
        if let Some(&existing) = self.interned.get(&key) {
            return existing;
        }
        let id = TypeId(self.types.len() as u32);
        self.types.push(ty);
        self.interned.insert(key, id);
        id
    }

    fn find_or_insert_primitive(&mut self, ty: &Type) -> TypeId {
        for (i, existing) in self.types.iter().enumerate() {
            if std::mem::discriminant(existing) == std::mem::discriminant(ty) {
                return TypeId(i as u32);
            }
        }
        let id = TypeId(self.types.len() as u32);
        self.types.push(ty.clone());
        id
    }

    /// Look up a type by TypeId.
    pub fn get_type(&self, id: TypeId) -> Option<&Type> {
        self.types.get(id.0 as usize)
    }

    /// Register a nominal type and return both TypeId and NominalTypeId.
    pub fn create_nominal(&mut self, nominal: NominalType) -> (TypeId, NominalTypeId) {
        let nid = NominalTypeId(self.nominals.len() as u32);
        self.nominals.push(nominal);
        let tid = self.intern_type(Type::Nominal(nid));
        self.nominal_to_type.insert(nid, tid);
        (tid, nid)
    }

    /// Look up nominal type metadata.
    pub fn get_nominal(&self, id: NominalTypeId) -> Option<&NominalType> {
        self.nominals.get(id.0 as usize)
    }

    /// Convert a NominalTypeId to its TypeId.
    pub fn nominal_type_id(&self, nid: NominalTypeId) -> Option<TypeId> {
        self.nominal_to_type.get(&nid).copied()
    }

    /// Register a generic parameter.
    pub fn create_generic_param(&mut self, info: GenericParameterInfo) -> GenericParamId {
        let gpid = GenericParamId(self.generic_params.len() as u32);
        self.generic_params.push(info);
        gpid
    }

    /// Look up generic parameter info.
    pub fn get_generic_param(&self, id: GenericParamId) -> Option<&GenericParameterInfo> {
        self.generic_params.get(id.0 as usize)
    }

    /// Constraints registradas para um parâmetro genérico.
    pub fn get_generic_param_constraints(&self, id: GenericParamId) -> Vec<Constraint> {
        self.generic_params
            .get(id.0 as usize)
            .map(|i| i.constraints.clone())
            .unwrap_or_default()
    }

    /// Define as constraints de um parâmetro genérico (bounds inline e `where`).
    pub fn set_generic_param_constraints(
        &mut self,
        id: GenericParamId,
        constraints: Vec<Constraint>,
    ) {
        if let Some(info) = self.generic_params.get_mut(id.0 as usize) {
            info.constraints = constraints;
        }
    }

    /// Intern a generic application `Base<A1, A2, ...>`.
    pub fn create_applied(&mut self, base: TypeId, arguments: Vec<TypeId>) -> TypeId {
        self.intern_type(Type::Applied { base, arguments })
    }

    /// Register an implementation record.
    pub fn register_implementation(&mut self, impl_record: Implementation) -> ImplementationId {
        let id = ImplementationId(self.implementations.len() as u32);
        self.implementations.push(impl_record);
        id
    }

    /// Look up an implementation record.
    pub fn get_implementation(&self, id: ImplementationId) -> Option<&Implementation> {
        self.implementations.get(id.0 as usize)
    }

    /// Check if this type uses indirect storage (breaks layout recursion).
    pub fn is_indirect_storage(&self, id: TypeId) -> bool {
        match self.get_type(id) {
            Some(Type::Ref(_)) | Some(Type::MutRef(_)) | Some(Type::Array(_)) => true,
            Some(Type::Applied { base, .. }) => self
                .get_type(*base)
                .is_some_and(|b| matches!(b, Type::Array(_) | Type::Task(_))),
            _ => false,
        }
    }

    /// Layout recursion (§87-97, §654-658): `true` se o nominal `nid` contém
    /// (via storage owned, sem passar por contêiner indireto) o nominal `target`.
    /// Usado para rejeitar `struct Node { next: Node }` (layout infinito).
    pub fn nominal_contains(
        &self,
        from: NominalTypeId,
        target: NominalTypeId,
        guard: &mut std::collections::HashSet<(NominalTypeId, NominalTypeId)>,
    ) -> bool {
        if !guard.insert((from, target)) {
            return false;
        }
        let Some(def) = self.get_nominal_definition(from) else {
            return false;
        };
        let mut leaves = Vec::new();
        TypeStore::def_leaves(def, &mut leaves);
        for leaf in leaves {
            if self.type_contains_nominal(leaf, target, guard) {
                return true;
            }
        }
        false
    }

    fn type_contains_nominal(
        &self,
        ty: TypeId,
        target: NominalTypeId,
        guard: &mut std::collections::HashSet<(NominalTypeId, NominalTypeId)>,
    ) -> bool {
        if self.is_indirect_storage(ty) {
            return false;
        }
        match self.get_type(ty) {
            None => false,
            Some(Type::Nominal(nid)) => {
                if *nid == target {
                    true
                } else {
                    self.nominal_contains(*nid, target, guard)
                }
            }
            Some(Type::Applied { base, arguments }) => {
                for a in arguments {
                    if self.type_contains_nominal(*a, target, guard) {
                        return true;
                    }
                }
                match self.get_type(*base) {
                    Some(Type::Nominal(nid)) => {
                        if *nid == target {
                            true
                        } else {
                            self.nominal_contains(*nid, target, guard)
                        }
                    }
                    _ => false,
                }
            }
            Some(Type::Callable(c)) => {
                c.parameters
                    .iter()
                    .any(|p| self.type_contains_nominal(*p, target, guard))
                    || self.type_contains_nominal(c.return_type, target, guard)
            }
            _ => false,
        }
    }

    fn def_leaves(def: &TypeDefinition, out: &mut Vec<TypeId>) {
        match def {
            TypeDefinition::Struct(s) => out.extend(s.fields.iter().map(|f| f.ty)),
            TypeDefinition::Enum(e) => {
                for v in &e.variants {
                    match &v.kind {
                        VariantKind::Unit => {}
                        VariantKind::Tuple(ts) => out.extend(ts.iter().copied()),
                        VariantKind::Struct(fs) => out.extend(fs.iter().map(|f| f.ty)),
                    }
                }
            }
            TypeDefinition::Interface(_i) => {}
            TypeDefinition::Distinct(d) => out.push(d.base),
        }
    }

    pub fn type_count(&self) -> usize {
        self.types.len()
    }

    pub fn nominal_count(&self) -> usize {
        self.nominals.len()
    }

    pub fn generic_param_count(&self) -> usize {
        self.generic_params.len()
    }

    pub fn implementation_count(&self) -> usize {
        self.implementations.len()
    }

    /// Find the nominal type that wraps a given symbol, if any.
    pub fn find_nominal_by_symbol(&self, symbol: SymbolId) -> Option<(TypeId, &NominalType)> {
        for (nid, nominal) in self.nominals.iter().enumerate() {
            if nominal.symbol == symbol {
                let nid = NominalTypeId(nid as u32);
                if let Some(&tid) = self.nominal_to_type.get(&nid) {
                    return Some((tid, nominal));
                }
            }
        }
        None
    }
}

impl Default for TypeStore {
    fn default() -> Self {
        Self::new()
    }
}
