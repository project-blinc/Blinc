#![allow(dead_code)]

use blinc_app::{BlincComponent, windowed::WindowedContext};
use blinc_core::{
    Computed, Signal,
    reactive::{ReactiveGraph, computed, effect, signal},
};
use blinc_layout::{
    binding::Reactive,
    div::div,
    renderer::{ElementType, RenderTree},
    text::Text,
};
use std::sync::{Arc, Mutex};

#[derive(BlincComponent)]
struct Badge {
    pub label: String,
    pub disabled: bool,
    pub opacity: f32,
    #[animation]
    pulse: f32,
}

#[derive(BlincComponent)]
struct Generic<C, const N: usize>
where
    C: Clone + Send + 'static,
{
    value: C,
    r#type: String,
}

#[derive(BlincComponent)]
struct AnimationOnly<T> {
    #[animation]
    animation: T,
}

#[derive(BlincComponent)]
struct Marker;

#[derive(BlincComponent)]
struct Shared<T> {
    value: Arc<T>,
}

#[derive(BlincComponent)]
struct Borrowed<'a> {
    label: &'a str,
}

#[derive(blinc_app::Component)]
struct Shadow<String> {
    value: String,
}

mod other {
    use super::*;
    #[derive(BlincComponent)]
    pub struct Badge {
        pub label: String,
    }
}

fn auto_at_one_site(ctx: &WindowedContext) -> Signal<String> {
    Badge::use_label_auto(ctx, "initial".into())
}

#[test]
fn field_hooks_return_persistent_signals_and_isolate_instances() {
    let ctx = WindowedContext::headless(640.0, 480.0);
    let label: Signal<String> = Badge::use_label(&ctx, "initial".into());
    label.set("updated".into());
    let again = Badge::use_label(&ctx, "ignored".into());
    assert_eq!(label.id(), again.id());
    let previous_key = format!("{}:label", Badge::COMPONENT_KEY);
    assert_eq!(
        label.id(),
        ctx.use_signal_keyed(&previous_key, || "ignored".to_string())
            .id()
    );
    assert_eq!(again.get(), "updated");
    let first = Badge::use_label_for(&ctx, "first", "one".into());
    let second = Badge::use_label_for(&ctx, "second", "two".into());
    assert_ne!(first.id(), second.id());
    assert_ne!(first.id(), label.id());
    first.set("changed".into());
    assert_eq!(
        Badge::use_label_for(&ctx, "first", "ignored".into()).get(),
        "changed"
    );
    assert_eq!(second.get(), "two");
    assert_eq!(auto_at_one_site(&ctx).id(), auto_at_one_site(&ctx).id());
    assert_ne!(
        Badge::use_label_auto(&ctx, "a".into()).id(),
        Badge::use_label_auto(&ctx, "b".into()).id()
    );
    assert_ne!(
        label.id(),
        other::Badge::use_label(&ctx, "other".into()).id()
    );
}

#[test]
fn props_accept_constants_signals_and_computeds_without_copying_their_values() {
    let label = signal("First".to_string());
    let disabled = signal(false);
    let opacity: Computed<f32> =
        computed(move |g| if g.get(disabled).unwrap() { 0.4 } else { 1.0 });
    let props: BadgeProps = Badge::props(label, disabled, opacity.clone());
    match &props.label {
        Reactive::Bound(source) => assert_eq!(source.signal_id(), label.id()),
        _ => panic!("label lost its source"),
    }
    match &props.opacity {
        Reactive::Computed(source) => assert_eq!(source.derived().id(), opacity.derived().id()),
        _ => panic!("opacity lost its computation"),
    }
    let copy = props.clone();
    label.set("Next".into());
    disabled.set(true);
    assert_eq!(copy.label.now().as_deref(), Some("Next"));
    assert_eq!(copy.disabled.now(), Some(true));
    assert_eq!(copy.opacity.now(), Some(0.4));
    let replaced = copy.label("Fixed").disabled(false).opacity(0.8);
    label.set("Later".into());
    assert_eq!(replaced.label.now().as_deref(), Some("Fixed"));
    assert_eq!(replaced.disabled.now(), Some(false));
    assert_eq!(replaced.opacity.now(), Some(0.8));
}

#[test]
fn fluent_setters_accept_borrowed_sources_and_existing_reactive_props() {
    let label = signal("Bound".to_string());
    let disabled = signal(false);
    let opacity = computed(move |g| if g.get(disabled).unwrap() { 0.2 } else { 0.9 });
    let props = Badge::props("Initial", true, 1.0)
        .label(&label)
        .disabled(&disabled)
        .opacity(&opacity);
    let props = Badge::props(props.label, props.disabled, props.opacity);
    label.set("Changed".into());
    disabled.set(true);
    assert_eq!(props.label.now().as_deref(), Some("Changed"));
    assert_eq!(props.disabled.now(), Some(true));
    assert_eq!(props.opacity.now(), Some(0.2));
}

#[test]
fn props_patch_mounted_text_and_properties_without_rebuilding() {
    let label = signal("First".to_string());
    let disabled = signal(false);
    let opacity = computed(move |g| if g.get(disabled).unwrap() { 0.4 } else { 1.0 });
    let props = Badge::props(label, disabled, opacity);
    let view = div()
        .id("badge")
        .w(160.0)
        .h(40.0)
        .opacity(props.opacity)
        .attr_when("disabled", "", props.disabled)
        .child(Text::bound(props.label));
    let mut tree = RenderTree::from_element(&view);
    tree.compute_layout(640.0, 480.0);
    let root = tree.root().unwrap();
    let text = tree.layout_tree.children(root)[0];
    let stable = tree.stable_id(root);
    label.set("Changed".into());
    disabled.set(true);
    let updates = tree.take_pending_property_updates();
    assert!(!updates.is_empty());
    tree.apply_partial_property_updates(updates);
    assert_eq!(tree.root(), Some(root));
    assert_eq!(tree.stable_id(root), stable);
    assert_eq!(tree.get_render_node(root).unwrap().props.opacity, 0.4);
    assert!(
        tree.element_registry()
            .attribute(root, "disabled")
            .is_some()
    );
    match &tree.get_render_node(text).unwrap().element_type {
        ElementType::Text(value) => assert_eq!(value.content, "Changed"),
        _ => panic!("expected text"),
    }
    assert!(blinc_layout::take_pending_subtree_rebuilds().is_empty());
}

#[test]
fn generated_signals_feed_computeds_and_effects() {
    let ctx = WindowedContext::headless(640.0, 480.0);
    let disabled = Badge::use_disabled(&ctx, false);
    let label = computed(move |g| {
        if g.get(disabled).unwrap() {
            "Disabled".to_string()
        } else {
            "Enabled".to_string()
        }
    });
    let props = Badge::props(label, disabled, 1.0);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let record = seen.clone();
    let label = props.label.clone();
    let owner = blinc_core::owner::Owner::new();
    owner.run(|| {
        effect(move |g: &ReactiveGraph| record.lock().unwrap().push(label.read(g).unwrap()))
    });
    disabled.set(true);
    assert_eq!(props.label.now().as_deref(), Some("Disabled"));
    assert_eq!(*seen.lock().unwrap(), ["Enabled", "Disabled"]);
    owner.dispose();
    disabled.set(false);
    assert_eq!(*seen.lock().unwrap(), ["Enabled", "Disabled"]);
}

#[test]
fn generic_props_and_hooks_preserve_bounds_raw_fields_and_type_identity() {
    let ctx = WindowedContext::headless(640.0, 480.0);
    let value = Generic::<String, 2>::use_value(&ctx, "first".into());
    let props: GenericProps<String, 2> = Generic::props(value, "kind");
    assert_eq!(props.value.now().as_deref(), Some("first"));
    assert_eq!(props.r#type.now().as_deref(), Some("kind"));
    let kind: Signal<String> = Generic::<String, 2>::use_type(&ctx, "raw".into());
    assert_eq!(kind.get(), "raw");
    let other = Generic::<String, 3>::use_value(&ctx, "other".into());
    assert_ne!(value.id(), other.id());
    let _: AnimationOnlyProps<u8> = AnimationOnly::props();
}

#[test]
fn animation_and_unit_struct_hooks_remain_available() {
    let ctx = WindowedContext::headless(640.0, 480.0);
    let animation = Badge::use_pulse(&ctx, 1.0, blinc_animation::SpringConfig::snappy());
    let again = Badge::use_pulse(&ctx, 2.0, blinc_animation::SpringConfig::snappy());
    assert!(Arc::ptr_eq(&animation, &again));
    let _: BadgeProps = Badge::props("Label", false, 1.0);
    let _ = Marker::use_animated_value(&ctx, 1.0, blinc_animation::SpringConfig::default());
    let _ = Marker::use_animated_timeline(&ctx);
    assert_ne!(Marker::instance_key(), Marker::instance_key());
}

#[test]
fn props_clone_only_requires_the_declared_value_type_to_be_cloneable() {
    struct NoClone(u8);
    let value = Arc::new(NoClone(7));
    let props = Shared::props(value.clone());
    match props.clone().value {
        Reactive::Const(copy) => assert!(Arc::ptr_eq(&value, &copy)),
        _ => panic!("expected the shared constant"),
    }
    let props = other::Badge::props("Public");
    assert_eq!(props.label.now().as_deref(), Some("Public"));
}

#[test]
fn borrowed_constant_props_keep_the_source_lifetime() {
    let label = String::from("Borrowed");
    let props = Borrowed::props(label.as_str()).clone();
    match props.label {
        Reactive::Const(value) => assert_eq!(value, label.as_str()),
        _ => panic!("expected a borrowed constant"),
    }
}

#[test]
fn alias_derive_and_generated_paths_work_when_generic_names_shadow_the_prelude() {
    let ctx = WindowedContext::headless(640.0, 480.0);
    let value: Signal<u8> = Shadow::use_value(&ctx, 7);
    let props: ShadowProps<u8> = Shadow::props(value);
    value.set(9);
    assert_eq!(props.value.now(), Some(9));
    assert!(Shadow::<u8>::instance_key().contains("Shadow<u8>"));
}
