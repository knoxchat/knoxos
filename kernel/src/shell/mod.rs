/// Shell — KnoxOS built-in fish-inspired command shell
///
/// Module layout:
///   shell/
///   ├── mod.rs           ← This file: public API, init, execute (pipe/redirect/logical ops)
///   ├── types.rs         ← Command, RedirectType, ShellResult, CommandLine, LogicalOp, Pipeline
///   ├── env.rs           ← Environment variables, command history, exit code tracking, job table
///   ├── parser.rs        ← Tokenizer, variable expansion, pipe/redirect/logical/glob/brace parsing
///   ├── helpers.rs       ← Path resolution, prompt generation, permission formatting
///   └── builtins/
///       ├── mod.rs       ← Builtin dispatcher & BUILTIN_NAMES list
///       ├── core_cmds.rs ← echo, cd, pwd, export, exit, history, help, type, test …
///       ├── files.rs     ← ls, cat, touch, mkdir, rm, cp, mv, head, tail, grep …
///       ├── system.rs    ← uname, hostname, whoami, ps, kill, free, df, date …
///       ├── net.rs       ← ifconfig, ping
///       ├── kernel.rs    ← dmesg, lsmod, insmod, rmmod, mount, poweroff, reboot …
///       ├── package.rs   ← kpm (package manager)
///       └── security.rs  ← getenforce, sestatus, lscgroup, gpu_info
// ── Sub-modules ─────────────────────────────────────────────────
pub mod builtins;
pub mod env;
pub mod helpers;
pub mod parser;
pub mod scripting;
pub mod types;

// ── Re-exports (public API consumed by terminal.rs, main.rs, etc.) ──
pub use env::{ENV_VARS, add_history, get_history};
pub use helpers::{get_prompt, get_prompt_plain};
pub use parser::parse_command_line;
pub use types::{Command, CommandLine, LogicalOp, Pipeline, RedirectType, ShellResult};

use crate::serial_println;
use alloc::string::String;
use alloc::vec::Vec;

/// Execute a command line (the primary entry point called from the terminal)
///
/// Supports:
///   - Pipes: `cmd1 | cmd2 | cmd3` — real data flow between commands
///   - Redirects: `cmd > file`, `cmd >> file`, `cmd < file`, `cmd 2> file`
///   - Logical operators: `cmd1 && cmd2`, `cmd1 || cmd2`, `cmd1 ; cmd2`
///   - Background: `cmd &`
///   - Glob expansion: `ls *.rs`, `cat file?.txt`, `rm [abc].log`
///   - Brace expansion: `echo {1..5}`, `touch file{a,b,c}.txt`
///   - Variable expansion: `$VAR`, `${VAR}`, `$?`, `$$`, `$!`
pub fn execute(line: &str) -> ShellResult {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return ShellResult::ok("");
    }

    // Add to history
    add_history(line);

    // Expand aliases: replace the first word with alias value if defined
    let line = expand_aliases(line);

    // Parse the full command line with logical operators
    let cmd_line = parser::parse_command_line(&line);

    // Execute the command line with logical operator handling
    execute_command_line(&cmd_line)
}

/// Execute a CommandLine (pipelines connected by &&, ||, ;)
fn execute_command_line(cmd_line: &CommandLine) -> ShellResult {
    // Execute the first pipeline
    let mut last_result = execute_pipeline(&cmd_line.first);
    env::set_last_exit_code(last_result.exit_code);

    // Execute remaining pipelines based on logical operators
    for (op, pipeline) in &cmd_line.rest {
        match op {
            LogicalOp::And => {
                // && — only execute if previous succeeded
                if last_result.exit_code == 0 {
                    last_result = execute_pipeline(pipeline);
                    env::set_last_exit_code(last_result.exit_code);
                }
            }
            LogicalOp::Or => {
                // || — only execute if previous failed
                if last_result.exit_code != 0 {
                    last_result = execute_pipeline(pipeline);
                    env::set_last_exit_code(last_result.exit_code);
                }
            }
            LogicalOp::Semi => {
                // ; — always execute
                last_result = execute_pipeline(pipeline);
                env::set_last_exit_code(last_result.exit_code);
            }
        }
    }

    last_result
}

/// Execute a pipeline (commands connected by |) with real data flow
fn execute_pipeline(pipeline: &Pipeline) -> ShellResult {
    let commands = &pipeline.commands;

    if commands.is_empty() {
        return ShellResult::ok("");
    }

    // Single command: simple execution with I/O redirect
    if commands.len() == 1 {
        let cmd = &commands[0];

        // Handle background execution
        if cmd.background {
            return execute_background(cmd);
        }

        // Read stdin from file if redirected
        let stdin_data = read_stdin_redirect(cmd);

        // Execute the command
        let mut result = execute_command_with_stdin(cmd, stdin_data.as_deref());

        // Handle stdout redirection
        handle_stdout_redirect(cmd, &mut result);

        // Handle stderr redirection (for error output)
        handle_stderr_redirect(cmd, &result);

        return result;
    }

    // Multi-command pipeline: pipe stdout of each command to stdin of next
    let mut pipe_data: Option<String> = None;
    let mut last_result = ShellResult::ok("");

    for (i, cmd) in commands.iter().enumerate() {
        let is_last = i == commands.len() - 1;

        // For the first command, use stdin redirect if present; otherwise None
        // For subsequent commands, use the previous command's output as stdin
        let stdin_input = if i == 0 {
            read_stdin_redirect(cmd)
        } else {
            pipe_data.clone()
        };

        // Execute the command with piped stdin
        let mut result = execute_command_with_stdin(cmd, stdin_input.as_deref());

        if is_last {
            // Last command in pipeline: handle its stdout redirect
            handle_stdout_redirect(cmd, &mut result);
            handle_stderr_redirect(cmd, &result);
            last_result = result;
        } else {
            // Not the last command: capture output for the next command's stdin
            pipe_data = Some(result.output);
        }
    }

    last_result
}

/// Execute a single command, optionally providing stdin data from a pipe
fn execute_command_with_stdin(cmd: &Command, stdin_data: Option<&str>) -> ShellResult {
    if cmd.program.is_empty() {
        return ShellResult::ok("");
    }

    // Handle subshell: (...)
    if cmd.program.starts_with('(') && cmd.program.ends_with(')') {
        let inner = &cmd.program[1..cmd.program.len() - 1];
        return scripting::execute_subshell(inner);
    }

    // Try builtin dispatch first, passing stdin data
    if let Some(result) = builtins::dispatch_with_stdin(&cmd.program, &cmd.args, stdin_data) {
        return result;
    }

    // Try shell function call
    if scripting::is_function(&cmd.program) {
        if let Some(result) = scripting::call_function(&cmd.program, &cmd.args) {
            return result;
        }
    }

    // Not a builtin or function — try to resolve from VFS / PATH
    let path = helpers::resolve_command_path(&cmd.program);
    if let Some(ref elf_path) = path {
        // Check if it's a shell script (non-ELF)
        let is_script = {
            let vfs = crate::vfs::VFS.lock();
            if let Some(data) = vfs.read_file(elf_path) {
                data.starts_with(b"#!")
                    || data.starts_with(b"#!/")
                    || (!data.starts_with(b"\x7fELF") && core::str::from_utf8(data).is_ok())
            } else {
                false
            }
        };
        if is_script {
            return scripting::execute_script_file(elf_path, &cmd.args);
        }
        // External ELF binary execution via fork + exec_elf
        execute_external(elf_path, cmd, stdin_data)
    } else {
        ShellResult::err(&alloc::format!("ksh: {}: command not found", cmd.program))
    }
}

/// Execute an external binary (ELF) from the VFS
fn execute_external(elf_path: &str, cmd: &Command, _stdin_data: Option<&str>) -> ShellResult {
    // Read ELF binary data from VFS
    let elf_data = {
        let vfs = crate::vfs::VFS.lock();
        match vfs.read_file(elf_path) {
            Some(data) => data.to_vec(),
            None => {
                return ShellResult::err(&alloc::format!(
                    "ksh: {}: cannot read binary",
                    cmd.program
                ));
            }
        }
    };

    // Build argv: [program_name, arg1, arg2, ...]
    let mut argv_strings = Vec::new();
    argv_strings.push(cmd.program.clone());
    for arg in &cmd.args {
        argv_strings.push(arg.clone());
    }
    let argv_refs: Vec<&str> = argv_strings.iter().map(|s| s.as_str()).collect();

    // Build envp from shell ENV_VARS
    let envp_strings: Vec<String> = {
        let env = env::ENV_VARS.lock();
        env.iter()
            .map(|(k, v)| alloc::format!("{}={}", k, v))
            .collect()
    };
    let envp_refs: Vec<&str> = envp_strings.iter().map(|s| s.as_str()).collect();

    // Launch the ELF binary via exec_elf (fork + exec + address space setup)
    match crate::process::exec_elf(&elf_data, &cmd.program, &argv_refs, &envp_refs) {
        Some(child_pid) => {
            // Register child in session/process group
            let shell_pid = env::get_shell_pid();
            let _ = crate::pgrp::setpgid(child_pid, child_pid);

            if cmd.background {
                // Background execution
                let job_id = env::add_background_job(child_pid, &cmd.program);
                ShellResult::ok(&alloc::format!("[{}] {}\n", job_id, child_pid))
            } else {
                // Foreground execution — wait for child to finish
                // Set child as foreground process group
                let _ = crate::pgrp::tcsetpgrp(0, child_pid);

                // Wait for child process to complete
                let exit_code = wait_for_child(child_pid);

                // Restore shell as foreground process group
                let _ = crate::pgrp::tcsetpgrp(0, shell_pid);

                env::set_last_exit_code(exit_code);
                if exit_code == 0 {
                    ShellResult::ok("")
                } else {
                    ShellResult::with_code(exit_code, "")
                }
            }
        }
        None => ShellResult::err(&alloc::format!(
            "ksh: {}: failed to execute (bad ELF or out of resources)",
            cmd.program
        )),
    }
}

/// Wait for a child process to complete, returning exit code
fn wait_for_child(child_pid: u32) -> i32 {
    // Poll for child process completion
    // In a real preemptive OS we'd block; here we poll the process table
    loop {
        if let Some((_pid, status)) =
            crate::process::wait_child(env::get_shell_pid(), child_pid as i32)
        {
            return status;
        }
        // Check if process still exists
        let table = crate::process::PROCESS_TABLE.lock();
        if table.get_process(child_pid).is_none() {
            return 0; // Process already cleaned up
        }
        let state = table.get_process(child_pid).map(|p| p.state);
        drop(table);
        match state {
            Some(crate::process::ProcessState::Zombie) => {
                // Reap it
                if let Some((_pid, status)) =
                    crate::process::wait_child(env::get_shell_pid(), child_pid as i32)
                {
                    return status;
                }
                return 0;
            }
            Some(crate::process::ProcessState::Stopped) => {
                // Job was stopped (Ctrl+Z) — add to job table and return
                env::stop_foreground_job();
                return 128 + 20; // 128 + SIGTSTP
            }
            None => return 127, // Process gone
            _ => {
                // Still running — yield
                crate::scheduler::yield_current();
            }
        }
    }
}

/// Execute a command (backward-compatible, no stdin)
fn execute_single_command(cmd: &Command) -> ShellResult {
    execute_command_with_stdin(cmd, None)
}

/// Read stdin from a file redirect (`< file`) or here-string (`<<< word`)
fn read_stdin_redirect(cmd: &Command) -> Option<String> {
    // Here-string takes priority: <<< "word"
    if let Some(ref hs) = cmd.herestring {
        let mut content = hs.clone();
        content.push('\n'); // Here-strings append a trailing newline
        return Some(content);
    }

    if let Some(ref path) = cmd.stdin_redirect {
        let resolved = helpers::resolve_path(path);
        let vfs = crate::vfs::VFS.lock();
        if let Some(data) = vfs.read_file(&resolved) {
            if let Ok(s) = core::str::from_utf8(data) {
                return Some(String::from(s));
            }
        }
    }
    None
}

/// Handle stdout redirection (> or >>)
fn handle_stdout_redirect(cmd: &Command, result: &mut ShellResult) {
    if let Some(ref redirect) = cmd.stdout_redirect {
        let (path, append) = match redirect {
            RedirectType::Overwrite(p) => (p.clone(), false),
            RedirectType::Append(p) => (p.clone(), true),
        };

        let resolved = helpers::resolve_path(&path);
        let mut vfs = crate::vfs::VFS.lock();

        if append {
            // >> : append to existing file
            let existing = vfs
                .read_file(&resolved)
                .map(|data| {
                    let mut v = Vec::from(data);
                    v.extend_from_slice(result.output.as_bytes());
                    v
                })
                .unwrap_or_else(|| Vec::from(result.output.as_bytes()));
            vfs.write_file(&resolved, &existing);
        } else {
            // > : overwrite
            vfs.write_file(&resolved, result.output.as_bytes());
        }

        // Clear output since it was redirected to a file
        result.output = String::new();
    }
}

/// Handle stderr redirection (2> or 2>>)
fn handle_stderr_redirect(cmd: &Command, result: &ShellResult) {
    if let Some(ref redirect) = cmd.stderr_redirect {
        // Only redirect if the command actually failed (has error output)
        if result.exit_code != 0 && !result.output.is_empty() {
            let (path, append) = match redirect {
                RedirectType::Overwrite(p) => (p.clone(), false),
                RedirectType::Append(p) => (p.clone(), true),
            };

            let resolved = helpers::resolve_path(&path);
            let mut vfs = crate::vfs::VFS.lock();

            if append {
                let existing = vfs
                    .read_file(&resolved)
                    .map(|data| {
                        let mut v = Vec::from(data);
                        v.extend_from_slice(result.output.as_bytes());
                        v
                    })
                    .unwrap_or_else(|| Vec::from(result.output.as_bytes()));
                vfs.write_file(&resolved, &existing);
            } else {
                vfs.write_file(&resolved, result.output.as_bytes());
            }
        }
    }
}

/// Execute a command in the background
fn execute_background(cmd: &Command) -> ShellResult {
    // In our in-kernel shell, we simulate background execution
    // by running the command and tracking it as a "background job"
    let command_str = alloc::format!("{} {}", cmd.program, cmd.args.join(" "));

    // Spawn a "background" process entry
    let pid = {
        let mut table = crate::process::PROCESS_TABLE.lock();
        let parent_pid = crate::scheduler::current_pid().unwrap_or(1);
        table.spawn(&cmd.program, parent_pid)
    };

    // Track the job
    let job_id = env::add_background_job(pid, &command_str);

    // Actually run the command (in-kernel, this is synchronous for builtins)
    let mut bg_cmd = cmd.clone();
    bg_cmd.background = false; // prevent infinite recursion
    let result = execute_single_command(&bg_cmd);

    // Mark the job as done
    {
        let mut jobs = env::BACKGROUND_JOBS.lock();
        if let Some(job) = jobs.iter_mut().find(|j| j.job_id == job_id) {
            job.status = env::JobStatus::Done;
        }
    }

    env::set_last_exit_code(result.exit_code);

    ShellResult::ok(&alloc::format!("[{}] {}\n", job_id, pid))
}

/// Expand aliases in a command line
/// Replaces the first word of each command segment with alias expansion
fn expand_aliases(line: &str) -> String {
    let aliases = env::SHELL_ALIASES.lock();
    if aliases.is_empty() {
        return String::from(line);
    }

    // Split by pipes and logical operators, expand alias on first word of each
    let mut result = String::new();
    let mut remaining = line;
    let mut first_in_segment = true;

    let chars: Vec<char> = remaining.chars().collect();
    let mut i = 0;
    let mut word_start = 0;
    let mut in_quote = false;
    let mut quote_char = ' ';

    while i < chars.len() {
        match chars[i] {
            '\'' | '"' if !in_quote => {
                in_quote = true;
                quote_char = chars[i];
                first_in_segment = false;
            }
            c if in_quote && c == quote_char => {
                in_quote = false;
            }
            '|' | ';' if !in_quote => {
                // Flush current content
                result.push(chars[i]);
                first_in_segment = true;
                i += 1;
                // Skip || (logical OR)
                if i < chars.len() && chars[i] == '|' {
                    result.push('|');
                    i += 1;
                }
                word_start = i;
                continue;
            }
            '&' if !in_quote && i + 1 < chars.len() && chars[i + 1] == '&' => {
                result.push('&');
                result.push('&');
                i += 2;
                first_in_segment = true;
                word_start = i;
                continue;
            }
            ' ' | '\t' if first_in_segment && !in_quote => {
                if i > word_start {
                    // We have a word — check if it's an alias
                    let word: String = chars[word_start..i].iter().collect();
                    let word_trimmed = word.trim();
                    if let Some(expansion) = aliases.get(word_trimmed) {
                        result.push_str(expansion);
                    } else {
                        result.push_str(&word);
                    }
                    first_in_segment = false;
                }
                result.push(chars[i]);
                i += 1;
                word_start = i;
                continue;
            }
            _ => {}
        }
        // If we hit a non-space after the first word, just copy
        if !first_in_segment {
            result.push(chars[i]);
        }
        i += 1;
    }

    // Handle the last word segment
    if first_in_segment && word_start < chars.len() {
        let word: String = chars[word_start..].iter().collect();
        let word_trimmed = word.trim();
        if let Some(expansion) = aliases.get(word_trimmed) {
            result.push_str(expansion);
        } else {
            result.push_str(&word);
        }
    } else if word_start < i {
        // Characters from the last non-first segment might not have been flushed
        // (they were pushed inside the loop)
    }

    if result.is_empty() {
        // Fallback: simple first-word alias expansion
        let first_space = line.find(' ').unwrap_or(line.len());
        let first_word = &line[..first_space];
        if let Some(expansion) = aliases.get(first_word) {
            let mut s = expansion.clone();
            s.push_str(&line[first_space..]);
            return s;
        }
        return String::from(line);
    }

    result
}

/// Set an environment variable for the shell
pub fn set_env_var(key: &str, value: &str) {
    serial_println!("[shell] set_env_var: {}={}", key, value);
    // Store in a simple environment table
    // For now this is a stub; a full implementation would maintain
    // a BTreeMap<String, String> as the shell environment.
}

/// Execute a command string publicly (for use by other subsystems)
pub fn execute_command(cmd_str: &str) -> ShellResult {
    let pipeline = parser::parse_pipeline(cmd_str);
    execute_pipeline(&pipeline)
}

/// Initialize the shell subsystem
pub fn init() {
    // Load persistent history from disk
    builtins::core_cmds::load_history();
    // Source .kshrc if it exists
    builtins::core_cmds::source_kshrc();
    serial_println!("[KnoxOS] Shell (ksh) initialized - Fish-inspired smart shell");
    serial_println!(
        "[KnoxOS]   {} builtin commands available",
        builtins::BUILTIN_NAMES.len()
    );
    serial_println!("[KnoxOS]   Features: autosuggestion, syntax highlighting, tab completion");
}
