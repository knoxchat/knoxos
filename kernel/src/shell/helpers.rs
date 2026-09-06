/// Shell helper utilities — path resolution, prompt generation, permission formatting
use alloc::string::String;
use alloc::vec::Vec;

use super::env::ENV_VARS;

/// Resolve a path relative to the current working directory
pub fn resolve_path(path: &str) -> String {
    if path.starts_with('/') {
        return String::from(path);
    }
    if path == "." {
        return ENV_VARS
            .lock()
            .get("PWD")
            .cloned()
            .unwrap_or_else(|| String::from("/"));
    }
    if path == ".." {
        let pwd = ENV_VARS
            .lock()
            .get("PWD")
            .cloned()
            .unwrap_or_else(|| String::from("/"));
        let parts: Vec<&str> = pwd.trim_end_matches('/').rsplitn(2, '/').collect();
        return if parts.len() > 1 && !parts[1].is_empty() {
            String::from(parts[1])
        } else {
            String::from("/")
        };
    }
    let pwd = ENV_VARS
        .lock()
        .get("PWD")
        .cloned()
        .unwrap_or_else(|| String::from("/"));
    if pwd == "/" {
        alloc::format!("/{}", path)
    } else {
        alloc::format!("{}/{}", pwd, path)
    }
}

/// Resolve a command name to a full path by searching $PATH
pub fn resolve_command_path(name: &str) -> Option<String> {
    // If it contains a slash, use as-is
    if name.contains('/') {
        let vfs = crate::vfs::VFS.lock();
        if vfs.resolve_path(name).is_some() {
            return Some(String::from(name));
        }
        return None;
    }

    // Search PATH
    let path_var = ENV_VARS
        .lock()
        .get("PATH")
        .cloned()
        .unwrap_or_else(|| String::from("/bin:/usr/bin"));
    let vfs = crate::vfs::VFS.lock();
    for dir in path_var.split(':') {
        let full_path = alloc::format!("{}/{}", dir, name);
        if vfs.resolve_path(&full_path).is_some() {
            return Some(full_path);
        }
    }
    None
}

/// Format Unix permissions as `rwxrwxrwx` string
pub fn format_permissions(mode: u16) -> String {
    let mut s = String::with_capacity(9);
    let chars = ['r', 'w', 'x'];
    for shift in (0..3).rev() {
        let bits = (mode >> (shift * 3)) & 7;
        for (i, &ch) in chars.iter().enumerate() {
            if bits & (4 >> i) != 0 {
                s.push(ch);
            } else {
                s.push('-');
            }
        }
    }
    s
}

/// Get the prompt string for display (with ANSI color codes)
/// If PS1 is set to a custom value (not the default), use PS1 parsing.
/// Otherwise, use the built-in colorized prompt.
pub fn get_prompt() -> String {
    let ps1 = ENV_VARS.lock().get("PS1").cloned();
    // If user has set a custom PS1, parse it
    if let Some(ref ps1_val) = ps1 {
        if ps1_val != "\\u@\\h:\\w\\$ " {
            return crate::shell::builtins::core_cmds::parse_ps1();
        }
    }

    // Default: colorized prompt
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

    let sigil = if user == "root" { "#" } else { "$" };

    // Detect git branch
    let git_branch = detect_git_branch(&pwd);
    let git_info = if let Some(branch) = git_branch {
        alloc::format!(" \x1b[1;33m({})\x1b[0m", branch)
    } else {
        String::new()
    };

    // Colorized prompt: user@host in bold green, path in bold cyan, git in yellow, sigil in green
    alloc::format!(
        "\x1b[1;32m{}\x1b[0m@\x1b[1;32m{}\x1b[0m:\x1b[1;34m{}\x1b[0m{} \x1b[1;32m{}\x1b[0m ",
        user,
        hostname,
        display_pwd,
        git_info,
        sigil
    )
}

/// Detect git branch from the current working directory
/// Walks up the directory tree looking for a .git directory or .git/HEAD file
pub fn detect_git_branch(cwd: &str) -> Option<String> {
    let vfs = crate::vfs::VFS.lock();

    // Walk up the directory tree from cwd looking for .git
    let mut dir = String::from(cwd);
    loop {
        let git_head_path = alloc::format!("{}/.git/HEAD", dir);

        // Check if .git/HEAD exists
        if let Some(data) = vfs.read_file(&git_head_path) {
            if let Ok(content) = core::str::from_utf8(data) {
                let content = content.trim();
                // HEAD format: "ref: refs/heads/branch_name" or a commit hash
                if let Some(branch) = content.strip_prefix("ref: refs/heads/") {
                    return Some(String::from(branch));
                } else if content.len() >= 7 {
                    // Detached HEAD — show short hash
                    return Some(alloc::format!("{}...", &content[..7]));
                }
            }
        }

        // Also check for a plain .git file (git worktrees use "gitdir: path")
        let git_file_path = alloc::format!("{}/.git", dir);
        if let Some(ino) = vfs.resolve_path(&git_file_path) {
            if let Some(inode) = vfs.get_inode(ino) {
                if inode.file_type == crate::vfs::FileType::Directory {
                    // .git is a directory but we couldn't read HEAD — might be bare
                    break;
                }
            }
        }

        // Move to parent directory
        if dir == "/" || dir.is_empty() {
            break;
        }
        if let Some(last_slash) = dir.rfind('/') {
            if last_slash == 0 {
                dir = String::from("/");
            } else {
                dir.truncate(last_slash);
            }
        } else {
            break;
        }
    }

    None
}

/// Get a plain prompt (no ANSI codes) for length calculation
pub fn get_prompt_plain() -> String {
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

    let sigil = if user == "root" { "#" } else { "$" };

    // Include git branch info in the plain prompt for accurate length calculation
    let git_branch = detect_git_branch(&pwd);
    let git_info = if let Some(branch) = git_branch {
        alloc::format!(" ({})", branch)
    } else {
        String::new()
    };

    alloc::format!(
        "{}@{}:{}{} {} ",
        user,
        hostname,
        display_pwd,
        git_info,
        sigil
    )
}
