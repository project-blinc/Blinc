//! Visual Test Runner
//!
//! Command-line tool for running visual regression tests for Blinc.
//!
//! Usage:
//!   blinc-visual-tests              # Run all tests
//!   blinc-visual-tests --filter foo # Run tests matching "foo"
//!   blinc-visual-tests --list       # List all tests

use anyhow::Result;
use blinc_test_suite::{runner::TestRunner, tests};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args: Vec<String> = std::env::args().collect();

    // Check for --list flag
    if args.iter().any(|a| a == "--list") {
        println!("Available test suites:");
        for suite in tests::all_suites() {
            println!("\n  {}:", suite.name);
            for case in &suite.cases {
                println!("    - {}", case.name);
            }
        }
        return Ok(());
    }

    // Check for --filter flag
    let filter = args
        .iter()
        .position(|a| a == "--filter")
        .and_then(|i| args.get(i + 1))
        .cloned();

    println!("╔══════════════════════════════════════════╗");
    println!("║      BLINC VISUAL REGRESSION TESTS       ║");
    println!("╚══════════════════════════════════════════╝\n");

    // Create test runner
    let mut runner = TestRunner::new()?;

    // Add all test suites
    for suite in tests::all_suites() {
        runner.add_suite(suite);
    }

    // Apply filter if provided
    if let Some(ref pattern) = filter {
        println!("Running tests matching: {}\n", pattern);
        runner.filter(pattern);
    }

    // Run tests
    let result = runner.run();

    // Print summary
    result.print_summary();

    // Exit with error code if any tests failed
    if result.all_passed() {
        println!("\nAll tests passed!");
        Ok(())
    } else {
        std::process::exit(1);
    }
}
