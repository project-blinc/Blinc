//! Media controls are built once: playing, the position and the volume
//! patch the play icon, the time and the volume in place, with no subtree
//! rebuilt. Controls for a player that publishes nothing read it as they paint.
#![cfg(feature = "media")]

use std::cell::Cell;
use std::rc::Rc;

use blinc_core::events::event_types;
use blinc_core::reactive::global_graph;
use blinc_layout::LayoutNodeId;
use blinc_layout::div::div;
use blinc_layout::renderer::{ElementType, RenderTree};
use blinc_layout::widgets::media::{MediaControls, video_player};
use blinc_media::{Frame, Player, VideoPlayer};

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

#[test]
fn the_frame_size_is_shown_once_there_is_a_frame() {
    init();
    let player = Rc::new(VideoPlayer::new());
    let host = div().w(640.0).h(400.0).child(
        video_player(Rc::clone(&player))
            .show_dimensions()
            .into_div(),
    );
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(640.0, 400.0);
    let size = |tree: &RenderTree| shown(tree).0.into_iter().find(|t| t.contains('×'));
    assert_eq!(size(&tree), None);

    player.push_frame(Frame::from_rgba(vec![0; 320 * 180 * 4], 320, 180));
    frame(&mut tree);
    assert_eq!(size(&tree).as_deref(), Some("320 × 180"));

    player.stop();
    frame(&mut tree);
    assert_eq!(size(&tree), None);
}

/// A player that tells nobody when it changes.
#[derive(Default)]
struct Track {
    playing: Cell<bool>,
    position: Cell<u64>,
}

impl Player for Track {
    fn play(&self) {
        self.playing.set(true);
    }
    fn pause(&self) {
        self.playing.set(false);
    }
    fn stop(&self) {}
    fn seek(&self, position_ms: u64) {
        self.position.set(position_ms);
    }
    fn position_ms(&self) -> u64 {
        self.position.get()
    }
    fn duration_ms(&self) -> u64 {
        60_000
    }
    fn volume(&self) -> f32 {
        1.0
    }
    fn set_volume(&self, _: f32) {}
    fn is_playing(&self) -> bool {
        self.playing.get()
    }
}

fn nodes(tree: &RenderTree) -> Vec<LayoutNodeId> {
    let mut found = Vec::new();
    let mut stack = vec![tree.root().unwrap()];
    while let Some(node) = stack.pop() {
        found.push(node);
        stack.extend(tree.layout_tree.children(node));
    }
    found
}

fn find(tree: &RenderTree, class: &str) -> LayoutNodeId {
    nodes(tree)
        .into_iter()
        .find(|node| tree.element_registry().has_class(*node, class))
        .unwrap_or_else(|| panic!("no .{class}"))
}

/// Runs the canvas that reads the player, as a frame does, and returns whether it asks for
/// another.
fn paint(tree: &mut RenderTree) -> bool {
    let canvas = nodes(tree)
        .into_iter()
        .find_map(|node| match &tree.get_render_node(node)?.element_type {
            ElementType::Canvas(canvas) => Some(canvas.clone()),
            _ => None,
        })
        .expect("the controls have a canvas");
    let mut ctx = blinc_core::RecordingContext::new(blinc_core::Size::new(400.0, 60.0));
    (canvas.render_fn.as_ref().expect("a paint"))(
        &mut ctx,
        blinc_layout::CanvasBounds {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        },
    );
    frame(tree);
    canvas.wants_frames()
}

fn click(tree: &mut RenderTree, class: &str, across: f32) {
    let node = find(tree, class);
    let b = tree.get_absolute_bounds(node).unwrap();
    let (x, y) = (b.width * across, b.height / 2.0);
    tree.dispatch_event_with_local(
        node,
        event_types::POINTER_UP,
        b.x + x,
        b.y + y,
        x,
        y,
        b.x,
        b.y,
        b.width,
        b.height,
    );
    frame(tree);
}

/// The share of the seek bar that is filled.
fn filled(tree: &RenderTree) -> f32 {
    let width = |class| tree.get_absolute_bounds(find(tree, class)).unwrap().width;
    width("blinc-media-seek-fill") / width("blinc-media-seek-rail")
}

#[test]
fn controls_follow_a_player_that_publishes_nothing() {
    init();
    let player = Rc::new(Track::default());
    // A column, as the audio player is, so the controls take its width.
    let host = div()
        .w(400.0)
        .h(60.0)
        .flex_col()
        .child(MediaControls::new(Rc::clone(&player)).into_div());
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(400.0, 60.0);
    assert!(!paint(&mut tree), "a still player asks for no frame");
    assert_eq!(shown(&tree).0, ["0:00 / 1:00", "100%"]);
    assert!(filled(&tree) < 0.01);
    let play_icon = shown(&tree).1;

    // The player moves on its own.
    player.playing.set(true);
    player.position.set(30_000);
    assert!(paint(&mut tree), "a playing player asks for frames");
    assert_eq!(shown(&tree).0[0], "0:30 / 1:00");
    assert!(
        (filled(&tree) - 0.5).abs() < 0.01,
        "filled {}",
        filled(&tree)
    );
    assert_ne!(shown(&tree).1, play_icon, "the button offers to pause");

    // What the controls do shows with no paint in between.
    click(&mut tree, "blinc-media-play-btn", 0.5);
    assert!(!player.playing.get());
    assert_eq!(shown(&tree).1, play_icon, "the button offers to play");
    click(&mut tree, "blinc-media-seek-track", 0.25);
    assert_eq!(player.position.get(), 15_000);
    assert_eq!(shown(&tree).0[0], "0:15 / 1:00");
    assert!(
        (filled(&tree) - 0.25).abs() < 0.01,
        "filled {}",
        filled(&tree)
    );

    // A player that stops by itself is read once more before the frames stop.
    player.playing.set(true);
    paint(&mut tree);
    player.playing.set(false);
    player.position.set(60_000);
    assert!(!paint(&mut tree));
    assert_eq!(shown(&tree).0[0], "1:00 / 1:00");
}
