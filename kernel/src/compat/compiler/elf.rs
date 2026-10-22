use alloc::string::String;
use alloc::vec::Vec;

// ═══════════════════════════════════════════════════════════════════════
// ELF LINKER
// ═══════════════════════════════════════════════════════════════════════

/// ELF section
#[derive(Debug, Clone)]
pub struct ElfSection {
    pub name: String,
    pub data: Vec<u8>,
    pub addr: u64,
    pub section_type: u32,
    pub flags: u64,
}

/// ELF symbol
#[derive(Debug, Clone)]
pub struct ElfSymbol {
    pub name: String,
    pub value: u64,
    pub size: u64,
    pub section: usize,
    pub sym_type: u8,
    pub binding: u8,
    pub global: bool,
}

/// Simple ELF builder
pub struct ElfBuilder {
    pub sections: Vec<ElfSection>,
    pub symbols: Vec<ElfSymbol>,
    pub entry_point: u64,
    pub base_addr: u64,
}

impl ElfBuilder {
    pub fn new() -> Self {
        Self {
            sections: Vec::new(),
            symbols: Vec::new(),
            entry_point: 0x400000,
            base_addr: 0x400000,
        }
    }

    pub fn add_section(&mut self, name: &str, data: Vec<u8>, flags: u64) -> usize {
        let idx = self.sections.len();
        self.sections.push(ElfSection {
            name: String::from(name),
            data,
            addr: 0,
            section_type: 1, // SHT_PROGBITS
            flags,
        });
        idx
    }

    pub fn add_symbol(&mut self, name: &str, value: u64, section: usize, global: bool) {
        self.symbols.push(ElfSymbol {
            name: String::from(name),
            value,
            size: 0,
            section,
            sym_type: 2, // STT_FUNC
            binding: if global { 1 } else { 0 },
            global,
        });
    }

    /// Build ELF binary
    pub fn build(&mut self) -> Vec<u8> {
        let mut elf = Vec::new();

        // ELF header (64 bytes)
        // Magic
        elf.extend_from_slice(&[0x7f, b'E', b'L', b'F']);
        elf.push(2); // ELFCLASS64
        elf.push(1); // ELFDATA2LSB
        elf.push(1); // EV_CURRENT
        elf.push(0); // ELFOSABI_NONE
        elf.extend_from_slice(&[0; 8]); // padding
        elf.extend_from_slice(&2u16.to_le_bytes()); // ET_EXEC
        elf.extend_from_slice(&0x3Eu16.to_le_bytes()); // EM_X86_64
        elf.extend_from_slice(&1u32.to_le_bytes()); // EV_CURRENT
        elf.extend_from_slice(&self.entry_point.to_le_bytes()); // e_entry
        elf.extend_from_slice(&64u64.to_le_bytes()); // e_phoff
        elf.extend_from_slice(&0u64.to_le_bytes()); // e_shoff (filled later)
        elf.extend_from_slice(&0u32.to_le_bytes()); // e_flags
        elf.extend_from_slice(&64u16.to_le_bytes()); // e_ehsize
        elf.extend_from_slice(&56u16.to_le_bytes()); // e_phentsize
        elf.extend_from_slice(&1u16.to_le_bytes()); // e_phnum
        elf.extend_from_slice(&64u16.to_le_bytes()); // e_shentsize
        elf.extend_from_slice(&0u16.to_le_bytes()); // e_shnum
        elf.extend_from_slice(&0u16.to_le_bytes()); // e_shstrndx

        // Program header (LOAD segment)
        let text_size: u64 = self.sections.iter().map(|s| s.data.len() as u64).sum();
        elf.extend_from_slice(&1u32.to_le_bytes()); // PT_LOAD
        elf.extend_from_slice(&5u32.to_le_bytes()); // PF_R | PF_X
        elf.extend_from_slice(&0x1000u64.to_le_bytes()); // p_offset
        elf.extend_from_slice(&self.base_addr.to_le_bytes()); // p_vaddr
        elf.extend_from_slice(&self.base_addr.to_le_bytes()); // p_paddr
        elf.extend_from_slice(&text_size.to_le_bytes()); // p_filesz
        elf.extend_from_slice(&text_size.to_le_bytes()); // p_memsz
        elf.extend_from_slice(&0x1000u64.to_le_bytes()); // p_align

        // Pad to page boundary
        while elf.len() < 0x1000 {
            elf.push(0);
        }

        // Write sections
        for section in &self.sections {
            elf.extend_from_slice(&section.data);
        }

        elf
    }
}
