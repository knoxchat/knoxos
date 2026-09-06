#![allow(
    clippy::manual_strip,
    clippy::manual_div_ceil,
    clippy::trim_split_whitespace,
    clippy::manual_range_contains,
    clippy::needless_range_loop,
    clippy::if_same_then_else,
    clippy::collapsible_if
)]
/// Text processing builtins — sort, uniq, cut, tr, sed, awk, rev, nl, fold, fmt,
/// tac, diff, comm, paste, join, split, tee, xargs, strings, xxd/hexdump, od,
/// base64, md5sum, sha256sum, cksum, expr, bc/calc, printf, column, expand, unexpand
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt::Write;

use crate::shell::helpers::resolve_path;
use crate::shell::types::ShellResult;

// ── tac ─────────────────────────────────────────────────────────
pub fn tac(args: &[String]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::err("tac: missing file operand");
    }
    let mut output = String::new();
    let vfs = crate::vfs::VFS.lock();
    for arg in args {
        let path = resolve_path(arg);
        if let Some(data) = vfs.read_file(&path) {
            if let Ok(s) = core::str::from_utf8(data) {
                let lines: Vec<&str> = s.lines().collect();
                for line in lines.iter().rev() {
                    writeln!(output, "{}", line).unwrap();
                }
            }
        } else {
            return ShellResult::err(&alloc::format!("tac: {}: No such file", arg));
        }
    }
    ShellResult::ok(&output)
}

// ── sort ────────────────────────────────────────────────────────
pub fn sort(args: &[String]) -> ShellResult {
    let mut reverse = false;
    let mut numeric = false;
    let mut unique = false;
    let mut files = Vec::new();

    for arg in args {
        match arg.as_str() {
            "-r" => reverse = true,
            "-n" => numeric = true,
            "-u" => unique = true,
            "-rn" | "-nr" => {
                reverse = true;
                numeric = true;
            }
            _ => files.push(arg.clone()),
        }
    }

    let vfs = crate::vfs::VFS.lock();
    let mut all_lines: Vec<String> = Vec::new();

    if files.is_empty() {
        return ShellResult::ok("");
    }

    for file in &files {
        let path = resolve_path(file);
        if let Some(data) = vfs.read_file(&path) {
            if let Ok(s) = core::str::from_utf8(data) {
                for line in s.lines() {
                    all_lines.push(line.to_string());
                }
            }
        } else {
            return ShellResult::err(&alloc::format!("sort: {}: No such file", file));
        }
    }

    if numeric {
        all_lines.sort_by(|a, b| {
            let na: i64 = a
                .trim()
                .split_whitespace()
                .next()
                .unwrap_or("0")
                .parse()
                .unwrap_or(0);
            let nb: i64 = b
                .trim()
                .split_whitespace()
                .next()
                .unwrap_or("0")
                .parse()
                .unwrap_or(0);
            na.cmp(&nb)
        });
    } else {
        all_lines.sort();
    }

    if reverse {
        all_lines.reverse();
    }

    if unique {
        all_lines.dedup();
    }

    let mut output = String::new();
    for line in &all_lines {
        writeln!(output, "{}", line).unwrap();
    }
    ShellResult::ok(&output)
}

// ── uniq ────────────────────────────────────────────────────────
pub fn uniq(args: &[String]) -> ShellResult {
    let mut count = false;
    let mut only_dups = false;
    let mut only_unique = false;
    let mut files = Vec::new();

    for arg in args {
        match arg.as_str() {
            "-c" => count = true,
            "-d" => only_dups = true,
            "-u" => only_unique = true,
            _ => files.push(arg.clone()),
        }
    }

    let vfs = crate::vfs::VFS.lock();
    let mut lines: Vec<String> = Vec::new();

    if files.is_empty() {
        return ShellResult::ok("");
    }

    for file in &files {
        let path = resolve_path(file);
        if let Some(data) = vfs.read_file(&path) {
            if let Ok(s) = core::str::from_utf8(data) {
                for line in s.lines() {
                    lines.push(line.to_string());
                }
            }
        }
    }

    let mut output = String::new();
    let mut i = 0;
    while i < lines.len() {
        let mut c = 1;
        while i + c < lines.len() && lines[i] == lines[i + c] {
            c += 1;
        }
        if only_dups && c == 1 {
            i += c;
            continue;
        }
        if only_unique && c > 1 {
            i += c;
            continue;
        }
        if count {
            writeln!(output, "{:>7} {}", c, lines[i]).unwrap();
        } else {
            writeln!(output, "{}", lines[i]).unwrap();
        }
        i += c;
    }
    ShellResult::ok(&output)
}

// ── cut ─────────────────────────────────────────────────────────
pub fn cut(args: &[String]) -> ShellResult {
    let mut delimiter = '\t';
    let mut fields: Vec<usize> = Vec::new();
    let mut files = Vec::new();
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "-d" if i + 1 < args.len() => {
                delimiter = args[i + 1].chars().next().unwrap_or('\t');
                i += 2;
                continue;
            }
            "-f" if i + 1 < args.len() => {
                for f in args[i + 1].split(',') {
                    if let Ok(n) = f.parse::<usize>() {
                        if n > 0 {
                            fields.push(n - 1);
                        }
                    }
                }
                i += 2;
                continue;
            }
            _ => files.push(args[i].clone()),
        }
        i += 1;
    }

    if fields.is_empty() {
        return ShellResult::err("cut: you must specify a list of fields");
    }

    let vfs = crate::vfs::VFS.lock();
    let mut output = String::new();

    for file in &files {
        let path = resolve_path(file);
        if let Some(data) = vfs.read_file(&path) {
            if let Ok(s) = core::str::from_utf8(data) {
                for line in s.lines() {
                    let parts: Vec<&str> = line.split(delimiter).collect();
                    let selected: Vec<&str> = fields
                        .iter()
                        .filter_map(|&f| parts.get(f).copied())
                        .collect();
                    writeln!(output, "{}", selected.join(&delimiter.to_string())).unwrap();
                }
            }
        }
    }
    ShellResult::ok(&output)
}

// ── tr ──────────────────────────────────────────────────────────
pub fn tr(args: &[String]) -> ShellResult {
    if args.len() < 2 {
        return ShellResult::err("tr: missing operand\nUsage: tr SET1 SET2");
    }
    let set1: Vec<char> = args[0].chars().collect();
    let set2: Vec<char> = args[1].chars().collect();
    // tr with no stdin just shows usage in this context
    ShellResult::ok("tr: reads from stdin (use with pipes: echo hello | tr a-z A-Z)")
}

// ── rev ─────────────────────────────────────────────────────────
pub fn rev(args: &[String]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::ok("");
    }
    let vfs = crate::vfs::VFS.lock();
    let mut output = String::new();
    for arg in args {
        let path = resolve_path(arg);
        if let Some(data) = vfs.read_file(&path) {
            if let Ok(s) = core::str::from_utf8(data) {
                for line in s.lines() {
                    let reversed: String = line.chars().rev().collect();
                    writeln!(output, "{}", reversed).unwrap();
                }
            }
        }
    }
    ShellResult::ok(&output)
}

// ── nl ──────────────────────────────────────────────────────────
pub fn nl(args: &[String]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::err("nl: missing file operand");
    }
    let vfs = crate::vfs::VFS.lock();
    let mut output = String::new();
    let mut num = 1;
    for arg in args {
        let path = resolve_path(arg);
        if let Some(data) = vfs.read_file(&path) {
            if let Ok(s) = core::str::from_utf8(data) {
                for line in s.lines() {
                    if line.trim().is_empty() {
                        writeln!(output, "       {}", line).unwrap();
                    } else {
                        writeln!(output, "{:>6}\t{}", num, line).unwrap();
                        num += 1;
                    }
                }
            }
        }
    }
    ShellResult::ok(&output)
}

// ── fold ────────────────────────────────────────────────────────
pub fn fold(args: &[String]) -> ShellResult {
    let mut width = 80usize;
    let mut files = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-w" if i + 1 < args.len() => {
                width = args[i + 1].parse().unwrap_or(80);
                i += 2;
                continue;
            }
            _ => files.push(args[i].clone()),
        }
        i += 1;
    }
    let vfs = crate::vfs::VFS.lock();
    let mut output = String::new();
    for file in &files {
        let path = resolve_path(file);
        if let Some(data) = vfs.read_file(&path) {
            if let Ok(s) = core::str::from_utf8(data) {
                for line in s.lines() {
                    let chars: Vec<char> = line.chars().collect();
                    for chunk in chars.chunks(width) {
                        let s: String = chunk.iter().collect();
                        writeln!(output, "{}", s).unwrap();
                    }
                }
            }
        }
    }
    ShellResult::ok(&output)
}

// ── fmt ─────────────────────────────────────────────────────────
pub fn fmt(args: &[String]) -> ShellResult {
    let mut width = 75usize;
    let mut files = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-w" if i + 1 < args.len() => {
                width = args[i + 1].parse().unwrap_or(75);
                i += 2;
                continue;
            }
            _ => files.push(args[i].clone()),
        }
        i += 1;
    }
    let vfs = crate::vfs::VFS.lock();
    let mut output = String::new();
    for file in &files {
        let path = resolve_path(file);
        if let Some(data) = vfs.read_file(&path) {
            if let Ok(s) = core::str::from_utf8(data) {
                let words: Vec<&str> = s.split_whitespace().collect();
                let mut line_len = 0;
                for word in &words {
                    if line_len + word.len() + 1 > width && line_len > 0 {
                        writeln!(output).unwrap();
                        line_len = 0;
                    }
                    if line_len > 0 {
                        write!(output, " ").unwrap();
                        line_len += 1;
                    }
                    write!(output, "{}", word).unwrap();
                    line_len += word.len();
                }
                if line_len > 0 {
                    writeln!(output).unwrap();
                }
            }
        }
    }
    ShellResult::ok(&output)
}

// ── sed (basic s///g) ───────────────────────────────────────────
pub fn sed(args: &[String]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::err(
            "sed: missing script\nUsage: sed 's/pattern/replacement/g' <file>",
        );
    }
    // Find the script and files
    let mut script = String::new();
    let mut files = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "-e" && i + 1 < args.len() {
            script = args[i + 1].clone();
            i += 2;
            continue;
        } else if script.is_empty() && (args[i].starts_with("s/") || args[i].starts_with("s|")) {
            script = args[i].clone();
        } else {
            files.push(args[i].clone());
        }
        i += 1;
    }

    if script.is_empty() {
        return ShellResult::err("sed: no script provided");
    }

    // Parse s/pattern/replacement/flags
    let sep = if script.len() > 1 {
        script.as_bytes()[1] as char
    } else {
        '/'
    };
    let parts: Vec<&str> = script[2..].splitn(3, sep).collect();
    if parts.len() < 2 {
        return ShellResult::err("sed: invalid script");
    }
    let pattern = parts[0];
    let replacement = parts[1];
    let flags = if parts.len() > 2 { parts[2] } else { "" };
    let global = flags.contains('g');

    let vfs = crate::vfs::VFS.lock();
    let mut output = String::new();
    for file in &files {
        let path = resolve_path(file);
        if let Some(data) = vfs.read_file(&path) {
            if let Ok(s) = core::str::from_utf8(data) {
                for line in s.lines() {
                    let new_line = if global {
                        line.replace(pattern, replacement)
                    } else {
                        line.replacen(pattern, replacement, 1)
                    };
                    writeln!(output, "{}", new_line).unwrap();
                }
            }
        } else {
            return ShellResult::err(&alloc::format!("sed: {}: No such file", file));
        }
    }
    ShellResult::ok(&output)
}

// ── awk (basic field extraction) ────────────────────────────────
pub fn awk(args: &[String]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::err("awk: missing program\nUsage: awk '{print $1}' <file>");
    }

    let mut program = String::new();
    let mut files = Vec::new();
    let mut field_sep = " ";
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "-F" if i + 1 < args.len() => {
                field_sep = &args[i + 1];
                i += 2;
                continue;
            }
            _ => {
                if program.is_empty() {
                    program = args[i].clone();
                } else {
                    files.push(args[i].clone());
                }
            }
        }
        i += 1;
    }

    // Parse simple awk programs: '{print $N}' or '{print $N, $M}'
    let prog = program.trim_matches(|c| c == '\'' || c == '{' || c == '}');
    let is_print = prog.starts_with("print ");

    if !is_print {
        return ShellResult::ok("awk: only '{print $N}' patterns are supported in ksh");
    }

    let field_refs: Vec<&str> = prog[6..].split(',').map(|s| s.trim()).collect();

    let vfs = crate::vfs::VFS.lock();
    let mut output = String::new();

    for file in &files {
        let path = resolve_path(file);
        if let Some(data) = vfs.read_file(&path) {
            if let Ok(s) = core::str::from_utf8(data) {
                for line in s.lines() {
                    let fields: Vec<&str> = if field_sep == " " {
                        line.split_whitespace().collect()
                    } else {
                        line.split(field_sep).collect()
                    };

                    let mut parts = Vec::new();
                    for fref in &field_refs {
                        if *fref == "$0" {
                            parts.push(line.to_string());
                        } else if fref.starts_with('$') {
                            if let Ok(n) = fref[1..].parse::<usize>() {
                                if n > 0 && n <= fields.len() {
                                    parts.push(fields[n - 1].to_string());
                                } else {
                                    parts.push(String::new());
                                }
                            } else if fref.starts_with("$NF") {
                                if let Some(last) = fields.last() {
                                    parts.push(last.to_string());
                                }
                            }
                        } else {
                            // Literal string
                            parts.push(fref.trim_matches('"').to_string());
                        }
                    }
                    writeln!(output, "{}", parts.join(" ")).unwrap();
                }
            }
        }
    }
    ShellResult::ok(&output)
}

// ── diff ────────────────────────────────────────────────────────
pub fn diff(args: &[String]) -> ShellResult {
    if args.len() < 2 {
        return ShellResult::err("diff: missing operand\nUsage: diff <file1> <file2>");
    }
    let vfs = crate::vfs::VFS.lock();
    let path1 = resolve_path(&args[args.len() - 2]);
    let path2 = resolve_path(&args[args.len() - 1]);

    let data1 = match vfs.read_file(&path1) {
        Some(d) => core::str::from_utf8(d).unwrap_or("").to_string(),
        None => {
            return ShellResult::err(&alloc::format!(
                "diff: {}: No such file",
                args[args.len() - 2]
            ));
        }
    };
    let data2 = match vfs.read_file(&path2) {
        Some(d) => core::str::from_utf8(d).unwrap_or("").to_string(),
        None => {
            return ShellResult::err(&alloc::format!(
                "diff: {}: No such file",
                args[args.len() - 1]
            ));
        }
    };

    let lines1: Vec<&str> = data1.lines().collect();
    let lines2: Vec<&str> = data2.lines().collect();

    let mut output = String::new();
    let mut has_diff = false;

    // Simple line-by-line diff (unified style)
    writeln!(output, "--- {}", path1).unwrap();
    writeln!(output, "+++ {}", path2).unwrap();

    let max = lines1.len().max(lines2.len());
    for i in 0..max {
        let l1 = lines1.get(i).copied().unwrap_or("");
        let l2 = lines2.get(i).copied().unwrap_or("");
        if l1 != l2 {
            has_diff = true;
            if i < lines1.len() {
                writeln!(output, "-{}", l1).unwrap();
            }
            if i < lines2.len() {
                writeln!(output, "+{}", l2).unwrap();
            }
        }
    }

    if !has_diff {
        return ShellResult::ok("");
    }
    ShellResult::ok(&output)
}

// ── comm ────────────────────────────────────────────────────────
pub fn comm(args: &[String]) -> ShellResult {
    if args.len() < 2 {
        return ShellResult::err("comm: missing operand");
    }
    let vfs = crate::vfs::VFS.lock();
    let path1 = resolve_path(&args[args.len() - 2]);
    let path2 = resolve_path(&args[args.len() - 1]);

    let data1 = vfs
        .read_file(&path1)
        .map(|d| core::str::from_utf8(d).unwrap_or("").to_string())
        .unwrap_or_default();
    let data2 = vfs
        .read_file(&path2)
        .map(|d| core::str::from_utf8(d).unwrap_or("").to_string())
        .unwrap_or_default();

    let lines1: Vec<&str> = data1.lines().collect();
    let lines2: Vec<&str> = data2.lines().collect();

    let mut output = String::new();
    let (mut i, mut j) = (0, 0);
    while i < lines1.len() || j < lines2.len() {
        if i >= lines1.len() {
            writeln!(output, "\t\t{}", lines2[j]).unwrap();
            j += 1;
        } else if j >= lines2.len() {
            writeln!(output, "{}", lines1[i]).unwrap();
            i += 1;
        } else if lines1[i] < lines2[j] {
            writeln!(output, "{}", lines1[i]).unwrap();
            i += 1;
        } else if lines1[i] > lines2[j] {
            writeln!(output, "\t{}", lines2[j]).unwrap();
            j += 1;
        } else {
            writeln!(output, "\t\t{}", lines1[i]).unwrap();
            i += 1;
            j += 1;
        }
    }
    ShellResult::ok(&output)
}

// ── paste ───────────────────────────────────────────────────────
pub fn paste(args: &[String]) -> ShellResult {
    let mut delim = "\t";
    let mut files = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-d" if i + 1 < args.len() => {
                delim = &args[i + 1];
                i += 2;
                continue;
            }
            _ => files.push(args[i].clone()),
        }
        i += 1;
    }

    let vfs = crate::vfs::VFS.lock();
    let mut file_lines: Vec<Vec<String>> = Vec::new();
    let mut max_lines = 0;

    for file in &files {
        let path = resolve_path(file);
        if let Some(data) = vfs.read_file(&path) {
            if let Ok(s) = core::str::from_utf8(data) {
                let lines: Vec<String> = s.lines().map(|l| l.to_string()).collect();
                if lines.len() > max_lines {
                    max_lines = lines.len();
                }
                file_lines.push(lines);
            }
        }
    }

    let mut output = String::new();
    for i in 0..max_lines {
        let parts: Vec<String> = file_lines
            .iter()
            .map(|lines| lines.get(i).cloned().unwrap_or_default())
            .collect();
        writeln!(output, "{}", parts.join(delim)).unwrap();
    }
    ShellResult::ok(&output)
}

// ── join ────────────────────────────────────────────────────────
pub fn join(args: &[String]) -> ShellResult {
    if args.len() < 2 {
        return ShellResult::err("join: missing operand");
    }
    let vfs = crate::vfs::VFS.lock();
    let path1 = resolve_path(&args[0]);
    let path2 = resolve_path(&args[1]);

    let data1 = vfs
        .read_file(&path1)
        .map(|d| core::str::from_utf8(d).unwrap_or("").to_string())
        .unwrap_or_default();
    let data2 = vfs
        .read_file(&path2)
        .map(|d| core::str::from_utf8(d).unwrap_or("").to_string())
        .unwrap_or_default();

    let mut output = String::new();
    for line1 in data1.lines() {
        let key1 = line1.split_whitespace().next().unwrap_or("");
        for line2 in data2.lines() {
            let key2 = line2.split_whitespace().next().unwrap_or("");
            if key1 == key2 {
                writeln!(
                    output,
                    "{} {}",
                    line1,
                    line2
                        .split_whitespace()
                        .skip(1)
                        .collect::<Vec<_>>()
                        .join(" ")
                )
                .unwrap();
            }
        }
    }
    ShellResult::ok(&output)
}

// ── split ───────────────────────────────────────────────────────
pub fn split(args: &[String]) -> ShellResult {
    let mut lines_per = 1000usize;
    let mut file = String::new();
    let mut prefix = String::from("x");
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-l" if i + 1 < args.len() => {
                lines_per = args[i + 1].parse().unwrap_or(1000);
                i += 2;
                continue;
            }
            _ => {
                if file.is_empty() {
                    file = args[i].clone();
                } else {
                    prefix = args[i].clone();
                }
            }
        }
        i += 1;
    }
    if file.is_empty() {
        return ShellResult::err("split: missing file operand");
    }
    let all_lines: Vec<String> = {
        let vfs = crate::vfs::VFS.lock();
        let path = resolve_path(&file);
        if let Some(data) = vfs.read_file(&path) {
            if let Ok(s) = core::str::from_utf8(data) {
                s.lines().map(|l| l.to_string()).collect()
            } else {
                return ShellResult::err(&alloc::format!("split: {}: binary file", file));
            }
        } else {
            return ShellResult::err(&alloc::format!("split: {}: No such file", file));
        }
    };
    let chunks = (all_lines.len() + lines_per - 1) / lines_per;
    let mut mvfs = crate::vfs::VFS.lock();
    for c in 0..chunks {
        let start = c * lines_per;
        let end = ((c + 1) * lines_per).min(all_lines.len());
        let suffix = alloc::format!(
            "{}{}",
            (b'a' + (c / 26) as u8) as char,
            (b'a' + (c % 26) as u8) as char
        );
        let chunk_path = alloc::format!("{}{}", prefix, suffix);
        let content: Vec<&str> = all_lines[start..end].iter().map(|s| s.as_str()).collect();
        let joined = content.join("\n");
        mvfs.write_file(&chunk_path, joined.as_bytes());
    }
    ShellResult::ok(&alloc::format!("split: created {} files", chunks))
}

// ── tee ─────────────────────────────────────────────────────────
pub fn tee(args: &[String]) -> ShellResult {
    // tee needs stdin from pipe; in ksh it just shows usage
    let append = args.contains(&String::from("-a"));
    let files: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();
    if files.is_empty() {
        return ShellResult::ok(
            "tee: reads from stdin, writes to stdout and files (use with pipes)",
        );
    }
    ShellResult::ok("tee: use with pipes: command | tee file1 file2")
}

// ── xargs ───────────────────────────────────────────────────────
pub fn xargs(_args: &[String]) -> ShellResult {
    ShellResult::ok("xargs: reads from stdin and builds command lines (use with pipes)")
}

// ── strings ─────────────────────────────────────────────────────
pub fn strings(args: &[String]) -> ShellResult {
    let min_len = 4;
    let mut files = Vec::new();
    for arg in args {
        files.push(arg.clone());
    }
    if files.is_empty() {
        return ShellResult::err("strings: missing file operand");
    }
    let vfs = crate::vfs::VFS.lock();
    let mut output = String::new();
    for file in &files {
        let path = resolve_path(file);
        if let Some(data) = vfs.read_file(&path) {
            let mut current = String::new();
            for &b in data {
                if b >= 0x20 && b < 0x7f {
                    current.push(b as char);
                } else {
                    if current.len() >= min_len {
                        writeln!(output, "{}", current).unwrap();
                    }
                    current.clear();
                }
            }
            if current.len() >= min_len {
                writeln!(output, "{}", current).unwrap();
            }
        }
    }
    ShellResult::ok(&output)
}

// ── xxd / hexdump ───────────────────────────────────────────────
pub fn xxd(args: &[String]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::err("xxd: missing file operand");
    }
    let vfs = crate::vfs::VFS.lock();
    let path = resolve_path(&args[args.len() - 1]);
    if let Some(data) = vfs.read_file(&path) {
        let mut output = String::new();
        let limit = data.len().min(256); // limit output
        for i in (0..limit).step_by(16) {
            write!(output, "{:08x}: ", i).unwrap();
            for j in 0..16 {
                if i + j < data.len() {
                    write!(output, "{:02x}", data[i + j]).unwrap();
                } else {
                    write!(output, "  ").unwrap();
                }
                if j % 2 == 1 {
                    write!(output, " ").unwrap();
                }
            }
            write!(output, " ").unwrap();
            for j in 0..16 {
                if i + j < data.len() {
                    let c = data[i + j];
                    if c >= 0x20 && c < 0x7f {
                        write!(output, "{}", c as char).unwrap();
                    } else {
                        write!(output, ".").unwrap();
                    }
                }
            }
            writeln!(output).unwrap();
        }
        if data.len() > limit {
            writeln!(output, "... ({} bytes total)", data.len()).unwrap();
        }
        ShellResult::ok(&output)
    } else {
        ShellResult::err(&alloc::format!(
            "xxd: {}: No such file",
            args[args.len() - 1]
        ))
    }
}

// ── od ──────────────────────────────────────────────────────────
pub fn od(args: &[String]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::err("od: missing file operand");
    }
    let vfs = crate::vfs::VFS.lock();
    let file = args
        .iter()
        .find(|a| !a.starts_with('-'))
        .unwrap_or(&args[0]);
    let path = resolve_path(file);
    if let Some(data) = vfs.read_file(&path) {
        let mut output = String::new();
        let limit = data.len().min(256);
        for i in (0..limit).step_by(16) {
            write!(output, "{:07o}", i).unwrap();
            for j in 0..16 {
                if i + j < data.len() {
                    write!(output, " {:03o}", data[i + j]).unwrap();
                }
            }
            writeln!(output).unwrap();
        }
        write!(output, "{:07o}", data.len()).unwrap();
        writeln!(output).unwrap();
        ShellResult::ok(&output)
    } else {
        ShellResult::err(&alloc::format!("od: {}: No such file", file))
    }
}

// ── base64 ──────────────────────────────────────────────────────
pub fn base64(args: &[String]) -> ShellResult {
    let decode = args.contains(&String::from("-d")) || args.contains(&String::from("--decode"));
    let files: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();
    if files.is_empty() {
        return ShellResult::err("base64: missing file operand");
    }
    let vfs = crate::vfs::VFS.lock();
    let path = resolve_path(files[0]);
    if let Some(data) = vfs.read_file(&path) {
        if decode {
            if let Ok(s) = core::str::from_utf8(data) {
                match base64_decode(s.trim()) {
                    Ok(decoded) => {
                        let text = core::str::from_utf8(&decoded).unwrap_or("(binary data)");
                        ShellResult::ok(text)
                    }
                    Err(e) => ShellResult::err(&alloc::format!("base64: {}", e)),
                }
            } else {
                ShellResult::err("base64: invalid input")
            }
        } else {
            let encoded = base64_encode(data);
            ShellResult::ok(&encoded)
        }
    } else {
        ShellResult::err(&alloc::format!("base64: {}: No such file", files[0]))
    }
}

fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    let chunks = data.chunks(3);
    for chunk in chunks {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(TABLE[((triple >> 18) & 0x3F) as usize] as char);
        result.push(TABLE[((triple >> 12) & 0x3F) as usize] as char);
        if chunk.len() > 1 {
            result.push(TABLE[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(TABLE[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

fn base64_decode(input: &str) -> Result<Vec<u8>, &'static str> {
    fn val(c: u8) -> Result<u8, &'static str> {
        match c {
            b'A'..=b'Z' => Ok(c - b'A'),
            b'a'..=b'z' => Ok(c - b'a' + 26),
            b'0'..=b'9' => Ok(c - b'0' + 52),
            b'+' => Ok(62),
            b'/' => Ok(63),
            _ => Err("base64: invalid character"),
        }
    }
    let input: Vec<u8> = input
        .bytes()
        .filter(|&b| b != b'\n' && b != b'\r')
        .collect();
    let mut result = Vec::new();
    for chunk in input.chunks(4) {
        if chunk.len() < 2 {
            break;
        }
        let a = val(chunk[0])?;
        let b = val(chunk[1])?;
        result.push((a << 2) | (b >> 4));
        if chunk.len() > 2 && chunk[2] != b'=' {
            let c = val(chunk[2])?;
            result.push(((b & 0xF) << 4) | (c >> 2));
            if chunk.len() > 3 && chunk[3] != b'=' {
                let d = val(chunk[3])?;
                result.push(((c & 0x3) << 6) | d);
            }
        }
    }
    Ok(result)
}

// ── md5sum (simple hash, not cryptographic) ─────────────────────
pub fn md5sum(args: &[String]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::err("md5sum: missing file operand");
    }
    let vfs = crate::vfs::VFS.lock();
    let mut output = String::new();
    for arg in args {
        let path = resolve_path(arg);
        if let Some(data) = vfs.read_file(&path) {
            let hash = simple_hash(data);
            writeln!(
                output,
                "{:016x}{:016x}  {}",
                hash,
                hash.wrapping_mul(0x517cc1b727220a95),
                arg
            )
            .unwrap();
        } else {
            return ShellResult::err(&alloc::format!("md5sum: {}: No such file", arg));
        }
    }
    ShellResult::ok(&output)
}

// ── sha256sum ───────────────────────────────────────────────────
pub fn sha256sum(args: &[String]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::err("sha256sum: missing file operand");
    }
    let vfs = crate::vfs::VFS.lock();
    let mut output = String::new();
    for arg in args {
        let path = resolve_path(arg);
        if let Some(data) = vfs.read_file(&path) {
            let h = simple_hash(data);
            let h2 = h.wrapping_mul(0x517cc1b727220a95);
            let h3 = h.wrapping_mul(0x6c62272e07bb0142);
            let h4 = h.wrapping_mul(0x846ca68b08cfa3dc);
            writeln!(
                output,
                "{:016x}{:016x}{:016x}{:016x}  {}",
                h, h2, h3, h4, arg
            )
            .unwrap();
        } else {
            return ShellResult::err(&alloc::format!("sha256sum: {}: No such file", arg));
        }
    }
    ShellResult::ok(&output)
}

// ── cksum ───────────────────────────────────────────────────────
pub fn cksum(args: &[String]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::err("cksum: missing file operand");
    }
    let vfs = crate::vfs::VFS.lock();
    let mut output = String::new();
    for arg in args {
        let path = resolve_path(arg);
        if let Some(data) = vfs.read_file(&path) {
            let hash = simple_hash(data) as u32;
            writeln!(output, "{} {} {}", hash, data.len(), arg).unwrap();
        } else {
            return ShellResult::err(&alloc::format!("cksum: {}: No such file", arg));
        }
    }
    ShellResult::ok(&output)
}

fn simple_hash(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

// ── expr ────────────────────────────────────────────────────────
pub fn expr(args: &[String]) -> ShellResult {
    if args.len() == 3 {
        let a: i64 = args[0].parse().unwrap_or(0);
        let b: i64 = args[2].parse().unwrap_or(0);
        let result = match args[1].as_str() {
            "+" => a + b,
            "-" => a - b,
            "*" => a * b,
            "/" => {
                if b != 0 {
                    a / b
                } else {
                    return ShellResult::err("expr: division by zero");
                }
            }
            "%" => {
                if b != 0 {
                    a % b
                } else {
                    return ShellResult::err("expr: division by zero");
                }
            }
            "=" => {
                if a == b {
                    1
                } else {
                    0
                }
            }
            "!=" => {
                if a != b {
                    1
                } else {
                    0
                }
            }
            "<" => {
                if a < b {
                    1
                } else {
                    0
                }
            }
            ">" => {
                if a > b {
                    1
                } else {
                    0
                }
            }
            "<=" => {
                if a <= b {
                    1
                } else {
                    0
                }
            }
            ">=" => {
                if a >= b {
                    1
                } else {
                    0
                }
            }
            _ => return ShellResult::err(&alloc::format!("expr: unknown operator: {}", args[1])),
        };
        ShellResult::ok(&alloc::format!("{}", result))
    } else if args.len() == 1 {
        ShellResult::ok(&args[0])
    } else {
        ShellResult::err("expr: syntax error")
    }
}

// ── bc / calc ───────────────────────────────────────────────────
pub fn bc(args: &[String]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::ok("bc: interactive mode not available; use: bc 'expression'");
    }
    let expr_str = args.join(" ");
    // Simple expression evaluator
    let expr_str = expr_str.trim().trim_matches('\'').trim_matches('"');

    // Handle simple integer operations
    for op in &["+", "-", "*", "/", "%"] {
        if let Some(pos) = expr_str.rfind(op) {
            if pos > 0 {
                let left: i64 = expr_str[..pos].trim().parse().unwrap_or(0);
                let right: i64 = expr_str[pos + 1..].trim().parse().unwrap_or(0);
                let result = match *op {
                    "+" => left + right,
                    "-" => left - right,
                    "*" => left * right,
                    "/" => {
                        if right != 0 {
                            left / right
                        } else {
                            return ShellResult::err("bc: division by zero");
                        }
                    }
                    "%" => {
                        if right != 0 {
                            left % right
                        } else {
                            return ShellResult::err("bc: division by zero");
                        }
                    }
                    _ => 0,
                };
                return ShellResult::ok(&alloc::format!("{}", result));
            }
        }
    }

    // Try as a plain number
    if let Ok(n) = expr_str.parse::<i64>() {
        return ShellResult::ok(&alloc::format!("{}", n));
    }

    ShellResult::err("bc: parse error")
}

// ── printf ──────────────────────────────────────────────────────
pub fn printf(args: &[String]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::err("printf: missing format string");
    }
    let fmt_str = &args[0];
    let mut output = String::new();
    let mut arg_idx = 1;
    let chars: Vec<char> = fmt_str.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() {
            match chars[i + 1] {
                'n' => output.push('\n'),
                't' => output.push('\t'),
                '\\' => output.push('\\'),
                '"' => output.push('"'),
                _ => {
                    output.push('\\');
                    output.push(chars[i + 1]);
                }
            }
            i += 2;
        } else if chars[i] == '%' && i + 1 < chars.len() {
            match chars[i + 1] {
                's' => {
                    if arg_idx < args.len() {
                        output.push_str(&args[arg_idx]);
                        arg_idx += 1;
                    }
                    i += 2;
                }
                'd' => {
                    if arg_idx < args.len() {
                        let n: i64 = args[arg_idx].parse().unwrap_or(0);
                        write!(output, "{}", n).unwrap();
                        arg_idx += 1;
                    }
                    i += 2;
                }
                '%' => {
                    output.push('%');
                    i += 2;
                }
                _ => {
                    output.push(chars[i]);
                    i += 1;
                }
            }
        } else {
            output.push(chars[i]);
            i += 1;
        }
    }

    ShellResult::ok(&output)
}

// ── column ──────────────────────────────────────────────────────
pub fn column(args: &[String]) -> ShellResult {
    let table_mode = args.contains(&String::from("-t"));
    let files: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();
    if files.is_empty() {
        return ShellResult::ok("column: reads from stdin (use with pipes or provide a file)");
    }
    let vfs = crate::vfs::VFS.lock();
    let mut all_lines = Vec::new();
    for file in &files {
        let path = resolve_path(file);
        if let Some(data) = vfs.read_file(&path) {
            if let Ok(s) = core::str::from_utf8(data) {
                for line in s.lines() {
                    all_lines.push(line.to_string());
                }
            }
        }
    }

    if !table_mode {
        let mut output = String::new();
        for line in &all_lines {
            writeln!(output, "{}", line).unwrap();
        }
        return ShellResult::ok(&output);
    }

    // Table mode: align columns
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut max_widths: Vec<usize> = Vec::new();
    for line in &all_lines {
        let fields: Vec<String> = line.split_whitespace().map(|s| s.to_string()).collect();
        for (i, f) in fields.iter().enumerate() {
            if i >= max_widths.len() {
                max_widths.push(0);
            }
            if f.len() > max_widths[i] {
                max_widths[i] = f.len();
            }
        }
        rows.push(fields);
    }

    let mut output = String::new();
    for row in &rows {
        for (i, field) in row.iter().enumerate() {
            if i > 0 {
                write!(output, "  ").unwrap();
            }
            write!(output, "{}", field).unwrap();
            if i < row.len() - 1 {
                let pad = max_widths.get(i).unwrap_or(&0).saturating_sub(field.len());
                for _ in 0..pad {
                    write!(output, " ").unwrap();
                }
            }
        }
        writeln!(output).unwrap();
    }
    ShellResult::ok(&output)
}

// ── expand / unexpand ───────────────────────────────────────────
pub fn expand(args: &[String]) -> ShellResult {
    let tab_width = 8;
    let files: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();
    if files.is_empty() {
        return ShellResult::err("expand: missing file operand");
    }
    let vfs = crate::vfs::VFS.lock();
    let mut output = String::new();
    for file in &files {
        let path = resolve_path(file);
        if let Some(data) = vfs.read_file(&path) {
            if let Ok(s) = core::str::from_utf8(data) {
                for line in s.lines() {
                    let expanded = line.replace('\t', &" ".repeat(tab_width));
                    writeln!(output, "{}", expanded).unwrap();
                }
            }
        }
    }
    ShellResult::ok(&output)
}

pub fn unexpand(args: &[String]) -> ShellResult {
    let tab_width = 8;
    let files: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();
    if files.is_empty() {
        return ShellResult::err("unexpand: missing file operand");
    }
    let vfs = crate::vfs::VFS.lock();
    let mut output = String::new();
    for file in &files {
        let path = resolve_path(file);
        if let Some(data) = vfs.read_file(&path) {
            if let Ok(s) = core::str::from_utf8(data) {
                for line in s.lines() {
                    let spaces = " ".repeat(tab_width);
                    let unexpanded = line.replace(&spaces, "\t");
                    writeln!(output, "{}", unexpanded).unwrap();
                }
            }
        }
    }
    ShellResult::ok(&output)
}

// ── cal ─────────────────────────────────────────────────────────
pub fn cal(_args: &[String]) -> ShellResult {
    // Show a simple calendar for current month
    let mut output = String::new();
    writeln!(output, "      2026").unwrap();
    writeln!(output, "Su Mo Tu We Th Fr Sa").unwrap();
    writeln!(output, "                   1").unwrap();
    writeln!(output, " 2  3  4  5  6  7  8").unwrap();
    writeln!(output, " 9 10 11 12 13 14 15").unwrap();
    writeln!(output, "16 17 18 19 20 21 22").unwrap();
    writeln!(output, "23 24 25 26 27 28").unwrap();
    ShellResult::ok(&output)
}

// ── factor ──────────────────────────────────────────────────────
pub fn factor(args: &[String]) -> ShellResult {
    let mut output = String::new();
    for arg in args {
        if let Ok(mut n) = arg.parse::<u64>() {
            write!(output, "{}:", n).unwrap();
            let orig = n;
            let mut d = 2u64;
            while d * d <= n {
                while n % d == 0 {
                    write!(output, " {}", d).unwrap();
                    n /= d;
                }
                d += 1;
            }
            if n > 1 {
                write!(output, " {}", n).unwrap();
            }
            if orig <= 1 {
                // no factors
            }
            writeln!(output).unwrap();
        } else {
            return ShellResult::err(&alloc::format!("factor: '{}' is not a valid number", arg));
        }
    }
    ShellResult::ok(&output)
}

// ── shuf ────────────────────────────────────────────────────────
pub fn shuf(args: &[String]) -> ShellResult {
    // Simple shuf: shuf -i LO-HI -n COUNT or shuf <file>
    let mut lo = 0i64;
    let mut hi = 0i64;
    let mut count = 0usize;
    let mut has_range = false;
    let mut files = Vec::new();
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "-i" if i + 1 < args.len() => {
                if let Some(dash) = args[i + 1].find('-') {
                    lo = args[i + 1][..dash].parse().unwrap_or(0);
                    hi = args[i + 1][dash + 1..].parse().unwrap_or(0);
                    has_range = true;
                }
                i += 2;
                continue;
            }
            "-n" if i + 1 < args.len() => {
                count = args[i + 1].parse().unwrap_or(0);
                i += 2;
                continue;
            }
            _ => files.push(args[i].clone()),
        }
        i += 1;
    }

    let mut output = String::new();

    if has_range {
        let mut nums: Vec<i64> = (lo..=hi).collect();
        // Simple shuffle using varying step
        let n = nums.len();
        for j in (1..n).rev() {
            let k = (j * 7 + 3) % (j + 1); // deterministic pseudo-shuffle
            nums.swap(j, k);
        }
        let limit = if count > 0 {
            count.min(nums.len())
        } else {
            nums.len()
        };
        for k in 0..limit {
            writeln!(output, "{}", nums[k]).unwrap();
        }
    } else if !files.is_empty() {
        let vfs = crate::vfs::VFS.lock();
        let mut lines = Vec::new();
        for file in &files {
            let path = resolve_path(file);
            if let Some(data) = vfs.read_file(&path) {
                if let Ok(s) = core::str::from_utf8(data) {
                    for line in s.lines() {
                        lines.push(line.to_string());
                    }
                }
            }
        }
        let n = lines.len();
        for j in (1..n).rev() {
            let k = (j * 7 + 3) % (j + 1);
            lines.swap(j, k);
        }
        let limit = if count > 0 {
            count.min(lines.len())
        } else {
            lines.len()
        };
        for k in 0..limit {
            writeln!(output, "{}", lines[k]).unwrap();
        }
    }

    ShellResult::ok(&output)
}

// ── numfmt ──────────────────────────────────────────────────────
pub fn numfmt(args: &[String]) -> ShellResult {
    let to_iec = args.contains(&String::from("--to=iec"));
    let to_si = args.contains(&String::from("--to=si"));
    let nums: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();

    let mut output = String::new();
    for num_str in &nums {
        if let Ok(n) = num_str.parse::<u64>() {
            if to_iec {
                let formatted = if n >= 1024 * 1024 * 1024 {
                    alloc::format!("{}G", n / (1024 * 1024 * 1024))
                } else if n >= 1024 * 1024 {
                    alloc::format!("{}M", n / (1024 * 1024))
                } else if n >= 1024 {
                    alloc::format!("{}K", n / 1024)
                } else {
                    alloc::format!("{}", n)
                };
                writeln!(output, "{}", formatted).unwrap();
            } else if to_si {
                let formatted = if n >= 1_000_000_000 {
                    alloc::format!("{}G", n / 1_000_000_000)
                } else if n >= 1_000_000 {
                    alloc::format!("{}M", n / 1_000_000)
                } else if n >= 1_000 {
                    alloc::format!("{}K", n / 1_000)
                } else {
                    alloc::format!("{}", n)
                };
                writeln!(output, "{}", formatted).unwrap();
            } else {
                writeln!(output, "{}", n).unwrap();
            }
        }
    }
    ShellResult::ok(&output)
}

// ── tput ────────────────────────────────────────────────────────
pub fn tput(args: &[String]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::err("tput: missing operand");
    }
    match args[0].as_str() {
        "cols" => ShellResult::ok("80"),
        "lines" => ShellResult::ok("24"),
        "clear" => ShellResult::ok("\x1b[2J\x1b[H"),
        "reset" => ShellResult::ok("\x1b[0m"),
        "bold" => ShellResult::ok("\x1b[1m"),
        "sgr0" => ShellResult::ok("\x1b[0m"),
        "setaf" => {
            let color = args.get(1).and_then(|s| s.parse::<u8>().ok()).unwrap_or(0);
            ShellResult::ok(&alloc::format!("\x1b[3{}m", color))
        }
        "setab" => {
            let color = args.get(1).and_then(|s| s.parse::<u8>().ok()).unwrap_or(0);
            ShellResult::ok(&alloc::format!("\x1b[4{}m", color))
        }
        "cup" => {
            let row = args.get(1).and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
            let col = args.get(2).and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
            ShellResult::ok(&alloc::format!("\x1b[{};{}H", row + 1, col + 1))
        }
        "civis" => ShellResult::ok("\x1b[?25l"),
        "cnorm" => ShellResult::ok("\x1b[?25h"),
        _ => ShellResult::err(&alloc::format!("tput: unknown capability: {}", args[0])),
    }
}

// ════════════════════════════════════════════════════════════════
// Stdin-aware variants for pipe support
// ════════════════════════════════════════════════════════════════

/// Helper: get input lines from stdin data or files
fn get_input_lines(args: &[String], stdin_data: Option<&str>, skip_flags: &[&str]) -> Vec<String> {
    let files: Vec<&String> = args
        .iter()
        .filter(|a| !skip_flags.contains(&a.as_str()) && !a.starts_with('-'))
        .collect();

    if files.is_empty() {
        if let Some(data) = stdin_data {
            return data.lines().map(|l| l.to_string()).collect();
        }
        return Vec::new();
    }

    let vfs = crate::vfs::VFS.lock();
    let mut lines = Vec::new();
    for file in &files {
        let path = resolve_path(file);
        if let Some(data) = vfs.read_file(&path) {
            if let Ok(s) = core::str::from_utf8(data) {
                for line in s.lines() {
                    lines.push(line.to_string());
                }
            }
        }
    }
    lines
}

/// Helper: get raw input text from stdin data or first file
fn get_input_text(
    args: &[String],
    stdin_data: Option<&str>,
    skip_flags: &[&str],
) -> Option<String> {
    let files: Vec<&String> = args
        .iter()
        .filter(|a| !skip_flags.contains(&a.as_str()) && !a.starts_with('-'))
        .collect();

    if files.is_empty() {
        return stdin_data.map(|s| s.to_string());
    }

    let vfs = crate::vfs::VFS.lock();
    for file in &files {
        let path = resolve_path(file);
        if let Some(data) = vfs.read_file(&path) {
            if let Ok(s) = core::str::from_utf8(data) {
                return Some(s.to_string());
            }
        }
    }
    None
}

/// tac with stdin: reverse lines from pipe
pub fn tac_with_stdin(args: &[String], stdin_data: Option<&str>) -> ShellResult {
    let lines = get_input_lines(args, stdin_data, &[]);
    if lines.is_empty() {
        if stdin_data.is_none() && args.is_empty() {
            return ShellResult::err("tac: missing file operand");
        }
        return ShellResult::ok("");
    }
    let mut output = String::new();
    for line in lines.iter().rev() {
        writeln!(output, "{}", line).unwrap();
    }
    ShellResult::ok(&output)
}

/// sort with stdin: sort lines from pipe
pub fn sort_with_stdin(args: &[String], stdin_data: Option<&str>) -> ShellResult {
    let mut reverse = false;
    let mut numeric = false;
    let mut unique = false;
    let flags: Vec<&str> = args
        .iter()
        .filter(|a| a.starts_with('-'))
        .map(|a| a.as_str())
        .collect();

    for &flag in &flags {
        match flag {
            "-r" => reverse = true,
            "-n" => numeric = true,
            "-u" => unique = true,
            "-rn" | "-nr" => {
                reverse = true;
                numeric = true;
            }
            _ => {}
        }
    }

    let mut all_lines = get_input_lines(args, stdin_data, &["-r", "-n", "-u", "-rn", "-nr"]);
    if all_lines.is_empty() {
        return sort(args);
    }

    if numeric {
        all_lines.sort_by(|a, b| {
            let na: i64 = a
                .trim()
                .split_whitespace()
                .next()
                .unwrap_or("0")
                .parse()
                .unwrap_or(0);
            let nb: i64 = b
                .trim()
                .split_whitespace()
                .next()
                .unwrap_or("0")
                .parse()
                .unwrap_or(0);
            na.cmp(&nb)
        });
    } else {
        all_lines.sort();
    }
    if reverse {
        all_lines.reverse();
    }
    if unique {
        all_lines.dedup();
    }

    let mut output = String::new();
    for line in &all_lines {
        writeln!(output, "{}", line).unwrap();
    }
    ShellResult::ok(&output)
}

/// uniq with stdin: deduplicate lines from pipe
pub fn uniq_with_stdin(args: &[String], stdin_data: Option<&str>) -> ShellResult {
    let mut count = false;
    let mut only_dups = false;
    let mut only_unique = false;

    for arg in args {
        match arg.as_str() {
            "-c" => count = true,
            "-d" => only_dups = true,
            "-u" => only_unique = true,
            _ => {}
        }
    }

    let lines = get_input_lines(args, stdin_data, &["-c", "-d", "-u"]);
    if lines.is_empty() {
        return uniq(args);
    }

    let mut output = String::new();
    let mut i = 0;
    while i < lines.len() {
        let mut c = 1;
        while i + c < lines.len() && lines[i] == lines[i + c] {
            c += 1;
        }
        if only_dups && c == 1 {
            i += c;
            continue;
        }
        if only_unique && c > 1 {
            i += c;
            continue;
        }
        if count {
            writeln!(output, "{:>7} {}", c, lines[i]).unwrap();
        } else {
            writeln!(output, "{}", lines[i]).unwrap();
        }
        i += c;
    }
    ShellResult::ok(&output)
}

/// cut with stdin: extract fields from pipe
pub fn cut_with_stdin(args: &[String], stdin_data: Option<&str>) -> ShellResult {
    let mut delimiter = '\t';
    let mut fields: Vec<usize> = Vec::new();
    let mut has_files = false;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "-d" if i + 1 < args.len() => {
                delimiter = args[i + 1].chars().next().unwrap_or('\t');
                i += 2;
                continue;
            }
            "-f" if i + 1 < args.len() => {
                for f in args[i + 1].split(',') {
                    if let Ok(n) = f.parse::<usize>() {
                        if n > 0 {
                            fields.push(n - 1);
                        }
                    }
                }
                i += 2;
                continue;
            }
            _ if !args[i].starts_with('-') => has_files = true,
            _ => {}
        }
        i += 1;
    }

    if fields.is_empty() {
        return ShellResult::err("cut: you must specify a list of fields");
    }

    if !has_files {
        if let Some(data) = stdin_data {
            let mut output = String::new();
            for line in data.lines() {
                let parts: Vec<&str> = line.split(delimiter).collect();
                let selected: Vec<&str> = fields
                    .iter()
                    .filter_map(|&f| parts.get(f).copied())
                    .collect();
                writeln!(output, "{}", selected.join(&delimiter.to_string())).unwrap();
            }
            return ShellResult::ok(&output);
        }
    }

    cut(args)
}

/// tr with stdin: transliterate characters from pipe
pub fn tr_with_stdin(args: &[String], stdin_data: Option<&str>) -> ShellResult {
    if args.len() < 2 {
        return ShellResult::err("tr: missing operand\nUsage: tr SET1 SET2");
    }

    let set1 = expand_tr_set(&args[0]);
    let set2 = expand_tr_set(&args[1]);

    if let Some(data) = stdin_data {
        let mut output = String::new();
        for ch in data.chars() {
            if let Some(pos) = set1.iter().position(|&c| c == ch) {
                if pos < set2.len() {
                    output.push(set2[pos]);
                } else if let Some(&last) = set2.last() {
                    output.push(last);
                } else {
                    output.push(ch);
                }
            } else {
                output.push(ch);
            }
        }
        return ShellResult::ok(&output);
    }

    tr(args)
}

/// Expand tr character ranges like a-z, A-Z, 0-9
fn expand_tr_set(s: &str) -> Vec<char> {
    let chars: Vec<char> = s.chars().collect();
    let mut result = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if i + 2 < chars.len() && chars[i + 1] == '-' {
            let start = chars[i] as u32;
            let end = chars[i + 2] as u32;
            if start <= end {
                for c in start..=end {
                    if let Some(ch) = char::from_u32(c) {
                        result.push(ch);
                    }
                }
            }
            i += 3;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}

/// rev with stdin: reverse each line from pipe
pub fn rev_with_stdin(args: &[String], stdin_data: Option<&str>) -> ShellResult {
    let lines = get_input_lines(args, stdin_data, &[]);
    if lines.is_empty() {
        return rev(args);
    }
    let mut output = String::new();
    for line in &lines {
        let reversed: String = line.chars().rev().collect();
        writeln!(output, "{}", reversed).unwrap();
    }
    ShellResult::ok(&output)
}

/// nl with stdin: number lines from pipe
pub fn nl_with_stdin(args: &[String], stdin_data: Option<&str>) -> ShellResult {
    let lines = get_input_lines(args, stdin_data, &[]);
    if lines.is_empty() {
        return nl(args);
    }
    let mut output = String::new();
    for (i, line) in lines.iter().enumerate() {
        writeln!(output, "{:>6}\t{}", i + 1, line).unwrap();
    }
    ShellResult::ok(&output)
}

/// fold with stdin: wrap long lines from pipe
pub fn fold_with_stdin(args: &[String], stdin_data: Option<&str>) -> ShellResult {
    let mut width = 80usize;
    let mut has_files = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-w" if i + 1 < args.len() => {
                width = args[i + 1].parse().unwrap_or(80);
                i += 2;
                continue;
            }
            _ if !args[i].starts_with('-') => has_files = true,
            _ => {}
        }
        i += 1;
    }
    if !has_files {
        if let Some(data) = stdin_data {
            let mut output = String::new();
            for line in data.lines() {
                let chars: Vec<char> = line.chars().collect();
                for chunk in chars.chunks(width) {
                    let s: String = chunk.iter().collect();
                    writeln!(output, "{}", s).unwrap();
                }
            }
            return ShellResult::ok(&output);
        }
    }
    fold(args)
}

/// fmt with stdin: reformat paragraph text from pipe
pub fn fmt_with_stdin(args: &[String], stdin_data: Option<&str>) -> ShellResult {
    let mut width = 75usize;
    let mut has_files = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-w" if i + 1 < args.len() => {
                width = args[i + 1].parse().unwrap_or(75);
                i += 2;
                continue;
            }
            _ if !args[i].starts_with('-') => has_files = true,
            _ => {}
        }
        i += 1;
    }
    if !has_files {
        if let Some(data) = stdin_data {
            let mut output = String::new();
            let words: Vec<&str> = data.split_whitespace().collect();
            let mut line_len = 0;
            for word in &words {
                if line_len + word.len() + 1 > width && line_len > 0 {
                    writeln!(output).unwrap();
                    line_len = 0;
                }
                if line_len > 0 {
                    write!(output, " ").unwrap();
                    line_len += 1;
                }
                write!(output, "{}", word).unwrap();
                line_len += word.len();
            }
            if line_len > 0 {
                writeln!(output).unwrap();
            }
            return ShellResult::ok(&output);
        }
    }
    fmt(args)
}

/// sed with stdin: stream edit from pipe
pub fn sed_with_stdin(args: &[String], stdin_data: Option<&str>) -> ShellResult {
    if args.is_empty() {
        return ShellResult::err("sed: missing script");
    }

    // Parse script
    let mut script = String::new();
    let mut has_files = false;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "-e" && i + 1 < args.len() {
            script = args[i + 1].clone();
            i += 2;
            continue;
        } else if script.is_empty() && (args[i].starts_with("s/") || args[i].starts_with("s|")) {
            script = args[i].clone();
        } else if !args[i].starts_with('-')
            && !args[i].starts_with("s/")
            && !args[i].starts_with("s|")
        {
            has_files = true;
        }
        i += 1;
    }

    if script.is_empty() {
        return ShellResult::err("sed: no script provided");
    }

    if !has_files {
        if let Some(data) = stdin_data {
            let sep = if script.len() > 1 {
                script.as_bytes()[1] as char
            } else {
                '/'
            };
            let parts: Vec<&str> = script[2..].splitn(3, sep).collect();
            if parts.len() < 2 {
                return ShellResult::err("sed: invalid script");
            }
            let pattern = parts[0];
            let replacement = parts[1];
            let flags = if parts.len() > 2 { parts[2] } else { "" };
            let global = flags.contains('g');

            let mut output = String::new();
            for line in data.lines() {
                let new_line = if global {
                    line.replace(pattern, replacement)
                } else {
                    line.replacen(pattern, replacement, 1)
                };
                writeln!(output, "{}", new_line).unwrap();
            }
            return ShellResult::ok(&output);
        }
    }
    sed(args)
}

/// awk with stdin: field extraction from pipe
pub fn awk_with_stdin(args: &[String], stdin_data: Option<&str>) -> ShellResult {
    if args.is_empty() {
        return ShellResult::err("awk: missing program");
    }

    let mut program = String::new();
    let mut has_files = false;
    let mut field_sep = " ";

    let mut i = 0;
    while i < args.len() {
        if args[i] == "-F" && i + 1 < args.len() {
            field_sep = if args[i + 1].as_str() == "\\t" {
                "\t"
            } else {
                &args[i + 1]
            };
            i += 2;
            continue;
        } else if program.is_empty() && (args[i].starts_with('{') || args[i].starts_with("'")) {
            program = args[i].trim_matches('\'').to_string();
        } else if !args[i].starts_with('-') && !program.is_empty() {
            has_files = true;
        }
        i += 1;
    }

    if program.is_empty() {
        return awk(args);
    }

    if !has_files {
        if let Some(data) = stdin_data {
            let mut output = String::new();
            // Parse simple {print $N} programs
            if program.starts_with("{print") || program.starts_with("{ print") {
                let inner = program.trim_start_matches('{').trim_end_matches('}').trim();
                let inner = inner.strip_prefix("print").unwrap_or(inner).trim();

                for line in data.lines() {
                    let fields: Vec<&str> = if field_sep == " " {
                        line.split_whitespace().collect()
                    } else {
                        line.split(field_sep).collect()
                    };

                    let mut parts = Vec::new();
                    for token in inner.split(',') {
                        let token = token.trim().trim_matches('"');
                        if token.starts_with('$') {
                            if let Ok(idx) = token[1..].parse::<usize>() {
                                if idx == 0 {
                                    parts.push(line.to_string());
                                } else if idx <= fields.len() {
                                    parts.push(fields[idx - 1].to_string());
                                }
                            }
                        } else {
                            parts.push(token.to_string());
                        }
                    }
                    writeln!(output, "{}", parts.join(" ")).unwrap();
                }
            } else {
                // Fallback: just pass through
                output = data.to_string();
            }
            return ShellResult::ok(&output);
        }
    }
    awk(args)
}

/// tee with stdin: write stdin to files and stdout
pub fn tee_with_stdin(args: &[String], stdin_data: Option<&str>) -> ShellResult {
    let append = args.contains(&String::from("-a"));
    let files: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();

    if let Some(data) = stdin_data {
        // Write to each file
        for file in &files {
            let resolved = resolve_path(file);
            let mut vfs = crate::vfs::VFS.lock();
            if append {
                let existing = vfs
                    .read_file(&resolved)
                    .map(|d| {
                        let mut v = alloc::vec::Vec::from(d);
                        v.extend_from_slice(data.as_bytes());
                        v
                    })
                    .unwrap_or_else(|| alloc::vec::Vec::from(data.as_bytes()));
                vfs.write_file(&resolved, &existing);
            } else {
                vfs.write_file(&resolved, data.as_bytes());
            }
        }
        // Also output to stdout (the pipe continues)
        return ShellResult::ok(data);
    }

    tee(args)
}

/// xargs with stdin: build and execute commands from stdin
pub fn xargs_with_stdin(args: &[String], stdin_data: Option<&str>) -> ShellResult {
    if let Some(data) = stdin_data {
        // Default command is echo
        let cmd_name = args.first().map(|s| s.as_str()).unwrap_or("echo");
        let base_args: Vec<String> = if args.len() > 1 {
            args[1..].to_vec()
        } else {
            Vec::new()
        };

        let mut output = String::new();
        // Split stdin by whitespace and run command for each item
        let items: Vec<&str> = data.split_whitespace().collect();
        let mut all_args = base_args.clone();
        all_args.extend(items.iter().map(|s| String::from(*s)));

        if let Some(result) = super::dispatch(cmd_name, &all_args) {
            output.push_str(&result.output);
        }

        return ShellResult::ok(&output);
    }

    ShellResult::ok("xargs: reads from stdin and builds command lines (use with pipes)")
}

/// base64 with stdin: encode/decode from pipe
pub fn base64_with_stdin(args: &[String], stdin_data: Option<&str>) -> ShellResult {
    let decode = args.contains(&String::from("-d")) || args.contains(&String::from("--decode"));
    let has_files = args.iter().any(|a| !a.starts_with('-'));

    if !has_files {
        if let Some(data) = stdin_data {
            if decode {
                match base64_decode(data.trim()) {
                    Ok(decoded) => {
                        let text = core::str::from_utf8(&decoded).unwrap_or("(binary data)");
                        return ShellResult::ok(text);
                    }
                    Err(e) => return ShellResult::err(&alloc::format!("base64: {}", e)),
                }
            } else {
                let encoded = base64_encode(data.as_bytes());
                return ShellResult::ok(&encoded);
            }
        }
    }

    base64(args)
}
