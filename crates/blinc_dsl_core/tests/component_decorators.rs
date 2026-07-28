//! `@stateful` on the component, not on its view.
//!
//! A component owns exactly one view, so the decorator belongs on the
//! component — and that is the only placement carrying the component's
//! NAME. On the view it is an `impl <C> { fn view() }` method, so the
//! name has to be recovered from the impl target, and the entry
//! `view { }` is indistinguishable from a component's.
use blinc_dsl_core::BlincDsl;

const FSM: &str = "fsm F { context { flag: bool = false } state I initial I }\n";

/// Compile and report the components the DSL attributed a decoration to.
fn decorated(src: &str) -> Vec<String> {
    let dsl = BlincDsl::new().expect("dsl");
    dsl.compile_source(src, "dec.blinc").expect("compile");
    dsl.stateful_components()
}

#[test]
fn a_decorator_on_the_component_names_the_component() {
    let got = decorated(&format!(
        "{FSM}@stateful @fsm([F])\ncomponent Panel {{ view {{ Div {{ Text(\"a\") }} }} }}\nview {{ Panel() }}"
    ));
    assert_eq!(got, vec!["Panel".to_string()]);
}

/// The older placement still compiles and still makes the view
/// reactive — through the whole-program wrap.
///
/// It is NOT attributed to a component, so it cannot be scoped. The
/// marker sits in an `impl <C> { fn view() }` method body and carries
/// no name; recovering one means reading the impl target, which is a
/// `Type` the detector would have to resolve through the program's
/// type registry while already holding it mutably. Putting the
/// decorator on the component sidesteps all of that, which is the
/// reason to prefer it.
#[test]
fn a_decorator_on_the_view_compiles_but_is_not_attributed() {
    let got = decorated(&format!(
        "{FSM}component Panel {{ @stateful @fsm([F]) view {{ Div {{ Text(\"a\") }} }} }}\nview {{ Panel() }}"
    ));
    assert!(
        got.is_empty(),
        "view-placed decorators name no component: {got:?}"
    );
}

#[test]
fn an_undecorated_component_is_not_listed() {
    let got = decorated("component Panel { view { Div { Text(\"a\") } } }\nview { Panel() }");
    assert!(got.is_empty(), "nothing decorated: {got:?}");
}

/// Two decorated components are both attributed, which is what a
/// per-component `Stateful` needs.
#[test]
fn each_decorated_component_is_named() {
    let got = decorated(&format!(
        "{FSM}@stateful @fsm([F])\ncomponent A {{ view {{ Div {{ Text(\"a\") }} }} }}\n\
         @stateful @fsm([F])\ncomponent B {{ view {{ Div {{ Text(\"b\") }} }} }}\n\
         component C {{ view {{ Div {{ Text(\"c\") }} }} }}\n\
         view {{ Div {{ A() B() C() }} }}"
    ));
    assert_eq!(got, vec!["A".to_string(), "B".to_string()]);
}
