use nexa_ast::{Expr, ExprKind, ItemKind, SourceUnit, StmtKind, StructConstructExpr};
use nexa_lexer::lex;
use nexa_parser::{parse, ParseMode};
use nexa_source::{SourceFile, SourceId};

fn sf(text: &str) -> SourceFile {
    SourceFile::from_text(
        SourceId(0),
        std::path::PathBuf::from("<smoke.nexa>"),
        text.to_string(),
    )
}

fn parse_ok(text: &str) -> nexa_parser::ParseResult {
    let source = sf(text);
    parse(&source, ParseMode::SingleFile)
}

#[test]
fn reconstruction_is_lossless() {
    let src = "module m\nlet x: Int = 1\n";
    let r = parse_ok(src);
    assert_eq!(r.reconstruction, src);
}

#[test]
fn module_function_let_parse_clean() {
    let src = "module m\nfunction add(a: Int, b: Int) -> Int {\n    let c: Int = a + b\n    return c\n}\n";
    let r = parse_ok(src);
    assert!(
        !r.has_errors(),
        "unexpected errors: {:?}",
        r.diagnostics
            .iter()
            .map(|d| format!("{} {}", d.code, d.message))
            .collect::<Vec<_>>()
    );
    let unit = &r.ast;
    assert!(unit.module.is_some());
    assert_eq!(unit.imports.len(), 0);
    assert_eq!(unit.items.len(), 1);
}

#[test]
fn import_single_file_mode() {
    let src = "import std::io\nmodule m\nconst N: Int = 10\n";
    let r = parse_ok(src);
    assert!(!r.has_errors());
    let unit = &r.ast;
    assert_eq!(unit.imports.len(), 1);
}

#[test]
fn if_expression_and_binary_precedence() {
    let src = "module m\nfunction f(x: Int) -> Int {\n    let y: Int = x + 2 * 3\n    if y > 5 {\n        return y\n    }\n    return 0\n}\n";
    let r = parse_ok(src);
    assert!(!r.has_errors(), "errors: {:?}", r.diagnostics);
}

#[test]
fn match_expression() {
    let src = "module m\nfunction f(x: Int) -> Bool {\n    match x {\n        0 => true\n        _ => false\n    }\n}\n";
    let r = parse_ok(src);
    assert!(!r.has_errors(), "errors: {:?}", r.diagnostics);
}

#[test]
fn string_with_interpolation() {
    let src =
        "module m\nfunction greet(name: String) -> String {\n    return \"Hello, ${name}!\"\n}\n";
    let r = parse_ok(src);
    assert!(!r.has_errors(), "errors: {:?}", r.diagnostics);
}

#[test]
fn struct_and_optional_types() {
    let src = "module m\nstruct User {\n    name: String\n    age: Int\n}\nfunction make() -> Optional<User> {\n    return create_user()\n}\n";
    let r = parse_ok(src);
    assert!(!r.has_errors(), "errors: {:?}", r.diagnostics);
}

#[test]
fn unexpected_token_reports_diagnostic() {
    let src = "module m\nfunction f( : Int {}\n";
    let r = parse_ok(src);
    assert!(r.has_errors());
    assert!(r
        .diagnostics
        .iter()
        .any(|d| d.code.as_str() == "NEXA-PARSE-0002"));
}

#[test]
fn lexer_lexmes_route_equivalent() {
    let src = "module m\nlet x: Int = 1\n";
    let source = sf(src);
    let lr = lex(&source);
    let via_lexemes = nexa_parser::parse_with_lexemes(&source, &lr.lexemes, ParseMode::SingleFile);
    let via_parse = parse(&source, ParseMode::SingleFile);
    assert_eq!(via_lexemes.has_errors(), via_parse.has_errors());
    assert_eq!(via_lexemes.ast.items.len(), via_parse.ast.items.len());
}

#[test]
fn brace_recovery_does_not_swallow_following_declaration() {
    // §464-465: `function broken( -> Int {` não deve engolir o corpo de
    // `function good()` abaixo.
    let src = "module m\nfunction broken( -> Int {\n    return 1\n}\nfunction good() -> Int {\n    return 2\n}\n";
    let r = parse_ok(src);
    assert!(r.has_errors(), "broken function must diagnose");
    assert_eq!(
        r.ast.items.len(),
        2,
        "recovery must still see both top-level functions"
    );
    let ItemKind::Function(good) = &r.ast.items[1].kind else {
        panic!("expected a function as second item");
    };
    assert_eq!(good.name.name, "good");
}

#[test]
fn import_after_declaration_is_rejected() {
    // §418-419: import após qualquer declaração de topo (todos os modos).
    let src = "module m\nfunction f( -> Int {\n    return 1\n}\nimport std::io\n";
    let r = parse_ok(src);
    assert!(r
        .diagnostics
        .iter()
        .any(|d| d.code.as_str() == "NEXA-PARSE-0011"));
}

#[test]
fn consecutive_imports_in_single_file_are_allowed() {
    // §425: SingleFile aceita imports em cadeia sem `module`.
    let src = "import std::io\nimport std::log\nconst N: Int = 1\n";
    let r = parse_ok(src);
    assert!(!r.has_errors(), "errors: {:?}", r.diagnostics);
    assert_eq!(r.ast.imports.len(), 2);
}

#[test]
fn struct_missing_brace_does_not_swallow_following_declaration() {
    // §465: `struct` sem `}` final não deve engolir o próximo `function`.
    let src = "module m\nstruct Point {\n    x: Int\n    y: Int\nfunction origin() -> Point {\n    return Point\n}\n";
    let r = parse_ok(src);
    assert!(r.has_errors(), "unterminated struct must diagnose");
    let ItemKind::Function(origin) = &r.ast.items[r.ast.items.len() - 1].kind else {
        panic!("expected the last item to be a function");
    };
    assert_eq!(origin.name.name, "origin");
    assert_eq!(r.ast.items.len(), 2, "struct + function must both be items");
}

fn collect_struct_constructs<'a>(expr: &'a Expr, out: &mut Vec<&'a StructConstructExpr>) {
    match &expr.kind {
        ExprKind::Paren(inner) => collect_struct_constructs(inner, out),
        ExprKind::StructConstruct(sc) => out.push(sc),
        ExprKind::Field { target, .. } => collect_struct_constructs(target, out),
        ExprKind::Binary(_, l, r) => {
            collect_struct_constructs(l, out);
            collect_struct_constructs(r, out);
        }
        ExprKind::If(ifx) => collect_struct_constructs(&ifx.condition, out),
        _ => {}
    }
}

fn struct_constructs(unit: &SourceUnit) -> Vec<&StructConstructExpr> {
    let mut out = Vec::new();
    for item in &unit.items {
        let ItemKind::Function(f) = &item.kind else {
            continue;
        };
        for stmt in &f.body.stmts {
            match &stmt.kind {
                StmtKind::Let(b) => collect_struct_constructs(&b.init, &mut out),
                StmtKind::Var(v) => {
                    if let Some(init) = &v.init {
                        collect_struct_constructs(init, &mut out);
                    }
                }
                StmtKind::Expr(e) => collect_struct_constructs(e, &mut out),
                StmtKind::Return(r) => {
                    if let Some(value) = &r.value {
                        collect_struct_constructs(value, &mut out);
                    }
                }
                _ => {}
            }
        }
    }
    out
}

#[test]
fn struct_literal_full_and_shorthand() {
    // §130-141: `Point { x: 1, y: 2 }` completo e `Point { x, y }` shorthand.
    let src = "module m\nstruct Point {\n    x: Int\n    y: Int\n}\nfunction origin() -> Point {\n    let p = Point { x: 1, y: 2 }\n    return p\n}\nfunction both(a: Int) -> Point {\n    let s = Point { x: a, y: a }\n    return s\n}\n";
    let r = parse_ok(src);
    assert!(!r.has_errors(), "errors: {:?}", r.diagnostics);
    let constructs = struct_constructs(&r.ast);
    assert_eq!(constructs.len(), 2, "expected two struct constructs");
    let full = constructs[0];
    assert_eq!(full.path.segments[0].name, "Point");
    assert_eq!(full.fields.len(), 2);
    assert!(matches!(full.fields[0].expr.kind, ExprKind::IntLiteral(1)));
    let shorthand = constructs[1];
    assert_eq!(shorthand.fields.len(), 2);
    assert!(
        matches!(shorthand.fields[0].expr.kind, ExprKind::Ident(_)),
        "shorthand `x` must desugar to `x: x`"
    );
    assert_eq!(r.reconstruction, src, "reconstruction must be lossless");
}

#[test]
fn struct_literal_in_condition_requires_parens() {
    // §551-553: em condição/scrutinee o `{` top-level pertence ao bloco do
    // constructo; sem parênteses o literal é desligado; dentro de
    // `( ... )` ele volta a ser permitido.
    let bare = "module m\nstruct Point {\n    x: Int\n    y: Int\n}\nfunction f(a: Point) -> Bool {\n    if Point { x: 1, y: 2 }.x == a.x {\n        return true\n    }\n    return false\n}\n";
    assert!(
        parse_ok(bare).has_errors(),
        "bare left brace in condition must diagnose"
    );

    let paren = "module m\nstruct Point {\n    x: Int\n    y: Int\n}\nfunction f(a: Point) -> Bool {\n    if (Point { x: 1, y: 2 }).x == a.x {\n        return true\n    }\n    return false\n}\n";
    let r = parse_ok(paren);
    assert!(!r.has_errors(), "errors: {:?}", r.diagnostics);
    assert_eq!(struct_constructs(&r.ast).len(), 1);
}
