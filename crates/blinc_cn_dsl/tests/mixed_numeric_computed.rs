//! An integer literal in a float expression must compile.
use blinc_dsl_core::BlincDsl;

#[test]
fn an_int_literal_in_a_float_computed_compiles() {
    let dsl = BlincDsl::new().expect("dsl init");
    blinc_cn_dsl::register_all(&dsl).expect("register cn.* widgets");
    let src = r#"
        signal pct: f64 = 4.0
        signal radius: f64 = 1.0
        view {
            cn.Progress(value = computed { pct.get() + radius.get() + 2 } : f64)
        }
    "#;
    dsl.compile_source(src, "mixed.blinc")
        .expect("compile mixed int/float computed");
    let _ = dsl.view_widget();
}
