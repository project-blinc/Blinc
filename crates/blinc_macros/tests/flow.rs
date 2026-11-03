//! `flow!` emits the WGSL the run-time path generates, as a constant, and the WGSL is valid.

use blinc_core::{FlowShader, FlowTarget};
use blinc_macros::flow;

const RIPPLE: FlowShader = flow!(ripple, fragment, {
    input uv: builtin(uv);
    input time: builtin(time);
    node d = distance(uv, vec2(0.5, 0.5));
    node wave = sin(d * 20.0 - time * 4.0);
    output color = vec4(wave, wave, wave, 1.0);
});

const TERRAIN: FlowShader = flow!(terrain, fragment, {
    input uv: builtin(uv);
    input time: builtin(time);
    step noise: pattern_noise { scale: 4.0; detail: 6; animation: time * 0.3; }
    node contrast = smoothstep(0.2, 0.8, noise);
    step palette: color_ramp {
        source: contrast;
        stops: 0x1a3a5a 0.0, 0x4488aa 0.35, 0x88cc66 0.55, 0xccdd44 0.75, 0xffffff 1.0;
    }
    output color = palette;
});

fn validates(shader: &FlowShader) {
    let module = naga::front::wgsl::parse_str(&shader.wgsl).unwrap_or_else(|e| {
        panic!(
            "{} does not parse: {}",
            shader.name,
            e.emit_to_string(&shader.wgsl)
        )
    });
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap_or_else(|e| panic!("{} does not validate: {e:?}", shader.name));
}

#[test]
fn the_macro_describes_the_shader() {
    let ripple = RIPPLE.clone();
    assert_eq!(ripple.name, "ripple");
    assert_eq!(ripple.target, FlowTarget::Fragment);
    assert!(!ripple.needs_scene);
    assert!(ripple.buffers.is_empty());
    assert!(ripple.wgsl.contains("fn fs_main"));
}

#[test]
fn the_macro_matches_run_time_codegen() {
    let runtime = blinc_flow::flow_shader(
        "@flow ripple { target: fragment; input uv: builtin(uv); input time: builtin(time); \
         node d = distance(uv, vec2(0.5, 0.5)); node wave = sin(d * 20.0 - time * 4.0); \
         output color = vec4(wave, wave, wave, 1.0); }",
    )
    .unwrap();
    assert_eq!(RIPPLE, runtime);
}

#[test]
fn tokens_carry_steps_and_hex_stops() {
    assert_eq!(TERRAIN.name, "terrain");
    let runtime = blinc_flow::flow_shader(
        "@flow terrain { target: fragment; input uv: builtin(uv); input time: builtin(time); \
         step noise: pattern-noise { scale: 4.0; detail: 6; animation: time * 0.3; } \
         node contrast = smoothstep(0.2, 0.8, noise); \
         step palette: color-ramp { source: contrast; \
         stops: #1a3a5a 0.0, #4488aa 0.35, #88cc66 0.55, #ccdd44 0.75, #ffffff 1.0; } \
         output color = palette; }",
    )
    .unwrap();
    assert_eq!(TERRAIN.wgsl, runtime.wgsl);
}

#[test]
fn the_generated_wgsl_is_valid() {
    validates(&RIPPLE);
    validates(&TERRAIN);
    let scene = flow!(glassy, fragment, {
        input uv: builtin(uv);
        node behind = sample_scene(uv);
        output color = vec4(behind.x, behind.y, behind.z, 0.9);
    });
    assert!(scene.needs_scene);
    validates(&scene);
}
