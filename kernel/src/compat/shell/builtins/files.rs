#![allow(
    clippy::manual_strip,
    clippy::manual_div_ceil,
    clippy::too_many_arguments,
    clippy::manual_range_contains,
    clippy::collapsible_if,
    clippy::needless_range_loop,
    clippy::if_same_then_else
)]
/// File operation builtins — ls, cat, touch, mkdir, rm, rmdir, cp, mv, head, tail,
/// wc, grep, find, stat, tree, ln, readlink, realpath, basename, dirname,
/// chmod, chown, du, mktemp, file, more/less, dd, tar
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt::Write;

use crate::shell::env::ENV_VARS;
use crate::shell::helpers::{format_permissions, resolve_path};
use crate::shell::types::ShellResult;

pub fn ls(args: &[String]) -> ShellResult {
    let mut show_all = false;
    let mut long_format = false;
    let mut paths = Vec::new();

    for arg in args {
        match arg.as_str() {
            "-a" => show_all = true,
            "-l" => long_format = true,
            "-la" | "-al" => {
                show_all = true;
                long_format = true;
            }
            _ => paths.push(arg.clone()),
        }
    }

    if paths.is_empty() {
        paths.push(
            ENV_VARS
                .lock()
                .get("PWD")
                .cloned()
                .unwrap_or_else(|| String::from("/")),
        );
    }

    let mut output = String::new();
    let vfs = crate::vfs::VFS.lock();

    for path in &paths {
        let resolved = resolve_path(path);
        if let Some(entries) = vfs.list_dir(&resolved) {
            if paths.len() > 1 {
                writeln!(output, "{}:", path).unwrap();
            }

            let mut visible_entries: Vec<&String> = entries
                .iter()
                .filter(|e| show_all || !e.starts_with('.'))
                .collect();
            visible_entries.sort();

            if long_format {
                writeln!(output, "total {}", visible_entries.len()).unwrap();
                for entry in &visible_entries {
                    let entry_path = if resolved == "/" {
                        alloc::format!("/{}", entry)
                    } else {
                        alloc::format!("{}/{}", resolved, entry)
                    };

                    if let Some(ino) = vfs.resolve_path(&entry_path) {
                        if let Some(inode) = vfs.get_inode(ino) {
                            let type_char = match inode.file_type {
                                crate::vfs::FileType::Directory => 'd',
                                crate::vfs::FileType::SymLink => 'l',
                                crate::vfs::FileType::CharDevice => 'c',
                                crate::vfs::FileType::BlockDevice => 'b',
                                crate::vfs::FileType::Pipe => 'p',
                                crate::vfs::FileType::Socket => 's',
                                _ => '-',
                            };
                            let perms = format_permissions(inode.permissions);
                            let name_colored = match inode.file_type {
                                crate::vfs::FileType::Directory => {
                                    alloc::format!("\x1b[1;34m{}/\x1b[0m", entry)
                                }
                                crate::vfs::FileType::SymLink => {
                                    alloc::format!("\x1b[1;36m{}\x1b[0m", entry)
                                }
                                crate::vfs::FileType::CharDevice
                                | crate::vfs::FileType::BlockDevice => {
                                    alloc::format!("\x1b[1;33m{}\x1b[0m", entry)
                                }
                                crate::vfs::FileType::Pipe => {
                                    alloc::format!("\x1b[33m{}\x1b[0m", entry)
                                }
                                crate::vfs::FileType::Socket => {
                                    alloc::format!("\x1b[1;35m{}\x1b[0m", entry)
                                }
                                _ => {
                                    if inode.permissions & 0o111 != 0 {
                                        alloc::format!("\x1b[1;32m{}\x1b[0m", entry)
                                    } else {
                                        entry.to_string()
                                    }
                                }
                            };
                            writeln!(
                                output,
                                "{}{} {:>3} {:>5} {:>5} {:>8} Jan  1 00:00 {}",
                                type_char,
                                perms,
                                if inode.file_type == crate::vfs::FileType::Directory {
                                    2 + inode.children.len()
                                } else {
                                    1
                                },
                                inode.uid,
                                inode.gid,
                                inode.size,
                                name_colored
                            )
                            .unwrap();
                        }
                    }
                }
            } else {
                // Collect display strings with ANSI colors and calculate column widths
                let mut display_items: Vec<(String, usize)> = Vec::new(); // (colored_str, raw_len)
                for entry in &visible_entries {
                    let entry_path = if resolved == "/" {
                        alloc::format!("/{}", entry)
                    } else {
                        alloc::format!("{}/{}", resolved, entry)
                    };
                    let (colored, raw_len) = if let Some(ino) = vfs.resolve_path(&entry_path) {
                        if let Some(inode) = vfs.get_inode(ino) {
                            match inode.file_type {
                                crate::vfs::FileType::Directory => (
                                    alloc::format!("\x1b[1;34m{}/\x1b[0m", entry),
                                    entry.len() + 1,
                                ),
                                crate::vfs::FileType::SymLink => (
                                    alloc::format!("\x1b[1;36m{}@\x1b[0m", entry),
                                    entry.len() + 1,
                                ),
                                crate::vfs::FileType::CharDevice
                                | crate::vfs::FileType::BlockDevice => {
                                    (alloc::format!("\x1b[1;33m{}\x1b[0m", entry), entry.len())
                                }
                                crate::vfs::FileType::Pipe => {
                                    (alloc::format!("\x1b[33m{}|\x1b[0m", entry), entry.len() + 1)
                                }
                                crate::vfs::FileType::Socket => (
                                    alloc::format!("\x1b[1;35m{}=\x1b[0m", entry),
                                    entry.len() + 1,
                                ),
                                _ => {
                                    // Check if executable (any x bit set)
                                    if inode.permissions & 0o111 != 0 {
                                        (
                                            alloc::format!("\x1b[1;32m{}*\x1b[0m", entry),
                                            entry.len() + 1,
                                        )
                                    } else {
                                        (entry.to_string(), entry.len())
                                    }
                                }
                            }
                        } else {
                            (entry.to_string(), entry.len())
                        }
                    } else {
                        (entry.to_string(), entry.len())
                    };
                    display_items.push((colored, raw_len));
                }

                // Calculate column layout (like real ls)
                if !display_items.is_empty() {
                    let max_name_len = display_items.iter().map(|(_, l)| *l).max().unwrap_or(8);
                    let col_width = max_name_len + 2; // 2-char gap between columns
                    let term_width = 80usize; // default terminal width
                    let num_cols = (term_width / col_width).max(1);
                    let num_rows = (display_items.len() + num_cols - 1) / num_cols;

                    for row_idx in 0..num_rows {
                        for col_idx in 0..num_cols {
                            let item_idx = col_idx * num_rows + row_idx;
                            if item_idx < display_items.len() {
                                let (ref colored, raw_len) = display_items[item_idx];
                                write!(output, "{}", colored).unwrap();
                                // Pad to column width (except last column)
                                if col_idx + 1 < num_cols
                                    && item_idx + num_rows < display_items.len()
                                {
                                    let padding = col_width.saturating_sub(raw_len);
                                    for _ in 0..padding {
                                        output.push(' ');
                                    }
                                }
                            }
                        }
                        output.push('\n');
                    }
                }
            }
        } else {
            writeln!(
                output,
                "ls: cannot access '{}': No such file or directory",
                path
            )
            .unwrap();
        }
    }

    ShellResult::ok(&output)
}

pub fn cat(args: &[String]) -> ShellResult {
    let mut output = String::new();
    let vfs = crate::vfs::VFS.lock();

    for arg in args {
        let path = resolve_path(arg);
        if let Some(data) = vfs.read_file(&path) {
            if let Ok(s) = core::str::from_utf8(data) {
                output.push_str(s);
            } else {
                return ShellResult::err(&alloc::format!("cat: {}: binary file", arg));
            }
        } else {
            return ShellResult::err(&alloc::format!("cat: {}: No such file or directory", arg));
        }
    }

    ShellResult::ok(&output)
}

pub fn touch(args: &[String]) -> ShellResult {
    for arg in args {
        let path = resolve_path(arg);
        let mut vfs = crate::vfs::VFS.lock();
        if vfs.resolve_path(&path).is_none() {
            vfs.write_file(&path, &[]);
        }
    }
    ShellResult::ok("")
}

pub fn mkdir(args: &[String]) -> ShellResult {
    let mut parents = false;
    let mut dirs = Vec::new();

    for arg in args {
        match arg.as_str() {
            "-p" => parents = true,
            _ => dirs.push(arg.clone()),
        }
    }

    for dir in &dirs {
        let path = resolve_path(dir);
        let mut vfs = crate::vfs::VFS.lock();
        match vfs.mkdir(&path, 0o755) {
            Ok(_) => {}
            Err(-17) if parents => {} // Already exists, -p ignores
            Err(-17) => {
                return ShellResult::err(&alloc::format!(
                    "mkdir: cannot create '{}': File exists",
                    dir
                ));
            }
            Err(_) => {
                return ShellResult::err(&alloc::format!(
                    "mkdir: cannot create '{}': No such file or directory",
                    dir
                ));
            }
        }
    }

    ShellResult::ok("")
}

pub fn rm(args: &[String]) -> ShellResult {
    let mut recursive = false;
    let mut force = false;
    let mut files = Vec::new();

    for arg in args {
        match arg.as_str() {
            "-r" | "-R" | "--recursive" => recursive = true,
            "-f" | "--force" => force = true,
            "-rf" | "-fr" => {
                recursive = true;
                force = true;
            }
            _ => files.push(arg.clone()),
        }
    }

    for file in &files {
        let path = resolve_path(file);
        let mut vfs = crate::vfs::VFS.lock();
        match vfs.unlink(&path) {
            Ok(_) => {}
            Err(-21) if recursive => {
                // Directory - recursive remove
                let _ = vfs.rmdir(&path);
            }
            Err(-2) if force => {} // Not found, -f ignores
            Err(-2) => {
                return ShellResult::err(&alloc::format!(
                    "rm: cannot remove '{}': No such file or directory",
                    file
                ));
            }
            Err(-21) => {
                return ShellResult::err(&alloc::format!(
                    "rm: cannot remove '{}': Is a directory",
                    file
                ));
            }
            Err(_) => return ShellResult::err(&alloc::format!("rm: cannot remove '{}'", file)),
        }
    }

    ShellResult::ok("")
}

pub fn rmdir(args: &[String]) -> ShellResult {
    for arg in args {
        let path = resolve_path(arg);
        let mut vfs = crate::vfs::VFS.lock();
        match vfs.rmdir(&path) {
            Ok(_) => {}
            Err(-39) => {
                return ShellResult::err(&alloc::format!("rmdir: {}: Directory not empty", arg));
            }
            Err(-2) => {
                return ShellResult::err(&alloc::format!(
                    "rmdir: {}: No such file or directory",
                    arg
                ));
            }
            Err(_) => return ShellResult::err(&alloc::format!("rmdir: {}: failed", arg)),
        }
    }
    ShellResult::ok("")
}

pub fn cp(args: &[String]) -> ShellResult {
    if args.len() < 2 {
        return ShellResult::err("cp: missing operand");
    }

    let src_path = resolve_path(&args[0]);
    let dst_path = resolve_path(&args[1]);

    let vfs = crate::vfs::VFS.lock();
    if let Some(data) = vfs.read_file(&src_path) {
        let data_copy = data.to_vec();
        drop(vfs);
        crate::vfs::VFS.lock().write_file(&dst_path, &data_copy);
        ShellResult::ok("")
    } else {
        ShellResult::err(&alloc::format!(
            "cp: cannot stat '{}': No such file or directory",
            args[0]
        ))
    }
}

pub fn mv(args: &[String]) -> ShellResult {
    if args.len() < 2 {
        return ShellResult::err("mv: missing operand");
    }

    let src_path = resolve_path(&args[0]);
    let dst_path = resolve_path(&args[1]);

    let mut vfs = crate::vfs::VFS.lock();
    match vfs.rename(&src_path, &dst_path) {
        Ok(_) => ShellResult::ok(""),
        Err(_) => ShellResult::err(&alloc::format!(
            "mv: cannot move '{}' to '{}'",
            args[0],
            args[1]
        )),
    }
}

pub fn head(args: &[String]) -> ShellResult {
    let mut lines = 10usize;
    let mut files = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-n" if i + 1 < args.len() => {
                lines = args[i + 1].parse().unwrap_or(10);
                i += 2;
                continue;
            }
            _ => files.push(args[i].clone()),
        }
        i += 1;
    }

    let mut output = String::new();
    let vfs = crate::vfs::VFS.lock();

    for file in &files {
        let path = resolve_path(file);
        if let Some(data) = vfs.read_file(&path) {
            if let Ok(s) = core::str::from_utf8(data) {
                for (j, line) in s.lines().enumerate() {
                    if j >= lines {
                        break;
                    }
                    writeln!(output, "{}", line).unwrap();
                }
            }
        } else {
            return ShellResult::err(&alloc::format!("head: {}: No such file", file));
        }
    }

    ShellResult::ok(&output)
}

pub fn tail(args: &[String]) -> ShellResult {
    let mut lines = 10usize;
    let mut files = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-n" if i + 1 < args.len() => {
                lines = args[i + 1].parse().unwrap_or(10);
                i += 2;
                continue;
            }
            _ => files.push(args[i].clone()),
        }
        i += 1;
    }

    let mut output = String::new();
    let vfs = crate::vfs::VFS.lock();

    for file in &files {
        let path = resolve_path(file);
        if let Some(data) = vfs.read_file(&path) {
            if let Ok(s) = core::str::from_utf8(data) {
                let all_lines: Vec<&str> = s.lines().collect();
                let start = if all_lines.len() > lines {
                    all_lines.len() - lines
                } else {
                    0
                };
                for line in &all_lines[start..] {
                    writeln!(output, "{}", line).unwrap();
                }
            }
        } else {
            return ShellResult::err(&alloc::format!("tail: {}: No such file", file));
        }
    }

    ShellResult::ok(&output)
}

pub fn wc(args: &[String]) -> ShellResult {
    let mut output = String::new();
    let vfs = crate::vfs::VFS.lock();

    for arg in args {
        let path = resolve_path(arg);
        if let Some(data) = vfs.read_file(&path) {
            if let Ok(s) = core::str::from_utf8(data) {
                let lines = s.lines().count();
                let words = s.split_whitespace().count();
                let bytes = data.len();
                writeln!(output, " {:>7} {:>7} {:>7} {}", lines, words, bytes, arg).unwrap();
            }
        } else {
            return ShellResult::err(&alloc::format!("wc: {}: No such file", arg));
        }
    }

    ShellResult::ok(&output)
}

pub fn grep(args: &[String]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::err("grep: missing pattern");
    }

    let mut case_insensitive = false;
    let mut invert = false;
    let mut count_only = false;
    let mut line_numbers = false;
    let mut pattern_idx = 0;

    for (i, arg) in args.iter().enumerate() {
        match arg.as_str() {
            "-i" => case_insensitive = true,
            "-v" => invert = true,
            "-c" => count_only = true,
            "-n" => line_numbers = true,
            _ if pattern_idx == 0 => {
                pattern_idx = i;
                break;
            }
            _ => {}
        }
    }

    if pattern_idx >= args.len() {
        return ShellResult::err("grep: missing pattern");
    }

    let pattern = &args[pattern_idx];
    let files = &args[pattern_idx + 1..];

    let mut output = String::new();
    let vfs = crate::vfs::VFS.lock();

    for file in files {
        let path = resolve_path(file);
        if let Some(data) = vfs.read_file(&path) {
            if let Ok(s) = core::str::from_utf8(data) {
                let mut count = 0;
                for (line_num, line) in s.lines().enumerate() {
                    let matches = if case_insensitive {
                        line.to_lowercase().contains(&pattern.to_lowercase())
                    } else {
                        line.contains(pattern.as_str())
                    };
                    let matches = if invert { !matches } else { matches };

                    if matches {
                        count += 1;
                        if !count_only {
                            if line_numbers {
                                writeln!(output, "{}:{}", line_num + 1, line).unwrap();
                            } else {
                                writeln!(output, "{}", line).unwrap();
                            }
                        }
                    }
                }
                if count_only {
                    writeln!(output, "{}", count).unwrap();
                }
            }
        }
    }

    ShellResult::ok(&output)
}

pub fn find(args: &[String]) -> ShellResult {
    let start_path = if args.is_empty() {
        ENV_VARS
            .lock()
            .get("PWD")
            .cloned()
            .unwrap_or_else(|| String::from("/"))
    } else {
        resolve_path(&args[0])
    };

    let mut output = String::new();
    let vfs = crate::vfs::VFS.lock();
    find_recursive(&vfs, &start_path, &mut output);
    ShellResult::ok(&output)
}

fn find_recursive(vfs: &crate::vfs::VirtualFS, path: &str, output: &mut String) {
    writeln!(output, "{}", path).unwrap();
    if let Some(entries) = vfs.list_dir(path) {
        for entry in entries {
            let child_path = if path == "/" {
                alloc::format!("/{}", entry)
            } else {
                alloc::format!("{}/{}", path, entry)
            };
            find_recursive(vfs, &child_path, output);
        }
    }
}

pub fn stat(args: &[String]) -> ShellResult {
    let mut output = String::new();
    let vfs = crate::vfs::VFS.lock();

    for arg in args {
        let path = resolve_path(arg);
        match vfs.stat(&path) {
            Ok(st) => {
                writeln!(output, "  File: {}", path).unwrap();
                write!(
                    output,
                    "  Size: {}\tBlocks: {}\t",
                    st.size,
                    st.size.div_ceil(512)
                )
                .unwrap();
                let ftype = match st.file_type {
                    crate::vfs::FileType::Regular => "regular file",
                    crate::vfs::FileType::Directory => "directory",
                    crate::vfs::FileType::SymLink => "symbolic link",
                    crate::vfs::FileType::CharDevice => "character special file",
                    crate::vfs::FileType::BlockDevice => "block special file",
                    crate::vfs::FileType::Pipe => "fifo",
                    crate::vfs::FileType::Socket => "socket",
                };
                writeln!(output, "{}", ftype).unwrap();
                writeln!(output, "  Inode: {}\tLinks: {}", st.ino, st.nlink).unwrap();
                writeln!(
                    output,
                    "Access: ({:04o}/{})  Uid: ({:>5}/{:>8})  Gid: ({:>5}/{:>8})",
                    st.permissions,
                    format_permissions(st.permissions),
                    st.uid,
                    "user",
                    st.gid,
                    "user"
                )
                .unwrap();
            }
            Err(_) => {
                return ShellResult::err(&alloc::format!(
                    "stat: cannot stat '{}': No such file",
                    arg
                ));
            }
        }
    }

    ShellResult::ok(&output)
}

// ── tree ────────────────────────────────────────────────────────
pub fn tree(args: &[String]) -> ShellResult {
    let mut show_all = false;
    let mut max_depth: usize = 8;
    let mut dir_only = false;
    let mut target = String::new();
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "-a" => show_all = true,
            "-d" => dir_only = true,
            "-L" if i + 1 < args.len() => {
                max_depth = args[i + 1].parse().unwrap_or(8);
                i += 2;
                continue;
            }
            _ => target = args[i].clone(),
        }
        i += 1;
    }

    let path = if target.is_empty() {
        ENV_VARS
            .lock()
            .get("PWD")
            .cloned()
            .unwrap_or_else(|| String::from("/"))
    } else {
        resolve_path(&target)
    };

    let vfs = crate::vfs::VFS.lock();
    // Verify it's a directory
    if vfs.list_dir(&path).is_none() {
        return ShellResult::err(&alloc::format!("tree: '{}': No such directory", path));
    }

    let mut output = String::new();
    writeln!(output, "\x1b[38;2;21;153;148m{}\x1b[0m", path).unwrap();

    let (dir_count, file_count) = tree_recursive(
        &vfs,
        &path,
        "",
        0,
        max_depth,
        show_all,
        dir_only,
        &mut output,
    );

    writeln!(output).unwrap();
    writeln!(output, "{} directories, {} files", dir_count, file_count).unwrap();
    ShellResult::ok(&output)
}

fn tree_recursive(
    vfs: &crate::vfs::VirtualFS,
    path: &str,
    prefix: &str,
    depth: usize,
    max_depth: usize,
    show_all: bool,
    dir_only: bool,
    output: &mut String,
) -> (usize, usize) {
    if depth >= max_depth {
        return (0, 0);
    }

    let entries = match vfs.list_dir(path) {
        Some(e) => e,
        None => return (0, 0),
    };

    let mut sorted: Vec<String> = entries
        .iter()
        .filter(|e| show_all || !e.starts_with('.'))
        .cloned()
        .collect();
    sorted.sort();

    let mut dir_count = 0usize;
    let mut file_count = 0usize;
    let total = sorted.len();

    for (i, entry) in sorted.iter().enumerate() {
        let is_last = i == total - 1;
        let connector = if is_last { "└── " } else { "├── " };
        let child_prefix = if is_last { "    " } else { "│   " };

        let child_path = if path == "/" {
            alloc::format!("/{}", entry)
        } else {
            alloc::format!("{}/{}", path, entry)
        };

        let is_dir = vfs.list_dir(&child_path).is_some();

        if dir_only && !is_dir {
            continue;
        }

        if is_dir {
            dir_count += 1;
            writeln!(
                output,
                "{}{}\x1b[38;2;21;153;148m{}\x1b[0m",
                prefix, connector, entry
            )
            .unwrap();
            let new_prefix = alloc::format!("{}{}", prefix, child_prefix);
            let (d, f) = tree_recursive(
                vfs,
                &child_path,
                &new_prefix,
                depth + 1,
                max_depth,
                show_all,
                dir_only,
                output,
            );
            dir_count += d;
            file_count += f;
        } else {
            file_count += 1;
            writeln!(output, "{}{}{}", prefix, connector, entry).unwrap();
        }
    }

    (dir_count, file_count)
}

// ── ln ──────────────────────────────────────────────────────────
pub fn ln(args: &[String]) -> ShellResult {
    let symbolic = args.contains(&String::from("-s")) || args.contains(&String::from("-sf"));
    let force = args.contains(&String::from("-f")) || args.contains(&String::from("-sf"));
    let positional: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();

    if positional.len() < 2 {
        return ShellResult::err(
            "ln: missing target or link name\nUsage: ln [-sf] <target> <link_name>",
        );
    }

    let target = resolve_path(positional[0]);
    let link_name = resolve_path(positional[1]);

    let mut vfs = crate::vfs::VFS.lock();

    if force {
        let _ = vfs.unlink(&link_name);
    }

    // In VFS, create a symlink by writing the target path as the link content
    if symbolic {
        vfs.write_file(&link_name, target.as_bytes());
        ShellResult::ok("")
    } else {
        // Hard link — copy the file
        if let Some(data) = vfs.read_file(&target) {
            let data_copy = data.to_vec();
            vfs.write_file(&link_name, &data_copy);
            ShellResult::ok("")
        } else {
            ShellResult::err(&alloc::format!("ln: {}: No such file", target))
        }
    }
}

// ── readlink ────────────────────────────────────────────────────
pub fn readlink(args: &[String]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::err("readlink: missing operand");
    }
    let canonicalize = args.contains(&String::from("-f")) || args.contains(&String::from("-e"));
    let file = args
        .iter()
        .find(|a| !a.starts_with('-'))
        .unwrap_or(&args[0]);
    let path = resolve_path(file);

    let vfs = crate::vfs::VFS.lock();
    if let Some(data) = vfs.read_file(&path) {
        if let Ok(target) = core::str::from_utf8(data) {
            if canonicalize {
                ShellResult::ok(&resolve_path(target))
            } else {
                ShellResult::ok(target)
            }
        } else {
            ShellResult::ok(&path)
        }
    } else {
        ShellResult::ok(&path)
    }
}

// ── realpath ────────────────────────────────────────────────────
pub fn realpath(args: &[String]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::err("realpath: missing operand");
    }
    let mut output = String::new();
    for arg in args {
        if arg.starts_with('-') {
            continue;
        }
        writeln!(output, "{}", resolve_path(arg)).unwrap();
    }
    ShellResult::ok(&output)
}

// ── basename ────────────────────────────────────────────────────
pub fn basename(args: &[String]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::err("basename: missing operand");
    }
    let path = &args[0];
    let suffix = if args.len() > 1 { &args[1] } else { "" };
    let name = path.rsplit('/').next().unwrap_or(path);
    let result = if !suffix.is_empty() && name.ends_with(suffix) {
        &name[..name.len() - suffix.len()]
    } else {
        name
    };
    ShellResult::ok(result)
}

// ── dirname ─────────────────────────────────────────────────────
pub fn dirname(args: &[String]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::err("dirname: missing operand");
    }
    let mut output = String::new();
    for arg in args {
        if let Some(pos) = arg.rfind('/') {
            if pos == 0 {
                writeln!(output, "/").unwrap();
            } else {
                writeln!(output, "{}", &arg[..pos]).unwrap();
            }
        } else {
            writeln!(output, ".").unwrap();
        }
    }
    ShellResult::ok(&output)
}

// ── chmod ───────────────────────────────────────────────────────
pub fn chmod(args: &[String]) -> ShellResult {
    let _recursive = args.contains(&String::from("-R"));
    let positional: Vec<&String> = args
        .iter()
        .filter(|a| !a.starts_with('-') || a.parse::<u32>().is_ok())
        .collect();

    if positional.len() < 2 {
        return ShellResult::err("chmod: missing operand\nUsage: chmod [-R] <mode> <file...>");
    }
    // Accept the command gracefully (VFS doesn't yet expose set_permissions)
    ShellResult::ok("")
}

// ── chown ───────────────────────────────────────────────────────
pub fn chown(args: &[String]) -> ShellResult {
    let positional: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();
    if positional.len() < 2 {
        return ShellResult::err(
            "chown: missing operand\nUsage: chown [-R] <owner[:group]> <file...>",
        );
    }
    // Accept gracefully — ownership is kernel-managed
    ShellResult::ok("")
}

// ── du ──────────────────────────────────────────────────────────
pub fn du(args: &[String]) -> ShellResult {
    let human = args.contains(&String::from("-h"));
    let summary = args.contains(&String::from("-s"));
    let paths: Vec<String> = args
        .iter()
        .filter(|a| !a.starts_with('-'))
        .cloned()
        .collect();

    let target = if paths.is_empty() {
        ENV_VARS
            .lock()
            .get("PWD")
            .cloned()
            .unwrap_or_else(|| String::from("/"))
    } else {
        resolve_path(&paths[0])
    };

    let vfs = crate::vfs::VFS.lock();
    let mut output = String::new();
    let total = du_recursive(&vfs, &target, summary, human, &mut output);

    if summary {
        let size_str = if human {
            format_size_human(total)
        } else {
            alloc::format!("{}", total / 1024)
        };
        writeln!(output, "{}\t{}", size_str, target).unwrap();
    }

    ShellResult::ok(&output)
}

fn du_recursive(
    vfs: &crate::vfs::VirtualFS,
    path: &str,
    summary: bool,
    human: bool,
    output: &mut String,
) -> u64 {
    let mut total: u64 = 0;

    if let Some(entries) = vfs.list_dir(path) {
        for entry in &entries {
            if entry == "." || entry == ".." {
                continue;
            }
            let child = if path == "/" {
                alloc::format!("/{}", entry)
            } else {
                alloc::format!("{}/{}", path, entry)
            };

            if vfs.list_dir(&child).is_some() {
                let sub = du_recursive(vfs, &child, summary, human, output);
                total += sub;
                if !summary {
                    let size_str = if human {
                        format_size_human(sub)
                    } else {
                        alloc::format!("{}", sub / 1024)
                    };
                    writeln!(output, "{}\t{}", size_str, child).unwrap();
                }
            } else if let Ok(st) = vfs.stat(&child) {
                total += st.size;
            }
        }
    }

    total += 4096;
    total
}

fn format_size_human(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        alloc::format!(
            "{}.{}G",
            bytes / (1024 * 1024 * 1024),
            (bytes % (1024 * 1024 * 1024)) * 10 / (1024 * 1024 * 1024)
        )
    } else if bytes >= 1024 * 1024 {
        alloc::format!(
            "{}.{}M",
            bytes / (1024 * 1024),
            (bytes % (1024 * 1024)) * 10 / (1024 * 1024)
        )
    } else if bytes >= 1024 {
        alloc::format!("{}.{}K", bytes / 1024, (bytes % 1024) * 10 / 1024)
    } else {
        alloc::format!("{}B", bytes)
    }
}

// ── mktemp ──────────────────────────────────────────────────────
pub fn mktemp(args: &[String]) -> ShellResult {
    let dir = args.contains(&String::from("-d"));
    let template = args
        .iter()
        .find(|a| !a.starts_with('-'))
        .map(|s| s.as_str())
        .unwrap_or("tmp.XXXXXXXXXX");

    // Generate a pseudo-random suffix
    let suffix = {
        let mut h: u64 = 0xcbf29ce484222325;
        for &b in template.as_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        h ^= h >> 16;
        alloc::format!("{:08x}", h as u32)
    };

    let name = if template.contains("XXXX") {
        template
            .replace("XXXXXXXXXX", &suffix)
            .replace("XXXX", &suffix[..4])
    } else {
        alloc::format!("{}.{}", template, suffix)
    };

    let path = alloc::format!("/tmp/{}", name);

    if dir {
        let mut vfs = crate::vfs::VFS.lock();
        let _ = vfs.mkdir(&path, 0o700);
    } else {
        let mut vfs = crate::vfs::VFS.lock();
        vfs.write_file(&path, b"");
    }

    ShellResult::ok(&path)
}

// ── file ────────────────────────────────────────────────────────
pub fn file(args: &[String]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::err("file: missing file operand");
    }
    let vfs = crate::vfs::VFS.lock();
    let mut output = String::new();

    for arg in args {
        if arg.starts_with('-') {
            continue;
        }
        let path = resolve_path(arg);
        if vfs.list_dir(&path).is_some() {
            writeln!(output, "{}: directory", arg).unwrap();
        } else if let Some(data) = vfs.read_file(&path) {
            let desc = if data.len() >= 4
                && data[0] == 0x7f
                && data[1] == b'E'
                && data[2] == b'L'
                && data[3] == b'F'
            {
                "ELF executable"
            } else if data.len() >= 2 && data[0] == 0x1f && data[1] == 0x8b {
                "gzip compressed data"
            } else if data.len() >= 4
                && data[0] == 0x50
                && data[1] == 0x4b
                && data[2] == 0x03
                && data[3] == 0x04
            {
                "Zip archive"
            } else if data.len() >= 8
                && data[0] == 0x89
                && data[1] == b'P'
                && data[2] == b'N'
                && data[3] == b'G'
            {
                "PNG image data"
            } else if data.len() >= 3 && data[0] == 0xff && data[1] == 0xd8 && data[2] == 0xff {
                "JPEG image data"
            } else if data.len() >= 4
                && data[0] == b'R'
                && data[1] == b'I'
                && data[2] == b'F'
                && data[3] == b'F'
            {
                "RIFF media data"
            } else if data.len() >= 2 && data[0] == b'#' && data[1] == b'!' {
                "script, executable"
            } else if data
                .iter()
                .take(512.min(data.len()))
                .all(|&b| b.is_ascii() || b == b'\n' || b == b'\r' || b == b'\t')
            {
                if data.is_empty() {
                    "empty"
                } else {
                    "ASCII text"
                }
            } else {
                "data"
            };
            writeln!(output, "{}: {}", arg, desc).unwrap();
        } else {
            writeln!(output, "{}: cannot open (No such file or directory)", arg).unwrap();
        }
    }
    ShellResult::ok(&output)
}

// ── more / less ─────────────────────────────────────────────────
pub fn less(args: &[String]) -> ShellResult {
    // In kernel shell, less/more just outputs the full file (no pagination)
    if args.is_empty() {
        return ShellResult::err("less: missing file operand");
    }
    cat(args)
}

// ── dd ──────────────────────────────────────────────────────────
pub fn dd(args: &[String]) -> ShellResult {
    let mut input_file = String::new();
    let mut output_file = String::new();
    let mut bs: usize = 512;
    let mut count: usize = 0;

    for arg in args {
        if arg.starts_with("if=") {
            input_file = arg[3..].to_string();
        } else if arg.starts_with("of=") {
            output_file = arg[3..].to_string();
        } else if arg.starts_with("bs=") {
            bs = parse_size_spec(&arg[3..]);
        } else if arg.starts_with("count=") {
            count = arg[6..].parse().unwrap_or(0);
        }
    }

    if input_file.is_empty() || output_file.is_empty() {
        return ShellResult::err("dd: usage: dd if=<input> of=<output> [bs=N] [count=N]");
    }

    let in_path = resolve_path(&input_file);
    let out_path = resolve_path(&output_file);

    let vfs = crate::vfs::VFS.lock();
    if let Some(data) = vfs.read_file(&in_path) {
        let total = if count > 0 {
            (count * bs).min(data.len())
        } else {
            data.len()
        };
        let chunk = data[..total].to_vec();
        let bytes_copied = chunk.len();
        drop(vfs);
        let mut mvfs = crate::vfs::VFS.lock();
        mvfs.write_file(&out_path, &chunk);
        let records = (bytes_copied + bs - 1) / bs;
        let mut output = String::new();
        writeln!(output, "{}+0 records in", records).unwrap();
        writeln!(output, "{}+0 records out", records).unwrap();
        writeln!(output, "{} bytes copied", bytes_copied).unwrap();
        ShellResult::ok(&output)
    } else {
        ShellResult::err(&alloc::format!("dd: {}: No such file", input_file))
    }
}

fn parse_size_spec(s: &str) -> usize {
    let s = s.trim();
    if s.ends_with('K') || s.ends_with('k') {
        s[..s.len() - 1].parse::<usize>().unwrap_or(1) * 1024
    } else if s.ends_with('M') || s.ends_with('m') {
        s[..s.len() - 1].parse::<usize>().unwrap_or(1) * 1024 * 1024
    } else if s.ends_with('G') || s.ends_with('g') {
        s[..s.len() - 1].parse::<usize>().unwrap_or(1) * 1024 * 1024 * 1024
    } else {
        s.parse().unwrap_or(512)
    }
}

// ── tar (basic list/extract/create) ─────────────────────────────
pub fn tar(args: &[String]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::err(
            "tar: missing operand\nUsage: tar [-c|-x|-t] -f <archive> [files...]",
        );
    }

    let mut create = false;
    let mut extract = false;
    let mut list = false;
    let mut archive = String::new();
    let mut files = Vec::new();
    let mut verbose = false;
    let mut i = 0;

    while i < args.len() {
        let arg = &args[i];
        if arg.starts_with('-') || (i == 0 && !arg.starts_with('/')) {
            for c in arg.chars() {
                match c {
                    'c' => create = true,
                    'x' => extract = true,
                    't' => list = true,
                    'v' => verbose = true,
                    'f' if i + 1 < args.len() => {
                        archive = args[i + 1].clone();
                        i += 1;
                    }
                    'z' | 'j' | '-' => {} // Accept but ignore compression flags
                    _ => {}
                }
            }
        } else {
            files.push(arg.clone());
        }
        i += 1;
    }

    if archive.is_empty() {
        return ShellResult::err("tar: no archive file specified");
    }

    let archive_path = resolve_path(&archive);

    if list {
        let vfs = crate::vfs::VFS.lock();
        if let Some(data) = vfs.read_file(&archive_path) {
            if let Ok(s) = core::str::from_utf8(data) {
                let mut output = String::new();
                for line in s.lines() {
                    if line.starts_with("TAR:") {
                        writeln!(output, "{}", &line[4..]).unwrap();
                    }
                }
                return ShellResult::ok(&output);
            }
        }
        return ShellResult::err(&alloc::format!("tar: {}: No such file", archive));
    }

    if create {
        let vfs = crate::vfs::VFS.lock();
        let mut content = String::new();
        for file in &files {
            let path = resolve_path(file);
            if let Some(data) = vfs.read_file(&path) {
                writeln!(content, "TAR:{}", file).unwrap();
                if let Ok(s) = core::str::from_utf8(data) {
                    writeln!(content, "DATA:{}", s).unwrap();
                }
            }
        }
        drop(vfs);
        let mut mvfs = crate::vfs::VFS.lock();
        mvfs.write_file(&archive_path, content.as_bytes());
        let mut output = String::new();
        if verbose {
            for file in &files {
                writeln!(output, "{}", file).unwrap();
            }
        }
        return ShellResult::ok(&output);
    }

    if extract {
        let vfs = crate::vfs::VFS.lock();
        if let Some(data) = vfs.read_file(&archive_path) {
            if let Ok(s) = core::str::from_utf8(data) {
                let mut current_file = String::new();
                let mut current_data = String::new();
                let mut extracted: Vec<(String, String)> = Vec::new();

                for line in s.lines() {
                    if line.starts_with("TAR:") {
                        if !current_file.is_empty() {
                            extracted.push((current_file.clone(), current_data.clone()));
                            current_data.clear();
                        }
                        current_file = line[4..].to_string();
                    } else if line.starts_with("DATA:") {
                        current_data.push_str(&line[5..]);
                        current_data.push('\n');
                    }
                }
                if !current_file.is_empty() {
                    extracted.push((current_file, current_data));
                }

                drop(vfs);
                let mut mvfs = crate::vfs::VFS.lock();
                let mut output = String::new();
                for (name, data) in &extracted {
                    let path = resolve_path(name);
                    mvfs.write_file(&path, data.as_bytes());
                    if verbose {
                        writeln!(output, "{}", name).unwrap();
                    }
                }
                return ShellResult::ok(&output);
            }
        }
        return ShellResult::err(&alloc::format!("tar: {}: No such file", archive));
    }

    ShellResult::err("tar: must specify one of -c, -x, -t")
}

// ── umask ───────────────────────────────────────────────────────
pub fn umask(args: &[String]) -> ShellResult {
    if args.is_empty() {
        ShellResult::ok("0022")
    } else {
        // Accept and store umask
        ShellResult::ok("")
    }
}

// ════════════════════════════════════════════════════════════════
// Stdin-aware variants for pipe support
// ════════════════════════════════════════════════════════════════

/// cat with stdin support — if no file args and stdin is provided, output stdin
pub fn cat_with_stdin(args: &[String], stdin_data: Option<&str>) -> ShellResult {
    if args.is_empty() {
        if let Some(data) = stdin_data {
            return ShellResult::ok(data);
        }
        return ShellResult::ok("");
    }
    cat(args)
}

/// head with stdin support — reads from stdin when no file args
pub fn head_with_stdin(args: &[String], stdin_data: Option<&str>) -> ShellResult {
    let mut lines = 10usize;
    let mut files = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-n" if i + 1 < args.len() => {
                lines = args[i + 1].parse().unwrap_or(10);
                i += 2;
                continue;
            }
            _ if !args[i].starts_with('-') => files.push(args[i].clone()),
            _ => {}
        }
        i += 1;
    }

    // If no files and stdin data available, use stdin
    if files.is_empty() {
        if let Some(data) = stdin_data {
            let mut output = String::new();
            for (j, line) in data.lines().enumerate() {
                if j >= lines {
                    break;
                }
                writeln!(output, "{}", line).unwrap();
            }
            return ShellResult::ok(&output);
        }
    }

    // Fall through to file-based head
    head(args)
}

/// tail with stdin support — reads from stdin when no file args
pub fn tail_with_stdin(args: &[String], stdin_data: Option<&str>) -> ShellResult {
    let mut lines = 10usize;
    let mut files = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-n" if i + 1 < args.len() => {
                lines = args[i + 1].parse().unwrap_or(10);
                i += 2;
                continue;
            }
            _ if !args[i].starts_with('-') => files.push(args[i].clone()),
            _ => {}
        }
        i += 1;
    }

    if files.is_empty() {
        if let Some(data) = stdin_data {
            let mut output = String::new();
            let all_lines: Vec<&str> = data.lines().collect();
            let start = if all_lines.len() > lines {
                all_lines.len() - lines
            } else {
                0
            };
            for line in &all_lines[start..] {
                writeln!(output, "{}", line).unwrap();
            }
            return ShellResult::ok(&output);
        }
    }

    tail(args)
}

/// wc with stdin support — counts lines/words/bytes from stdin
pub fn wc_with_stdin(args: &[String], stdin_data: Option<&str>) -> ShellResult {
    // Check if there are any non-flag arguments (i.e., file names)
    let has_files = args.iter().any(|a| !a.starts_with('-'));

    if !has_files {
        if let Some(data) = stdin_data {
            let lines = data.lines().count();
            let words = data.split_whitespace().count();
            let bytes = data.len();
            let output = alloc::format!(" {:>7} {:>7} {:>7}\n", lines, words, bytes);
            return ShellResult::ok(&output);
        }
    }

    wc(args)
}

/// grep with stdin support — searches stdin when no file args
pub fn grep_with_stdin(args: &[String], stdin_data: Option<&str>) -> ShellResult {
    if args.is_empty() {
        return ShellResult::err("grep: missing pattern");
    }

    let mut case_insensitive = false;
    let mut invert = false;
    let mut count_only = false;
    let mut line_numbers = false;
    let mut pattern_idx = 0;

    for (i, arg) in args.iter().enumerate() {
        match arg.as_str() {
            "-i" => case_insensitive = true,
            "-v" => invert = true,
            "-c" => count_only = true,
            "-n" => line_numbers = true,
            _ => {
                pattern_idx = i;
                break;
            }
        }
    }

    if pattern_idx >= args.len() {
        return ShellResult::err("grep: missing pattern");
    }

    let pattern = &args[pattern_idx];
    let files = &args[pattern_idx + 1..];

    // If no files specified but stdin data available, grep from stdin
    if files.is_empty() {
        if let Some(data) = stdin_data {
            let mut output = String::new();
            let mut count = 0;

            for (line_num, line) in data.lines().enumerate() {
                let matches = if case_insensitive {
                    line.to_lowercase().contains(&pattern.to_lowercase())
                } else {
                    line.contains(pattern.as_str())
                };
                let matches = if invert { !matches } else { matches };

                if matches {
                    count += 1;
                    if !count_only {
                        if line_numbers {
                            writeln!(output, "{}:{}", line_num + 1, line).unwrap();
                        } else {
                            writeln!(output, "{}", line).unwrap();
                        }
                    }
                }
            }
            if count_only {
                writeln!(output, "{}", count).unwrap();
            }
            if count == 0 {
                return ShellResult::with_code(1, &output);
            }
            return ShellResult::ok(&output);
        }
    }

    grep(args)
}
