#![cfg(feature = "headless")]
//! A line or area chart draws its series in the palette, marks the category
//! under the pointer with a tooltip of its values, and eases to new data.

use blinc_cn::{
    ChartBand, ChartSeries, PieSlice, area_chart, bar_chart, comparison_bar_chart, histogram,
    line_chart, pie_chart, spark_line, threshold_line_chart,
};
use blinc_core::reactive::signal;
use blinc_layout::LayoutNodeId;
use blinc_layout::div::div;
use blinc_layout::renderer::ElementType;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

const W: u32 = 480;
const H: u32 = 300;

fn session() -> Option<Headless> {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return None;
    }
    let mut ui = Headless::new(W, H).ok()?;
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Dark,
    );
    Some(ui)
}

fn months() -> Vec<String> {
    ["Jan", "Feb", "Mar", "Apr", "May", "Jun"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

fn mount_area(ui: &mut Headless) {
    ui.mount(|_| {
        div()
            .w(W as f32)
            .h(H as f32)
            .padding(blinc_layout::units::px(16.0))
            .child(
                area_chart()
                    .series("Desktop", &[186.0, 305.0, 237.0, 73.0, 209.0, 214.0])
                    .series("Mobile", &[80.0, 200.0, 120.0, 190.0, 130.0, 140.0])
                    .labels(months())
                    .stacked(),
            )
    });
    ui.leave().settle();
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

/// The text under each node with `class`, joined.
fn texts(ui: &Headless, class: &str) -> Vec<String> {
    let tree = ui.tree();
    all(ui, class)
        .into_iter()
        .map(|node| {
            let mut out = String::new();
            let mut stack = vec![node];
            while let Some(n) = stack.pop() {
                if let Some(r) = tree.get_render_node(n) {
                    if let ElementType::Text(t) = &r.element_type {
                        out.push_str(&t.content);
                    }
                }
                let mut children = tree.layout_tree.children(n);
                children.reverse();
                stack.extend(children);
            }
            out
        })
        .collect()
}

fn token(name: &str) -> [u8; 3] {
    let sheet = blinc_layout::css_parser::active_stylesheet().expect("a stylesheet");
    let c = sheet.resolve_color(&format!("var(--{name})")).expect(name);
    [
        (c.r * 255.0) as u8,
        (c.g * 255.0) as u8,
        (c.b * 255.0) as u8,
    ]
}

fn near(a: [u8; 3], b: [u8; 3], tolerance: u8) -> bool {
    a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= tolerance)
}

/// The topmost row in column `x` painted `color`, if any.
fn top_of(ui: &mut Headless, x: u32, color: [u8; 3]) -> Option<u32> {
    let frame = ui.snapshot().unwrap();
    (0..frame.height).find(|y| {
        let p = frame.get_pixel(x, *y).unwrap();
        near([p[0], p[1], p[2]], color, 24)
    })
}

/// The centre of category `i` of six, from the label under it.
fn category_x(ui: &Headless, i: usize) -> f32 {
    let label = all(ui, "cn-chart-axis")
        .into_iter()
        .filter(|n| texts_of(ui, *n) == months()[i])
        .next()
        .expect("the category's label");
    let b = ui.tree().get_absolute_bounds(label).unwrap();
    b.x + b.width / 2.0
}

fn texts_of(ui: &Headless, node: LayoutNodeId) -> String {
    let tree = ui.tree();
    let mut out = String::new();
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if let Some(r) = tree.get_render_node(n) {
            if let ElementType::Text(t) = &r.element_type {
                out.push_str(&t.content);
            }
        }
        stack.extend(tree.layout_tree.children(n));
    }
    out
}

#[test]
fn the_series_are_drawn_in_the_palette_with_their_labels() {
    let Some(mut ui) = session() else { return };
    mount_area(&mut ui);
    let x = category_x(&ui, 1) as u32;
    assert!(
        top_of(&mut ui, x, token("chart-1")).is_some(),
        "no --chart-1 line"
    );
    assert!(
        top_of(&mut ui, x, token("chart-2")).is_some(),
        "no --chart-2 line"
    );
    assert_eq!(texts(&ui, "cn-chart-legend-name"), ["Desktop", "Mobile"]);
    // Stacked: the value axis reaches the piled 505.
    let ticks = texts(&ui, "cn-chart-ticks");
    assert!(ticks[0].starts_with("600"), "{ticks:?}");
    ui.assert_snapshot("cn_chart_area");
}

#[test]
fn the_pointer_shows_the_values_under_it() {
    let Some(mut ui) = session() else { return };
    mount_area(&mut ui);
    assert!(
        all(&ui, "cn-chart-tooltip").is_empty(),
        "a tooltip before any hover"
    );
    let plot = ui
        .tree()
        .get_absolute_bounds(all(&ui, "cn-chart-plot")[0])
        .unwrap();
    ui.move_to(category_x(&ui, 2), plot.y + 60.0).settle();
    assert_eq!(texts(&ui, "cn-chart-tooltip-title"), ["Mar"]);
    assert_eq!(texts(&ui, "cn-chart-tooltip-name"), ["Desktop", "Mobile"]);
    assert_eq!(texts(&ui, "cn-chart-tooltip-value"), ["237", "120"]);
    let tip = ui
        .tree()
        .get_absolute_bounds(all(&ui, "cn-chart-tooltip")[0])
        .unwrap();
    assert!(tip.x > category_x(&ui, 2), "left of the point at the left");
    ui.assert_snapshot("cn_chart_hover");

    // Past 60% of the width it goes to the left of the point.
    ui.move_to(category_x(&ui, 5), plot.y + 60.0).settle();
    assert_eq!(texts(&ui, "cn-chart-tooltip-title"), ["Jun"]);
    let tip = ui
        .tree()
        .get_absolute_bounds(all(&ui, "cn-chart-tooltip")[0])
        .unwrap();
    assert!(
        tip.x + tip.width < category_x(&ui, 5),
        "right of the point at the right"
    );

    ui.leave().settle();
    assert!(
        all(&ui, "cn-chart-tooltip").is_empty(),
        "the tooltip stayed"
    );
}

#[test]
fn new_data_eases_from_the_old() {
    let Some(mut ui) = session() else { return };
    let data = signal(vec![ChartSeries::new("Load", vec![10.0, 10.0, 10.0])]);
    ui.mount(move |_| {
        div()
            .w(W as f32)
            .h(H as f32)
            .padding(blinc_layout::units::px(16.0))
            .child(line_chart().data(data).min(0.0).max(100.0))
    });
    ui.leave().settle();
    let x = {
        let plot = ui
            .tree()
            .get_absolute_bounds(all(&ui, "cn-chart-plot")[0])
            .unwrap();
        (plot.x + plot.width / 2.0) as u32
    };
    let line = token("chart-1");
    let low = top_of(&mut ui, x, line).expect("the line");

    data.set(vec![ChartSeries::new("Load", vec![90.0, 90.0, 90.0])]);
    ui.advance(120);
    let partway = top_of(&mut ui, x, line).expect("the line");
    ui.settle();
    let high = top_of(&mut ui, x, line).expect("the line");
    assert!(high + 50 < low, "the line did not rise: {low} to {high}");
    assert!(
        partway < low - 5 && partway > high + 5,
        "the line jumped: {partway} between {low} and {high}"
    );
}

#[test]
fn crowded_labels_thin_out() {
    let Some(mut ui) = session() else { return };
    let labels: Vec<String> = (1..=40).map(|d| format!("Day {d}")).collect();
    let values: Vec<f64> = (0..40).map(|i| (i % 7) as f64).collect();
    ui.mount(move |_| {
        div().w(W as f32).h(H as f32).child(
            line_chart()
                .series("Visits", &values)
                .labels(labels.clone()),
        )
    });
    ui.leave().settle();
    let registry = ui.tree().element_registry();
    let hidden = all(&ui, "cn-chart-axis")
        .into_iter()
        .filter(|n| registry.attribute(*n, "data-hidden").is_some())
        .count();
    assert!(hidden > 20, "only {hidden} of 40 labels hidden in 480px");
    assert!(hidden < 40);
}

#[test]
fn grouped_bars_mark_the_category_under_the_pointer() {
    let Some(mut ui) = session() else { return };
    ui.mount(|_| {
        div()
            .w(W as f32)
            .h(H as f32)
            .padding(blinc_layout::units::px(16.0))
            .child(
                bar_chart()
                    .series("Desktop", &[186.0, 305.0, 237.0, 73.0, 209.0, 214.0])
                    .series("Mobile", &[80.0, 200.0, 120.0, 190.0, 130.0, 140.0])
                    .labels(months()),
            )
    });
    ui.leave().settle();
    // Side by side: in Feb's slot, the Desktop bar then the Mobile bar.
    let x = category_x(&ui, 1);
    let desktop = top_of(&mut ui, (x - 8.0) as u32, token("chart-1")).expect("a Desktop bar");
    let mobile = top_of(&mut ui, (x + 8.0) as u32, token("chart-2")).expect("a Mobile bar");
    assert!(
        desktop < mobile,
        "305 is taller than 200: {desktop} vs {mobile}"
    );
    // Bars start at zero.
    assert!(texts(&ui, "cn-chart-ticks")[0].ends_with('0'));
    let plot = ui
        .tree()
        .get_absolute_bounds(all(&ui, "cn-chart-plot")[0])
        .unwrap();
    ui.move_to(x, plot.y + 40.0).settle();
    assert_eq!(texts(&ui, "cn-chart-tooltip-title"), ["Feb"]);
    assert_eq!(texts(&ui, "cn-chart-tooltip-value"), ["305", "200"]);
    ui.assert_snapshot("cn_chart_bars_hover");
}

#[test]
fn horizontal_bars_put_the_categories_beside() {
    let Some(mut ui) = session() else { return };
    let names: Vec<String> = ["React", "Vue", "Svelte", "Angular"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    ui.mount(move |_| {
        div()
            .w(W as f32)
            .h(H as f32)
            .padding(blinc_layout::units::px(16.0))
            .child(
                bar_chart()
                    .series("Popularity", &[85.0, 65.0, 45.0, 40.0])
                    .labels(names.clone())
                    .horizontal(),
            )
    });
    ui.leave().settle();
    let react = all(&ui, "cn-chart-axis")
        .into_iter()
        .find(|n| texts_of(&ui, *n) == "React")
        .expect("the React label");
    let label = ui.tree().get_absolute_bounds(react).unwrap();
    let plot = ui
        .tree()
        .get_absolute_bounds(all(&ui, "cn-chart-plot")[0])
        .unwrap();
    assert!(label.x < plot.x + 80.0, "the category is beside the plot");
    // The React bar runs right from the axis, level with its label.
    let y = (label.y + label.height / 2.0) as u32;
    let frame = ui.snapshot().unwrap();
    let painted = (0..frame.width)
        .filter(|x| {
            let p = frame.get_pixel(*x, y).unwrap();
            near([p[0], p[1], p[2]], token("chart-1"), 24)
        })
        .count();
    assert!(painted > 150, "the React bar is {painted}px long");
    ui.move_to(plot.x + 200.0, label.y + label.height / 2.0)
        .settle();
    assert_eq!(texts(&ui, "cn-chart-tooltip-title"), ["React"]);
    ui.assert_snapshot("cn_chart_bars_horizontal");
}

#[test]
fn stacked_bars_pile() {
    let Some(mut ui) = session() else { return };
    ui.mount(|_| {
        div()
            .w(W as f32)
            .h(H as f32)
            .padding(blinc_layout::units::px(16.0))
            .child(
                bar_chart()
                    .series("Desktop", &[186.0, 305.0, 237.0, 73.0, 209.0, 214.0])
                    .series("Mobile", &[80.0, 200.0, 120.0, 190.0, 130.0, 140.0])
                    .labels(months())
                    .stacked(),
            )
    });
    ui.leave().settle();
    // Feb piles to 505, so the axis reaches 600; Mobile sits on Desktop.
    assert!(texts(&ui, "cn-chart-ticks")[0].starts_with("600"));
    let x = category_x(&ui, 1) as u32;
    let mobile = top_of(&mut ui, x, token("chart-2")).expect("Mobile");
    let desktop = top_of(&mut ui, x, token("chart-1")).expect("Desktop");
    assert!(mobile < desktop, "Mobile is on top: {mobile} vs {desktop}");
    ui.assert_snapshot("cn_chart_bars_stacked");
}

fn browsers() -> Vec<PieSlice> {
    vec![
        PieSlice::new("Chrome", 275.0),
        PieSlice::new("Safari", 200.0),
        PieSlice::new("Firefox", 187.0),
        PieSlice::new("Edge", 173.0),
        PieSlice::new("Other", 90.0),
    ]
}

#[test]
fn a_pie_shows_its_slices_and_the_one_under_the_pointer() {
    let Some(mut ui) = session() else { return };
    ui.mount(|_| {
        div()
            .w(W as f32)
            .h(H as f32)
            .padding(blinc_layout::units::px(16.0))
            .child(pie_chart().data(browsers()))
    });
    ui.leave().settle();
    assert_eq!(
        texts(&ui, "cn-chart-legend-name"),
        ["Chrome", "Safari", "Firefox", "Edge", "Other"]
    );
    let plot = ui
        .tree()
        .get_absolute_bounds(all(&ui, "cn-chart-plot")[0])
        .unwrap();
    let (cx, cy) = (plot.x + plot.width / 2.0, plot.y + plot.height / 2.0);
    // Chrome starts at the top and runs clockwise: right of the middle.
    let frame = ui.snapshot().unwrap();
    let p = frame
        .get_pixel((cx + 50.0) as u32, (cy - 20.0) as u32)
        .unwrap();
    assert!(near([p[0], p[1], p[2]], token("chart-1"), 24), "{p:?}");
    ui.assert_snapshot("cn_chart_pie");

    ui.move_to(cx + 50.0, cy - 20.0).settle();
    assert_eq!(texts(&ui, "cn-chart-tooltip-name"), ["Chrome"]);
    assert_eq!(texts(&ui, "cn-chart-tooltip-value"), ["275 · 30%"]);
    ui.leave().settle();
    assert!(all(&ui, "cn-chart-tooltip").is_empty());
}

#[test]
fn a_donut_writes_its_total_in_the_hole() {
    let Some(mut ui) = session() else { return };
    ui.mount(|_| {
        div()
            .w(W as f32)
            .h(H as f32)
            .padding(blinc_layout::units::px(16.0))
            .child(pie_chart().data(browsers()).donut(0.6).total("Visitors"))
    });
    ui.leave().settle();
    assert_eq!(texts(&ui, "cn-chart-pie-total"), ["925"]);
    assert_eq!(texts(&ui, "cn-chart-pie-caption"), ["Visitors"]);
    let plot = ui
        .tree()
        .get_absolute_bounds(all(&ui, "cn-chart-plot")[0])
        .unwrap();
    let (cx, cy) = (plot.x + plot.width / 2.0, plot.y + plot.height / 2.0);
    // The hole is left as it was: no slice colour at the middle.
    let frame = ui.snapshot().unwrap();
    let p = frame.get_pixel(cx as u32, (cy + 25.0) as u32).unwrap();
    for k in 1..=5 {
        assert!(
            !near([p[0], p[1], p[2]], token(&format!("chart-{k}")), 24),
            "{p:?}"
        );
    }
    ui.assert_snapshot("cn_chart_donut");
}

#[test]
fn a_pie_sweeps_in() {
    let Some(mut ui) = session() else { return };
    ui.mount(|_| {
        div()
            .w(W as f32)
            .h(H as f32)
            .padding(blinc_layout::units::px(16.0))
            .child(pie_chart().data(vec![PieSlice::new("All", 1.0)]))
    });
    ui.leave();
    ui.advance(80);
    let plot = ui
        .tree()
        .get_absolute_bounds(all(&ui, "cn-chart-plot")[0])
        .unwrap();
    let (cx, cy) = (plot.x + plot.width / 2.0, plot.y + plot.height / 2.0);
    // Left of the middle is the last quarter of the sweep: not reached yet.
    let at = |ui: &mut Headless| {
        let frame = ui.snapshot().unwrap();
        let p = frame.get_pixel((cx - 50.0) as u32, cy as u32).unwrap();
        near([p[0], p[1], p[2]], token("chart-1"), 24)
    };
    assert!(!at(&mut ui), "the pie was whole at once");
    ui.settle();
    assert!(at(&mut ui), "the pie never closed");
}

#[test]
fn a_sparkline_is_only_its_line() {
    let Some(mut ui) = session() else { return };
    ui.mount(|_| {
        div()
            .w(W as f32)
            .h(H as f32)
            .padding(blinc_layout::units::px(16.0))
            .child(spark_line(vec![1.0, 3.0, 2.0, 5.0, 4.0]).color("var(--chart-2)"))
    });
    ui.leave().settle();
    assert!(
        all(&ui, "cn-chart-axis").is_empty(),
        "a sparkline has no labels"
    );
    let plot = ui
        .tree()
        .get_absolute_bounds(all(&ui, "cn-chart-plot")[0])
        .unwrap();
    assert_eq!((plot.width, plot.height), (100.0, 24.0));
    let x = (plot.x + plot.width / 2.0) as u32;
    assert!(
        top_of(&mut ui, x, token("chart-2")).is_some(),
        "no line in its colour"
    );
}

#[test]
fn a_threshold_chart_colours_dots_by_band() {
    let Some(mut ui) = session() else { return };
    ui.mount(|_| {
        div()
            .w(W as f32)
            .h(H as f32)
            .padding(blinc_layout::units::px(16.0))
            .child(
                threshold_line_chart()
                    .values(vec![12.0, 14.0, 20.0, 28.0])
                    .band(ChartBand::new(0.0, 16.67).label("60 fps"))
                    .band(ChartBand::new(16.67, 33.33).label("30 fps"))
                    .baseline(16.67),
            )
    });
    ui.leave().settle();
    assert_eq!(texts(&ui, "cn-chart-band-label"), ["60 fps", "30 fps"]);
    assert!(
        all(&ui, "cn-chart-legend").is_empty(),
        "a legend for one series"
    );
    ui.assert_snapshot("cn_chart_threshold");
}

#[test]
fn a_histogram_counts_into_bins() {
    let Some(mut ui) = session() else { return };
    ui.mount(|_| {
        div()
            .w(W as f32)
            .h(H as f32)
            .padding(blinc_layout::units::px(16.0))
            .child(
                histogram(vec![0.0, 1.0, 1.5, 2.0, 9.0, 10.0])
                    .bins(5)
                    .marker(5.0),
            )
    });
    ui.leave().settle();
    // Five bins from 0 to 10; the first, from 0, holds 0, 1 and 1.5.
    let plot = ui
        .tree()
        .get_absolute_bounds(all(&ui, "cn-chart-plot")[0])
        .unwrap();
    let zero = all(&ui, "cn-chart-axis")
        .into_iter()
        .filter(|n| texts_of(&ui, *n) == "0")
        .map(|n| ui.tree().get_absolute_bounds(n).unwrap())
        .max_by(|a, b| a.y.total_cmp(&b.y))
        .expect("the first bin's label");
    ui.move_to(zero.x + zero.width / 2.0, plot.y + 60.0)
        .settle();
    assert_eq!(texts(&ui, "cn-chart-tooltip-title"), ["0"]);
    assert_eq!(texts(&ui, "cn-chart-tooltip-value"), ["3"]);
    ui.assert_snapshot("cn_chart_histogram");
}

#[test]
fn a_comparison_notes_each_change() {
    let Some(mut ui) = session() else { return };
    ui.mount(|_| {
        div()
            .w(W as f32)
            .h(H as f32)
            .padding(blinc_layout::units::px(16.0))
            .child(
                comparison_bar_chart()
                    .item("Render", 12.5, 14.2)
                    .item("Layout", 3.2, 3.0)
                    .item("Composite", 2.1, 2.3),
            )
    });
    ui.leave().settle();
    assert_eq!(texts(&ui, "cn-chart-note"), ["+13.6%", "-6.3%", "+9.5%"]);
    let registry = ui.tree().element_registry();
    let tones: Vec<Option<String>> = all(&ui, "cn-chart-note")
        .into_iter()
        .map(|n| registry.attribute(n, "data-tone"))
        .collect();
    assert_eq!(tones, [Some("bad".into()), Some("good".into()), None]);
    assert_eq!(texts(&ui, "cn-chart-legend-name"), ["Baseline", "Current"]);
    ui.assert_snapshot("cn_chart_comparison");
}
