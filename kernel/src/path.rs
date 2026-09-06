/// Unix/Linux Path Implementation — POSIX-compliant path manipulation
///
/// Provides `Path` (borrowed) and `PathBuf` (owned) types modeled after
/// Unix filesystem semantics. All path operations follow POSIX rules:
///   - `/` is the only path separator (never `\`)
///   - `.` is the current directory, `..` is the parent
///   - Paths are case-sensitive
///   - No drive letters, no UNC paths
///   - Symlink-aware resolution
///   - Proper handling of trailing slashes, double slashes, etc.
///
/// Mirrors Rust `std::path` API but for `no_std` bare-metal usage.
use alloc::borrow::ToOwned;
use alloc::string::String;
use alloc::vec::Vec;

/// The Unix path separator
pub const SEPARATOR: char = '/';
pub const SEPARATOR_STR: &str = "/";

// ─── PathBuf (owned, mutable path) ─────────────────────────────────

/// An owned, mutable Unix path (analogous to `std::path::PathBuf`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PathBuf {
    inner: String,
}

impl PathBuf {
    /// Creates an empty `PathBuf`.
    pub fn new() -> Self {
        Self {
            inner: String::new(),
        }
    }

    /// Creates a `PathBuf` from a string.
    pub fn from(s: &str) -> Self {
        Self {
            inner: String::from(s),
        }
    }

    /// Returns this path as a borrowed `Path`.
    pub fn as_path(&self) -> Path<'_> {
        Path::new(&self.inner)
    }

    /// Returns the inner string slice.
    pub fn as_str(&self) -> &str {
        &self.inner
    }

    /// Consumes the `PathBuf`, returning the inner `String`.
    pub fn into_string(self) -> String {
        self.inner
    }

    /// Pushes a path component onto this path.
    /// If `path` is absolute, it replaces the current path.
    /// Otherwise, it is appended with a separator.
    pub fn push(&mut self, path: &str) {
        if path.starts_with('/') {
            // Absolute path replaces everything
            self.inner = String::from(path);
            return;
        }
        if !self.inner.is_empty() && !self.inner.ends_with('/') {
            self.inner.push('/');
        }
        self.inner.push_str(path);
    }

    /// Removes the last component of the path.
    /// Returns `false` if the path is already root or empty.
    pub fn pop(&mut self) -> bool {
        if self.inner.is_empty() || self.inner == "/" {
            return false;
        }
        // Remove trailing slash first
        let trimmed = self.inner.trim_end_matches('/');
        if let Some(pos) = trimmed.rfind('/') {
            if pos == 0 {
                self.inner = String::from("/");
            } else {
                self.inner.truncate(pos);
            }
            true
        } else {
            self.inner.clear();
            true
        }
    }

    /// Sets the file name of this path.
    /// If the path has no file name, this is equivalent to `push`.
    pub fn set_file_name(&mut self, name: &str) {
        if self.as_path().file_name().is_some() {
            self.pop();
        }
        self.push(name);
    }

    /// Sets the extension of the final component.
    pub fn set_extension(&mut self, ext: &str) -> bool {
        let file = match self.as_path().file_name() {
            Some(f) => f,
            None => return false,
        };

        let stem = match self.as_path().file_stem() {
            Some(s) => String::from(s),
            None => return false,
        };

        self.pop();
        if ext.is_empty() {
            self.push(&stem);
        } else {
            let new_name = alloc::format!("{}.{}", stem, ext);
            self.push(&new_name);
        }
        true
    }

    /// Normalizes the path by resolving `.`, `..`, and redundant separators.
    /// Does NOT resolve symlinks (use `canonicalize` for that).
    pub fn normalize(&mut self) {
        let normalized = normalize_path(&self.inner);
        self.inner = normalized;
    }

    /// Returns a normalized copy.
    pub fn normalized(&self) -> PathBuf {
        PathBuf::from(&normalize_path(&self.inner))
    }

    /// Joins this path with another, returning a new `PathBuf`.
    pub fn join(&self, other: &str) -> PathBuf {
        let mut result = self.clone();
        result.push(other);
        result
    }

    /// Returns the length of the path string.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns whether the path is empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl Default for PathBuf {
    fn default() -> Self {
        Self::new()
    }
}

impl core::fmt::Display for PathBuf {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl From<&str> for PathBuf {
    fn from(s: &str) -> Self {
        PathBuf::from(s)
    }
}

impl From<String> for PathBuf {
    fn from(s: String) -> Self {
        PathBuf { inner: s }
    }
}

// ─── Path (borrowed, immutable path) ────────────────────────────────

/// A borrowed reference to a Unix path (analogous to `std::path::Path`).
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct Path<'a> {
    inner: &'a str,
}

impl<'a> Path<'a> {
    /// Creates a new `Path` from a string slice.
    pub fn new(s: &'a str) -> Self {
        Self { inner: s }
    }

    /// Returns the underlying string slice.
    pub fn as_str(&self) -> &str {
        self.inner
    }

    /// Returns an owned `PathBuf`.
    pub fn to_path_buf(&self) -> PathBuf {
        PathBuf::from(self.inner)
    }

    /// Returns `true` if the path is absolute (starts with `/`).
    pub fn is_absolute(&self) -> bool {
        self.inner.starts_with('/')
    }

    /// Returns `true` if the path is relative (does not start with `/`).
    pub fn is_relative(&self) -> bool {
        !self.is_absolute()
    }

    /// Returns the parent directory of this path, or `None` if at root/empty.
    pub fn parent(&self) -> Option<Path<'a>> {
        if self.inner.is_empty() || self.inner == "/" {
            return None;
        }
        let trimmed = self.inner.trim_end_matches('/');
        if let Some(pos) = trimmed.rfind('/') {
            if pos == 0 {
                Some(Path::new("/"))
            } else {
                Some(Path::new(&self.inner[..pos]))
            }
        } else {
            // No slash at all → parent is current directory
            Some(Path::new(""))
        }
    }

    /// Returns the final component of the path, if any.
    /// Returns `None` for `/` or empty path.
    pub fn file_name(&self) -> Option<&str> {
        if self.inner.is_empty() {
            return None;
        }
        let trimmed = self.inner.trim_end_matches('/');
        if trimmed.is_empty() {
            return None; // Root "/"
        }
        match trimmed.rfind('/') {
            Some(pos) => {
                let name = &trimmed[pos + 1..];
                if name.is_empty() { None } else { Some(name) }
            }
            None => Some(trimmed), // No slash → entire string is the file name
        }
    }

    /// Returns the file stem (file name without the final extension).
    /// `.bashrc` → `.bashrc` (leading dot is not an extension separator)
    /// `archive.tar.gz` → `archive.tar`
    pub fn file_stem(&self) -> Option<&str> {
        let name = self.file_name()?;
        // Don't count a leading dot as an extension
        let search_from = if name.starts_with('.') { 1 } else { 0 };
        match name[search_from..].rfind('.') {
            Some(pos) => Some(&name[..search_from + pos]),
            None => Some(name),
        }
    }

    /// Returns the extension of the final component, if any.
    /// `.bashrc` → `None` (leading dot is not an extension)
    /// `archive.tar.gz` → `Some("gz")`
    pub fn extension(&self) -> Option<&str> {
        let name = self.file_name()?;
        let search_from = if name.starts_with('.') { 1 } else { 0 };
        match name[search_from..].rfind('.') {
            Some(pos) => {
                let ext = &name[search_from + pos + 1..];
                if ext.is_empty() { None } else { Some(ext) }
            }
            None => None,
        }
    }

    /// Returns `true` if this path starts with the given prefix.
    pub fn starts_with(&self, prefix: &str) -> bool {
        if prefix.is_empty() {
            return true;
        }
        // Component-aware starts_with
        if self.inner == prefix {
            return true;
        }
        if self.inner.starts_with(prefix) {
            // Must be followed by a separator or be exact
            let next_byte = self.inner.as_bytes().get(prefix.len());
            match next_byte {
                Some(&b'/') => true,
                None => true,
                _ => prefix.ends_with('/'),
            }
        } else {
            false
        }
    }

    /// Returns `true` if this path ends with the given suffix.
    pub fn ends_with(&self, suffix: &str) -> bool {
        if suffix.is_empty() {
            return true;
        }
        if self.inner == suffix {
            return true;
        }
        if self.inner.ends_with(suffix) {
            let offset = self.inner.len() - suffix.len();
            if offset == 0 {
                return true;
            }
            let prev_byte = self.inner.as_bytes().get(offset - 1);
            matches!(prev_byte, Some(&b'/'))
        } else {
            false
        }
    }

    /// Returns `true` if the final component starts with `.`
    pub fn is_hidden(&self) -> bool {
        match self.file_name() {
            Some(name) => name.starts_with('.'),
            None => false,
        }
    }

    /// Iterate over the components of the path.
    pub fn components(&self) -> Components<'a> {
        Components::new(self.inner)
    }

    /// Iterate over the ancestors (path, its parent, grandparent, ..., root).
    pub fn ancestors(&self) -> Ancestors<'a> {
        Ancestors {
            current: Some(*self),
        }
    }

    /// Joins this path with another, returning a new `PathBuf`.
    pub fn join(&self, other: &str) -> PathBuf {
        let mut buf = self.to_path_buf();
        buf.push(other);
        buf
    }

    /// Returns a normalized path that resolves `.` and `..` lexically.
    pub fn normalize(&self) -> PathBuf {
        PathBuf::from(&normalize_path(self.inner))
    }

    /// Strips a prefix from this path, returning the remainder.
    pub fn strip_prefix(&self, prefix: &str) -> Option<&str> {
        if self.inner == prefix {
            return Some("");
        }
        if let Some(rest) = self.inner.strip_prefix(prefix) {
            if let Some(stripped) = rest.strip_prefix('/') {
                Some(stripped)
            } else if prefix.ends_with('/') {
                Some(rest)
            } else {
                None // prefix boundary not at a component
            }
        } else {
            None
        }
    }

    /// Returns the path with the given root prepended if relative.
    pub fn with_root(&self, root: &str) -> PathBuf {
        if self.is_absolute() {
            self.to_path_buf()
        } else {
            let mut buf = PathBuf::from(root);
            buf.push(self.inner);
            buf
        }
    }

    /// Computes a relative path from `base` to `self`.
    /// Both paths should be absolute for meaningful results.
    pub fn relative_to(&self, base: &str) -> Option<PathBuf> {
        if !self.is_absolute() || !base.starts_with('/') {
            return None;
        }

        let self_components = split_components(self.inner);
        let base_components = split_components(base);

        // Find common prefix length
        let common = self_components
            .iter()
            .zip(base_components.iter())
            .take_while(|(a, b)| a == b)
            .count();

        let mut result = PathBuf::new();
        // Go up from base to common ancestor
        for _ in common..base_components.len() {
            result.push("..");
        }
        // Descend into self from common ancestor
        for component in &self_components[common..] {
            result.push(component);
        }

        if result.is_empty() {
            Some(PathBuf::from("."))
        } else {
            Some(result)
        }
    }
}

impl<'a> Copy for Path<'a> {}
impl<'a> Clone for Path<'a> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a> core::fmt::Display for Path<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.inner)
    }
}

// ─── Component types ────────────────────────────────────────────────

/// A single component of a Unix path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Component<'a> {
    /// The root directory `/`
    RootDir,
    /// Current directory `.`
    CurDir,
    /// Parent directory `..`
    ParentDir,
    /// A normal component (file or directory name)
    Normal(&'a str),
}

impl<'a> Component<'a> {
    /// Returns the component as a string slice.
    pub fn as_str(&self) -> &str {
        match self {
            Component::RootDir => "/",
            Component::CurDir => ".",
            Component::ParentDir => "..",
            Component::Normal(s) => s,
        }
    }
}

impl<'a> core::fmt::Display for Component<'a> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Iterator over the components of a path.
pub struct Components<'a> {
    path: &'a str,
    pos: usize,
    emitted_root: bool,
}

impl<'a> Components<'a> {
    fn new(path: &'a str) -> Self {
        Self {
            path,
            pos: 0,
            emitted_root: false,
        }
    }
}

impl<'a> Iterator for Components<'a> {
    type Item = Component<'a>;

    fn next(&mut self) -> Option<Component<'a>> {
        // Emit root if path is absolute and we haven't yet
        if !self.emitted_root && self.path.starts_with('/') {
            self.emitted_root = true;
            self.pos = 1;
            // Skip leading slashes
            while self.pos < self.path.len() && self.path.as_bytes()[self.pos] == b'/' {
                self.pos += 1;
            }
            return Some(Component::RootDir);
        }
        self.emitted_root = true;

        // Skip separators
        while self.pos < self.path.len() && self.path.as_bytes()[self.pos] == b'/' {
            self.pos += 1;
        }

        if self.pos >= self.path.len() {
            return None;
        }

        // Find end of component
        let start = self.pos;
        while self.pos < self.path.len() && self.path.as_bytes()[self.pos] != b'/' {
            self.pos += 1;
        }

        let component = &self.path[start..self.pos];
        match component {
            "." => Some(Component::CurDir),
            ".." => Some(Component::ParentDir),
            _ => Some(Component::Normal(component)),
        }
    }
}

/// Iterator over ancestors of a path.
pub struct Ancestors<'a> {
    current: Option<Path<'a>>,
}

impl<'a> Iterator for Ancestors<'a> {
    type Item = Path<'a>;

    fn next(&mut self) -> Option<Path<'a>> {
        let current = self.current?;
        self.current = current.parent();
        Some(current)
    }
}

// ─── Path normalization ─────────────────────────────────────────────

/// Normalize a path string: resolve `.`, `..`, and redundant separators.
/// Does NOT touch symlinks—this is purely lexical.
///
/// Examples:
///   `/home/user/../etc` → `/etc`
///   `a/b/./c` → `a/b/c`
///   `//a///b` → `/a/b`
///   `/a/b/../../c` → `/c`
///   `..` → `..`
///   `a/../../b` → `../b`
pub fn normalize_path(path: &str) -> String {
    if path.is_empty() {
        return String::from(".");
    }

    let is_absolute = path.starts_with('/');
    let mut stack: Vec<&str> = Vec::new();

    for part in path.split('/') {
        match part {
            "" | "." => continue,
            ".." => {
                if is_absolute {
                    // Can't go above root
                    stack.pop();
                } else if stack.last().is_none_or(|&s| s == "..") {
                    // Relative path: keep `..` if at top or already have `..`
                    stack.push("..");
                } else {
                    stack.pop();
                }
            }
            component => {
                stack.push(component);
            }
        }
    }

    if is_absolute {
        if stack.is_empty() {
            String::from("/")
        } else {
            let mut result = String::new();
            for component in &stack {
                result.push('/');
                result.push_str(component);
            }
            result
        }
    } else if stack.is_empty() {
        String::from(".")
    } else {
        let mut result = String::new();
        for (i, component) in stack.iter().enumerate() {
            if i > 0 {
                result.push('/');
            }
            result.push_str(component);
        }
        result
    }
}

/// Split a path into its non-empty components (for internal use).
fn split_components(path: &str) -> Vec<&str> {
    path.split('/')
        .filter(|s| !s.is_empty() && *s != ".")
        .collect()
}

// ─── Glob pattern matching ──────────────────────────────────────────

/// Match a filename against a Unix glob pattern.
/// Supports:
///   `*`  — matches zero or more characters (not `/`)
///   `?`  — matches exactly one character (not `/`)
///   `[abc]` — character class
///   `[a-z]` — character range
///   `[!abc]` or `[^abc]` — negated character class
///   `**` — matches any number of path components (recursive)
///
/// Returns `true` if `name` matches `pattern`.
pub fn glob_match(pattern: &str, name: &str) -> bool {
    glob_match_recursive(pattern.as_bytes(), name.as_bytes())
}

fn glob_match_recursive(pattern: &[u8], name: &[u8]) -> bool {
    let mut pi = 0;
    let mut ni = 0;

    // Saved positions for backtracking on `*`
    let mut star_pi: Option<usize> = None;
    let mut star_ni: Option<usize> = None;

    while ni < name.len() {
        if pi < pattern.len() {
            match pattern[pi] {
                b'?' => {
                    if name[ni] == b'/' {
                        // `?` doesn't match `/`
                        if let (Some(sp), Some(sn)) = (star_pi, star_ni) {
                            pi = sp + 1;
                            star_ni = Some(sn + 1);
                            ni = sn + 1;
                            continue;
                        }
                        return false;
                    }
                    pi += 1;
                    ni += 1;
                    continue;
                }
                b'*' => {
                    // Check for `**` (globstar)
                    if pi + 1 < pattern.len() && pattern[pi + 1] == b'*' {
                        // `**` matches anything including `/`
                        pi += 2;
                        // Skip optional trailing `/`
                        if pi < pattern.len() && pattern[pi] == b'/' {
                            pi += 1;
                        }
                        // Try matching the rest from every position
                        for i in ni..=name.len() {
                            if glob_match_recursive(&pattern[pi..], &name[i..]) {
                                return true;
                            }
                        }
                        return false;
                    }
                    // Single `*` — doesn't match `/`
                    star_pi = Some(pi);
                    star_ni = Some(ni);
                    pi += 1;
                    continue;
                }
                b'[' => {
                    // Character class
                    pi += 1;
                    let negated =
                        pi < pattern.len() && (pattern[pi] == b'!' || pattern[pi] == b'^');
                    if negated {
                        pi += 1;
                    }
                    let mut matched = false;
                    let mut first = true;
                    while pi < pattern.len() && (first || pattern[pi] != b']') {
                        first = false;
                        if pi + 2 < pattern.len() && pattern[pi + 1] == b'-' {
                            // Range [a-z]
                            if name[ni] >= pattern[pi] && name[ni] <= pattern[pi + 2] {
                                matched = true;
                            }
                            pi += 3;
                        } else {
                            if name[ni] == pattern[pi] {
                                matched = true;
                            }
                            pi += 1;
                        }
                    }
                    if pi < pattern.len() && pattern[pi] == b']' {
                        pi += 1;
                    }
                    if matched == negated {
                        // Mismatch
                        if let (Some(sp), Some(sn)) = (star_pi, star_ni) {
                            pi = sp + 1;
                            star_ni = Some(sn + 1);
                            ni = sn + 1;
                            continue;
                        }
                        return false;
                    }
                    ni += 1;
                    continue;
                }
                c => {
                    if c == name[ni] {
                        pi += 1;
                        ni += 1;
                        continue;
                    }
                    // Mismatch — try backtracking to last `*`
                    if let (Some(sp), Some(sn)) = (star_pi, star_ni) {
                        pi = sp + 1;
                        star_ni = Some(sn + 1);
                        ni = sn + 1;
                        continue;
                    }
                    return false;
                }
            }
        } else {
            // Pattern exhausted but name has more chars
            if let (Some(sp), Some(sn)) = (star_pi, star_ni) {
                pi = sp + 1;
                star_ni = Some(sn + 1);
                ni = sn + 1;
                continue;
            }
            return false;
        }
    }

    // Consume remaining `*` or `**` in pattern
    while pi < pattern.len() && pattern[pi] == b'*' {
        pi += 1;
    }

    pi == pattern.len()
}

/// Expand a glob pattern against VFS entries at a given directory.
/// Returns a sorted list of matching paths.
pub fn glob_expand(pattern: &str) -> Vec<PathBuf> {
    let mut results = Vec::new();

    // Split pattern into directory prefix and glob part
    let (dir, glob_part) = if let Some(pos) = pattern.rfind('/') {
        // Check if there's a glob character before the last /
        let has_glob_before = pattern[..pos].contains('*')
            || pattern[..pos].contains('?')
            || pattern[..pos].contains('[');
        if has_glob_before {
            // Recursive glob — find the first component with a glob
            glob_expand_recursive(pattern, &mut results);
            results.sort();
            return results;
        }
        (&pattern[..pos + 1], &pattern[pos + 1..])
    } else {
        (".", pattern)
    };

    if glob_part.is_empty() {
        results.push(PathBuf::from(dir));
        return results;
    }

    let vfs = crate::vfs::VFS.lock();
    let resolved_dir = if dir == "." {
        crate::shell::helpers::resolve_path(".")
    } else {
        crate::shell::helpers::resolve_path(dir)
    };

    if let Some(entries) = vfs.list_dir(&resolved_dir) {
        for entry in &entries {
            if glob_match(glob_part, entry) {
                let full = if resolved_dir == "/" {
                    alloc::format!("/{}", entry)
                } else {
                    alloc::format!("{}/{}", resolved_dir, entry)
                };
                results.push(PathBuf::from(&full));
            }
        }
    }

    results.sort();
    results
}

fn glob_expand_recursive(pattern: &str, results: &mut Vec<PathBuf>) {
    let parts: Vec<&str> = pattern.split('/').collect();
    glob_expand_parts(&parts, "/", results);
}

fn glob_expand_parts(parts: &[&str], current_dir: &str, results: &mut Vec<PathBuf>) {
    if parts.is_empty() {
        results.push(PathBuf::from(current_dir));
        return;
    }

    let part = parts[0];
    let rest = &parts[1..];

    if part == "**" {
        // Recursive: match current level and all subdirectories
        glob_expand_parts(rest, current_dir, results);
        let vfs = crate::vfs::VFS.lock();
        if let Some(entries) = vfs.list_dir(current_dir) {
            for entry in &entries {
                let child_path = if current_dir == "/" {
                    alloc::format!("/{}", entry)
                } else {
                    alloc::format!("{}/{}", current_dir, entry)
                };
                if vfs.list_dir(&child_path).is_some() {
                    drop(vfs);
                    glob_expand_parts(parts, &child_path, results);
                    return; // Re-acquire VFS lock in recursive call
                }
            }
        }
        return;
    }

    let has_glob = part.contains('*') || part.contains('?') || part.contains('[');
    if !has_glob {
        // Literal component
        let next_dir = if current_dir == "/" {
            alloc::format!("/{}", part)
        } else {
            alloc::format!("{}/{}", current_dir, part)
        };
        if rest.is_empty() {
            let vfs = crate::vfs::VFS.lock();
            if vfs.resolve_path(&next_dir).is_some() {
                results.push(PathBuf::from(&next_dir));
            }
        } else {
            glob_expand_parts(rest, &next_dir, results);
        }
        return;
    }

    // Glob component — match against entries
    let vfs = crate::vfs::VFS.lock();
    if let Some(entries) = vfs.list_dir(current_dir) {
        let matching: Vec<String> = entries
            .iter()
            .filter(|e| glob_match(part, e))
            .cloned()
            .collect();
        drop(vfs);
        for entry in &matching {
            let child_path = if current_dir == "/" {
                alloc::format!("/{}", entry)
            } else {
                alloc::format!("{}/{}", current_dir, entry)
            };
            if rest.is_empty() {
                results.push(PathBuf::from(&child_path));
            } else {
                glob_expand_parts(rest, &child_path, results);
            }
        }
    }
}

// ─── Utility functions ──────────────────────────────────────────────

/// Resolve a path relative to a working directory.
/// If `path` is absolute, it is returned normalized.
/// If relative, it is joined with `cwd` first.
pub fn resolve(path: &str, cwd: &str) -> PathBuf {
    let full = if path.starts_with('/') {
        PathBuf::from(path)
    } else if path.starts_with('~') {
        // Home directory expansion
        let home = crate::shell::env::ENV_VARS
            .lock()
            .get("HOME")
            .cloned()
            .unwrap_or_else(|| String::from("/root"));
        if path == "~" {
            PathBuf::from(&home)
        } else if let Some(rest) = path.strip_prefix("~/") {
            let mut buf = PathBuf::from(&home);
            buf.push(rest);
            buf
        } else {
            // ~user syntax — just use as-is for now
            let mut buf = PathBuf::from(cwd);
            buf.push(path);
            buf
        }
    } else {
        let mut buf = PathBuf::from(cwd);
        buf.push(path);
        buf
    };
    full.normalized()
}

/// Check if a path contains only valid Unix characters.
/// Unix allows any byte except `\0` and `/` in file names.
/// `/` is allowed as a separator. `\0` is never allowed.
pub fn is_valid_filename(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('\0')
        && !name.contains('/')
        && name != "."
        && name != ".."
        && name.len() <= 255 // NAME_MAX on Linux
}

/// Check if a path is valid.
pub fn is_valid_path(path: &str) -> bool {
    if path.is_empty() {
        return false;
    }
    if path.contains('\0') {
        return false;
    }
    if path.len() > 4096 {
        return false; // PATH_MAX on Linux
    }
    // Each component must be <= 255 bytes
    for component in path.split('/') {
        if component.len() > 255 {
            return false;
        }
    }
    true
}

/// Extract the "basename" of a path (final component).
pub fn basename(path: &str) -> &str {
    if path.is_empty() {
        return path;
    }
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return "/";
    }
    match trimmed.rfind('/') {
        Some(pos) => {
            let name = &trimmed[pos + 1..];
            if name.is_empty() { trimmed } else { name }
        }
        None => trimmed,
    }
}

/// Extract the "dirname" of a path (everything before the final component).
pub fn dirname(path: &str) -> &str {
    if let Some(pos) = path.trim_end_matches('/').rfind('/') {
        if pos == 0 { "/" } else { &path[..pos] }
    } else {
        "."
    }
}

/// Join two path segments.
pub fn join(base: &str, child: &str) -> String {
    if child.starts_with('/') {
        return String::from(child);
    }
    if base.is_empty() || base == "." {
        return String::from(child);
    }
    if base.ends_with('/') {
        alloc::format!("{}{}", base, child)
    } else {
        alloc::format!("{}/{}", base, child)
    }
}

/// Compute the common prefix path of two absolute paths.
pub fn common_prefix(a: &str, b: &str) -> String {
    let a_parts = split_components(a);
    let b_parts = split_components(b);

    let common: Vec<&str> = a_parts
        .iter()
        .zip(b_parts.iter())
        .take_while(|(x, y)| x == y)
        .map(|(x, _)| *x)
        .collect();

    if common.is_empty() {
        if a.starts_with('/') && b.starts_with('/') {
            String::from("/")
        } else {
            String::new()
        }
    } else {
        let mut result = String::new();
        if a.starts_with('/') {
            result.push('/');
        }
        for (i, part) in common.iter().enumerate() {
            if i > 0 {
                result.push('/');
            }
            result.push_str(part);
        }
        result
    }
}

/// Initialize the path subsystem.
pub fn init() {
    crate::serial_println!("[KnoxOS] Unix path subsystem initialized (POSIX-compliant)");
}

// ─── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_normalize() {
        assert_eq!(normalize_path("/a/b/../c"), "/c");
        assert_eq!(normalize_path("/a/./b/./c"), "/a/b/c");
        assert_eq!(normalize_path("//a///b"), "/a/b");
        assert_eq!(normalize_path("/a/b/../../c"), "/c");
        assert_eq!(normalize_path("/"), "/");
        assert_eq!(normalize_path(""), ".");
        assert_eq!(normalize_path("a/b/c"), "a/b/c");
        assert_eq!(normalize_path("a/../b"), "b");
        assert_eq!(normalize_path("../a"), "../a");
    }

    #[test_case]
    fn test_path_components() {
        let p = Path::new("/home/user/Documents");
        assert!(p.is_absolute());
        assert_eq!(p.file_name(), Some("Documents"));
        let parent = p.parent().expect("parent");
        assert_eq!(parent.as_str(), "/home/user");
        assert_eq!(p.extension(), None);

        let p2 = Path::new("/etc/config.toml");
        assert_eq!(p2.file_stem(), Some("config"));
        assert_eq!(p2.extension(), Some("toml"));
    }

    #[test_case]
    fn test_glob_match() {
        assert!(glob_match("*.rs", "main.rs"));
        assert!(glob_match("*.rs", "lib.rs"));
        assert!(!glob_match("*.rs", "main.py"));
        assert!(glob_match("test?", "test1"));
        assert!(glob_match("test?", "testA"));
        assert!(!glob_match("test?", "test"));
        assert!(glob_match("[abc]", "a"));
        assert!(!glob_match("[abc]", "d"));
        assert!(glob_match("[a-z]", "m"));
        assert!(!glob_match("[a-z]", "M"));
        assert!(glob_match("[!a-z]", "M"));
    }
}
