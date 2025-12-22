//! Test runner for executing test suites
//!
//! Manages test execution, result collection, and reporting.

use crate::harness::{TestHarness, TestResult};
use anyhow::Result;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Type of test case
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TestType {
    /// Standard test using DrawContext
    Standard,
    /// Glass test using multi-pass rendering
    Glass,
    /// Text test using text rendering pipeline
    Text,
}

/// A single test case
pub struct TestCase {
    /// Test name
    pub name: String,
    /// Test category
    pub category: String,
    /// Test function
    pub test_fn: Box<dyn FnOnce(&mut crate::harness::TestContext) + Send>,
    /// Type of test
    pub test_type: TestType,
}

/// Backward compatibility alias
impl TestCase {
    pub fn uses_glass(&self) -> bool {
        self.test_type == TestType::Glass
    }
}

impl TestCase {
    pub fn new<F>(name: &str, category: &str, test_fn: F) -> Self
    where
        F: FnOnce(&mut crate::harness::TestContext) + Send + 'static,
    {
        Self {
            name: name.to_string(),
            category: category.to_string(),
            test_fn: Box::new(test_fn),
            test_type: TestType::Standard,
        }
    }

    pub fn new_glass<F>(name: &str, category: &str, test_fn: F) -> Self
    where
        F: FnOnce(&mut crate::harness::TestContext) + Send + 'static,
    {
        Self {
            name: name.to_string(),
            category: category.to_string(),
            test_fn: Box::new(test_fn),
            test_type: TestType::Glass,
        }
    }

    pub fn new_text<F>(name: &str, category: &str, test_fn: F) -> Self
    where
        F: FnOnce(&mut crate::harness::TestContext) + Send + 'static,
    {
        Self {
            name: name.to_string(),
            category: category.to_string(),
            test_fn: Box::new(test_fn),
            test_type: TestType::Text,
        }
    }
}

/// Result of running a test
pub struct TestRun {
    /// Test name
    pub name: String,
    /// Test category
    pub category: String,
    /// Test result
    pub result: TestResult,
    /// Time taken
    pub duration: Duration,
}

impl TestRun {
    pub fn is_passed(&self) -> bool {
        self.result.is_passed()
    }
}

/// Test suite containing multiple test cases
pub struct TestSuite {
    /// Suite name
    pub name: String,
    /// Test cases
    pub cases: Vec<TestCase>,
}

impl TestSuite {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            cases: Vec::new(),
        }
    }

    pub fn add<F>(&mut self, name: &str, test_fn: F) -> &mut Self
    where
        F: FnOnce(&mut crate::harness::TestContext) + Send + 'static,
    {
        self.cases.push(TestCase::new(name, &self.name, test_fn));
        self
    }

    /// Add a glass test case (uses multi-pass rendering for backdrop blur)
    pub fn add_glass<F>(&mut self, name: &str, test_fn: F) -> &mut Self
    where
        F: FnOnce(&mut crate::harness::TestContext) + Send + 'static,
    {
        self.cases
            .push(TestCase::new_glass(name, &self.name, test_fn));
        self
    }
}

/// Test runner for executing suites
pub struct TestRunner {
    /// Test harness
    harness: TestHarness,
    /// Test suites to run
    suites: Vec<TestSuite>,
    /// Filter pattern (None = run all)
    filter: Option<String>,
}

impl TestRunner {
    /// Create a new test runner
    pub fn new() -> Result<Self> {
        Ok(Self {
            harness: TestHarness::new()?,
            suites: Vec::new(),
            filter: None,
        })
    }

    /// Create with custom harness
    pub fn with_harness(harness: TestHarness) -> Self {
        Self {
            harness,
            suites: Vec::new(),
            filter: None,
        }
    }

    /// Add a test suite
    pub fn add_suite(&mut self, suite: TestSuite) -> &mut Self {
        self.suites.push(suite);
        self
    }

    /// Set a filter pattern
    pub fn filter(&mut self, pattern: &str) -> &mut Self {
        self.filter = Some(pattern.to_string());
        self
    }

    /// Run all tests
    pub fn run(&mut self) -> RunResult {
        let start = Instant::now();
        let mut results = Vec::new();

        for suite in self.suites.drain(..) {
            tracing::info!("Running suite: {}", suite.name);

            for case in suite.cases {
                // Apply filter if set
                if let Some(ref pattern) = self.filter {
                    if !case.name.contains(pattern) && !case.category.contains(pattern) {
                        continue;
                    }
                }

                let test_start = Instant::now();
                let full_name = format!("{}::{}", case.category, case.name);

                tracing::debug!("Running test: {}", full_name);

                // Use appropriate test runner based on test type
                let result = match case.test_type {
                    TestType::Glass => {
                        match self.harness.run_glass_test(&full_name, case.test_fn) {
                            Ok(result) => result,
                            Err(e) => {
                                tracing::error!("Glass test {} failed with error: {}", full_name, e);
                                TestResult::Failed {
                                    difference: 1.0,
                                    diff_path: self.harness.diff_path(&full_name),
                                }
                            }
                        }
                    }
                    TestType::Text => {
                        // Text tests use the same runner as standard but will render text
                        match self.harness.run_test(&full_name, case.test_fn) {
                            Ok(result) => result,
                            Err(e) => {
                                tracing::error!("Text test {} failed with error: {}", full_name, e);
                                TestResult::Failed {
                                    difference: 1.0,
                                    diff_path: self.harness.diff_path(&full_name),
                                }
                            }
                        }
                    }
                    TestType::Standard => {
                        match self.harness.run_test(&full_name, case.test_fn) {
                            Ok(result) => result,
                            Err(e) => {
                                tracing::error!("Test {} failed with error: {}", full_name, e);
                                TestResult::Failed {
                                    difference: 1.0,
                                    diff_path: self.harness.diff_path(&full_name),
                                }
                            }
                        }
                    }
                };

                let duration = test_start.elapsed();
                let passed = result.is_passed();

                if passed {
                    tracing::info!("  ✓ {} ({:?})", case.name, duration);
                } else {
                    tracing::error!("  ✗ {} ({:?})", case.name, duration);
                }

                results.push(TestRun {
                    name: case.name,
                    category: case.category,
                    result,
                    duration,
                });
            }
        }

        let total_duration = start.elapsed();
        RunResult::new(results, total_duration)
    }

    /// Get the harness
    pub fn harness(&self) -> &TestHarness {
        &self.harness
    }
}

/// Results from running tests
pub struct RunResult {
    /// Individual test results
    pub results: Vec<TestRun>,
    /// Total time taken
    pub duration: Duration,
}

impl RunResult {
    pub fn new(results: Vec<TestRun>, duration: Duration) -> Self {
        Self { results, duration }
    }

    /// Count of passed tests
    pub fn passed(&self) -> usize {
        self.results.iter().filter(|r| r.is_passed()).count()
    }

    /// Count of failed tests
    pub fn failed(&self) -> usize {
        self.results.iter().filter(|r| !r.is_passed()).count()
    }

    /// Total test count
    pub fn total(&self) -> usize {
        self.results.len()
    }

    /// All tests passed
    pub fn all_passed(&self) -> bool {
        self.results.iter().all(|r| r.is_passed())
    }

    /// Get results by category
    pub fn by_category(&self) -> HashMap<String, Vec<&TestRun>> {
        let mut map: HashMap<String, Vec<&TestRun>> = HashMap::new();
        for result in &self.results {
            map.entry(result.category.clone()).or_default().push(result);
        }
        map
    }

    /// Print summary
    pub fn print_summary(&self) {
        println!("\n╔══════════════════════════════════════════╗");
        println!("║           TEST RESULTS SUMMARY           ║");
        println!("╠══════════════════════════════════════════╣");
        println!(
            "║  Passed:  {:>5}                          ║",
            self.passed()
        );
        println!(
            "║  Failed:  {:>5}                          ║",
            self.failed()
        );
        println!("║  Total:   {:>5}                          ║", self.total());
        println!("║  Time:    {:>8.2?}                      ║", self.duration);
        println!("╚══════════════════════════════════════════╝");

        if self.failed() > 0 {
            println!("\nFailed tests:");
            for result in &self.results {
                if !result.is_passed() {
                    println!("  ✗ {}::{}", result.category, result.name);
                }
            }
        }
    }
}
