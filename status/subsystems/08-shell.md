← [Status index](../README.md) · [Do this first](../next.md)

# 8. Shell & Terminal

**Grade: Live (82%)** · `shell/`, `terminal/`, `pty.rs`, `tty.rs`

### Live
- [x] Parser: pipes, redirects, background, quoting
- [x] Scripting: variables, conditionals, loops, glob (`*`, `?`, `[...]`), `export` / PATH
- [x] 60+ builtins (files, text, net, packages, system)
- [x] Terminal grid, split panes, history, completion, highlighting, sixel
- [x] PTY Unix98 pairs, 4 KB rings, termios; ISIG → SIGINT
- [x] Kernel `signals::kill` for Ctrl+C **to kernel tasks**

### Remaining (userspace-shaped)
- [x] Shell as `/bin/sh` in Ring 3 attached to a PTY slave
- [x] Live `sigreturn` for a custom SIGINT handler (B7)
- [ ] Full VT100/xterm-256
- [ ] Here-docs, functions, `~/.profile` once a real home exists on disk
- [ ] Job control against **processes**, not kernel windows. Ring 3 `pipe()` is live (M2).

---
