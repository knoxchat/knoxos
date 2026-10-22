/// Man Pages — Built-in manual page system
///
/// Provides:
///   - In-memory man page storage
///   - Formatted text display
///   - Section-based organization (1=commands, 2=syscalls, 5=config, 8=admin)
///   - Built-in man pages for all shell builtins
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// MAN PAGE TYPES
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ManSection {
    Commands = 1,         // User commands
    SystemCalls = 2,      // System calls
    LibraryFunctions = 3, // Library functions
    Devices = 4,          // Device files
    FileFormats = 5,      // File formats
    Games = 6,            // Games
    Miscellaneous = 7,    // Miscellaneous
    Administration = 8,   // System administration
}

impl ManSection {
    pub fn name(&self) -> &'static str {
        match self {
            ManSection::Commands => "User Commands",
            ManSection::SystemCalls => "System Calls",
            ManSection::LibraryFunctions => "Library Functions",
            ManSection::Devices => "Special Files",
            ManSection::FileFormats => "File Formats",
            ManSection::Games => "Games",
            ManSection::Miscellaneous => "Miscellaneous",
            ManSection::Administration => "System Administration",
        }
    }

    pub fn from_num(n: u8) -> Option<Self> {
        match n {
            1 => Some(ManSection::Commands),
            2 => Some(ManSection::SystemCalls),
            3 => Some(ManSection::LibraryFunctions),
            4 => Some(ManSection::Devices),
            5 => Some(ManSection::FileFormats),
            6 => Some(ManSection::Games),
            7 => Some(ManSection::Miscellaneous),
            8 => Some(ManSection::Administration),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ManPage {
    pub name: String,
    pub section: ManSection,
    pub synopsis: String,
    pub description: String,
    pub options: Vec<(String, String)>,
    pub examples: Vec<(String, String)>,
    pub see_also: Vec<String>,
}

impl ManPage {
    pub fn new(name: &str, section: ManSection, synopsis: &str, description: &str) -> Self {
        Self {
            name: String::from(name),
            section,
            synopsis: String::from(synopsis),
            description: String::from(description),
            options: Vec::new(),
            examples: Vec::new(),
            see_also: Vec::new(),
        }
    }

    pub fn add_option(&mut self, flag: &str, desc: &str) {
        self.options.push((String::from(flag), String::from(desc)));
    }

    pub fn add_example(&mut self, cmd: &str, desc: &str) {
        self.examples.push((String::from(cmd), String::from(desc)));
    }

    pub fn add_see_also(&mut self, name: &str) {
        self.see_also.push(String::from(name));
    }

    /// Format the man page for display
    pub fn format(&self) -> String {
        let mut out = String::new();

        // Header
        out.push_str(&alloc::format!(
            "{}({}) - KnoxOS Manual\n\n",
            self.name,
            self.section as u8
        ));

        // NAME
        out.push_str("NAME\n");
        out.push_str(&alloc::format!(
            "    {} - {}\n\n",
            self.name,
            self.description.lines().next().unwrap_or("")
        ));

        // SYNOPSIS
        out.push_str("SYNOPSIS\n");
        out.push_str(&alloc::format!("    {}\n\n", self.synopsis));

        // DESCRIPTION
        out.push_str("DESCRIPTION\n");
        for line in self.description.lines() {
            out.push_str(&alloc::format!("    {}\n", line));
        }
        out.push('\n');

        // OPTIONS
        if !self.options.is_empty() {
            out.push_str("OPTIONS\n");
            for (flag, desc) in &self.options {
                out.push_str(&alloc::format!("    {}\n        {}\n\n", flag, desc));
            }
        }

        // EXAMPLES
        if !self.examples.is_empty() {
            out.push_str("EXAMPLES\n");
            for (cmd, desc) in &self.examples {
                out.push_str(&alloc::format!("    $ {}\n        {}\n\n", cmd, desc));
            }
        }

        // SEE ALSO
        if !self.see_also.is_empty() {
            out.push_str("SEE ALSO\n    ");
            out.push_str(&self.see_also.join(", "));
            out.push('\n');
        }

        out
    }
}

// ═══════════════════════════════════════════════════════════════════════
// MAN PAGE DATABASE
// ═══════════════════════════════════════════════════════════════════════

pub struct ManDatabase {
    pages: BTreeMap<(ManSection, String), ManPage>,
}

impl ManDatabase {
    pub fn new() -> Self {
        Self {
            pages: BTreeMap::new(),
        }
    }

    /// Add a man page
    pub fn add(&mut self, page: ManPage) {
        let key = (page.section, page.name.clone());
        self.pages.insert(key, page);
    }

    /// Look up a man page by name (searches all sections)
    pub fn lookup(&self, name: &str) -> Option<&ManPage> {
        // Try section 1 first, then others
        for section_num in [1, 8, 2, 3, 5, 4, 6, 7] {
            if let Some(section) = ManSection::from_num(section_num) {
                let key = (section, String::from(name));
                if let Some(page) = self.pages.get(&key) {
                    return Some(page);
                }
            }
        }
        None
    }

    /// Look up a man page by name and section
    pub fn lookup_section(&self, name: &str, section: ManSection) -> Option<&ManPage> {
        let key = (section, String::from(name));
        self.pages.get(&key)
    }

    /// List all man pages in a section
    pub fn list_section(&self, section: ManSection) -> Vec<&ManPage> {
        self.pages
            .iter()
            .filter(|((s, _), _)| *s == section)
            .map(|(_, page)| page)
            .collect()
    }

    /// Search man pages by keyword
    pub fn search(&self, keyword: &str) -> Vec<&ManPage> {
        let kw = keyword.to_ascii_lowercase();
        self.pages
            .values()
            .filter(|p| {
                p.name.to_ascii_lowercase().contains(&kw)
                    || p.description.to_ascii_lowercase().contains(&kw)
            })
            .collect()
    }

    /// Total number of man pages
    pub fn count(&self) -> usize {
        self.pages.len()
    }
}

lazy_static::lazy_static! {
    pub static ref MAN_DB: Mutex<ManDatabase> = Mutex::new(ManDatabase::new());
}

/// Initialize man pages with built-in entries
pub fn init() {
    let mut db = MAN_DB.lock();

    // Core commands
    let mut ls = ManPage::new(
        "ls",
        ManSection::Commands,
        "ls [OPTIONS] [PATH]",
        "List directory contents.\nDisplays files and directories in the specified path.",
    );
    ls.add_option("-l", "Use long listing format");
    ls.add_option("-a", "Show hidden files (starting with .)");
    ls.add_option("-R", "List recursively");
    ls.add_option("-h", "Human-readable file sizes");
    ls.add_example("ls /home", "List files in /home");
    ls.add_example("ls -la", "List all files in long format");
    ls.add_see_also("cd(1)");
    ls.add_see_also("find(1)");
    db.add(ls);

    let mut cd = ManPage::new(
        "cd",
        ManSection::Commands,
        "cd [DIR]",
        "Change the working directory.\nIf DIR is omitted, changes to home directory.",
    );
    cd.add_example("cd /tmp", "Change to /tmp");
    cd.add_example("cd ..", "Go up one directory");
    cd.add_see_also("pwd(1)");
    db.add(cd);

    let mut pwd = ManPage::new(
        "pwd",
        ManSection::Commands,
        "pwd",
        "Print the current working directory.",
    );
    db.add(pwd);

    let mut cat = ManPage::new(
        "cat",
        ManSection::Commands,
        "cat [FILE...]",
        "Concatenate and display file contents.",
    );
    cat.add_example("cat /etc/hostname", "Display hostname file");
    db.add(cat);

    let mut echo = ManPage::new(
        "echo",
        ManSection::Commands,
        "echo [STRING...]",
        "Display a line of text.\nPrints arguments separated by spaces, followed by a newline.",
    );
    echo.add_option("-n", "Do not output trailing newline");
    echo.add_option("-e", "Enable backslash escape interpretation");
    db.add(echo);

    let mut mkdir = ManPage::new(
        "mkdir",
        ManSection::Commands,
        "mkdir [OPTIONS] DIR...",
        "Create directories.",
    );
    mkdir.add_option("-p", "Create parent directories as needed");
    db.add(mkdir);

    let mut rm = ManPage::new(
        "rm",
        ManSection::Commands,
        "rm [OPTIONS] FILE...",
        "Remove files or directories.",
    );
    rm.add_option("-r, -R", "Remove directories recursively");
    rm.add_option("-f", "Force removal without prompt");
    db.add(rm);

    let mut cp = ManPage::new(
        "cp",
        ManSection::Commands,
        "cp [OPTIONS] SRC DEST",
        "Copy files and directories.",
    );
    cp.add_option("-r, -R", "Copy directories recursively");
    db.add(cp);

    let mut mv = ManPage::new(
        "mv",
        ManSection::Commands,
        "mv SRC DEST",
        "Move or rename files and directories.",
    );
    db.add(mv);

    let mut grep = ManPage::new(
        "grep",
        ManSection::Commands,
        "grep PATTERN [FILE...]",
        "Search for patterns in files.\nPrints lines matching the given pattern.",
    );
    grep.add_option("-i", "Case-insensitive matching");
    grep.add_option("-r", "Recursive search");
    grep.add_option("-n", "Show line numbers");
    grep.add_option("-c", "Show count of matches only");
    db.add(grep);

    let mut ps = ManPage::new(
        "ps",
        ManSection::Commands,
        "ps",
        "Display running processes.\nShows PID, state, and command for all processes.",
    );
    db.add(ps);

    let mut kill = ManPage::new(
        "kill",
        ManSection::Commands,
        "kill [-SIGNAL] PID",
        "Send a signal to a process.\nDefault signal is SIGTERM (15).",
    );
    kill.add_option("-9", "Send SIGKILL (force kill)");
    kill.add_option("-15", "Send SIGTERM (graceful termination)");
    db.add(kill);

    let mut man = ManPage::new(
        "man",
        ManSection::Commands,
        "man [SECTION] NAME",
        "Display manual pages.\nLook up the manual page for a command or topic.",
    );
    man.add_option("-k KEYWORD", "Search man pages by keyword");
    man.add_example("man ls", "Show the manual page for ls");
    man.add_example("man 2 open", "Show the open(2) system call page");
    db.add(man);

    let mut shutdown = ManPage::new(
        "shutdown",
        ManSection::Administration,
        "shutdown [OPTIONS]",
        "Shut down or restart the system.",
    );
    shutdown.add_option("-r", "Reboot instead of shutting down");
    shutdown.add_option("-h", "Halt after shutdown");
    db.add(shutdown);

    drop(db);
    serial_println!(
        "[KnoxOS] Man pages initialized ({} pages)",
        MAN_DB.lock().count()
    );
}
