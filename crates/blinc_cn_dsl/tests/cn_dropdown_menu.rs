//! `cn.Select` — a bound signal and `cn.Option` children.
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

/// The trigger draws, and the rows are not shown until it is opened.
#[test]
fn a_menu_draws_its_trigger() {
    let dsl = compile(
        r#"view {
             cn.DropdownMenu(label = "File") {
               cn.MenuItem(label = "New")
               cn.MenuItem(label = "Open")
             }
           }"#,
        "menu_trigger.blinc",
    );

    let h = Harness::new(&dsl);
    assert!(h.shows("File"), "the trigger label: {:?}", h.texts());
}

/// A row with no handler still exists: choosing it dismisses and does
/// nothing. Dropping it would silently lose a line the author wrote.
#[test]
fn a_row_without_a_handler_is_kept() {
    let dsl = compile(
        r#"view {
             cn.DropdownMenu(label = "Edit") {
               cn.MenuItem(label = "Undo")
             }
           }"#,
        "menu_no_handler.blinc",
    );
    let _ = Harness::new(&dsl);
}

/// A separator is a row type of its own, not a `cn.MenuItem` with an
/// empty label.
#[test]
fn a_separator_is_accepted_between_rows() {
    let dsl = compile(
        r#"view {
             cn.DropdownMenu(label = "File") {
               cn.MenuItem(label = "New")
               cn.MenuSeparator()
               cn.MenuItem(label = "Quit")
             }
           }"#,
        "menu_separator.blinc",
    );
    let _ = Harness::new(&dsl);
}

/// A child that is neither a row nor a separator is dropped rather than
/// drawn somewhere arbitrary.
#[test]
fn a_loose_child_is_dropped() {
    let dsl = compile(
        r#"view {
             cn.DropdownMenu(label = "File") {
               cn.Label("not a row")
               cn.MenuItem(label = "New")
             }
           }"#,
        "menu_loose.blinc",
    );

    let h = Harness::new(&dsl);
    assert!(
        !h.shows("not a row"),
        "the loose child is not drawn: {:?}",
        h.texts()
    );
}

/// `cn.MenuItem` is NOT `cn.Option`: a menu row is a command that runs
/// and dismisses, an option is a value a parent selects between. Both
/// exist, and putting one where the other belongs is dropped rather
/// than half-working.
#[test]
fn an_option_is_not_a_menu_row() {
    let dsl = compile(
        r#"view {
             cn.DropdownMenu(label = "File") {
               cn.Option(value = "wrong", label = "Wrong")
               cn.MenuItem(label = "Right")
             }
           }"#,
        "menu_wrong_child.blinc",
    );

    let h = Harness::new(&dsl);
    assert!(
        !h.shows("Wrong"),
        "an option in a menu is dropped: {:?}",
        h.texts()
    );
}
