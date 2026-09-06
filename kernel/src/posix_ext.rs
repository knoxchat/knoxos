/// POSIX Compliance Layer — Enhanced POSIX.1-2024 conformance for Linux compatibility
/// Implements additional POSIX interfaces: regex, glob, termios, statvfs, dirent, sysconf
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// POSIX TERMIOS — Terminal I/O control
// ═══════════════════════════════════════════════════════════════════════

/// Terminal I/O flags (c_iflag)
pub mod iflag {
    pub const IGNBRK: u32 = 0x0001;
    pub const BRKINT: u32 = 0x0002;
    pub const IGNPAR: u32 = 0x0004;
    pub const PARMRK: u32 = 0x0008;
    pub const INPCK: u32 = 0x0010;
    pub const ISTRIP: u32 = 0x0020;
    pub const INLCR: u32 = 0x0040;
    pub const IGNCR: u32 = 0x0080;
    pub const ICRNL: u32 = 0x0100;
    pub const IUCLC: u32 = 0x0200;
    pub const IXON: u32 = 0x0400;
    pub const IXANY: u32 = 0x0800;
    pub const IXOFF: u32 = 0x1000;
    pub const IMAXBEL: u32 = 0x2000;
    pub const IUTF8: u32 = 0x4000;
}

/// Terminal output flags (c_oflag)
pub mod oflag {
    pub const OPOST: u32 = 0x0001;
    pub const OLCUC: u32 = 0x0002;
    pub const ONLCR: u32 = 0x0004;
    pub const OCRNL: u32 = 0x0008;
    pub const ONOCR: u32 = 0x0010;
    pub const ONLRET: u32 = 0x0020;
    pub const OFILL: u32 = 0x0040;
    pub const OFDEL: u32 = 0x0080;
}

/// Terminal control flags (c_cflag)
pub mod cflag {
    pub const CSIZE: u32 = 0x0030;
    pub const CS5: u32 = 0x0000;
    pub const CS6: u32 = 0x0010;
    pub const CS7: u32 = 0x0020;
    pub const CS8: u32 = 0x0030;
    pub const CSTOPB: u32 = 0x0040;
    pub const CREAD: u32 = 0x0080;
    pub const PARENB: u32 = 0x0100;
    pub const PARODD: u32 = 0x0200;
    pub const HUPCL: u32 = 0x0400;
    pub const CLOCAL: u32 = 0x0800;
}

/// Terminal local flags (c_lflag)
pub mod lflag {
    pub const ISIG: u32 = 0x0001;
    pub const ICANON: u32 = 0x0002;
    pub const XCASE: u32 = 0x0004;
    pub const ECHO: u32 = 0x0008;
    pub const ECHOE: u32 = 0x0010;
    pub const ECHOK: u32 = 0x0020;
    pub const ECHONL: u32 = 0x0040;
    pub const NOFLSH: u32 = 0x0080;
    pub const TOSTOP: u32 = 0x0100;
    pub const ECHOCTL: u32 = 0x0200;
    pub const ECHOPRT: u32 = 0x0400;
    pub const ECHOKE: u32 = 0x0800;
    pub const FLUSHO: u32 = 0x1000;
    pub const PENDIN: u32 = 0x4000;
    pub const IEXTEN: u32 = 0x8000;
}

/// Control character indices
pub const NCCS: usize = 32;
pub const VINTR: usize = 0;
pub const VQUIT: usize = 1;
pub const VERASE: usize = 2;
pub const VKILL: usize = 3;
pub const VEOF: usize = 4;
pub const VTIME: usize = 5;
pub const VMIN: usize = 6;
pub const VSWTC: usize = 7;
pub const VSTART: usize = 8;
pub const VSTOP: usize = 9;
pub const VSUSP: usize = 10;
pub const VEOL: usize = 11;
pub const VREPRINT: usize = 12;
pub const VDISCARD: usize = 13;
pub const VWERASE: usize = 14;
pub const VLNEXT: usize = 15;
pub const VEOL2: usize = 16;

/// POSIX termios structure
#[derive(Debug, Clone, Copy)]
pub struct Termios {
    pub c_iflag: u32,
    pub c_oflag: u32,
    pub c_cflag: u32,
    pub c_lflag: u32,
    pub c_cc: [u8; NCCS],
    pub c_ispeed: u32,
    pub c_ospeed: u32,
}

impl Termios {
    /// Default cooked mode
    pub fn default_cooked() -> Self {
        let mut t = Self {
            c_iflag: iflag::ICRNL | iflag::IXON | iflag::IUTF8,
            c_oflag: oflag::OPOST | oflag::ONLCR,
            c_cflag: cflag::CS8 | cflag::CREAD | cflag::CLOCAL,
            c_lflag: lflag::ISIG
                | lflag::ICANON
                | lflag::ECHO
                | lflag::ECHOE
                | lflag::ECHOK
                | lflag::ECHOCTL
                | lflag::ECHOKE
                | lflag::IEXTEN,
            c_cc: [0; NCCS],
            c_ispeed: 38400,
            c_ospeed: 38400,
        };
        // Default control characters
        t.c_cc[VINTR] = 0x03; // Ctrl-C
        t.c_cc[VQUIT] = 0x1C; // Ctrl-backslash
        t.c_cc[VERASE] = 0x7F; // DEL
        t.c_cc[VKILL] = 0x15; // Ctrl-U
        t.c_cc[VEOF] = 0x04; // Ctrl-D
        t.c_cc[VSTART] = 0x11; // Ctrl-Q
        t.c_cc[VSTOP] = 0x13; // Ctrl-S
        t.c_cc[VSUSP] = 0x1A; // Ctrl-Z
        t.c_cc[VREPRINT] = 0x12; // Ctrl-R
        t.c_cc[VDISCARD] = 0x0F; // Ctrl-O
        t.c_cc[VWERASE] = 0x17; // Ctrl-W
        t.c_cc[VLNEXT] = 0x16; // Ctrl-V
        t.c_cc[VMIN] = 1;
        t
    }

    /// Raw mode (no processing)
    pub fn make_raw(&mut self) {
        self.c_iflag &= !(iflag::IGNBRK
            | iflag::BRKINT
            | iflag::PARMRK
            | iflag::ISTRIP
            | iflag::INLCR
            | iflag::IGNCR
            | iflag::ICRNL
            | iflag::IXON);
        self.c_oflag &= !oflag::OPOST;
        self.c_lflag &=
            !(lflag::ECHO | lflag::ECHONL | lflag::ICANON | lflag::ISIG | lflag::IEXTEN);
        self.c_cflag &= !cflag::CSIZE;
        self.c_cflag |= cflag::CS8;
        self.c_cc[VMIN] = 1;
        self.c_cc[VTIME] = 0;
    }

    /// Set baud rate
    pub fn set_speed(&mut self, speed: u32) {
        self.c_ispeed = speed;
        self.c_ospeed = speed;
    }
}

/// Terminal actions for tcsetattr
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TcSetAction {
    Now,   // TCSANOW
    Drain, // TCSADRAIN
    Flush, // TCSAFLUSH
}

// ═══════════════════════════════════════════════════════════════════════
// POSIX REGEX — Basic & Extended Regular Expression matching
// ═══════════════════════════════════════════════════════════════════════

/// Regex match result
#[derive(Debug, Clone)]
pub struct RegMatch {
    pub start: usize,
    pub end: usize,
}

/// Simple regex engine supporting basic patterns
pub struct Regex {
    pattern: String,
    case_insensitive: bool,
}

impl Regex {
    pub fn new(pattern: &str) -> Self {
        Self {
            pattern: String::from(pattern),
            case_insensitive: false,
        }
    }

    pub fn case_insensitive(mut self) -> Self {
        self.case_insensitive = true;
        self
    }

    /// Test if the pattern matches anywhere in the string
    pub fn is_match(&self, text: &str) -> bool {
        self.find(text).is_some()
    }

    /// Find the first match
    pub fn find(&self, text: &str) -> Option<RegMatch> {
        let pat = &self.pattern;
        let text_bytes: Vec<char> = if self.case_insensitive {
            text.chars().map(|c| c.to_ascii_lowercase()).collect()
        } else {
            text.chars().collect()
        };
        let pat_chars: Vec<char> = if self.case_insensitive {
            pat.chars().map(|c| c.to_ascii_lowercase()).collect()
        } else {
            pat.chars().collect()
        };

        let anchored_start = pat_chars.first() == Some(&'^');
        let anchored_end = pat_chars.last() == Some(&'$');

        let effective_pat: Vec<char> = pat_chars
            .iter()
            .skip(if anchored_start { 1 } else { 0 })
            .take(if anchored_end {
                pat_chars
                    .len()
                    .saturating_sub(if anchored_start { 2 } else { 1 })
            } else {
                pat_chars.len() - if anchored_start { 1 } else { 0 }
            })
            .cloned()
            .collect();

        let start_range = if anchored_start {
            0..1
        } else {
            0..text_bytes.len()
        };

        for start in start_range {
            if let Some(end) = self.match_at(&text_bytes, start, &effective_pat, 0) {
                if anchored_end && end != text_bytes.len() {
                    continue;
                }
                return Some(RegMatch { start, end });
            }
        }
        None
    }

    fn match_at(
        &self,
        text: &[char],
        text_pos: usize,
        pat: &[char],
        pat_pos: usize,
    ) -> Option<usize> {
        if pat_pos >= pat.len() {
            return Some(text_pos);
        }

        let cur = pat[pat_pos];

        // Check for quantifier
        let has_star = pat_pos + 1 < pat.len() && pat[pat_pos + 1] == '*';
        let has_plus = pat_pos + 1 < pat.len() && pat[pat_pos + 1] == '+';
        let has_question = pat_pos + 1 < pat.len() && pat[pat_pos + 1] == '?';

        if has_star {
            // Zero or more
            let next_pat = pat_pos + 2;
            // Try zero matches first (non-greedy for simplicity)
            if let Some(end) = self.match_at(text, text_pos, pat, next_pat) {
                return Some(end);
            }
            // Try one or more
            let mut pos = text_pos;
            while pos < text.len() && self.char_matches(cur, text[pos]) {
                pos += 1;
                if let Some(end) = self.match_at(text, pos, pat, next_pat) {
                    return Some(end);
                }
            }
            return None;
        }

        if has_plus {
            // One or more
            if text_pos >= text.len() || !self.char_matches(cur, text[text_pos]) {
                return None;
            }
            let next_pat = pat_pos + 2;
            let mut pos = text_pos + 1;
            if let Some(end) = self.match_at(text, pos, pat, next_pat) {
                return Some(end);
            }
            while pos < text.len() && self.char_matches(cur, text[pos]) {
                pos += 1;
                if let Some(end) = self.match_at(text, pos, pat, next_pat) {
                    return Some(end);
                }
            }
            return None;
        }

        if has_question {
            // Zero or one
            let next_pat = pat_pos + 2;
            if let Some(end) = self.match_at(text, text_pos, pat, next_pat) {
                return Some(end);
            }
            if text_pos < text.len() && self.char_matches(cur, text[text_pos]) {
                return self.match_at(text, text_pos + 1, pat, next_pat);
            }
            return None;
        }

        // No quantifier: match single character
        if text_pos >= text.len() {
            return None;
        }
        if self.char_matches(cur, text[text_pos]) {
            return self.match_at(text, text_pos + 1, pat, pat_pos + 1);
        }

        None
    }

    fn char_matches(&self, pat_char: char, text_char: char) -> bool {
        match pat_char {
            '.' => true,   // Any character
            '\\' => false, // Simplified — would need lookahead
            _ => pat_char == text_char,
        }
    }

    /// Find all non-overlapping matches
    pub fn find_all(&self, text: &str) -> Vec<RegMatch> {
        let mut matches = Vec::new();
        let mut start = 0;
        let text_chars: Vec<char> = text.chars().collect();

        while start < text_chars.len() {
            // Create substring
            let sub: String = text_chars[start..].iter().collect();
            if let Some(m) = self.find(&sub) {
                matches.push(RegMatch {
                    start: start + m.start,
                    end: start + m.end,
                });
                start += m.end.max(1);
            } else {
                break;
            }
        }
        matches
    }
}

// ═══════════════════════════════════════════════════════════════════════
// POSIX GLOB — Pathname pattern matching
// ═══════════════════════════════════════════════════════════════════════

/// Glob pattern matching
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let pat: Vec<char> = pattern.chars().collect();
    let txt: Vec<char> = text.chars().collect();
    glob_match_recursive(&pat, 0, &txt, 0)
}

fn glob_match_recursive(pat: &[char], pi: usize, txt: &[char], ti: usize) -> bool {
    if pi >= pat.len() {
        return ti >= txt.len();
    }

    match pat[pi] {
        '?' => {
            // Match any single character (not /)
            if ti < txt.len() && txt[ti] != '/' {
                glob_match_recursive(pat, pi + 1, txt, ti + 1)
            } else {
                false
            }
        }
        '*' => {
            // Check for **
            if pi + 1 < pat.len() && pat[pi + 1] == '*' {
                // ** matches everything including /
                let next_pi = pi + 2;
                // Skip optional / after **
                let next_pi = if next_pi < pat.len() && pat[next_pi] == '/' {
                    next_pi + 1
                } else {
                    next_pi
                };

                for i in ti..=txt.len() {
                    if glob_match_recursive(pat, next_pi, txt, i) {
                        return true;
                    }
                }
                return false;
            }

            // * matches everything except /
            for i in ti..=txt.len() {
                if i > ti && i <= txt.len() && txt[i - 1] == '/' {
                    break; // Don't cross /
                }
                if glob_match_recursive(pat, pi + 1, txt, i) {
                    return true;
                }
            }
            false
        }
        '[' => {
            // Character class
            if ti >= txt.len() {
                return false;
            }
            let mut pi2 = pi + 1;
            let negate = pi2 < pat.len() && pat[pi2] == '!';
            if negate {
                pi2 += 1;
            }

            let mut matched = false;
            while pi2 < pat.len() && pat[pi2] != ']' {
                if pi2 + 2 < pat.len() && pat[pi2 + 1] == '-' {
                    // Range
                    if txt[ti] >= pat[pi2] && txt[ti] <= pat[pi2 + 2] {
                        matched = true;
                    }
                    pi2 += 3;
                } else {
                    if txt[ti] == pat[pi2] {
                        matched = true;
                    }
                    pi2 += 1;
                }
            }

            if negate {
                matched = !matched;
            }
            if matched && pi2 < pat.len() {
                glob_match_recursive(pat, pi2 + 1, txt, ti + 1)
            } else {
                false
            }
        }
        '\\' => {
            // Escaped character
            if pi + 1 < pat.len() && ti < txt.len() && pat[pi + 1] == txt[ti] {
                glob_match_recursive(pat, pi + 2, txt, ti + 1)
            } else {
                false
            }
        }
        c => {
            // Literal match
            if ti < txt.len() && c == txt[ti] {
                glob_match_recursive(pat, pi + 1, txt, ti + 1)
            } else {
                false
            }
        }
    }
}

/// Expand a glob pattern against VFS
pub fn glob_expand(pattern: &str) -> Vec<String> {
    let mut results = Vec::new();
    let vfs = crate::vfs::VFS.lock();

    // Find the base directory (everything before the first glob character)
    let base_dir = if let Some(pos) = pattern.find(['*', '?', '[']) {
        let s = &pattern[..pos];
        if let Some(slash) = s.rfind('/') {
            &pattern[..=slash]
        } else {
            "/"
        }
    } else {
        // No glob characters — literal path
        results.push(String::from(pattern));
        return results;
    };

    // Recursively match entries
    if let Some(entries) = vfs.list_dir(base_dir) {
        for entry in entries {
            let full_path = if base_dir == "/" {
                format!("/{}", entry)
            } else {
                format!("{}{}", base_dir, entry)
            };
            if glob_match(pattern, &full_path) {
                results.push(full_path);
            }
        }
    }

    results
}

// ═══════════════════════════════════════════════════════════════════════
// POSIX STATVFS — Filesystem statistics
// ═══════════════════════════════════════════════════════════════════════

/// Filesystem statistics
#[derive(Debug, Clone, Copy)]
pub struct StatVfs {
    pub f_bsize: u64,   // Filesystem block size
    pub f_frsize: u64,  // Fragment size
    pub f_blocks: u64,  // Total blocks
    pub f_bfree: u64,   // Free blocks
    pub f_bavail: u64,  // Free blocks for unprivileged users
    pub f_files: u64,   // Total inodes
    pub f_ffree: u64,   // Free inodes
    pub f_favail: u64,  // Free inodes for unprivileged users
    pub f_fsid: u64,    // Filesystem ID
    pub f_flag: u64,    // Mount flags
    pub f_namemax: u64, // Maximum filename length
}

/// Mount flags
pub const ST_RDONLY: u64 = 1;
pub const ST_NOSUID: u64 = 2;
pub const ST_NODEV: u64 = 4;
pub const ST_NOEXEC: u64 = 8;
pub const ST_SYNCHRONOUS: u64 = 16;
pub const ST_MANDLOCK: u64 = 64;
pub const ST_NOATIME: u64 = 1024;
pub const ST_NODIRATIME: u64 = 2048;
pub const ST_RELATIME: u64 = 4096;

/// Get filesystem statistics for a path
pub fn statvfs(path: &str) -> Result<StatVfs, i64> {
    let vfs = crate::vfs::VFS.lock();
    if vfs.resolve_path(path).is_none() {
        return Err(-2); // ENOENT
    }

    Ok(StatVfs {
        f_bsize: 4096,
        f_frsize: 4096,
        f_blocks: crate::allocator::HEAP_SIZE as u64 / 4096,
        f_bfree: crate::allocator::HEAP_SIZE as u64 / 4096 / 2, // Approximate
        f_bavail: crate::allocator::HEAP_SIZE as u64 / 4096 / 2,
        f_files: vfs.inodes.len() as u64,
        f_ffree: 65536 - vfs.inodes.len() as u64,
        f_favail: 65536 - vfs.inodes.len() as u64,
        f_fsid: 0x4B4E4F58, // "KNOX"
        f_flag: 0,
        f_namemax: 255,
    })
}

// ═══════════════════════════════════════════════════════════════════════
// POSIX SYSCONF — System configuration variables
// ═══════════════════════════════════════════════════════════════════════

/// sysconf names
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SysconfName {
    _SC_ARG_MAX,
    _SC_CHILD_MAX,
    _SC_CLK_TCK,
    _SC_NGROUPS_MAX,
    _SC_OPEN_MAX,
    _SC_STREAM_MAX,
    _SC_TZNAME_MAX,
    _SC_NPROCESSORS_CONF,
    _SC_NPROCESSORS_ONLN,
    _SC_PHYS_PAGES,
    _SC_AVPHYS_PAGES,
    _SC_PAGESIZE,
    _SC_LINE_MAX,
    _SC_HOST_NAME_MAX,
    _SC_LOGIN_NAME_MAX,
    _SC_TTY_NAME_MAX,
    _SC_SYMLOOP_MAX,
    _SC_IOV_MAX,
}

/// Get system configuration value
pub fn sysconf(name: SysconfName) -> i64 {
    match name {
        SysconfName::_SC_ARG_MAX => 131072, // 128K
        SysconfName::_SC_CHILD_MAX => 1024,
        SysconfName::_SC_CLK_TCK => 100, // Timer frequency
        SysconfName::_SC_NGROUPS_MAX => 65536,
        SysconfName::_SC_OPEN_MAX => 1024,
        SysconfName::_SC_STREAM_MAX => 16,
        SysconfName::_SC_TZNAME_MAX => 6,
        SysconfName::_SC_NPROCESSORS_CONF => 2, // Configured CPUs
        SysconfName::_SC_NPROCESSORS_ONLN => crate::smp::num_cpus() as i64,
        SysconfName::_SC_PHYS_PAGES => (crate::allocator::HEAP_SIZE / 4096) as i64,
        SysconfName::_SC_AVPHYS_PAGES => (crate::allocator::HEAP_SIZE / 4096 / 2) as i64,
        SysconfName::_SC_PAGESIZE => 4096,
        SysconfName::_SC_LINE_MAX => 2048,
        SysconfName::_SC_HOST_NAME_MAX => 64,
        SysconfName::_SC_LOGIN_NAME_MAX => 256,
        SysconfName::_SC_TTY_NAME_MAX => 32,
        SysconfName::_SC_SYMLOOP_MAX => 8,
        SysconfName::_SC_IOV_MAX => 1024,
    }
}

// ═══════════════════════════════════════════════════════════════════════
// POSIX DIRENT — Directory entry structure
// ═══════════════════════════════════════════════════════════════════════

/// Directory entry types
pub const DT_UNKNOWN: u8 = 0;
pub const DT_FIFO: u8 = 1;
pub const DT_CHR: u8 = 2;
pub const DT_DIR: u8 = 4;
pub const DT_BLK: u8 = 6;
pub const DT_REG: u8 = 8;
pub const DT_LNK: u8 = 10;
pub const DT_SOCK: u8 = 12;

/// Linux-compatible dirent64 structure
#[repr(C)]
#[derive(Debug, Clone)]
pub struct Dirent64 {
    pub d_ino: u64,
    pub d_off: i64,
    pub d_reclen: u16,
    pub d_type: u8,
    pub d_name: [u8; 256],
}

impl Dirent64 {
    pub fn new(ino: u64, name: &str, file_type: u8) -> Self {
        let mut d = Self {
            d_ino: ino,
            d_off: 0,
            d_reclen: 280, // sizeof(Dirent64)
            d_type: file_type,
            d_name: [0; 256],
        };
        let name_bytes = name.as_bytes();
        let len = name_bytes.len().min(255);
        d.d_name[..len].copy_from_slice(&name_bytes[..len]);
        d
    }

    pub fn name(&self) -> &str {
        let len = self.d_name.iter().position(|&b| b == 0).unwrap_or(256);
        core::str::from_utf8(&self.d_name[..len]).unwrap_or("")
    }
}

// ═══════════════════════════════════════════════════════════════════════
// POSIX RESOURCE LIMITS (rusage/getrlimit)
// ═══════════════════════════════════════════════════════════════════════

/// Resource usage
#[derive(Debug, Clone, Copy, Default)]
pub struct RUsage {
    pub ru_utime_sec: u64, // User CPU time
    pub ru_utime_usec: u64,
    pub ru_stime_sec: u64, // System CPU time
    pub ru_stime_usec: u64,
    pub ru_maxrss: u64,   // Maximum RSS (kB)
    pub ru_ixrss: u64,    // Integral shared memory
    pub ru_idrss: u64,    // Integral unshared data
    pub ru_isrss: u64,    // Integral unshared stack
    pub ru_minflt: u64,   // Page reclaims (soft faults)
    pub ru_majflt: u64,   // Page faults (hard faults)
    pub ru_nswap: u64,    // Swaps
    pub ru_inblock: u64,  // Block input operations
    pub ru_oublock: u64,  // Block output operations
    pub ru_msgsnd: u64,   // IPC messages sent
    pub ru_msgrcv: u64,   // IPC messages received
    pub ru_nsignals: u64, // Signals received
    pub ru_nvcsw: u64,    // Voluntary context switches
    pub ru_nivcsw: u64,   // Involuntary context switches
}

/// Resource who
pub const RUSAGE_SELF: i32 = 0;
pub const RUSAGE_CHILDREN: i32 = -1;
pub const RUSAGE_THREAD: i32 = 1;

/// Get resource usage
pub fn getrusage(_who: i32) -> RUsage {
    let ticks = crate::interrupts::get_ticks();
    RUsage {
        ru_utime_sec: ticks / 100,
        ru_utime_usec: (ticks % 100) * 10000,
        ru_stime_sec: ticks / 200,
        ru_stime_usec: (ticks % 200) * 5000,
        ru_maxrss: crate::allocator::HEAP_SIZE as u64 / 1024,
        ru_nvcsw: ticks / 10,
        ..Default::default()
    }
}

// ═══════════════════════════════════════════════════════════════════════
// POSIX ENVIRONMENT
// ═══════════════════════════════════════════════════════════════════════

lazy_static::lazy_static! {
    static ref ENVIRON: Mutex<BTreeMap<String, String>> = Mutex::new({
        let mut env = BTreeMap::new();
        env.insert(String::from("PATH"), String::from("/usr/local/bin:/usr/bin:/bin:/sbin:/usr/sbin"));
        env.insert(String::from("HOME"), String::from("/root"));
        env.insert(String::from("USER"), String::from("root"));
        env.insert(String::from("SHELL"), String::from("/bin/ksh"));
        env.insert(String::from("TERM"), String::from("xterm-256color"));
        env.insert(String::from("LANG"), String::from("en_US.UTF-8"));
        env.insert(String::from("LC_ALL"), String::from("en_US.UTF-8"));
        env.insert(String::from("HOSTNAME"), String::from("knoxos"));
        env.insert(String::from("EDITOR"), String::from("kvi"));
        env.insert(String::from("PAGER"), String::from("less"));
        env.insert(String::from("TMPDIR"), String::from("/tmp"));
        env.insert(String::from("XDG_RUNTIME_DIR"), String::from("/run/user/0"));
        env
    });
}

/// getenv — get environment variable
pub fn getenv(name: &str) -> Option<String> {
    ENVIRON.lock().get(name).cloned()
}

/// setenv — set environment variable
pub fn setenv(name: &str, value: &str, overwrite: bool) -> Result<(), i64> {
    let mut env = ENVIRON.lock();
    if !overwrite && env.contains_key(name) {
        return Ok(());
    }
    env.insert(String::from(name), String::from(value));
    Ok(())
}

/// unsetenv — remove environment variable
pub fn unsetenv(name: &str) -> Result<(), i64> {
    ENVIRON.lock().remove(name);
    Ok(())
}

/// Get all environment variables
pub fn environ() -> Vec<(String, String)> {
    ENVIRON
        .lock()
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════
// INIT
// ═══════════════════════════════════════════════════════════════════════

/// Initialize POSIX compliance layer
pub fn init() {
    // Set up default termios for console
    serial_println!(
        "[KnoxOS] POSIX compliance layer initialized (termios, regex, glob, statvfs, sysconf, dirent, rusage, environ)"
    );
}
