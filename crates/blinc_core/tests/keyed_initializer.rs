use blinc_core::{
    context_state::{BlincContextState, HookState},
    owner::Owner,
    reactive::{global_graph, signal},
};
use std::sync::{Arc, Mutex, atomic::AtomicBool};

#[test]
fn keyed_initializers_allow_nested_reactivity_and_outlive_builds() {
    BlincContextState::init(
        global_graph(),
        Arc::new(Mutex::new(HookState::new())),
        Arc::new(AtomicBool::new(false)),
    );
    let ctx = BlincContextState::get();
    let build = Owner::new();
    let parent = build.run(|| {
        ctx.use_signal_keyed("parent", || {
            (ctx.use_signal_keyed("child", || 12_u32), signal(7_u32))
        })
    });
    let (keyed, nested) = parent.try_get().unwrap();
    build.dispose();
    assert_eq!(keyed.get(), 12);
    assert_eq!(nested.get(), 7);
    nested.set(9);
    let again: blinc_core::reactive::Signal<(
        blinc_core::reactive::Signal<u32>,
        blinc_core::reactive::Signal<u32>,
    )> = ctx.use_signal_keyed("parent", || panic!("must reuse the initializer's value"));
    assert_eq!(parent.id(), again.id());
    assert_eq!(again.try_get().unwrap().1.get(), 9);
    let legacy = build.run(|| ctx.use_state_keyed("legacy", || signal(15_u32)));
    assert_eq!(legacy.try_get().unwrap().get(), 15);
}
