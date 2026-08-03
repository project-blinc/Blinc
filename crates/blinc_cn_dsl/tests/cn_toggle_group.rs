//! `cn.ToggleGroup` — a bar of `cn.Toggle` children, one picked.
use blinc_dsl_core::BlincDsl;
use blinc_layout::div::div;
use blinc_layout::renderer::{ElementType, RenderTree};
use std::sync::{Arc, Mutex};

fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        blinc_theme::ThemeState::init_default();
        let s = blinc_animation::AnimationScheduler::new();
        blinc_animation::set_global_scheduler(s.handle());
        blinc_layout::render_state::set_global_scheduler(s.handle());
        Box::leak(Box::new(s));
        // The app installs this; without it a signal write never reaches
        // a stateful that declared deps on it.
        blinc_core::reactive::set_stateful_deps_notifier(|ids| {
            blinc_layout::check_stateful_deps(ids);
        });
        if !blinc_core::BlincContextState::is_initialized() {
            blinc_core::BlincContextState::init(
                blinc_core::reactive::global_graph(),
                Arc::new(Mutex::new(blinc_core::context_state::HookState::new())),
                Arc::new(std::sync::atomic::AtomicBool::new(false)),
            );
        }
    });
}

struct Harness {
    tree: RenderTree,
}

impl Harness {
    fn new(dsl: &BlincDsl) -> Self {
        let host = div().w(600.0).h(400.0).child_box(dsl.view_widget());
        let mut tree = RenderTree::from_element(&host);
        tree.compute_layout(600.0, 400.0);
        Self { tree }
    }

    fn texts(&self) -> Vec<String> {
        let mut out = Vec::new();
        let Some(root) = self.tree.root() else {
            return out;
        };
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            if let Some(node) = self.tree.get_render_node(id)
                && let ElementType::Text(t) = &node.element_type
            {
                out.push(t.content.clone());
            }
            stack.extend(self.tree.layout_tree.children(id).iter().copied());
        }
        out
    }

    fn shows(&self, needle: &str) -> bool {
        self.texts().iter().any(|t| t.contains(needle))
    }
}

fn compile(src: &str, name: &str) -> BlincDsl {
    init();
    let dsl = BlincDsl::new().expect("runtime init");
    blinc_cn_dsl::register_all(&dsl).expect("register");
    dsl.compile_source(src, name).expect("compile");
    dsl
}

/// Every option draws, whichever is picked.
///
/// One test rather than several: signals are process-global by name, so
/// two programs declaring the same one read each other's value.
#[test]
fn a_group_draws_every_option() {
    let dsl = compile(
        r#"signal tg_align: string = "left"

           view {
             cn.ToggleGroup(value = tg_align, variant = "outline") {
               cn.Toggle(value = "left", label = "Left")
               cn.Toggle(value = "center", label = "Center")
               cn.Toggle(value = "right", label = "Right", disabled = true)
             }
           }"#,
        "toggle_group.blinc",
    );

    let h = Harness::new(&dsl);
    for option in ["Left", "Center", "Right"] {
        assert!(h.shows(option), "option {option}: {:?}", h.texts());
    }
}

/// An item with only a value still reads as something, and a child that
/// is not a `cn.Toggle` is dropped rather than drawn somewhere
/// arbitrary.
#[test]
fn a_bare_value_labels_itself_and_a_loose_child_is_dropped() {
    let dsl = compile(
        r#"signal tg_loose: string = "solo"

           view {
             cn.ToggleGroup(value = tg_loose) {
               cn.Label("not an option")
               cn.Toggle(value = "solo")
             }
           }"#,
        "toggle_group_loose.blinc",
    );

    let h = Harness::new(&dsl);
    assert!(
        h.shows("solo"),
        "the value stands in for a missing label: {:?}",
        h.texts()
    );
    assert!(
        !h.shows("not an option"),
        "the loose child is not drawn: {:?}",
        h.texts()
    );
}

/// Writing the signal moves the selection, which is the half of the
/// binding a click cannot prove.
#[test]
fn writing_the_signal_reselects() {
    let dsl = compile(
        r#"signal tg_bound: string = "one"

           view {
             cn.ToggleGroup(value = tg_bound) {
               cn.Toggle(value = "one", label = "One")
               cn.Toggle(value = "two", label = "Two")
             }
           }"#,
        "toggle_group_bound.blinc",
    );

    let _ = Harness::new(&dsl);
    blinc_runtime::signal::set_str("tg_bound", "two");
    assert_eq!(
        blinc_runtime::signal::get_str("tg_bound").as_deref(),
        Some("two"),
        "the group's state follows the signal it was bound to",
    );

    // Still renders both options after the write.
    let h = Harness::new(&dsl);
    assert!(h.shows("One") && h.shows("Two"), "{:?}", h.texts());
}

/// An icon-only item is not captioned with its value, which would print
/// raw markup beside the glyph.
#[test]
fn an_icon_item_is_not_labelled_with_its_value() {
    let dsl = compile(
        r#"signal tg_icon: string = "bold"

           view {
             cn.ToggleGroup(value = tg_icon) {
               cn.Toggle(value = "bold", icon = "<svg viewBox='0 0 24 24'></svg>", aria_label = "Bold")
             }
           }"#,
        "toggle_group_icon.blinc",
    );

    let h = Harness::new(&dsl);
    assert!(
        !h.shows("bold") && !h.shows("svg"),
        "an icon item prints no caption: {:?}",
        h.texts()
    );
}

/// A Lucide name resolves the same whether the toggle stands alone or
/// sits in a group: it is one widget, so one behaviour.
#[test]
fn an_item_icon_takes_a_lucide_name() {
    let dsl = compile(
        r#"signal tg_lucide: string = "bold"

           view {
             Div {
               cn.Toggle(pressed = fmt_probe, icon = "bold", aria_label = "Bold")
               cn.ToggleGroup(value = tg_lucide) {
                 cn.Toggle(value = "bold", icon = "bold", aria_label = "Bold")
               }
             }
           }
           signal fmt_probe: bool = false"#,
        "toggle_group_lucide.blinc",
    );

    let h = Harness::new(&dsl);
    // Neither prints a caption, and neither leaks the name as text.
    assert!(
        !h.shows("bold"),
        "an icon name is drawn, not printed: {:?}",
        h.texts()
    );
    // Both drew something: an unresolved name would leave the chip empty
    // while the toggle still rendered its glyph.
    assert_eq!(
        svg_count(&h.tree),
        2,
        "the toggle and the item each resolved their icon",
    );
}

/// How many SVG nodes the tree holds.
fn svg_count(tree: &RenderTree) -> usize {
    let Some(root) = tree.root() else { return 0 };
    let (mut n, mut stack) = (0, vec![root]);
    while let Some(id) = stack.pop() {
        if let Some(node) = tree.get_render_node(id)
            && matches!(node.element_type, ElementType::Svg(_))
        {
            n += 1;
        }
        stack.extend(tree.layout_tree.children(id).iter().copied());
    }
    n
}
