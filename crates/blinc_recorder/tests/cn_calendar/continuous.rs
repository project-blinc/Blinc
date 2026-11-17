use super::{all, has, mount, session};
use blinc_core::reactive::signal;
use blinc_layout::renderer::ElementType;
use blinc_recorder::Headless;

fn caption(ui: &Headless) -> Vec<String> {
    let mut nodes = vec![ui.find(".cn-calendar-caption").unwrap()];
    let mut text = Vec::new();
    while let Some(node) = nodes.pop() {
        nodes.extend(ui.tree().layout_tree.children(node).into_iter().rev());
        if let Some(render) = ui.tree().get_render_node(node)
            && let ElementType::Text(data) = &render.element_type
        {
            text.push(data.content.clone());
        }
    }
    text
}

fn assert_months_carried(ui: &Headless) {
    for grid in all(ui, "cn-calendar-grid") {
        let tree = ui.tree();
        let playing = tree.stable_id(grid).is_some_and(|stable| {
            tree.css_anim_store()
                .lock()
                .unwrap()
                .animations
                .get(&stable)
                .is_some_and(|a| a.is_playing)
        });
        if playing {
            let props = &tree.get_render_node(grid).unwrap().props;
            assert!(
                tree.css_carried(grid),
                "sliding month lost its group: opacity={}, transform={:?}",
                props.opacity,
                props.transform,
            );
        }
    }
}

#[test]
fn fast_continuous_swipes_in_one_direction_keep_cached_months_aligned() {
    let Some(mut ui) = session() else { return };
    ui.gate_state_styles().gate_animation_apply();
    mount(&mut ui, signal(None), signal(Vec::new()));
    let node = ui.find(".cn-calendar-months").unwrap();
    let b = ui.tree().get_absolute_bounds(node).unwrap();
    ui.move_to(b.x + b.width * 0.5, b.y + b.height * 0.5);
    ui.snapshot().unwrap();
    let months = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    for turn in 0..12 {
        ui.scroll_start();
        for _ in 0..6 {
            ui.scroll(-10.0, 0.0).advance(8);
            ui.snapshot_fast().unwrap();
            assert_months_carried(&ui);
        }
        ui.scroll_end().scroll_momentum_end();
        let month = 11 + turn;
        assert_eq!(
            caption(&ui),
            [
                months[month % 12].to_string(),
                (2026 + month / 12).to_string()
            ]
        );
        // Every gesture goes forward, with no settle between gestures. Exiting months overlap.
        if turn % 3 == 2 {
            assert!(all(&ui, "cn-calendar-grid").len() > 1);
            let cached = ui.snapshot_fast().unwrap();
            let full = ui.snapshot().unwrap();
            let changed = cached
                .data
                .chunks_exact(4)
                .zip(full.data.chunks_exact(4))
                .filter(|(a, b)| a.iter().zip(*b).any(|(a, b)| a.abs_diff(*b) > 2))
                .count();
            assert_eq!(
                changed, 0,
                "cached pixels drifted on continuous turn {turn}"
            );
        }
    }
    ui.settle();
    assert_eq!(
        all(&ui, "cn-calendar-grid").len(),
        1,
        "finished exits must be removed"
    );
    let index = ui.tree().element_registry().class_to_nodes_index();
    for (class, count) in [
        ("cn-calendar-page", 1),
        ("cn-calendar-grid", 1),
        ("cn-calendar-day", 42),
    ] {
        let nodes = &index[class];
        assert_eq!(nodes.len(), count, "retired {class} nodes remain indexed");
        assert!(
            nodes
                .iter()
                .all(|node| ui.tree().get_render_node(*node).is_some())
        );
    }
    let first = all(&ui, "cn-calendar-day")
        .into_iter()
        .position(|node| !has(&ui, node, "data-outside"))
        .unwrap();
    assert_eq!(first, 1, "twelve forward swipes show November 2027");
}

#[test]
fn a_month_slide_can_retain_rows_after_its_entry_settles() {
    let Some(mut ui) = session() else { return };
    ui.gate_state_styles().gate_animation_apply();
    mount(&mut ui, signal(None), signal(Vec::new()));
    let node = ui.find(".cn-calendar-months").unwrap();
    let b = ui.tree().get_absolute_bounds(node).unwrap();
    ui.move_to(b.x + b.width * 0.5, b.y + b.height * 0.5);
    ui.scroll_start()
        .scroll(-60.0, 0.0)
        .scroll_end()
        .scroll_momentum_end()
        .settle();
    let grid = all(&ui, "cn-calendar-grid")[0];
    assert!(
        ui.tree()
            .get_render_node(grid)
            .unwrap()
            .props
            .transform
            .is_some(),
        "the completed entry must leave its final identity transform"
    );
    ui.snapshot().unwrap();
    ui.scroll_start()
        .scroll(-60.0, 0.0)
        .scroll_end()
        .advance(16);
    assert_months_carried(&ui);
    let cached = ui.snapshot_fast().unwrap();
    let full = ui.snapshot().unwrap();
    assert!(
        cached
            .data
            .iter()
            .zip(&full.data)
            .all(|(a, b)| a.abs_diff(*b) <= 2),
        "the retained exit must match its full group paint"
    );
}

#[test]
fn swiping_suspends_day_hover_until_the_gesture_ends() {
    let Some(mut ui) = session() else { return };
    ui.gate_state_styles().gate_animation_apply();
    mount(&mut ui, signal(None), signal(Vec::new()));
    // November starts on Sunday, so the first cell is the fixture's today.
    let today = ".cn-calendar-day";
    ui.hover(today).settle();
    assert!(ui.is_hovered(today));

    ui.scroll_start().scroll(-12.0, 0.0).advance(16);
    assert!(
        !ui.is_hovered(today),
        "a stationary pointer must lose day hover"
    );
    assert!(
        ui.is_hovered(".cn-calendar-months"),
        "the gesture owner keeps leave detection"
    );
    let node = ui.find(today).unwrap();
    let bounds = ui.tree().get_absolute_bounds(node).unwrap();
    ui.move_to(
        bounds.x + bounds.width / 2.0,
        bounds.y + bounds.height / 2.0,
    );
    assert!(
        !ui.is_hovered(today),
        "moving the pointer must not hover days during the swipe"
    );

    ui.scroll_end().scroll_momentum_end().settle();
    ui.hover(today).settle();
    assert!(
        ui.is_hovered(today),
        "ordinary hover resumes after the gesture"
    );
}
