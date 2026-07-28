//! `[a, b, c]` in the DSL.
//!
//! The literal lowers to Zyntax's native `TypedExpression::Array`,
//! which the compiler already turns into a `List<T> { data, len,
//! capacity }`. So these tests pin the AST shape, not a marshalling
//! scheme: getting `Array` is what lets a collection prop cross the
//! extern ABI as one pointer.
use blinc_dsl_core::BlincDsl;
use zyntax_typed_ast::TypedExpression;

/// Count `TypedExpression::Array` nodes anywhere in the program, with
/// the element count of each.
fn array_shapes(dsl: &BlincDsl, src: &str) -> Vec<usize> {
    let program = dsl
        .parse_to_typed_ast(src, "arrays.blinc")
        .expect("must parse");
    let mut found = Vec::new();
    // The typed AST has no generic visitor, and an array literal in
    // these tests is always a `let` initializer or a call argument, so
    // walk the JSON rather than every statement shape.
    let json = serde_json::to_value(&program).expect("serialisable");
    fn walk(v: &serde_json::Value, found: &mut Vec<usize>) {
        match v {
            serde_json::Value::Object(map) => {
                if let Some(serde_json::Value::Array(elems)) = map.get("Array") {
                    found.push(elems.len());
                }
                for value in map.values() {
                    walk(value, found);
                }
            }
            serde_json::Value::Array(items) => {
                for item in items {
                    walk(item, found);
                }
            }
            _ => {}
        }
    }
    walk(&json, &mut found);
    let _ = TypedExpression::Array(vec![]); // pin the variant name
    found
}

#[test]
fn a_list_of_strings_parses_as_an_array() {
    let dsl = BlincDsl::new().expect("dsl");
    let shapes = array_shapes(
        &dsl,
        "view { Div {} }\nfn f() { let a = [\"x\", \"y\", \"z\"] }",
    );
    assert_eq!(shapes, vec![3], "one array of three elements: {shapes:?}");
}

#[test]
fn an_empty_list_parses() {
    let dsl = BlincDsl::new().expect("dsl");
    let shapes = array_shapes(&dsl, "view { Div {} }\nfn f() { let a = [] }");
    assert_eq!(shapes, vec![0], "one array, no elements: {shapes:?}");
}

#[test]
fn a_trailing_comma_is_allowed() {
    let dsl = BlincDsl::new().expect("dsl");
    let shapes = array_shapes(&dsl, "view { Div {} }\nfn f() { let a = [1.0, 2.0,] }");
    assert_eq!(
        shapes,
        vec![2],
        "trailing comma adds no element: {shapes:?}"
    );
}

/// The shape that matters: a list as a widget prop. No widget takes one
/// yet, so this pins that the ARGUMENT parses as an array — the prop
/// type checking is the macro's job.
#[test]
fn a_list_in_argument_position_parses_as_an_array() {
    let dsl = BlincDsl::new().expect("dsl");
    let shapes = array_shapes(
        &dsl,
        "view { Div {} }\nfn f() { let a = g([\"one\", \"two\"]) }",
    );
    assert_eq!(shapes, vec![2], "the argument is an array: {shapes:?}");
}

/// Nested lists compose, which is what a table's rows-of-cells needs.
#[test]
fn lists_nest() {
    let dsl = BlincDsl::new().expect("dsl");
    let shapes = array_shapes(
        &dsl,
        "view { Div {} }\nfn f() { let a = [[\"a\", \"b\"], [\"c\"]] }",
    );
    // Outer of 2, then the two inners. Order follows the walk, so
    // compare as a multiset.
    let mut sorted = shapes.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, vec![1, 2, 2], "outer + two inners: {shapes:?}");
}

/// The literal has to survive lowering, not just parsing: `compile_source`
/// runs the JIT, so a shape the backend can't handle fails here.
#[test]
fn array_literals_compile() {
    let dsl = BlincDsl::new().expect("dsl");
    dsl.compile_source(
        "view { Div {} }\nfn f() { let a = [\"x\", \"y\"] }",
        "compile.blinc",
    )
    .expect("a string list must compile");
}
