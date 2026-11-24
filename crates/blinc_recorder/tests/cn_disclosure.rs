#![cfg(feature = "headless")]
//! A tree's groups open over time, and the keys walk its rows; a sidebar's
//! width eases by rule, and the content beside it moves with it.

use blinc_core::reactive::signal;
use blinc_layout::LayoutNodeId;
use blinc_layout::div::div;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

const ICON: &str = r#"<svg viewBox="0 0 24 24"><rect width="24" height="24"/></svg>"#;
const DOWN: u32 = 40;
const LEFT: u32 = 37;

fn session(w: u32, h: u32) -> Option<Headless> {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return None;
    }
    let mut ui = Headless::new(w, h).ok()?;
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Dark,
    );
    Some(ui)
}

/// Every node with `class`, in document order.
fn all(ui: &Headless, class: &str) -> Vec<LayoutNodeId> {
    let tree = ui.tree();
    let registry = tree.element_registry();
    let mut found = Vec::new();
    let mut stack = vec![tree.root().unwrap()];
    while let Some(node) = stack.pop() {
        if registry.has_class(node, class) {
            found.push(node);
        }
        let mut children = tree.layout_tree.children(node);
        children.reverse();
        stack.extend(children);
    }
    found
}

fn click(ui: &mut Headless, node: LayoutNodeId) {
    let b = ui.tree().get_absolute_bounds(node).unwrap();
    ui.move_to(b.x + b.width / 2.0, b.y + b.height / 2.0)
        .mouse_down()
        .mouse_up();
}

/// The first column from the left painted red along row `y`.
fn red_edge(ui: &mut Headless, y: u32) -> u32 {
    let frame = ui.snapshot().expect("snapshot");
    (0..frame.width)
        .find(|x| {
            let i = ((y * frame.width + x) * 4) as usize;
            frame.data[i] > 200 && frame.data[i + 1] < 60 && frame.data[i + 2] < 60
        })
        .unwrap_or(0)
}

fn mount_tree(ui: &mut Headless) {
    ui.mount(move |_| {
        div().w(240.0).h(240.0).flex_col().child(
            blinc_cn::tree_view()
                .node("src", "src", |n| {
                    n.child("main", "main.rs", |c| c.icon(ICON))
                        .child("lib", "lib.rs", |c| c.icon(ICON))
                })
                .node("docs", "docs", |n| n.child("readme", "README.md", |c| c)),
        )
    });
    ui.leave().settle();
}

/// Opening an item moves the rows below it down over the theme's normal
/// duration rather than at once.
#[test]
fn a_tree_item_opens_over_time() {
    let Some(mut ui) = session(240, 240) else {
        return;
    };
    mount_tree(&mut ui);
    let rows = all(&ui, "cn-tree-row");
    let docs_at = |ui: &Headless| {
        let docs = all(ui, "cn-tree-row")[3];
        ui.tree().get_visual_render_bounds(docs).unwrap().y
    };
    let shut = ui.tree().get_absolute_bounds(rows[1]).unwrap().y;
    click(&mut ui, rows[0]);
    ui.advance(50);
    let partway = docs_at(&ui);
    ui.settle();
    let open = docs_at(&ui);
    assert!(
        open > shut + 50.0,
        "the item did not open: {shut} to {open}"
    );
    assert!(
        partway > shut + 1.0 && partway < open - 1.0,
        "the rows below jumped: {partway} between {shut} and {open}"
    );
}

/// After a click, Down moves the focus ring through the shown rows, Left on
/// a child returns to its parent, and Left again closes it.
#[test]
fn the_keys_walk_a_tree() {
    let Some(mut ui) = session(240, 240) else {
        return;
    };
    mount_tree(&mut ui);
    let rows = all(&ui, "cn-tree-row");
    click(&mut ui, rows[0]);
    ui.settle();
    let ringed = |ui: &Headless| -> Vec<bool> {
        all(ui, "cn-tree-row")
            .into_iter()
            .map(|r| {
                ui.tree()
                    .element_registry()
                    .attribute(r, "data-focus-visible")
                    .is_some()
            })
            .collect()
    };
    assert!(!ringed(&ui).contains(&true), "a click shows no ring");

    // The rows of a group are built when it first opens: docs is shut, so README.md is not.
    ui.key(DOWN).key(DOWN).settle();
    assert_eq!(ringed(&ui), [false, false, true, false], "on lib.rs");

    ui.key(LEFT).settle();
    assert_eq!(ringed(&ui), [true, false, false, false], "back on src");
    ui.key(LEFT).settle();
    let group = all(&ui, "cn-tree-group")[0];
    let height = ui.tree().get_absolute_bounds(group).unwrap().height;
    assert!(height < 1.0, "Left did not close src: {height}");
}

/// Collapsing the sidebar eases its width, and the content beside it moves
/// with it frame by frame.
#[test]
fn the_content_follows_the_sidebar() {
    let Some(mut ui) = session(400, 200) else {
        return;
    };
    let collapsed = signal(false);
    ui.mount(move |_| {
        div().w(400.0).h(200.0).child(
            blinc_cn::sidebar(collapsed)
                .section("Mail")
                .item_active("Inbox", ICON, || {})
                .item("Drafts", ICON, || {})
                .content(|_| {
                    div()
                        .w_full()
                        .h_full()
                        .bg(blinc_core::Color::rgb(1.0, 0.0, 0.0))
                }),
        )
    });
    ui.leave().settle();
    let open = red_edge(&mut ui, 150);
    ui.click(".cn-sidebar-toggle");
    ui.advance(60);
    let partway = red_edge(&mut ui, 150);
    ui.settle();
    let shut = red_edge(&mut ui, 150);
    assert!(collapsed.get(), "the toggle did not collapse the rail");
    assert_eq!(open, 200, "open width");
    assert_eq!(shut, 41, "collapsed width");
    assert!(
        partway < open - 5 && partway > shut + 5,
        "the content jumped: {partway} between {open} and {shut}"
    );
}
