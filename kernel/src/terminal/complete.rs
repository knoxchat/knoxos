/// Smart context-aware tab completion engine
use alloc::string::String;
use alloc::vec::Vec;

use super::highlight::KNOWN_COMMANDS;

// ═══════════════════════════════════════════════════════════════════════════
// TAB COMPLETION ENGINE — Smart context-aware completion
// ═══════════════════════════════════════════════════════════════════════════

/// Completion candidate
#[derive(Debug, Clone)]
pub struct Completion {
    pub text: String,
    pub display: String,
    pub kind: CompletionKind,
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionKind {
    Command,
    File,
    Directory,
    Variable,
    Flag,
}

/// Generate tab completions for the current input
pub fn complete(input: &str, cursor_pos: usize) -> Vec<Completion> {
    let mut completions = Vec::new();
    let input_to_cursor = if cursor_pos <= input.len() {
        &input[..cursor_pos]
    } else {
        input
    };

    let parts: Vec<&str> = input_to_cursor.split_whitespace().collect();
    let current_word = parts.last().copied().unwrap_or("");
    let is_first_word = parts.len() <= 1;

    if is_first_word {
        // Complete commands
        complete_commands(current_word, &mut completions);
    } else {
        let cmd = parts[0];

        // Complete flags for known commands
        if current_word.starts_with('-') {
            complete_flags(cmd, current_word, &mut completions);
        }
        // Complete variables
        else if let Some(stripped) = current_word.strip_prefix('$') {
            complete_variables(stripped, &mut completions);
        }
        // Complete paths/files
        else {
            complete_paths(current_word, &mut completions);
        }
    }

    completions
}

fn complete_commands(prefix: &str, completions: &mut Vec<Completion>) {
    // 1. Exact prefix matches first
    for &cmd in KNOWN_COMMANDS {
        if cmd.starts_with(prefix) {
            completions.push(Completion {
                text: String::from(cmd),
                display: String::from(cmd),
                kind: CompletionKind::Command,
                description: command_description(cmd),
            });
        }
    }
    // 2. Substring matches (fuzzy) — only if no exact prefix matches
    if completions.is_empty() && prefix.len() >= 2 {
        for &cmd in KNOWN_COMMANDS {
            if cmd.contains(prefix) && !cmd.starts_with(prefix) {
                completions.push(Completion {
                    text: String::from(cmd),
                    display: String::from(cmd),
                    kind: CompletionKind::Command,
                    description: command_description(cmd),
                });
            }
        }
    }
    // 3. Fuzzy character-subsequence matches — if still nothing
    if completions.is_empty() && prefix.len() >= 2 {
        for &cmd in KNOWN_COMMANDS {
            if fuzzy_match(prefix, cmd) {
                completions.push(Completion {
                    text: String::from(cmd),
                    display: String::from(cmd),
                    kind: CompletionKind::Command,
                    description: command_description(cmd),
                });
            }
        }
    }
}

/// Fuzzy subsequence match: every character in `query` appears in `target` in order
fn fuzzy_match(query: &str, target: &str) -> bool {
    let mut target_iter = target.chars();
    for qc in query.chars() {
        let qc_lower = qc.to_ascii_lowercase();
        let mut found = false;
        for tc in target_iter.by_ref() {
            if tc.to_ascii_lowercase() == qc_lower {
                found = true;
                break;
            }
        }
        if !found {
            return false;
        }
    }
    true
}

fn complete_paths(prefix: &str, completions: &mut Vec<Completion>) {
    use crate::shell;

    // Resolve the directory to list
    let (dir_part, file_prefix) = if let Some(last_slash) = prefix.rfind('/') {
        (&prefix[..=last_slash], &prefix[last_slash + 1..])
    } else {
        ("", prefix)
    };

    let search_dir = if dir_part.is_empty() {
        shell::ENV_VARS
            .lock()
            .get("PWD")
            .cloned()
            .unwrap_or_else(|| String::from("/"))
    } else if dir_part.starts_with('/') {
        String::from(dir_part)
    } else if dir_part.starts_with("~/") {
        let home = shell::ENV_VARS
            .lock()
            .get("HOME")
            .cloned()
            .unwrap_or_else(|| String::from("/home/user"));
        alloc::format!("{}{}", home, &dir_part[1..])
    } else {
        let pwd = shell::ENV_VARS
            .lock()
            .get("PWD")
            .cloned()
            .unwrap_or_else(|| String::from("/"));
        alloc::format!("{}/{}", pwd, dir_part)
    };

    let vfs = crate::vfs::VFS.lock();
    if let Some(entries) = vfs.list_dir(&search_dir) {
        for entry in entries {
            // Match: prefix, substring, or fuzzy
            let is_match = entry.starts_with(file_prefix)
                || (file_prefix.len() >= 2 && entry.contains(file_prefix))
                || (file_prefix.len() >= 2 && fuzzy_match(file_prefix, &entry));
            if is_match {
                let full_entry_path = if search_dir == "/" {
                    alloc::format!("/{}", entry)
                } else {
                    alloc::format!("{}/{}", search_dir.trim_end_matches('/'), entry)
                };

                let is_dir = vfs
                    .resolve_path(&full_entry_path)
                    .and_then(|ino| vfs.get_inode(ino))
                    .map(|n| n.file_type == crate::vfs::FileType::Directory)
                    .unwrap_or(false);

                let completion_text =
                    alloc::format!("{}{}{}", dir_part, entry, if is_dir { "/" } else { "" });

                completions.push(Completion {
                    text: completion_text,
                    display: entry.clone(),
                    kind: if is_dir {
                        CompletionKind::Directory
                    } else {
                        CompletionKind::File
                    },
                    description: if is_dir {
                        String::from("directory")
                    } else {
                        String::from("file")
                    },
                });
            }
        }
    }
}

fn complete_variables(prefix: &str, completions: &mut Vec<Completion>) {
    use crate::shell;

    let env = shell::ENV_VARS.lock();
    for (key, value) in env.iter() {
        if key.starts_with(prefix) {
            completions.push(Completion {
                text: alloc::format!("${}", key),
                display: alloc::format!("${}", key),
                kind: CompletionKind::Variable,
                description: if value.len() > 30 {
                    alloc::format!("{}...", &value[..27])
                } else {
                    value.clone()
                },
            });
        }
    }
}

fn complete_flags(cmd: &str, prefix: &str, completions: &mut Vec<Completion>) {
    // Comprehensive built-in flag database covering all 137+ builtins and common
    // Unix commands. This serves as a kernel-resident flag knowledge base,
    // equivalent to what man-page parsing would provide.
    let flags: &[(&str, &str)] = match cmd {
        // ── Core shell builtins ─────────────────────────────────────
        "echo" => &[
            ("-n", "no trailing newline"),
            ("-e", "enable escape sequences"),
            ("-E", "disable escape sequences"),
        ],
        "cd" => &[
            ("-P", "use physical dir structure"),
            ("-L", "follow symlinks (default)"),
            ("-", "go to OLDPWD"),
        ],
        "export" => &[
            ("-n", "remove export property"),
            ("-p", "list all exported vars"),
        ],
        "set" => &[
            ("-e", "exit on error"),
            ("-u", "error on unset variables"),
            ("-x", "print commands as executed"),
            ("-o", "set option by name"),
            ("+o", "unset option by name"),
            ("-f", "disable filename expansion"),
            ("-n", "read commands but don't execute"),
            ("-v", "print shell input lines"),
        ],
        "history" => &[
            ("-c", "clear history"),
            ("-d", "delete entry at offset"),
            ("-w", "write history to file"),
            ("-r", "read history from file"),
        ],
        "test" | "[" => &[
            ("-e", "file exists"),
            ("-f", "is regular file"),
            ("-d", "is directory"),
            ("-r", "is readable"),
            ("-w", "is writable"),
            ("-x", "is executable"),
            ("-s", "file size > 0"),
            ("-L", "is symlink"),
            ("-z", "string is zero length"),
            ("-n", "string is non-zero length"),
            ("-eq", "integer equal"),
            ("-ne", "integer not equal"),
            ("-lt", "integer less than"),
            ("-le", "integer less or equal"),
            ("-gt", "integer greater than"),
            ("-ge", "integer greater or equal"),
        ],
        "alias" => &[("-p", "print all aliases")],
        "unalias" => &[("-a", "remove all aliases")],
        "source" | "." => &[],
        "type" | "which" => &[
            ("-a", "show all matches"),
            ("-t", "show type only"),
            ("-p", "show path only"),
        ],
        "help" => &[
            ("-d", "short description"),
            ("-m", "man page format"),
            ("-s", "short usage"),
        ],
        // ── Job control ─────────────────────────────────────────────
        "jobs" => &[
            ("-l", "show PIDs"),
            ("-p", "PIDs only"),
            ("-n", "only changed since notified"),
            ("-r", "running jobs only"),
            ("-s", "stopped jobs only"),
        ],
        "wait" => &[
            ("-n", "wait for any job"),
            ("-p", "store PID in variable"),
            ("-f", "wait for termination"),
        ],
        "disown" => &[
            ("-a", "remove all jobs"),
            ("-h", "mark to not receive SIGHUP"),
            ("-r", "remove running jobs"),
        ],
        "kill" => &[
            ("-9", "SIGKILL (force)"),
            ("-15", "SIGTERM (terminate)"),
            ("-1", "SIGHUP (hangup)"),
            ("-2", "SIGINT (interrupt)"),
            ("-3", "SIGQUIT (quit)"),
            ("-STOP", "SIGSTOP (stop)"),
            ("-CONT", "SIGCONT (continue)"),
            ("-TSTP", "SIGTSTP (terminal stop)"),
            ("-USR1", "SIGUSR1 (user 1)"),
            ("-USR2", "SIGUSR2 (user 2)"),
            ("-l", "list signal names"),
            ("-s", "specify signal by name"),
        ],
        // ── File operations ─────────────────────────────────────────
        "ls" => &[
            ("-a", "show hidden files"),
            ("-A", "show hidden except . and .."),
            ("-l", "long listing format"),
            ("-la", "long + hidden"),
            ("-lh", "long + human-readable"),
            ("-h", "human-readable sizes"),
            ("-R", "recursive listing"),
            ("-r", "reverse sort order"),
            ("-S", "sort by file size"),
            ("-t", "sort by modification time"),
            ("-d", "list directories themselves"),
            ("-i", "show inode numbers"),
            ("-1", "one entry per line"),
            ("-F", "append type indicator"),
            ("--color", "colorized output"),
            ("--group-directories-first", "dirs before files"),
        ],
        "cat" => &[
            ("-n", "number all lines"),
            ("-b", "number non-blank lines"),
            ("-s", "squeeze blank lines"),
            ("-E", "show $ at end of lines"),
            ("-T", "show tabs as ^I"),
            ("-A", "show all (equiv -vET)"),
        ],
        "touch" => &[
            ("-a", "change access time only"),
            ("-m", "change modification time only"),
            ("-c", "do not create file"),
            ("-d", "use specified date string"),
            ("-t", "use specified timestamp"),
            ("-r", "use reference file's times"),
        ],
        "mkdir" => &[
            ("-p", "create parent directories"),
            ("-m", "set permissions mode"),
            ("-v", "verbose output"),
        ],
        "rm" => &[
            ("-r", "recursive"),
            ("-R", "recursive (same as -r)"),
            ("-f", "force, no prompt"),
            ("-rf", "recursive + force"),
            ("-i", "prompt before each removal"),
            ("-I", "prompt once for >3 files"),
            ("-v", "verbose output"),
            ("-d", "remove empty directories"),
        ],
        "rmdir" => &[
            ("-p", "remove parent dirs too"),
            ("-v", "verbose output"),
            ("--ignore-fail-on-non-empty", "suppress non-empty errors"),
        ],
        "cp" => &[
            ("-r", "recursive copy"),
            ("-R", "recursive (same as -r)"),
            ("-i", "prompt before overwrite"),
            ("-f", "force overwrite"),
            ("-v", "verbose output"),
            ("-a", "archive (preserve all attributes)"),
            ("-l", "hard link instead of copy"),
            ("-s", "symbolic link instead of copy"),
            ("-u", "update (skip newer dest)"),
            ("-p", "preserve mode, ownership, timestamps"),
            ("-n", "do not overwrite existing"),
            ("--backup", "make backup of each dest"),
        ],
        "mv" => &[
            ("-i", "prompt before overwrite"),
            ("-f", "force overwrite"),
            ("-v", "verbose output"),
            ("-n", "do not overwrite existing"),
            ("-u", "update (skip newer dest)"),
            ("--backup", "make backup of each dest"),
        ],
        "head" => &[
            ("-n", "number of lines to show"),
            ("-c", "number of bytes to show"),
            ("-q", "never print headers"),
            ("-v", "always print headers"),
        ],
        "tail" => &[
            ("-n", "number of lines to show"),
            ("-c", "number of bytes to show"),
            ("-f", "follow (output appended data)"),
            ("-F", "follow and retry if renamed"),
            ("-q", "never print headers"),
            ("-v", "always print headers"),
            ("--pid", "terminate after PID dies"),
        ],
        "wc" => &[
            ("-l", "count lines"),
            ("-w", "count words"),
            ("-c", "count bytes"),
            ("-m", "count characters"),
            ("-L", "max line length"),
        ],
        "grep" => &[
            ("-i", "case insensitive"),
            ("-v", "invert match"),
            ("-n", "show line numbers"),
            ("-c", "count matches only"),
            ("-l", "files with matches"),
            ("-L", "files without matches"),
            ("-r", "recursive search"),
            ("-R", "recursive (follow symlinks)"),
            ("-w", "match whole words"),
            ("-x", "match whole lines"),
            ("-E", "extended regex"),
            ("-F", "fixed string (no regex)"),
            ("-P", "Perl-compatible regex"),
            ("-o", "show only matching part"),
            ("-q", "quiet (exit status only)"),
            ("-s", "suppress error messages"),
            ("-H", "print filename"),
            ("-h", "suppress filename"),
            ("-A", "lines after match"),
            ("-B", "lines before match"),
            ("-C", "lines around match"),
            ("--color", "highlight matches"),
            ("--include", "search only matching files"),
            ("--exclude", "skip matching files"),
            ("--exclude-dir", "skip matching dirs"),
        ],
        "find" => &[
            ("-name", "match filename pattern"),
            ("-iname", "case-insensitive name"),
            ("-type", "file type (f,d,l,p,s,b,c)"),
            ("-size", "file size (+/-N[cwbkMG])"),
            ("-mtime", "modified N days ago"),
            ("-atime", "accessed N days ago"),
            ("-ctime", "changed N days ago"),
            ("-newer", "newer than file"),
            ("-perm", "permission match"),
            ("-user", "owned by user"),
            ("-group", "owned by group"),
            ("-maxdepth", "max directory depth"),
            ("-mindepth", "min directory depth"),
            ("-exec", "execute command on results"),
            ("-delete", "delete matching files"),
            ("-print", "print pathnames"),
            ("-print0", "print null-delimited"),
            ("-empty", "empty files or dirs"),
            ("-regex", "path matches regex"),
            ("-not", "negate expression"),
            ("-or", "logical OR"),
            ("-and", "logical AND"),
        ],
        "stat" => &[
            ("-c", "use format string"),
            ("-f", "display filesystem status"),
            ("-L", "follow symlinks"),
            ("-t", "terse output"),
        ],
        "tree" => &[
            ("-a", "show hidden files"),
            ("-d", "directories only"),
            ("-f", "print full path"),
            ("-L", "max display depth"),
            ("-I", "exclude pattern"),
            ("-p", "show permissions"),
            ("-s", "show file size"),
            ("-h", "human-readable sizes"),
            ("--dirsfirst", "list dirs before files"),
            ("--noreport", "omit summary report"),
        ],
        "ln" => &[
            ("-s", "create symbolic link"),
            ("-f", "force (remove existing)"),
            ("-v", "verbose output"),
            ("-i", "prompt before removal"),
            ("-r", "create relative symlink"),
        ],
        "readlink" => &[
            ("-f", "canonicalize (follow all symlinks)"),
            ("-e", "canonicalize (all must exist)"),
            ("-m", "canonicalize (no existence required)"),
            ("-n", "no trailing newline"),
        ],
        "chmod" => &[
            ("-R", "recursive"),
            ("-v", "verbose output"),
            ("-c", "report only changes"),
            ("-f", "suppress errors"),
            ("--reference", "use reference file perms"),
        ],
        "chown" => &[
            ("-R", "recursive"),
            ("-v", "verbose output"),
            ("-c", "report only changes"),
            ("-f", "suppress errors"),
            ("-h", "affect symlinks"),
            ("--reference", "use reference file owner"),
        ],
        "du" => &[
            ("-h", "human-readable sizes"),
            ("-s", "summary only (total)"),
            ("-a", "all files, not just dirs"),
            ("-c", "produce grand total"),
            ("-d", "max depth"),
            ("-k", "sizes in kilobytes"),
            ("-m", "sizes in megabytes"),
            ("--max-depth", "max directory depth"),
            ("--exclude", "exclude pattern"),
        ],
        "mktemp" => &[
            ("-d", "create directory"),
            ("-p", "use specified directory"),
            ("-t", "interpret template relative to tmpdir"),
            ("-u", "dry run (unsafe)"),
            ("-q", "suppress error messages"),
        ],
        "file" => &[
            ("-b", "brief (no filename)"),
            ("-i", "output mime type"),
            ("-L", "follow symlinks"),
            ("-z", "look inside compressed files"),
        ],
        "less" | "more" => &[
            ("-N", "show line numbers"),
            ("-S", "chop long lines"),
            ("-R", "raw control characters"),
            ("-F", "quit if one screen"),
            ("-X", "no init/deinit termcap"),
            ("-i", "case insensitive search"),
            ("-g", "highlight last search only"),
        ],
        "dd" => &[
            ("if=", "input file"),
            ("of=", "output file"),
            ("bs=", "block size"),
            ("count=", "number of blocks"),
            ("skip=", "skip N input blocks"),
            ("seek=", "skip N output blocks"),
            ("conv=", "conversion options"),
            ("status=", "transfer stats (none/noxfer/progress)"),
        ],
        "tar" => &[
            ("-c", "create archive"),
            ("-x", "extract archive"),
            ("-t", "list archive contents"),
            ("-f", "archive file name"),
            ("-v", "verbose output"),
            ("-z", "gzip compression"),
            ("-j", "bzip2 compression"),
            ("-J", "xz compression"),
            ("-C", "change to directory"),
            ("-p", "preserve permissions"),
            ("--exclude", "exclude pattern"),
            ("--strip-components", "strip leading dirs"),
        ],
        "umask" => &[("-S", "symbolic output"), ("-p", "output in reusable form")],
        // ── Text processing ─────────────────────────────────────────
        "tac" => &[
            ("-s", "use string as separator"),
            ("-b", "attach separator before"),
            ("-r", "separator is a regex"),
        ],
        "sort" => &[
            ("-r", "reverse order"),
            ("-n", "numeric sort"),
            ("-k", "sort by key field"),
            ("-t", "field separator"),
            ("-u", "unique (remove dupes)"),
            ("-f", "case insensitive"),
            ("-h", "human numeric sort"),
            ("-V", "version sort"),
            ("-s", "stable sort"),
            ("-o", "output to file"),
            ("-c", "check if sorted"),
            ("-m", "merge sorted files"),
        ],
        "uniq" => &[
            ("-c", "prefix with count"),
            ("-d", "only print duplicates"),
            ("-u", "only print unique lines"),
            ("-i", "case insensitive"),
            ("-f", "skip N fields"),
            ("-s", "skip N characters"),
            ("-w", "compare at most N chars"),
        ],
        "cut" => &[
            ("-f", "select fields"),
            ("-d", "field delimiter"),
            ("-c", "select characters"),
            ("-b", "select bytes"),
            ("-s", "only delimited lines"),
            ("--complement", "complement selection"),
            ("--output-delimiter", "output separator"),
        ],
        "tr" => &[
            ("-d", "delete characters"),
            ("-s", "squeeze repeats"),
            ("-c", "complement set"),
            ("-C", "complement (char values)"),
            ("-t", "truncate set1 to set2 length"),
        ],
        "rev" => &[],
        "nl" => &[
            ("-b", "body numbering style"),
            ("-n", "number format (ln/rn/rz)"),
            ("-w", "number width"),
            ("-s", "separator after number"),
            ("-i", "line number increment"),
            ("-v", "starting line number"),
        ],
        "fold" => &[
            ("-w", "line width (default 80)"),
            ("-s", "break at spaces"),
            ("-b", "count bytes, not columns"),
        ],
        "fmt" => &[
            ("-w", "max line width"),
            ("-s", "split only, no joining"),
            ("-u", "uniform spacing"),
            ("-p", "only lines starting with prefix"),
        ],
        "sed" => &[
            ("-n", "suppress auto-print"),
            ("-e", "add script command"),
            ("-f", "script from file"),
            ("-i", "edit files in place"),
            ("-r", "extended regex"),
            ("-E", "extended regex (same as -r)"),
            ("--quiet", "same as -n"),
            ("--silent", "same as -n"),
        ],
        "awk" => &[
            ("-F", "field separator"),
            ("-v", "assign variable"),
            ("-f", "program from file"),
            ("--csv", "CSV mode"),
        ],
        "diff" => &[
            ("-u", "unified format"),
            ("-c", "context format"),
            ("-r", "recursive"),
            ("-q", "report only if different"),
            ("-s", "report identical files"),
            ("-i", "case insensitive"),
            ("-w", "ignore all whitespace"),
            ("-b", "ignore space changes"),
            ("-B", "ignore blank lines"),
            ("-N", "treat absent files as empty"),
            ("--color", "colorized output"),
            ("-y", "side-by-side"),
        ],
        "comm" => &[
            ("-1", "suppress column 1 (unique to file1)"),
            ("-2", "suppress column 2 (unique to file2)"),
            ("-3", "suppress column 3 (common lines)"),
        ],
        "paste" => &[
            ("-d", "delimiter list"),
            ("-s", "serial (one file per line)"),
        ],
        "join" => &[
            ("-1", "join field from file 1"),
            ("-2", "join field from file 2"),
            ("-t", "field separator"),
            ("-a", "print unpairable lines"),
            ("-e", "replace empty fields"),
            ("-o", "output format"),
            ("-v", "like -a but suppress paired"),
            ("-i", "case insensitive"),
        ],
        "split" => &[
            ("-l", "lines per output file"),
            ("-b", "bytes per output file"),
            ("-n", "number of output files"),
            ("-d", "use numeric suffixes"),
            ("-a", "suffix length"),
            ("--additional-suffix", "append suffix"),
        ],
        "tee" => &[
            ("-a", "append to files"),
            ("-i", "ignore interrupt signals"),
            ("-p", "diagnose write errors"),
        ],
        "xargs" => &[
            ("-I", "replace string"),
            ("-n", "max args per command"),
            ("-P", "max parallel processes"),
            ("-d", "input delimiter"),
            ("-0", "null-delimited input"),
            ("-t", "print commands before exec"),
            ("-p", "prompt before exec"),
            ("-r", "no run if empty input"),
            ("-L", "max input lines per cmd"),
        ],
        "strings" => &[
            ("-a", "scan whole file"),
            ("-n", "min string length"),
            ("-t", "print offset (d/o/x)"),
            ("-e", "encoding (s/S/b/l/B/L)"),
        ],
        "xxd" | "hexdump" => &[
            ("-l", "output length"),
            ("-s", "start offset"),
            ("-c", "columns per line"),
            ("-g", "group size"),
            ("-r", "reverse (hex to binary)"),
            ("-p", "plain hex dump"),
            ("-i", "C include style"),
            ("-u", "uppercase hex"),
        ],
        "od" => &[
            ("-A", "address radix (d/o/x/n)"),
            ("-t", "output type"),
            ("-c", "printable chars or escapes"),
            ("-x", "hexadecimal shorts"),
            ("-N", "limit bytes to dump"),
            ("-j", "skip bytes"),
            ("-w", "output width"),
        ],
        "base64" => &[
            ("-d", "decode"),
            ("-w", "wrap at N columns"),
            ("-i", "ignore garbage"),
        ],
        "md5sum" => &[
            ("-c", "check against file"),
            ("-b", "binary mode"),
            ("-t", "text mode"),
            ("--quiet", "suppress OK messages"),
            ("--status", "exit code only"),
        ],
        "sha256sum" => &[
            ("-c", "check against file"),
            ("-b", "binary mode"),
            ("-t", "text mode"),
            ("--quiet", "suppress OK messages"),
            ("--status", "exit code only"),
        ],
        "cksum" => &[],
        "expr" => &[],
        "bc" | "calc" => &[
            ("-l", "load math library"),
            ("-q", "quiet (no banner)"),
            ("-s", "strict POSIX mode"),
        ],
        "printf" => &[],
        "column" => &[
            ("-t", "create table"),
            ("-s", "column separator"),
            ("-o", "output separator"),
            ("-c", "output width"),
            ("-x", "fill columns before rows"),
        ],
        "expand" => &[
            ("-t", "tab stop positions"),
            ("--initial", "only leading tabs"),
        ],
        "unexpand" => &[
            ("-a", "convert all spaces"),
            ("-t", "tab stop positions"),
            ("--first-only", "only leading spaces"),
        ],
        "cal" => &[
            ("-3", "show 3 months"),
            ("-y", "show whole year"),
            ("-m", "start week on Monday"),
            ("-j", "Julian dates"),
        ],
        "factor" => &[],
        "shuf" => &[
            ("-n", "output at most N lines"),
            ("-e", "treat args as input"),
            ("-i", "range LO-HI"),
            ("-o", "output to file"),
            ("-r", "allow repeats"),
        ],
        "numfmt" => &[
            ("--from", "input scale (none/si/iec/iec-i/auto)"),
            ("--to", "output scale"),
            ("--padding", "output width"),
            ("--suffix", "append suffix"),
            ("--round", "rounding method"),
            ("-d", "field delimiter"),
            ("--field", "convert specific fields"),
        ],
        "tput" => &[
            ("clear", "clear screen"),
            ("reset", "reset terminal"),
            ("cols", "number of columns"),
            ("lines", "number of lines"),
            ("bold", "bold text"),
            ("sgr0", "reset attributes"),
            ("cup", "move cursor to row col"),
            ("setaf", "set foreground color"),
            ("setab", "set background color"),
        ],
        // ── System info ─────────────────────────────────────────────
        "uname" => &[
            ("-a", "all system info"),
            ("-s", "kernel name"),
            ("-n", "network hostname"),
            ("-r", "kernel release"),
            ("-v", "kernel version"),
            ("-m", "machine hardware"),
            ("-p", "processor type"),
            ("-o", "operating system"),
        ],
        "hostname" => &[
            ("-f", "FQDN"),
            ("-i", "IP address"),
            ("-d", "domain name"),
            ("-s", "short hostname"),
        ],
        "id" => &[
            ("-u", "effective user ID"),
            ("-g", "effective group ID"),
            ("-G", "all group IDs"),
            ("-n", "print name, not number"),
            ("-r", "real ID instead of effective"),
        ],
        "uptime" => &[("-p", "pretty format"), ("-s", "system up since")],
        "date" => &[
            ("-u", "UTC time"),
            ("-d", "display specified date"),
            ("-r", "last modification of file"),
            ("-I", "ISO 8601 format"),
            ("-R", "RFC 2822 format"),
            ("+%Y", "year"),
            ("+%m", "month"),
            ("+%d", "day"),
            ("+%H", "hour"),
            ("+%M", "minute"),
            ("+%S", "second"),
            ("+%s", "epoch seconds"),
        ],
        "free" => &[
            ("-h", "human-readable"),
            ("-b", "bytes"),
            ("-k", "kilobytes"),
            ("-m", "megabytes"),
            ("-g", "gigabytes"),
            ("-t", "show total"),
            ("-s", "repeat every N seconds"),
        ],
        "df" => &[
            ("-h", "human-readable"),
            ("-i", "show inodes"),
            ("-T", "show filesystem type"),
            ("-a", "include pseudo filesystems"),
            ("-l", "local filesystems only"),
            ("-k", "1K blocks"),
            ("-P", "POSIX output"),
        ],
        "ps" => &[
            ("-e", "all processes"),
            ("-f", "full format"),
            ("-l", "long format"),
            ("-A", "all processes (same as -e)"),
            ("-u", "user-oriented format"),
            ("-a", "all with tty except leaders"),
            ("-x", "no controlling tty"),
            ("--sort", "sort by field"),
            ("-o", "custom output format"),
            ("-p", "select by PID"),
            ("-C", "select by command name"),
            ("--forest", "process tree"),
        ],
        "sleep" => &[],
        // ── Network ─────────────────────────────────────────────────
        "ifconfig" => &[
            ("-a", "show all interfaces"),
            ("up", "activate interface"),
            ("down", "deactivate interface"),
        ],
        "ip" => &[
            ("addr", "manage addresses"),
            ("link", "manage interfaces"),
            ("route", "manage routes"),
            ("neigh", "manage ARP table"),
            ("-4", "IPv4 only"),
            ("-6", "IPv6 only"),
            ("-s", "show statistics"),
            ("-c", "color output"),
            ("-br", "brief output"),
        ],
        "ping" => &[
            ("-c", "count of pings"),
            ("-i", "interval (seconds)"),
            ("-w", "deadline (seconds)"),
            ("-W", "timeout (seconds)"),
            ("-s", "packet size"),
            ("-t", "TTL value"),
            ("-q", "quiet output"),
            ("-f", "flood ping"),
            ("-n", "numeric output"),
        ],
        // ── Kernel management ───────────────────────────────────────
        "dmesg" => &[
            ("-n", "last N entries"),
            ("-c", "clear after read"),
            ("-T", "human-readable timestamps"),
            ("-l", "filter by level"),
            ("-f", "filter by facility"),
            ("-w", "follow (like tail -f)"),
            ("-H", "human-readable output"),
            ("--color", "colorized output"),
        ],
        "lsmod" => &[],
        "modinfo" => &[
            ("-a", "author"),
            ("-d", "description"),
            ("-l", "license"),
            ("-p", "parameters"),
            ("-n", "filename"),
        ],
        "insmod" => &[],
        "rmmod" => &[("-f", "force removal"), ("-w", "wait until unused")],
        "lsblk" => &[
            ("-a", "show all devices"),
            ("-f", "show filesystem info"),
            ("-l", "list format"),
            ("-o", "output columns"),
            ("-p", "print full device path"),
            ("-n", "no headings"),
            ("-d", "no slaves/partitions"),
            ("-t", "topology info"),
            ("-J", "JSON output"),
        ],
        "mount" => &[
            ("-t", "filesystem type"),
            ("-o", "mount options"),
            ("-a", "mount all from fstab"),
            ("-r", "read-only"),
            ("-w", "read-write"),
            ("-v", "verbose"),
            ("-n", "don't write to mtab"),
            ("--bind", "bind mount"),
            ("--move", "move mount"),
        ],
        "umount" => &[
            ("-f", "force unmount"),
            ("-l", "lazy unmount"),
            ("-a", "unmount all"),
            ("-R", "recursive unmount"),
            ("-v", "verbose"),
            ("-n", "don't write to mtab"),
        ],
        "poweroff" => &[("-f", "force (no init)"), ("--halt", "halt instead")],
        "shutdown" => &[
            ("-h", "halt after shutdown"),
            ("-r", "reboot after shutdown"),
            ("-c", "cancel pending shutdown"),
            ("-k", "warn only, don't shutdown"),
            ("now", "immediate shutdown"),
        ],
        "reboot" => &[("-f", "force (no init)")],
        // ── Package manager ─────────────────────────────────────────
        "kpm" => &[
            ("install", "install package"),
            ("remove", "remove package"),
            ("update", "update package index"),
            ("upgrade", "upgrade packages"),
            ("search", "search packages"),
            ("list", "list installed"),
            ("info", "package information"),
        ],
        "apt" | "apt-get" => &[
            ("install", "install package"),
            ("remove", "remove package"),
            ("purge", "remove with config"),
            ("update", "update package list"),
            ("upgrade", "upgrade packages"),
            ("autoremove", "remove unused deps"),
            ("search", "search packages"),
            ("-y", "assume yes"),
            ("-q", "quiet output"),
        ],
        // ── Security ────────────────────────────────────────────────
        "lscgroup" => &[],
        "getenforce" => &[],
        "sestatus" => &[("-v", "verbose"), ("-b", "show booleans")],
        "lspci" => &[
            ("-v", "verbose"),
            ("-vv", "very verbose"),
            ("-t", "tree view"),
            ("-n", "show numeric IDs"),
            ("-k", "show kernel drivers"),
            ("-s", "show specific slot"),
        ],
        "glxinfo" => &[("-B", "brief output"), ("-v", "verbose")],
        "xrandr" => &[
            ("--query", "show current state"),
            ("--output", "select output"),
            ("--mode", "set resolution"),
            ("--rate", "set refresh rate"),
            ("--auto", "auto-configure"),
            ("--off", "disable output"),
            ("--rotate", "rotate output"),
        ],
        // ── Misc / system utilities ─────────────────────────────────
        "yes" => &[],
        "seq" => &[
            ("-s", "separator string"),
            ("-w", "equal width (zero pad)"),
            ("-f", "printf-style format"),
        ],
        "man" => &[
            ("-k", "search man pages (apropos)"),
            ("-f", "whatis (short description)"),
            ("-a", "show all matching pages"),
        ],
        "neofetch" => &[
            ("--off", "disable ASCII art"),
            ("--ascii", "force ASCII art"),
        ],
        "watch" => &[
            ("-n", "interval (seconds)"),
            ("-d", "highlight differences"),
            ("-t", "no title bar"),
            ("-e", "exit on error"),
            ("-c", "interpret ANSI colors"),
        ],
        "timeout" => &[
            ("-s", "signal to send"),
            ("-k", "kill after duration"),
            ("--foreground", "don't background"),
            ("--preserve-status", "keep exit status"),
        ],
        "nohup" => &[],
        "w" => &[("-h", "no header"), ("-s", "short format")],
        "who" => &[
            ("-a", "all information"),
            ("-b", "last system boot"),
            ("-H", "print headers"),
            ("-q", "count of logged-in users"),
        ],
        "last" => &[
            ("-n", "number of entries"),
            ("-a", "show hostname last"),
            ("-x", "show shutdown/runlevel entries"),
            ("-f", "use specific file"),
        ],
        "groups" => &[],
        "logname" => &[],
        "hostnamectl" => &[
            ("status", "show hostname info"),
            ("set-hostname", "set hostname"),
        ],
        "timedatectl" => &[
            ("status", "show time info"),
            ("set-time", "set system time"),
            ("set-timezone", "set timezone"),
            ("list-timezones", "list available zones"),
            ("set-ntp", "enable/disable NTP"),
        ],
        "locale" => &[("-a", "list all locales"), ("-m", "list charmaps")],
        "lsof" => &[
            ("-i", "network files"),
            ("-p", "by PID"),
            ("-u", "by user"),
            ("-c", "by command name"),
            ("-t", "terse (PIDs only)"),
            ("+D", "directory recursively"),
            ("-n", "no name resolution"),
        ],
        "vmstat" => &[
            ("-s", "table of stats"),
            ("-d", "disk statistics"),
            ("-w", "wide output"),
            ("-t", "add timestamp"),
            ("-a", "active/inactive memory"),
        ],
        "iostat" => &[
            ("-c", "CPU stats only"),
            ("-d", "device stats only"),
            ("-x", "extended stats"),
            ("-k", "kilobytes"),
            ("-m", "megabytes"),
            ("-t", "display time"),
            ("-p", "specific device"),
        ],
        "dstat" => &[
            ("-c", "CPU stats"),
            ("-d", "disk stats"),
            ("-n", "network stats"),
            ("-m", "memory stats"),
            ("-g", "page stats"),
            ("--top-cpu", "most expensive CPU process"),
            ("--top-mem", "most expensive memory process"),
        ],
        "top" | "htop" => &[
            ("-d", "update interval"),
            ("-p", "monitor specific PIDs"),
            ("-u", "filter by user"),
            ("-n", "number of iterations"),
            ("-b", "batch mode"),
            ("-H", "show threads"),
        ],
        "arch" => &[],
        "nproc" => &[("--all", "all installed CPUs")],
        "getconf" => &[],
        "reset" | "tset" => &[],
        "sync" => &[("-f", "sync specific filesystem"), ("-d", "sync data only")],
        // ── Phase 7/8 additions ─────────────────────────────────────
        "bind" => &[
            ("-p", "list all key bindings"),
            ("-l", "list readline functions"),
            ("-v", "list variables"),
        ],
        "save_history" => &[],
        "theme" => &[
            ("tokyo-night", "Tokyo Night (default)"),
            ("solarized-dark", "Solarized Dark"),
            ("monokai", "Monokai"),
            ("dracula", "Dracula"),
            ("--list", "list available themes"),
        ],
        "tmux" => &[
            ("new-session", "create new session"),
            ("ls", "list sessions"),
            ("attach", "attach to session"),
            ("detach", "detach from session"),
            ("kill-session", "kill a session"),
            ("split-window", "split pane"),
            ("-h", "horizontal split"),
            ("-v", "vertical split"),
            ("select-pane", "select pane"),
            ("new-window", "create window"),
            ("next-window", "go to next window"),
            ("prev-window", "go to previous window"),
        ],
        "ssh" => &[
            ("-p", "port number"),
            ("-i", "identity (key) file"),
            ("-l", "login user name"),
            ("-v", "verbose mode"),
            ("-o", "set option"),
            ("-L", "local port forwarding"),
            ("-R", "remote port forwarding"),
            ("-D", "dynamic (SOCKS) forwarding"),
            ("-N", "no remote command"),
            ("-f", "go to background"),
            ("-q", "quiet mode"),
            ("-X", "enable X11 forwarding"),
            ("--server", "server control (start|stop|status)"),
            ("--sessions", "list active sessions"),
        ],
        "setxkbmap" => &[
            ("us", "US English layout"),
            ("uk", "UK English layout"),
            ("de", "German layout"),
            ("fr", "French layout"),
            ("es", "Spanish layout"),
            ("dvorak", "Dvorak layout"),
        ],
        "compose" => &[("--list", "list compose pairs")],
        // ── Fallback ────────────────────────────────────────────────
        _ => &[],
    };

    for &(flag, desc) in flags {
        if flag.starts_with(prefix) {
            completions.push(Completion {
                text: String::from(flag),
                display: String::from(flag),
                kind: CompletionKind::Flag,
                description: String::from(desc),
            });
        }
    }
}

fn command_description(cmd: &str) -> String {
    String::from(match cmd {
        // Core shell builtins
        "echo" => "Display text",
        "cd" => "Change directory",
        "pwd" => "Print working directory",
        "export" => "Set environment variable",
        "unset" => "Unset variable",
        "env" | "printenv" => "Print environment",
        "set" => "Set shell options",
        "exit" | "logout" => "Exit shell",
        "history" => "Command history",
        "help" => "Show help",
        "type" | "which" => "Show command type/path",
        "alias" => "Define command alias",
        "unalias" => "Remove command alias",
        "true" => "Return success (0)",
        "false" => "Return failure (1)",
        "test" | "[" => "Evaluate conditional expression",
        "source" | "." => "Execute commands from file",
        // Job control
        "jobs" => "List active jobs",
        "fg" => "Resume job in foreground",
        "bg" => "Resume job in background",
        "wait" => "Wait for job completion",
        "disown" => "Remove job from table",
        // Phase 7/8
        "bind" => "Display or set key bindings",
        "save_history" => "Save history to disk",
        "theme" => "Switch terminal theme",
        "tmux" => "Terminal multiplexer",
        "ssh" => "Secure shell client/server",
        "setxkbmap" => "Set keyboard layout",
        "compose" => "Compose key sequences",
        // File operations
        "ls" => "List directory contents",
        "cat" => "Concatenate and display files",
        "touch" => "Create file or update timestamps",
        "mkdir" => "Create directory",
        "rm" => "Remove files or directories",
        "rmdir" => "Remove empty directories",
        "cp" => "Copy files and directories",
        "mv" => "Move or rename files",
        "head" => "Output first part of files",
        "tail" => "Output last part of files",
        "wc" => "Count lines, words, bytes",
        "grep" => "Search text patterns",
        "find" => "Find files in directory tree",
        "stat" => "Display file status",
        "tree" => "List directory tree",
        "ln" => "Create links",
        "readlink" => "Print resolved symbolic link",
        "realpath" => "Print resolved path",
        "basename" => "Strip directory from filename",
        "dirname" => "Strip filename from path",
        "chmod" => "Change file permissions",
        "chown" => "Change file owner",
        "du" => "Estimate disk usage",
        "mktemp" => "Create temporary file/dir",
        "file" => "Determine file type",
        "less" | "more" => "Page through text",
        "dd" => "Convert and copy file data",
        "tar" => "Archive files",
        "umask" => "Set file creation mask",
        // Text processing
        "tac" => "Reverse line order",
        "sort" => "Sort lines of text",
        "uniq" => "Filter duplicate lines",
        "cut" => "Cut selected fields from lines",
        "tr" => "Translate or delete characters",
        "rev" => "Reverse each line",
        "nl" => "Number lines",
        "fold" => "Wrap lines to fit width",
        "fmt" => "Reformat paragraph text",
        "sed" => "Stream editor",
        "awk" => "Pattern scanning and processing",
        "diff" => "Compare files line by line",
        "comm" => "Compare two sorted files",
        "paste" => "Merge lines of files",
        "join" => "Join lines on common field",
        "split" => "Split file into pieces",
        "tee" => "Duplicate stdin to stdout and files",
        "xargs" => "Build command lines from stdin",
        "strings" => "Find printable strings in files",
        "xxd" | "hexdump" => "Hex dump",
        "od" => "Octal dump",
        "base64" => "Base64 encode/decode",
        "md5sum" => "Compute MD5 checksum",
        "sha256sum" => "Compute SHA-256 checksum",
        "cksum" => "Compute CRC checksum",
        "expr" => "Evaluate expression",
        "bc" | "calc" => "Calculator",
        "printf" => "Format and print data",
        "column" => "Columnate lists",
        "expand" => "Convert tabs to spaces",
        "unexpand" => "Convert spaces to tabs",
        "cal" => "Display calendar",
        "factor" => "Print prime factors",
        "shuf" => "Shuffle lines",
        "numfmt" => "Convert numbers to/from SI/IEC",
        "tput" => "Set terminal capabilities",
        // System info
        "uname" => "System information",
        "hostname" => "Show/set hostname",
        "whoami" => "Print current user",
        "id" => "Print user/group IDs",
        "uptime" => "System uptime and load",
        "date" => "Display or set date/time",
        "free" => "Memory usage",
        "df" => "Disk free space",
        "ps" => "List processes",
        "kill" => "Send signal to process",
        "clear" => "Clear terminal screen",
        "sleep" => "Delay for a specified time",
        // Network
        "ifconfig" => "Configure network interface",
        "ip" => "Network configuration",
        "ping" => "Send ICMP echo requests",
        // GPU / display
        "lspci" => "List PCI devices",
        "glxinfo" => "Display OpenGL info",
        "xrandr" => "Display/output configuration",
        // Kernel management
        "dmesg" => "Kernel message buffer",
        "lsmod" => "List loaded modules",
        "modinfo" => "Module information",
        "insmod" => "Insert kernel module",
        "rmmod" => "Remove kernel module",
        "lsblk" => "List block devices",
        "mount" => "Mount filesystem",
        "umount" => "Unmount filesystem",
        "poweroff" => "Power off the system",
        "shutdown" => "Shutdown the system",
        "reboot" => "Reboot the system",
        // Package manager
        "kpm" => "KnoxOS package manager",
        "apt" | "apt-get" => "Package manager",
        // Security
        "lscgroup" => "List control groups",
        "getenforce" => "Get SELinux mode",
        "sestatus" => "SELinux status",
        // Misc / system utilities
        "yes" => "Repeatedly output a string",
        "seq" => "Print numeric sequence",
        "man" => "Display manual pages",
        "neofetch" => "System information display",
        "watch" => "Execute command periodically",
        "timeout" => "Run command with time limit",
        "nohup" => "Run immune to hangups",
        "w" => "Show who is logged in",
        "who" => "Show logged-in users",
        "last" => "Show last logins",
        "groups" => "Print group memberships",
        "logname" => "Print login name",
        "hostnamectl" => "Hostname control",
        "timedatectl" => "Time/date control",
        "locale" => "Locale information",
        "lsof" => "List open files",
        "vmstat" => "Virtual memory statistics",
        "iostat" => "I/O statistics",
        "dstat" => "System resource statistics",
        "top" | "htop" => "Interactive process viewer",
        "arch" => "Print machine architecture",
        "nproc" => "Print number of CPUs",
        "getconf" => "Get system configuration",
        "reset" | "tset" => "Reset terminal",
        "sync" => "Flush filesystem buffers",
        _ => "command",
    })
}
