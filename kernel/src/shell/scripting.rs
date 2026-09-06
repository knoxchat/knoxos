#![allow(
    clippy::manual_strip,
    clippy::collapsible_if,
    clippy::if_same_then_else,
    clippy::needless_range_loop,
    clippy::manual_range_contains,
    clippy::single_match
)]
/// Shell scripting engine — control flow, functions, and advanced features
///
/// Supports:
///   - if/then/elif/else/fi
///   - for var in ...; do ...; done
///   - while/until loops
///   - case/esac pattern matching
///   - Shell functions (function name { ... } and name() { ... })
///   - local variables with scope
///   - return/break/continue
///   - trap (signal handlers)
///   - Command substitution $()
///   - Arithmetic expansion $(())
///   - Here-documents <<EOF
///   - Subshells (...)
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use super::env;
use super::types::ShellResult;

// ═══════════════════════════════════════════════════════════════
// Flow control signals (propagated via return value)
// ═══════════════════════════════════════════════════════════════

/// Special flow control result that wraps ShellResult
#[derive(Debug, Clone)]
pub enum FlowControl {
    Normal(ShellResult),
    Return(i32),
    Break(u32),    // break N levels
    Continue(u32), // continue N levels
}

impl FlowControl {
    pub fn into_result(self) -> ShellResult {
        match self {
            FlowControl::Normal(r) => r,
            FlowControl::Return(code) => ShellResult::with_code(code, ""),
            FlowControl::Break(_) => ShellResult::ok(""),
            FlowControl::Continue(_) => ShellResult::ok(""),
        }
    }
}

// ═══════════════════════════════════════════════════════════════
// Shell Function Storage
// ═══════════════════════════════════════════════════════════════

lazy_static::lazy_static! {
    /// Registered shell functions: name -> body (lines)
    pub static ref SHELL_FUNCTIONS: spin::Mutex<BTreeMap<String, ShellFunction>> =
        spin::Mutex::new(BTreeMap::new());

    /// Signal trap handlers: signal_name -> command
    pub static ref TRAP_HANDLERS: spin::Mutex<BTreeMap<String, String>> =
        spin::Mutex::new(BTreeMap::new());
}

#[derive(Debug, Clone)]
pub struct ShellFunction {
    pub name: String,
    pub body: Vec<String>, // Lines of the function body
}

/// Local variable scope stack
static LOCAL_SCOPE: spin::Mutex<Vec<BTreeMap<String, String>>> = spin::Mutex::new(Vec::new());

fn push_local_scope() {
    LOCAL_SCOPE.lock().push(BTreeMap::new());
}

fn pop_local_scope() {
    if let Some(locals) = LOCAL_SCOPE.lock().pop() {
        // Restore any overridden variables
        let mut env = env::ENV_VARS.lock();
        for key in locals.keys() {
            // Remove local variable from environment
            // (In a full impl, we'd restore the previous value)
            let _ = env.remove(key);
        }
    }
}

fn set_local_var(name: &str, value: &str) {
    let mut scopes = LOCAL_SCOPE.lock();
    if let Some(scope) = scopes.last_mut() {
        scope.insert(String::from(name), String::from(value));
    }
    // Also set it in the environment so it's visible
    env::ENV_VARS
        .lock()
        .insert(String::from(name), String::from(value));
}

// ═══════════════════════════════════════════════════════════════
// Script Execution (main entry point)
// ═══════════════════════════════════════════════════════════════

/// Execute a script (multiple lines)
pub fn execute_script(script: &str) -> ShellResult {
    let lines: Vec<&str> = script.lines().collect();
    execute_lines(&lines, 0, lines.len()).into_result()
}

/// Execute a range of lines, handling control flow structures
fn execute_lines(lines: &[&str], start: usize, end: usize) -> FlowControl {
    let mut i = start;
    let mut last_result = ShellResult::ok("");

    while i < end && i < lines.len() {
        let line = lines[i].trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            i += 1;
            continue;
        }

        // Check for control flow structures
        if line.starts_with("if ") || line == "if" {
            let (result, next_i) = execute_if(lines, i, end);
            match result {
                FlowControl::Normal(r) => last_result = r,
                other => return other,
            }
            i = next_i;
            continue;
        }

        if line.starts_with("for ") {
            let (result, next_i) = execute_for(lines, i, end);
            match result {
                FlowControl::Normal(r) => last_result = r,
                FlowControl::Break(n) if n > 1 => return FlowControl::Break(n - 1),
                FlowControl::Continue(n) if n > 1 => return FlowControl::Continue(n - 1),
                FlowControl::Break(_) | FlowControl::Continue(_) => {}
                other => return other,
            }
            i = next_i;
            continue;
        }

        if line.starts_with("while ") {
            let (result, next_i) = execute_while(lines, i, end, false);
            match result {
                FlowControl::Normal(r) => last_result = r,
                FlowControl::Break(n) if n > 1 => return FlowControl::Break(n - 1),
                FlowControl::Continue(n) if n > 1 => return FlowControl::Continue(n - 1),
                FlowControl::Break(_) | FlowControl::Continue(_) => {}
                other => return other,
            }
            i = next_i;
            continue;
        }

        if line.starts_with("until ") {
            let (result, next_i) = execute_while(lines, i, end, true);
            match result {
                FlowControl::Normal(r) => last_result = r,
                FlowControl::Break(n) if n > 1 => return FlowControl::Break(n - 1),
                FlowControl::Continue(n) if n > 1 => return FlowControl::Continue(n - 1),
                FlowControl::Break(_) | FlowControl::Continue(_) => {}
                other => return other,
            }
            i = next_i;
            continue;
        }

        if line.starts_with("case ") {
            let (result, next_i) = execute_case(lines, i, end);
            match result {
                FlowControl::Normal(r) => last_result = r,
                other => return other,
            }
            i = next_i;
            continue;
        }

        // Function definition: function name { ... } or name() { ... }
        if line.starts_with("function ") || line.contains("()") {
            let next_i = define_function(lines, i, end);
            i = next_i;
            continue;
        }

        // Flow control keywords
        if line == "return" || line.starts_with("return ") {
            let code = line
                .strip_prefix("return")
                .unwrap()
                .trim()
                .parse::<i32>()
                .unwrap_or(last_result.exit_code);
            return FlowControl::Return(code);
        }

        if line == "break" || line.starts_with("break ") {
            let n = line
                .strip_prefix("break")
                .unwrap()
                .trim()
                .parse::<u32>()
                .unwrap_or(1);
            return FlowControl::Break(n);
        }

        if line == "continue" || line.starts_with("continue ") {
            let n = line
                .strip_prefix("continue")
                .unwrap()
                .trim()
                .parse::<u32>()
                .unwrap_or(1);
            return FlowControl::Continue(n);
        }

        // `local var=value`
        if line.starts_with("local ") {
            let rest = &line[6..];
            if let Some(eq) = rest.find('=') {
                let name = &rest[..eq];
                let value = &rest[eq + 1..];
                set_local_var(name.trim(), value.trim());
            }
            i += 1;
            continue;
        }

        // `trap 'command' SIGNAL`
        if line.starts_with("trap ") {
            parse_trap(line);
            i += 1;
            continue;
        }

        if line.contains("<<") && !line.contains("<<<") {
            let (expanded_line, consumed) = expand_heredoc(lines, i);
            last_result = super::execute(&expanded_line);
            i += consumed;
            continue;
        }

        // Regular command execution
        last_result = super::execute(line);
        i += 1;
    }

    FlowControl::Normal(last_result)
}

// ═══════════════════════════════════════════════════════════════
// if / then / elif / else / fi
// ═══════════════════════════════════════════════════════════════

/// Execute an if/then/elif/else/fi block
/// Returns (result, next_line_index after fi)
fn execute_if(lines: &[&str], start: usize, end: usize) -> (FlowControl, usize) {
    // Parse the condition from the "if" line
    let cond_line = lines[start].trim();
    let condition = cond_line.strip_prefix("if ").unwrap_or("").trim();
    let condition = condition.strip_suffix("; then").unwrap_or(condition).trim();

    let mut i = start + 1;

    // Skip "then" if it's on its own line
    if i < end && lines[i].trim() == "then" {
        i += 1;
    }

    // Find matching elif/else/fi, tracking nesting depth
    let mut body_start = i;
    let mut branches: Vec<(&str, usize, usize)> = Vec::new(); // (condition, body_start, body_end)
    let mut else_body: Option<(usize, usize)> = None;
    let mut depth = 1u32;
    let mut current_cond = condition;
    let mut current_body_start = body_start;

    while i < end {
        let line = lines[i].trim();

        if line.starts_with("if ") || line == "if" {
            depth += 1;
        } else if line == "fi" {
            depth -= 1;
            if depth == 0 {
                // Close current branch
                branches.push((current_cond, current_body_start, i));
                let fi_line = i + 1;

                // Evaluate branches
                for (cond, bstart, bend) in &branches {
                    if cond.is_empty() {
                        // else branch
                        let result = execute_lines(lines, *bstart, *bend);
                        return (result, fi_line);
                    }
                    let result = super::execute(cond);
                    if result.success() {
                        let result = execute_lines(lines, *bstart, *bend);
                        return (result, fi_line);
                    }
                }
                if let Some((es, ee)) = else_body {
                    let result = execute_lines(lines, es, ee);
                    return (result, fi_line);
                }

                return (FlowControl::Normal(ShellResult::ok("")), fi_line);
            }
        } else if depth == 1 {
            if line.starts_with("elif ") {
                // Close previous branch
                branches.push((current_cond, current_body_start, i));
                current_cond = line.strip_prefix("elif ").unwrap_or("").trim();
                let current_cond_str = current_cond.strip_suffix("; then").unwrap_or(current_cond);
                // SAFETY: we need to store this — but we can just re-extract later
                // For simplicity, re-parse during execution
                i += 1;
                if i < end && lines[i].trim() == "then" {
                    i += 1;
                }
                current_body_start = i;
                current_cond = current_cond_str;
                continue;
            } else if line == "else" {
                branches.push((current_cond, current_body_start, i));
                i += 1;
                // The else body continues until fi
                else_body = Some((i, end)); // Will be truncated when fi is found
                current_cond = ""; // Mark as else
                current_body_start = i;
                continue;
            }
        }
        i += 1;
    }

    // Malformed: no matching fi
    (
        FlowControl::Normal(ShellResult::err("ksh: syntax error: missing `fi`")),
        end,
    )
}

// ═══════════════════════════════════════════════════════════════
// for var in ...; do ...; done
// ═══════════════════════════════════════════════════════════════

fn execute_for(lines: &[&str], start: usize, end: usize) -> (FlowControl, usize) {
    let line = lines[start].trim();
    // Parse: for VAR in WORD1 WORD2 ...; do
    let rest = line.strip_prefix("for ").unwrap_or("");
    let (var_name, words_str) = if let Some(in_pos) = rest.find(" in ") {
        (&rest[..in_pos], &rest[in_pos + 4..])
    } else {
        return (
            FlowControl::Normal(ShellResult::err("ksh: syntax error in for loop")),
            start + 1,
        );
    };
    let var_name = var_name.trim();
    let words_str = words_str.strip_suffix("; do").unwrap_or(words_str).trim();

    // Expand the word list
    let words: Vec<&str> = words_str.split_whitespace().collect();

    // Find "do" and "done"
    let mut i = start + 1;
    if i < end && lines[i].trim() == "do" {
        i += 1;
    }
    let body_start = i;

    // Find matching "done"
    let mut depth = 1u32;
    while i < end {
        let l = lines[i].trim();
        if l.starts_with("for ") || l.starts_with("while ") || l.starts_with("until ") {
            depth += 1;
        } else if l == "done" {
            depth -= 1;
            if depth == 0 {
                break;
            }
        }
        i += 1;
    }
    let body_end = i;
    let next_i = i + 1;

    // Execute loop body for each word
    let mut last_result = ShellResult::ok("");
    for word in words {
        env::ENV_VARS
            .lock()
            .insert(String::from(var_name), String::from(word));
        match execute_lines(lines, body_start, body_end) {
            FlowControl::Normal(r) => last_result = r,
            FlowControl::Break(1) => break,
            FlowControl::Break(n) => return (FlowControl::Break(n - 1), next_i),
            FlowControl::Continue(1) => continue,
            FlowControl::Continue(n) => return (FlowControl::Continue(n - 1), next_i),
            other => return (other, next_i),
        }
    }

    (FlowControl::Normal(last_result), next_i)
}

// ═══════════════════════════════════════════════════════════════
// while / until loops
// ═══════════════════════════════════════════════════════════════

fn execute_while(lines: &[&str], start: usize, end: usize, negate: bool) -> (FlowControl, usize) {
    let line = lines[start].trim();
    let keyword = if negate { "until " } else { "while " };
    let condition = line.strip_prefix(keyword).unwrap_or("").trim();
    let condition = condition.strip_suffix("; do").unwrap_or(condition).trim();

    let mut i = start + 1;
    if i < end && lines[i].trim() == "do" {
        i += 1;
    }
    let body_start = i;

    // Find matching "done"
    let mut depth = 1u32;
    while i < end {
        let l = lines[i].trim();
        if l.starts_with("for ") || l.starts_with("while ") || l.starts_with("until ") {
            depth += 1;
        } else if l == "done" {
            depth -= 1;
            if depth == 0 {
                break;
            }
        }
        i += 1;
    }
    let body_end = i;
    let next_i = i + 1;

    let mut last_result = ShellResult::ok("");
    let mut iterations = 0u32;
    const MAX_ITERATIONS: u32 = 10_000; // Safety limit

    loop {
        iterations += 1;
        if iterations > MAX_ITERATIONS {
            return (
                FlowControl::Normal(ShellResult::err("ksh: loop iteration limit exceeded")),
                next_i,
            );
        }

        let cond_result = super::execute(condition);
        let should_run = if negate {
            !cond_result.success()
        } else {
            cond_result.success()
        };
        if !should_run {
            break;
        }

        match execute_lines(lines, body_start, body_end) {
            FlowControl::Normal(r) => last_result = r,
            FlowControl::Break(1) => break,
            FlowControl::Break(n) => return (FlowControl::Break(n - 1), next_i),
            FlowControl::Continue(1) => continue,
            FlowControl::Continue(n) => return (FlowControl::Continue(n - 1), next_i),
            other => return (other, next_i),
        }
    }

    (FlowControl::Normal(last_result), next_i)
}

// ═══════════════════════════════════════════════════════════════
// case / esac
// ═══════════════════════════════════════════════════════════════

fn execute_case(lines: &[&str], start: usize, end: usize) -> (FlowControl, usize) {
    let line = lines[start].trim();
    // case WORD in
    let rest = line.strip_prefix("case ").unwrap_or("");
    let word = rest.strip_suffix(" in").unwrap_or(rest).trim();
    // Expand the word
    let expanded = { super::parser::expand_variables(word) };

    let mut i = start + 1;

    // Find matching esac and evaluate patterns
    while i < end {
        let l = lines[i].trim();
        if l == "esac" {
            return (FlowControl::Normal(ShellResult::ok("")), i + 1);
        }

        // Pattern line: pattern) or pattern1|pattern2)
        if l.ends_with(')') {
            let pattern_str = &l[..l.len() - 1];
            let patterns: Vec<&str> = pattern_str.split('|').collect();

            let matched = patterns.iter().any(|p| {
                let p = p.trim();
                if p == "*" {
                    true
                } else {
                    super::parser::glob_match(p, &expanded)
                }
            });

            i += 1;

            if matched {
                // Execute until ;; or esac
                let body_start = i;
                while i < end {
                    let bl = lines[i].trim();
                    if bl == ";;" || bl == "esac" {
                        break;
                    }
                    i += 1;
                }
                let result = execute_lines(lines, body_start, i);
                // Skip to esac
                while i < end && lines[i].trim() != "esac" {
                    i += 1;
                }
                return (result, if i < end { i + 1 } else { end });
            } else {
                // Skip this branch's body
                while i < end {
                    let bl = lines[i].trim();
                    if bl == ";;" || bl == "esac" {
                        break;
                    }
                    i += 1;
                }
                if i < end && lines[i].trim() == ";;" {
                    i += 1;
                }
            }
        } else {
            i += 1;
        }
    }

    (FlowControl::Normal(ShellResult::ok("")), end)
}

// ═══════════════════════════════════════════════════════════════
// Shell Functions
// ═══════════════════════════════════════════════════════════════

/// Parse and register a function definition
fn define_function(lines: &[&str], start: usize, end: usize) -> usize {
    let line = lines[start].trim();

    // Parse function name
    let name = if let Some(rest) = line.strip_prefix("function ") {
        rest.trim().strip_suffix('{').unwrap_or(rest).trim()
    } else if let Some(paren_pos) = line.find("()") {
        line[..paren_pos].trim()
    } else {
        return start + 1;
    };

    let name = String::from(name.trim_end_matches(|c: char| c == '{' || c.is_whitespace()));

    // Find function body between { and }
    let mut i = start;
    // Check if { is on the same line
    let has_open_brace = lines[i].contains('{');
    if !has_open_brace {
        i += 1;
        // Look for opening brace
        while i < end && lines[i].trim() != "{" {
            i += 1;
        }
    }
    i += 1;

    let body_start = i;
    let mut depth = 1u32;

    while i < end {
        let l = lines[i].trim();
        if l.contains('{') && !l.starts_with('#') {
            depth += 1;
        }
        if l == "}" || l.ends_with('}') {
            depth -= 1;
            if depth == 0 {
                break;
            }
        }
        i += 1;
    }

    let body_end = i;
    let body: Vec<String> = lines[body_start..body_end]
        .iter()
        .map(|l| String::from(*l))
        .collect();

    SHELL_FUNCTIONS
        .lock()
        .insert(name.clone(), ShellFunction { name, body });

    i + 1 // skip past closing }
}

/// Call a shell function
pub fn call_function(name: &str, args: &[String]) -> Option<ShellResult> {
    let func = {
        let funcs = SHELL_FUNCTIONS.lock();
        funcs.get(name)?.clone()
    };

    // Set up positional parameters
    push_local_scope();
    {
        let mut env = env::ENV_VARS.lock();
        for (idx, arg) in args.iter().enumerate() {
            env.insert(format!("{}", idx + 1), arg.clone());
        }
        env.insert(String::from("#"), format!("{}", args.len()));
        env.insert(String::from("@"), args.join(" "));
        env.insert(String::from("*"), args.join(" "));
    }

    let lines: Vec<&str> = func.body.iter().map(|s| s.as_str()).collect();
    let result = execute_lines(&lines, 0, lines.len());
    pop_local_scope();

    Some(result.into_result())
}

/// Check if a function is defined
pub fn is_function(name: &str) -> bool {
    SHELL_FUNCTIONS.lock().contains_key(name)
}

// ═══════════════════════════════════════════════════════════════
// trap
// ═══════════════════════════════════════════════════════════════

fn parse_trap(line: &str) {
    let rest = line.strip_prefix("trap ").unwrap_or("").trim();

    // trap 'command' SIGNAL
    // or trap - SIGNAL (to reset)
    let parts: Vec<&str> = rest.splitn(2, ' ').collect();
    if parts.len() < 2 {
        return;
    }

    let command = parts[0].trim_matches('\'').trim_matches('"');
    let signal = parts[1].trim();

    let mut traps = TRAP_HANDLERS.lock();
    if command == "-" {
        traps.remove(signal);
    } else {
        traps.insert(String::from(signal), String::from(command));
    }
}

/// Execute trap handler for a given signal name
pub fn execute_trap(signal_name: &str) -> Option<ShellResult> {
    let command = {
        let traps = TRAP_HANDLERS.lock();
        traps.get(signal_name)?.clone()
    };
    Some(super::execute(&command))
}

// ═══════════════════════════════════════════════════════════════
// Command Substitution $()
// ═══════════════════════════════════════════════════════════════

/// Expand command substitutions in a string
/// Replaces $(...) with the output of the enclosed command
pub fn expand_command_substitution(input: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if i + 1 < chars.len() && chars[i] == '$' && chars[i + 1] == '(' {
            if i + 2 < chars.len() && chars[i + 2] == '(' {
                // Arithmetic expansion $((...)) — skip, handled separately
                result.push(chars[i]);
                i += 1;
                continue;
            }
            // Command substitution $(...)
            let start = i + 2;
            let mut depth = 1u32;
            let mut end = start;
            while end < chars.len() {
                if chars[end] == '(' {
                    depth += 1;
                } else if chars[end] == ')' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                end += 1;
            }

            let cmd: String = chars[start..end].iter().collect();
            let output = super::execute(&cmd);
            // Trim trailing newline from command output
            let trimmed = output.output.trim_end_matches('\n');
            result.push_str(trimmed);
            i = end + 1;
        } else if i + 1 < chars.len() && chars[i] == '`' {
            // Backtick command substitution `...`
            let start = i + 1;
            let mut end = start;
            while end < chars.len() && chars[end] != '`' {
                end += 1;
            }
            let cmd: String = chars[start..end].iter().collect();
            let output = super::execute(&cmd);
            let trimmed = output.output.trim_end_matches('\n');
            result.push_str(trimmed);
            i = if end < chars.len() { end + 1 } else { end };
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    result
}

// ═══════════════════════════════════════════════════════════════
// Arithmetic Expansion $((...))
// ═══════════════════════════════════════════════════════════════

/// Expand arithmetic expressions in a string
/// Replaces $((...)) with the result of integer arithmetic
pub fn expand_arithmetic(input: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if i + 3 < chars.len() && chars[i] == '$' && chars[i + 1] == '(' && chars[i + 2] == '(' {
            // Find matching ))
            let start = i + 3;
            let mut depth = 1u32;
            let mut end = start;
            while end + 1 < chars.len() {
                if chars[end] == '(' && chars[end + 1] == '(' {
                    depth += 1;
                    end += 1;
                } else if chars[end] == ')' && chars[end + 1] == ')' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    end += 1;
                }
                end += 1;
            }

            let expr: String = chars[start..end].iter().collect();
            let value = eval_arithmetic(&expr);
            result.push_str(&format!("{}", value));
            i = end + 2; // skip ))
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    result
}

/// Evaluate an arithmetic expression (integer math)
/// Supports: +, -, *, /, %, **, ==, !=, <, >, <=, >=, &&, ||, !, ~, &, |, ^, <<, >>
/// Also supports variable references (without $)
fn eval_arithmetic(expr: &str) -> i64 {
    let expr = expr.trim();

    // Variable expansion: replace bare variable names with their values
    let expanded = expand_arith_vars(expr);

    // Parse and evaluate
    eval_arith_expr(&expanded)
}

fn expand_arith_vars(expr: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = expr.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i].is_alphabetic() || chars[i] == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let name: String = chars[start..i].iter().collect();
            // Try to resolve as variable
            let value = env::ENV_VARS
                .lock()
                .get(&name)
                .cloned()
                .unwrap_or_else(|| String::from("0"));
            result.push_str(&value);
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    result
}

/// Simple recursive-descent arithmetic evaluator
fn eval_arith_expr(expr: &str) -> i64 {
    let expr = expr.trim();
    if expr.is_empty() {
        return 0;
    }

    // Try to parse as number directly
    if let Ok(n) = expr.parse::<i64>() {
        return n;
    }

    // Handle ternary: expr ? expr : expr
    if let Some(q_pos) = find_op(expr, "?") {
        let cond = eval_arith_expr(&expr[..q_pos]);
        let rest = &expr[q_pos + 1..];
        if let Some(c_pos) = find_op(rest, ":") {
            let then_val = eval_arith_expr(&rest[..c_pos]);
            let else_val = eval_arith_expr(&rest[c_pos + 1..]);
            return if cond != 0 { then_val } else { else_val };
        }
    }

    // Handle || (logical or)
    if let Some(pos) = find_op(expr, "||") {
        let l = eval_arith_expr(&expr[..pos]);
        let r = eval_arith_expr(&expr[pos + 2..]);
        return if l != 0 || r != 0 { 1 } else { 0 };
    }

    // Handle && (logical and)
    if let Some(pos) = find_op(expr, "&&") {
        let l = eval_arith_expr(&expr[..pos]);
        let r = eval_arith_expr(&expr[pos + 2..]);
        return if l != 0 && r != 0 { 1 } else { 0 };
    }

    // Handle == and !=
    if let Some(pos) = find_op(expr, "==") {
        let l = eval_arith_expr(&expr[..pos]);
        let r = eval_arith_expr(&expr[pos + 2..]);
        return if l == r { 1 } else { 0 };
    }
    if let Some(pos) = find_op(expr, "!=") {
        let l = eval_arith_expr(&expr[..pos]);
        let r = eval_arith_expr(&expr[pos + 2..]);
        return if l != r { 1 } else { 0 };
    }

    // Handle <= >= < >
    if let Some(pos) = find_op(expr, "<=") {
        let l = eval_arith_expr(&expr[..pos]);
        let r = eval_arith_expr(&expr[pos + 2..]);
        return if l <= r { 1 } else { 0 };
    }
    if let Some(pos) = find_op(expr, ">=") {
        let l = eval_arith_expr(&expr[..pos]);
        let r = eval_arith_expr(&expr[pos + 2..]);
        return if l >= r { 1 } else { 0 };
    }
    if let Some(pos) = find_op(expr, "<") {
        if !expr[..pos].ends_with('<') && !expr[pos + 1..].starts_with('<') {
            let l = eval_arith_expr(&expr[..pos]);
            let r = eval_arith_expr(&expr[pos + 1..]);
            return if l < r { 1 } else { 0 };
        }
    }
    if let Some(pos) = find_op(expr, ">") {
        if !expr[..pos].ends_with('>') && !expr[pos + 1..].starts_with('>') {
            let l = eval_arith_expr(&expr[..pos]);
            let r = eval_arith_expr(&expr[pos + 1..]);
            return if l > r { 1 } else { 0 };
        }
    }

    // Handle + and - (addition/subtraction)
    // Find rightmost + or - at depth 0 (not inside parens)
    if let Some(pos) = find_op_right(expr, &['+', '-']) {
        if pos > 0 {
            let l = eval_arith_expr(&expr[..pos]);
            let r = eval_arith_expr(&expr[pos + 1..]);
            return if expr.as_bytes()[pos] == b'+' {
                l + r
            } else {
                l - r
            };
        }
    }

    // Handle * / %
    if let Some(pos) = find_op_right(expr, &['*', '/', '%']) {
        if pos > 0 && (expr.as_bytes()[pos] != b'*' || !expr[pos + 1..].starts_with('*')) {
            let l = eval_arith_expr(&expr[..pos]);
            let r = eval_arith_expr(&expr[pos + 1..]);
            return match expr.as_bytes()[pos] {
                b'*' => l * r,
                b'/' if r != 0 => l / r,
                b'%' if r != 0 => l % r,
                _ => 0,
            };
        }
    }

    // Handle ** (exponentiation)
    if let Some(pos) = find_op(expr, "**") {
        let l = eval_arith_expr(&expr[..pos]);
        let r = eval_arith_expr(&expr[pos + 2..]);
        return l.pow(r as u32);
    }

    // Handle unary - and !
    if expr.starts_with('-') {
        return -eval_arith_expr(&expr[1..]);
    }
    if expr.starts_with('!') {
        return if eval_arith_expr(&expr[1..]) == 0 {
            1
        } else {
            0
        };
    }
    if expr.starts_with('~') {
        return !eval_arith_expr(&expr[1..]);
    }

    // Handle parenthesized expressions
    if expr.starts_with('(') && expr.ends_with(')') {
        return eval_arith_expr(&expr[1..expr.len() - 1]);
    }

    // Fallback: try parse as number
    expr.parse::<i64>().unwrap_or(0)
}

/// Find operator at depth 0 (not inside parentheses), left-to-right
fn find_op(expr: &str, op: &str) -> Option<usize> {
    let bytes = expr.as_bytes();
    let op_bytes = op.as_bytes();
    let mut depth = 0i32;

    for i in 0..bytes.len() {
        if bytes[i] == b'(' {
            depth += 1;
        }
        if bytes[i] == b')' {
            depth -= 1;
        }
        if depth == 0 && i + op_bytes.len() <= bytes.len() {
            if &bytes[i..i + op_bytes.len()] == op_bytes {
                return Some(i);
            }
        }
    }
    None
}

/// Find rightmost single-char operator at depth 0
fn find_op_right(expr: &str, ops: &[char]) -> Option<usize> {
    let bytes = expr.as_bytes();
    let mut depth = 0i32;
    let mut result = None;

    for i in 0..bytes.len() {
        if bytes[i] == b'(' {
            depth += 1;
        }
        if bytes[i] == b')' {
            depth -= 1;
        }
        if depth == 0 {
            for &op in ops {
                if bytes[i] == op as u8 {
                    result = Some(i);
                }
            }
        }
    }
    result
}

// ═══════════════════════════════════════════════════════════════
// Here-documents (<<EOF ... EOF)
// ═══════════════════════════════════════════════════════════════

/// Expand here-document in a command line
/// Returns (modified_line_with_heredoc_replaced, lines_consumed)
pub fn expand_heredoc(lines: &[&str], line_index: usize) -> (String, usize) {
    let line = lines[line_index];

    // Find <<DELIMITER or <<-DELIMITER or <<'DELIMITER'
    if let Some(pos) = line.find("<<") {
        let after = &line[pos + 2..];
        let strip_tabs = after.starts_with('-');
        let delim_start = if strip_tabs { 1 } else { 0 };
        let delim = after[delim_start..]
            .trim()
            .trim_matches('\'')
            .trim_matches('"');

        if delim.is_empty() {
            return (String::from(line), 1);
        }

        // Collect lines until delimiter
        let mut content = String::new();
        let mut consumed = 1;
        let mut i = line_index + 1;

        while i < lines.len() {
            let l = if strip_tabs {
                lines[i].trim_start_matches('\t')
            } else {
                lines[i]
            };

            if l.trim() == delim {
                consumed = i - line_index + 1;
                break;
            }

            content.push_str(l);
            content.push('\n');
            i += 1;
            consumed = i - line_index;
        }

        // Replace the heredoc with a temporary file or inline the content
        let cmd_before = &line[..pos];
        let modified = format!("{} /dev/stdin", cmd_before.trim());
        // Store heredoc content for the command to use
        env::ENV_VARS
            .lock()
            .insert(String::from("__HEREDOC__"), content);

        (modified, consumed)
    } else {
        (String::from(line), 1)
    }
}

// ═══════════════════════════════════════════════════════════════
// Subshells (...)
// ═══════════════════════════════════════════════════════════════

/// Execute a subshell command: (...) runs in isolated environment
pub fn execute_subshell(cmd: &str) -> ShellResult {
    // Save current environment
    let saved_env: BTreeMap<String, String> = env::ENV_VARS.lock().clone();

    // Execute in "subshell" (same process but isolated env)
    let result = super::execute(cmd);

    // Restore parent environment (subshell changes don't propagate)
    *env::ENV_VARS.lock() = saved_env;

    result
}

// ═══════════════════════════════════════════════════════════════
// Script file execution
// ═══════════════════════════════════════════════════════════════

/// Execute a shell script file from VFS
pub fn execute_script_file(path: &str, args: &[String]) -> ShellResult {
    let resolved = super::helpers::resolve_path(path);
    let content = {
        let vfs = crate::vfs::VFS.lock();
        match vfs.read_file(&resolved) {
            Some(data) => match core::str::from_utf8(data) {
                Ok(s) => String::from(s),
                Err(_) => return ShellResult::err(&format!("ksh: {}: binary file", path)),
            },
            None => return ShellResult::err(&format!("ksh: {}: No such file", path)),
        }
    };

    // Check for shebang
    let script = if content.starts_with("#!") {
        // Parse shebang line
        let first_newline = content.find('\n').unwrap_or(content.len());
        let shebang = content[2..first_newline].trim();

        // If shebang points to a different interpreter, delegate
        if !shebang.contains("sh") && !shebang.contains("ksh") {
            return ShellResult::err(&format!(
                "ksh: {}: unsupported interpreter: {}",
                path, shebang
            ));
        }

        // Skip shebang line
        &content[first_newline + 1..]
    } else {
        &content
    };

    // Set up positional parameters
    push_local_scope();
    {
        let mut env_vars = env::ENV_VARS.lock();
        env_vars.insert(String::from("0"), String::from(path));
        for (idx, arg) in args.iter().enumerate() {
            env_vars.insert(format!("{}", idx + 1), arg.clone());
        }
        env_vars.insert(String::from("#"), format!("{}", args.len()));
        env_vars.insert(String::from("@"), args.join(" "));
        env_vars.insert(String::from("*"), args.join(" "));
    }

    let result = execute_script(script);
    pop_local_scope();
    result
}
