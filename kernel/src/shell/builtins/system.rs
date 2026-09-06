/// System info builtins — uname, hostname, whoami, id, uptime, date, free, df, ps, kill, sleep, seq
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

use crate::serial_println;
use crate::shell::env::ENV_VARS;
use crate::shell::types::ShellResult;

pub fn uname(args: &[String]) -> ShellResult {
    let mut show_all = false;
    let mut show_sysname = false;
    let mut show_nodename = false;
    let mut show_release = false;
    let mut show_version = false;
    let mut show_machine = false;

    if args.is_empty() {
        show_sysname = true;
    }

    for arg in args {
        match arg.as_str() {
            "-a" => show_all = true,
            "-s" => show_sysname = true,
            "-n" => show_nodename = true,
            "-r" => show_release = true,
            "-v" => show_version = true,
            "-m" => show_machine = true,
            _ => {}
        }
    }

    let mut parts = Vec::new();
    if show_all || show_sysname {
        parts.push("KnoxOS");
    }
    if show_all || show_nodename {
        parts.push("knoxos");
    }
    if show_all || show_release {
        parts.push("0.1.0-knoxos");
    }
    if show_all || show_version {
        parts.push("#1 SMP PREEMPT_DYNAMIC");
    }
    if show_all || show_machine {
        parts.push("x86_64");
    }

    ShellResult::ok(&alloc::format!("{}\n", parts.join(" ")))
}

pub fn hostname() -> ShellResult {
    ShellResult::ok("knoxos\n")
}

pub fn whoami() -> ShellResult {
    let user = ENV_VARS
        .lock()
        .get("USER")
        .cloned()
        .unwrap_or_else(|| String::from("user"));
    ShellResult::ok(&alloc::format!("{}\n", user))
}

pub fn id() -> ShellResult {
    let pid = crate::scheduler::current_pid().unwrap_or(0);
    let table = crate::process::PROCESS_TABLE.lock();
    let (uid, gid) = table
        .get_process(pid)
        .map(|p| (p.uid, p.gid))
        .unwrap_or((1000, 1000));
    ShellResult::ok(&alloc::format!(
        "uid={}(user) gid={}(user) groups={}(user)\n",
        uid,
        gid,
        gid
    ))
}

pub fn uptime() -> ShellResult {
    let ticks = crate::interrupts::get_ticks();
    let seconds = ticks / 18; // PIT ~18.2 Hz
    let minutes = seconds / 60;
    let hours = minutes / 60;
    let procs = crate::process::PROCESS_TABLE.lock().count();
    ShellResult::ok(&alloc::format!(
        " up {:>2}:{:02}:{:02}, {} users, load average: 0.00, 0.00, 0.00\n",
        hours,
        minutes % 60,
        seconds % 60,
        procs
    ))
}

pub fn date() -> ShellResult {
    let ticks = crate::interrupts::get_ticks();
    let seconds = ticks / 18;
    let hours = (seconds / 3600) % 24;
    let minutes = (seconds / 60) % 60;
    let secs = seconds % 60;
    ShellResult::ok(&alloc::format!(
        "Thu Jan  1 {:02}:{:02}:{:02} UTC 1970\n",
        hours,
        minutes,
        secs
    ))
}

pub fn free() -> ShellResult {
    let total = crate::allocator::HEAP_SIZE;
    let total_kb = total / 1024;
    ShellResult::ok(&alloc::format!(
        "              total        used        free      shared  buff/cache   available\n\
         Mem:    {:>10}  {:>10}  {:>10}           0           0  {:>10}\n\
         Swap:            0           0           0\n",
        total_kb,
        total_kb / 2,
        total_kb / 2,
        total_kb / 2
    ))
}

pub fn df() -> ShellResult {
    let total_kb = crate::allocator::HEAP_SIZE / 1024;
    ShellResult::ok(&alloc::format!(
        "Filesystem     1K-blocks   Used Available Use%% Mounted on\n\
         knoxosfs          {:>9} {:>6}    {:>6}  50%% /\n\
         proc                   0      0         0    0%% /proc\n\
         sysfs                  0      0         0    0%% /sys\n\
         tmpfs          {:>9}      0    {:>6}   0%% /tmp\n",
        total_kb,
        total_kb / 2,
        total_kb / 2,
        total_kb / 4,
        total_kb / 4
    ))
}

pub fn ps(args: &[String]) -> ShellResult {
    let show_all = args
        .iter()
        .any(|a| a == "-e" || a == "-A" || a == "aux" || a == "-aux");

    let mut output = String::new();
    writeln!(output, "  PID TTY      STAT   TIME COMMAND").unwrap();

    let table = crate::process::PROCESS_TABLE.lock();
    for proc in table.list_processes() {
        if proc.state == crate::process::ProcessState::Zombie {
            continue;
        }
        if !show_all && proc.uid != 1000 {
            continue;
        }
        let state_char = match proc.state {
            crate::process::ProcessState::Running => 'R',
            crate::process::ProcessState::Ready => 'S',
            crate::process::ProcessState::Sleeping => 'S',
            crate::process::ProcessState::Stopped => 'T',
            crate::process::ProcessState::Zombie => 'Z',
        };
        writeln!(
            output,
            "{:>5} tty0     {}      0:00 {}",
            proc.pid, state_char, proc.name
        )
        .unwrap();
    }

    ShellResult::ok(&output)
}

pub fn kill(args: &[String]) -> ShellResult {
    let mut signal = 15u32; // SIGTERM default
    let mut pids = Vec::new();

    for arg in args {
        if let Some(stripped) = arg.strip_prefix('-') {
            if let Ok(sig) = stripped.parse::<u32>() {
                signal = sig;
            } else {
                match arg.as_str() {
                    "-KILL" | "-SIGKILL" => signal = 9,
                    "-INT" | "-SIGINT" => signal = 2,
                    "-TERM" | "-SIGTERM" => signal = 15,
                    "-STOP" | "-SIGSTOP" => signal = 19,
                    "-CONT" | "-SIGCONT" => signal = 18,
                    "-HUP" | "-SIGHUP" => signal = 1,
                    _ => {}
                }
            }
        } else if let Some(job_ref) = arg.strip_prefix('%') {
            // %N job reference — resolve to PID from job table
            if let Some(pid) = resolve_job_to_pid(job_ref) {
                pids.push(pid);
            } else {
                return ShellResult::err(&alloc::format!("kill: %{}: no such job", job_ref));
            }
        } else if let Ok(pid) = arg.parse::<u32>() {
            pids.push(pid);
        }
    }

    for pid in pids {
        if let Some(sig) = crate::signals::Signal::from_number(signal) {
            match crate::signals::kill(pid, sig, 0) {
                Ok(_) => {}
                Err(_) => {
                    return ShellResult::err(&alloc::format!("kill: ({}) - No such process", pid));
                }
            }
        }
    }

    ShellResult::ok("")
}

/// Resolve a job reference (from %N) to a PID
fn resolve_job_to_pid(job_ref: &str) -> Option<u32> {
    let jobs = crate::shell::env::BACKGROUND_JOBS.lock();
    if job_ref.is_empty() || job_ref == "%" || job_ref == "+" {
        // %% or %+ means most recent job
        jobs.last().map(|j| j.pid)
    } else if job_ref == "-" {
        // %- means previous job
        if jobs.len() >= 2 {
            Some(jobs[jobs.len() - 2].pid)
        } else {
            None
        }
    } else if let Ok(id) = job_ref.parse::<usize>() {
        // %N — job number
        jobs.iter().find(|j| j.job_id == id).map(|j| j.pid)
    } else {
        // %string — match command prefix
        jobs.iter()
            .find(|j| j.command.starts_with(job_ref))
            .map(|j| j.pid)
    }
}

pub fn sleep(args: &[String]) -> ShellResult {
    if let Some(secs) = args.first().and_then(|s| s.parse::<u64>().ok()) {
        serial_println!("[KnoxOS] sleep {} (stub - no real blocking)", secs);
    }
    ShellResult::ok("")
}

pub fn seq(args: &[String]) -> ShellResult {
    let (start, end) = match args.len() {
        1 => (1i64, args[0].parse::<i64>().unwrap_or(1)),
        2 => (
            args[0].parse::<i64>().unwrap_or(1),
            args[1].parse::<i64>().unwrap_or(1),
        ),
        _ => return ShellResult::err("seq: usage: seq [START] END"),
    };

    let mut output = String::new();
    for i in start..=end {
        writeln!(output, "{}", i).unwrap();
    }
    ShellResult::ok(&output)
}
