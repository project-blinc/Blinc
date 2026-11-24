//! What a change of the hovered node costs the state-style pass, in a tree with many nodes.

use blinc_layout::css_parser::Stylesheet;
use blinc_layout::div::div;
use blinc_layout::event_router::EventRouter;
use blinc_layout::renderer::RenderTree;

const ROW: f32 = 30.0;

fn list(rows: usize, css: &str) -> (RenderTree, EventRouter) {
    let mut list = div().flex_col().w(300.0).class("group");
    for _ in 0..rows {
        list = list.child(div().w_full().h(ROW - 2.0).mb(0.5).class("row"));
    }
    let mut tree = RenderTree::from_element(&div().w(300.0).h(600.0).child(list));
    tree.set_stylesheet(Stylesheet::parse(css).expect("css"));
    tree.apply_stylesheet_layout_overrides();
    tree.apply_stylesheet_base_styles();
    tree.compute_layout(300.0, 600.0);
    (tree, EventRouter::new())
}

/// Not a check: milliseconds a hover change costs, at `ROWS` rows (default 20000).
#[test]
#[ignore]
fn what_a_hover_change_costs_the_state_pass() {
    let rows: usize = std::env::var("ROWS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20_000);
    // A theme has many state rules; `RULES` more of them (default 60), none matching a row.
    let extra: usize = std::env::var("RULES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60);
    let mut css = String::from(
        ".row { border-radius: 6px; } .row:hover { background: #ffffff33; } \
         .row + .row:hover { border-color: #ffffff55; } ",
    );
    for i in 0..extra {
        css.push_str(&format!(
            ".k{i}:hover {{ opacity: 0.9; }} .k{i}:focus {{ outline-width: 2px; }} "
        ));
    }
    let (mut tree, mut router) = list(rows, &css);
    let mut costs = Vec::new();
    for step in 0..30 {
        let _ = router.on_mouse_move(&tree, 60.0, 15.0 + ROW * (step % 18) as f32);
        let started = std::time::Instant::now();
        tree.apply_stylesheet_state_styles(&router);
        costs.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    costs.sort_by(|a, b| a.total_cmp(b));
    eprintln!(
        "{rows} rows, {extra} more state rules: state pass per hover change, ms: median {:.2}  p90 {:.2}  max {:.2}",
        costs[costs.len() / 2],
        costs[costs.len() * 9 / 10],
        costs[costs.len() - 1]
    );
}
