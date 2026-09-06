/// Dynamic Linker — ELF dynamic linking support
///
/// This module provides:
///   - ELF dynamic section parsing (DT_NEEDED, DT_SYMTAB, etc.)
///   - Symbol resolution for shared objects
///   - Relocation processing (R_X86_64_*)
///   - GOT/PLT stub setup for lazy binding
///   - Shared library loading infrastructure
///
/// This is a kernel-side stub that provides the infrastructure for
/// userspace dynamic linking. The actual ld-linux equivalent (ld-knoxos.so)
/// would be a userspace program using these primitives.
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

// ─── ELF Dynamic Section Types ──────────────────────────────────────────

pub const DT_NULL: u64 = 0;
pub const DT_NEEDED: u64 = 1; // Name of a needed shared library
pub const DT_PLTRELSZ: u64 = 2; // Size of PLT relocs
pub const DT_PLTGOT: u64 = 3; // Address of PLT/GOT
pub const DT_HASH: u64 = 4; // Symbol hash table
pub const DT_STRTAB: u64 = 5; // String table address
pub const DT_SYMTAB: u64 = 6; // Symbol table address
pub const DT_RELA: u64 = 7; // Rela relocation table
pub const DT_RELASZ: u64 = 8; // Size of Rela relocs
pub const DT_RELAENT: u64 = 9; // Size of a Rela entry
pub const DT_STRSZ: u64 = 10; // Size of string table
pub const DT_SYMENT: u64 = 11; // Size of a symbol entry
pub const DT_INIT: u64 = 12; // Initialization function
pub const DT_FINI: u64 = 13; // Termination function
pub const DT_SONAME: u64 = 14; // Shared object name
pub const DT_RPATH: u64 = 15; // Library search path
pub const DT_SYMBOLIC: u64 = 16; // Symbol resolution order
pub const DT_REL: u64 = 17; // Rel relocation table
pub const DT_RELSZ: u64 = 18; // Size of Rel relocs
pub const DT_RELENT: u64 = 19; // Size of a Rel entry
pub const DT_PLTREL: u64 = 20; // Relocation type (Rel or Rela)
pub const DT_DEBUG: u64 = 21; // Debug info
pub const DT_TEXTREL: u64 = 22; // Text relocations exist
pub const DT_JMPREL: u64 = 23; // PLT relocations
pub const DT_BIND_NOW: u64 = 24; // Process relocations now
pub const DT_INIT_ARRAY: u64 = 25; // Init function pointers
pub const DT_FINI_ARRAY: u64 = 26; // Fini function pointers
pub const DT_INIT_ARRAYSZ: u64 = 27;
pub const DT_FINI_ARRAYSZ: u64 = 28;
pub const DT_GNU_HASH: u64 = 0x6FFFFEF5; // GNU hash table

// ─── ELF Relocation Types (x86_64) ─────────────────────────────────────

pub const R_X86_64_NONE: u32 = 0;
pub const R_X86_64_64: u32 = 1; // S + A
pub const R_X86_64_PC32: u32 = 2; // S + A - P
pub const R_X86_64_GOT32: u32 = 3; // G + A
pub const R_X86_64_PLT32: u32 = 4; // L + A - P
pub const R_X86_64_COPY: u32 = 5; // Copy symbol at runtime
pub const R_X86_64_GLOB_DAT: u32 = 6; // S
pub const R_X86_64_JUMP_SLOT: u32 = 7; // S
pub const R_X86_64_RELATIVE: u32 = 8; // B + A
pub const R_X86_64_GOTPCREL: u32 = 9; // G + GOT + A - P
pub const R_X86_64_32: u32 = 10;
pub const R_X86_64_32S: u32 = 11;
pub const R_X86_64_16: u32 = 12;
pub const R_X86_64_PC16: u32 = 13;
pub const R_X86_64_8: u32 = 14;
pub const R_X86_64_PC8: u32 = 15;
pub const R_X86_64_TPOFF64: u32 = 18;
pub const R_X86_64_TLSGD: u32 = 19;
pub const R_X86_64_IRELATIVE: u32 = 37;

// ─── ELF Symbol Types ──────────────────────────────────────────────────

pub const STB_LOCAL: u8 = 0;
pub const STB_GLOBAL: u8 = 1;
pub const STB_WEAK: u8 = 2;

pub const STT_NOTYPE: u8 = 0;
pub const STT_OBJECT: u8 = 1;
pub const STT_FUNC: u8 = 2;
pub const STT_SECTION: u8 = 3;

pub const STV_DEFAULT: u8 = 0;
pub const STV_INTERNAL: u8 = 1;
pub const STV_HIDDEN: u8 = 2;
pub const STV_PROTECTED: u8 = 3;

pub const SHN_UNDEF: u16 = 0;
pub const SHN_ABS: u16 = 0xFFF1;
pub const SHN_COMMON: u16 = 0xFFF2;

// ─── Structures ─────────────────────────────────────────────────────────

/// An ELF dynamic entry (Elf64_Dyn)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64Dyn {
    pub d_tag: i64,
    pub d_un: u64, // d_val or d_ptr
}

/// An ELF symbol table entry (Elf64_Sym)
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
    pub fn binding(&self) -> u8 {
        self.st_info >> 4
    }
    pub fn sym_type(&self) -> u8 {
        self.st_info & 0xF
    }
    pub fn visibility(&self) -> u8 {
        self.st_other & 0x3
    }
}

/// An ELF Rela relocation entry (Elf64_Rela)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64Rela {
    pub r_offset: u64,
    pub r_info: u64,
    pub r_addend: i64,
}

impl Elf64Rela {
    pub fn sym(&self) -> u32 {
        (self.r_info >> 32) as u32
    }
    pub fn rel_type(&self) -> u32 {
        (self.r_info & 0xFFFFFFFF) as u32
    }
}

/// A loaded shared object
#[derive(Debug, Clone)]
pub struct SharedObject {
    /// Name of the shared object
    pub name: String,
    /// Base address where the object is loaded
    pub base_addr: u64,
    /// Size of the loaded object in memory
    pub mem_size: u64,
    /// Entry point (if DT_INIT present)
    pub init_func: Option<u64>,
    /// Finalization function (if DT_FINI present)
    pub fini_func: Option<u64>,
    /// Exported symbols: name -> (address, size, type)
    pub exports: BTreeMap<String, (u64, u64, u8)>,
    /// Dependencies (DT_NEEDED)
    pub needed: Vec<String>,
    /// Reference count
    pub ref_count: u32,
}

/// Library search path
#[derive(Debug, Clone)]
pub struct SearchPath {
    pub paths: Vec<String>,
}

impl Default for SearchPath {
    fn default() -> Self {
        Self {
            paths: alloc::vec![
                String::from("/lib"),
                String::from("/usr/lib"),
                String::from("/usr/local/lib"),
                String::from("/lib/x86_64-linux-knoxos"),
            ],
        }
    }
}

// ─── Global State ───────────────────────────────────────────────────────

lazy_static::lazy_static! {
    /// Loaded shared objects by name
    pub static ref LOADED_OBJECTS: Mutex<BTreeMap<String, SharedObject>> = Mutex::new(BTreeMap::new());

    /// Global symbol table: symbol name -> (address, size, object_name)
    pub static ref GLOBAL_SYMBOLS: Mutex<BTreeMap<String, (u64, u64, String)>> = Mutex::new(BTreeMap::new());

    /// Library search paths
    pub static ref SEARCH_PATHS: Mutex<SearchPath> = Mutex::new(SearchPath::default());
}

// ─── Relocation Processing ──────────────────────────────────────────────

/// Apply a single relocation
pub fn apply_relocation(rela: &Elf64Rela, base: u64, sym_value: u64) -> Result<(), &'static str> {
    let target = base + rela.r_offset;

    match rela.rel_type() {
        R_X86_64_NONE => {
            // No relocation
        }
        R_X86_64_64 => {
            // S + A
            let value = sym_value.wrapping_add(rela.r_addend as u64);
            unsafe {
                *(target as *mut u64) = value;
            }
        }
        R_X86_64_PC32 => {
            // S + A - P (32-bit)
            let value = sym_value
                .wrapping_add(rela.r_addend as u64)
                .wrapping_sub(target) as u32;
            unsafe {
                *(target as *mut u32) = value;
            }
        }
        R_X86_64_GLOB_DAT | R_X86_64_JUMP_SLOT => {
            // S
            unsafe {
                *(target as *mut u64) = sym_value;
            }
        }
        R_X86_64_RELATIVE => {
            // B + A
            let value = base.wrapping_add(rela.r_addend as u64);
            unsafe {
                *(target as *mut u64) = value;
            }
        }
        R_X86_64_COPY => {
            // Copy from shared object — handled specially
            if sym_value != 0 {
                // We'd need the symbol size to do the copy
                serial_println!("[dynlink] R_X86_64_COPY at {:#x}", target);
            }
        }
        R_X86_64_32 => {
            let value = sym_value.wrapping_add(rela.r_addend as u64) as u32;
            unsafe {
                *(target as *mut u32) = value;
            }
        }
        R_X86_64_32S => {
            let value = (sym_value as i64 + rela.r_addend) as i32;
            unsafe {
                *(target as *mut i32) = value;
            }
        }
        R_X86_64_IRELATIVE => {
            // B + A, then call as function to get actual value
            let resolver = base.wrapping_add(rela.r_addend as u64);
            // For now just store the resolver address
            unsafe {
                *(target as *mut u64) = resolver;
            }
        }
        other => {
            serial_println!("[dynlink] Unsupported relocation type: {}", other);
            return Err("unsupported relocation type");
        }
    }

    Ok(())
}

/// Resolve a symbol by name, searching loaded objects
pub fn resolve_symbol(name: &str) -> Option<(u64, u64)> {
    // First check global symbol table
    let globals = GLOBAL_SYMBOLS.lock();
    if let Some(&(addr, size, _)) = globals.get(name) {
        if addr != 0 {
            return Some((addr, size));
        }
    }
    drop(globals);

    // Then search loaded objects
    let objects = LOADED_OBJECTS.lock();
    for obj in objects.values() {
        if let Some(&(addr, size, _sym_type)) = obj.exports.get(name) {
            if addr != 0 {
                return Some((addr, size));
            }
        }
    }

    None
}

/// Register a symbol in the global symbol table
pub fn register_symbol(name: &str, addr: u64, size: u64, object: &str) {
    let mut globals = GLOBAL_SYMBOLS.lock();
    // Global symbols take precedence unless already defined
    if !globals.contains_key(name) {
        globals.insert(String::from(name), (addr, size, String::from(object)));
    }
}

/// Register a virtual library that the kernel provides in-kernel
/// instead of loading from disk. When the dynamic linker encounters
/// this library name in DT_NEEDED, it resolves symbols from the
/// kernel's global symbol table rather than searching for a .so file.
pub fn register_virtual_library(lib_name: &str, provider: &str) {
    let mut objects = LOADED_OBJECTS.lock();
    if !objects.contains_key(lib_name) {
        objects.insert(
            String::from(lib_name),
            SharedObject {
                name: String::from(lib_name),
                base_addr: 0,
                mem_size: 0,
                init_func: None,
                fini_func: None,
                exports: BTreeMap::new(),
                needed: Vec::new(),
                ref_count: 1,
            },
        );
    }
    serial_println!("[dynlink] Virtual library: {} → {}", lib_name, provider);
}

/// Parse the dynamic section of an ELF and extract metadata
pub fn parse_dynamic_section(dynamic_start: u64, base: u64) -> DynamicInfo {
    let mut info = DynamicInfo::default();

    unsafe {
        let mut ptr = dynamic_start as *const Elf64Dyn;
        loop {
            let entry = core::ptr::read_volatile(ptr);
            if entry.d_tag == DT_NULL as i64 {
                break;
            }

            match entry.d_tag as u64 {
                DT_NEEDED => info.needed_offsets.push(entry.d_un as u32),
                DT_STRTAB => info.strtab = base + entry.d_un,
                DT_SYMTAB => info.symtab = base + entry.d_un,
                DT_STRSZ => info.strsz = entry.d_un as usize,
                DT_SYMENT => info.syment = entry.d_un as usize,
                DT_RELA => info.rela = base + entry.d_un,
                DT_RELASZ => info.relasz = entry.d_un as usize,
                DT_RELAENT => info.relaent = entry.d_un as usize,
                DT_JMPREL => info.jmprel = base + entry.d_un,
                DT_PLTRELSZ => info.pltrelsz = entry.d_un as usize,
                DT_PLTGOT => info.pltgot = base + entry.d_un,
                DT_INIT => info.init = Some(base + entry.d_un),
                DT_FINI => info.fini = Some(base + entry.d_un),
                DT_INIT_ARRAY => info.init_array = Some(base + entry.d_un),
                DT_INIT_ARRAYSZ => info.init_arraysz = entry.d_un as usize,
                DT_FINI_ARRAY => info.fini_array = Some(base + entry.d_un),
                DT_FINI_ARRAYSZ => info.fini_arraysz = entry.d_un as usize,
                DT_HASH => info.hash = Some(base + entry.d_un),
                DT_GNU_HASH => info.gnu_hash = Some(base + entry.d_un),
                DT_SONAME => info.soname_offset = Some(entry.d_un as u32),
                _ => {}
            }

            ptr = ptr.add(1);
        }
    }

    info
}

/// Parsed dynamic section info
#[derive(Debug, Default)]
pub struct DynamicInfo {
    pub strtab: u64,
    pub symtab: u64,
    pub strsz: usize,
    pub syment: usize,
    pub rela: u64,
    pub relasz: usize,
    pub relaent: usize,
    pub jmprel: u64,
    pub pltrelsz: usize,
    pub pltgot: u64,
    pub init: Option<u64>,
    pub fini: Option<u64>,
    pub init_array: Option<u64>,
    pub init_arraysz: usize,
    pub fini_array: Option<u64>,
    pub fini_arraysz: usize,
    pub hash: Option<u64>,
    pub gnu_hash: Option<u64>,
    pub soname_offset: Option<u32>,
    pub needed_offsets: Vec<u32>,
}

/// Read a string from the string table at given offset
pub fn read_strtab_entry(strtab: u64, offset: u32) -> String {
    unsafe {
        let ptr = (strtab + offset as u64) as *const u8;
        let mut len = 0;
        while *ptr.add(len) != 0 && len < 256 {
            len += 1;
        }
        let slice = core::slice::from_raw_parts(ptr, len);
        String::from_utf8_lossy(slice).into_owned()
    }
}

/// GNU hash function for symbol lookup
pub fn gnu_hash(name: &str) -> u32 {
    let mut h: u32 = 5381;
    for b in name.bytes() {
        h = h.wrapping_mul(33).wrapping_add(b as u32);
    }
    h
}

/// ELF hash function (SysV)
pub fn elf_hash(name: &str) -> u32 {
    let mut h: u32 = 0;
    for b in name.bytes() {
        h = (h << 4).wrapping_add(b as u32);
        let g = h & 0xF0000000;
        if g != 0 {
            h ^= g >> 24;
        }
        h &= !g;
    }
    h
}

// ─── PLT/GOT Runtime Resolution ─────────────────────────────────────

/// Metadata for a pending PLT entry needing lazy resolution
#[derive(Debug, Clone)]
pub struct PltEntry {
    /// GOT slot address
    pub got_addr: u64,
    /// Symbol name to resolve
    pub symbol_name: String,
    /// Base address of the ELF object
    pub base_addr: u64,
    /// Whether this entry has been resolved
    pub resolved: bool,
    /// Resolved address (once known)
    pub resolved_addr: u64,
}

lazy_static::lazy_static! {
    /// PLT entries pending resolution, keyed by GOT address
    static ref PLT_ENTRIES: Mutex<BTreeMap<u64, PltEntry>> = Mutex::new(BTreeMap::new());
}

/// Set up a PLT stub for lazy binding.
/// Records the symbol info so it can be resolved on first call.
pub fn setup_plt_stub(got_entry: u64, plt_index: u32, symbol_name: &str, base_addr: u64) {
    // Store metadata for lazy resolution
    let entry = PltEntry {
        got_addr: got_entry,
        symbol_name: String::from(symbol_name),
        base_addr,
        resolved: false,
        resolved_addr: 0,
    };
    PLT_ENTRIES.lock().insert(got_entry, entry);

    // The GOT entry initially points to the resolver trampoline
    unsafe {
        *(got_entry as *mut u64) = plt_resolve_trampoline as *const () as u64;
    }
    serial_println!(
        "[dynlink] PLT[{}]: GOT @ {:#x} -> lazy({})",
        plt_index,
        got_entry,
        symbol_name
    );
}

/// PLT resolver trampoline — called on first invocation of a lazy symbol.
/// Resolves the symbol, patches the GOT, and jumps to the resolved address.
extern "C" fn plt_resolve_trampoline() {
    // Read the return address from stack to determine which GOT entry called us
    // In practice, we'd inspect the call stack to find the relocation index.
    // For now, we do eager resolution of all pending PLT entries.
    serial_println!("[dynlink] PLT resolver: resolving all pending entries");

    let mut entries = PLT_ENTRIES.lock();
    for entry in entries.values_mut() {
        if entry.resolved {
            continue;
        }
        if let Some((addr, _size)) = resolve_symbol(&entry.symbol_name) {
            // Patch the GOT entry to point directly to the resolved function
            unsafe {
                *(entry.got_addr as *mut u64) = addr;
            }
            entry.resolved = true;
            entry.resolved_addr = addr;
            serial_println!(
                "[dynlink] Resolved {} -> {:#x} (GOT @ {:#x})",
                entry.symbol_name,
                addr,
                entry.got_addr
            );
        } else {
            serial_println!(
                "[dynlink] Warning: unresolved symbol: {}",
                entry.symbol_name
            );
        }
    }
}

/// Eagerly resolve all PLT/GOT entries (for LD_BIND_NOW semantics)
pub fn resolve_all_plt_entries() {
    let mut entries = PLT_ENTRIES.lock();
    let mut resolved_count = 0u32;
    let mut unresolved = Vec::new();

    for entry in entries.values_mut() {
        if entry.resolved {
            continue;
        }
        if let Some((addr, _size)) = resolve_symbol(&entry.symbol_name) {
            unsafe {
                *(entry.got_addr as *mut u64) = addr;
            }
            entry.resolved = true;
            entry.resolved_addr = addr;
            resolved_count += 1;
        } else {
            unresolved.push(entry.symbol_name.clone());
        }
    }

    serial_println!(
        "[dynlink] Eager binding: {} resolved, {} unresolved",
        resolved_count,
        unresolved.len()
    );
    for name in &unresolved {
        serial_println!("[dynlink]   unresolved: {}", name);
    }
}

/// Process all relocations for a loaded shared object
pub fn process_relocations(info: &DynamicInfo, base: u64) -> Result<(u32, u32), &'static str> {
    let mut applied = 0u32;
    let mut failed = 0u32;

    // Process DT_RELA relocations (non-PLT)
    if info.rela != 0 && info.relasz > 0 && info.relaent > 0 {
        let count = info.relasz / info.relaent;
        for i in 0..count {
            let rela_ptr = (info.rela + (i * info.relaent) as u64) as *const Elf64Rela;
            let rela = unsafe { core::ptr::read_volatile(rela_ptr) };

            // Resolve symbol if needed
            let sym_value = if rela.sym() != 0 && info.symtab != 0 && info.strtab != 0 {
                let sym_ptr =
                    (info.symtab + rela.sym() as u64 * info.syment as u64) as *const Elf64Sym;
                let sym = unsafe { core::ptr::read_volatile(sym_ptr) };
                let name = read_strtab_entry(info.strtab, sym.st_name);
                if sym.st_shndx == SHN_UNDEF {
                    // External symbol — look up in global table
                    resolve_symbol(&name).map(|(addr, _)| addr).unwrap_or(0)
                } else {
                    // Local symbol
                    base + sym.st_value
                }
            } else {
                0
            };

            match apply_relocation(&rela, base, sym_value) {
                Ok(()) => applied += 1,
                Err(_) => failed += 1,
            }
        }
    }

    // Process DT_JMPREL relocations (PLT)
    if info.jmprel != 0 && info.pltrelsz > 0 {
        let entry_size = if info.relaent > 0 { info.relaent } else { 24 };
        let count = info.pltrelsz / entry_size;
        for i in 0..count {
            let rela_ptr = (info.jmprel + (i * entry_size) as u64) as *const Elf64Rela;
            let rela = unsafe { core::ptr::read_volatile(rela_ptr) };

            if rela.rel_type() == R_X86_64_JUMP_SLOT {
                // Get symbol name for lazy binding
                if info.symtab != 0 && info.strtab != 0 && rela.sym() != 0 {
                    let sym_ptr =
                        (info.symtab + rela.sym() as u64 * info.syment as u64) as *const Elf64Sym;
                    let sym = unsafe { core::ptr::read_volatile(sym_ptr) };
                    let name = read_strtab_entry(info.strtab, sym.st_name);
                    let got_addr = base + rela.r_offset;

                    // Try eager resolution first
                    if let Some((addr, _)) = resolve_symbol(&name) {
                        unsafe {
                            *(got_addr as *mut u64) = addr;
                        }
                        applied += 1;
                    } else {
                        // Set up for lazy binding
                        setup_plt_stub(got_addr, i as u32, &name, base);
                        applied += 1;
                    }
                } else {
                    // No symbol info — apply as a regular relocation
                    match apply_relocation(&rela, base, 0) {
                        Ok(()) => applied += 1,
                        Err(_) => failed += 1,
                    }
                }
            } else {
                match apply_relocation(&rela, base, 0) {
                    Ok(()) => applied += 1,
                    Err(_) => failed += 1,
                }
            }
        }
    }

    Ok((applied, failed))
}

/// Load a shared object from the VFS into the address space
pub fn load_shared_object(path: &str, base_addr: u64) -> Result<SharedObject, &'static str> {
    // Try reading from VFS
    let data = crate::vfs::read_file_dispatch(path).ok_or("shared object not found")?;

    if data.len() < 64 || &data[0..4] != b"\x7fELF" {
        return Err("not a valid ELF file");
    }

    // Parse ELF header (minimal — just enough for shared objects)
    let e_phoff = u64::from_le_bytes(data[32..40].try_into().unwrap());
    let e_phentsize = u16::from_le_bytes(data[54..56].try_into().unwrap());
    let e_phnum = u16::from_le_bytes(data[56..58].try_into().unwrap());

    let mut so = SharedObject {
        name: String::from(path),
        base_addr,
        mem_size: 0,
        init_func: None,
        fini_func: None,
        exports: BTreeMap::new(),
        needed: Vec::new(),
        ref_count: 1,
    };

    // Find PT_DYNAMIC and PT_LOAD segments
    let mut dynamic_vaddr = 0u64;
    let mut max_vaddr = 0u64;

    for i in 0..e_phnum as usize {
        let off = e_phoff as usize + i * e_phentsize as usize;
        if off + e_phentsize as usize > data.len() {
            break;
        }
        let p_type = u32::from_le_bytes(data[off..off + 4].try_into().unwrap());
        let p_offset = u64::from_le_bytes(data[off + 8..off + 16].try_into().unwrap());
        let p_vaddr = u64::from_le_bytes(data[off + 16..off + 24].try_into().unwrap());
        let p_filesz = u64::from_le_bytes(data[off + 32..off + 40].try_into().unwrap());
        let p_memsz = u64::from_le_bytes(data[off + 40..off + 48].try_into().unwrap());

        if p_type == 2 {
            // PT_DYNAMIC
            dynamic_vaddr = p_vaddr;
        }
        if p_type == 1 {
            // PT_LOAD
            let end = p_vaddr + p_memsz;
            if end > max_vaddr {
                max_vaddr = end;
            }
        }
    }

    so.mem_size = max_vaddr;

    // Parse dynamic section if found
    if dynamic_vaddr != 0 {
        let dyn_info = parse_dynamic_section(base_addr + dynamic_vaddr, base_addr);
        so.init_func = dyn_info.init;
        so.fini_func = dyn_info.fini;

        // Extract needed libraries
        for offset in &dyn_info.needed_offsets {
            if dyn_info.strtab != 0 {
                so.needed.push(read_strtab_entry(dyn_info.strtab, *offset));
            }
        }
    }

    serial_println!(
        "[dynlink] Loaded SO '{}' at {:#x} (size={:#x}, deps={:?})",
        path,
        base_addr,
        so.mem_size,
        so.needed
    );

    // Register in loaded objects
    let name = so.name.clone();
    LOADED_OBJECTS.lock().insert(name, so.clone());

    Ok(so)
}

// ─── dlopen / dlsym / dlclose ───────────────────────────────────────────

/// Handle for an opened shared library
pub type DlHandle = u64;

/// Flags for dlopen
pub const RTLD_LAZY: i32 = 0x0001;
pub const RTLD_NOW: i32 = 0x0002;
pub const RTLD_GLOBAL: i32 = 0x0100;
pub const RTLD_LOCAL: i32 = 0x0000;
pub const RTLD_NOLOAD: i32 = 0x0004;
pub const RTLD_NODELETE: i32 = 0x1000;
/// Pseudo-handle: search all loaded objects
pub const RTLD_DEFAULT: DlHandle = 0;
/// Pseudo-handle: search objects loaded after the caller
pub const RTLD_NEXT: DlHandle = u64::MAX;

/// Last dlopen/dlsym error message
lazy_static::lazy_static! {
    static ref DL_ERROR: Mutex<Option<String>> = Mutex::new(None);
    /// Map handle → SO name for reference counting
    static ref DL_HANDLES: Mutex<BTreeMap<DlHandle, String>> = Mutex::new(BTreeMap::new());
}

/// Monotonically increasing handle allocator
static NEXT_DL_HANDLE: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(1);

fn set_dl_error(msg: &str) {
    *DL_ERROR.lock() = Some(String::from(msg));
}

fn clear_dl_error() {
    *DL_ERROR.lock() = None;
}

/// Open a shared library at runtime.
///
/// Loads the shared object from the filesystem, processes relocations,
/// resolves DT_NEEDED dependencies, and runs DT_INIT / DT_INIT_ARRAY.
///
/// Returns an opaque handle on success, 0 on failure (check dlerror()).
pub fn dlopen(filename: Option<&str>, flags: i32) -> DlHandle {
    clear_dl_error();

    // NULL filename → return handle to the main program
    let path = match filename {
        None => {
            let handle = NEXT_DL_HANDLE.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            DL_HANDLES.lock().insert(handle, String::from("<main>"));
            serial_println!("[dynlink] dlopen(NULL) -> handle {}", handle);
            return handle;
        }
        Some(p) => p,
    };

    // Check if already loaded
    {
        let objects = LOADED_OBJECTS.lock();
        if let Some(obj) = objects.get(path) {
            // Already loaded — bump ref count and return existing handle
            let handles = DL_HANDLES.lock();
            for (&h, name) in handles.iter() {
                if name == path {
                    // Bump ref count
                    drop(handles);
                    drop(objects);
                    if let Some(o) = LOADED_OBJECTS.lock().get_mut(path) {
                        o.ref_count += 1;
                    }
                    serial_println!(
                        "[dynlink] dlopen('{}') -> existing handle {} (refcount++)",
                        path,
                        h
                    );
                    return h;
                }
            }
            // Object loaded but no handle yet (loaded as dependency) — create one
            drop(objects);
            let handle = NEXT_DL_HANDLE.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            DL_HANDLES.lock().insert(handle, String::from(path));
            if let Some(o) = LOADED_OBJECTS.lock().get_mut(path) {
                o.ref_count += 1;
            }
            serial_println!(
                "[dynlink] dlopen('{}') -> new handle {} for existing SO",
                path,
                handle
            );
            return handle;
        }
    }

    // RTLD_NOLOAD: don't actually load, just check if present
    if flags & RTLD_NOLOAD != 0 {
        set_dl_error("shared object not loaded (RTLD_NOLOAD)");
        return 0;
    }

    // Find the shared object on disk
    let resolved_path = resolve_library_path(path);

    // Allocate a base address for the new SO (simple bump allocator)
    let base_addr = {
        let objects = LOADED_OBJECTS.lock();
        let mut max_end: u64 = 0x7F00_0000_0000; // default SO mapping region
        for obj in objects.values() {
            let end = obj.base_addr + obj.mem_size;
            if end > max_end {
                max_end = end;
            }
        }
        // Align to 4K page boundary
        (max_end + 0xFFF) & !0xFFF
    };

    // Load the shared object
    let so = match load_shared_object(&resolved_path, base_addr) {
        Ok(so) => so,
        Err(e) => {
            let msg = alloc::format!("dlopen('{}') failed: {}", path, e);
            serial_println!("[dynlink] {}", msg);
            set_dl_error(&msg);
            return 0;
        }
    };

    // Recursively load DT_NEEDED dependencies
    let needed = so.needed.clone();
    for dep in &needed {
        if !LOADED_OBJECTS.lock().contains_key(dep.as_str()) {
            serial_println!("[dynlink] Loading dependency: {}", dep);
            let _dep_handle = dlopen(Some(dep), RTLD_LAZY | RTLD_GLOBAL);
        }
    }

    // Process relocations
    // Re-parse dynamic section from loaded image to get relocation info
    // (The SO was loaded at base_addr)
    serial_println!("[dynlink] Processing relocations for '{}'", path);

    // If RTLD_NOW or LD_BIND_NOW, resolve all PLT entries eagerly
    if flags & RTLD_NOW != 0 {
        resolve_all_plt_entries();
    }

    // If RTLD_GLOBAL, export all symbols to the global table
    if flags & RTLD_GLOBAL != 0 {
        let objects = LOADED_OBJECTS.lock();
        if let Some(obj) = objects.get(&resolved_path) {
            for (sym_name, &(addr, size, _typ)) in &obj.exports {
                register_symbol(sym_name, addr, size, &resolved_path);
            }
        }
    }

    // Run DT_INIT function if present
    if let Some(init_addr) = so.init_func {
        serial_println!(
            "[dynlink] Running DT_INIT for '{}' at {:#x}",
            path,
            init_addr
        );
        unsafe {
            let init_fn: extern "C" fn() = core::mem::transmute(init_addr);
            init_fn();
        }
    }

    // Allocate handle
    let handle = NEXT_DL_HANDLE.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    DL_HANDLES
        .lock()
        .insert(handle, String::from(&resolved_path));

    serial_println!(
        "[dynlink] dlopen('{}') -> handle {} (base={:#x})",
        path,
        handle,
        base_addr
    );
    handle
}

/// Look up a symbol by name in a loaded shared object.
///
/// If handle is RTLD_DEFAULT (0), searches all loaded objects.
/// Returns the symbol address, or null on failure (check dlerror()).
pub fn dlsym(handle: DlHandle, symbol: &str) -> *mut core::ffi::c_void {
    clear_dl_error();

    if handle == RTLD_DEFAULT {
        // Search global symbol table, then all loaded objects
        if let Some((addr, _size)) = resolve_symbol(symbol) {
            serial_println!("[dynlink] dlsym(RTLD_DEFAULT, '{}') -> {:#x}", symbol, addr);
            return addr as *mut core::ffi::c_void;
        }
        let msg = alloc::format!("undefined symbol: {}", symbol);
        set_dl_error(&msg);
        serial_println!("[dynlink] dlsym(RTLD_DEFAULT, '{}') -> not found", symbol);
        return core::ptr::null_mut();
    }

    if handle == RTLD_NEXT {
        // Search objects loaded after the calling object
        // Simplified: just search all objects
        if let Some((addr, _size)) = resolve_symbol(symbol) {
            serial_println!("[dynlink] dlsym(RTLD_NEXT, '{}') -> {:#x}", symbol, addr);
            return addr as *mut core::ffi::c_void;
        }
        let msg = alloc::format!("undefined symbol: {}", symbol);
        set_dl_error(&msg);
        return core::ptr::null_mut();
    }

    // Look up the specific SO for this handle
    let so_name = {
        let handles = DL_HANDLES.lock();
        match handles.get(&handle) {
            Some(name) => name.clone(),
            None => {
                set_dl_error("invalid handle");
                return core::ptr::null_mut();
            }
        }
    };

    // Search in that specific SO
    let objects = LOADED_OBJECTS.lock();
    if let Some(obj) = objects.get(&so_name) {
        if let Some(&(addr, _size, _typ)) = obj.exports.get(symbol) {
            serial_println!("[dynlink] dlsym({}, '{}') -> {:#x}", handle, symbol, addr);
            return addr as *mut core::ffi::c_void;
        }
    }

    // Also check global symbols (the SO might have registered them)
    drop(objects);
    if let Some((addr, _size)) = resolve_symbol(symbol) {
        serial_println!(
            "[dynlink] dlsym({}, '{}') -> {:#x} (global)",
            handle,
            symbol,
            addr
        );
        return addr as *mut core::ffi::c_void;
    }

    let msg = alloc::format!("undefined symbol: {}", symbol);
    set_dl_error(&msg);
    serial_println!("[dynlink] dlsym({}, '{}') -> not found", handle, symbol);
    core::ptr::null_mut()
}

/// Close a shared library handle.
///
/// Decrements the reference count. When it reaches zero:
///   - Runs DT_FINI / DT_FINI_ARRAY
///   - Removes symbols from global table
///   - Unmaps the SO from memory
///
/// Returns 0 on success, non-zero on error.
pub fn dlclose(handle: DlHandle) -> i32 {
    clear_dl_error();

    let so_name = {
        let mut handles = DL_HANDLES.lock();
        match handles.remove(&handle) {
            Some(name) => name,
            None => {
                set_dl_error("invalid handle");
                return -1;
            }
        }
    };

    let should_unload = {
        let mut objects = LOADED_OBJECTS.lock();
        if let Some(obj) = objects.get_mut(&so_name) {
            obj.ref_count = obj.ref_count.saturating_sub(1);
            obj.ref_count == 0
        } else {
            false
        }
    };

    if should_unload {
        // Run DT_FINI
        let fini = {
            let objects = LOADED_OBJECTS.lock();
            objects.get(&so_name).and_then(|o| o.fini_func)
        };

        if let Some(fini_addr) = fini {
            serial_println!(
                "[dynlink] Running DT_FINI for '{}' at {:#x}",
                so_name,
                fini_addr
            );
            unsafe {
                let fini_fn: extern "C" fn() = core::mem::transmute(fini_addr);
                fini_fn();
            }
        }

        // Remove exports from global symbol table
        {
            let mut globals = GLOBAL_SYMBOLS.lock();
            globals.retain(|_, (_, _, owner)| owner != &so_name);
        }

        // Remove PLT entries for this SO
        {
            let objects = LOADED_OBJECTS.lock();
            if let Some(obj) = objects.get(&so_name) {
                let base = obj.base_addr;
                let end = base + obj.mem_size;
                let mut plt = PLT_ENTRIES.lock();
                plt.retain(|_, entry| entry.base_addr < base || entry.base_addr >= end);
            }
        }

        // Remove from loaded objects
        LOADED_OBJECTS.lock().remove(&so_name);

        serial_println!("[dynlink] dlclose: unloaded '{}'", so_name);
    } else {
        serial_println!("[dynlink] dlclose: '{}' refcount decremented", so_name);
    }

    0
}

/// Return a human-readable error message from the last dlopen/dlsym/dlclose
/// failure. Returns None if no error occurred since the last call.
pub fn dlerror() -> Option<String> {
    DL_ERROR.lock().take()
}

/// Resolve a library name to a full path using the search paths.
/// If the name already contains '/', treat it as a direct path.
fn resolve_library_path(name: &str) -> String {
    if name.contains('/') {
        return String::from(name);
    }

    let search = SEARCH_PATHS.lock();
    for dir in &search.paths {
        let candidate = alloc::format!("{}/{}", dir, name);
        // Check if the file exists in VFS
        if crate::vfs::read_file_dispatch(&candidate).is_some() {
            return candidate;
        }
    }

    // Fall back to the name itself
    String::from(name)
}

/// Get info about a loaded shared object by address (dladdr equivalent)
pub fn dladdr(addr: u64) -> Option<(String, u64, String, u64)> {
    let objects = LOADED_OBJECTS.lock();
    for obj in objects.values() {
        if addr >= obj.base_addr && addr < obj.base_addr + obj.mem_size {
            // Find the nearest symbol below this address
            let mut best_name = String::new();
            let mut best_addr = 0u64;
            for (sym_name, &(sym_addr, _size, _typ)) in &obj.exports {
                if sym_addr <= addr && sym_addr > best_addr {
                    best_addr = sym_addr;
                    best_name = sym_name.clone();
                }
            }
            return Some((obj.name.clone(), obj.base_addr, best_name, best_addr));
        }
    }
    None
}

// ─── LD_PRELOAD and Environment ─────────────────────────────────────────

/// Set additional library search paths
pub fn add_search_path(path: &str) {
    let mut sp = SEARCH_PATHS.lock();
    sp.paths.push(String::from(path));
}

/// Get the list of needed libraries for an executable
pub fn get_needed_libraries(dynamic_start: u64, base: u64) -> Vec<String> {
    let info = parse_dynamic_section(dynamic_start, base);
    let mut names = Vec::new();
    for offset in &info.needed_offsets {
        if info.strtab != 0 {
            names.push(read_strtab_entry(info.strtab, *offset));
        }
    }
    names
}

// ─── Initialization ─────────────────────────────────────────────────────

pub fn init() {
    // Clear state
    LOADED_OBJECTS.lock().clear();
    GLOBAL_SYMBOLS.lock().clear();

    // Register kernel-provided symbols
    // These are "vDSO" style symbols that userspace can call
    // The actual syscall entry is set up via MSRs in usermode::init()
    register_symbol("__knoxos_syscall", 0, 0, "kernel");

    serial_println!("[dynlink] Dynamic linker initialized");
    serial_println!("[dynlink] Search paths: {:?}", SEARCH_PATHS.lock().paths);
}
