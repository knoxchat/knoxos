// SPDX-License-Identifier: MIT
//! Here documents, .profile/.bashrc loading (items 10.32, 10.34)
//!
//! Extends the shell with:
//! - Here documents (<<EOF ... EOF)
//! - Startup script loading (.profile, .bashrc, /etc/profile)

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

static PROFILE_LOADED: AtomicBool = AtomicBool::new(false);
static HEREDOCS_PROCESSED: AtomicU64 = AtomicU64::new(0);

/// Startup script paths (loaded in order)
const STARTUP_SCRIPTS: &[&str] = &[
    "/etc/profile",
    "/etc/profile.d/*.sh",
    "/home/user/.profile",
    "/home/user/.bashrc",
    "/home/user/.knoxrc",
];

/// Parse a here document from shell input lines
///
/// Given input starting with `<<DELIMITER`, reads lines until a line
/// contains only DELIMITER, and returns the collected text.
///
/// Supports:
/// - `<<EOF` — standard here document (tabs preserved)
/// - `<<-EOF` — strip leading tabs
/// - `<<"EOF"` — no variable expansion
/// - `<<'EOF'` — no variable expansion
pub fn parse_heredoc(
    delimiter_line: &str,
    remaining_lines: &[String],
) -> Result<(String, usize), &'static str> {
    // Extract delimiter and flags
    let trimmed = delimiter_line.trim();

    // Find the << operator
    let heredoc_start = trimmed.find("<<").ok_or("no << operator found")?;
    let after_op = &trimmed[heredoc_start + 2..];

    let strip_tabs = after_op.starts_with('-');
    let delim_str = if strip_tabs { &after_op[1..] } else { after_op };

    // Remove quotes from delimiter
    let delimiter = delim_str.trim().trim_matches('"').trim_matches('\'').trim();

    let no_expand = delim_str.contains('"') || delim_str.contains('\'');

    if delimiter.is_empty() {
        return Err("empty heredoc delimiter");
    }

    let mut content = String::new();
    let mut lines_consumed = 0;

    for line in remaining_lines {
        lines_consumed += 1;
        let check_line = if strip_tabs {
            line.trim_start_matches('\t')
        } else {
            line.as_str()
        };

        if check_line.trim() == delimiter {
            HEREDOCS_PROCESSED.fetch_add(1, Ordering::Relaxed);
            return Ok((content, lines_consumed));
        }

        let processed_line = if strip_tabs {
            String::from(line.trim_start_matches('\t'))
        } else {
            line.clone()
        };

        // Variable expansion unless quoted delimiter
        let final_line = if no_expand {
            processed_line
        } else {
            expand_variables(&processed_line)
        };

        if !content.is_empty() {
            content.push('\n');
        }
        content.push_str(&final_line);
    }

    Err("heredoc: unexpected end of input, missing delimiter")
}

/// Simple variable expansion for here documents
fn expand_variables(line: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '$' && i + 1 < chars.len() {
            if chars[i + 1] == '{' {
                // ${VAR} form
                if let Some(end) = line[i + 2..].find('}') {
                    let var_name = &line[i + 2..i + 2 + end];
                    if let Some(val) = lookup_env_var(var_name) {
                        result.push_str(&val);
                    }
                    i += end + 3;
                    continue;
                }
            } else if chars[i + 1].is_alphanumeric() || chars[i + 1] == '_' {
                // $VAR form
                let start = i + 1;
                let mut end = start;
                while end < chars.len() && (chars[end].is_alphanumeric() || chars[end] == '_') {
                    end += 1;
                }
                let var_name: String = chars[start..end].iter().collect();
                if let Some(val) = lookup_env_var(&var_name) {
                    result.push_str(&val);
                }
                i = end;
                continue;
            }
        }
        result.push(chars[i]);
        i += 1;
    }

    result
}

/// Look up an environment variable
fn lookup_env_var(name: &str) -> Option<String> {
    if let Some(value) = crate::shell::env::ENV_VARS.lock().get(name).cloned() {
        return Some(value);
    }

    match name {
        "HOME" => Some(String::from("/home/user")),
        "USER" => Some(String::from("user")),
        "SHELL" => Some(String::from("/bin/knsh")),
        "PATH" => Some(String::from("/bin:/usr/bin:/usr/local/bin")),
        "HOSTNAME" => Some(String::from("knoxos")),
        _ => None,
    }
}

/// Load and execute startup scripts
pub fn load_startup_scripts() {
    if PROFILE_LOADED.load(Ordering::Acquire) {
        return;
    }

    for script_path in STARTUP_SCRIPTS {
        if script_path.contains('*') {
            // Glob pattern — skip for now
            continue;
        }

        match crate::file_manager::read_file(script_path) {
            Ok(data) => {
                if let Ok(content) = core::str::from_utf8(&data) {
                    crate::serial_println!("[shell_startup] loading {}", script_path);
                    execute_script(content);
                }
            }
            Err(_) => {
                // Script doesn't exist, that's fine
            }
        }
    }

    PROFILE_LOADED.store(true, Ordering::Release);
    crate::serial_println!("[shell_startup] startup scripts loaded");
}

/// Execute a shell script line by line
fn execute_script(content: &str) {
    let lines: Vec<&str> = content.lines().collect();

    for line in &lines {
        let trimmed = line.trim();
        // Skip empty lines and comments
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Handle export VAR=value
        if let Some(rest) = trimmed.strip_prefix("export ") {
            if let Some(eq_pos) = rest.find('=') {
                let var = rest[..eq_pos].trim();
                let raw_val = rest[eq_pos + 1..].trim();
                let unquoted = raw_val.trim_matches('"').trim_matches('\'');
                let val = expand_variables(unquoted);
                crate::shell::env::ENV_VARS
                    .lock()
                    .insert(String::from(var), val);
            }
            continue;
        }

        // Handle alias name='value'
        if let Some(rest) = trimmed.strip_prefix("alias ") {
            if let Some(eq_pos) = rest.find('=') {
                let name = rest[..eq_pos].trim();
                let value = rest[eq_pos + 1..]
                    .trim()
                    .trim_matches('\'')
                    .trim_matches('"');
                crate::shell::env::SHELL_ALIASES
                    .lock()
                    .insert(String::from(name), String::from(value));
            }
            continue;
        }

        // Other commands — delegate to shell
        let _ = crate::shell::execute(trimmed);
    }
}

/// Create default .profile if it doesn't exist
pub fn create_default_profile() {
    let profile_path = "/home/user/.profile";
    if crate::file_manager::read_file(profile_path).is_err() {
        let default_profile = "\
# KnoxOS user profile
# This file is sourced at login

export PATH=\"/bin:/usr/bin:/usr/local/bin:$HOME/bin\"
export EDITOR=kedit
export PAGER=less
export LANG=en_US.UTF-8

# Aliases
alias ll='ls -la'
alias la='ls -a'
alias ..='cd ..'
alias ...='cd ../..'
alias grep='grep --color=auto'

# Welcome message
echo \"Welcome to KnoxOS, $USER!\"
";
        let _ = crate::file_manager::write_file(profile_path, default_profile.as_bytes());
        crate::serial_println!("[shell_startup] created default {}", profile_path);
    }
}

/// Check if startup scripts have been loaded
pub fn is_profile_loaded() -> bool {
    PROFILE_LOADED.load(Ordering::Acquire)
}

pub fn stats() -> u64 {
    HEREDOCS_PROCESSED.load(Ordering::Relaxed)
}

/// Initialize the shell startup subsystem
pub fn init() {
    create_default_profile();
    load_startup_scripts();
    crate::serial_println!("[shell_startup] initialized");
}
