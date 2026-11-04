use super::super::Headless;
use blinc_core::reactive::signal;
use blinc_layout::{binding::IntoReactive, div::div, text::Text};
use blinc_theme::{ColorScheme, HybridTheme};

#[test]
fn select_popover_occludes_background_text_in_both_render_routes() {
    if !Headless::available() {
        return;
    }
    for scheme in [ColorScheme::Light, ColorScheme::Dark] {
        for compositor in [true, false] {
            let mut ui = Headless::new(360, 280).expect("offscreen GPU");
            ui.theme(
                HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
                scheme,
            );
            ui.load_font_data(
                include_bytes!(
                    "../../../../../../examples/blinc_component_demos/assets/NotoSans-Regular.ttf"
                )
                .to_vec(),
            );
            let caption = signal("Selected: rust".to_string());
            let value = signal("rust".to_string());
            ui.mount(move |_| {
                div()
                    .w(360.0)
                    .h(280.0)
                    .child(
                        div().absolute().left(40.0).top(20.0).w(260.0).child(
                            blinc_cn::select(value)
                                .label("Language")
                                .option("rust", "Rust")
                                .option("typescript", "TypeScript")
                                .option("python", "Python"),
                        ),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(44.0)
                            .top(100.0)
                            .child(Text::bound(caption.into_reactive()).size(18.0)),
                    )
            })
            .settle();
            let closed = ui.snapshot().unwrap();
            caption.set(String::new());
            ui.settle();
            assert!(
                closed.diff_pixel_count(&ui.snapshot().unwrap()) > 50,
                "caption must render before opening"
            );
            caption.set("Selected: rust".to_string());
            ui.settle();
            ui.click(".cn-select-trigger").leave().settle();
            let panel = ui
                .tree()
                .get_absolute_bounds(ui.find(".cn-select-content").unwrap())
                .unwrap();
            assert!(
                panel.y < 100.0 && panel.y + panel.height > 125.0,
                "panel must cover the caption: {panel:?}"
            );
            let paint = |ui: &mut Headless| {
                ui.frame(0);
                ui.app
                    .render_tree_with_motion_opt(
                        ui.tree.as_ref().unwrap(),
                        &ui.render_state,
                        &ui.view,
                        compositor.then_some(&ui.texture),
                        ui.width,
                        ui.height,
                        false,
                    )
                    .unwrap();
                ui.read_frame().unwrap()
            };
            let with_caption = paint(&mut ui);
            caption.set(String::new());
            ui.settle();
            let without_caption = paint(&mut ui);
            let mut occluded_pixels = 0;
            for y in 100..124 {
                for x in 46..160 {
                    let before = with_caption.get_pixel(x, y).unwrap();
                    let after = without_caption.get_pixel(x, y).unwrap();
                    assert_eq!(
                        before, after,
                        "background text leaked at ({x}, {y}), {scheme:?}, compositor={compositor}"
                    );
                    occluded_pixels += usize::from(before != closed.get_pixel(x, y).unwrap());
                }
            }
            assert!(
                occluded_pixels > 100,
                "the popover must actually cover the caption"
            );
            ui.click(".cn-select-trigger").settle();
        }
    }
}
