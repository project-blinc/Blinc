//! `@flow` DAG parsing.

use blinc_core::{FlowExpr, FlowFunc, FlowTarget};

use crate::parser::*;

#[test]
fn test_flow_basic_fragment() {
    let css = r#"
        @flow ripple {
            target: fragment;
            input uv;
            input time;
            node dist = distance(uv, vec2(0.5, 0.5));
            node wave = sin(dist * 20.0 - time * 4.0);
            output color = vec4(wave, wave, wave, 1.0);
        }
    "#;
    let result = Stylesheet::parse_with_errors(css);
    assert!(result.stylesheet.contains_flow("ripple"));
    let flow = result.stylesheet.get_flow("ripple").unwrap();
    assert_eq!(flow.target, FlowTarget::Fragment);
    assert_eq!(flow.inputs.len(), 2);
    assert_eq!(flow.nodes.len(), 2);
    assert_eq!(flow.outputs.len(), 1);
}

#[test]
fn test_flow_compute_target() {
    let css = r#"
        @flow sim {
            target: compute;
            workgroup: 64;
            input pos: buffer(positions, vec4);
            node new_pos = pos + 1.0;
            output buffer(positions) = new_pos;
        }
    "#;
    let result = Stylesheet::parse_with_errors(css);
    let flow = result.stylesheet.get_flow("sim").unwrap();
    assert_eq!(flow.target, FlowTarget::Compute);
    assert_eq!(flow.workgroup_size, Some(64));
    assert_eq!(flow.inputs.len(), 1);
}

#[test]
fn test_flow_expr_arithmetic() {
    let expr = parse_flow_expr("a * 2.0 + b").unwrap();
    // Should parse as (a * 2.0) + b due to precedence
    match &expr {
        FlowExpr::Add(left, right) => {
            assert!(matches!(left.as_ref(), FlowExpr::Mul(_, _)));
            assert!(matches!(right.as_ref(), FlowExpr::Ref(_)));
        }
        _ => panic!("expected Add, got {:?}", expr),
    }
}

#[test]
fn test_flow_expr_function_call() {
    let expr = parse_flow_expr("sin(x * 3.14)").unwrap();
    match &expr {
        FlowExpr::Call { func, args } => {
            assert_eq!(*func, FlowFunc::Sin);
            assert_eq!(args.len(), 1);
        }
        _ => panic!("expected Call, got {:?}", expr),
    }
}

#[test]
fn test_flow_expr_nested_functions() {
    let expr = parse_flow_expr("smoothstep(0.0, 1.0, distance(uv, vec2(0.5, 0.5)))").unwrap();
    match &expr {
        FlowExpr::Call { func, args } => {
            assert_eq!(*func, FlowFunc::Smoothstep);
            assert_eq!(args.len(), 3);
            // Third arg should be a distance() call
            assert!(matches!(
                &args[2],
                FlowExpr::Call {
                    func: FlowFunc::Distance,
                    ..
                }
            ));
        }
        _ => panic!("expected Call, got {:?}", expr),
    }
}

#[test]
fn test_flow_expr_color_literal() {
    let expr = parse_flow_expr("#ff0000").unwrap();
    match &expr {
        FlowExpr::Color(r, _g, _b, _a) => {
            assert!((r - 1.0).abs() < 0.01);
        }
        _ => panic!("expected Color, got {:?}", expr),
    }
}

#[test]
fn test_flow_expr_negative_number() {
    let expr = parse_flow_expr("-1.5").unwrap();
    match &expr {
        FlowExpr::Float(v) => assert!((*v - (-1.5)).abs() < 0.001),
        _ => panic!("expected Float, got {:?}", expr),
    }
}

#[test]
fn test_flow_expr_vec_constructors() {
    let expr = parse_flow_expr("vec3(1.0, 2.0, 3.0)").unwrap();
    assert!(matches!(expr, FlowExpr::Vec3(_, _, _)));
}

#[test]
fn test_flow_cycle_reported_as_error() {
    let css = r#"
        @flow bad {
            target: fragment;
            node a = b + 1.0;
            node b = a + 1.0;
            output color = a;
        }
    "#;
    let result = Stylesheet::parse_with_errors(css);
    // Should still parse but with errors
    assert!(!result.errors.is_empty());
    let has_cycle_error = result.errors.iter().any(|e| e.message.contains("cycle"));
    assert!(
        has_cycle_error,
        "expected cycle error, got: {:?}",
        result.errors
    );
}

#[test]
fn test_flow_with_comments() {
    let css = r#"
        @flow test {
            target: fragment;
            /* This is a comment */
            input time;
            node x = sin(time);
            output color = vec4(x, x, x, 1.0);
        }
    "#;
    let result = Stylesheet::parse_with_errors(css);
    assert!(result.stylesheet.contains_flow("test"));
}

#[test]
fn test_flow_alongside_rules() {
    let css = r#"
        #card { opacity: 0.5; }

        @flow glow {
            target: fragment;
            input uv;
            node c = length(uv);
            output color = vec4(c, c, c, 1.0);
        }

        #button { background: #ff0000; }
    "#;
    let result = Stylesheet::parse_with_errors(css);
    assert!(result.stylesheet.get("card").is_some());
    assert!(result.stylesheet.get("button").is_some());
    assert!(result.stylesheet.contains_flow("glow"));
}

#[test]
fn test_flow_multiple_flows() {
    let css = r#"
        @flow a {
            target: fragment;
            input uv;
            node x = length(uv);
            output color = vec4(x, x, x, 1.0);
        }
        @flow b {
            target: fragment;
            input time;
            node y = sin(time);
            output color = vec4(y, y, y, 1.0);
        }
    "#;
    let result = Stylesheet::parse_with_errors(css);
    assert_eq!(result.stylesheet.flow_count(), 2);
    assert!(result.stylesheet.contains_flow("a"));
    assert!(result.stylesheet.contains_flow("b"));
}

#[test]
fn test_flow_bare_output() {
    let css = r#"
        @flow test {
            target: fragment;
            input uv;
            node color = vec4(1.0, 0.0, 0.0, 1.0);
            output color;
        }
    "#;
    let result = Stylesheet::parse_with_errors(css);
    assert!(result.stylesheet.contains_flow("test"));
    let flow = result.stylesheet.get_flow("test").unwrap();
    assert_eq!(flow.outputs.len(), 1);
    assert!(flow.outputs[0].expr.is_none());
}

#[test]
fn test_flow_expr_parenthesized() {
    let expr = parse_flow_expr("(a + b) * c").unwrap();
    match &expr {
        FlowExpr::Mul(left, _right) => {
            assert!(matches!(left.as_ref(), FlowExpr::Add(_, _)));
        }
        _ => panic!("expected Mul, got {:?}", expr),
    }
}
