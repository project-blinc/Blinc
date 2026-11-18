//! Browser-only fixture: count full tree paints and compare retained frames with fresh paint.

use std::cell::Cell;
use std::rc::Rc;

use blinc_app::web::{WebApp, WebInput};
use blinc_core::reactive::{Signal, signal};
use blinc_core::{Color, Rect, Transform};
use blinc_layout::canvas::CanvasRenderFn;
use blinc_layout::div::{ElementBuilder, ElementTypeId};
use blinc_layout::element::RenderProps;
use blinc_layout::prelude::*;
use blinc_layout::widgets::text_input::text_input_data;
use blinc_layout::{Canvas, LayoutNodeId, LayoutTree};
use wasm_bindgen::prelude::*;

struct PaintMarker(Canvas);

impl ElementBuilder for PaintMarker {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.0.build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.0.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        &[]
    }

    fn element_type_id(&self) -> ElementTypeId {
        ElementTypeId::Canvas
    }

    fn canvas_render_info(&self) -> Option<CanvasRenderFn> {
        self.0.canvas_render_info()
    }

    fn is_static_canvas(&self) -> bool {
        true
    }
}

#[wasm_bindgen]
pub struct CacheProbe {
    app: WebApp,
    paints: Rc<Cell<u32>>,
    builds: Rc<Cell<u32>>,
    offset: Signal<f32>,
    color: Signal<Color>,
    size: Signal<f32>,
    show_input: Signal<bool>,
}

fn error(value: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&value.to_string())
}

#[wasm_bindgen]
pub async fn create_cache_probe(
    canvas: web_sys::OffscreenCanvas,
    dpr: f64,
    cards: u32,
) -> Result<CacheProbe, JsValue> {
    console_error_panic_hook::set_once();
    blinc_theme::ThemeState::init(
        blinc_theme::HybridTheme::bundle(),
        blinc_theme::ColorScheme::Dark,
    );
    let mut app = WebApp::new_offscreen(canvas, 640.0, 480.0, dpr)
        .await
        .map_err(error)?;
    for font in crate::FONTS {
        app.load_font_data(font.to_vec());
    }
    app.context_mut().add_css(blinc_cn::cn_styles::CN_STYLES);
    app.context_mut().add_css(
        "#hover-target { background: #224466; } #hover-target:hover { background: #cc6633; }",
    );
    let paints = Rc::new(Cell::new(0));
    let builds = Rc::new(Cell::new(0));
    let offset = signal(0.0);
    let color = signal(Color::rgb(0.8, 0.2, 0.3));
    let size = signal(80.0);
    let show_input = signal(false);
    let field = text_input_data();
    let (paint_count, build_count) = (paints.clone(), builds.clone());
    app.set_ui_builder(move |_| {
        build_count.set(build_count.get() + 1);
        let paint_count = paint_count.clone();
        let marker = PaintMarker(
            blinc_layout::canvas(move |ctx, _| {
                paint_count.set(paint_count.get() + 1);
                ctx.fill_rect(
                    Rect::new(0.0, 0.0, 8.0, 8.0),
                    0.0.into(),
                    Color::WHITE.into(),
                );
            })
            .w(8.0)
            .h(8.0),
        );
        let mut ui = div()
            .w_full()
            .h_full()
            .bg(Color::rgb(0.05, 0.07, 0.1))
            .child(
                div()
                    .id("moving")
                    .absolute()
                    .left(24.0)
                    .top(24.0)
                    .w(size)
                    .h(80.0)
                    .bg(color)
                    .bind_transform_from(offset, |x| Transform::translate(x, 0.0))
                    .child(marker)
                    .child(text("Moving card").size(14.0)),
            )
            .child(
                div()
                    .id("hover-target")
                    .absolute()
                    .left(320.0)
                    .top(24.0)
                    .w(120.0)
                    .h(80.0)
                    .child(text("Hover").size(14.0)),
            )
            .child(
                div()
                    .absolute()
                    .left(24.0)
                    .top(140.0)
                    .w(260.0)
                    .visible(show_input)
                    .child(blinc_cn::input(&field).placeholder("Type here")),
            );
        for i in 0..cards {
            ui = ui.child(
                div()
                    .absolute()
                    .left(24.0 + (i % 32) as f32 * 18.0)
                    .top(220.0 + (i / 32) as f32 * 18.0)
                    .w(14.0)
                    .h(14.0)
                    .bg(Color::rgb(0.2, 0.3, 0.4)),
            );
        }
        ui
    });
    Ok(CacheProbe {
        app,
        paints,
        builds,
        offset,
        color,
        size,
        show_input,
    })
}

#[wasm_bindgen]
impl CacheProbe {
    pub fn frame(&mut self, force_full: bool) -> Result<String, JsValue> {
        if force_full {
            blinc_layout::request_redraw();
        }
        self.app.run_one_frame().map_err(error)?;
        Ok(serde_json::json!({
            "paints": self.paints.get(), "builds": self.builds.get(),
            "gpu": self.app.blinc_app().has_storage_buffers(),
        })
        .to_string())
    }

    pub fn change(&mut self, kind: &str, value: f32) {
        match kind {
            "translate" => self.offset.set(value),
            "color" => self.color.set(Color::rgb(value, 0.2, 0.3)),
            "width" => self.size.set(value),
            "input" => self.show_input.set(value != 0.0),
            "rebuild" => self.app.request_rebuild(),
            _ => panic!("unknown probe update"),
        }
    }

    pub fn input(&mut self, input: &str) -> Result<(), JsValue> {
        let input: WebInput = serde_json::from_str(input).map_err(error)?;
        self.app.dispatch_input(input, Default::default());
        Ok(())
    }

    pub fn css(&mut self, source: &str) -> Result<(), JsValue> {
        self.app
            .context_mut()
            .set_css("probe", source)
            .map_err(error)?;
        self.app.request_rebuild();
        Ok(())
    }

    pub fn resize(&mut self, width: f32, height: f32, dpr: f64) {
        self.app.resize(width, height, dpr);
    }
}
