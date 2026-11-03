//! Fluid Surface Demo
//!
//! Combines a canvas flow shader with `pointer-query` CSS-driven interaction.
//! A central card renders a pointer-reactive fluid shader, while surrounding
//! labels respond to cursor proximity via `calc(env(pointer-distance))`.
//!
//! Run with: cargo run -p blinc_app_examples --example fluid_demo

use blinc_app::prelude::*;
use blinc_app::windowed::WindowedApp;
use blinc_app::windowed::WindowedContext;
use blinc_core::FlowShader;
use blinc_core::{Color, Shadow};
use blinc_layout::flow;

// Silk satin: pale steel blue with very subtle S-curve folds. The cursor angle drives the fold
// direction and its distance their visibility.
// Layout: Still(TL) Mist(TR) | Wave(L) [card] Ocean(R) | Storm(BL) Abyss(BR)
const FLUID: FlowShader = flow!(fluid, fragment, {

    input uv: builtin(uv);
    input time: builtin(time);
    input pointer: builtin(pointer);

    /* Pointer position → mode axes */
    node cursor = clamp(pointer, vec2(0.0, 0.0), vec2(1.0, 1.0));
    node px = cursor.x - 0.5;
    node py = cursor.y - 0.5;
    node xn = cursor.x;
    node yn = cursor.y;
    node angle = atan2(py, px);
    node pdist = clamp(length(vec2(px, py)) * 2.0, 0.0, 1.0);

    /* Mode intensity: yn drives turbulence, xn*yn adds power.
       yn=0 kills intensity → top row (Still,Mist) always calm.
       Still=0  Mist=0  Wave=0.3  Ocean=0.5  Storm=0.6  Abyss=1.0 */
    node mix_factor = 0.6 + xn * 0.4;
    node intensity = yn * mix_factor;
    node speed = mix(0.20, 0.65, intensity);
    node t = time * speed;
    node amp = mix(0.10, 0.38, intensity);
    node freq = mix(3.0, 5.5, intensity);

    /* Fold direction rotates with pointer angle */
    node ca = cos(angle + t * 0.12);
    node sa = sin(angle + t * 0.12);
    node cx = uv.x - 0.5;
    node cy = uv.y - 0.5;
    node rx = cx * ca + cy * sa + 0.5;
    node ry = cx * -sa + cy * ca + 0.5;

    /* Center vignette — folds concentrated in middle */
    node cdist = length(vec2(cx, cy));
    node focus = smoothstep(0.5, 0.08, cdist);
    node wamp = amp * focus;

    /* ---- Height at UV ---- */
    node sx0 = rx + sin(ry * 3.14159 + t) * wamp;
    node sy0 = ry + cos(rx * 2.6 + t * 0.7) * wamp * 0.8;
    node h0 = sin(sx0 * freq + sy0 * freq * 0.5 + t * 0.3) * 0.7
             + sin(sy0 * freq * 1.4 + sx0 * freq * 0.3 + t * 0.5) * 0.2;

    /* ---- Height at UV + (eps, 0) ---- */
    node erx = rx + 0.005;
    node sx1 = erx + sin(ry * 3.14159 + t) * wamp;
    node sy1 = ry + cos(erx * 2.6 + t * 0.7) * wamp * 0.8;
    node h1 = sin(sx1 * freq + sy1 * freq * 0.5 + t * 0.3) * 0.7
             + sin(sy1 * freq * 1.4 + sx1 * freq * 0.3 + t * 0.5) * 0.2;

    /* ---- Height at UV + (0, eps) ---- */
    node ery = ry + 0.005;
    node sx2 = rx + sin(ery * 3.14159 + t) * wamp;
    node sy2 = ery + cos(rx * 2.6 + t * 0.7) * wamp * 0.8;
    node h2 = sin(sx2 * freq + sy2 * freq * 0.5 + t * 0.3) * 0.7
             + sin(sy2 * freq * 1.4 + sx2 * freq * 0.3 + t * 0.5) * 0.2;

    /* Gradient (finite difference) */
    node gx = (h1 - h0) * 200.0;
    node gy = (h2 - h0) * 200.0;

    /* Soft diffuse light from upper-left */
    node dot_val = 2.0 - gx * 0.5 - gy * 0.6;
    node len = sqrt(gx * gx + gy * gy + 4.0);
    node shade = clamp(dot_val / len, 0.0, 1.0);

    /* Blend to flat mid-tone at edges */
    node shade_c = shade * focus + 0.5 * (1.0 - focus);

    /* Fold visibility: visible early, stronger at edges */
    node fold_strength = mix(0.08, 0.18, pdist);

    /* Darkness: mode intensity × distance from center.
       Center (pdist=0) always stays pale; modes activate at edges. */
    node darkness = (yn * 0.6 + xn * yn * 0.4) * pdist;

    /* Blue shift: xn pulls red DOWN and blue UP → actual blue, not grey.
       Left side (xn=0) stays warm/neutral; right side (xn=1) goes blue. */
    node blue_shift = xn * pdist;

    /* Base color: R drops extra for blue modes, B gains extra.
       Still =(0.84,0.84,0.88) warm silver
       Mist =(0.59,0.74,1.0)  bright blue
       Ocean=(0.19,0.35,0.64) proper dark blue
       Storm=(0.35,0.37,0.41) dark warm grey
       Abyss=(0.0, 0.0, 0.25) near black blue */
    node base_r = mix(0.84, 0.03, darkness) - blue_shift * 0.25;
    node base_g = mix(0.84, 0.05, darkness) - blue_shift * 0.10;
    node base_b = mix(0.88, 0.10, darkness) + blue_shift * 0.15;

    /* Fold shading: ±fold_strength around base */
    node fold = (shade_c - 0.5) * 2.0 * fold_strength;
    node r = base_r + fold;
    node g = base_g + fold;
    node b = base_b + fold * 0.7;

    output color = vec4(clamp(r, 0.0, 1.0), clamp(g, 0.0, 1.0), clamp(b, 0.0, 1.0), 1.0);
});

const STYLESHEET: &str = r#"
    /* ====== Label proximity — wrapping divs carry the pointer query ====== */
    .label-wrap {
        pointer-space: self;
        pointer-origin: center;
        pointer-range: 0.0 1.0;
        pointer-smoothing: 0.1;
        opacity: calc(mix(0.35, 1.0, smoothstep(2.5, 0.0, env(pointer-distance))));
    }
"#;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let config = WindowConfig {
        title: "Fluid Surface".to_string(),
        width: 1000,
        height: 750,
        resizable: true,
        ..Default::default()
    };

    WindowedApp::run(config, build_ui)
}

pub fn build_ui(ctx: &mut WindowedContext) -> impl ElementBuilder + use<> {
    if ctx.rebuild_count == 0 {
        ctx.add_css(STYLESHEET);
    }

    let bg = Color::rgba(0.93, 0.93, 0.93, 1.0);
    let card_size = 300.0;
    let label_color = Color::rgba(0.25, 0.25, 0.30, 1.0);

    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(bg)
        .items_center()
        .justify_center()
        .child(
            div()
                .w(700.0)
                .h(500.0)
                .items_center()
                .justify_center()
                .child(
                    div()
                        .flex_col()
                        .items_center()
                        .gap(24.0)
                        .child(
                            // Top row
                            div()
                                .flex_row()
                                .justify_between()
                                .items_center()
                                .w(620.0)
                                .child(label("Still", label_color))
                                .child(label("Mist", label_color)),
                        )
                        .child(
                            // Middle row
                            div()
                                .flex_row()
                                .justify_between()
                                .items_center()
                                .w(620.0)
                                .child(label("Wave", label_color))
                                .child(
                                    div()
                                        .id("fluid-card")
                                        .w(card_size)
                                        .h(card_size)
                                        .rounded(card_size / 2.0)
                                        .bg(Color::rgba(0.78, 0.82, 0.88, 1.0))
                                        .child(
                                            canvas(|_, _| {})
                                                .w(card_size)
                                                .h(card_size)
                                                .rounded(card_size / 2.0)
                                                .flow(FLUID),
                                        )
                                        .shadow(Shadow::new(
                                            0.0,
                                            -8.0,
                                            24.0,
                                            Color::rgba(1.0, 1.0, 1.0, 0.7),
                                        ))
                                        .shadow(Shadow::new(
                                            0.0,
                                            8.0,
                                            24.0,
                                            Color::rgba(0.0, 0.0, 0.0, 0.12),
                                        )),
                                )
                                .child(label("Ocean", label_color)),
                        )
                        .child(
                            // Bottom row
                            div()
                                .flex_row()
                                .justify_between()
                                .items_center()
                                .w(620.0)
                                .child(label("Storm", label_color))
                                .child(label("Abyss", label_color)),
                        ),
                ),
        )
}

/// Label wrapped in a div so pointer-query CSS can apply opacity.
fn label(name: &str, color: Color) -> impl ElementBuilder + use<> {
    div()
        .class("label-wrap")
        .child(text(name).size(22.0).weight(FontWeight::Bold).color(color))
}
