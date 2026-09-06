/// Fish-inspired live command syntax highlighting
use alloc::string::String;
use alloc::vec::Vec;

use super::theme::TerminalTheme;
use crate::gui::framebuffer::Pixel;

// ═══════════════════════════════════════════════════════════════════════════
// SYNTAX HIGHLIGHTER — Fish-inspired live command coloring
// ═══════════════════════════════════════════════════════════════════════════

/// Token type for syntax highlighting
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Command,        // First word — valid command (blue)
    InvalidCommand, // First word — unknown command (red)
    Argument,       // Regular argument
    Path,           // Argument that looks like a path
    String,         // Quoted string
    Variable,       // $VAR
    Operator,       // | > >> < & ; &&  ||
    Comment,        // # ...
    Flag,           // -x or --flag
    Whitespace,
}

/// A highlighted token
#[derive(Debug, Clone)]
pub struct HighlightToken {
    pub text: String,
    pub kind: TokenKind,
    pub start: usize,
    pub end: usize,
}

/// Known shell builtins and common commands for syntax validation
pub(crate) const KNOWN_COMMANDS: &[&str] = &[
    "echo",
    "cd",
    "pwd",
    "export",
    "unset",
    "env",
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
    "source",
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
    "ifconfig",
    "ip",
    "ping",
    "yes",
    "seq",
    "dmesg",
    "lsmod",
    "modinfo",
    "insmod",
    "rmmod",
    "lsblk",
    "mount",
    "umount",
    "poweroff",
    "reboot",
    "shutdown",
    "kpm",
    "apt",
    "apt-get",
    "lscgroup",
    "getenforce",
    "sestatus",
    "lspci",
    "glxinfo",
    "xrandr",
    "printenv",
];

/// Perform fish-style syntax highlighting on a command line
pub fn highlight_line(input: &str) -> Vec<HighlightToken> {
    let mut tokens = Vec::new();
    if input.is_empty() {
        return tokens;
    }

    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut pos = 0;
    let mut is_first_word = true;
    let mut after_pipe = false;

    while pos < len {
        let start = pos;

        // Whitespace
        if chars[pos] == ' ' || chars[pos] == '\t' {
            while pos < len && (chars[pos] == ' ' || chars[pos] == '\t') {
                pos += 1;
            }
            tokens.push(HighlightToken {
                text: chars[start..pos].iter().collect(),
                kind: TokenKind::Whitespace,
                start,
                end: pos,
            });
            continue;
        }

        // Comment
        if chars[pos] == '#' && (start == 0 || chars[start - 1] == ' ') {
            let text: String = chars[pos..].iter().collect();
            tokens.push(HighlightToken {
                text,
                kind: TokenKind::Comment,
                start: pos,
                end: len,
            });
            pos = len;
            continue;
        }

        // Operators: | > >> < & ; && ||
        if matches!(chars[pos], '|' | '>' | '<' | '&' | ';') {
            let mut op = String::new();
            op.push(chars[pos]);
            pos += 1;
            // Check for >> || &&
            if pos < len
                && ((chars[pos - 1] == '>' && chars[pos] == '>')
                    || (chars[pos - 1] == '|' && chars[pos] == '|')
                    || (chars[pos - 1] == '&' && chars[pos] == '&'))
            {
                op.push(chars[pos]);
                pos += 1;
            }
            tokens.push(HighlightToken {
                text: op,
                kind: TokenKind::Operator,
                start,
                end: pos,
            });
            if chars[start] == '|' || chars[start] == ';' {
                is_first_word = true;
                after_pipe = true;
            }
            continue;
        }

        // Variable: $VAR or ${VAR}
        if chars[pos] == '$' {
            let mut var = String::new();
            var.push(chars[pos]);
            pos += 1;
            if pos < len && chars[pos] == '{' {
                var.push(chars[pos]);
                pos += 1;
                while pos < len && chars[pos] != '}' {
                    var.push(chars[pos]);
                    pos += 1;
                }
                if pos < len {
                    var.push(chars[pos]);
                    pos += 1;
                }
            } else {
                while pos < len && (chars[pos].is_alphanumeric() || chars[pos] == '_') {
                    var.push(chars[pos]);
                    pos += 1;
                }
            }
            tokens.push(HighlightToken {
                text: var,
                kind: TokenKind::Variable,
                start,
                end: pos,
            });
            is_first_word = false;
            continue;
        }

        // Quoted string
        if chars[pos] == '"' || chars[pos] == '\'' {
            let quote = chars[pos];
            let mut s = String::new();
            s.push(chars[pos]);
            pos += 1;
            while pos < len && chars[pos] != quote {
                if chars[pos] == '\\' && quote == '"' && pos + 1 < len {
                    s.push(chars[pos]);
                    pos += 1;
                }
                s.push(chars[pos]);
                pos += 1;
            }
            if pos < len {
                s.push(chars[pos]);
                pos += 1;
            }
            tokens.push(HighlightToken {
                text: s,
                kind: TokenKind::String,
                start,
                end: pos,
            });
            is_first_word = false;
            continue;
        }

        // Word (command, argument, flag, path)
        let mut word = String::new();
        while pos < len && !matches!(chars[pos], ' ' | '\t' | '|' | '>' | '<' | '&' | ';' | '#') {
            word.push(chars[pos]);
            pos += 1;
        }

        if word.is_empty() {
            pos += 1; // safety: skip unrecognized char
            continue;
        }

        let kind = if is_first_word || after_pipe {
            is_first_word = false;
            after_pipe = false;
            if KNOWN_COMMANDS.contains(&word.as_str()) {
                TokenKind::Command
            } else if word.starts_with('/') || word.starts_with("./") {
                TokenKind::Path
            } else {
                TokenKind::InvalidCommand
            }
        } else if word.starts_with('-') {
            TokenKind::Flag
        } else if word.starts_with('/')
            || word.starts_with("./")
            || word.starts_with("~/")
            || word.contains('/')
        {
            TokenKind::Path
        } else {
            TokenKind::Argument
        };

        tokens.push(HighlightToken {
            text: word,
            kind,
            start,
            end: pos,
        });
    }

    tokens
}

/// Get the color for a token kind
pub fn token_color(kind: TokenKind, theme: &TerminalTheme) -> Pixel {
    match kind {
        TokenKind::Command => theme.command_fg,
        TokenKind::InvalidCommand => theme.error_fg,
        TokenKind::Argument => theme.argument_fg,
        TokenKind::Path => theme.path_fg,
        TokenKind::String => theme.string_fg,
        TokenKind::Variable => theme.variable_fg,
        TokenKind::Operator => theme.operator_fg,
        TokenKind::Comment => theme.comment_fg,
        TokenKind::Flag => theme.foreground,
        TokenKind::Whitespace => theme.foreground,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// ANSI UTILITY — Strip escape sequences to compute visible character count
// ═══════════════════════════════════════════════════════════════════════════

/// Compute the visible character length of a string that may contain ANSI
/// escape sequences. This is used to accurately compute prompt width for
/// cursor positioning, regardless of how many color codes are embedded.
pub fn strip_ansi_visible_len(s: &str) -> usize {
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut visible = 0usize;
    let mut i = 0;
    while i < len {
        if bytes[i] == 0x1b {
            // Skip ESC [ ... <final byte>
            i += 1;
            if i < len && bytes[i] == b'[' {
                i += 1;
                // Skip parameter bytes (0x30–0x3F), intermediate (0x20–0x2F), until final (0x40–0x7E)
                while i < len && !(0x40..=0x7E).contains(&bytes[i]) {
                    i += 1;
                }
                if i < len {
                    i += 1; // skip final byte
                }
            }
        } else {
            visible += 1;
            i += 1;
        }
    }
    visible
}
