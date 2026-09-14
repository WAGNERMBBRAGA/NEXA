pub mod diagnostics;
pub mod model;
pub mod typecheck;

pub use diagnostics::{TypeDiagnostic, TypeDiagnosticCode};
pub use model::{DeclarationTypeInfo, ExprInfo, TypedSemanticModel};
pub use typecheck::TypeChecker;

#[cfg(test)]
mod integration {
    use super::*;
    use nexa_source::{SourceFile, SourceId};
    use nexa_symbols::ModuleId;

    fn check_module(text: &str) -> TypeChecker {
        let sf = SourceFile::from_text(
            SourceId(0),
            std::path::PathBuf::from("<test>.nexa"),
            text.to_string(),
        );
        let parsed = nexa_parser::parse(&sf, nexa_parser::ParseMode::SingleFile);
        assert!(
            !parsed.has_errors(),
            "parser produced errors for module:\n{:?}",
            parsed.diagnostics
        );
        let resolved = nexa_resolver::resolve(sf.id, &parsed.ast);
        assert!(
            resolved.diagnostics.is_empty(),
            "resolver produced errors:\n{:?}",
            resolved.diagnostics
        );
        let mut tc = TypeChecker::with_index(resolved.index, ModuleId(0), sf.id);
        tc.check_module(&parsed.ast);
        tc
    }

    fn type_diagnostics(tc: &TypeChecker) -> Vec<String> {
        tc.diagnostics
            .iter()
            .map(|d| format!("{:?}: {}", d.code, d.message))
            .collect()
    }

    #[test]
    fn module_functions_match_and_const() {
        let tc = check_module(
            r#"module main

const MAX_LIMIT: Int = 100

enum Color {
    Red
    Green
    Blue
}

function describe_color(c: Color) -> String {
    return match c {
        Color::Red => "red"
        Color::Green => "green"
        Color::Blue => "blue"
    }
}

function area(width: Int, height: Int) -> Int {
    let w = width
    let h: Int = height
    if w == h {
        return w * h
    }
    return w + h
}
"#,
        );
        assert!(
            tc.diagnostics.is_empty(),
            "unexpected type diagnostics:\n{}",
            type_diagnostics(&tc).join("\n")
        );
        assert!(
            tc.semantic.expression_info.count() > 0,
            "expression side table should be populated"
        );
    }

    #[test]
    fn module_interface_and_impl_methods() {
        let tc = check_module(
            r#"module main

interface Describe {
    describe() -> String
    prefix(pre: String) -> String
}

enum Shape {
    Circle
    Square
}

implement Describe for Shape {
    describe() -> String {
        return "shape"
    }
    prefix(pre: String) -> String {
        return pre
    }
}

function label(s: Shape) -> String {
    return s.describe()
}

function decorate(s: Shape) -> String {
    return s.prefix("item: ")
}
"#,
        );
        assert!(
            tc.diagnostics.is_empty(),
            "unexpected type diagnostics:\n{}",
            type_diagnostics(&tc).join("\n")
        );
        assert!(
            tc.semantic.callable_signatures.count() >= 2,
            "function signatures should be recorded"
        );
    }

    #[test]
    fn module_optional_binding() {
        let tc = check_module(
            r#"module main

function default_if_none(x: Optional<Int>) -> Int {
    let v = x
    return 1
}

function make() -> Optional<Int> {
    return Some(10)
}
"#,
        );
        assert!(
            tc.diagnostics.is_empty(),
            "unexpected type diagnostics:\n{}",
            type_diagnostics(&tc).join("\n")
        );
    }

    #[test]
    fn module_struct_literal_and_shorthand() {
        let tc = check_module(
            r#"module main

struct Point {
    x: Int
    y: Int
}

struct User {
    id: Int
    name: String
}

function step(p: Point) -> Point {
    return Point { x: p.x + 1, y: p.y + 1 }
}

function main() -> Unit {
    let o = Point { x: 10, y: 20 }
    let u = User { id: 7, name: "ada" }
    let id: Int = 3
    let name: String = "b"
    let shorthand = User { id, name }
    let total: Int = o.x + o.y + shorthand.id + u.id
}
"#,
        );
        assert!(
            tc.diagnostics.is_empty(),
            "unexpected type diagnostics:\n{}",
            type_diagnostics(&tc).join("\n")
        );
    }

    #[test]
    fn module_struct_literal_missing_unknown_duplicate() {
        let tc = check_module(
            r#"module main

struct Point {
    x: Int
    y: Int
}

function missing() -> Point {
    return Point { x: 1 }
}

function unknown() -> Point {
    return Point { x: 1, y: 2, extra: 3 }
}

function duplicate() -> Point {
    return Point { x: 1, x: 2, y: 3 }
}
"#,
        );
        let did = |code: TypeDiagnosticCode| {
            tc.diagnostics
                .iter()
                .any(|d| std::mem::discriminant(&d.code) == std::mem::discriminant(&code))
        };
        assert!(
            did(TypeDiagnosticCode::MissingStructField),
            "missing field should be reported"
        );
        assert!(
            did(TypeDiagnosticCode::UnknownStructField),
            "unknown field should be reported"
        );
        assert!(
            did(TypeDiagnosticCode::DuplicateStructField),
            "duplicate field should be reported"
        );
    }

    #[test]
    fn module_generic_function_inference() {
        let tc = check_module(
            r#"module main

function identity<T>(x: T) -> T {
    return x
}

function first<T>(a: T, b: T) -> T {
    return a
}

function main() -> Unit {
    let n: Int = identity(41)
    let s: String = identity("hi")
    let a: Int = first(1, 2)
    let b: String = first("x", "y")
}
"#,
        );
        assert!(
            tc.diagnostics.is_empty(),
            "unexpected type diagnostics:\n{}",
            type_diagnostics(&tc).join("\n")
        );
    }

    #[test]
    fn module_methods_receiver_and_interface_dispatch() {
        let tc = check_module(
            r#"module main

struct Rect {
    width: Int
    height: Int
}

interface Shape {
    area() -> Int
    scale(factor: Int) -> Shape
}

implement Shape for Rect {
    area(self) -> Int {
        return self.width * self.height
    }
    scale(self, factor: Int) -> Shape {
        return Rect { width: self.width * factor, height: self.height * factor }
    }
}

function measure(r: Rect) -> Int {
    let s: Shape = r.scale(2)
    return s.area()
}

function main() -> Unit {
    let r = Rect { width: 3, height: 4 }
    let total: Int = r.area() + measure(r)
}
"#,
        );
        assert!(
            tc.diagnostics.is_empty(),
            "unexpected type diagnostics:\n{}",
            type_diagnostics(&tc).join("\n")
        );
    }

    #[test]
    fn module_interface_impl_validation() {
        let happy = check_module(
            r#"module main

interface Stringify {
    to_string() -> String
    describe(prefix: String) -> String
}

struct Widget {
    tag: String
}

implement Stringify for Widget {
    to_string(self) -> String {
        return self.tag
    }
    describe(self, prefix: String) -> String {
        return prefix
    }
}
"#,
        );
        assert!(
            happy.diagnostics.is_empty(),
            "valid implement must produce no diagnostics:\n{}",
            type_diagnostics(&happy).join("\n")
        );

        // Membro da interface sem correspondente no implement → 0025.
        let missing = check_module(
            r#"module main

interface Greeter {
    greet() -> String
    farewell() -> String
}

struct Person {
    name: String
}

implement Greeter for Person {
    greet(self) -> String {
        return "hello"
    }
}
"#,
        );
        let has = |code: TypeDiagnosticCode| {
            missing
                .diagnostics
                .iter()
                .any(|d| std::mem::discriminant(&d.code) == std::mem::discriminant(&code))
        };
        assert!(
            has(TypeDiagnosticCode::MissingInterfaceMember),
            "missing interface member must be reported:\n{}",
            type_diagnostics(&missing).join("\n")
        );

        // Assinatura incompatível com a interface → 0026.
        let wrong_sig = check_module(
            r#"module main

interface Greeter {
    greet() -> String
}

struct Person {
    name: String
}

implement Greeter for Person {
    greet(self) -> Int {
        return 7
    }
}
"#,
        );
        let has = |code: TypeDiagnosticCode| {
            wrong_sig
                .diagnostics
                .iter()
                .any(|d| std::mem::discriminant(&d.code) == std::mem::discriminant(&code))
        };
        assert!(
            has(TypeDiagnosticCode::IncompatibleInterfaceMember),
            "incompatible member signature must be reported:\n{}",
            type_diagnostics(&wrong_sig).join("\n")
        );
    }

    #[test]
    fn module_generic_constraints() {
        // Constraint `where T: Show` satisfeita: bound registra, o corpo
        // dispatcha via constraint e a chamada concreta implementa → 0 erros.
        let happy = check_module(
            r#"module main

interface Show {
    show() -> String
}

struct Item {
    id: Int
}

implement Show for Item {
    show(self) -> String {
        return "item"
    }
}

function describe<T>(x: T) -> String
where T: Show
{
    return x.show()
}

function main() -> String {
    let it = Item { id: 1 }
    return describe(it)
}
"#,
        );
        assert!(
            happy.diagnostics.is_empty(),
            "valid constraint usage must produce no diagnostics:\n{}",
            type_diagnostics(&happy).join("\n")
        );

        // Chamada com tipo que não implementa o bound → 0004.
        let unsatisfied = check_module(
            r#"module main

interface Show {
    show() -> String
}

struct Raw {
    value: Int
}

function describe<T>(x: T) -> String
where T: Show
{
    return x.show()
}

function main() -> String {
    let r = Raw { value: 1 }
    return describe(r)
}
"#,
        );
        let has = |code: TypeDiagnosticCode| {
            unsatisfied
                .diagnostics
                .iter()
                .any(|d| std::mem::discriminant(&d.code) == std::mem::discriminant(&code))
        };
        assert!(
            has(TypeDiagnosticCode::GenericConstraintNotSatisfied),
            "unsatisfied constraint must be reported:\n{}",
            type_diagnostics(&unsatisfied).join("\n")
        );

        // Bound que não resolve para interface → 0032.
        let not_iface = check_module(
            r#"module main

struct Point {
    x: Int
    y: Int
}

function describe<T>(x: T) -> Int
where T: Point
{
    return x.x
}

function main() -> Int {
    return 0
}
"#,
        );
        let has = |code: TypeDiagnosticCode| {
            not_iface
                .diagnostics
                .iter()
                .any(|d| std::mem::discriminant(&d.code) == std::mem::discriminant(&code))
        };
        assert!(
            has(TypeDiagnosticCode::ConstraintMustBeInterface),
            "non-interface bound must be reported:\n{}",
            type_diagnostics(&not_iface).join("\n")
        );
    }

    #[test]
    fn module_generic_argument_arity() {
        let wrong_arity = check_module(
            r#"module main

struct Box<T> {
    value: T
}

function id(b: Box<Int, String>) -> Int {
    return 0
}
"#,
        );
        let has = |code: TypeDiagnosticCode| {
            wrong_arity
                .diagnostics
                .iter()
                .any(|d| std::mem::discriminant(&d.code) == std::mem::discriminant(&code))
        };
        assert!(
            has(TypeDiagnosticCode::GenericArgumentCountMismatch),
            "wrong generic arity must be reported:\n{}",
            type_diagnostics(&wrong_arity).join("\n")
        );
    }

    #[test]
    fn module_implicit_integer_conversion() {
        let m = check_module(
            r#"module main

function main() -> Int {
    let x: Int32 = 5
    let y: Int64 = x
    return 0
}
"#,
        );
        let has = |code: TypeDiagnosticCode| {
            m.diagnostics
                .iter()
                .any(|d| std::mem::discriminant(&d.code) == std::mem::discriminant(&code))
        };
        assert!(
            has(TypeDiagnosticCode::InvalidImplicitConversion),
            "Int32→Int64 must be 0003, not plain mismatch:\n{}",
            type_diagnostics(&m).join("\n")
        );
    }

    #[test]
    fn module_distinct_identity_no_implicit_conversion() {
        // distinct preserva identidade nominal (§33-39): bind base↔distinct é 0006.
        let m = check_module(
            r#"module main

type Meters distinct = Int

function main() -> Int {
    let m: Meters = 5
    let i: Int = m
    return 0
}
"#,
        );
        let has = |code: TypeDiagnosticCode| {
            m.diagnostics
                .iter()
                .any(|d| std::mem::discriminant(&d.code) == std::mem::discriminant(&code))
        };
        assert!(
            has(TypeDiagnosticCode::InvalidDistinctTypeUse),
            "distinct/base implicit bind must be 0006:\n{}",
            type_diagnostics(&m).join("\n")
        );

        // Declaração de distinct sem uso → sem erros (baseline CTS-TYPE-0011).
        let ok = check_module(
            r#"module main

type UserId distinct = Int

function main() -> Int {
    return 0
}
"#,
        );
        assert!(
            ok.diagnostics.is_empty(),
            "distinct declaration with no implicit use is fine:\n{}",
            type_diagnostics(&ok).join("\n")
        );
    }

    #[test]
    fn module_public_const_requires_explicit_type() {
        // const público/exportado sem tipo explícito → 0043 (§467-469).
        let missing = check_module(
            r#"module main

export const Max = 10
"#,
        );
        let has = |code: TypeDiagnosticCode| {
            missing
                .diagnostics
                .iter()
                .any(|d| std::mem::discriminant(&d.code) == std::mem::discriminant(&code))
        };
        assert!(
            has(TypeDiagnosticCode::PublicConstRequiresExplicitType),
            "export const without type must be 0043:\n{}",
            type_diagnostics(&missing).join("\n")
        );

        // const local sem tipo (inferência) → sem erro.
        let local_ok = check_module(
            r#"module main

const Max = 10
"#,
        );
        assert!(
            local_ok.diagnostics.is_empty(),
            "local const with inferred type is fine:\n{}",
            type_diagnostics(&local_ok).join("\n")
        );
    }

    #[test]
    fn module_public_api_visibility() {
        // API pública consistente (tudo exportado) → sem diagnóstico.
        let happy = check_module(
            r#"module main

export struct Item {
    id: Int
}

export function get_item() -> Item {
    return Item { id: 1 }
}

export struct Inner {
    name: String
}

export struct Outer {
    inner: Inner
}

export enum Shape {
    Circle(Inner)
    Square
}
"#,
        );
        assert!(
            happy.diagnostics.is_empty(),
            "consistent public API must produce no diagnostics:\n{}",
            type_diagnostics(&happy).join("\n")
        );

        // Função exportada retornando tipo module-private → 0044.
        let fn_leak = check_module(
            r#"module main

struct Secret {
    code: Int
}

export function leak() -> Secret {
    return Secret { code: 0 }
}

function main() -> Int {
    return 0
}
"#,
        );
        let has = |code: TypeDiagnosticCode| {
            fn_leak
                .diagnostics
                .iter()
                .any(|d| std::mem::discriminant(&d.code) == std::mem::discriminant(&code))
        };
        assert!(
            has(TypeDiagnosticCode::PublicApiExposesNonPublicType),
            "function exposing a private type must be reported:\n{}",
            type_diagnostics(&fn_leak).join("\n")
        );

        // Struct exportada com campo de tipo module-private → 0044.
        let field_leak = check_module(
            r#"module main

struct Inner {
    name: String
}

export struct Outer {
    inner: Inner
}
"#,
        );
        let has = |code: TypeDiagnosticCode| {
            field_leak
                .diagnostics
                .iter()
                .any(|d| std::mem::discriminant(&d.code) == std::mem::discriminant(&code))
        };
        assert!(
            has(TypeDiagnosticCode::PublicApiExposesNonPublicType),
            "public struct with a private field type must be reported:\n{}",
            type_diagnostics(&field_leak).join("\n")
        );
    }

    #[test]
    fn module_recursive_type_cycles() {
        // Recursividade indireta via `ref` quebra o ciclo de layout → ok.
        let ref_ok = check_module(
            r#"module main

struct Node {
    next: ref Node
}

function main() -> Int {
    return 0
}
"#,
        );
        assert!(
            ref_ok.diagnostics.is_empty(),
            "indirect recursion via ref must be accepted:\n{}",
            type_diagnostics(&ref_ok).join("\n")
        );

        // Tipo distinto sem auto-referência → ok.
        let distinct_ok = check_module(
            r#"module main

type Meters distinct = Int

function main() -> Int {
    return 0
}
"#,
        );
        assert!(
            distinct_ok.diagnostics.is_empty(),
            "distinct type without self-reference must be accepted:\n{}",
            type_diagnostics(&distinct_ok).join("\n")
        );

        // Aliases sem ciclo → ok.
        let alias_ok = check_module(
            r#"module main

type A = B
type B = Int

function main() -> Int {
    return 0
}
"#,
        );
        assert!(
            alias_ok.diagnostics.is_empty(),
            "transparent alias chain must be accepted:\n{}",
            type_diagnostics(&alias_ok).join("\n")
        );

        let has = |m: &TypeChecker, code: TypeDiagnosticCode| {
            m.diagnostics
                .iter()
                .any(|d| std::mem::discriminant(&d.code) == std::mem::discriminant(&code))
        };

        // struct recursiva por valor → 0030.
        let layout = check_module(
            r#"module main

struct Node {
    next: Node
}

function main() -> Int {
    return 0
}
"#,
        );
        assert!(
            has(&layout, TypeDiagnosticCode::RecursiveTypeCycle),
            "value-recursive struct must be reported:\n{}",
            type_diagnostics(&layout).join("\n")
        );

        // Optional inline mantém recorrência de layout → 0030.
        let opt = check_module(
            r#"module main

struct Node {
    next: Optional<Node>
}

function main() -> Int {
    return 0
}
"#,
        );
        assert!(
            has(&opt, TypeDiagnosticCode::RecursiveTypeCycle),
            "Optional inline recursion must remain a layout cycle:\n{}",
            type_diagnostics(&opt).join("\n")
        );

        // Aliases em ciclo → 0030.
        let alias = check_module(
            r#"module main

type A = B
type B = A

function main() -> Int {
    return 0
}
"#,
        );
        assert!(
            has(&alias, TypeDiagnosticCode::RecursiveTypeCycle),
            "alias cycle must be reported:\n{}",
            type_diagnostics(&alias).join("\n")
        );

        // distinct auto-referenciado → 0030.
        let distinct_self = check_module(
            r#"module main

type A distinct = A

function main() -> Int {
    return 0
}
"#,
        );
        assert!(
            has(&distinct_self, TypeDiagnosticCode::RecursiveTypeCycle),
            "self-referencing distinct type must be reported:\n{}",
            type_diagnostics(&distinct_self).join("\n")
        );
    }

    #[test]
    fn module_enum_construction_and_match() {
        // Enum com variantes (payload + unit), match tipado com guard válido,
        // construção via `Type::Variant` → sem diagnóstico.
        let happy = check_module(
            r#"module main

enum Shape {
    Circle(Int)
    Square
}

function classify(s: Shape) -> Int {
    return match s {
        Shape::Circle(r) if r > 0 => r
        Shape::Circle(_) => 0
        Shape::Square => 0
    }
}

function main() -> Int {
    let c = Shape::Circle(5)
    let u = Shape::Square
    return classify(c) + classify(u)
}
"#,
        );
        assert!(
            happy.diagnostics.is_empty(),
            "enum construction + match must produce no diagnostics:\n{}",
            type_diagnostics(&happy).join("\n")
        );

        let has = |m: &TypeChecker, code: TypeDiagnosticCode| {
            m.diagnostics
                .iter()
                .any(|d| std::mem::discriminant(&d.code) == std::mem::discriminant(&code))
        };

        // Payload tipo divergente → 0022.
        let payload = check_module(
            r#"module main

enum Shape {
    Circle(Int)
    Square
}

function main() -> Int {
    let s = Shape::Circle("boom")
    return 0
}
"#,
        );
        assert!(
            has(&payload, TypeDiagnosticCode::InvalidEnumVariantPayload),
            "payload type mismatch must be reported:\n{}",
            type_diagnostics(&payload).join("\n")
        );

        // Arm com tipo divergente dos anteriores → 0023.
        let arm = check_module(
            r#"module main

enum Shape {
    Circle(Int)
    Square
}

function main() -> Int {
    let s = Shape::Circle(1)
    let v = match s {
        Shape::Circle(r) => r
        Shape::Square => "nope"
    }
    return v
}
"#,
        );
        assert!(
            has(&arm, TypeDiagnosticCode::MatchArmTypeMismatch),
            "divergent match arm must be reported:\n{}",
            type_diagnostics(&arm).join("\n")
        );

        // Guard que não é Bool → 0040.
        let guard = check_module(
            r#"module main

enum Shape {
    Circle(Int)
    Square
}

function main() -> Int {
    let s = Shape::Circle(1)
    let v = match s {
        Shape::Circle(r) if 5 => r
        Shape::Square => 0
    }
    return v
}
"#,
        );
        assert!(
            has(&guard, TypeDiagnosticCode::MatchGuardMustBeBool),
            "non-Bool guard must be reported:\n{}",
            type_diagnostics(&guard).join("\n")
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexa_source::{SourceId, SourceSpan};
    use nexa_types::ty;

    fn dummy_span() -> SourceSpan {
        SourceSpan::new(SourceId(0), 0, 0)
    }

    #[test]
    fn type_checker_creation() {
        let tc = TypeChecker::new();
        assert!(
            tc.store.type_count() >= 19,
            "should bootstrap at least the 19 primitive types (plus nominal prelude)"
        );
        assert!(
            tc.prelude_nominal.optional != tc.prelude_nominal.result,
            "nominal Optional/Result prelude should be distinct"
        );
        assert_eq!(tc.diagnostics.len(), 0);
    }

    #[test]
    fn prelude_types_correct() {
        let tc = TypeChecker::new();
        assert!(nexa_types::same_type(
            &tc.store,
            tc.prelude.int,
            tc.prelude.int
        ));
        assert!(!nexa_types::same_type(
            &tc.store,
            tc.prelude.int,
            tc.prelude.bool
        ));
        assert!(!nexa_types::same_type(
            &tc.store,
            tc.prelude.string,
            tc.prelude.int
        ));
        assert!(nexa_types::same_type(
            &tc.store,
            tc.prelude.unit,
            tc.prelude.unit
        ));
    }

    #[test]
    fn int_literal_default_type() {
        let mut tc = TypeChecker::new();
        let expr = nexa_ast::Expr {
            kind: nexa_ast::ExprKind::IntLiteral(42),
            span: dummy_span(),
        };
        let ty = tc.check_expression(&expr, None);
        assert!(
            nexa_types::same_type(&tc.store, ty, tc.prelude.int),
            "unsuffixed int literal should default to Int"
        );
        assert_eq!(tc.diagnostics.len(), 0);
    }

    #[test]
    fn int_literal_contextual_type() {
        let mut tc = TypeChecker::new();
        let expr = nexa_ast::Expr {
            kind: nexa_ast::ExprKind::IntLiteral(42),
            span: dummy_span(),
        };
        let ty = tc.check_expression(&expr, Some(tc.prelude.int32));
        assert!(
            nexa_types::same_type(&tc.store, ty, tc.prelude.int32),
            "int literal should adopt Int32 from context"
        );
    }

    #[test]
    fn float_literal_default_type() {
        let mut tc = TypeChecker::new();
        let expr = nexa_ast::Expr {
            kind: nexa_ast::ExprKind::FloatLiteral(3.5),
            span: dummy_span(),
        };
        let ty = tc.check_expression(&expr, None);
        assert!(nexa_types::same_type(&tc.store, ty, tc.prelude.float64));
    }

    #[test]
    fn bool_literal_type() {
        let mut tc = TypeChecker::new();
        let expr = nexa_ast::Expr {
            kind: nexa_ast::ExprKind::BoolLiteral(true),
            span: dummy_span(),
        };
        let ty = tc.check_expression(&expr, None);
        assert!(nexa_types::same_type(&tc.store, ty, tc.prelude.bool));
    }

    #[test]
    fn char_literal_type() {
        let mut tc = TypeChecker::new();
        let expr = nexa_ast::Expr {
            kind: nexa_ast::ExprKind::CharLiteral('a'),
            span: dummy_span(),
        };
        let ty = tc.check_expression(&expr, None);
        assert!(nexa_types::same_type(&tc.store, ty, tc.prelude.char));
    }

    #[test]
    fn string_literal_type() {
        let mut tc = TypeChecker::new();
        let expr = nexa_ast::Expr {
            kind: nexa_ast::ExprKind::StringLiteral(nexa_ast::StringLiteral {
                parts: vec![nexa_ast::StringPart::Text("hello".to_string())],
                multiline: false,
            }),
            span: dummy_span(),
        };
        let ty = tc.check_expression(&expr, None);
        assert!(nexa_types::same_type(&tc.store, ty, tc.prelude.string));
    }

    #[test]
    fn binary_int_addition() {
        let mut tc = TypeChecker::new();
        let expr = nexa_ast::Expr {
            kind: nexa_ast::ExprKind::Binary(
                nexa_ast::BinaryOp::Add,
                Box::new(nexa_ast::Expr {
                    kind: nexa_ast::ExprKind::IntLiteral(1),
                    span: dummy_span(),
                }),
                Box::new(nexa_ast::Expr {
                    kind: nexa_ast::ExprKind::IntLiteral(2),
                    span: dummy_span(),
                }),
            ),
            span: dummy_span(),
        };
        let ty = tc.check_expression(&expr, None);
        assert!(nexa_types::same_type(&tc.store, ty, tc.prelude.int));
        assert_eq!(tc.diagnostics.len(), 0);
    }

    #[test]
    fn binary_int_bool_rejected() {
        let mut tc = TypeChecker::new();
        let expr = nexa_ast::Expr {
            kind: nexa_ast::ExprKind::Binary(
                nexa_ast::BinaryOp::Add,
                Box::new(nexa_ast::Expr {
                    kind: nexa_ast::ExprKind::IntLiteral(1),
                    span: dummy_span(),
                }),
                Box::new(nexa_ast::Expr {
                    kind: nexa_ast::ExprKind::BoolLiteral(true),
                    span: dummy_span(),
                }),
            ),
            span: dummy_span(),
        };
        let _ty = tc.check_expression(&expr, None);
        assert_eq!(tc.diagnostics.len(), 1, "should emit error for int + bool");
        assert_eq!(
            tc.diagnostics[0].code,
            TypeDiagnosticCode::InvalidOperatorOperands
        );
    }

    #[test]
    fn logical_and_bool_only() {
        let mut tc = TypeChecker::new();
        let expr = nexa_ast::Expr {
            kind: nexa_ast::ExprKind::Binary(
                nexa_ast::BinaryOp::And,
                Box::new(nexa_ast::Expr {
                    kind: nexa_ast::ExprKind::BoolLiteral(true),
                    span: dummy_span(),
                }),
                Box::new(nexa_ast::Expr {
                    kind: nexa_ast::ExprKind::IntLiteral(1),
                    span: dummy_span(),
                }),
            ),
            span: dummy_span(),
        };
        let _ty = tc.check_expression(&expr, None);
        assert_eq!(tc.diagnostics.len(), 1, "logical AND requires both Bool");
        assert!(nexa_types::same_type(&tc.store, _ty, tc.prelude.unit));
    }

    #[test]
    fn unary_not_bool() {
        let mut tc = TypeChecker::new();
        let expr = nexa_ast::Expr {
            kind: nexa_ast::ExprKind::Unary(
                nexa_ast::UnaryOp::Not,
                Box::new(nexa_ast::Expr {
                    kind: nexa_ast::ExprKind::BoolLiteral(true),
                    span: dummy_span(),
                }),
            ),
            span: dummy_span(),
        };
        let ty = tc.check_expression(&expr, None);
        assert!(nexa_types::same_type(&tc.store, ty, tc.prelude.bool));
        assert_eq!(tc.diagnostics.len(), 0);
    }

    #[test]
    fn unary_not_non_bool_rejected() {
        let mut tc = TypeChecker::new();
        let expr = nexa_ast::Expr {
            kind: nexa_ast::ExprKind::Unary(
                nexa_ast::UnaryOp::Not,
                Box::new(nexa_ast::Expr {
                    kind: nexa_ast::ExprKind::IntLiteral(1),
                    span: dummy_span(),
                }),
            ),
            span: dummy_span(),
        };
        let _ty = tc.check_expression(&expr, None);
        assert_eq!(tc.diagnostics.len(), 1, "! requires Bool");
    }

    #[test]
    fn comparison_returns_bool() {
        let mut tc = TypeChecker::new();
        let expr = nexa_ast::Expr {
            kind: nexa_ast::ExprKind::Binary(
                nexa_ast::BinaryOp::Lt,
                Box::new(nexa_ast::Expr {
                    kind: nexa_ast::ExprKind::IntLiteral(1),
                    span: dummy_span(),
                }),
                Box::new(nexa_ast::Expr {
                    kind: nexa_ast::ExprKind::IntLiteral(2),
                    span: dummy_span(),
                }),
            ),
            span: dummy_span(),
        };
        let ty = tc.check_expression(&expr, None);
        assert!(nexa_types::same_type(&tc.store, ty, tc.prelude.bool));
        assert_eq!(tc.diagnostics.len(), 0);
    }

    #[test]
    fn array_literal_infers_common_type() {
        let mut tc = TypeChecker::new();
        let expr = nexa_ast::Expr {
            kind: nexa_ast::ExprKind::Array(vec![
                nexa_ast::Expr {
                    kind: nexa_ast::ExprKind::IntLiteral(1),
                    span: dummy_span(),
                },
                nexa_ast::Expr {
                    kind: nexa_ast::ExprKind::IntLiteral(2),
                    span: dummy_span(),
                },
                nexa_ast::Expr {
                    kind: nexa_ast::ExprKind::IntLiteral(3),
                    span: dummy_span(),
                },
            ]),
            span: dummy_span(),
        };
        let ty = tc.check_expression(&expr, None);
        assert!(
            matches!(tc.store.get_type(ty), Some(ty::Type::Array(inner)) if nexa_types::same_type(&tc.store, *inner, tc.prelude.int))
        );
    }

    #[test]
    fn array_mixed_types_rejected() {
        let mut tc = TypeChecker::new();
        let expr = nexa_ast::Expr {
            kind: nexa_ast::ExprKind::Array(vec![
                nexa_ast::Expr {
                    kind: nexa_ast::ExprKind::IntLiteral(1),
                    span: dummy_span(),
                },
                nexa_ast::Expr {
                    kind: nexa_ast::ExprKind::BoolLiteral(true),
                    span: dummy_span(),
                },
            ]),
            span: dummy_span(),
        };
        let _ty = tc.check_expression(&expr, None);
        assert_eq!(tc.diagnostics.len(), 1, "mixed array types should error");
    }

    #[test]
    fn parentheses_passthrough() {
        let mut tc = TypeChecker::new();
        let expr = nexa_ast::Expr {
            kind: nexa_ast::ExprKind::Paren(Box::new(nexa_ast::Expr {
                kind: nexa_ast::ExprKind::IntLiteral(42),
                span: dummy_span(),
            })),
            span: dummy_span(),
        };
        let ty = tc.check_expression(&expr, None);
        assert!(nexa_types::same_type(&tc.store, ty, tc.prelude.int));
    }

    #[test]
    fn expression_info_recorded() {
        let mut tc = TypeChecker::new();
        let expr = nexa_ast::Expr {
            kind: nexa_ast::ExprKind::IntLiteral(42),
            span: dummy_span(),
        };
        let ty = tc.check_expression(&expr, None);
        assert_eq!(tc.semantic.expression_info.count(), 1);
        let info = tc.semantic.expression_info.get(dummy_span()).unwrap();
        assert!(nexa_types::same_type(&tc.store, info.ty, ty));
    }

    #[test]
    fn type_syntax_resolution_builtins() {
        let mut tc = TypeChecker::new();
        let empty_map = std::collections::HashMap::new();

        let int_syntax = nexa_ast::Type {
            kind: nexa_ast::TypeKind::Path(nexa_ast::QualifiedName {
                segments: vec![nexa_ast::Ident {
                    name: "Int".to_string(),
                    span: dummy_span(),
                }],
                span: dummy_span(),
            }),
            span: dummy_span(),
        };
        let int_ty = tc.resolve_type_syntax(&int_syntax, &empty_map);
        assert!(nexa_types::same_type(&tc.store, int_ty, tc.prelude.int));

        let bool_syntax = nexa_ast::Type {
            kind: nexa_ast::TypeKind::Path(nexa_ast::QualifiedName {
                segments: vec![nexa_ast::Ident {
                    name: "Bool".to_string(),
                    span: dummy_span(),
                }],
                span: dummy_span(),
            }),
            span: dummy_span(),
        };
        let bool_ty = tc.resolve_type_syntax(&bool_syntax, &empty_map);
        assert!(nexa_types::same_type(&tc.store, bool_ty, tc.prelude.bool));

        let string_syntax = nexa_ast::Type {
            kind: nexa_ast::TypeKind::Path(nexa_ast::QualifiedName {
                segments: vec![nexa_ast::Ident {
                    name: "String".to_string(),
                    span: dummy_span(),
                }],
                span: dummy_span(),
            }),
            span: dummy_span(),
        };
        let string_ty = tc.resolve_type_syntax(&string_syntax, &empty_map);
        assert!(nexa_types::same_type(
            &tc.store,
            string_ty,
            tc.prelude.string
        ));
    }

    #[test]
    fn ref_type_syntax() {
        let mut tc = TypeChecker::new();
        let empty_map = std::collections::HashMap::new();
        let ref_syntax = nexa_ast::Type {
            kind: nexa_ast::TypeKind::Ref(Box::new(nexa_ast::Type {
                kind: nexa_ast::TypeKind::Path(nexa_ast::QualifiedName {
                    segments: vec![nexa_ast::Ident {
                        name: "Int".to_string(),
                        span: dummy_span(),
                    }],
                    span: dummy_span(),
                }),
                span: dummy_span(),
            })),
            span: dummy_span(),
        };
        let ref_ty = tc.resolve_type_syntax(&ref_syntax, &empty_map);
        assert!(
            matches!(tc.store.get_type(ref_ty), Some(ty::Type::Ref(inner)) if nexa_types::same_type(&tc.store, *inner, tc.prelude.int))
        );
    }

    #[test]
    fn array_type_syntax() {
        let mut tc = TypeChecker::new();
        let empty_map = std::collections::HashMap::new();
        let arr_syntax = nexa_ast::Type {
            kind: nexa_ast::TypeKind::Array(Box::new(nexa_ast::Type {
                kind: nexa_ast::TypeKind::Path(nexa_ast::QualifiedName {
                    segments: vec![nexa_ast::Ident {
                        name: "String".to_string(),
                        span: dummy_span(),
                    }],
                    span: dummy_span(),
                }),
                span: dummy_span(),
            })),
            span: dummy_span(),
        };
        let arr_ty = tc.resolve_type_syntax(&arr_syntax, &empty_map);
        assert!(
            matches!(tc.store.get_type(arr_ty), Some(ty::Type::Array(inner)) if nexa_types::same_type(&tc.store, *inner, tc.prelude.string))
        );
    }
}
