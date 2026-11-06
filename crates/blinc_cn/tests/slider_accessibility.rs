mod common;

use blinc_core::reactive::signal;
use blinc_layout::{
    accessibility::{AccessibilityAction, AccessibilityRole},
    div::div,
    renderer::RenderTree,
};
use std::sync::{Arc, Mutex};

#[test]
fn slider_semantics_follow_the_value_label_bounds_and_disabled_state() {
    common::guarded(|| {
        common::init();
        let value = signal(0.0f32);
        let label = signal("Volume".to_string());
        let disabled = signal(false);
        let changes = Arc::new(Mutex::new(Vec::new()));
        let changed = changes.clone();
        let mut tree = RenderTree::from_element(
            &div().w(400.0).h(160.0).child(
                blinc_cn::slider(value)
                    .min(-10.0)
                    .max(10.0)
                    .step(2.0)
                    .label(label)
                    .show_value()
                    .disabled(disabled)
                    .on_change(move |next| changed.lock().unwrap().push(next)),
            ),
        );
        tree.compute_layout(400.0, 160.0);
        let nodes = tree.accessibility_nodes();
        assert_eq!(
            nodes.len(),
            1,
            "the label, value and track form one control"
        );
        assert_eq!(nodes[0].role, AccessibilityRole::Slider);
        assert_eq!(nodes[0].label, "Volume");
        assert_eq!(nodes[0].value, "0");
        let id = nodes[0].id;

        assert!(tree.perform_accessibility_action(id, AccessibilityAction::Increment));
        assert_eq!(value.get(), 2.0);
        assert_eq!(*changes.lock().unwrap(), [2.0]);
        value.set(6.0);
        label.set("Output volume".to_string());
        assert_eq!(tree.accessibility_nodes()[0].value, "6");
        assert_eq!(tree.accessibility_nodes()[0].label, "Output volume");
        assert!(tree.perform_accessibility_action(id, AccessibilityAction::Decrement));
        assert_eq!(value.get(), 4.0);
        assert_eq!(*changes.lock().unwrap(), [2.0, 4.0]);

        for _ in 0..10 {
            assert!(tree.perform_accessibility_action(id, AccessibilityAction::Increment));
        }
        assert_eq!(value.get(), 10.0);
        assert_eq!(*changes.lock().unwrap(), [2.0, 4.0, 6.0, 8.0, 10.0]);
        disabled.set(true);
        tree.apply_partial_property_updates(blinc_layout::take_pending_partial_prop_updates());
        assert!(tree.accessibility_nodes()[0].disabled);
        assert!(!tree.perform_accessibility_action(id, AccessibilityAction::Decrement));
        assert_eq!(value.get(), 10.0);
        assert_eq!(changes.lock().unwrap().len(), 5);
        assert!(!blinc_layout::stateful::has_pending_subtree_rebuilds());
    });
}

#[test]
fn fractional_adjustments_write_back_to_f64_signals() {
    common::guarded(|| {
        common::init();
        let value = signal(1.5f64);
        let mut tree = RenderTree::from_element(
            &div().w(400.0).h(100.0).child(
                blinc_cn::slider(value)
                    .min(1.0)
                    .max(2.0)
                    .step(0.25)
                    .label("Playback speed"),
            ),
        );
        tree.compute_layout(400.0, 100.0);
        let id = tree.accessibility_nodes()[0].id;
        assert!(tree.perform_accessibility_action(id, AccessibilityAction::Increment));
        assert_eq!(value.get(), 1.75);
        assert_eq!(tree.accessibility_nodes()[0].value, "1.75");
        for _ in 0..10 {
            assert!(tree.perform_accessibility_action(id, AccessibilityAction::Decrement));
        }
        assert_eq!(value.get(), 1.0);
        assert_eq!(tree.accessibility_nodes()[0].value, "1.00");
    });
}
