//! `computed { }` over an FSM context field inside a namespaced module.
//!
//! Under `compile_project` every file carries a module namespace, so the
//! FSM declaration is mangled (`Play` -> `shell$Play`) and its
//! synthesised `__fsm_ctx_<Fsm>_<field>` signal with it, while the
//! source-level reference stays `Play.pct`.
//!
//! `resolve_dotted_fsm_field_access` used to build only the unmangled
//! candidate, so the Field access was left untouched. SSA then read a
//! bare `Play` as an undefined variable and the entire view fn was
//! silently dropped -- the caller died on an undefined
//! `<module>$<Component>$view` with no error naming the real cause.
//!
//! Asserting on the emitted function names rather than just "compiles":
//! the failure mode was a *missing* function, which a compile check on
//! the module alone would not have caught.

use blinc_dsl_core::BlincDsl;
use std::path::Path;

fn write(dir: &Path, name: &str, body: &str) {
    std::fs::write(dir.join(name), body).unwrap();
}

#[test]
fn computed_over_fsm_context_lowers_under_a_module_namespace() {
    let _ = tracing_subscriber::fmt::try_init();
    let dir = std::env::temp_dir().join("blinc_ns_fsm_ctx_probe");
    std::fs::create_dir_all(&dir).unwrap();

    write(
        &dir,
        "shell.blinc",
        r#"
        fsm Play {
            context { pct: f64 = 0.0 }
            state Idle
            initial Idle
            on Idle.Grow -> Idle { ctx.pct += 10.0 }
        }
        component Shell {
            view {
                Div {
                    cn.Progress(value = Play.pct)
                    cn.Progress(value = computed { Play.pct / 2.0 } : f64)
                }
            }
        }
        "#,
    );
    write(
        &dir,
        "main.blinc",
        "import { Shell } from \"./shell\"\n\nview { Shell() }\n",
    );

    let dsl = BlincDsl::new().expect("dsl init");
    blinc_cn_dsl::register_all(&dsl).expect("register cn.* widgets");
    let names = dsl
        .compile_project(&dir.join("main.blinc"), &dir)
        .expect("namespaced module with a computed over FSM context must compile");

    assert!(
        names.iter().any(|n| n == "shell$Shell$view"),
        "the namespaced view fn must be emitted, not silently dropped: {names:?}"
    );
    assert!(
        names.iter().any(|n| n == "render_view"),
        "entry view must resolve the imported component: {names:?}"
    );
}
