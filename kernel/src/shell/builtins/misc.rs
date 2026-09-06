/// Miscellaneous utility builtins — man, neofetch, watch, timeout, nohup,
/// env manipulation, which_full, lsof, strace-like, dstat, vmstat, iostat,
/// w, who, last, groups, logname, hostnamectl, timedatectl, locale
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt::Write;

use crate::shell::env::ENV_VARS;
use crate::shell::types::ShellResult;

// ── man ─────────────────────────────────────────────────────────
pub fn man(args: &[String]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::err("What manual page do you want?\nUsage: man <command>");
    }
    let cmd = &args[args.len() - 1];
    let mut output = String::new();
    writeln!(
        output,
        "{}(1)                    KnoxOS Manual                    {}(1)",
        cmd, cmd
    )
    .unwrap();
    writeln!(output).unwrap();
    writeln!(output, "NAME").unwrap();
    writeln!(output, "       {} - {}", cmd, get_cmd_description(cmd)).unwrap();
    writeln!(output).unwrap();
    writeln!(output, "SYNOPSIS").unwrap();
    writeln!(output, "       {} [OPTIONS] [ARGS...]", cmd).unwrap();
    writeln!(output).unwrap();
    writeln!(output, "DESCRIPTION").unwrap();
    writeln!(output, "       {}", get_cmd_long_description(cmd)).unwrap();
    writeln!(output).unwrap();
    writeln!(
        output,
        "KnoxOS                        2026                       {}(1)",
        cmd
    )
    .unwrap();
    ShellResult::ok(&output)
}

fn get_cmd_description(cmd: &str) -> &'static str {
    match cmd {
        "ls" => "list directory contents",
        "cat" => "concatenate files and print on the standard output",
        "cd" => "change the working directory",
        "cp" => "copy files and directories",
        "mv" => "move (rename) files",
        "rm" => "remove files or directories",
        "mkdir" => "make directories",
        "rmdir" => "remove empty directories",
        "pwd" => "print name of current/working directory",
        "echo" => "display a line of text",
        "grep" => "print lines that match patterns",
        "find" => "search for files in a directory hierarchy",
        "head" => "output the first part of files",
        "tail" => "output the last part of files",
        "wc" => "print newline, word, and byte counts for each file",
        "sort" => "sort lines of text files",
        "uniq" => "report or omit repeated lines",
        "cut" => "remove sections from each line of files",
        "tr" => "translate or delete characters",
        "sed" => "stream editor for filtering and transforming text",
        "awk" => "pattern scanning and processing language",
        "diff" => "compare files line by line",
        "tree" => "list contents of directories in a tree-like format",
        "touch" => "change file timestamps / create empty file",
        "stat" => "display file or file system status",
        "du" => "estimate file space usage",
        "df" => "report file system disk space usage",
        "ps" => "report a snapshot of the current processes",
        "kill" => "send a signal to a process",
        "uname" => "print system information",
        "chmod" => "change file mode bits",
        "chown" => "change file owner and group",
        "ln" => "make links between files",
        "tar" => "archive utility",
        "dd" => "convert and copy a file",
        "mount" => "mount a filesystem",
        "umount" => "unmount a filesystem",
        "ping" => "send ICMP ECHO_REQUEST to network hosts",
        "ifconfig" => "configure a network interface",
        _ => "user command",
    }
}

fn get_cmd_long_description(cmd: &str) -> &'static str {
    match cmd {
        "ls" => {
            "List information about the FILEs (the current directory by default).\n       Sort entries alphabetically. Supports -a (all) and -l (long format)."
        }
        "tree" => {
            "List contents of directories in a tree-like format. Supports -a (all),\n       -d (directories only), and -L <level> (max display depth)."
        }
        "grep" => {
            "Search for PATTERN in each FILE. Supports -i (case-insensitive),\n       -v (invert match), -c (count), and -n (line numbers)."
        }
        "find" => {
            "Search for files in a directory hierarchy. Supports -name <pattern>\n       and -type d/f for filtering."
        }
        "sed" => "Stream editor. Supports basic s/pattern/replacement/[g] substitution.",
        "awk" => {
            "Pattern scanning. Supports basic '{print $N}' field extraction\n       and -F field separator."
        }
        _ => {
            "Standard Unix/Linux command implemented in KnoxOS kernel shell (ksh).\n       Refer to standard POSIX/Linux documentation for full details."
        }
    }
}

// ── neofetch ────────────────────────────────────────────────────
pub fn neofetch(_args: &[String]) -> ShellResult {
    let mut output = String::new();
    let hostname = ENV_VARS
        .lock()
        .get("HOSTNAME")
        .cloned()
        .unwrap_or_else(|| String::from("knoxos"));
    let user = ENV_VARS
        .lock()
        .get("USER")
        .cloned()
        .unwrap_or_else(|| String::from("root"));

    writeln!(
        output,
        "\x1b[38;2;86;156;214m        ██╗  ██╗\x1b[0m    {}@{}",
        user, hostname
    )
    .unwrap();
    writeln!(
        output,
        "\x1b[38;2;86;156;214m        ██║ ██╔╝\x1b[0m    ──────────────────"
    )
    .unwrap();
    writeln!(
        output,
        "\x1b[38;2;86;156;214m        █████╔╝ \x1b[0m    \x1b[1mOS:\x1b[0m KnoxOS x86_64"
    )
    .unwrap();
    writeln!(
        output,
        "\x1b[38;2;86;156;214m        ██╔═██╗ \x1b[0m    \x1b[1mKernel:\x1b[0m 0.1.0-knoxos"
    )
    .unwrap();
    writeln!(
        output,
        "\x1b[38;2;86;156;214m        ██║  ██╗\x1b[0m    \x1b[1mShell:\x1b[0m ksh 1.0"
    )
    .unwrap();
    writeln!(
        output,
        "\x1b[38;2;86;156;214m        ╚═╝  ╚═╝\x1b[0m    \x1b[1mTerminal:\x1b[0m /dev/tty0"
    )
    .unwrap();
    writeln!(output, "                    \x1b[1mCPU:\x1b[0m x86_64").unwrap();
    writeln!(
        output,
        "                    \x1b[1mMemory:\x1b[0m 256MiB / 1024MiB"
    )
    .unwrap();
    writeln!(output).unwrap();
    // Color blocks
    write!(output, "                    ").unwrap();
    for i in 0..8 {
        write!(output, "\x1b[4{}m   \x1b[0m", i).unwrap();
    }
    writeln!(output).unwrap();
    write!(output, "                    ").unwrap();
    for i in 0..8 {
        write!(output, "\x1b[10{}m   \x1b[0m", i).unwrap();
    }
    writeln!(output).unwrap();

    ShellResult::ok(&output)
}

// ── watch ───────────────────────────────────────────────────────
pub fn watch(args: &[String]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::err("watch: missing command\nUsage: watch [-n <seconds>] <command>");
    }
    // In kernel shell, watch can only run once (no loop)
    let cmd: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();
    ShellResult::ok(&alloc::format!(
        "watch: would repeatedly run: {}",
        cmd.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(" ")
    ))
}

// ── timeout ─────────────────────────────────────────────────────
pub fn timeout(args: &[String]) -> ShellResult {
    if args.len() < 2 {
        return ShellResult::err("timeout: missing operand\nUsage: timeout <duration> <command>");
    }
    // Just run the command (no actual timeout in kernel context)
    let cmd_args: Vec<String> = args[1..].to_vec();
    ShellResult::ok(&alloc::format!(
        "timeout: would run '{}' with {}s timeout",
        cmd_args.join(" "),
        args[0]
    ))
}

// ── nohup ───────────────────────────────────────────────────────
pub fn nohup(args: &[String]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::err("nohup: missing operand");
    }
    ShellResult::ok(&alloc::format!("nohup: appending output to 'nohup.out'"))
}

// ── w / who / last ──────────────────────────────────────────────
pub fn w(_args: &[String]) -> ShellResult {
    let user = ENV_VARS
        .lock()
        .get("USER")
        .cloned()
        .unwrap_or_else(|| String::from("root"));
    let mut output = String::new();
    writeln!(
        output,
        " 00:00:00 up 0 min,  1 user,  load average: 0.00, 0.00, 0.00"
    )
    .unwrap();
    writeln!(
        output,
        "USER     TTY      FROM             LOGIN@   IDLE   JCPU   PCPU WHAT"
    )
    .unwrap();
    writeln!(
        output,
        "{:<8} tty0     :0               00:00    0.00s  0.00s  0.00s ksh",
        user
    )
    .unwrap();
    ShellResult::ok(&output)
}

pub fn who(_args: &[String]) -> ShellResult {
    let user = ENV_VARS
        .lock()
        .get("USER")
        .cloned()
        .unwrap_or_else(|| String::from("root"));
    ShellResult::ok(&alloc::format!("{:<8} tty0         2026-01-01 00:00", user))
}

pub fn last(_args: &[String]) -> ShellResult {
    let user = ENV_VARS
        .lock()
        .get("USER")
        .cloned()
        .unwrap_or_else(|| String::from("root"));
    let mut output = String::new();
    writeln!(
        output,
        "{:<8} tty0         :0               Thu Jan  1 00:00   still logged in",
        user
    )
    .unwrap();
    writeln!(
        output,
        "reboot   system boot  0.1.0-knoxos     Thu Jan  1 00:00   still running"
    )
    .unwrap();
    writeln!(output).unwrap();
    writeln!(output, "wtmp begins Thu Jan  1 00:00:00 2026").unwrap();
    ShellResult::ok(&output)
}

// ── groups ──────────────────────────────────────────────────────
pub fn groups(_args: &[String]) -> ShellResult {
    ShellResult::ok("root wheel sudo")
}

// ── logname ─────────────────────────────────────────────────────
pub fn logname(_args: &[String]) -> ShellResult {
    let user = ENV_VARS
        .lock()
        .get("USER")
        .cloned()
        .unwrap_or_else(|| String::from("root"));
    ShellResult::ok(&user)
}

// ── hostnamectl ─────────────────────────────────────────────────
pub fn hostnamectl(_args: &[String]) -> ShellResult {
    let hostname = ENV_VARS
        .lock()
        .get("HOSTNAME")
        .cloned()
        .unwrap_or_else(|| String::from("knoxos"));
    let mut output = String::new();
    writeln!(output, "   Static hostname: {}", hostname).unwrap();
    writeln!(output, "         Icon name: computer").unwrap();
    writeln!(output, "           Chassis: desktop").unwrap();
    writeln!(output, "  Operating System: KnoxOS").unwrap();
    writeln!(output, "            Kernel: KnoxOS 0.1.0").unwrap();
    writeln!(output, "      Architecture: x86-64").unwrap();
    ShellResult::ok(&output)
}

// ── timedatectl ─────────────────────────────────────────────────
pub fn timedatectl(_args: &[String]) -> ShellResult {
    let mut output = String::new();
    writeln!(
        output,
        "               Local time: Thu 2026-01-01 00:00:00 UTC"
    )
    .unwrap();
    writeln!(
        output,
        "           Universal time: Thu 2026-01-01 00:00:00 UTC"
    )
    .unwrap();
    writeln!(output, "                 RTC time: Thu 2026-01-01 00:00:00").unwrap();
    writeln!(output, "                Time zone: UTC (UTC, +0000)").unwrap();
    writeln!(output, "System clock synchronized: yes").unwrap();
    writeln!(output, "              NTP service: active").unwrap();
    writeln!(output, "          RTC in local TZ: no").unwrap();
    ShellResult::ok(&output)
}

// ── locale ──────────────────────────────────────────────────────
pub fn locale(_args: &[String]) -> ShellResult {
    let mut output = String::new();
    writeln!(output, "LANG=en_US.UTF-8").unwrap();
    writeln!(output, "LC_CTYPE=\"en_US.UTF-8\"").unwrap();
    writeln!(output, "LC_NUMERIC=\"en_US.UTF-8\"").unwrap();
    writeln!(output, "LC_TIME=\"en_US.UTF-8\"").unwrap();
    writeln!(output, "LC_COLLATE=\"en_US.UTF-8\"").unwrap();
    writeln!(output, "LC_MONETARY=\"en_US.UTF-8\"").unwrap();
    writeln!(output, "LC_MESSAGES=\"en_US.UTF-8\"").unwrap();
    writeln!(output, "LC_ALL=").unwrap();
    ShellResult::ok(&output)
}

// ── lsof ────────────────────────────────────────────────────────
pub fn lsof(_args: &[String]) -> ShellResult {
    let mut output = String::new();
    writeln!(
        output,
        "COMMAND  PID  USER   FD   TYPE DEVICE SIZE/OFF NODE NAME"
    )
    .unwrap();
    writeln!(
        output,
        "ksh        1  root  cwd    DIR    0,1     4096    2 /"
    )
    .unwrap();
    writeln!(
        output,
        "ksh        1  root  rtd    DIR    0,1     4096    2 /"
    )
    .unwrap();
    writeln!(
        output,
        "ksh        1  root    0u   CHR    5,0      0t0    5 /dev/tty0"
    )
    .unwrap();
    writeln!(
        output,
        "ksh        1  root    1u   CHR    5,0      0t0    5 /dev/tty0"
    )
    .unwrap();
    writeln!(
        output,
        "ksh        1  root    2u   CHR    5,0      0t0    5 /dev/tty0"
    )
    .unwrap();
    ShellResult::ok(&output)
}

// ── vmstat ──────────────────────────────────────────────────────
pub fn vmstat(_args: &[String]) -> ShellResult {
    let mut output = String::new();
    writeln!(
        output,
        "procs -----------memory---------- ---swap-- -----io---- -system-- ------cpu-----"
    )
    .unwrap();
    writeln!(
        output,
        " r  b   swpd   free   buff  cache   si   so    bi    bo   in   cs us sy id wa st"
    )
    .unwrap();
    writeln!(
        output,
        " 1  0      0 524288  32768 131072    0    0     0     0    1    1  0  0 100  0  0"
    )
    .unwrap();
    ShellResult::ok(&output)
}

// ── iostat ──────────────────────────────────────────────────────
pub fn iostat(_args: &[String]) -> ShellResult {
    let mut output = String::new();
    writeln!(
        output,
        "KnoxOS 0.1.0 (knoxos)     01/01/2026     _x86_64_    (1 CPU)"
    )
    .unwrap();
    writeln!(output).unwrap();
    writeln!(
        output,
        "avg-cpu:  %user   %nice %system %iowait  %steal   %idle"
    )
    .unwrap();
    writeln!(
        output,
        "           0.00    0.00    0.00    0.00    0.00  100.00"
    )
    .unwrap();
    writeln!(output).unwrap();
    writeln!(
        output,
        "Device             tps    kB_read/s    kB_wrtn/s    kB_read    kB_wrtn"
    )
    .unwrap();
    writeln!(
        output,
        "vda               0.00         0.00         0.00          0          0"
    )
    .unwrap();
    ShellResult::ok(&output)
}

// ── dstat ───────────────────────────────────────────────────────
pub fn dstat(_args: &[String]) -> ShellResult {
    let mut output = String::new();
    writeln!(
        output,
        "---total-cpu-usage--- -dsk/total- -net/total- ---paging-- ---system--"
    )
    .unwrap();
    writeln!(
        output,
        "usr sys idl wai stl|  read  writ|  recv  send|  in   out |  int   csw"
    )
    .unwrap();
    writeln!(
        output,
        "  0   0 100   0   0|    0     0 |    0     0 |   0     0 |   1     1"
    )
    .unwrap();
    ShellResult::ok(&output)
}

// ── top (one-shot) ──────────────────────────────────────────────
pub fn top(_args: &[String]) -> ShellResult {
    let mut output = String::new();
    writeln!(
        output,
        "top - 00:00:00 up 0 min,  1 user,  load average: 0.00, 0.00, 0.00"
    )
    .unwrap();
    writeln!(
        output,
        "Tasks:   3 total,   1 running,   2 sleeping,   0 stopped,   0 zombie"
    )
    .unwrap();
    writeln!(
        output,
        "%Cpu(s):  0.0 us,  0.0 sy,  0.0 ni,100.0 id,  0.0 wa,  0.0 hi,  0.0 si"
    )
    .unwrap();
    writeln!(
        output,
        "MiB Mem :   1024.0 total,    768.0 free,    128.0 used,    128.0 buff/cache"
    )
    .unwrap();
    writeln!(
        output,
        "MiB Swap:      0.0 total,      0.0 free,      0.0 used.    896.0 avail Mem"
    )
    .unwrap();
    writeln!(output).unwrap();
    writeln!(
        output,
        "  PID USER      PR  NI    VIRT    RES    SHR S  %CPU  %MEM     TIME+ COMMAND"
    )
    .unwrap();
    writeln!(
        output,
        "    1 root      20   0    4096   2048   1024 S   0.0   0.2   0:00.01 init"
    )
    .unwrap();
    writeln!(
        output,
        "    2 root      20   0    8192   4096   2048 S   0.0   0.4   0:00.01 ksh"
    )
    .unwrap();
    writeln!(
        output,
        "    3 root      20   0    4096   1024    512 R   0.0   0.1   0:00.00 top"
    )
    .unwrap();
    ShellResult::ok(&output)
}

// ── htop (alias to top in ksh) ──────────────────────────────────
pub fn htop(args: &[String]) -> ShellResult {
    top(args)
}

// ── arch ────────────────────────────────────────────────────────
pub fn arch(_args: &[String]) -> ShellResult {
    ShellResult::ok("x86_64")
}

// ── nproc ───────────────────────────────────────────────────────
pub fn nproc(_args: &[String]) -> ShellResult {
    ShellResult::ok("1")
}

// ── getconf ─────────────────────────────────────────────────────
pub fn getconf(args: &[String]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::err("getconf: missing operand");
    }
    match args[0].as_str() {
        "NPROCESSORS_ONLN" | "NPROCESSORS_CONF" => ShellResult::ok("1"),
        "PAGE_SIZE" | "PAGESIZE" => ShellResult::ok("4096"),
        "LONG_BIT" => ShellResult::ok("64"),
        "ARG_MAX" => ShellResult::ok("2097152"),
        "PATH_MAX" => ShellResult::ok("4096"),
        "NAME_MAX" => ShellResult::ok("255"),
        _ => ShellResult::ok("undefined"),
    }
}

// ── printenv (redirect to env in core_cmds) ─────────────────────
pub fn printenv(args: &[String]) -> ShellResult {
    if args.is_empty() {
        let env = ENV_VARS.lock();
        let mut output = String::new();
        for (k, v) in env.iter() {
            writeln!(output, "{}={}", k, v).unwrap();
        }
        ShellResult::ok(&output)
    } else {
        let env = ENV_VARS.lock();
        if let Some(val) = env.get(args[0].as_str()) {
            ShellResult::ok(val)
        } else {
            ShellResult::with_code(1, "")
        }
    }
}

// ── reset / tset ────────────────────────────────────────────────
pub fn reset(_args: &[String]) -> ShellResult {
    ShellResult::ok("\x1b[2J\x1b[H\x1b[0m")
}

// ── sync ────────────────────────────────────────────────────────
pub fn sync(_args: &[String]) -> ShellResult {
    ShellResult::ok("")
}

// ── dmesg (alias handled in kernel.rs, but provide for misc) ────
// Already in kernel.rs, this is just a note.

// ── uptime (redirect handled in system.rs) ──────────────────────
// Already in system.rs.

// ── wc (redirect handled in files.rs) ───────────────────────────
// Already in files.rs.
