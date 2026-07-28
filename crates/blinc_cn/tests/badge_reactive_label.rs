//! A bound Badge label must still be styled by the variant class.
//!
//! `.cn-badge--{style}-{variant}` carries the chip's background, border
//! AND its text colour, which the label inherits. Building the classed
//! node inside the `Stateful` put it behind the rebuild boundary and the
//! label rendered in the default text colour instead of the variant's,
//! so the class has to live on a stable node outside it.

use blinc_core::reactive::{State, global_dirty_flag, global_graph, signal};
use blinc_layout::binding::Reactive;
use blinc_layout::div::ElementBuilder;

fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        blinc_theme::ThemeState::init_default();
        if !blinc_animation::is_scheduler_initialized() {
            let s = blinc_animation::AnimationScheduler::new();
            blinc_animation::set_global_scheduler(s.handle());
            Box::leak(Box::new(s));
        }
        if !blinc_core::BlincContextState::is_initialized() {
            blinc_core::BlincContextState::init(
                global_graph(),
                std::sync::Arc::new(std::sync::Mutex::new(
                    blinc_core::context_state::HookState::new(),
                )),
                std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            );
        }
    });
}

fn classes(el: &dyn ElementBuilder) -> Vec<String> {
    el.element_classes().iter().map(|c| c.to_string()).collect()
}

#[test]
fn a_bound_label_keeps_the_variant_class_on_the_outer_node() {
    init();
    let stat = blinc_cn::badge("secondary").variant(blinc_cn::BadgeVariant::Secondary);
    let expected = classes(&stat);
    assert!(
        expected.iter().any(|c| c == "cn-badge--soft-secondary"),
        "sanity: a static chip carries its variant class, got {expected:?}"
    );

    let label = State::new(
        signal::<String>("secondary".to_string()),
        global_graph(),
        global_dirty_flag(),
    );
    let bound = blinc_cn::badge("")
        .variant(blinc_cn::BadgeVariant::Secondary)
        .reactive_label(Reactive::Bound(label));
    assert_eq!(
        classes(&bound),
        expected,
        "a bound chip must carry the same classes, on the same outer node"
    );
}

/// A `Const` label takes the plain path, unchanged.
#[test]
fn a_const_label_is_the_static_path() {
    init();
    let stat = blinc_cn::badge("hi").variant(blinc_cn::BadgeVariant::Success);
    let konst = blinc_cn::badge("")
        .variant(blinc_cn::BadgeVariant::Success)
        .reactive_label(Reactive::Const("hi".to_string()));
    assert_eq!(classes(&konst), classes(&stat));
}

/// An Alert with no variant is the info one, classes and all.
#[test]
fn a_variant_less_alert_is_info() {
    init();
    let plain = blinc_cn::alert("hello");
    let explicit = blinc_cn::alert("hello").variant(blinc_cn::AlertVariant::Default);
    let cls = classes(&plain);
    println!("ALERT classes {cls:?}");
    assert_eq!(cls, classes(&explicit));
    assert!(
        cls.iter().any(|c| c == "cn-alert--info"),
        "the default variant paints as info, got {cls:?}"
    );
}
