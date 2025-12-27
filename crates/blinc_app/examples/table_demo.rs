//! Table Builder Demo
//!
//! This example demonstrates the TableBuilder API for declarative table creation.
//!
//! Run with: cargo run -p blinc_app --example table_demo --features windowed

use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};
use blinc_core::Color;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let config = WindowConfig {
        title: "Table Builder Demo".to_string(),
        width: 900,
        height: 700,
        resizable: true,
        ..Default::default()
    };

    WindowedApp::run(config, |ctx| build_ui(ctx))
}

fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(Color::rgba(0.08, 0.08, 0.1, 1.0))
        .flex_col()
        .gap(8.0)
        .pt(18.0)
        .pb(10.0)
        .px(10.0)
        .child(
            div()
                .flex_col()
                .gap(4.0)
                .child(h1("TableBuilder Demo").color(Color::WHITE))
                .child(muted("Declarative table creation from data")),
        )
        .child(
            scroll()
                .w_full()
                .h_full()
                .direction(ScrollDirection::Vertical)
                .child(
                    div()
                        .w_full()
                        .flex_col()
                        .gap(18.0)
                        .child(simple_table_section())
                        .child(striped_table_section())
                        .child(manual_table_section()),
                ),
        )
}

/// Simple TableBuilder example
fn simple_table_section() -> Div {
    let users_table = TableBuilder::new()
        .headers(&["ID", "Name", "Email", "Role"])
        .row(&["1", "Alice Johnson", "alice@example.com", "Admin"])
        .row(&["2", "Bob Smith", "bob@example.com", "User"])
        .row(&["3", "Carol White", "carol@example.com", "Editor"])
        .build();

    div()
        .flex_col()
        .gap(4.0)
        .child(h3("Simple Table").color(Color::WHITE))
        .child(
            code(
                r#"TableBuilder::new()
    .headers(&["ID", "Name", "Email", "Role"])
    .row(&["1", "Alice Johnson", "alice@example.com", "Admin"])
    .row(&["2", "Bob Smith", "bob@example.com", "User"])
    .row(&["3", "Carol White", "carol@example.com", "Editor"])
    .build()"#,
            )
            .syntax(SyntaxConfig::new(RustHighlighter::new()))
            .font_size(12.0)
            .w_full(),
        )
        .child(
            users_table
                .w_full()
                .bg(Color::rgba(0.12, 0.12, 0.15, 1.0))
                .rounded(8.0)
                .overflow_clip(),
        )
}

/// Striped TableBuilder example
fn striped_table_section() -> Div {
    let products_table = TableBuilder::new()
        .headers(&["Product", "Price", "Stock", "Category"])
        .row(&["Laptop Pro 16\"", "$1,299.00", "45", "Electronics"])
        .row(&["Wireless Mouse", "$49.99", "230", "Accessories"])
        .row(&["USB-C Hub 7-in-1", "$79.00", "120", "Accessories"])
        .row(&["Monitor 27\" 4K", "$399.00", "65", "Electronics"])
        .row(&["Mechanical Keyboard", "$129.00", "89", "Accessories"])
        .row(&["Webcam HD", "$89.00", "156", "Electronics"])
        .striped(true)
        .build();

    div()
        .flex_col()
        .gap(4.0)
        .child(h3("Striped Table").color(Color::WHITE))
        .child(
            code(
                r#"TableBuilder::new()
    .headers(&["Product", "Price", "Stock", "Category"])
    .row(&["Laptop Pro 16\"", "$1,299.00", "45", "Electronics"])
    .row(&["Wireless Mouse", "$49.99", "230", "Accessories"])
    // ... more rows
    .striped(true)  // Enable zebra striping
    .build()"#,
            )
            .syntax(SyntaxConfig::new(RustHighlighter::new()))
            .font_size(12.0)
            .w_full(),
        )
        .child(
            products_table
                .w_full()
                .bg(Color::rgba(0.12, 0.12, 0.15, 1.0))
                .rounded(8.0)
                .overflow_clip(),
        )
}

/// Manual table construction example
fn manual_table_section() -> Div {
    div()
        .flex_col()
        .gap(4.0)
        .child(h3("Manual Table Construction").color(Color::WHITE))
        .child(muted(
            "For more control, use table(), thead(), tbody(), tr(), th(), td() directly",
        ))
        .child(
            code(
                r#"table()
    .child(thead().child(
        tr().child(th("Status"))
            .child(th("Count"))
    ))
    .child(tbody()
        .child(tr()
            .child(td("Active"))
            .child(td("42").justify_end()))
    )"#,
            )
            .syntax(SyntaxConfig::new(RustHighlighter::new()))
            .font_size(12.0)
            .w_full(),
        )
        .child(
            table()
                .w_full()
                .bg(Color::rgba(0.12, 0.12, 0.15, 1.0))
                .rounded(8.0)
                .overflow_clip()
                .child(
                    thead().child(
                        tr().child(th("Status"))
                            .child(th("Description"))
                            .child(th("Count").justify_end()),
                    ),
                )
                .child(
                    tbody()
                        .child(
                            striped_tr(0)
                                .child(td("Active").bg(Color::rgba(0.2, 0.5, 0.2, 0.3)))
                                .child(td("Currently running tasks"))
                                .child(td("42").justify_end()),
                        )
                        .child(
                            striped_tr(1)
                                .child(td("Pending").bg(Color::rgba(0.5, 0.5, 0.2, 0.3)))
                                .child(td("Waiting to be processed"))
                                .child(td("18").justify_end()),
                        )
                        .child(
                            striped_tr(2)
                                .child(td("Completed").bg(Color::rgba(0.2, 0.2, 0.5, 0.3)))
                                .child(td("Successfully finished"))
                                .child(td("156").justify_end()),
                        )
                        .child(
                            striped_tr(3)
                                .child(td("Failed").bg(Color::rgba(0.5, 0.2, 0.2, 0.3)))
                                .child(td("Encountered errors"))
                                .child(td("3").justify_end()),
                        ),
                ),
        )
}
