//! Three consecutive `import { X } from "./y"` lines must all parse and
//! all resolve. A truncated "Could not resolve import:" warning with an
//! empty path suggested only some were being picked up.
use blinc_dsl_core::BlincDsl;
use std::path::Path;

#[test]
fn three_import_lines_all_resolve() {
    let _ = tracing_subscriber::fmt::try_init();
    let dir = std::env::temp_dir().join("blinc_multi_import_probe");
    std::fs::create_dir_all(&dir).unwrap();
    for (f, c) in [
        ("a.blinc", "component A { view { cn.Badge(\"a\") } }"),
        ("b.blinc", "component B { view { cn.Badge(\"b\") } }"),
        ("c.blinc", "component C { view { cn.Badge(\"c\") } }"),
    ] {
        std::fs::write(dir.join(f), c).unwrap();
    }
    std::fs::write(
        dir.join("main.blinc"),
        "import { A } from \"./a\"\nimport { B } from \"./b\"\nimport { C } from \"./c\"\n\nview { Div { A() B() C() } }\n",
    )
    .unwrap();

    let dsl = BlincDsl::new().expect("dsl init");
    blinc_cn_dsl::register_all(&dsl).expect("register cn.*");
    let names = dsl
        .compile_project(&dir.join("main.blinc"), &dir)
        .expect("all three imports must resolve");
    for expected in ["a$A$view", "b$B$view", "c$C$view", "render_view"] {
        assert!(
            names.iter().any(|n| n == expected),
            "missing {expected}: {names:?}"
        );
    }
}
