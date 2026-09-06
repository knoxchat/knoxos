/// Shell command-line parser — tokenizer, variable expansion, pipe/redirect handling
/// Supports: pipes (|), redirects (>, >>, <, 2>), logical operators (&&, ||),
/// glob expansion (*, ?, [...]), brace expansion ({a,b}, {1..5}),
/// and full POSIX-style quoting.
use alloc::string::String;
use alloc::vec::Vec;

use super::env::{ENV_VARS, LAST_BG_PID, get_last_exit_code, get_shell_pid};
use super::types::{Command, CommandLine, LogicalOp, Pipeline, RedirectType};

/// Parse a full command line into a CommandLine structure with logical operators
pub fn parse_command_line(line: &str) -> CommandLine {
    let line = line.trim();

    // Split by logical operators (&&, ||, ;) while respecting quotes
    let segments = split_logical(line);

    if segments.is_empty() {
        return CommandLine {
            first: Pipeline {
                commands: Vec::new(),
            },
            rest: Vec::new(),
        };
    }

    let first = parse_pipeline(segments[0].1);
    let mut rest = Vec::new();

    for &(op, segment) in &segments[1..] {
        rest.push((op, parse_pipeline(segment)));
    }

    CommandLine { first, rest }
}

/// Split a command line by logical operators (&&, ||, ;) respecting quotes
fn split_logical(line: &str) -> Vec<(LogicalOp, &str)> {
    let mut segments = Vec::new();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut escape_next = false;
    let mut last_start = 0;
    let mut last_op = LogicalOp::Semi; // First segment has no preceding operator
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if escape_next {
            escape_next = false;
            i += 1;
            continue;
        }

        match chars[i] {
            '\\' if !in_single_quote => {
                escape_next = true;
            }
            '\'' if !in_double_quote => {
                in_single_quote = !in_single_quote;
            }
            '"' if !in_single_quote => {
                in_double_quote = !in_double_quote;
            }
            '&' if !in_single_quote
                && !in_double_quote
                && i + 1 < chars.len()
                && chars[i + 1] == '&' =>
            {
                let segment = &line[last_start..byte_index(line, i)];
                let segment = segment.trim();
                if !segment.is_empty() {
                    segments.push((last_op, segment));
                }
                last_op = LogicalOp::And;
                i += 2;
                last_start = byte_index(line, i);
                continue;
            }
            '|' if !in_single_quote
                && !in_double_quote
                && i + 1 < chars.len()
                && chars[i + 1] == '|' =>
            {
                let segment = &line[last_start..byte_index(line, i)];
                let segment = segment.trim();
                if !segment.is_empty() {
                    segments.push((last_op, segment));
                }
                last_op = LogicalOp::Or;
                i += 2;
                last_start = byte_index(line, i);
                continue;
            }
            ';' if !in_single_quote && !in_double_quote => {
                let segment = &line[last_start..byte_index(line, i)];
                let segment = segment.trim();
                if !segment.is_empty() {
                    segments.push((last_op, segment));
                }
                last_op = LogicalOp::Semi;
                i += 1;
                last_start = byte_index(line, i);
                continue;
            }
            _ => {}
        }
        i += 1;
    }

    // Last segment
    if last_start < line.len() {
        let segment = &line[last_start..];
        let segment = segment.trim();
        if !segment.is_empty() {
            segments.push((last_op, segment));
        }
    }

    segments
}

/// Helper to get byte index from char index in a string
fn byte_index(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(i, _)| i)
        .unwrap_or(s.len())
}

/// Parse a pipeline segment (commands separated by |)
pub fn parse_pipeline(line: &str) -> Pipeline {
    let line = line.trim();
    if line.is_empty() {
        return Pipeline {
            commands: Vec::new(),
        };
    }

    // Split by single pipe (but not ||) respecting quotes
    let pipe_segments = split_pipes(line);
    let mut commands = Vec::new();

    for (i, segment) in pipe_segments.iter().enumerate() {
        let segment = segment.trim();
        if segment.is_empty() {
            continue;
        }

        let mut cmd = parse_single_command(segment);

        // Only the last command can be backgrounded
        if i < pipe_segments.len() - 1 {
            cmd.background = false;
        }

        commands.push(cmd);
    }

    Pipeline { commands }
}

/// Split by single pipe | but not || , respecting quotes
fn split_pipes(line: &str) -> Vec<&str> {
    let mut segments = Vec::new();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut escape_next = false;
    let mut last_start = 0;
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if escape_next {
            escape_next = false;
            i += 1;
            continue;
        }

        match chars[i] {
            '\\' if !in_single_quote => {
                escape_next = true;
            }
            '\'' if !in_double_quote => {
                in_single_quote = !in_single_quote;
            }
            '"' if !in_single_quote => {
                in_double_quote = !in_double_quote;
            }
            '|' if !in_single_quote && !in_double_quote => {
                // Check it's not ||
                if i + 1 < chars.len() && chars[i + 1] == '|' {
                    // This is || — skip (handled by logical parser above)
                    i += 2;
                    continue;
                }
                let byte_i = byte_index(line, i);
                segments.push(&line[last_start..byte_i]);
                i += 1;
                last_start = byte_index(line, i);
                continue;
            }
            _ => {}
        }
        i += 1;
    }

    if last_start < line.len() {
        segments.push(&line[last_start..]);
    }

    segments
}

/// Backward-compatible: parse a command line into a simple Vec<Command> (for pipe-only usage)
pub fn parse_command_line_simple(line: &str) -> Vec<Command> {
    let pipeline = parse_pipeline(line);
    pipeline.commands
}

/// Parse a single command segment (no pipes)
fn parse_single_command(input: &str) -> Command {
    let mut program = String::new();
    let mut args = Vec::new();
    let mut stdin_redirect = None;
    let mut stdout_redirect = None;
    let mut stderr_redirect = None;
    let mut background = false;
    let mut herestring = None;

    // Expand environment variables
    // First: process substitution <(...) and >(...)
    let proc_expanded = expand_process_substitution(input);
    // Then: command substitution $() and backticks ``
    let cmd_expanded = super::scripting::expand_command_substitution(&proc_expanded);
    // Then: arithmetic expansion $(())
    let arith_expanded = super::scripting::expand_arithmetic(&cmd_expanded);
    let expanded = expand_variables(&arith_expanded);
    // Expand braces first (before tokenizing)
    let brace_expanded = expand_braces(&expanded);
    let tokens = tokenize(&brace_expanded);
    // Expand globs after tokenizing
    let tokens = expand_globs_in_tokens(tokens);

    let mut i = 0;
    while i < tokens.len() {
        match tokens[i].as_str() {
            "<<<"
                // Here-string: <<< "word"
                if i + 1 < tokens.len() => {
                    herestring = Some(tokens[i + 1].clone());
                    i += 2;
                    continue;
                }
            "<"
                if i + 1 < tokens.len() => {
                    stdin_redirect = Some(tokens[i + 1].clone());
                    i += 2;
                    continue;
                }
            ">"
                if i + 1 < tokens.len() => {
                    stdout_redirect = Some(RedirectType::Overwrite(tokens[i + 1].clone()));
                    i += 2;
                    continue;
                }
            ">>"
                if i + 1 < tokens.len() => {
                    stdout_redirect = Some(RedirectType::Append(tokens[i + 1].clone()));
                    i += 2;
                    continue;
                }
            "2>"
                if i + 1 < tokens.len() => {
                    stderr_redirect = Some(RedirectType::Overwrite(tokens[i + 1].clone()));
                    i += 2;
                    continue;
                }
            "2>>"
                if i + 1 < tokens.len() => {
                    stderr_redirect = Some(RedirectType::Append(tokens[i + 1].clone()));
                    i += 2;
                    continue;
                }
            "&" => {
                background = true;
                i += 1;
                continue;
            }
            _ => {}
        }

        if program.is_empty() {
            program = tokens[i].clone();
        } else {
            args.push(tokens[i].clone());
        }
        i += 1;
    }

    // Check if last char of last token is &
    if !args.is_empty() {
        if let Some(last) = args.last_mut() {
            if last.ends_with('&') {
                background = true;
                let new_len = last.len() - 1;
                last.truncate(new_len);
                if last.is_empty() {
                    args.pop();
                }
            }
        }
    }

    Command {
        program,
        args,
        stdin_redirect,
        stdout_redirect,
        stderr_redirect,
        background,
        herestring,
    }
}

/// Tokenize a command string, respecting single/double quotes and backslash escapes
pub fn tokenize(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut escape_next = false;

    for ch in input.chars() {
        if escape_next {
            current.push(ch);
            escape_next = false;
            continue;
        }

        match ch {
            '\\' if !in_single_quote => {
                escape_next = true;
            }
            '\'' if !in_double_quote => {
                in_single_quote = !in_single_quote;
            }
            '"' if !in_single_quote => {
                in_double_quote = !in_double_quote;
            }
            ' ' | '\t' if !in_single_quote && !in_double_quote => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
            }
            _ => {
                current.push(ch);
            }
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

/// Expand environment variables (`$VAR`, `${VAR}`, `$?`, `$$`, `$!`, `~`)
/// Also supports advanced parameter expansion:
///   ${VAR:-default}  — default value if unset/empty
///   ${VAR:+alternate} — alternate value if set
///   ${VAR:=assign}   — assign default if unset
///   ${VAR:?error}    — error if unset
///   ${#VAR}          — string length
///   ${VAR%pattern}   — remove shortest suffix match
///   ${VAR%%pattern}  — remove longest suffix match
///   ${VAR#pattern}   — remove shortest prefix match
///   ${VAR##pattern}  — remove longest prefix match
///   ${VAR/pat/rep}   — first pattern substitution
///   ${VAR//pat/rep}  — global pattern substitution
pub fn expand_variables(input: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        if chars[i] == '$' && i + 1 < len {
            if chars[i + 1] == '{' {
                // ${...} — advanced parameter expansion
                let start = i + 2;
                let mut depth = 1u32;
                let mut end = start;
                while end < len {
                    if chars[end] == '{' {
                        depth += 1;
                    }
                    if chars[end] == '}' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    end += 1;
                }
                let inner: String = chars[start..end].iter().collect();
                let expanded = expand_param_expression(&inner);
                result.push_str(&expanded);
                i = end + 1;
            } else if chars[i + 1] == '?' {
                let code = get_last_exit_code();
                result.push_str(&alloc::format!("{}", code));
                i += 2;
            } else if chars[i + 1] == '$' {
                let pid = get_shell_pid();
                result.push_str(&alloc::format!("{}", pid));
                i += 2;
            } else if chars[i + 1] == '!' {
                let pid = LAST_BG_PID.load(core::sync::atomic::Ordering::Relaxed);
                if pid > 0 {
                    result.push_str(&alloc::format!("{}", pid));
                }
                i += 2;
            } else if chars[i + 1] == '#' && i + 2 < len && chars[i + 2] != ' ' {
                // $# — number of positional parameters (if not followed by space/end)
                // But if it's just $# alone, get positional param count
                i += 1;
                let mut name = String::new();
                i += 1; // skip #
                while i < len && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    name.push(chars[i]);
                    i += 1;
                }
                if name.is_empty() {
                    // $# — positional parameter count
                    let env = ENV_VARS.lock();
                    let count = env.get("#").cloned().unwrap_or_else(|| String::from("0"));
                    result.push_str(&count);
                } else {
                    // Not a standard form, treat as $# followed by name
                    let env = ENV_VARS.lock();
                    let count = env.get("#").cloned().unwrap_or_else(|| String::from("0"));
                    result.push_str(&count);
                    result.push_str(&name);
                }
            } else if chars[i + 1] == '@' || chars[i + 1] == '*' {
                // $@ or $* — all positional parameters
                let env = ENV_VARS.lock();
                let val = env.get("@").cloned().unwrap_or_default();
                result.push_str(&val);
                i += 2;
            } else if chars[i + 1].is_ascii_digit() {
                // $0, $1, ... $9 — positional parameters
                let digit = chars[i + 1];
                let env = ENV_VARS.lock();
                if let Some(val) = env.get(&alloc::format!("{}", digit)) {
                    result.push_str(val);
                }
                i += 2;
            } else {
                // $VAR — simple variable
                i += 1;
                let mut name = String::new();
                while i < len && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    name.push(chars[i]);
                    i += 1;
                }
                let env = ENV_VARS.lock();
                if let Some(val) = env.get(&name) {
                    result.push_str(val);
                }
            }
        } else if chars[i] == '~' && result.is_empty() {
            let env = ENV_VARS.lock();
            if let Some(home) = env.get("HOME") {
                result.push_str(home);
            } else {
                result.push('~');
            }
            i += 1;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    result
}

/// Expand a ${...} parameter expression
fn expand_param_expression(expr: &str) -> String {
    let env = ENV_VARS.lock();

    // ${#VAR} — string length
    if let Some(name) = expr.strip_prefix('#') {
        let val = env.get(name).cloned().unwrap_or_default();
        return alloc::format!("{}", val.len());
    }

    // ${VAR/pat/rep} or ${VAR//pat/rep} — pattern substitution
    if let Some(slash_pos) = expr.find('/') {
        let name = &expr[..slash_pos];
        let rest = &expr[slash_pos + 1..];
        let global = rest.starts_with('/');
        let rest = if global { &rest[1..] } else { rest };

        let (pattern, replacement) = if let Some(sep) = rest.find('/') {
            (&rest[..sep], &rest[sep + 1..])
        } else {
            (rest, "")
        };

        let val = env.get(name).cloned().unwrap_or_default();
        drop(env);

        if global {
            return val.replace(pattern, replacement);
        } else {
            return val.replacen(pattern, replacement, 1);
        }
    }

    // ${VAR%%pattern} — remove longest suffix match
    if let Some(pct) = expr.find("%%") {
        let name = &expr[..pct];
        let pattern = &expr[pct + 2..];
        let val = env.get(name).cloned().unwrap_or_default();
        drop(env);
        // Find longest suffix match
        for i in 0..=val.len() {
            if glob_match(pattern, &val[i..]) {
                return String::from(&val[..i]);
            }
        }
        return val;
    }

    // ${VAR%pattern} — remove shortest suffix match
    if let Some(pct) = expr.find('%') {
        let name = &expr[..pct];
        let pattern = &expr[pct + 1..];
        let val = env.get(name).cloned().unwrap_or_default();
        drop(env);
        // Find shortest suffix match
        for i in (0..=val.len()).rev() {
            if glob_match(pattern, &val[i..]) {
                return String::from(&val[..i]);
            }
        }
        return val;
    }

    // ${VAR##pattern} — remove longest prefix match
    if let Some(hash) = expr.find("##") {
        let name = &expr[..hash];
        let pattern = &expr[hash + 2..];
        let val = env.get(name).cloned().unwrap_or_default();
        drop(env);
        // Find longest prefix match
        for i in (0..=val.len()).rev() {
            if glob_match(pattern, &val[..i]) {
                return String::from(&val[i..]);
            }
        }
        return val;
    }

    // ${VAR#pattern} — remove shortest prefix match
    if let Some(hash) = expr.find('#') {
        let name = &expr[..hash];
        let pattern = &expr[hash + 1..];
        let val = env.get(name).cloned().unwrap_or_default();
        drop(env);
        // Find shortest prefix match
        for i in 0..=val.len() {
            if glob_match(pattern, &val[..i]) {
                return String::from(&val[i..]);
            }
        }
        return val;
    }

    // ${VAR:-default} — use default if unset or empty
    if let Some(pos) = expr.find(":-") {
        let name = &expr[..pos];
        let default = &expr[pos + 2..];
        let val = env.get(name).cloned().unwrap_or_default();
        return if val.is_empty() {
            String::from(default)
        } else {
            val
        };
    }

    // ${VAR-default} — use default if unset (not if empty)
    if let Some(pos) = expr.find('-') {
        if pos > 0 && !expr[..pos].contains(':') {
            let name = &expr[..pos];
            return env
                .get(name)
                .cloned()
                .unwrap_or_else(|| String::from(&expr[pos + 1..]));
        }
    }

    // ${VAR:+alternate} — use alternate if set and non-empty
    if let Some(pos) = expr.find(":+") {
        let name = &expr[..pos];
        let alternate = &expr[pos + 2..];
        let val = env.get(name).cloned().unwrap_or_default();
        return if !val.is_empty() {
            String::from(alternate)
        } else {
            String::new()
        };
    }

    // ${VAR:=assign} — assign default if unset or empty
    if let Some(pos) = expr.find(":=") {
        let name = &expr[..pos];
        let default = &expr[pos + 2..];
        let val = env.get(name).cloned().unwrap_or_default();
        if val.is_empty() {
            drop(env);
            ENV_VARS
                .lock()
                .insert(String::from(name), String::from(default));
            return String::from(default);
        }
        return val;
    }

    // ${VAR:?error} — error if unset or empty
    if let Some(pos) = expr.find(":?") {
        let name = &expr[..pos];
        let error_msg = &expr[pos + 2..];
        let val = env.get(name).cloned().unwrap_or_default();
        if val.is_empty() {
            crate::serial_println!("ksh: {}: {}", name, error_msg);
            return String::new();
        }
        return val;
    }

    // Simple ${VAR}
    env.get(expr).cloned().unwrap_or_default()
}

// ── Brace Expansion ─────────────────────────────────────────────

/// Expand brace expressions: {a,b,c} and {1..5}
/// e.g., "file{1,2,3}.txt" → "file1.txt file2.txt file3.txt"
/// e.g., "file{1..3}.txt" → "file1.txt file2.txt file3.txt"
pub fn expand_braces(input: &str) -> String {
    let tokens = tokenize(input);
    let mut result_tokens = Vec::new();

    for token in &tokens {
        let expanded = expand_brace_token(token);
        result_tokens.extend(expanded);
    }

    result_tokens.join(" ")
}

/// Expand braces in a single token
fn expand_brace_token(token: &str) -> Vec<String> {
    // Find the first { that has a matching }
    let mut depth = 0;
    let mut brace_start = None;
    let mut brace_end = None;

    for (i, ch) in token.char_indices() {
        match ch {
            '{' => {
                if depth == 0 {
                    brace_start = Some(i);
                }
                depth += 1;
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    brace_end = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }

    let (start, end) = match (brace_start, brace_end) {
        (Some(s), Some(e)) => (s, e),
        _ => return alloc::vec![String::from(token)],
    };

    let prefix = &token[..start];
    let brace_content = &token[start + 1..end];
    let suffix = &token[end + 1..];

    // Check for range pattern: {N..M}
    if let Some(range_items) = try_expand_range(brace_content) {
        let mut results = Vec::new();
        for item in &range_items {
            let expanded = alloc::format!("{}{}{}", prefix, item, suffix);
            // Recursively expand remaining braces in suffix
            results.extend(expand_brace_token(&expanded));
        }
        return results;
    }

    // Comma-separated: {a,b,c}
    let items = split_brace_items(brace_content);
    if items.len() <= 1 {
        // Not a valid brace expansion (no comma, not a range)
        return alloc::vec![String::from(token)];
    }

    let mut results = Vec::new();
    for item in &items {
        let expanded = alloc::format!("{}{}{}", prefix, item, suffix);
        results.extend(expand_brace_token(&expanded));
    }

    results
}

/// Try to expand a range like "1..5" or "a..z"
fn try_expand_range(content: &str) -> Option<Vec<String>> {
    let parts: Vec<&str> = content.splitn(2, "..").collect();
    if parts.len() != 2 {
        return None;
    }

    // Try numeric range
    if let (Ok(start), Ok(end)) = (parts[0].parse::<i64>(), parts[1].parse::<i64>()) {
        let mut items = Vec::new();
        if start <= end {
            let mut i = start;
            while i <= end {
                items.push(alloc::format!("{}", i));
                i += 1;
            }
        } else {
            let mut i = start;
            while i >= end {
                items.push(alloc::format!("{}", i));
                i -= 1;
            }
        }
        return Some(items);
    }

    // Try single-character range (a..z)
    let start_chars: Vec<char> = parts[0].chars().collect();
    let end_chars: Vec<char> = parts[1].chars().collect();
    if start_chars.len() == 1 && end_chars.len() == 1 {
        let s = start_chars[0] as u32;
        let e = end_chars[0] as u32;
        let mut items = Vec::new();
        if s <= e {
            let mut c = s;
            while c <= e {
                if let Some(ch) = char::from_u32(c) {
                    items.push(alloc::format!("{}", ch));
                }
                c += 1;
            }
        } else {
            let mut c = s;
            while c >= e {
                if let Some(ch) = char::from_u32(c) {
                    items.push(alloc::format!("{}", ch));
                }
                c -= 1;
            }
        }
        return Some(items);
    }

    None
}

/// Split brace content by commas, respecting nested braces
fn split_brace_items(content: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut current = String::new();
    let mut depth = 0;

    for ch in content.chars() {
        match ch {
            '{' => {
                depth += 1;
                current.push(ch);
            }
            '}' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => {
                items.push(current.clone());
                current.clear();
            }
            _ => {
                current.push(ch);
            }
        }
    }
    if !current.is_empty() || content.ends_with(',') {
        items.push(current);
    }

    items
}

// ── Glob Expansion ──────────────────────────────────────────────

/// Expand glob patterns in a list of tokens
fn expand_globs_in_tokens(tokens: Vec<String>) -> Vec<String> {
    let mut result = Vec::new();

    for token in &tokens {
        // Don't glob-expand redirect operators and similar
        if token == ">"
            || token == ">>"
            || token == "<"
            || token == "2>"
            || token == "2>>"
            || token == "&"
        {
            result.push(token.clone());
            continue;
        }

        if contains_glob(token) {
            let matches = expand_glob(token);
            if matches.is_empty() {
                // No matches: keep the original pattern (bash behavior)
                result.push(token.clone());
            } else {
                result.extend(matches);
            }
        } else {
            result.push(token.clone());
        }
    }

    result
}

/// Check if a token contains glob metacharacters
fn contains_glob(token: &str) -> bool {
    let mut escape_next = false;
    for ch in token.chars() {
        if escape_next {
            escape_next = false;
            continue;
        }
        match ch {
            '\\' => escape_next = true,
            '*' | '?' | '[' => return true,
            _ => {}
        }
    }
    false
}

/// Expand a glob pattern against the VFS
fn expand_glob(pattern: &str) -> Vec<String> {
    use crate::shell::helpers::resolve_path;

    // Split pattern into directory and file parts
    let (dir_part, file_pattern) = if let Some(last_slash) = pattern.rfind('/') {
        (&pattern[..last_slash], &pattern[last_slash + 1..])
    } else {
        (".", pattern)
    };

    let dir_path = resolve_path(dir_part);
    let vfs = crate::vfs::VFS.lock();

    let entries = match vfs.list_dir(&dir_path) {
        Some(e) => e,
        None => return Vec::new(),
    };

    let mut matches: Vec<String> = Vec::new();
    for entry in &entries {
        if glob_match(file_pattern, entry) {
            let full = if dir_part == "." {
                entry.clone()
            } else {
                alloc::format!("{}/{}", dir_part, entry)
            };
            matches.push(full);
        }
    }

    matches.sort();
    matches
}

/// Match a string against a glob pattern supporting *, ?, and [...]
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let pat: Vec<char> = pattern.chars().collect();
    let txt: Vec<char> = text.chars().collect();
    glob_match_impl(&pat, 0, &txt, 0)
}

fn glob_match_impl(pat: &[char], pi: usize, txt: &[char], ti: usize) -> bool {
    let mut pi = pi;
    let mut ti = ti;

    while pi < pat.len() {
        match pat[pi] {
            '*' => {
                // Skip consecutive stars
                while pi < pat.len() && pat[pi] == '*' {
                    pi += 1;
                }
                if pi == pat.len() {
                    return true; // trailing * matches everything
                }
                // Try matching rest of pattern at each position
                while ti <= txt.len() {
                    if glob_match_impl(pat, pi, txt, ti) {
                        return true;
                    }
                    ti += 1;
                }
                return false;
            }
            '?' => {
                if ti >= txt.len() {
                    return false;
                }
                // Don't match hidden files with ? at start
                if ti == 0 && txt[ti] == '.' {
                    return false;
                }
                pi += 1;
                ti += 1;
            }
            '[' => {
                if ti >= txt.len() {
                    return false;
                }
                pi += 1;
                let negate = pi < pat.len() && (pat[pi] == '!' || pat[pi] == '^');
                if negate {
                    pi += 1;
                }
                let mut matched = false;
                let mut first = true;
                while pi < pat.len() && (first || pat[pi] != ']') {
                    first = false;
                    if pi + 2 < pat.len() && pat[pi + 1] == '-' {
                        // Range: [a-z]
                        let lo = pat[pi];
                        let hi = pat[pi + 2];
                        if txt[ti] >= lo && txt[ti] <= hi {
                            matched = true;
                        }
                        pi += 3;
                    } else {
                        if pat[pi] == txt[ti] {
                            matched = true;
                        }
                        pi += 1;
                    }
                }
                if pi < pat.len() && pat[pi] == ']' {
                    pi += 1;
                }
                if negate {
                    matched = !matched;
                }
                if !matched {
                    return false;
                }
                ti += 1;
            }
            ch => {
                if ti >= txt.len() || txt[ti] != ch {
                    return false;
                }
                // Don't match hidden files unless pattern starts with .
                if ti == 0 && txt[ti] == '.' && ch != '.' {
                    return false;
                }
                pi += 1;
                ti += 1;
            }
        }
    }

    ti == txt.len()
}

// ── Process Substitution ────────────────────────────────────────

/// Expand process substitution: <(command) and >(command)
/// <(cmd) runs cmd and substitutes its output inline (as if from a temp file)
/// >(cmd) captures output directed to it and feeds to cmd's stdin
fn expand_process_substitution(input: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        // Detect <( or >(
        if i + 1 < len && chars[i + 1] == '(' && (chars[i] == '<' || chars[i] == '>') {
            let is_input = chars[i] == '<'; // <(...) = input substitution
            // Find matching closing paren
            let start = i + 2;
            let mut depth = 1u32;
            let mut end = start;
            while end < len {
                if chars[end] == '(' {
                    depth += 1;
                }
                if chars[end] == ')' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                end += 1;
            }

            if depth == 0 {
                // Extract the command inside the parens
                let inner_cmd: String = chars[start..end].iter().collect();

                if is_input {
                    // <(cmd) — execute command and substitute output
                    let cmd_result = super::execute(&inner_cmd);
                    // Write output to a temp path in VFS and substitute path
                    let temp_path = alloc::format!("/tmp/.proc_subst_{}", i);
                    {
                        let mut vfs = crate::vfs::VFS.lock();
                        vfs.write_file(&temp_path, cmd_result.output.as_bytes());
                    }
                    result.push_str(&temp_path);
                } else {
                    // >(cmd) — create a temp file that will be fed to cmd
                    let temp_path = alloc::format!("/tmp/.proc_subst_out_{}", i);
                    result.push_str(&temp_path);
                    // The command will be executed after the main command writes to temp_path
                    // Store it for later execution
                    crate::shell::env::ENV_VARS
                        .lock()
                        .insert(alloc::format!("__PROC_SUBST_{}__", i), inner_cmd);
                }
                i = end + 1;
                continue;
            }
        }
        result.push(chars[i]);
        i += 1;
    }

    result
}
