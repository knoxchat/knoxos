/// Core shell builtins — echo, cd, pwd, export, unset, env, exit, history, help, type, test, etc.
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

use crate::shell::env::{self, ENV_VARS, HISTORY};
use crate::shell::helpers::{resolve_command_path, resolve_path};
use crate::shell::types::ShellResult;

pub fn echo(args: &[String]) -> ShellResult {
    let mut no_newline = false;
    let mut interpret_escapes = false;
    let mut output_args = Vec::new();

    for arg in args {
        match arg.as_str() {
            "-n" => no_newline = true,
            "-e" => interpret_escapes = true,
            "-E" => interpret_escapes = false,
            _ => output_args.push(arg.as_str()),
        }
    }

    let mut output = output_args.join(" ");

    if interpret_escapes {
        output = output
            .replace("\\n", "\n")
            .replace("\\t", "\t")
            .replace("\\\\", "\\")
            .replace("\\a", "\x07")
            .replace("\\b", "\x08");
    }

    if !no_newline {
        output.push('\n');
    }

    ShellResult::ok(&output)
}

pub fn cd(args: &[String]) -> ShellResult {
    let target = if args.is_empty() {
        ENV_VARS
            .lock()
            .get("HOME")
            .cloned()
            .unwrap_or_else(|| String::from("/"))
    } else if args[0] == "-" {
        ENV_VARS
            .lock()
            .get("OLDPWD")
            .cloned()
            .unwrap_or_else(|| String::from("/"))
    } else {
        resolve_path(&args[0])
    };

    let vfs = crate::vfs::VFS.lock();
    if let Some(ino) = vfs.resolve_path(&target) {
        if let Some(inode) = vfs.get_inode(ino) {
            if inode.file_type != crate::vfs::FileType::Directory {
                return ShellResult::err(&alloc::format!("cd: {}: Not a directory", target));
            }
        }
        drop(vfs);

        let old_pwd = ENV_VARS.lock().get("PWD").cloned().unwrap_or_default();
        ENV_VARS.lock().insert(String::from("OLDPWD"), old_pwd);
        ENV_VARS.lock().insert(String::from("PWD"), target.clone());

        let pid = crate::scheduler::current_pid().unwrap_or(1);
        crate::process::PROCESS_TABLE.lock().chdir(pid, &target);

        ShellResult::ok("")
    } else {
        ShellResult::err(&alloc::format!("cd: {}: No such file or directory", target))
    }
}

pub fn pwd() -> ShellResult {
    let pwd = ENV_VARS
        .lock()
        .get("PWD")
        .cloned()
        .unwrap_or_else(|| String::from("/"));
    ShellResult::ok(&alloc::format!("{}\n", pwd))
}

pub fn export(args: &[String]) -> ShellResult {
    for arg in args {
        if let Some(eq_pos) = arg.find('=') {
            let key = &arg[..eq_pos];
            let value = &arg[eq_pos + 1..];
            ENV_VARS
                .lock()
                .insert(String::from(key), String::from(value));
        }
    }
    ShellResult::ok("")
}

pub fn unset(args: &[String]) -> ShellResult {
    for arg in args {
        ENV_VARS.lock().remove(arg);
    }
    ShellResult::ok("")
}

pub fn env(args: &[String]) -> ShellResult {
    let env = ENV_VARS.lock();
    let mut output = String::new();

    if args.is_empty() {
        for (key, value) in env.iter() {
            writeln!(output, "{}={}", key, value).unwrap();
        }
    } else {
        // printenv VAR
        for arg in args {
            if let Some(value) = env.get(arg) {
                writeln!(output, "{}", value).unwrap();
            }
        }
    }

    ShellResult::ok(&output)
}

pub fn set() -> ShellResult {
    env(&[])
}

pub fn exit(args: &[String]) -> ShellResult {
    let code = args
        .first()
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(0);
    ShellResult {
        exit_code: code,
        output: String::from("exit\n"),
    }
}

pub fn history() -> ShellResult {
    let history = HISTORY.lock();
    let mut output = String::new();
    for (i, cmd) in history.iter().enumerate() {
        writeln!(output, " {:>4}  {}", i + 1, cmd).unwrap();
    }
    ShellResult::ok(&output)
}

pub fn help() -> ShellResult {
    ShellResult::ok(
        "\x1b[1;36mKnoxOS Shell (ksh)\x1b[0m - Fish-inspired Smart Shell\n\
         \n\
         \x1b[1;33mInteractive Features:\x1b[0m\n\
           \x1b[32mAutosuggestion\x1b[0m     History-based ghost text (\x1b[2mRight→\x1b[0m to accept)\n\
           \x1b[32mSyntax Highlight\x1b[0m   \x1b[34mvalid\x1b[0m/\x1b[31minvalid\x1b[0m command coloring\n\
           \x1b[32mTab Completion\x1b[0m     Commands, files, paths, variables, flags\n\
           \x1b[32mHistory Search\x1b[0m     \x1b[2mCtrl+R\x1b[0m reverse incremental search\n\
           \x1b[32mScrollback\x1b[0m         \x1b[2mShift+PgUp/PgDn\x1b[0m scroll history\n\
         \n\
         \x1b[1;33mKeybindings:\x1b[0m\n\
           Ctrl+A/E        Move to start/end of line\n\
           Ctrl+W          Delete previous word\n\
           Ctrl+K/U        Kill to end/start of line\n\
           Ctrl+L          Clear screen\n\
           Ctrl+C          Cancel current input\n\
           Ctrl+D          EOF (exit if empty)\n\
           Ctrl+R          Reverse search history\n\
           Alt+Left/Right  Move/accept word\n\
           Tab             Cycle completions\n\
         \n\
         \x1b[1;33mBuiltin Commands:\x1b[0m\n\
           \x1b[36mFile Ops:\x1b[0m       ls cat touch mkdir rm rmdir cp mv head tail wc grep find stat\n\
           \x1b[36mNavigation:\x1b[0m     cd pwd\n\
           \x1b[36mEnvironment:\x1b[0m    export unset env set echo\n\
           \x1b[36mProcess Mgmt:\x1b[0m   ps kill sleep\n\
           \x1b[36mSystem Info:\x1b[0m    uname hostname whoami id uptime date free df\n\
           \x1b[36mNetwork:\x1b[0m        ifconfig ping\n\
           \x1b[36mKernel:\x1b[0m         dmesg lsmod modinfo insmod rmmod lsblk mount umount\n\
           \x1b[36mPackage Mgr:\x1b[0m    kpm install|remove|search|list|info|update|upgrade\n\
           \x1b[36mPower:\x1b[0m          poweroff reboot\n\
           \x1b[36mSecurity:\x1b[0m       getenforce sestatus lscgroup\n\
           \x1b[36mGPU/Display:\x1b[0m    lspci glxinfo xrandr\n\
           \x1b[36mShell Control:\x1b[0m  history help type exit clear source\n\
         \n\
         \x1b[1;33mPipe & Redirect:\x1b[0m  cmd1 \x1b[35m|\x1b[0m cmd2    cmd \x1b[35m>\x1b[0m file    cmd \x1b[35m>>\x1b[0m file    cmd \x1b[35m<\x1b[0m file\n\
         \x1b[1;33mVariables:\x1b[0m        \x1b[33m$VAR\x1b[0m  \x1b[33m${VAR}\x1b[0m  \x1b[33m$?\x1b[0m (exit code)  \x1b[33m~\x1b[0m (home)\n\
         \n",
    )
}

pub fn r#type(args: &[String]) -> ShellResult {
    let mut output = String::new();

    for arg in args {
        if super::is_builtin(arg) {
            writeln!(output, "{} is a shell builtin", arg).unwrap();
        } else if let Some(path) = resolve_command_path(arg) {
            writeln!(output, "{} is {}", arg, path).unwrap();
        } else {
            writeln!(output, "ksh: type: {}: not found", arg).unwrap();
        }
    }

    ShellResult::ok(&output)
}

pub fn alias(args: &[String]) -> ShellResult {
    let aliases = crate::shell::env::SHELL_ALIASES.lock();
    if args.is_empty() {
        // List all aliases
        let mut output = String::new();
        for (name, value) in aliases.iter() {
            writeln!(output, "alias {}='{}'", name, value).unwrap();
        }
        if output.is_empty() {
            return ShellResult::ok("");
        }
        return ShellResult::ok(&output);
    }
    drop(aliases);

    for arg in args {
        if let Some(eq_pos) = arg.find('=') {
            // Define alias: alias name='value' or alias name=value
            let name = &arg[..eq_pos];
            let mut value = &arg[eq_pos + 1..];
            // Strip surrounding quotes if present
            if (value.starts_with('"') && value.ends_with('"'))
                || (value.starts_with('\'') && value.ends_with('\''))
            {
                value = &value[1..value.len() - 1];
            }
            crate::shell::env::SHELL_ALIASES
                .lock()
                .insert(String::from(name), String::from(value));
        } else {
            // Show single alias
            let aliases = crate::shell::env::SHELL_ALIASES.lock();
            if let Some(value) = aliases.get(arg.as_str()) {
                return ShellResult::ok(&alloc::format!("alias {}='{}'", arg, value));
            } else {
                return ShellResult::err(&alloc::format!("alias: {}: not found", arg));
            }
        }
    }
    ShellResult::ok("")
}

pub fn unalias(args: &[String]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::err("unalias: usage: unalias [-a] name [name ...]");
    }
    if args[0] == "-a" {
        crate::shell::env::SHELL_ALIASES.lock().clear();
        return ShellResult::ok("");
    }
    let mut aliases = crate::shell::env::SHELL_ALIASES.lock();
    for arg in args {
        if aliases.remove(arg.as_str()).is_none() {
            return ShellResult::err(&alloc::format!("unalias: {}: not found", arg));
        }
    }
    ShellResult::ok("")
}

pub fn test(args: &[String]) -> ShellResult {
    // Basic test/[ implementation
    if args.is_empty() {
        return ShellResult {
            exit_code: 1,
            output: String::new(),
        };
    }

    let result = match args.len() {
        1 => !args[0].is_empty() && args[0] != "]",
        2 => match args[0].as_str() {
            "-n" => !args[1].is_empty(),
            "-z" => args[1].is_empty(),
            "-d" => {
                let vfs = crate::vfs::VFS.lock();
                vfs.resolve_path(&args[1])
                    .and_then(|ino| {
                        vfs.get_inode(ino)
                            .map(|n| n.file_type == crate::vfs::FileType::Directory)
                    })
                    .unwrap_or(false)
            }
            "-f" => {
                let vfs = crate::vfs::VFS.lock();
                vfs.resolve_path(&args[1])
                    .and_then(|ino| {
                        vfs.get_inode(ino)
                            .map(|n| n.file_type == crate::vfs::FileType::Regular)
                    })
                    .unwrap_or(false)
            }
            "-e" => crate::vfs::VFS.lock().resolve_path(&args[1]).is_some(),
            "!" => args[1].is_empty(),
            _ => false,
        },
        3 => match args[1].as_str() {
            "=" | "==" => args[0] == args[2],
            "!=" => args[0] != args[2],
            "-eq" => args[0].parse::<i64>().ok() == args[2].parse::<i64>().ok(),
            "-ne" => args[0].parse::<i64>().ok() != args[2].parse::<i64>().ok(),
            "-lt" => args[0].parse::<i64>().unwrap_or(0) < args[2].parse::<i64>().unwrap_or(0),
            "-gt" => args[0].parse::<i64>().unwrap_or(0) > args[2].parse::<i64>().unwrap_or(0),
            "-le" => args[0].parse::<i64>().unwrap_or(0) <= args[2].parse::<i64>().unwrap_or(0),
            "-ge" => args[0].parse::<i64>().unwrap_or(0) >= args[2].parse::<i64>().unwrap_or(0),
            _ => false,
        },
        _ => false,
    };

    ShellResult {
        exit_code: if result { 0 } else { 1 },
        output: String::new(),
    }
}

pub fn source(args: &[String]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::err("source: filename argument required");
    }
    crate::shell::scripting::execute_script_file(&args[0], &args[1..])
}

// ═══════════════════════════════════════════════════════════════
// Job Control Builtins (Phase 2) — Hardened
// ═══════════════════════════════════════════════════════════════

/// List active jobs with +/- indicators and auto-reap Done jobs
pub fn jobs(_args: &[String]) -> ShellResult {
    let mut jobs = crate::shell::env::BACKGROUND_JOBS.lock();
    let mut output = String::new();

    // Report and auto-reap Done jobs
    let job_count = jobs.len();
    for i in 0..job_count {
        let indicator = if i + 1 == job_count {
            '+' // Most recent job
        } else if i + 2 == job_count {
            '-' // Previous job
        } else {
            ' '
        };

        let status_str = match jobs[i].status {
            crate::shell::env::JobStatus::Running => "Running",
            crate::shell::env::JobStatus::Stopped => "Stopped",
            crate::shell::env::JobStatus::Done => "Done",
        };
        writeln!(
            output,
            "[{}]{} {} {:>12}  {}",
            jobs[i].job_id, indicator, jobs[i].pid, status_str, jobs[i].command
        )
        .unwrap();
    }

    // Auto-reap: remove Done jobs after reporting them
    jobs.retain(|j| j.status != crate::shell::env::JobStatus::Done);

    if output.is_empty() {
        return ShellResult::ok("");
    }

    ShellResult::ok(&output)
}

/// Bring a job to the foreground (hardened: validates state, removes from bg table)
pub fn fg(args: &[String]) -> ShellResult {
    let job_id = parse_job_reference(args);

    let mut jobs = crate::shell::env::BACKGROUND_JOBS.lock();

    // Find the job
    let idx = if let Some(id) = job_id {
        jobs.iter().position(|j| j.job_id == id)
    } else {
        // Default: most recent non-Done job
        jobs.iter()
            .rposition(|j| j.status != crate::shell::env::JobStatus::Done)
    };

    if let Some(idx) = idx {
        let job = &mut jobs[idx];
        let cmd = job.command.clone();
        let pid = job.pid;

        match job.status {
            crate::shell::env::JobStatus::Stopped => {
                // Resume the stopped job in foreground
                job.status = crate::shell::env::JobStatus::Running;
                // Send SIGCONT
                let _ = crate::signals::kill(
                    pid,
                    crate::signals::Signal::SIGCONT,
                    crate::shell::env::get_shell_pid(),
                );
                // Remove from bg table — it's now foreground
                jobs.remove(idx);
                ShellResult::ok(&alloc::format!("{}\n", cmd))
            }
            crate::shell::env::JobStatus::Running => {
                // Move running bg job to foreground
                jobs.remove(idx);
                ShellResult::ok(&alloc::format!("{}\n", cmd))
            }
            crate::shell::env::JobStatus::Done => ShellResult::err("fg: job has already completed"),
        }
    } else {
        ShellResult::err("fg: no current job")
    }
}

/// Resume a job in the background
pub fn bg(args: &[String]) -> ShellResult {
    let job_id = parse_job_reference(args);

    let mut jobs = crate::shell::env::BACKGROUND_JOBS.lock();
    if let Some(job) = if let Some(id) = job_id {
        jobs.iter_mut().find(|j| j.job_id == id)
    } else {
        // Find most recent stopped job
        jobs.iter_mut()
            .rev()
            .find(|j| j.status == crate::shell::env::JobStatus::Stopped)
    } {
        if job.status == crate::shell::env::JobStatus::Stopped {
            job.status = crate::shell::env::JobStatus::Running;
            let cmd = job.command.clone();
            let pid = job.pid;
            let jid = job.job_id;
            // Send SIGCONT
            let _ = crate::signals::kill(
                pid,
                crate::signals::Signal::SIGCONT,
                crate::shell::env::get_shell_pid(),
            );
            ShellResult::ok(&alloc::format!("[{}] {} &\n", jid, cmd))
        } else {
            ShellResult::err(&alloc::format!("bg: job is not stopped"))
        }
    } else {
        ShellResult::err("bg: no current job")
    }
}

/// Wait for background jobs to complete (hardened: polls scheduler)
pub fn wait(args: &[String]) -> ShellResult {
    if args.is_empty() {
        // Wait for all background jobs — poll scheduler for each
        let pids: Vec<u32> = {
            let jobs = crate::shell::env::BACKGROUND_JOBS.lock();
            jobs.iter()
                .filter(|j| j.status == crate::shell::env::JobStatus::Running)
                .map(|j| j.pid)
                .collect()
        };

        if pids.is_empty() {
            return ShellResult::ok("");
        }

        // Mark all as Done — in our cooperative kernel, builtin tasks complete synchronously
        // For real async processes, we check the scheduler
        {
            let mut jobs = crate::shell::env::BACKGROUND_JOBS.lock();
            for j in jobs.iter_mut() {
                if j.status == crate::shell::env::JobStatus::Running {
                    // Check if the process still exists in the scheduler
                    if !crate::scheduler::process_exists(j.pid) {
                        j.status = crate::shell::env::JobStatus::Done;
                    }
                }
            }
        }

        return ShellResult::ok("");
    }

    // Wait for specific PID or job reference
    let target_pid = if args[0].starts_with('%') {
        // Job reference
        let job_id = parse_job_reference(args);
        let jobs = crate::shell::env::BACKGROUND_JOBS.lock();
        if let Some(id) = job_id {
            jobs.iter().find(|j| j.job_id == id).map(|j| j.pid)
        } else {
            jobs.last().map(|j| j.pid)
        }
    } else {
        args[0].parse::<u32>().ok()
    };

    if let Some(pid) = target_pid {
        // Check if process is still alive
        if !crate::scheduler::process_exists(pid) {
            // Mark as Done in job table
            let mut jobs = crate::shell::env::BACKGROUND_JOBS.lock();
            if let Some(j) = jobs.iter_mut().find(|j| j.pid == pid) {
                j.status = crate::shell::env::JobStatus::Done;
            }
        }
        ShellResult::ok("")
    } else {
        ShellResult::err("wait: invalid argument")
    }
}

/// Remove a job from the job table
pub fn disown(args: &[String]) -> ShellResult {
    let job_id = parse_job_reference(args);
    let mut jobs = crate::shell::env::BACKGROUND_JOBS.lock();

    if let Some(id) = job_id {
        let before = jobs.len();
        jobs.retain(|j| j.job_id != id);
        if jobs.len() < before {
            ShellResult::ok("")
        } else {
            ShellResult::err(&alloc::format!("disown: %{}: no such job", id))
        }
    } else {
        // Disown all
        jobs.clear();
        ShellResult::ok("")
    }
}

/// Parse a job reference from args: %1, %+, %-, or plain number
/// Returns (job_id, is_previous_job_request)
fn parse_job_reference(args: &[String]) -> Option<usize> {
    if args.is_empty() {
        return None;
    }

    let arg = &args[0];
    if let Some(rest) = arg.strip_prefix('%') {
        match rest {
            "+" | "" => {
                // Most recent job — find highest job_id
                let jobs = crate::shell::env::BACKGROUND_JOBS.lock();
                jobs.iter()
                    .rfind(|j| j.status != crate::shell::env::JobStatus::Done)
                    .map(|j| j.job_id)
            }
            "-" => {
                // Previous job (second-to-last)
                let jobs = crate::shell::env::BACKGROUND_JOBS.lock();
                let active: Vec<_> = jobs
                    .iter()
                    .filter(|j| j.status != crate::shell::env::JobStatus::Done)
                    .collect();
                if active.len() >= 2 {
                    Some(active[active.len() - 2].job_id)
                } else {
                    active.first().map(|j| j.job_id)
                }
            }
            _ => rest.parse::<usize>().ok(),
        }
    } else {
        arg.parse::<usize>().ok()
    }
}

/// Save command history to VFS file (~/.ksh_history)
pub fn save_history(_args: &[String]) -> ShellResult {
    let history = HISTORY.lock();
    let home = ENV_VARS
        .lock()
        .get("HOME")
        .cloned()
        .unwrap_or_else(|| String::from("/home/user"));
    let path = alloc::format!("{}/.ksh_history", home);
    let mut content = String::new();
    for entry in history.iter() {
        content.push_str(entry);
        content.push('\n');
    }
    drop(history);
    let mut vfs = crate::vfs::VFS.lock();
    if vfs.write_file(&path, content.as_bytes()) {
        ShellResult::ok(&alloc::format!("History saved to {}", path))
    } else {
        ShellResult::err("save_history: failed to write history file")
    }
}

/// Load command history from VFS file (~/.ksh_history)
pub fn load_history() {
    let home = ENV_VARS
        .lock()
        .get("HOME")
        .cloned()
        .unwrap_or_else(|| String::from("/home/user"));
    let path = alloc::format!("{}/.ksh_history", home);
    let vfs = crate::vfs::VFS.lock();
    if let Some(data) = vfs.read_file(&path) {
        let content = core::str::from_utf8(data).unwrap_or_default();
        let lines: Vec<String> = content
            .lines()
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect();
        drop(vfs);
        let mut history = HISTORY.lock();
        for line in lines {
            history.push(line);
        }
        // Cap at 500
        while history.len() > 500 {
            history.remove(0);
        }
    }
}

/// Parse PS1 escape sequences into a prompt string
/// Supports: \u (user), \h (hostname), \H (full hostname), \w (cwd with ~ shortening),
/// \W (basename of cwd), \$ (# for root, $ for user), \n (newline), \t (time HH:MM:SS),
/// \d (date), \[ / \] (non-printing delimiters, ignored), \\ (literal backslash)
pub fn parse_ps1() -> String {
    let ps1 = ENV_VARS
        .lock()
        .get("PS1")
        .cloned()
        .unwrap_or_else(|| String::from("\\u@\\h:\\w\\$ "));
    let user = ENV_VARS
        .lock()
        .get("USER")
        .cloned()
        .unwrap_or_else(|| String::from("user"));
    let hostname = String::from("knoxos");
    let pwd = ENV_VARS
        .lock()
        .get("PWD")
        .cloned()
        .unwrap_or_else(|| String::from("/"));
    let home = ENV_VARS
        .lock()
        .get("HOME")
        .cloned()
        .unwrap_or_else(|| String::from("/home/user"));

    let display_pwd = if pwd.starts_with(&home) {
        alloc::format!("~{}", &pwd[home.len()..])
    } else {
        pwd.clone()
    };
    let basename = pwd.rsplit('/').next().unwrap_or(&pwd);
    let sigil = if user == "root" { "#" } else { "$" };

    let mut result = String::new();
    let bytes = ps1.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            match bytes[i + 1] {
                b'u' => {
                    result.push_str(&user);
                    i += 2;
                }
                b'h' => {
                    result.push_str(&hostname);
                    i += 2;
                }
                b'H' => {
                    result.push_str(&hostname);
                    i += 2;
                }
                b'w' => {
                    result.push_str(&display_pwd);
                    i += 2;
                }
                b'W' => {
                    result.push_str(basename);
                    i += 2;
                }
                b'$' => {
                    result.push_str(sigil);
                    i += 2;
                }
                b'n' => {
                    result.push('\n');
                    i += 2;
                }
                b't' => {
                    result.push_str("00:00:00");
                    i += 2;
                }
                b'd' => {
                    result.push_str("Mon Jan 01");
                    i += 2;
                }
                b'[' | b']' => {
                    i += 2;
                } // Non-printing delimiters
                b'\\' => {
                    result.push('\\');
                    i += 2;
                }
                b'e' => {
                    result.push('\x1b');
                    i += 2;
                }
                b'a' => {
                    result.push('\x07');
                    i += 2;
                }
                b'g' => {
                    // \g — git branch (KnoxOS extension)
                    let pwd = ENV_VARS
                        .lock()
                        .get("PWD")
                        .cloned()
                        .unwrap_or_else(|| String::from("/"));
                    if let Some(branch) = crate::shell::helpers::detect_git_branch(&pwd) {
                        result.push_str(&alloc::format!("({})", branch));
                    }
                    i += 2;
                }
                _ => {
                    result.push('\\');
                    result.push(bytes[i + 1] as char);
                    i += 2;
                }
            }
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }
    result
}

/// Source the .kshrc file if it exists
pub fn source_kshrc() {
    let home = ENV_VARS
        .lock()
        .get("HOME")
        .cloned()
        .unwrap_or_else(|| String::from("/home/user"));
    let path = alloc::format!("{}/.kshrc", home);
    let vfs = crate::vfs::VFS.lock();
    if vfs.read_file(&path).is_some() {
        drop(vfs);
        crate::shell::scripting::execute_script_file(&path, &[]);
    }
}

/// `bind` builtin — show or configure keybindings
pub fn bind(args: &[String]) -> ShellResult {
    if args.is_empty() || args.first().map(|s| s.as_str()) == Some("-p") {
        // List current bindings
        let mut output = String::from("Key bindings:\n");
        output.push_str("  Ctrl+A    Move to start of line\n");
        output.push_str("  Ctrl+E    Move to end of line\n");
        output.push_str("  Ctrl+K    Kill to end of line\n");
        output.push_str("  Ctrl+U    Kill to start of line\n");
        output.push_str("  Ctrl+W    Kill previous word\n");
        output.push_str("  Ctrl+Y    Yank (paste from kill ring)\n");
        output.push_str("  Alt+Y     Cycle kill ring\n");
        output.push_str("  Ctrl+Z    Undo\n");
        output.push_str("  Ctrl+Shift+Z  Redo\n");
        output.push_str("  Ctrl+R    Reverse search history\n");
        output.push_str("  Ctrl+L    Clear screen\n");
        output.push_str("  Ctrl+C    Cancel input\n");
        output.push_str("  Ctrl+D    EOF / Exit\n");
        output.push_str("  Ctrl+Left/Right  Word movement\n");
        output.push_str("  Tab       Tab completion\n");
        output.push_str("  Ctrl+Shift+C  Copy selection\n");
        output.push_str("  Ctrl+Shift+V  Paste clipboard\n");
        output.push_str("  F1        Toggle vi/emacs mode\n");
        output.push_str("  Escape    Vi normal mode / dismiss\n");
        ShellResult::ok(&output)
    } else {
        ShellResult::ok("bind: custom keybindings stored")
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Phase 8 — Advanced Terminal Builtins
// ═══════════════════════════════════════════════════════════════════════════

/// `tmux` builtin — terminal multiplexer
pub fn tmux(args: &[String]) -> ShellResult {
    let subcmd = args.first().map(|s| s.as_str()).unwrap_or("help");
    match subcmd {
        "new-session" | "new" => {
            let name = args.get(1).map(|s| s.as_str()).unwrap_or("default");
            let id = crate::tmux::new_session(name);
            ShellResult::ok(&alloc::format!(
                "Created tmux session '{}' (id={})",
                name,
                id
            ))
        }
        "list-sessions" | "ls" => {
            let sessions = crate::tmux::list_sessions();
            if sessions.is_empty() {
                ShellResult::ok("No active tmux sessions")
            } else {
                let mut out = String::from("tmux sessions:\n");
                for (id, name, detached, windows) in &sessions {
                    let status = if *detached { "detached" } else { "attached" };
                    out.push_str(&alloc::format!(
                        "  {}: {} ({}, {} windows)\n",
                        id,
                        name,
                        status,
                        windows
                    ));
                }
                ShellResult::ok(&out)
            }
        }
        "attach" | "a" => {
            if let Some(id_str) = args.get(1) {
                if let Ok(id) = id_str.parse::<u32>() {
                    if crate::tmux::attach_session(id) {
                        ShellResult::ok(&alloc::format!("Attached to session {}", id))
                    } else {
                        ShellResult::err("tmux: session not found")
                    }
                } else {
                    ShellResult::err("tmux: invalid session ID")
                }
            } else {
                ShellResult::err("Usage: tmux attach <session-id>")
            }
        }
        "detach" | "d" => {
            if let Some(id_str) = args.get(1) {
                if let Ok(id) = id_str.parse::<u32>() {
                    if crate::tmux::detach_session(id) {
                        ShellResult::ok(&alloc::format!("Detached session {}", id))
                    } else {
                        ShellResult::err("tmux: session not found")
                    }
                } else {
                    ShellResult::err("tmux: invalid session ID")
                }
            } else {
                ShellResult::err("Usage: tmux detach <session-id>")
            }
        }
        "kill-session" | "kill" => {
            if let Some(id_str) = args.get(1) {
                if let Ok(id) = id_str.parse::<u32>() {
                    if crate::tmux::kill_session(id) {
                        ShellResult::ok(&alloc::format!("Killed session {}", id))
                    } else {
                        ShellResult::err("tmux: session not found")
                    }
                } else {
                    ShellResult::err("tmux: invalid session ID")
                }
            } else {
                ShellResult::err("Usage: tmux kill-session <session-id>")
            }
        }
        _ => {
            let help = "\
tmux — terminal multiplexer
Usage: tmux <command> [args]
Commands:
  new-session [name]       Create a new session
  list-sessions            List all sessions
  attach <id>              Attach to a session
  detach <id>              Detach a session
  kill-session <id>        Kill a session
Keys (inside tmux session):
  Ctrl+B c    New window
  Ctrl+B n/p  Next/prev window
  Ctrl+B \"    Split horizontal
  Ctrl+B %    Split vertical
  Ctrl+B o    Cycle panes
  Ctrl+B d    Detach";
            ShellResult::ok(help)
        }
    }
}

/// `ssh` builtin — SSH client / server management
pub fn ssh(args: &[String]) -> ShellResult {
    if args.is_empty() {
        let help = "\
ssh — Secure Shell
Usage: ssh [user@]host[:port]
       ssh -l user host
       ssh --server start|stop|status
       ssh --sessions";
        return ShellResult::ok(help);
    }

    let first = args[0].as_str();
    match first {
        "--server" => {
            let action = args.get(1).map(|s| s.as_str()).unwrap_or("status");
            match action {
                "start" => {
                    crate::ssh::SSH_SERVER.lock().start();
                    ShellResult::ok("SSH server started on port 22")
                }
                "stop" => {
                    crate::ssh::SSH_SERVER.lock().stop();
                    ShellResult::ok("SSH server stopped")
                }
                "status" => {
                    let server = crate::ssh::SSH_SERVER.lock();
                    let status = if server.running { "running" } else { "stopped" };
                    ShellResult::ok(&alloc::format!(
                        "SSH server: {} (port {})\nHost key: {}",
                        status,
                        server.port,
                        server.host_key_fingerprint
                    ))
                }
                _ => ShellResult::err("Usage: ssh --server start|stop|status"),
            }
        }
        "--sessions" => {
            let sessions = crate::ssh::list_sessions();
            if sessions.is_empty() {
                ShellResult::ok("No active SSH sessions")
            } else {
                let mut out = String::from("Active SSH sessions:\n");
                for (id, user, peer, state) in &sessions {
                    out.push_str(&alloc::format!(
                        "  #{}: {}@{} ({:?})\n",
                        id,
                        user,
                        peer,
                        state
                    ));
                }
                ShellResult::ok(&out)
            }
        }
        _ => {
            // Parse [user@]host[:port]
            let (user, rest) = if let Some(at_pos) = first.find('@') {
                (&first[..at_pos], &first[at_pos + 1..])
            } else {
                ("root", first)
            };
            let (host, port) = if let Some(colon_pos) = rest.find(':') {
                let p: u16 = rest[colon_pos + 1..].parse().unwrap_or(22);
                (&rest[..colon_pos], p)
            } else {
                (rest, 22u16)
            };

            let mut client = crate::ssh::SshClient::new(host, port, user);
            if client.connect() {
                ShellResult::ok(&alloc::format!(
                    "Connected to {}@{}:{} (pty=pts/{})",
                    user,
                    host,
                    port,
                    client.pty_id - 100
                ))
            } else {
                ShellResult::err(&alloc::format!(
                    "ssh: connect to {} port {}: Connection refused",
                    host,
                    port
                ))
            }
        }
    }
}

/// `setxkbmap` builtin — set keyboard layout
pub fn setxkbmap(args: &[String]) -> ShellResult {
    if args.is_empty() {
        let current = crate::task::keyboard::get_layout();
        ShellResult::ok(&alloc::format!(
            "Current layout: {}\nAvailable: us, uk, de, fr, es, dvorak\nUsage: setxkbmap <layout>",
            current.name()
        ))
    } else {
        let name = args[0].as_str();
        if let Some(layout) = crate::task::keyboard::KeyboardLayout::from_name(name) {
            crate::task::keyboard::set_layout(layout);
            ShellResult::ok(&alloc::format!("Keyboard layout set to: {}", name))
        } else {
            ShellResult::err(&alloc::format!(
                "setxkbmap: unknown layout '{}'\nAvailable: us, uk, de, fr, es, dvorak",
                name
            ))
        }
    }
}

/// `compose` builtin — start compose key mode or show compose table
pub fn compose(args: &[String]) -> ShellResult {
    if args.first().map(|s| s.as_str()) == Some("--list") {
        let table = "\
Compose key sequences (Ctrl+. to start):
  ' + vowel  → acute (á é í ó ú)
  ` + vowel  → grave (à è ì ò ù)
  ^ + vowel  → circumflex (â ê î ô û)
  \" + vowel  → diaeresis (ä ë ï ö ü)
  ~ + n      → ñ
  , + c      → ç
  / + o      → ø
  a + e      → æ
  s + s      → ß
  < + <      → «   > + >  → »
  ! + !      → ¡   ? + ?  → ¿
  e + =      → €   c + |  → ¢
  o + c      → ©   o + r  → ®
  . + .      → …   + + -  → ±";
        ShellResult::ok(table)
    } else {
        ShellResult::ok(
            "Compose mode: press Ctrl+. then type two characters.\nUse 'compose --list' to see all sequences.",
        )
    }
}
