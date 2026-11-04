use crate::{
    DEMO_CSS, FONTS, SLUGS,
    demos::{DemoState, build},
    theme_bundle_for,
};
use blinc_app::web::{WebApp, WebInput};
use blinc_platform::Modifiers;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct CatalogRuntime {
    app: WebApp,
    state: DemoState,
    frames: u32,
    family: String,
}

#[wasm_bindgen]
pub async fn create_runtime(
    canvas: web_sys::OffscreenCanvas,
    width: f32,
    height: f32,
    dpr: f64,
    slug: String,
    dark: bool,
    family: String,
    css: String,
) -> Result<CatalogRuntime, JsValue> {
    console_error_panic_hook::set_once();
    if !SLUGS.contains(&slug.as_str()) {
        return Err(JsValue::from_str("Unknown demo"));
    }
    let mut bundle = theme_bundle_for(&family);
    bundle.css_sources.clear();
    blinc_theme::ThemeState::init(
        bundle,
        if dark {
            blinc_theme::ColorScheme::Dark
        } else {
            blinc_theme::ColorScheme::Light
        },
    );
    let mut app = WebApp::new_offscreen(canvas, width, height, dpr)
        .await
        .map_err(error)?;
    for font in FONTS {
        app.load_font_data(font.to_vec());
    }
    app.context_mut().add_css(blinc_cn::cn_styles::CN_STYLES);
    app.context_mut().add_css(DEMO_CSS);
    app.context_mut().set_css("preview", &css).map_err(error)?;
    let state = DemoState::default();
    let fixture = state.clone();
    app.set_ui_builder(move |ctx| build(&slug, &fixture, (ctx.width, ctx.height)));
    Ok(CatalogRuntime {
        app,
        state,
        frames: 0,
        family,
    })
}

fn error(value: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&value.to_string())
}

#[wasm_bindgen]
impl CatalogRuntime {
    pub fn frame(&mut self) -> Result<String, JsValue> {
        self.app.run_one_frame().map_err(error)?;
        self.frames += 1;
        Ok(self.status())
    }

    /// Resolve only after the first submitted GPU frame has completed.
    pub async fn warm_frame(&mut self) -> Result<String, JsValue> {
        let state = self.frame()?;
        let completed = js_sys::Promise::new(&mut |resolve, _| {
            self.app
                .blinc_app()
                .queue()
                .on_submitted_work_done(move || {
                    let _ = resolve.call0(&JsValue::NULL);
                });
        });
        wasm_bindgen_futures::JsFuture::from(completed).await?;
        Ok(state)
    }

    pub fn input(
        &mut self,
        json: &str,
        shift: bool,
        ctrl: bool,
        alt: bool,
        meta: bool,
    ) -> Result<bool, JsValue> {
        let event: WebInput = serde_json::from_str(json).map_err(error)?;
        Ok(self.app.dispatch_input(
            event,
            Modifiers {
                shift,
                ctrl,
                alt,
                meta,
            },
        ))
    }

    pub fn resize(&mut self, width: f32, height: f32, dpr: f64) {
        self.app.resize(width, height, dpr);
    }

    pub fn control(&mut self, disabled: bool, progress: f32) {
        self.state.disabled.set(disabled);
        self.state.value.set(progress.clamp(0.0, 100.0));
    }

    pub fn apply_css(&mut self, css: &str) -> Result<(), JsValue> {
        let variables = self
            .app
            .context_mut()
            .stylesheet
            .as_ref()
            .map(|sheet| sheet.variables().clone())
            .unwrap_or_else(|| blinc_theme::ThemeState::get().to_css_variable_map());
        blinc_layout::css_parser::Stylesheet::parse_with_variables(css, &variables)
            .map_err(|error| JsValue::from_str(&format!("CSS: {error}")))?;
        self.app
            .context_mut()
            .set_css("preview", css)
            .map_err(|error| JsValue::from_str(&format!("CSS: {error}")))?;
        self.app.request_rebuild();
        Ok(())
    }

    pub fn theme(&mut self, dark: bool, family: Option<String>) {
        let scheme = if dark {
            blinc_theme::ColorScheme::Dark
        } else {
            blinc_theme::ColorScheme::Light
        };
        let family = family.unwrap_or_else(|| self.family.clone());
        if family == self.family && blinc_theme::ThemeState::get().scheme() == scheme {
            return;
        }
        if family != self.family {
            let mut bundle = theme_bundle_for(&family);
            // The existing CN and application sheets reparse against the new tokens.
            bundle.css_sources.clear();
            blinc_theme::ThemeState::init(bundle, scheme);
            self.family = family;
        } else {
            blinc_theme::ThemeState::get().set_scheme(scheme);
        }
        self.app.request_rebuild();
    }

    pub fn status(&self) -> String {
        serde_json::json!({
            "frames": self.frames, "cursor": self.app.cursor(),
            "family": self.family,
            "backend": if self.app.blinc_app().has_storage_buffers() { "worker-webgpu" } else { "worker-webgl2" },
            "selection": blinc_layout::text_selection::get_selected_text(),
            "editable": blinc_layout::widgets::text_input::has_focused_text_input(),
            "clicks": self.state.clicks.get(), "value": self.state.value.get(),
            "name": self.state.name.get(), "choice": self.state.choice.get(),
            "confirmed": self.state.confirmed.get(),
        }).to_string()
    }
}
