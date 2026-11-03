//! Every `flow!` call in the example programs generates WGSL that naga accepts.

use std::path::Path;

/// The flow source of each `flow!(name, target, { body })` call, as the macro builds it.
fn flow_calls(source: &str) -> Vec<String> {
    let mut calls = Vec::new();
    let mut rest = source;
    while let Some(start) = rest.find("flow!(") {
        let args = &rest[start + "flow!(".len()..];
        let mut depth = 1;
        let mut end = None;
        for (i, c) in args.char_indices() {
            match c {
                '(' | '{' => depth += 1,
                ')' | '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(end) = end else { break };
        let call = &args[..end];
        rest = &args[end..];
        // Prose such as "the flow! macro" has no argument list of this shape.
        let mut parts = call.splitn(3, ',');
        let (Some(name), Some(target), Some(body)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let body = body.trim();
        if !body.starts_with('{') {
            continue;
        }
        let body = &body[1..body.len() - 1];
        calls.push(format!(
            "@flow {} {{ target: {}; {} }}",
            name.trim(),
            target.trim(),
            body
        ));
    }
    calls
}

#[test]
fn the_example_flows_generate_valid_wgsl() {
    let dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/blinc_app_examples/examples");
    let mut checked = 0;
    for name in ["flow_demo.rs", "semantic_flow_demo.rs", "fluid_demo.rs"] {
        let text = std::fs::read_to_string(dir.join(name)).unwrap();
        for source in flow_calls(&text) {
            let shader = blinc_flow::flow_shader(&source)
                .unwrap_or_else(|e| panic!("{name}: {e}\n{source}"));
            let module = naga::front::wgsl::parse_str(&shader.wgsl).unwrap_or_else(|e| {
                panic!("{name}/{}: {}", shader.name, e.emit_to_string(&shader.wgsl))
            });
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::all(),
            )
            .validate(&module)
            .unwrap_or_else(|e| panic!("{name}/{}: {e:?}", shader.name));
            checked += 1;
        }
    }
    assert!(checked >= 8, "found {checked} flows");
}
