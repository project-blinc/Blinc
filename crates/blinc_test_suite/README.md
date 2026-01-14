# blinc_test_suite

> **Part of the [Blinc UI Framework](https://project-blinc.github.io/Blinc)**
>
> This crate is a component of Blinc, a GPU-accelerated UI framework for Rust.
> For full documentation and guides, visit the [Blinc documentation](https://project-blinc.github.io/Blinc).

Visual test suite for Blinc UI rendering capabilities.

## Overview

`blinc_test_suite` provides a framework for visual regression testing of Blinc UI components. It enables automated screenshot comparison to catch unintended visual changes.

## Features

- **Headless Rendering**: Render UI without a window
- **Screenshot Capture**: Save rendered frames as images
- **Frame Comparison**: Pixel-by-pixel comparison with tolerance
- **Test Runner**: Execute tests with filtering and reporting
- **Interactive Mode**: Manual inspection of test results

## Quick Start

```rust
use blinc_test_suite::{TestRunner, TestConfig, TestContext};

#[test]
fn test_button_rendering() {
    let runner = TestRunner::new(TestConfig {
        baseline_dir: "tests/baselines",
        output_dir: "tests/output",
        ..Default::default()
    });

    runner.run("button_default", |ctx| {
        button("Click me")
    });
}
```

## Test Configuration

```rust
let config = TestConfig {
    // Directory for baseline screenshots
    baseline_dir: "tests/baselines",

    // Directory for test output
    output_dir: "tests/output",

    // Viewport size
    width: 800,
    height: 600,

    // Pixel difference tolerance (0-255)
    tolerance: 2,

    // Percentage of pixels allowed to differ
    threshold: 0.001,

    // Update baselines instead of comparing
    update_baselines: false,
};
```

## Test Context

```rust
fn test_case(ctx: &mut TestContext) -> impl ElementBuilder {
    // Access test utilities
    let width = ctx.width();
    let height = ctx.height();

    // Build test UI
    div()
        .w(width as f32)
        .h(height as f32)
        .child(/* test content */)
}
```

## Running Tests

```bash
# Run all visual tests
cargo test --package blinc_test_suite

# Run specific test
cargo test --package blinc_test_suite button_

# Update baselines
UPDATE_BASELINES=1 cargo test --package blinc_test_suite

# Interactive mode (requires feature)
cargo test --package blinc_test_suite --features interactive
```

## Frame Comparison

```rust
use blinc_test_suite::compare_frames;

let result = compare_frames(&baseline, &current, CompareConfig {
    tolerance: 2,
    threshold: 0.001,
});

match result {
    CompareResult::Match => println!("Frames match"),
    CompareResult::Mismatch { diff_pixels, diff_image } => {
        println!("Frames differ by {} pixels", diff_pixels);
        diff_image.save("diff.png")?;
    }
}
```

## Test Organization

```
tests/
├── baselines/          # Reference screenshots
│   ├── button_default.png
│   ├── button_hover.png
│   └── ...
├── output/             # Test run output
│   ├── button_default.png
│   ├── button_default_diff.png
│   └── ...
└── visual/
    ├── button_tests.rs
    ├── card_tests.rs
    └── ...
```

## Best Practices

1. **Isolate Tests**: Each test should render a single component state
2. **Consistent Size**: Use fixed viewport sizes for reproducibility
3. **Avoid Animation**: Disable animations in tests
4. **Meaningful Names**: Name tests after what they verify
5. **Update Carefully**: Review baseline updates before committing

## License

MIT OR Apache-2.0
