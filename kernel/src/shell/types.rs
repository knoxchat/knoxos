/// Shell shared types — Command representation and execution results
use alloc::string::String;
use alloc::vec::Vec;

/// Parsed command
#[derive(Debug, Clone)]
pub struct Command {
    pub program: String,
    pub args: Vec<String>,
    pub stdin_redirect: Option<String>,
    pub stdout_redirect: Option<RedirectType>,
    pub stderr_redirect: Option<RedirectType>,
    pub background: bool,
    /// Here-string content (<<<)
    pub herestring: Option<String>,
}

/// A pipeline segment connected by logical operators
#[derive(Debug, Clone)]
pub struct Pipeline {
    pub commands: Vec<Command>,
}

/// Logical operator connecting pipelines
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LogicalOp {
    /// `&&` — run next only if previous succeeded (exit code 0)
    And,
    /// `||` — run next only if previous failed (exit code != 0)
    Or,
    /// `;` — run next unconditionally (sequential)
    Semi,
}

/// A complete command line: a sequence of pipelines connected by logical operators
#[derive(Debug, Clone)]
pub struct CommandLine {
    /// The first pipeline
    pub first: Pipeline,
    /// Subsequent (operator, pipeline) pairs
    pub rest: Vec<(LogicalOp, Pipeline)>,
}

/// I/O redirection type
#[derive(Debug, Clone)]
pub enum RedirectType {
    /// `>` truncate and write
    Overwrite(String),
    /// `>>` append
    Append(String),
}

/// Shell execution result returned from every command
#[derive(Debug, Clone)]
pub struct ShellResult {
    pub exit_code: i32,
    pub output: String,
}

impl ShellResult {
    /// Successful execution (exit code 0)
    pub fn ok(output: &str) -> Self {
        Self {
            exit_code: 0,
            output: String::from(output),
        }
    }

    /// Failed execution (exit code 1)
    pub fn err(msg: &str) -> Self {
        Self {
            exit_code: 1,
            output: String::from(msg),
        }
    }

    /// Custom exit code with output
    pub fn with_code(code: i32, output: &str) -> Self {
        Self {
            exit_code: code,
            output: String::from(output),
        }
    }

    /// Check if the command succeeded
    pub fn success(&self) -> bool {
        self.exit_code == 0
    }
}

impl PartialEq<i32> for ShellResult {
    fn eq(&self, other: &i32) -> bool {
        self.exit_code == *other
    }
}
