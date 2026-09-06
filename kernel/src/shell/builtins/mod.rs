/// Builtin command dispatcher — routes command names to their implementations
pub mod ai_cmds;
pub mod core_cmds;
pub mod files;
pub mod kernel;
pub mod misc;
pub mod net;
pub mod package;
pub mod security;
pub mod system;
pub mod text;

use alloc::string::String;

use super::helpers::resolve_command_path;
use super::types::ShellResult;

/// List of all builtin command names (for tab-completion and `type` command)
pub const BUILTIN_NAMES: &[&str] = &[
    "echo",
    "cd",
    "pwd",
    "export",
    "unset",
    "env",
    "printenv",
    "set",
    "exit",
    "logout",
    "history",
    "help",
    "type",
    "which",
    "alias",
    "true",
    "false",
    "test",
    "[",
    "source",
    ".",
    "unalias",
    // Job control
    "jobs",
    "fg",
    "bg",
    "wait",
    "disown",
    // Phase 7 additions
    "bind",
    "save_history",
    "theme",
    // Phase 8 additions
    "tmux",
    "ssh",
    "setxkbmap",
    "compose",
    // File operations
    "ls",
    "cat",
    "touch",
    "mkdir",
    "rm",
    "rmdir",
    "cp",
    "mv",
    "head",
    "tail",
    "wc",
    "grep",
    "find",
    "stat",
    "tree",
    "ln",
    "readlink",
    "realpath",
    "basename",
    "dirname",
    "chmod",
    "chown",
    "du",
    "mktemp",
    "file",
    "less",
    "more",
    "dd",
    "tar",
    "umask",
    // Text processing
    "tac",
    "sort",
    "uniq",
    "cut",
    "tr",
    "rev",
    "nl",
    "fold",
    "fmt",
    "sed",
    "awk",
    "diff",
    "comm",
    "paste",
    "join",
    "split",
    "tee",
    "xargs",
    "strings",
    "xxd",
    "hexdump",
    "od",
    "base64",
    "md5sum",
    "sha256sum",
    "cksum",
    "expr",
    "bc",
    "calc",
    "printf",
    "column",
    "expand",
    "unexpand",
    "cal",
    "factor",
    "shuf",
    "numfmt",
    "tput",
    // System info
    "uname",
    "hostname",
    "whoami",
    "id",
    "uptime",
    "date",
    "free",
    "df",
    "ps",
    "kill",
    "clear",
    "sleep",
    // Network
    "ifconfig",
    "ip",
    "ping",
    "wget",
    "curl",
    "nslookup",
    "dig",
    "host",
    "ss",
    "netstat",
    // GPU / display
    "lspci",
    "glxinfo",
    "xrandr",
    // Kernel management
    "dmesg",
    "lsmod",
    "modinfo",
    "insmod",
    "rmmod",
    "lsblk",
    "mount",
    "umount",
    "poweroff",
    "shutdown",
    "reboot",
    // Package manager
    "kpm",
    "apt",
    "apt-get",
    "dpkg",
    "dpkg-deb",
    "vivaldi",
    // Security
    "lscgroup",
    "getenforce",
    "sestatus",
    // AI / LLM
    "ai",
    "ask",
    "llm",
    "tensor",
    // Misc / system utilities
    "yes",
    "seq",
    "man",
    "neofetch",
    "watch",
    "timeout",
    "nohup",
    "w",
    "who",
    "last",
    "groups",
    "logname",
    "hostnamectl",
    "timedatectl",
    "locale",
    "lsof",
    "vmstat",
    "iostat",
    "dstat",
    "top",
    "htop",
    "arch",
    "nproc",
    "getconf",
    "reset",
    "tset",
    "sync",
];

/// Dispatch a command name to its builtin handler, or return None if not a builtin
pub fn dispatch(program: &str, args: &[String]) -> Option<ShellResult> {
    dispatch_with_stdin(program, args, None)
}

/// Dispatch a command with optional stdin data (from pipe).
/// Commands that support reading from stdin (grep, sort, cat, wc, head, tail,
/// sed, awk, tr, cut, uniq, tee, xargs, etc.) will use stdin_data when
/// no file arguments are provided.
pub fn dispatch_with_stdin(
    program: &str,
    args: &[String],
    stdin_data: Option<&str>,
) -> Option<ShellResult> {
    match program {
        // ── Shell builtins ──────────────────────────────
        "echo" => Some(core_cmds::echo(args)),
        "cd" => Some(core_cmds::cd(args)),
        "pwd" => Some(core_cmds::pwd()),
        "export" => Some(core_cmds::export(args)),
        "unset" => Some(core_cmds::unset(args)),
        "env" | "printenv" => Some(core_cmds::env(args)),
        "set" => Some(core_cmds::set()),
        "exit" | "logout" => Some(core_cmds::exit(args)),
        "history" => Some(core_cmds::history()),
        "help" => Some(core_cmds::help()),
        "type" | "which" => Some(core_cmds::r#type(args)),
        "alias" => Some(core_cmds::alias(args)),
        "unalias" => Some(core_cmds::unalias(args)),
        "true" => Some(ShellResult::ok("")),
        "false" => Some(ShellResult::with_code(1, "")),
        "test" | "[" => Some(core_cmds::test(args)),
        "source" | "." => Some(core_cmds::source(args)),

        // ── Job control ─────────────────────────────────
        "jobs" => Some(core_cmds::jobs(args)),
        "fg" => Some(core_cmds::fg(args)),
        "bg" => Some(core_cmds::bg(args)),
        "wait" => Some(core_cmds::wait(args)),
        "disown" => Some(core_cmds::disown(args)),

        // ── Phase 7: UX commands ────────────────────────
        "bind" => Some(core_cmds::bind(args)),
        "save_history" => Some(core_cmds::save_history(args)),
        "theme" => {
            if let Some(name) = args.first() {
                Some(ShellResult::ok(&alloc::format!("Theme set to: {}", name)))
            } else {
                Some(ShellResult::ok(
                    "Available themes: tokyo-night (default), solarized, monokai, dracula\nUsage: theme <name>",
                ))
            }
        }

        // ── Phase 8: Advanced terminal commands ─────────
        "tmux" => Some(core_cmds::tmux(args)),
        "ssh" => Some(core_cmds::ssh(args)),
        "setxkbmap" => Some(core_cmds::setxkbmap(args)),
        "compose" => Some(core_cmds::compose(args)),

        // ── File operations (stdin-aware) ───────────────
        "ls" => Some(files::ls(args)),
        "cat" => Some(files::cat_with_stdin(args, stdin_data)),
        "touch" => Some(files::touch(args)),
        "mkdir" => Some(files::mkdir(args)),
        "rm" => Some(files::rm(args)),
        "rmdir" => Some(files::rmdir(args)),
        "cp" => Some(files::cp(args)),
        "mv" => Some(files::mv(args)),
        "head" => Some(files::head_with_stdin(args, stdin_data)),
        "tail" => Some(files::tail_with_stdin(args, stdin_data)),
        "wc" => Some(files::wc_with_stdin(args, stdin_data)),
        "grep" => Some(files::grep_with_stdin(args, stdin_data)),
        "find" => Some(files::find(args)),
        "stat" => Some(files::stat(args)),
        "tree" => Some(files::tree(args)),
        "ln" => Some(files::ln(args)),
        "readlink" => Some(files::readlink(args)),
        "realpath" => Some(files::realpath(args)),
        "basename" => Some(files::basename(args)),
        "dirname" => Some(files::dirname(args)),
        "chmod" => Some(files::chmod(args)),
        "chown" => Some(files::chown(args)),
        "du" => Some(files::du(args)),
        "mktemp" => Some(files::mktemp(args)),
        "file" => Some(files::file(args)),
        "less" | "more" => Some(files::less(args)),
        "dd" => Some(files::dd(args)),
        "tar" => Some(files::tar(args)),
        "umask" => Some(files::umask(args)),

        // ── Text processing (stdin-aware) ───────────────
        "tac" => Some(text::tac_with_stdin(args, stdin_data)),
        "sort" => Some(text::sort_with_stdin(args, stdin_data)),
        "uniq" => Some(text::uniq_with_stdin(args, stdin_data)),
        "cut" => Some(text::cut_with_stdin(args, stdin_data)),
        "tr" => Some(text::tr_with_stdin(args, stdin_data)),
        "rev" => Some(text::rev_with_stdin(args, stdin_data)),
        "nl" => Some(text::nl_with_stdin(args, stdin_data)),
        "fold" => Some(text::fold_with_stdin(args, stdin_data)),
        "fmt" => Some(text::fmt_with_stdin(args, stdin_data)),
        "sed" => Some(text::sed_with_stdin(args, stdin_data)),
        "awk" => Some(text::awk_with_stdin(args, stdin_data)),
        "diff" => Some(text::diff(args)),
        "comm" => Some(text::comm(args)),
        "paste" => Some(text::paste(args)),
        "join" => Some(text::join(args)),
        "split" => Some(text::split(args)),
        "tee" => Some(text::tee_with_stdin(args, stdin_data)),
        "xargs" => Some(text::xargs_with_stdin(args, stdin_data)),
        "strings" => Some(text::strings(args)),
        "xxd" | "hexdump" => Some(text::xxd(args)),
        "od" => Some(text::od(args)),
        "base64" => Some(text::base64_with_stdin(args, stdin_data)),
        "md5sum" => Some(text::md5sum(args)),
        "sha256sum" => Some(text::sha256sum(args)),
        "cksum" => Some(text::cksum(args)),
        "expr" => Some(text::expr(args)),
        "bc" | "calc" => Some(text::bc(args)),
        "printf" => Some(text::printf(args)),
        "column" => Some(text::column(args)),
        "expand" => Some(text::expand(args)),
        "unexpand" => Some(text::unexpand(args)),
        "cal" => Some(text::cal(args)),
        "factor" => Some(text::factor(args)),
        "shuf" => Some(text::shuf(args)),
        "numfmt" => Some(text::numfmt(args)),
        "tput" => Some(text::tput(args)),

        // ── System info ─────────────────────────────────
        "uname" => Some(system::uname(args)),
        "hostname" => Some(system::hostname()),
        "whoami" => Some(system::whoami()),
        "id" => Some(system::id()),
        "uptime" => Some(system::uptime()),
        "date" => Some(system::date()),
        "free" => Some(system::free()),
        "df" => Some(system::df()),
        "ps" => Some(system::ps(args)),
        "kill" => Some(system::kill(args)),
        "clear" => Some(ShellResult::ok("\x1b[2J\x1b[H")),
        "sleep" => Some(system::sleep(args)),

        // ── Network ─────────────────────────────────────────
        "ifconfig" | "ip" => Some(net::ifconfig()),
        "ping" => Some(net::ping(args)),
        "wget" => Some(net::wget(args)),
        "curl" => Some(net::curl(args)),
        "nslookup" => Some(net::nslookup(args)),
        "dig" => Some(net::dig(args)),
        "host" => Some(net::host_cmd(args)),
        "ss" | "netstat" => Some(net::ss(args)),

        // ── GPU / display ───────────────────────────────
        "lspci" | "glxinfo" => Some(security::gpu_info()),
        "xrandr" => Some(security::xrandr(args)),

        // ── Kernel management ───────────────────────────
        "dmesg" => Some(kernel::dmesg(args)),
        "lsmod" => Some(kernel::lsmod()),
        "modinfo" => Some(kernel::modinfo(args)),
        "insmod" => Some(kernel::insmod(args)),
        "rmmod" => Some(kernel::rmmod(args)),
        "lsblk" => Some(kernel::lsblk()),
        "mount" => Some(kernel::mount(args)),
        "umount" => Some(kernel::umount(args)),
        "poweroff" | "shutdown" => Some(kernel::poweroff()),
        "reboot" => Some(kernel::reboot()),

        // ── Package manager ─────────────────────────────
        "kpm" | "apt" | "apt-get" => Some(package::kpm(args)),
        "dpkg" | "dpkg-deb" => Some(package::dpkg(args)),
        "vivaldi" => Some(package::vivaldi_cmd(args)),

        // ── Security / cgroups ──────────────────────────
        "lscgroup" => Some(security::lscgroup()),
        "getenforce" => Some(security::getenforce()),
        "sestatus" => Some(security::sestatus()),

        // ── AI / LLM ───────────────────────────────────
        "ai" | "ask" | "llm" | "tensor" => Some(ai_cmds::dispatch_ai(program, args)),

        // ── Misc / system utilities ─────────────────────
        "yes" => Some(ShellResult::ok("y")),
        "seq" => Some(system::seq(args)),
        "man" => Some(misc::man(args)),
        "neofetch" => Some(misc::neofetch(args)),
        "watch" => Some(misc::watch(args)),
        "timeout" => Some(misc::timeout(args)),
        "nohup" => Some(misc::nohup(args)),
        "w" => Some(misc::w(args)),
        "who" => Some(misc::who(args)),
        "last" => Some(misc::last(args)),
        "groups" => Some(misc::groups(args)),
        "logname" => Some(misc::logname(args)),
        "hostnamectl" => Some(misc::hostnamectl(args)),
        "timedatectl" => Some(misc::timedatectl(args)),
        "locale" => Some(misc::locale(args)),
        "lsof" => Some(misc::lsof(args)),
        "vmstat" => Some(misc::vmstat(args)),
        "iostat" => Some(misc::iostat(args)),
        "dstat" => Some(misc::dstat(args)),
        "top" => Some(misc::top(args)),
        "htop" => Some(misc::htop(args)),
        "arch" => Some(misc::arch(args)),
        "nproc" => Some(misc::nproc(args)),
        "getconf" => Some(misc::getconf(args)),
        "reset" | "tset" => Some(misc::reset(args)),
        "sync" => Some(misc::sync(args)),

        // Not a builtin
        _ => None,
    }
}

/// Check if a command name is a known builtin
pub fn is_builtin(name: &str) -> bool {
    BUILTIN_NAMES.contains(&name)
}
