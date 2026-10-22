← [Status index](../README.md) · [Do this first](../next.md)

# 8. Shell & Terminal

**Grade: Live (86%)** · `shell/`, `terminal/`, `pty.rs`, `tty.rs`

### Live
- [x] Parser: pipes, redirects, background, quoting
- [x] Scripting: variables, conditionals, loops, glob (`*`, `?`, `[...]`), `export` / PATH
- [x] 60+ builtins (files, text, net, packages, system)
- [x] Terminal grid, split panes, history, completion, highlighting, sixel
- [x] PTY Unix98 pairs, 4 KB rings, termios; ISIG → SIGINT
- [x] Kernel `signals::kill` for Ctrl+C **to kernel tasks**

### Remaining (userspace-shaped)
- [x] Shell as `/bin/sh` in Ring 3 attached to a PTY slave
- [x] Ring 3 `/bin/sh` is the live parser; kernel `compat/shell` implements `ShellExecute` (`GATE_SH ring3 parser`)
- [x] Live `sigreturn` for a custom SIGINT handler (B7)
- [x] Full VT100/xterm-256 — CUP + SGR 256-color + ED (`GATE_VT100 xterm`)
- [x] Here-docs, functions, `~/.profile` (`GATE_HEREDOC expand`, `GATE_PROFILE sourced`)
- [x] Job control against **processes** — SIGTSTP stops; `waitpid(WUNTRACED)` reports `0x137F`; SIGCONT resumes (`GATE_JOB stop`). Ring 3 `pipe()` is live (M2).

---
