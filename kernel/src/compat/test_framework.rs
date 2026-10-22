use crate::serial_println;
/// Kernel Test Framework
///
/// Unit test runner, integration test harness, hardware abstraction layer tests,
/// benchmark framework, and test reporting for CI.
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// Test result
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TestResult {
    Pass,
    Fail,
    Skip,
    Timeout,
}

/// Individual test case
#[derive(Debug, Clone)]
pub struct TestCase {
    pub name: String,
    pub module: String,
    pub result: TestResult,
    pub duration_us: u64,
    pub message: Option<String>,
}

/// Test suite
#[derive(Debug)]
pub struct TestSuite {
    pub name: String,
    pub tests: Vec<TestCase>,
}

/// Benchmark result
#[derive(Debug, Clone)]
pub struct BenchResult {
    pub name: String,
    pub iterations: u64,
    pub avg_ns: u64,
    pub min_ns: u64,
    pub max_ns: u64,
}

/// Test framework state
pub struct TestFramework {
    pub suites: Vec<TestSuite>,
    pub benchmarks: Vec<BenchResult>,
    pub passed: u32,
    pub failed: u32,
    pub skipped: u32,
}

lazy_static::lazy_static! {
    static ref FRAMEWORK: Mutex<TestFramework> = Mutex::new(TestFramework {
        suites: Vec::new(),
        benchmarks: Vec::new(),
        passed: 0,
        failed: 0,
        skipped: 0,
    });
}

impl TestFramework {
    /// Register and run a test suite
    pub fn run_suite(&mut self, name: &str, tests: Vec<TestCase>) {
        serial_println!("[TEST] Running suite: {} ({} tests)", name, tests.len());
        for test in &tests {
            match test.result {
                TestResult::Pass => {
                    self.passed += 1;
                    serial_println!("[TEST]   PASS: {}", test.name);
                }
                TestResult::Fail => {
                    self.failed += 1;
                    if let Some(msg) = &test.message {
                        serial_println!("[TEST]   FAIL: {} - {}", test.name, msg);
                    } else {
                        serial_println!("[TEST]   FAIL: {}", test.name);
                    }
                }
                TestResult::Skip => {
                    self.skipped += 1;
                    serial_println!("[TEST]   SKIP: {}", test.name);
                }
                TestResult::Timeout => {
                    self.failed += 1;
                    serial_println!("[TEST]   TIMEOUT: {}", test.name);
                }
            }
        }
        self.suites.push(TestSuite {
            name: String::from(name),
            tests,
        });
    }

    /// Add a benchmark result
    pub fn add_benchmark(&mut self, bench: BenchResult) {
        serial_println!(
            "[BENCH] {}: avg={}ns min={}ns max={}ns ({} iters)",
            bench.name,
            bench.avg_ns,
            bench.min_ns,
            bench.max_ns,
            bench.iterations
        );
        self.benchmarks.push(bench);
    }

    /// Print summary
    pub fn summary(&self) {
        let total = self.passed + self.failed + self.skipped;
        serial_println!("[TEST] ========== RESULTS ==========");
        serial_println!(
            "[TEST] Total: {} | Pass: {} | Fail: {} | Skip: {}",
            total,
            self.passed,
            self.failed,
            self.skipped
        );
        if self.failed == 0 {
            serial_println!("[TEST] ALL TESTS PASSED");
        } else {
            serial_println!("[TEST] {} TESTS FAILED", self.failed);
        }
    }

    /// Generate JUnit XML-compatible output
    pub fn junit_xml(&self) -> String {
        let mut xml = String::from("<?xml version=\"1.0\"?>\n<testsuites>\n");
        for suite in &self.suites {
            xml.push_str(&alloc::format!(
                "  <testsuite name=\"{}\" tests=\"{}\">\n",
                suite.name,
                suite.tests.len()
            ));
            for tc in &suite.tests {
                match tc.result {
                    TestResult::Pass => {
                        xml.push_str(&alloc::format!(
                            "    <testcase name=\"{}\" time=\"{}\"/>\n",
                            tc.name,
                            tc.duration_us as f64 / 1_000_000.0
                        ));
                    }
                    TestResult::Fail => {
                        xml.push_str(&alloc::format!(
                            "    <testcase name=\"{}\"><failure/></testcase>\n",
                            tc.name
                        ));
                    }
                    _ => {}
                }
            }
            xml.push_str("  </testsuite>\n");
        }
        xml.push_str("</testsuites>\n");
        xml
    }
}

pub fn init() {
    serial_println!("[TEST] Kernel test framework initialized");
}
