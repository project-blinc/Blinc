//! Every `cn.*` widget takes a `class`, and it reaches CSS.
//!
//! It used to parse and go nowhere: the arg was accepted at the call
//! site, dropped by the extern thunk, and the rule silently never
//! applied — the worst shape for a gap, since nothing warns.
use blinc_dsl_core::BlincDsl;
use blinc_layout::div::div;
use blinc_layout::renderer::RenderTree;
use std::sync::{Arc, Mutex};

fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        blinc_theme::ThemeState::init_default();
        let s = blinc_animation::AnimationScheduler::new();
        blinc_animation::set_global_scheduler(s.handle());
        blinc_layout::render_state::set_global_scheduler(s.handle());
        Box::leak(Box::new(s));
        if !blinc_core::BlincContextState::is_initialized() {
            blinc_core::BlincContextState::init(
                blinc_core::reactive::global_graph(),
                Arc::new(Mutex::new(blinc_core::context_state::HookState::new())),
                Arc::new(std::sync::atomic::AtomicBool::new(false)),
            );
        }
    });
}

/// Whether any node in the tree ends up `width` wide, which is how a
/// class-only rule shows up in geometry.
fn has_node_of_width(src: &str, name: &str, width: f32) -> bool {
    init();
    let dsl = BlincDsl::new().expect("runtime init");
    blinc_cn_dsl::register_all(&dsl).expect("register");
    dsl.compile_source(src, name).expect("compile");

    let host = div().w(800.0).h(400.0).child_box(dsl.view_widget());
    let mut tree = RenderTree::from_element(&host);
    let css = dsl.compiled_stylesheets().join("\n");
    let sheet = blinc_layout::css_parser::Stylesheet::parse(&css).expect("the sheet parses");
    tree.set_stylesheet(sheet);
    tree.apply_stylesheet_base_styles();
    tree.apply_stylesheet_layout_overrides();
    tree.compute_layout(800.0, 400.0);

    let Some(root) = tree.root() else {
        return false;
    };
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        if let Some(b) = tree.get_absolute_bounds(id)
            && (b.width - width).abs() < 0.5
        {
            return true;
        }
        stack.extend(tree.layout_tree.children(id));
    }
    false
}

/// A styled widget: wrapped in `Styled` already, so the class rides
/// along with the inline overlay.
#[test]
fn a_class_on_a_styled_widget_reaches_css() {
    assert!(has_node_of_width(
        r#"component C {
             style { .wide { width: 333px } }
             view { Div { cn.Badge("hi", class = "wide") } }
           }
           view { C() }"#,
        "class_styled.blinc",
        333.0,
    ));
}

/// Two names on one call, the way CSS is usually written.
#[test]
fn several_class_names_all_apply() {
    assert!(has_node_of_width(
        r#"component C {
             style { .a { width: 222px } .b { height: 40px } }
             view { Div { cn.Badge("hi", class = "a b") } }
           }
           view { C() }"#,
        "class_multi.blinc",
        222.0,
    ));
}

/// The widget's own classes survive: `.cn-badge` styling is not lost
/// because the author added one of their own.
#[test]
fn an_author_class_does_not_displace_the_widgets_own() {
    assert!(has_node_of_width(
        r#"component C {
             style { .cn-badge { width: 177px } }
             view { Div { cn.Badge("hi", class = "mine") } }
           }
           view { C() }"#,
        "class_keeps_own.blinc",
        177.0,
    ));
}

/// A container widget, whose children are its own, still takes one.
#[test]
fn a_container_widget_takes_a_class() {
    assert!(has_node_of_width(
        r#"signal cp: string = "free"
           component C {
             style { .boxed { width: 288px } }
             view {
               cn.RadioGroup(value = cp, class = "boxed") {
                 cn.Radio(value = "free", label = "Free")
               }
             }
           }
           view { C() }"#,
        "class_container.blinc",
        288.0,
    ));
}

/// Omitting it changes nothing, so the common call is unaffected.
#[test]
fn omitting_class_is_inert() {
    assert!(!has_node_of_width(
        r#"component C {
             style { .wide { width: 333px } }
             view { Div { cn.Badge("hi") } }
           }
           view { C() }"#,
        "class_absent.blinc",
        333.0,
    ));
}
