use alloc::collections::BTreeMap;
/// Kernel Modules - Dynamic module loading framework
/// Compatible with Linux's module system (insmod/rmmod/lsmod)
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

/// Module state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleState {
    Live,
    Loading,
    Unloading,
    Builtin,
}

/// Module dependency type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepType {
    Hard, // Required dependency
    Soft, // Optional dependency
}

/// Kernel module information
#[derive(Debug, Clone)]
pub struct KernelModule {
    pub name: String,
    pub description: String,
    pub author: String,
    pub version: String,
    pub license: String,
    pub state: ModuleState,
    pub size_bytes: usize,
    pub dependencies: Vec<(String, DepType)>,
    pub ref_count: u32,
    pub parameters: BTreeMap<String, String>,
}

/// Global module registry
lazy_static::lazy_static! {
    static ref MODULES: Mutex<BTreeMap<String, KernelModule>> = Mutex::new(BTreeMap::new());
}

/// Register a built-in module
fn register_builtin(name: &str, description: &str, deps: &[(&str, DepType)], size: usize) {
    let mut modules = MODULES.lock();
    modules.insert(
        String::from(name),
        KernelModule {
            name: String::from(name),
            description: String::from(description),
            author: String::from("KnoxOS Contributors"),
            version: String::from("0.1.0"),
            license: String::from("MIT"),
            state: ModuleState::Builtin,
            size_bytes: size,
            dependencies: deps.iter().map(|(n, t)| (String::from(*n), *t)).collect(),
            ref_count: 1,
            parameters: BTreeMap::new(),
        },
    );
}

/// Insert a module (like insmod)
pub fn insert_module(module: KernelModule) -> Result<(), &'static str> {
    if module.state != ModuleState::Builtin
        && !crate::hardening::check_lockdown("load_unsigned_module")
    {
        crate::serial_println!("[KnoxOS] Module load denied by lockdown: {}", module.name);
        return Err("kernel lockdown: unsigned module");
    }
    let mut modules = MODULES.lock();

    if modules.contains_key(&module.name) {
        return Err("Module already loaded");
    }

    // Check dependencies
    for (dep, dep_type) in &module.dependencies {
        match modules.get(dep) {
            Some(dep_mod) => {
                if dep_mod.state != ModuleState::Live
                    && dep_mod.state != ModuleState::Builtin
                    && *dep_type == DepType::Hard
                {
                    return Err("Required dependency not available");
                }
            }
            None => {
                if *dep_type == DepType::Hard {
                    return Err("Required dependency not loaded");
                }
            }
        }
    }

    let name = module.name.clone();
    modules.insert(name.clone(), module);

    crate::serial_println!("[KnoxOS] Module loaded: {}", name);
    Ok(())
}

/// Remove a module (like rmmod)
pub fn remove_module(name: &str) -> Result<(), &'static str> {
    let mut modules = MODULES.lock();

    let module = modules.get(name).ok_or("Module not found")?;

    if module.state == ModuleState::Builtin {
        return Err("Cannot unload builtin module");
    }

    if module.ref_count > 0 {
        return Err("Module is in use");
    }

    // Check if any other module depends on this one
    let dependents: Vec<String> = modules
        .values()
        .filter(|m| m.dependencies.iter().any(|(d, _)| d == name))
        .map(|m| m.name.clone())
        .collect();

    if !dependents.is_empty() {
        return Err("Module is depended upon by other modules");
    }

    modules.remove(name);
    crate::serial_println!("[KnoxOS] Module unloaded: {}", name);
    Ok(())
}

/// List all modules (like lsmod)
pub fn list_modules() -> Vec<KernelModule> {
    MODULES.lock().values().cloned().collect()
}

/// Get module info (like modinfo)
pub fn module_info(name: &str) -> Option<KernelModule> {
    MODULES.lock().get(name).cloned()
}

/// Set a module parameter
pub fn set_param(module_name: &str, param: &str, value: &str) -> Result<(), &'static str> {
    let mut modules = MODULES.lock();
    let module = modules.get_mut(module_name).ok_or("Module not found")?;
    module
        .parameters
        .insert(String::from(param), String::from(value));
    Ok(())
}

/// Get a module parameter
pub fn get_param(module_name: &str, param: &str) -> Option<String> {
    MODULES
        .lock()
        .get(module_name)
        .and_then(|m| m.parameters.get(param).cloned())
}

/// Format module listing (like /proc/modules)
pub fn format_modules() -> String {
    let modules = MODULES.lock();
    let mut output = String::new();

    for module in modules.values() {
        let state_str = match module.state {
            ModuleState::Live => "Live",
            ModuleState::Loading => "Loading",
            ModuleState::Unloading => "Unloading",
            ModuleState::Builtin => "Builtin",
        };

        let deps: Vec<&str> = module
            .dependencies
            .iter()
            .map(|(name, _)| name.as_str())
            .collect();
        let deps_str = if deps.is_empty() {
            String::from("-")
        } else {
            deps.join(",")
        };

        output.push_str(&alloc::format!(
            "{:<20} {:>8}  {} - {} [{}]\n",
            module.name,
            module.size_bytes,
            module.ref_count,
            deps_str,
            state_str
        ));
    }

    output
}

/// Initialize the module system with built-in modules
pub fn init() {
    // Register all built-in kernel subsystems as modules
    register_builtin("knoxos_core", "KnoxOS Core Kernel", &[], 65536);
    register_builtin(
        "gdt",
        "Global Descriptor Table",
        &[("knoxos_core", DepType::Hard)],
        4096,
    );
    register_builtin(
        "idt",
        "Interrupt Descriptor Table",
        &[("knoxos_core", DepType::Hard)],
        8192,
    );
    register_builtin(
        "memory",
        "Memory Management",
        &[("knoxos_core", DepType::Hard)],
        16384,
    );
    register_builtin(
        "scheduler",
        "Process Scheduler",
        &[("knoxos_core", DepType::Hard)],
        8192,
    );
    register_builtin(
        "vfs",
        "Virtual File System",
        &[("knoxos_core", DepType::Hard)],
        12288,
    );
    register_builtin("ext2", "Ext2 Filesystem", &[("vfs", DepType::Hard)], 16384);
    register_builtin(
        "block",
        "Block Device Layer",
        &[("knoxos_core", DepType::Hard)],
        12288,
    );
    register_builtin(
        "net_core",
        "Network Stack",
        &[("knoxos_core", DepType::Hard)],
        32768,
    );
    register_builtin("tcp", "TCP Protocol", &[("net_core", DepType::Hard)], 16384);
    register_builtin("udp", "UDP Protocol", &[("net_core", DepType::Hard)], 8192);
    register_builtin("ip", "IPv4 Protocol", &[("net_core", DepType::Hard)], 8192);
    register_builtin(
        "pcspkr",
        "PC Speaker Driver",
        &[("knoxos_core", DepType::Hard)],
        2048,
    );
    register_builtin(
        "ps2_kbd",
        "PS/2 Keyboard Driver",
        &[("knoxos_core", DepType::Hard)],
        4096,
    );
    register_builtin(
        "ps2_mouse",
        "PS/2 Mouse Driver",
        &[("knoxos_core", DepType::Hard)],
        4096,
    );
    register_builtin(
        "fb",
        "Framebuffer Driver",
        &[("knoxos_core", DepType::Hard)],
        16384,
    );
    register_builtin(
        "tty",
        "TTY Subsystem",
        &[("knoxos_core", DepType::Hard)],
        8192,
    );
    register_builtin(
        "security",
        "Security Module",
        &[("knoxos_core", DepType::Hard)],
        8192,
    );
    register_builtin(
        "ai_engine",
        "AI Inference Engine",
        &[("knoxos_core", DepType::Hard)],
        32768,
    );

    let count = MODULES.lock().len();
    crate::serial_println!(
        "[KnoxOS] Module system initialized ({} builtin modules)",
        count
    );
}

pub const GATE_KO_MARKER: &str = "GATE_KO load";

const ET_REL: u16 = 1;
const EM_X86_64: u16 = 62;
const SHT_PROGBITS: u32 = 1;
const SHT_SYMTAB: u32 = 2;
const SHT_STRTAB: u32 = 3;
const SHT_RELA: u32 = 4;
const SHF_ALLOC: u64 = 0x2;
const STB_GLOBAL: u8 = 1;
const STT_FUNC: u8 = 2;
const R_X86_64_64: u32 = 1;
const R_X86_64_PC32: u32 = 2;
const R_X86_64_32S: u32 = 11;

#[repr(C)]
#[derive(Clone, Copy)]
struct Elf64Ehdr {
    e_ident: [u8; 16],
    e_type: u16,
    e_machine: u16,
    e_version: u32,
    e_entry: u64,
    e_phoff: u64,
    e_shoff: u64,
    e_flags: u32,
    e_ehsize: u16,
    e_phentsize: u16,
    e_phnum: u16,
    e_shentsize: u16,
    e_shnum: u16,
    e_shstrndx: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Elf64Shdr {
    sh_name: u32,
    sh_type: u32,
    sh_flags: u64,
    sh_addr: u64,
    sh_offset: u64,
    sh_size: u64,
    sh_link: u32,
    sh_info: u32,
    sh_addralign: u64,
    sh_entsize: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Elf64Sym {
    st_name: u32,
    st_info: u8,
    st_other: u8,
    st_shndx: u16,
    st_value: u64,
    st_size: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Elf64Rela {
    r_offset: u64,
    r_info: u64,
    r_addend: i64,
}

fn read_struct<T: Copy>(data: &[u8], off: usize) -> Option<T> {
    let bytes = data.get(off..off + core::mem::size_of::<T>())?;
    // SAFETY: T is a packed ELF header/record; bytes came from the module image.
    Some(unsafe { core::ptr::read_unaligned(bytes.as_ptr() as *const T) })
}

/// Load an ELF ET_REL kernel module: map SHF_ALLOC sections, apply relocs, call `init_module`.
pub fn load_elf_module(image: &[u8], signed: bool) -> Result<u32, &'static str> {
    if !signed && !crate::hardening::check_lockdown("load_unsigned_module") {
        return Err("kernel lockdown: unsigned module");
    }
    if image.len() < core::mem::size_of::<Elf64Ehdr>() {
        return Err("truncated ELF");
    }
    let ehdr: Elf64Ehdr = read_struct(image, 0).ok_or("bad ehdr")?;
    if ehdr.e_ident[0..4] != [0x7F, b'E', b'L', b'F'] {
        return Err("not ELF");
    }
    if ehdr.e_type != ET_REL || ehdr.e_machine != EM_X86_64 {
        return Err("not ET_REL x86_64");
    }
    let shentsize = ehdr.e_shentsize as usize;
    if shentsize < core::mem::size_of::<Elf64Shdr>() {
        return Err("bad shentsize");
    }
    let mut shdrs = Vec::new();
    for i in 0..ehdr.e_shnum as usize {
        let off = ehdr.e_shoff as usize + i * shentsize;
        shdrs.push(read_struct::<Elf64Shdr>(image, off).ok_or("bad shdr")?);
    }
    let shstr = shdrs
        .get(ehdr.e_shstrndx as usize)
        .copied()
        .ok_or("no shstrtab")?;

    let mut layout: Vec<(usize, u64, u64)> = Vec::new(); // (shndx, offset_in_blob, size)
    let mut blob_size: u64 = 0;
    for (idx, sh) in shdrs.iter().enumerate() {
        if sh.sh_flags & SHF_ALLOC == 0 || sh.sh_size == 0 {
            continue;
        }
        let align = sh.sh_addralign.max(1);
        blob_size = (blob_size + align - 1) & !(align - 1);
        layout.push((idx, blob_size, sh.sh_size));
        blob_size += sh.sh_size;
    }
    if blob_size == 0 {
        return Err("empty module");
    }
    let pages = (blob_size as usize).div_ceil(4096);
    let phys = crate::vmm::allocate_contiguous_frames(pages).ok_or("no frames")?;
    let load_va = crate::kaslr::module_load_base();
    let cr3 = crate::vmm::get_kernel_cr3();
    #[cfg(target_arch = "x86_64")]
    let flags = x86_64::structures::paging::PageTableFlags::PRESENT
        | x86_64::structures::paging::PageTableFlags::WRITABLE;
    #[cfg(not(target_arch = "x86_64"))]
    let flags = crate::arch_compat::structures::paging::PageTableFlags::PRESENT
        | crate::arch_compat::structures::paging::PageTableFlags::WRITABLE;
    for i in 0..pages {
        let va = load_va + (i as u64) * 4096;
        let pa = phys + (i as u64) * 4096;
        // SAFETY: kernel CR3; frames were just allocated for this module.
        unsafe {
            crate::vmm::map_page_in_table_pub(cr3, va, pa, flags);
        }
    }
    let dest = load_va as *mut u8;
    // SAFETY: mapped writable kernel pages we own.
    unsafe {
        core::ptr::write_bytes(dest, 0, pages * 4096);
    }
    for &(idx, off, size) in &layout {
        let sh = shdrs[idx];
        if sh.sh_type == SHT_PROGBITS {
            let src = image
                .get(sh.sh_offset as usize..sh.sh_offset as usize + size as usize)
                .ok_or("section truncated")?;
            // SAFETY: destination is the mapped module blob.
            unsafe {
                core::ptr::copy_nonoverlapping(src.as_ptr(), dest.add(off as usize), src.len());
            }
        }
    }

    let sec_base = |ndx: u16| -> Option<u64> {
        layout
            .iter()
            .find(|(i, _, _)| *i == ndx as usize)
            .map(|(_, off, _)| load_va + *off)
    };

    // Apply relocations
    for sh in &shdrs {
        if sh.sh_type != SHT_RELA {
            continue;
        }
        let target_base = sec_base(sh.sh_info as u16).ok_or("rela target")?;
        let mut off = sh.sh_offset as usize;
        let end = off + sh.sh_size as usize;
        let entsize = if sh.sh_entsize == 0 {
            core::mem::size_of::<Elf64Rela>()
        } else {
            sh.sh_entsize as usize
        };
        let symtab = shdrs.get(sh.sh_link as usize).ok_or("no symtab")?;
        while off + entsize <= end && off + entsize <= image.len() {
            let rela: Elf64Rela = read_struct(image, off).ok_or("bad rela")?;
            let sym_idx = (rela.r_info >> 32) as usize;
            let rtype = (rela.r_info & 0xFFFF_FFFF) as u32;
            let sym_off = symtab.sh_offset as usize + sym_idx * core::mem::size_of::<Elf64Sym>();
            let sym: Elf64Sym = read_struct(image, sym_off).ok_or("bad sym")?;
            let s = sec_base(sym.st_shndx).unwrap_or(0) + sym.st_value;
            let p = target_base + rela.r_offset;
            let a = rela.r_addend;
            // SAFETY: reloc site is inside the mapped module.
            unsafe {
                match rtype {
                    R_X86_64_64 => {
                        core::ptr::write_unaligned(p as *mut u64, s.wrapping_add(a as u64));
                    }
                    R_X86_64_PC32 | R_X86_64_32S => {
                        let val = (s as i64 + a - p as i64) as i32;
                        core::ptr::write_unaligned(p as *mut i32, val);
                    }
                    _ => {}
                }
            }
            off += entsize;
        }
    }

    // Find init_module
    let mut init_addr = None;
    for sh in &shdrs {
        if sh.sh_type != SHT_SYMTAB {
            continue;
        }
        let strtab = shdrs.get(sh.sh_link as usize).ok_or("no strtab")?;
        let mut off = sh.sh_offset as usize;
        let end = off + sh.sh_size as usize;
        while off + core::mem::size_of::<Elf64Sym>() <= end {
            let sym: Elf64Sym = read_struct(image, off).ok_or("bad sym")?;
            off += core::mem::size_of::<Elf64Sym>();
            if (sym.st_info >> 4) != STB_GLOBAL {
                continue;
            }
            let name_off = strtab.sh_offset as usize + sym.st_name as usize;
            let name_slice = image.get(name_off..).unwrap_or(&[]);
            let nlen = name_slice.iter().position(|&b| b == 0).unwrap_or(0);
            let name = core::str::from_utf8(&name_slice[..nlen]).unwrap_or("");
            if name == "init_module" {
                init_addr = sec_base(sym.st_shndx).map(|b| b + sym.st_value);
            }
        }
    }
    let entry = init_addr.ok_or("no init_module")?;
    // SAFETY: entry is the mapped ET_REL init_module we just relocated.
    let rc = unsafe {
        let f: unsafe extern "C" fn() -> u32 = core::mem::transmute(entry);
        f()
    };

    let module = KernelModule {
        name: String::from("gate_ko"),
        description: String::from("in-kernel ET_REL self-test"),
        author: String::from("knoxos"),
        version: String::from("0"),
        license: String::from("MIT"),
        state: ModuleState::Live,
        size_bytes: blob_size as usize,
        dependencies: Vec::new(),
        ref_count: 0,
        parameters: BTreeMap::new(),
    };
    // Signed: kernel-generated image, lockdown does not apply.
    {
        let mut modules = MODULES.lock();
        modules.insert(module.name.clone(), module);
    }
    crate::serial_println!("[KnoxOS] .ko loaded at {:#x} init_module={}", load_va, rc);
    let _ = shstr;
    Ok(rc)
}

fn build_gate_ko() -> Vec<u8> {
    // Minimal ET_REL: .text = mov eax, 0x42; ret  plus init_module symbol.
    let text: &[u8] = &[0xB8, 0x42, 0x00, 0x00, 0x00, 0xC3];
    let shstrtab = b"\0.text\0.shstrtab\0.symtab\0.strtab\0";
    let strtab = b"\0init_module\0";
    let ehdr_sz = 64usize;
    let shdr_sz = 64usize;
    let nsh = 5usize;
    let shoff = ehdr_sz;
    let mut off = ehdr_sz + nsh * shdr_sz;
    let text_off = off;
    off += text.len();
    let shstr_off = off;
    off += shstrtab.len();
    let mut pad = (8 - (off % 8)) % 8;
    off += pad;
    let sym_off = off;
    let nsyms = 2usize;
    off += nsyms * 24;
    let str_off = off;
    off += strtab.len();

    let mut buf = vec![0u8; off];
    buf[0..4].copy_from_slice(&[0x7F, b'E', b'L', b'F']);
    buf[4] = 2; // ELFCLASS64
    buf[5] = 1; // LSB
    buf[6] = 1; // version
    buf[16..18].copy_from_slice(&ET_REL.to_le_bytes());
    buf[18..20].copy_from_slice(&EM_X86_64.to_le_bytes());
    buf[20..24].copy_from_slice(&1u32.to_le_bytes());
    buf[40..48].copy_from_slice(&(shoff as u64).to_le_bytes());
    buf[52..54].copy_from_slice(&64u16.to_le_bytes());
    buf[58..60].copy_from_slice(&(shdr_sz as u16).to_le_bytes());
    buf[60..62].copy_from_slice(&(nsh as u16).to_le_bytes());
    buf[62..64].copy_from_slice(&2u16.to_le_bytes()); // shstrndx

    fn write_shdr(
        buf: &mut [u8],
        idx: usize,
        name: u32,
        typ: u32,
        flags: u64,
        offset: u64,
        size: u64,
        link: u32,
        info: u32,
        addralign: u64,
        entsize: u64,
    ) {
        let o = 64 + idx * 64;
        buf[o..o + 4].copy_from_slice(&name.to_le_bytes());
        buf[o + 4..o + 8].copy_from_slice(&typ.to_le_bytes());
        buf[o + 8..o + 16].copy_from_slice(&flags.to_le_bytes());
        buf[o + 24..o + 32].copy_from_slice(&offset.to_le_bytes());
        buf[o + 32..o + 40].copy_from_slice(&size.to_le_bytes());
        buf[o + 40..o + 44].copy_from_slice(&link.to_le_bytes());
        buf[o + 44..o + 48].copy_from_slice(&info.to_le_bytes());
        buf[o + 48..o + 56].copy_from_slice(&addralign.to_le_bytes());
        buf[o + 56..o + 64].copy_from_slice(&entsize.to_le_bytes());
    }

    write_shdr(&mut buf, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0);
    write_shdr(
        &mut buf,
        1,
        1,
        SHT_PROGBITS,
        SHF_ALLOC | 0x4,
        text_off as u64,
        text.len() as u64,
        0,
        0,
        1,
        0,
    );
    write_shdr(
        &mut buf,
        2,
        7,
        SHT_STRTAB,
        0,
        shstr_off as u64,
        shstrtab.len() as u64,
        0,
        0,
        1,
        0,
    );
    write_shdr(
        &mut buf,
        3,
        17,
        SHT_SYMTAB,
        0,
        sym_off as u64,
        (nsyms * 24) as u64,
        4,
        1,
        8,
        24,
    );
    write_shdr(
        &mut buf,
        4,
        25,
        SHT_STRTAB,
        0,
        str_off as u64,
        strtab.len() as u64,
        0,
        0,
        1,
        0,
    );

    buf[text_off..text_off + text.len()].copy_from_slice(text);
    buf[shstr_off..shstr_off + shstrtab.len()].copy_from_slice(shstrtab);
    // null symbol already zero
    let so = sym_off + 24;
    buf[so..so + 4].copy_from_slice(&1u32.to_le_bytes()); // st_name = "init_module"
    buf[so + 4] = (STB_GLOBAL << 4) | STT_FUNC;
    buf[so + 6..so + 8].copy_from_slice(&1u16.to_le_bytes()); // .text
    buf[so + 8..so + 16].copy_from_slice(&0u64.to_le_bytes());
    buf[so + 16..so + 24].copy_from_slice(&(text.len() as u64).to_le_bytes());
    buf[str_off..str_off + strtab.len()].copy_from_slice(strtab);
    let _ = pad;
    buf
}

/// Map a kernel-built ET_REL `.ko` at a KASLR address and run `init_module`.
pub fn ko_self_test() -> bool {
    match load_elf_module(&build_gate_ko(), true) {
        Ok(0x42) => {
            crate::serial_println!("[KnoxOS] {} (init_module=0x42)", GATE_KO_MARKER);
            true
        }
        Ok(rc) => {
            crate::serial_println!("[KnoxOS] GATE_KO FAILED: rc={:#x}", rc);
            false
        }
        Err(e) => {
            crate::serial_println!("[KnoxOS] GATE_KO FAILED: {}", e);
            false
        }
    }
}
