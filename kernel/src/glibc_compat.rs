/// glibc Compatibility Layer — GNU C Library ABI Compatibility for KnoxOS
///
/// Vivaldi is built against glibc (not musl). This module bridges the gap:
///   - glibc symbol versioning (GLIBC_2.17, GLIBC_2.34, etc.)
///   - __libc_start_main glibc variant (different signature from musl)
///   - glibc-specific TLS model (DTV, tcbhead_t)
///   - NSS (Name Service Switch) stub
///   - glibc-specific math functions (fegetround, etc.)
///   - locale support (LC_*, setlocale, nl_langinfo)
///   - glibc-specific threading (NPTL specifics)
///   - iconv character set conversion
///
/// Strategy: Register versioned symbols in the dynamic linker's global
/// table so that when Vivaldi's ELF references GLIBC_2.17::memcpy,
/// the linker resolves it to our implementation.
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// GLIBC SYMBOL VERSIONING
// ═══════════════════════════════════════════════════════════════════════

/// A versioned symbol definition
#[derive(Debug, Clone)]
pub struct VersionedSymbol {
    pub name: String,
    pub version: String, // e.g., "GLIBC_2.17"
    pub address: u64,    // kernel-provided implementation address
    pub size: u64,
    pub sym_type: SymbolType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolType {
    Function,
    Object,
    TlsObject,
}

/// Symbol version node (from ELF .gnu.version_r section)
#[derive(Debug, Clone)]
pub struct VersionNode {
    pub filename: String, // e.g., "libc.so.6"
    pub entries: Vec<VersionEntry>,
}

#[derive(Debug, Clone)]
pub struct VersionEntry {
    pub name: String, // e.g., "GLIBC_2.17"
    pub hash: u32,
    pub flags: u16,
    pub index: u16,
}

lazy_static::lazy_static! {
    /// Versioned symbol table: "name@@version" → address
    static ref VERSIONED_SYMBOLS: Mutex<BTreeMap<String, VersionedSymbol>> =
        Mutex::new(BTreeMap::new());

    /// Version definitions we provide
    static ref VERSION_DEFS: Mutex<Vec<VersionNode>> = Mutex::new(Vec::new());
}

/// Register a versioned symbol
pub fn register_versioned_symbol(
    name: &str,
    version: &str,
    address: u64,
    size: u64,
    sym_type: SymbolType,
) {
    let key = format!("{}@@{}", name, version);
    VERSIONED_SYMBOLS.lock().insert(
        key,
        VersionedSymbol {
            name: name.to_string(),
            version: version.to_string(),
            address,
            size,
            sym_type,
        },
    );
}

/// Resolve a symbol with version
pub fn resolve_versioned(name: &str, version: Option<&str>) -> Option<u64> {
    let symbols = VERSIONED_SYMBOLS.lock();

    if let Some(ver) = version {
        // Try exact version match
        let key = format!("{}@@{}", name, ver);
        if let Some(sym) = symbols.get(&key) {
            return Some(sym.address);
        }
    }

    // Try default version (highest available)
    let prefix = format!("{}@@", name);
    let mut best: Option<&VersionedSymbol> = None;
    for (key, sym) in symbols.iter() {
        if key.starts_with(&prefix) && (best.is_none() || sym.version > best.unwrap().version) {
            best = Some(sym);
        }
    }

    best.map(|s| s.address)
}

// ═══════════════════════════════════════════════════════════════════════
// GLIBC __libc_start_main
// ═══════════════════════════════════════════════════════════════════════

/// glibc's __libc_start_main signature (simplified):
///   int __libc_start_main(
///     int (*main)(int, char**, char**),
///     int argc,
///     char **argv,
///     void (*init)(void),     // DT_INIT or __libc_csu_init
///     void (*fini)(void),     // DT_FINI or __libc_csu_fini
///     void (*rtld_fini)(void), // cleanup from dynamic linker
///     void *stack_end
///   );
///
/// We register this as a kernel-provided symbol that userspace will call.
/// It sets up the CRT and calls main().
///
/// CRT startup state
#[derive(Debug)]
pub struct GlibcCrtState {
    pub main_fn: u64,
    pub argc: i32,
    pub argv: u64,
    pub init_fn: u64,
    pub fini_fn: u64,
    pub rtld_fini_fn: u64,
    pub stack_end: u64,
}

lazy_static::lazy_static! {
    static ref CRT_STATE: Mutex<Option<GlibcCrtState>> = Mutex::new(None);
}

/// Entry point for __libc_start_main (would be called via syscall ABI)
pub fn libc_start_main(
    main_fn: u64,
    argc: i32,
    argv: u64,
    init_fn: u64,
    fini_fn: u64,
    rtld_fini_fn: u64,
    stack_end: u64,
) -> i32 {
    serial_println!(
        "[glibc] __libc_start_main(main={:#x}, argc={}, argv={:#x})",
        main_fn,
        argc,
        argv
    );

    *CRT_STATE.lock() = Some(GlibcCrtState {
        main_fn,
        argc,
        argv,
        init_fn,
        fini_fn,
        rtld_fini_fn,
        stack_end,
    });

    // In a full implementation:
    // 1. Call init_fn() for DT_INIT / __libc_csu_init
    // 2. Call main(argc, argv, envp)
    // 3. Call exit(return_value)
    // 4. Call fini_fn() and rtld_fini_fn() during exit

    0
}

// ═══════════════════════════════════════════════════════════════════════
// GLIBC TLS MODEL (DTV + tcbhead_t)
// ═══════════════════════════════════════════════════════════════════════

/// glibc TLS control block header (at %fs:0 on x86_64)
/// This is the first thing at the thread pointer (TP).
///
/// struct tcbhead_t {
///     void *tcb;            // Points to itself (self-pointer)
///     dtv_t *dtv;           // Dynamic Thread Vector
///     void *self;           // Same as tcb
///     int multiple_threads; // Whether there are multiple threads
///     int gscope_flag;
///     uintptr_t sysinfo;
///     uintptr_t stack_guard; // Stack canary value
///     uintptr_t pointer_guard;
///     ... (more fields)
/// };
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TcbHead {
    pub tcb: u64,      // self pointer
    pub dtv: u64,      // Dynamic Thread Vector pointer
    pub self_ptr: u64, // self pointer (again)
    pub multiple_threads: i32,
    pub gscope_flag: i32,
    pub sysinfo: u64,
    pub stack_guard: u64, // __stack_chk_guard value
    pub pointer_guard: u64,
}

/// Dynamic Thread Vector entry
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct DtvEntry {
    pub generation: u64, // Generation counter
    pub val: u64,        // Pointer to TLS block (or special value)
}

/// Allocate and initialize a glibc-compatible TLS block
pub fn setup_glibc_tls(stack_guard: u64) -> u64 {
    // Allocate TCB + TLS area
    // glibc places TCB at the end of the TLS block on x86_64:
    //   [TLS data][padding][tcbhead_t] ← TP points here
    //
    // The thread pointer (%fs) points to tcbhead_t.

    let tcb_size = core::mem::size_of::<TcbHead>();
    let dtv_size = 16 * core::mem::size_of::<DtvEntry>(); // 16 module slots
    let total_size = tcb_size + dtv_size + 4096; // TLS + TCB + DTV

    // In a real kernel, this would be allocated from the process heap
    // For now, we describe the layout
    serial_println!(
        "[glibc] TLS block: {} bytes (TCB={}, DTV={})",
        total_size,
        tcb_size,
        dtv_size
    );

    // The TP value (what goes into %fs base via arch_prctl)
    // would be the address of the tcbhead_t
    0 // placeholder
}

// ═══════════════════════════════════════════════════════════════════════
// LOCALE SUPPORT
// ═══════════════════════════════════════════════════════════════════════

/// Locale categories
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LcCategory {
    LcAll = 6,
    LcCollate = 3,
    LcCtype = 0,
    LcMessages = 5,
    LcMonetary = 4,
    LcNumeric = 1,
    LcTime = 2,
}

/// Active locale settings
#[derive(Debug, Clone)]
pub struct LocaleData {
    pub name: String,
    pub encoding: String,
    pub decimal_point: String,
    pub thousands_sep: String,
    pub currency_symbol: String,
    pub date_fmt: String,
    pub time_fmt: String,
}

impl Default for LocaleData {
    fn default() -> Self {
        Self {
            name: String::from("en_US.UTF-8"),
            encoding: String::from("UTF-8"),
            decimal_point: String::from("."),
            thousands_sep: String::from(","),
            currency_symbol: String::from("$"),
            date_fmt: String::from("%m/%d/%Y"),
            time_fmt: String::from("%H:%M:%S"),
        }
    }
}

lazy_static::lazy_static! {
    static ref LOCALE: Mutex<LocaleData> = Mutex::new(LocaleData::default());
}

/// setlocale() implementation
pub fn setlocale(category: i32, locale: &str) -> String {
    let mut current = LOCALE.lock();

    if locale.is_empty() {
        // Query current locale
        return current.name.clone();
    }

    if locale == "C" || locale == "POSIX" {
        current.name = String::from("C");
        current.encoding = String::from("ASCII");
    } else {
        current.name = locale.to_string();
        if locale.contains("UTF-8") || locale.contains("utf8") {
            current.encoding = String::from("UTF-8");
        }
    }

    serial_println!("[glibc] setlocale({}, \"{}\")", category, locale);
    current.name.clone()
}

/// nl_langinfo() — locale information queries
pub fn nl_langinfo(item: i32) -> String {
    let locale = LOCALE.lock();
    match item {
        14 => locale.encoding.clone(), // CODESET
        _ => String::from(""),
    }
}

// ═══════════════════════════════════════════════════════════════════════
// NSS (NAME SERVICE SWITCH) STUBS
// ═══════════════════════════════════════════════════════════════════════

/// NSS result for host lookup
#[derive(Debug, Clone)]
pub struct NssHostResult {
    pub name: String,
    pub aliases: Vec<String>,
    pub addr_type: i32, // AF_INET or AF_INET6
    pub addresses: Vec<[u8; 16]>,
}

/// getaddrinfo() backing — DNS resolution via KnoxOS dns.rs
pub fn nss_resolve_host(hostname: &str) -> Option<NssHostResult> {
    serial_println!("[glibc:nss] Resolving: {}", hostname);

    // Delegate to kernel DNS resolver
    // In a full implementation: crate::dns::resolve(hostname)

    // For localhost, return immediately
    if hostname == "localhost" || hostname == "127.0.0.1" {
        return Some(NssHostResult {
            name: String::from("localhost"),
            aliases: vec![],
            addr_type: 2, // AF_INET
            addresses: vec![[127, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]],
        });
    }

    None
}

/// getpwuid_r() / getpwnam_r() — user database lookup
pub fn nss_getpwnam(name: &str) -> Option<PwEntry> {
    match name {
        "root" => Some(PwEntry {
            name: String::from("root"),
            uid: 0,
            gid: 0,
            home: String::from("/root"),
            shell: String::from("/bin/sh"),
            gecos: String::from("root"),
        }),
        "knoxos" => Some(PwEntry {
            name: String::from("knoxos"),
            uid: 1000,
            gid: 1000,
            home: String::from("/home/knoxos"),
            shell: String::from("/bin/ksh"),
            gecos: String::from("KnoxOS User"),
        }),
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub struct PwEntry {
    pub name: String,
    pub uid: u32,
    pub gid: u32,
    pub home: String,
    pub shell: String,
    pub gecos: String,
}

// ═══════════════════════════════════════════════════════════════════════
// ICONV (Character Set Conversion)
// ═══════════════════════════════════════════════════════════════════════

/// iconv descriptor
pub type IconvT = u32;

static NEXT_ICONV: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(1);

/// iconv_open() — open a conversion descriptor
pub fn iconv_open(tocode: &str, fromcode: &str) -> Result<IconvT, i32> {
    let id = NEXT_ICONV.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    serial_println!(
        "[glibc:iconv] Opened converter: {} → {} (fd={})",
        fromcode,
        tocode,
        id
    );
    Ok(id)
}

/// iconv() — convert character encoding
/// For UTF-8 → UTF-8 (the common case for Vivaldi), this is a no-op copy.
pub fn iconv_convert(_cd: IconvT, input: &[u8], _from: &str, _to: &str) -> Vec<u8> {
    // For the common UTF-8 ↔ UTF-8 case, just copy
    input.to_vec()
}

// ═══════════════════════════════════════════════════════════════════════
// REGISTER GLIBC SYMBOLS
// ═══════════════════════════════════════════════════════════════════════

/// Register all glibc-compatible symbols in the dynamic linker
fn register_all_symbols() {
    // Version nodes we provide (matching glibc's version script)
    let versions = [
        "GLIBC_2.2.5", // Original glibc 2 ABI
        "GLIBC_2.3",
        "GLIBC_2.3.4",
        "GLIBC_2.4",
        "GLIBC_2.5",
        "GLIBC_2.7",
        "GLIBC_2.9",
        "GLIBC_2.10",
        "GLIBC_2.14",
        "GLIBC_2.15",
        "GLIBC_2.16",
        "GLIBC_2.17", // Most common version for Vivaldi deps
        "GLIBC_2.25",
        "GLIBC_2.27",
        "GLIBC_2.28",
        "GLIBC_2.29",
        "GLIBC_2.30",
        "GLIBC_2.33",
        "GLIBC_2.34", // Modern glibc (libpthread merged)
        "GLIBC_2.35",
        "GLIBC_2.36",
        "GLIBC_2.38",
    ];

    // Core libc functions (with their introduction version)
    let core_symbols: &[(&str, &str, SymbolType)] = &[
        // String functions
        ("memcpy", "GLIBC_2.14", SymbolType::Function),
        ("memmove", "GLIBC_2.2.5", SymbolType::Function),
        ("memset", "GLIBC_2.2.5", SymbolType::Function),
        ("memcmp", "GLIBC_2.2.5", SymbolType::Function),
        ("strlen", "GLIBC_2.2.5", SymbolType::Function),
        ("strcmp", "GLIBC_2.2.5", SymbolType::Function),
        ("strncmp", "GLIBC_2.2.5", SymbolType::Function),
        ("strcpy", "GLIBC_2.2.5", SymbolType::Function),
        ("strncpy", "GLIBC_2.2.5", SymbolType::Function),
        ("strcat", "GLIBC_2.2.5", SymbolType::Function),
        ("strchr", "GLIBC_2.2.5", SymbolType::Function),
        ("strrchr", "GLIBC_2.2.5", SymbolType::Function),
        ("strstr", "GLIBC_2.2.5", SymbolType::Function),
        ("strtol", "GLIBC_2.2.5", SymbolType::Function),
        ("strtoul", "GLIBC_2.2.5", SymbolType::Function),
        ("strtod", "GLIBC_2.2.5", SymbolType::Function),
        // Memory management
        ("malloc", "GLIBC_2.2.5", SymbolType::Function),
        ("free", "GLIBC_2.2.5", SymbolType::Function),
        ("calloc", "GLIBC_2.2.5", SymbolType::Function),
        ("realloc", "GLIBC_2.2.5", SymbolType::Function),
        ("posix_memalign", "GLIBC_2.2.5", SymbolType::Function),
        ("aligned_alloc", "GLIBC_2.16", SymbolType::Function),
        ("memalign", "GLIBC_2.2.5", SymbolType::Function),
        ("mmap", "GLIBC_2.2.5", SymbolType::Function),
        ("munmap", "GLIBC_2.2.5", SymbolType::Function),
        ("mprotect", "GLIBC_2.2.5", SymbolType::Function),
        ("mremap", "GLIBC_2.2.5", SymbolType::Function),
        ("madvise", "GLIBC_2.2.5", SymbolType::Function),
        // I/O
        ("read", "GLIBC_2.2.5", SymbolType::Function),
        ("write", "GLIBC_2.2.5", SymbolType::Function),
        ("open", "GLIBC_2.2.5", SymbolType::Function),
        ("close", "GLIBC_2.2.5", SymbolType::Function),
        ("fopen", "GLIBC_2.2.5", SymbolType::Function),
        ("fclose", "GLIBC_2.2.5", SymbolType::Function),
        ("fread", "GLIBC_2.2.5", SymbolType::Function),
        ("fwrite", "GLIBC_2.2.5", SymbolType::Function),
        ("fprintf", "GLIBC_2.2.5", SymbolType::Function),
        ("printf", "GLIBC_2.2.5", SymbolType::Function),
        ("snprintf", "GLIBC_2.2.5", SymbolType::Function),
        // Process
        ("fork", "GLIBC_2.2.5", SymbolType::Function),
        ("execve", "GLIBC_2.2.5", SymbolType::Function),
        ("exit", "GLIBC_2.2.5", SymbolType::Function),
        ("_exit", "GLIBC_2.2.5", SymbolType::Function),
        ("getpid", "GLIBC_2.2.5", SymbolType::Function),
        ("getppid", "GLIBC_2.2.5", SymbolType::Function),
        ("waitpid", "GLIBC_2.2.5", SymbolType::Function),
        // Threading (NPTL)
        ("pthread_create", "GLIBC_2.34", SymbolType::Function),
        ("pthread_join", "GLIBC_2.34", SymbolType::Function),
        ("pthread_detach", "GLIBC_2.34", SymbolType::Function),
        ("pthread_exit", "GLIBC_2.34", SymbolType::Function),
        ("pthread_self", "GLIBC_2.34", SymbolType::Function),
        ("pthread_mutex_init", "GLIBC_2.34", SymbolType::Function),
        ("pthread_mutex_lock", "GLIBC_2.34", SymbolType::Function),
        ("pthread_mutex_unlock", "GLIBC_2.34", SymbolType::Function),
        ("pthread_mutex_destroy", "GLIBC_2.34", SymbolType::Function),
        ("pthread_cond_init", "GLIBC_2.3.2", SymbolType::Function),
        ("pthread_cond_wait", "GLIBC_2.3.2", SymbolType::Function),
        ("pthread_cond_signal", "GLIBC_2.3.2", SymbolType::Function),
        (
            "pthread_cond_broadcast",
            "GLIBC_2.3.2",
            SymbolType::Function,
        ),
        // Signals
        ("signal", "GLIBC_2.2.5", SymbolType::Function),
        ("sigaction", "GLIBC_2.2.5", SymbolType::Function),
        ("kill", "GLIBC_2.2.5", SymbolType::Function),
        // Networking
        ("socket", "GLIBC_2.2.5", SymbolType::Function),
        ("connect", "GLIBC_2.2.5", SymbolType::Function),
        ("bind", "GLIBC_2.2.5", SymbolType::Function),
        ("listen", "GLIBC_2.2.5", SymbolType::Function),
        ("accept", "GLIBC_2.2.5", SymbolType::Function),
        ("send", "GLIBC_2.2.5", SymbolType::Function),
        ("recv", "GLIBC_2.2.5", SymbolType::Function),
        ("sendmsg", "GLIBC_2.2.5", SymbolType::Function),
        ("recvmsg", "GLIBC_2.2.5", SymbolType::Function),
        ("getaddrinfo", "GLIBC_2.2.5", SymbolType::Function),
        ("freeaddrinfo", "GLIBC_2.2.5", SymbolType::Function),
        ("getnameinfo", "GLIBC_2.2.5", SymbolType::Function),
        // Time
        ("clock_gettime", "GLIBC_2.17", SymbolType::Function),
        ("clock_getres", "GLIBC_2.17", SymbolType::Function),
        ("gettimeofday", "GLIBC_2.2.5", SymbolType::Function),
        ("nanosleep", "GLIBC_2.2.5", SymbolType::Function),
        ("time", "GLIBC_2.2.5", SymbolType::Function),
        // Locale
        ("setlocale", "GLIBC_2.2.5", SymbolType::Function),
        ("nl_langinfo", "GLIBC_2.2.5", SymbolType::Function),
        // Dynamic linking
        ("dlopen", "GLIBC_2.34", SymbolType::Function),
        ("dlclose", "GLIBC_2.34", SymbolType::Function),
        ("dlsym", "GLIBC_2.34", SymbolType::Function),
        ("dlerror", "GLIBC_2.34", SymbolType::Function),
        // Misc
        ("getenv", "GLIBC_2.2.5", SymbolType::Function),
        ("setenv", "GLIBC_2.2.5", SymbolType::Function),
        ("abort", "GLIBC_2.2.5", SymbolType::Function),
        ("atexit", "GLIBC_2.2.5", SymbolType::Function),
        ("sysconf", "GLIBC_2.2.5", SymbolType::Function),
        ("prctl", "GLIBC_2.2.5", SymbolType::Function),
        ("ioctl", "GLIBC_2.2.5", SymbolType::Function),
        ("fcntl", "GLIBC_2.2.5", SymbolType::Function),
        // CRT entries
        ("__libc_start_main", "GLIBC_2.34", SymbolType::Function),
        ("__cxa_atexit", "GLIBC_2.2.5", SymbolType::Function),
        ("__cxa_finalize", "GLIBC_2.2.5", SymbolType::Function),
        ("__stack_chk_fail", "GLIBC_2.4", SymbolType::Function),
        ("__stack_chk_guard", "GLIBC_2.4", SymbolType::Object),
        // Error handling
        ("__errno_location", "GLIBC_2.2.5", SymbolType::Function),
        ("strerror", "GLIBC_2.2.5", SymbolType::Function),
        ("perror", "GLIBC_2.2.5", SymbolType::Function),
    ];

    for (name, version, sym_type) in core_symbols {
        // Address 0 = kernel will provide actual implementation via syscall dispatch
        register_versioned_symbol(name, version, 0, 0, *sym_type);

        // Also register in the dynamic linker's global table
        crate::dynlink::register_symbol(name, 0, 0, "libc.so.6");
    }

    serial_println!(
        "[glibc] Registered {} versioned symbols across {} versions",
        core_symbols.len(),
        versions.len()
    );
}

// ═══════════════════════════════════════════════════════════════════════
// ENVIRONMENT VARIABLES FOR GLIBC/VIVALDI
// ═══════════════════════════════════════════════════════════════════════

/// Standard environment variables needed by glibc and Vivaldi
pub fn get_standard_env() -> Vec<(String, String)> {
    let mut env = Vec::new();

    // Basic system
    env.push((String::from("HOME"), String::from("/home/knoxos")));
    env.push((String::from("USER"), String::from("knoxos")));
    env.push((String::from("LOGNAME"), String::from("knoxos")));
    env.push((String::from("SHELL"), String::from("/bin/ksh")));
    env.push((
        String::from("PATH"),
        String::from("/usr/local/bin:/usr/bin:/bin:/opt/vivaldi"),
    ));
    env.push((String::from("LANG"), String::from("en_US.UTF-8")));
    env.push((String::from("LC_ALL"), String::from("en_US.UTF-8")));
    env.push((String::from("TERM"), String::from("xterm-256color")));
    env.push((String::from("HOSTNAME"), String::from("knoxos")));

    // Display / Wayland
    env.push((String::from("WAYLAND_DISPLAY"), String::from("wayland-0")));
    env.push((String::from("XDG_SESSION_TYPE"), String::from("wayland")));
    env.push((String::from("GDK_BACKEND"), String::from("wayland,x11")));
    env.push((String::from("QT_QPA_PLATFORM"), String::from("wayland")));

    // D-Bus
    env.push((
        String::from("DBUS_SESSION_BUS_ADDRESS"),
        String::from("unix:path=/run/user/1000/bus"),
    ));

    // PulseAudio
    env.push((
        String::from("PULSE_SERVER"),
        String::from("unix:/run/user/1000/pulse/native"),
    ));

    // Vivaldi-specific
    env.push((String::from("VIVALDI_FFMPEG_FOUND"), String::from("YES")));
    env.push((
        String::from("CHROME_WRAPPER"),
        String::from("/opt/vivaldi/vivaldi"),
    ));

    // XDG directories
    let xdg = crate::xdg::XdgDirs::default();
    env.extend(xdg.to_env());

    env
}

// ═══════════════════════════════════════════════════════════════════════
// INITIALIZATION
// ═══════════════════════════════════════════════════════════════════════

/// Initialize the glibc compatibility layer
pub fn init() {
    serial_println!("[glibc] Initializing GNU C Library compatibility layer...");

    // Register all versioned symbols
    register_all_symbols();

    // Set up default locale
    setlocale(6, "en_US.UTF-8"); // LC_ALL

    serial_println!(
        "[glibc] {} versioned symbols registered",
        VERSIONED_SYMBOLS.lock().len()
    );
    serial_println!("[glibc] Locale: {}", LOCALE.lock().name);
    serial_println!("[glibc] glibc compatibility layer ready (emulating glibc 2.38)");
}
