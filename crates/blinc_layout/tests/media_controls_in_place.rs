//! The video controls are built once: playing, the position and the volume
//! patch the play icon, the time and the volume icon in place, with no
//! subtree rebuilt.
#![cfg(feature = "media")]

use std::rc::Rc;

use blinc_core::reactive::global_graph;
use blinc_layout::div::div;
use blinc_layout::renderer::{ElementType, RenderTree};
use blinc_layout::widgets::media::video_player;
use blinc_media::VideoPlayer;

fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        blinc_theme::ThemeState::init_default();
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

/// The texts and SVG sources in the controls, in tree order.
fn shown(tree: &RenderTree) -> (Vec<String>, Vec<String>) {
    let (mut texts, mut icons) = (Vec::new(), Vec::new());
    let mut stack = vec![tree.root().unwrap()];
    while let Some(node) = stack.pop() {
        match tree.get_render_node(node).map(|r| &r.element_type) {
            Some(ElementType::Text(t)) => texts.push(t.content.clone()),
            Some(ElementType::Svg(s)) => icons.push(s.source.to_string()),
            _ => {}
        }
        let mut children = tree.layout_tree.children(node);
        children.reverse();
        stack.extend(children);
    }
    (texts, icons)
}

fn frame(tree: &mut RenderTree) {
    assert!(
        !blinc_layout::stateful::has_pending_subtree_rebuilds(),
        "the controls queued a subtree rebuild"
    );
    let updates = blinc_layout::take_pending_partial_prop_updates();
    tree.apply_partial_property_updates(updates);
    tree.compute_layout(640.0, 400.0);
}

#[test]
fn playback_patches_the_controls_in_place() {
    init();
    let player = Rc::new(VideoPlayer::new());
    let host = div()
        .w(640.0)
        .h(400.0)
        .child(video_player(Rc::clone(&player)).into_div());
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(640.0, 400.0);
    let (texts, icons) = shown(&tree);

    player.position_signal.set(65_000);
    player.duration_signal.set(125_000);
    frame(&mut tree);
    let (after, _) = shown(&tree);
    assert_ne!(after, texts, "the time did not follow the position");

    player.playing_signal.set(true);
    player.volume_signal.set(0.0);
    frame(&mut tree);
    let (_, playing) = shown(&tree);
    assert_eq!(playing.len(), icons.len());
    assert_ne!(playing[0], icons[0], "the play icon did not turn to pause");
    assert_ne!(playing[1], icons[1], "the volume icon did not show muted");
}
