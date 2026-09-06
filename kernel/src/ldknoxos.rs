use crate::serial_println;
/// ld-knoxos.so — KnoxOS Dynamic Linker / Runtime Link Editor
///
/// This module provides a kernel-assisted dynamic linker for loading
/// shared libraries and resolving symbols at runtime, compatible with
/// the Linux ELF dynamic linking specification.
///
/// Architecture:
///   1. The kernel loads ld-knoxos.so as the ELF interpreter (PT_INTERP)
///   2. ld-knoxos.so maps all shared libraries into process memory
///   3. Relocations (R_X86_64_RELATIVE, R_X86_64_GLOB_DAT, etc.) are applied
///   4. PLT/GOT entries are patched for lazy binding
///   5. Control transfers to the application _start
///
/// This kernel module provides the infrastructure backing for the
/// dynamic linker:
///   - Shared library search paths (/lib, /usr/lib, /lib64)
///   - Library cache (ld.so.cache equivalent)
///   - Symbol hash table (GNU hash / SysV hash)
///   - Kernel-side mmap for library mapping
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

// ─── ELF Dynamic Section Tags ───────────────────────────────────────────

pub const DT_NULL: u64 = 0;
pub const DT_NEEDED: u64 = 1;
pub const DT_PLTRELSZ: u64 = 2;
pub const DT_PLTGOT: u64 = 3;
pub const DT_HASH: u64 = 4;
pub const DT_STRTAB: u64 = 5;
pub const DT_SYMTAB: u64 = 6;
pub const DT_RELA: u64 = 7;
pub const DT_RELASZ: u64 = 8;
pub const DT_RELAENT: u64 = 9;
pub const DT_STRSZ: u64 = 10;
pub const DT_SYMENT: u64 = 11;
pub const DT_INIT: u64 = 12;
pub const DT_FINI: u64 = 13;
pub const DT_SONAME: u64 = 14;
pub const DT_RPATH: u64 = 15;
pub const DT_SYMBOLIC: u64 = 16;
pub const DT_PLTREL: u64 = 20;
pub const DT_DEBUG: u64 = 21;
pub const DT_JMPREL: u64 = 23;
pub const DT_INIT_ARRAY: u64 = 25;
pub const DT_FINI_ARRAY: u64 = 26;
pub const DT_INIT_ARRAYSZ: u64 = 27;
pub const DT_FINI_ARRAYSZ: u64 = 28;
pub const DT_RUNPATH: u64 = 29;
pub const DT_FLAGS: u64 = 30;
pub const DT_GNU_HASH: u64 = 0x6FFFFEF5;
pub const DT_VERSYM: u64 = 0x6FFFFFF0;
pub const DT_VERNEED: u64 = 0x6FFFFFFE;
pub const DT_VERNEEDNUM: u64 = 0x6FFFFFFF;

// ─── ELF Relocation Types (x86_64) ─────────────────────────────────────

pub const R_X86_64_NONE: u32 = 0;
pub const R_X86_64_64: u32 = 1;
pub const R_X86_64_PC32: u32 = 2;
pub const R_X86_64_GOT32: u32 = 3;
pub const R_X86_64_PLT32: u32 = 4;
pub const R_X86_64_COPY: u32 = 5;
pub const R_X86_64_GLOB_DAT: u32 = 6;
pub const R_X86_64_JUMP_SLOT: u32 = 7;
pub const R_X86_64_RELATIVE: u32 = 8;
pub const R_X86_64_GOTPCREL: u32 = 9;
pub const R_X86_64_32: u32 = 10;
pub const R_X86_64_32S: u32 = 11;
pub const R_X86_64_TPOFF64: u32 = 18;
pub const R_X86_64_DTPMOD64: u32 = 16;
pub const R_X86_64_DTPOFF64: u32 = 17;

// ─── Structures ─────────────────────────────────────────────────────────

/// ELF dynamic table entry
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64Dyn {
    pub d_tag: i64,
    pub d_val: u64, // d_un (union of d_val and d_ptr)
}

/// ELF symbol table entry
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64Sym {
    pub st_name: u32,
    pub st_info: u8,
    pub st_other: u8,
    pub st_shndx: u16,
    pub st_value: u64,
    pub st_size: u64,
}

impl Elf64Sym {
    pub fn bind(&self) -> u8 {
        self.st_info >> 4
    }
    pub fn sym_type(&self) -> u8 {
        self.st_info & 0xf
    }
    pub fn is_undefined(&self) -> bool {
        self.st_shndx == 0
    }
}

/// ELF relocation entry with addend
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64Rela {
    pub r_offset: u64,
    pub r_info: u64,
    pub r_addend: i64,
}

impl Elf64Rela {
    pub fn sym_index(&self) -> u32 {
        (self.r_info >> 32) as u32
    }
    pub fn rel_type(&self) -> u32 {
        (self.r_info & 0xFFFF_FFFF) as u32
    }
}

/// A loaded shared library
#[derive(Debug, Clone)]
pub struct SharedLibrary {
    pub name: String,
    pub path: String,
    pub base_addr: u64,
    pub size: u64,
    pub entry: u64,
    pub init: u64,
    pub fini: u64,
    pub needed: Vec<String>, // DT_NEEDED library names
    pub symbol_count: usize,
    pub ref_count: u32,
}

/// Symbol resolution result
#[derive(Debug, Clone)]
pub struct ResolvedSymbol {
    pub name: String,
    pub address: u64,
    pub size: u64,
    pub library: String,
}

/// Library search path configuration
#[derive(Debug, Clone)]
pub struct LinkConfig {
    pub search_paths: Vec<String>,
    pub preload: Vec<String>, // LD_PRELOAD equivalent
    pub bind_now: bool,       // LD_BIND_NOW (disable lazy binding)
    pub secure_mode: bool,    // AT_SECURE (setuid binary)
}

impl Default for LinkConfig {
    fn default() -> Self {
        Self {
            search_paths: alloc::vec![
                String::from("/lib"),
                String::from("/lib64"),
                String::from("/usr/lib"),
                String::from("/usr/lib64"),
                String::from("/usr/local/lib"),
            ],
            preload: Vec::new(),
            bind_now: false,
            secure_mode: false,
        }
    }
}

// ─── Global State ───────────────────────────────────────────────────────

lazy_static::lazy_static! {
    /// Global library cache (name → loaded library)
    static ref LIBRARY_CACHE: Mutex<BTreeMap<String, SharedLibrary>> =
        Mutex::new(BTreeMap::new());

    /// Global symbol table (name → resolved address)
    static ref GLOBAL_SYMBOLS: Mutex<BTreeMap<String, ResolvedSymbol>> =
        Mutex::new(BTreeMap::new());

    /// Link configuration
    static ref LINK_CONFIG: Mutex<LinkConfig> = Mutex::new(LinkConfig::default());
}

// ─── GNU Hash ───────────────────────────────────────────────────────────

/// Compute GNU hash for a symbol name
pub fn gnu_hash(name: &str) -> u32 {
    let mut h: u32 = 5381;
    for &c in name.as_bytes() {
        h = h.wrapping_mul(33).wrapping_add(c as u32);
    }
    h
}

/// Compute SysV ELF hash for a symbol name
pub fn elf_hash(name: &str) -> u32 {
    let mut h: u32 = 0;
    for &c in name.as_bytes() {
        h = (h << 4).wrapping_add(c as u32);
        let g = h & 0xf000_0000;
        if g != 0 {
            h ^= g >> 24;
        }
        h &= !g;
    }
    h
}

// ─── Dynamic Linker Operations ──────────────────────────────────────────

/// Search for a shared library by name
pub fn find_library(name: &str) -> Option<String> {
    let config = LINK_CONFIG.lock();

    // If it's an absolute path, use it directly
    if name.starts_with('/') {
        // Check if file exists in VFS
        let vfs = crate::vfs::VFS.lock();
        if vfs.resolve_path(name).is_some() {
            return Some(String::from(name));
        }
        return None;
    }

    // Search through search paths
    for path in &config.search_paths {
        let full_path = alloc::format!("{}/{}", path, name);
        let vfs = crate::vfs::VFS.lock();
        if vfs.resolve_path(&full_path).is_some() {
            return Some(full_path);
        }
    }

    None
}

/// Load a shared library into the process address space
pub fn load_library(name: &str, pid: u64) -> Result<SharedLibrary, i32> {
    // Check cache first
    {
        let mut cache = LIBRARY_CACHE.lock();
        if let Some(lib) = cache.get_mut(name) {
            lib.ref_count += 1;
            return Ok(lib.clone());
        }
    }

    // Find the library file
    let path = find_library(name).ok_or(-2i32)?; // ENOENT

    // Read library ELF data
    let elf_data = {
        let vfs = crate::vfs::VFS.lock();
        match vfs.read_file(&path) {
            Some(data) => Vec::from(data),
            None => return Err(-2), // ENOENT
        }
    };

    // Validate ELF header
    if elf_data.len() < 64 || &elf_data[..4] != b"\x7fELF" {
        return Err(-8); // ENOEXEC
    }

    // Check it's a shared object (ET_DYN = 3)
    let e_type = u16::from_le_bytes([elf_data[16], elf_data[17]]);
    if e_type != 3 {
        serial_println!(
            "[ld-knoxos] Not a shared object: {} (type={})",
            name,
            e_type
        );
        return Err(-8);
    }

    // For now, create a library record (actual mapping would use VMM)
    let base_addr = allocate_library_address();

    let lib = SharedLibrary {
        name: String::from(name),
        path: path.clone(),
        base_addr,
        size: elf_data.len() as u64,
        entry: 0,
        init: 0,
        fini: 0,
        needed: Vec::new(),
        symbol_count: 0,
        ref_count: 1,
    };

    // Cache it
    let mut cache = LIBRARY_CACHE.lock();
    cache.insert(String::from(name), lib.clone());

    serial_println!("[ld-knoxos] Loaded library: {} at {:#x}", name, base_addr);
    Ok(lib)
}

/// Unload a shared library
pub fn unload_library(name: &str) -> Result<(), i32> {
    let mut cache = LIBRARY_CACHE.lock();
    if let Some(lib) = cache.get_mut(name) {
        lib.ref_count -= 1;
        if lib.ref_count == 0 {
            serial_println!("[ld-knoxos] Unloading library: {}", name);
            cache.remove(name);
        }
        Ok(())
    } else {
        Err(-2) // ENOENT
    }
}

/// Resolve a symbol by name across all loaded libraries
pub fn resolve_symbol(name: &str) -> Option<ResolvedSymbol> {
    // Check global symbol table first
    {
        let globals = GLOBAL_SYMBOLS.lock();
        if let Some(sym) = globals.get(name) {
            return Some(sym.clone());
        }
    }

    // Search loaded libraries
    let cache = LIBRARY_CACHE.lock();
    for (lib_name, _lib) in cache.iter() {
        // In a full implementation, this would search the library's
        // .dynsym + .dynstr using GNU hash or SysV hash
        serial_println!("[ld-knoxos] Symbol lookup: {} in {}", name, lib_name);
    }

    None
}

/// Register a global symbol (for export from main executable)
pub fn register_symbol(name: &str, address: u64, size: u64, library: &str) {
    let mut globals = GLOBAL_SYMBOLS.lock();
    globals.insert(
        String::from(name),
        ResolvedSymbol {
            name: String::from(name),
            address,
            size,
            library: String::from(library),
        },
    );
}

/// Apply relocations for a loaded library
pub fn apply_relocations(base: u64, rela: &[Elf64Rela], symtab: &[Elf64Sym], _strtab: &[u8]) {
    for rel in rela {
        let target = base + rel.r_offset;

        match rel.rel_type() {
            R_X86_64_RELATIVE => {
                // B + A (base + addend)
                let value = base.wrapping_add(rel.r_addend as u64);
                unsafe {
                    *(target as *mut u64) = value;
                }
            }
            R_X86_64_GLOB_DAT | R_X86_64_JUMP_SLOT => {
                let sym_idx = rel.sym_index() as usize;
                if sym_idx < symtab.len() {
                    let sym = &symtab[sym_idx];
                    if sym.st_value != 0 {
                        let value = base + sym.st_value;
                        unsafe {
                            *(target as *mut u64) = value;
                        }
                    }
                }
            }
            R_X86_64_64 => {
                let sym_idx = rel.sym_index() as usize;
                if sym_idx < symtab.len() {
                    let sym = &symtab[sym_idx];
                    let value = (base + sym.st_value).wrapping_add(rel.r_addend as u64);
                    unsafe {
                        *(target as *mut u64) = value;
                    }
                }
            }
            R_X86_64_NONE => {}
            other => {
                serial_println!("[ld-knoxos] Unsupported relocation type: {}", other);
            }
        }
    }
}

/// Process PT_INTERP — check if binary needs dynamic linker
pub fn needs_dynamic_linking(elf_data: &[u8]) -> bool {
    if elf_data.len() < 64 {
        return false;
    }

    let e_phoff = u64::from_le_bytes(elf_data[32..40].try_into().unwrap_or([0; 8]));
    let e_phentsize = u16::from_le_bytes([elf_data[54], elf_data[55]]);
    let e_phnum = u16::from_le_bytes([elf_data[56], elf_data[57]]);

    for i in 0..e_phnum {
        let offset = e_phoff as usize + (i as usize * e_phentsize as usize);
        if offset + 4 > elf_data.len() {
            break;
        }
        let p_type = u32::from_le_bytes(elf_data[offset..offset + 4].try_into().unwrap_or([0; 4]));
        if p_type == 3 {
            // PT_INTERP
            return true;
        }
        if p_type == 2 {
            // PT_DYNAMIC
            return true;
        }
    }

    false
}

/// dlopen() implementation — load a shared library at runtime
pub fn dlopen(name: &str, pid: u64) -> Result<u64, i32> {
    let lib = load_library(name, pid)?;
    Ok(lib.base_addr)
}

/// dlsym() implementation — find a symbol in a loaded library
pub fn dlsym(handle: u64, symbol: &str) -> Option<u64> {
    let cache = LIBRARY_CACHE.lock();
    for (_name, lib) in cache.iter() {
        if lib.base_addr == handle {
            // In full implementation, search the library's symbol table
            if let Some(resolved) = resolve_symbol(symbol) {
                return Some(resolved.address);
            }
        }
    }
    None
}

/// dlclose() implementation — unload a shared library
pub fn dlclose(handle: u64) -> Result<(), i32> {
    let name = {
        let cache = LIBRARY_CACHE.lock();
        cache
            .iter()
            .find(|(_, lib)| lib.base_addr == handle)
            .map(|(name, _)| name.clone())
    };

    match name {
        Some(name) => unload_library(&name),
        None => Err(-22), // EINVAL
    }
}

// ─── Internal Helpers ───────────────────────────────────────────────────

use core::sync::atomic::{AtomicU64, Ordering};

/// Next available library mapping address
static NEXT_LIB_ADDR: AtomicU64 = AtomicU64::new(0x7f00_0000_0000);

/// Allocate a virtual address range for a library
fn allocate_library_address() -> u64 {
    // Each library gets 64 MiB of address space
    NEXT_LIB_ADDR.fetch_add(0x400_0000, Ordering::SeqCst)
}

/// Get list of all loaded libraries
pub fn loaded_libraries() -> Vec<String> {
    let cache = LIBRARY_CACHE.lock();
    cache.keys().cloned().collect()
}

/// Get library info by name
pub fn library_info(name: &str) -> Option<SharedLibrary> {
    let cache = LIBRARY_CACHE.lock();
    cache.get(name).cloned()
}

// ─── Initialization ─────────────────────────────────────────────────────

pub fn init() {
    serial_println!("[KnoxOS] Dynamic linker (ld-knoxos.so) initialized");
    serial_println!("[KnoxOS]   Search paths: /lib, /usr/lib, /lib64");
    serial_println!("[KnoxOS]   ELF relocation types: RELATIVE, GLOB_DAT, JUMP_SLOT, 64");
    serial_println!("[KnoxOS]   dlopen/dlsym/dlclose: available");
}
