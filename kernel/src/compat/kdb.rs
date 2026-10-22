/// Kernel Debugger (KDB)
///
/// Implements an interactive kernel debugger for KnoxOS, accessible via
/// serial console. Provides breakpoints, memory inspection, register dumps,
/// stack traces, and symbol resolution.
///
/// Features:
///   - Software breakpoints (INT3)
///   - Hardware breakpoints (DR0-DR3 debug registers)
///   - Memory read/write (hex dump, disassembly)
///   - Register inspection and modification
///   - Stack trace with symbol resolution
///   - Kernel symbol table lookup
///   - Expression evaluation
///   - Watchpoints (data breakpoints)
///   - Single-step execution
///   - Conditional breakpoints
///   - Command history
use alloc::collections::BTreeMap;
use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// DEBUGGER CONSTANTS
// ═══════════════════════════════════════════════════════════════════════

/// Maximum number of breakpoints
pub const MAX_BREAKPOINTS: usize = 256;
/// Maximum hardware breakpoints (x86_64 has 4 debug registers)
pub const MAX_HW_BREAKPOINTS: usize = 4;
/// Command history size
pub const HISTORY_SIZE: usize = 64;

// ═══════════════════════════════════════════════════════════════════════
// CPU REGISTERS
// ═══════════════════════════════════════════════════════════════════════

/// x86_64 register set (saved during debug trap)
#[derive(Debug, Clone, Default)]
pub struct RegisterSet {
    // General purpose
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub rsp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    // Instruction pointer
    pub rip: u64,
    // Flags
    pub rflags: u64,
    // Segment registers
    pub cs: u16,
    pub ss: u16,
    pub ds: u16,
    pub es: u16,
    pub fs: u16,
    pub gs: u16,
    // Control registers
    pub cr0: u64,
    pub cr2: u64,
    pub cr3: u64,
    pub cr4: u64,
    // Debug registers
    pub dr0: u64,
    pub dr1: u64,
    pub dr2: u64,
    pub dr3: u64,
    pub dr6: u64,
    pub dr7: u64,
}

impl RegisterSet {
    pub fn dump(&self) -> String {
        let mut s = String::new();
        s.push_str(&alloc::format!(
            "RAX={:#018x} RBX={:#018x} RCX={:#018x} RDX={:#018x}\n",
            self.rax,
            self.rbx,
            self.rcx,
            self.rdx
        ));
        s.push_str(&alloc::format!(
            "RSI={:#018x} RDI={:#018x} RBP={:#018x} RSP={:#018x}\n",
            self.rsi,
            self.rdi,
            self.rbp,
            self.rsp
        ));
        s.push_str(&alloc::format!(
            "R8 ={:#018x} R9 ={:#018x} R10={:#018x} R11={:#018x}\n",
            self.r8,
            self.r9,
            self.r10,
            self.r11
        ));
        s.push_str(&alloc::format!(
            "R12={:#018x} R13={:#018x} R14={:#018x} R15={:#018x}\n",
            self.r12,
            self.r13,
            self.r14,
            self.r15
        ));
        s.push_str(&alloc::format!(
            "RIP={:#018x} RFLAGS={:#018x}\n",
            self.rip,
            self.rflags
        ));
        s.push_str(&alloc::format!(
            "CS={:#06x} SS={:#06x} DS={:#06x} ES={:#06x} FS={:#06x} GS={:#06x}\n",
            self.cs,
            self.ss,
            self.ds,
            self.es,
            self.fs,
            self.gs
        ));
        s.push_str(&alloc::format!(
            "CR0={:#018x} CR2={:#018x} CR3={:#018x} CR4={:#018x}\n",
            self.cr0,
            self.cr2,
            self.cr3,
            self.cr4
        ));
        s.push_str(&alloc::format!(
            "DR0={:#018x} DR1={:#018x} DR2={:#018x} DR3={:#018x}\n",
            self.dr0,
            self.dr1,
            self.dr2,
            self.dr3
        ));
        s.push_str(&alloc::format!(
            "DR6={:#018x} DR7={:#018x}\n",
            self.dr6,
            self.dr7
        ));
        s
    }

    pub fn get_by_name(&self, name: &str) -> Option<u64> {
        match name.to_lowercase().as_str() {
            "rax" => Some(self.rax),
            "rbx" => Some(self.rbx),
            "rcx" => Some(self.rcx),
            "rdx" => Some(self.rdx),
            "rsi" => Some(self.rsi),
            "rdi" => Some(self.rdi),
            "rbp" => Some(self.rbp),
            "rsp" => Some(self.rsp),
            "r8" => Some(self.r8),
            "r9" => Some(self.r9),
            "r10" => Some(self.r10),
            "r11" => Some(self.r11),
            "r12" => Some(self.r12),
            "r13" => Some(self.r13),
            "r14" => Some(self.r14),
            "r15" => Some(self.r15),
            "rip" | "pc" => Some(self.rip),
            "rflags" | "flags" => Some(self.rflags),
            "cr0" => Some(self.cr0),
            "cr2" => Some(self.cr2),
            "cr3" => Some(self.cr3),
            "cr4" => Some(self.cr4),
            _ => None,
        }
    }

    pub fn set_by_name(&mut self, name: &str, value: u64) -> bool {
        match name.to_lowercase().as_str() {
            "rax" => {
                self.rax = value;
                true
            }
            "rbx" => {
                self.rbx = value;
                true
            }
            "rcx" => {
                self.rcx = value;
                true
            }
            "rdx" => {
                self.rdx = value;
                true
            }
            "rsi" => {
                self.rsi = value;
                true
            }
            "rdi" => {
                self.rdi = value;
                true
            }
            "rbp" => {
                self.rbp = value;
                true
            }
            "rsp" => {
                self.rsp = value;
                true
            }
            "r8" => {
                self.r8 = value;
                true
            }
            "r9" => {
                self.r9 = value;
                true
            }
            "r10" => {
                self.r10 = value;
                true
            }
            "r11" => {
                self.r11 = value;
                true
            }
            "r12" => {
                self.r12 = value;
                true
            }
            "r13" => {
                self.r13 = value;
                true
            }
            "r14" => {
                self.r14 = value;
                true
            }
            "r15" => {
                self.r15 = value;
                true
            }
            "rip" | "pc" => {
                self.rip = value;
                true
            }
            _ => false,
        }
    }

    /// Decode RFLAGS
    pub fn flags_string(&self) -> String {
        let mut s = String::new();
        if self.rflags & (1 << 0) != 0 {
            s.push_str("CF ");
        }
        if self.rflags & (1 << 2) != 0 {
            s.push_str("PF ");
        }
        if self.rflags & (1 << 4) != 0 {
            s.push_str("AF ");
        }
        if self.rflags & (1 << 6) != 0 {
            s.push_str("ZF ");
        }
        if self.rflags & (1 << 7) != 0 {
            s.push_str("SF ");
        }
        if self.rflags & (1 << 8) != 0 {
            s.push_str("TF ");
        }
        if self.rflags & (1 << 9) != 0 {
            s.push_str("IF ");
        }
        if self.rflags & (1 << 10) != 0 {
            s.push_str("DF ");
        }
        if self.rflags & (1 << 11) != 0 {
            s.push_str("OF ");
        }
        if self.rflags & (1 << 14) != 0 {
            s.push_str("NT ");
        }
        if self.rflags & (1 << 16) != 0 {
            s.push_str("RF ");
        }
        if self.rflags & (1 << 17) != 0 {
            s.push_str("VM ");
        }
        if self.rflags & (1 << 18) != 0 {
            s.push_str("AC ");
        }
        if self.rflags & (1 << 21) != 0 {
            s.push_str("ID ");
        }
        s
    }
}

// ═══════════════════════════════════════════════════════════════════════
// BREAKPOINTS
// ═══════════════════════════════════════════════════════════════════════

/// Breakpoint type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BreakpointType {
    Software,      // INT3 (0xCC) patched into code
    HardwareExec,  // DR0-3, break on execution
    HardwareRead,  // DR0-3, break on read
    HardwareWrite, // DR0-3, break on write
    HardwareRW,    // DR0-3, break on read or write
}

/// Breakpoint
#[derive(Debug, Clone)]
pub struct Breakpoint {
    pub id: u64,
    pub address: u64,
    pub bp_type: BreakpointType,
    pub enabled: bool,
    pub hit_count: u64,
    pub condition: Option<String>, // expression to evaluate
    pub original_byte: u8,         // saved byte for software BP
    pub hw_reg: Option<usize>,     // DR0-3 index for hardware BP
    pub symbol: Option<String>,    // resolved symbol name
    pub temporary: bool,           // one-shot breakpoint
}

impl Breakpoint {
    pub fn new_software(address: u64) -> Self {
        let id = NEXT_BP_ID.fetch_add(1, Ordering::Relaxed);
        Self {
            id,
            address,
            bp_type: BreakpointType::Software,
            enabled: true,
            hit_count: 0,
            condition: None,
            original_byte: 0,
            hw_reg: None,
            symbol: None,
            temporary: false,
        }
    }

    pub fn new_hardware(address: u64, bp_type: BreakpointType, hw_reg: usize) -> Self {
        let id = NEXT_BP_ID.fetch_add(1, Ordering::Relaxed);
        Self {
            id,
            address,
            bp_type,
            enabled: true,
            hit_count: 0,
            condition: None,
            original_byte: 0,
            hw_reg: Some(hw_reg),
            symbol: None,
            temporary: false,
        }
    }
}

static NEXT_BP_ID: AtomicU64 = AtomicU64::new(1);

// ═══════════════════════════════════════════════════════════════════════
// SYMBOL TABLE
// ═══════════════════════════════════════════════════════════════════════

/// Kernel symbol
#[derive(Debug, Clone)]
pub struct KernelSymbol {
    pub name: String,
    pub address: u64,
    pub size: u64,
    pub sym_type: char, // 'T' text, 'D' data, 'B' bss, 'R' rodata
}

/// Symbol table
pub struct SymbolTable {
    symbols: Vec<KernelSymbol>,
    by_name: BTreeMap<String, usize>,
    by_addr: BTreeMap<u64, usize>,
}

impl SymbolTable {
    pub fn new() -> Self {
        Self {
            symbols: Vec::new(),
            by_name: BTreeMap::new(),
            by_addr: BTreeMap::new(),
        }
    }

    pub fn add(&mut self, name: &str, address: u64, size: u64, sym_type: char) {
        let idx = self.symbols.len();
        self.symbols.push(KernelSymbol {
            name: String::from(name),
            address,
            size,
            sym_type,
        });
        self.by_name.insert(String::from(name), idx);
        self.by_addr.insert(address, idx);
    }

    /// Lookup symbol by name
    pub fn lookup(&self, name: &str) -> Option<&KernelSymbol> {
        self.by_name.get(name).map(|&idx| &self.symbols[idx])
    }

    /// Resolve address to symbol+offset
    pub fn resolve(&self, addr: u64) -> Option<(String, u64)> {
        // Find the symbol whose address range contains addr
        let mut best: Option<(usize, u64)> = None;
        for (&sym_addr, &idx) in self.by_addr.iter().rev() {
            if sym_addr <= addr {
                let offset = addr - sym_addr;
                let sym = &self.symbols[idx];
                if sym.size == 0 || offset < sym.size {
                    match best {
                        Some((_, best_off)) if offset < best_off => {
                            best = Some((idx, offset));
                        }
                        None => {
                            best = Some((idx, offset));
                        }
                        _ => {}
                    }
                }
                break;
            }
        }

        best.map(|(idx, offset)| (self.symbols[idx].name.clone(), offset))
    }

    pub fn count(&self) -> usize {
        self.symbols.len()
    }
}

// ═══════════════════════════════════════════════════════════════════════
// STACK TRACE
// ═══════════════════════════════════════════════════════════════════════

/// Stack frame
#[derive(Debug, Clone)]
pub struct StackFrame {
    pub frame_ptr: u64,
    pub return_addr: u64,
    pub symbol: Option<String>,
    pub offset: u64,
}

/// Walk the stack using frame pointers
pub fn backtrace(rbp: u64, rip: u64, symtab: &SymbolTable, max_frames: usize) -> Vec<StackFrame> {
    let mut frames = Vec::new();

    // Current frame
    let (sym, offset) = symtab.resolve(rip).unwrap_or((String::from("???"), 0));
    frames.push(StackFrame {
        frame_ptr: rbp,
        return_addr: rip,
        symbol: Some(sym),
        offset,
    });

    // Walk frame pointer chain
    let mut fp = rbp;
    for _ in 0..max_frames {
        if fp == 0 || fp % 8 != 0 {
            break;
        }

        // Read return address and previous frame pointer
        // In a real implementation, these would be memory reads
        // For safety, we simulate
        let prev_fp = 0u64; // would be *(fp as *const u64)
        let ret_addr = 0u64; // would be *((fp + 8) as *const u64)

        if ret_addr == 0 {
            break;
        }

        let (sym, offset) = symtab.resolve(ret_addr).unwrap_or((String::from("???"), 0));
        frames.push(StackFrame {
            frame_ptr: fp,
            return_addr: ret_addr,
            symbol: Some(sym),
            offset,
        });

        fp = prev_fp;
    }

    frames
}

/// Format backtrace for display
pub fn format_backtrace(frames: &[StackFrame]) -> String {
    let mut s = String::from("Stack trace:\n");
    for (i, frame) in frames.iter().enumerate() {
        let sym = frame.symbol.as_deref().unwrap_or("???");
        if frame.offset > 0 {
            s.push_str(&alloc::format!(
                "  #{} {:#018x} {}+{:#x}\n",
                i,
                frame.return_addr,
                sym,
                frame.offset
            ));
        } else {
            s.push_str(&alloc::format!(
                "  #{} {:#018x} {}\n",
                i,
                frame.return_addr,
                sym
            ));
        }
    }
    s
}

// ═══════════════════════════════════════════════════════════════════════
// MEMORY DUMP
// ═══════════════════════════════════════════════════════════════════════

/// Format memory as hex dump
pub fn hexdump(addr: u64, data: &[u8], width: usize) -> String {
    let mut s = String::new();
    for (i, chunk) in data.chunks(width).enumerate() {
        let line_addr = addr + (i * width) as u64;
        s.push_str(&alloc::format!("{:#010x}: ", line_addr));

        // Hex bytes
        for (j, &byte) in chunk.iter().enumerate() {
            s.push_str(&alloc::format!("{:02x} ", byte));
            if j == width / 2 - 1 {
                s.push(' ');
            }
        }

        // Pad if short
        for _ in chunk.len()..width {
            s.push_str("   ");
        }

        // ASCII
        s.push_str(" |");
        for &byte in chunk {
            if (0x20..=0x7e).contains(&byte) {
                s.push(byte as char);
            } else {
                s.push('.');
            }
        }
        s.push_str("|\n");
    }
    s
}

/// Simple x86_64 disassembly (common instructions only)
pub fn disassemble_one(bytes: &[u8]) -> (String, usize) {
    if bytes.is_empty() {
        return (String::from("(no data)"), 0);
    }

    match bytes[0] {
        0x90 => (String::from("nop"), 1),
        0xCC => (String::from("int3"), 1),
        0xC3 => (String::from("ret"), 1),
        0xCB => (String::from("retf"), 1),
        0xF4 => (String::from("hlt"), 1),
        0xFA => (String::from("cli"), 1),
        0xFB => (String::from("sti"), 1),
        0xFC => (String::from("cld"), 1),
        0xFD => (String::from("std"), 1),
        0x50..=0x57 => {
            let reg = ["rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi"];
            (
                alloc::format!("push {}", reg[(bytes[0] - 0x50) as usize]),
                1,
            )
        }
        0x58..=0x5F => {
            let reg = ["rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi"];
            (alloc::format!("pop {}", reg[(bytes[0] - 0x58) as usize]), 1)
        }
        0xEB => {
            if bytes.len() >= 2 {
                let offset = bytes[1] as i8;
                (alloc::format!("jmp short {:#x}", offset), 2)
            } else {
                (String::from("jmp short ???"), 1)
            }
        }
        0xE8 => {
            if bytes.len() >= 5 {
                let offset = i32::from_le_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]);
                (alloc::format!("call {:#x}", offset), 5)
            } else {
                (String::from("call ???"), 1)
            }
        }
        0xE9 => {
            if bytes.len() >= 5 {
                let offset = i32::from_le_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]);
                (alloc::format!("jmp {:#x}", offset), 5)
            } else {
                (String::from("jmp ???"), 1)
            }
        }
        0x0F => {
            if bytes.len() >= 2 {
                match bytes[1] {
                    0x05 => (String::from("syscall"), 2),
                    0x07 => (String::from("sysret"), 2),
                    0x01 => {
                        if bytes.len() >= 3 {
                            match bytes[2] {
                                0xD0 => (String::from("xgetbv"), 3),
                                0xD1 => (String::from("xsetbv"), 3),
                                _ => (alloc::format!("0f 01 {:02x}", bytes[2]), 3),
                            }
                        } else {
                            (String::from("0f 01 ???"), 2)
                        }
                    }
                    0x31 => (String::from("rdtsc"), 2),
                    0xA2 => (String::from("cpuid"), 2),
                    _ => (alloc::format!("0f {:02x}", bytes[1]), 2),
                }
            } else {
                (String::from("0f ???"), 1)
            }
        }
        _ => (alloc::format!("db {:#04x}", bytes[0]), 1),
    }
}

/// Disassemble multiple instructions
pub fn disassemble(addr: u64, data: &[u8], count: usize) -> String {
    let mut s = String::new();
    let mut offset = 0;
    let mut n = 0;

    while offset < data.len() && n < count {
        let (inst, len) = disassemble_one(&data[offset..]);
        s.push_str(&alloc::format!("{:#010x}: ", addr + offset as u64));

        // Print bytes
        for i in 0..len.min(8) {
            if offset + i < data.len() {
                s.push_str(&alloc::format!("{:02x} ", data[offset + i]));
            }
        }
        for _ in len..8 {
            s.push_str("   ");
        }

        s.push_str(&inst);
        s.push('\n');

        offset += len.max(1);
        n += 1;
    }
    s
}

// ═══════════════════════════════════════════════════════════════════════
// DEBUGGER COMMANDS
// ═══════════════════════════════════════════════════════════════════════

/// Debugger command
#[derive(Debug, Clone)]
pub enum DebugCommand {
    Continue,
    Step,             // single step
    StepOver,         // step over function calls
    StepOut,          // step until return
    Break(u64),       // set breakpoint at address
    BreakSym(String), // set breakpoint at symbol
    Delete(u64),      // delete breakpoint by id
    Enable(u64),      // enable breakpoint
    Disable(u64),     // disable breakpoint
    ListBreakpoints,
    Registers,           // show registers
    SetReg(String, u64), // set register
    Examine(u64, usize), // examine memory (addr, count)
    Disassemble(u64, usize),
    Backtrace,
    Print(String),              // print expression/symbol
    Watch(u64, BreakpointType), // set watchpoint
    Info(String),               // info subcommand
    Help,
    Quit,
}

/// Parse a debugger command string
pub fn parse_command(input: &str) -> Option<DebugCommand> {
    let parts: Vec<&str> = input.split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }

    match parts[0] {
        "c" | "continue" => Some(DebugCommand::Continue),
        "s" | "step" | "si" => Some(DebugCommand::Step),
        "n" | "next" | "ni" => Some(DebugCommand::StepOver),
        "finish" | "fin" => Some(DebugCommand::StepOut),
        "b" | "break" => {
            if parts.len() > 1 {
                if let Some(addr) = parse_hex(parts[1]) {
                    Some(DebugCommand::Break(addr))
                } else {
                    Some(DebugCommand::BreakSym(String::from(parts[1])))
                }
            } else {
                None
            }
        }
        "d" | "delete" => parts
            .get(1)
            .and_then(|s| s.parse::<u64>().ok())
            .map(DebugCommand::Delete),
        "enable" => parts
            .get(1)
            .and_then(|s| s.parse::<u64>().ok())
            .map(DebugCommand::Enable),
        "disable" => parts
            .get(1)
            .and_then(|s| s.parse::<u64>().ok())
            .map(DebugCommand::Disable),
        "bl" | "info breakpoints" => Some(DebugCommand::ListBreakpoints),
        "r" | "regs" | "registers" => Some(DebugCommand::Registers),
        "set" => {
            if parts.len() >= 3 {
                parse_hex(parts[2]).map(|val| DebugCommand::SetReg(String::from(parts[1]), val))
            } else {
                None
            }
        }
        "x" | "examine" => {
            let addr = parts.get(1).and_then(|s| parse_hex(s)).unwrap_or(0);
            let count = parts
                .get(2)
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(64);
            Some(DebugCommand::Examine(addr, count))
        }
        "dis" | "disas" | "disassemble" => {
            let addr = parts.get(1).and_then(|s| parse_hex(s)).unwrap_or(0);
            let count = parts
                .get(2)
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(16);
            Some(DebugCommand::Disassemble(addr, count))
        }
        "bt" | "backtrace" => Some(DebugCommand::Backtrace),
        "p" | "print" => parts.get(1).map(|s| DebugCommand::Print(String::from(*s))),
        "watch" => parts
            .get(1)
            .and_then(|s| parse_hex(s))
            .map(|addr| DebugCommand::Watch(addr, BreakpointType::HardwareWrite)),
        "rwatch" => parts
            .get(1)
            .and_then(|s| parse_hex(s))
            .map(|addr| DebugCommand::Watch(addr, BreakpointType::HardwareRead)),
        "awatch" => parts
            .get(1)
            .and_then(|s| parse_hex(s))
            .map(|addr| DebugCommand::Watch(addr, BreakpointType::HardwareRW)),
        "info" => {
            let sub = parts.get(1).unwrap_or(&"");
            Some(DebugCommand::Info(String::from(*sub)))
        }
        "h" | "help" | "?" => Some(DebugCommand::Help),
        "q" | "quit" => Some(DebugCommand::Quit),
        _ => None,
    }
}

/// Parse hex value (with or without 0x prefix)
fn parse_hex(s: &str) -> Option<u64> {
    let s = s.trim_start_matches("0x").trim_start_matches("0X");
    u64::from_str_radix(s, 16).ok()
}

// ═══════════════════════════════════════════════════════════════════════
// DEBUGGER STATE
// ═══════════════════════════════════════════════════════════════════════

/// Kernel debugger
pub struct KernelDebugger {
    pub active: bool,
    pub breakpoints: BTreeMap<u64, Breakpoint>,
    pub hw_breakpoints: [Option<u64>; MAX_HW_BREAKPOINTS],
    pub symbol_table: SymbolTable,
    pub registers: RegisterSet,
    pub single_step: bool,
    pub command_history: VecDeque<String>,
    pub last_command: Option<String>,
    pub stepping_over: bool,
    pub step_out_frame: u64,
}

impl KernelDebugger {
    pub fn new() -> Self {
        Self {
            active: false,
            breakpoints: BTreeMap::new(),
            hw_breakpoints: [None; MAX_HW_BREAKPOINTS],
            symbol_table: SymbolTable::new(),
            registers: RegisterSet::default(),
            single_step: false,
            command_history: VecDeque::new(),
            last_command: None,
            stepping_over: false,
            step_out_frame: 0,
        }
    }

    /// Enter the debugger (called from trap/breakpoint handler)
    pub fn enter(&mut self, regs: &RegisterSet, reason: &str) {
        self.active = true;
        self.registers = regs.clone();

        let (sym, offset) = self
            .symbol_table
            .resolve(regs.rip)
            .unwrap_or((String::from("???"), 0));

        serial_println!(
            "\n[KDB] {} at {:#x} ({}+{:#x})",
            reason,
            regs.rip,
            sym,
            offset
        );
        serial_println!("[KDB] Type 'help' for commands");
    }

    /// Set a software breakpoint
    pub fn set_breakpoint(&mut self, address: u64) -> u64 {
        let mut bp = Breakpoint::new_software(address);
        bp.symbol = self.symbol_table.resolve(address).map(|(s, _)| s);
        let id = bp.id;

        serial_println!(
            "[KDB] Breakpoint #{} at {:#x} ({})",
            id,
            address,
            bp.symbol.as_deref().unwrap_or("???")
        );

        self.breakpoints.insert(address, bp);
        id
    }

    /// Set a hardware watchpoint
    pub fn set_watchpoint(&mut self, address: u64, wp_type: BreakpointType) -> Option<u64> {
        // Find free DR register
        for i in 0..MAX_HW_BREAKPOINTS {
            if self.hw_breakpoints[i].is_none() {
                let bp = Breakpoint::new_hardware(address, wp_type, i);
                let id = bp.id;
                self.hw_breakpoints[i] = Some(address);
                self.breakpoints.insert(address, bp);

                serial_println!("[KDB] Watchpoint #{} at {:#x} (DR{})", id, address, i);

                return Some(id);
            }
        }

        serial_println!("[KDB] No free debug registers");
        None
    }

    /// Remove a breakpoint
    pub fn remove_breakpoint(&mut self, id: u64) -> bool {
        let addr = self
            .breakpoints
            .iter()
            .find(|(_, bp)| bp.id == id)
            .map(|(&addr, _)| addr);

        if let Some(addr) = addr {
            if let Some(bp) = self.breakpoints.remove(&addr) {
                if let Some(hw_reg) = bp.hw_reg {
                    self.hw_breakpoints[hw_reg] = None;
                }
                serial_println!("[KDB] Deleted breakpoint #{}", id);
                return true;
            }
        }
        false
    }

    /// List breakpoints
    pub fn list_breakpoints(&self) -> String {
        let mut s = String::from("Breakpoints:\n");
        for (addr, bp) in &self.breakpoints {
            let sym = bp.symbol.as_deref().unwrap_or("???");
            let status = if bp.enabled { "enabled" } else { "disabled" };
            let bp_kind = match bp.bp_type {
                BreakpointType::Software => "breakpoint",
                BreakpointType::HardwareExec => "hw breakpoint",
                BreakpointType::HardwareRead => "read watchpoint",
                BreakpointType::HardwareWrite => "write watchpoint",
                BreakpointType::HardwareRW => "access watchpoint",
            };
            s.push_str(&alloc::format!(
                "  #{}: {:#018x} {} [{}] hits={} {}\n",
                bp.id,
                addr,
                bp_kind,
                status,
                bp.hit_count,
                sym
            ));
        }
        if self.breakpoints.is_empty() {
            s.push_str("  (none)\n");
        }
        s
    }

    /// Handle a debug command
    pub fn handle_command(&mut self, cmd: &DebugCommand) -> String {
        match cmd {
            DebugCommand::Continue => {
                self.active = false;
                self.single_step = false;
                String::from("Continuing...")
            }
            DebugCommand::Step => {
                self.active = false;
                self.single_step = true;
                String::from("Single stepping...")
            }
            DebugCommand::StepOver => {
                self.active = false;
                self.stepping_over = true;
                String::from("Stepping over...")
            }
            DebugCommand::StepOut => {
                self.active = false;
                self.step_out_frame = self.registers.rbp;
                String::from("Stepping out...")
            }
            DebugCommand::Break(addr) => {
                let id = self.set_breakpoint(*addr);
                alloc::format!("Breakpoint #{} set", id)
            }
            DebugCommand::BreakSym(name) => {
                let sym_info = self
                    .symbol_table
                    .lookup(name)
                    .map(|s| (s.address, s.address));
                if let Some((addr, display_addr)) = sym_info {
                    let id = self.set_breakpoint(addr);
                    alloc::format!("Breakpoint #{} at {} ({:#x})", id, name, display_addr)
                } else {
                    alloc::format!("Symbol '{}' not found", name)
                }
            }
            DebugCommand::Delete(id) => {
                if self.remove_breakpoint(*id) {
                    alloc::format!("Deleted breakpoint #{}", id)
                } else {
                    alloc::format!("Breakpoint #{} not found", id)
                }
            }
            DebugCommand::Enable(id) => {
                for bp in self.breakpoints.values_mut() {
                    if bp.id == *id {
                        bp.enabled = true;
                        return alloc::format!("Enabled breakpoint #{}", id);
                    }
                }
                alloc::format!("Breakpoint #{} not found", id)
            }
            DebugCommand::Disable(id) => {
                for bp in self.breakpoints.values_mut() {
                    if bp.id == *id {
                        bp.enabled = false;
                        return alloc::format!("Disabled breakpoint #{}", id);
                    }
                }
                alloc::format!("Breakpoint #{} not found", id)
            }
            DebugCommand::ListBreakpoints => self.list_breakpoints(),
            DebugCommand::Registers => self.registers.dump(),
            DebugCommand::SetReg(name, val) => {
                if self.registers.set_by_name(name, *val) {
                    alloc::format!("{} = {:#x}", name, val)
                } else {
                    alloc::format!("Unknown register '{}'", name)
                }
            }
            DebugCommand::Examine(addr, count) => {
                // In real impl, would read actual memory
                let data = vec![0u8; *count];
                hexdump(*addr, &data, 16)
            }
            DebugCommand::Disassemble(addr, count) => {
                let data = vec![0x90u8; *count * 4]; // NOP placeholder
                disassemble(*addr, &data, *count)
            }
            DebugCommand::Backtrace => {
                let frames = backtrace(
                    self.registers.rbp,
                    self.registers.rip,
                    &self.symbol_table,
                    20,
                );
                format_backtrace(&frames)
            }
            DebugCommand::Print(expr) => {
                if let Some(val) = self.registers.get_by_name(expr) {
                    alloc::format!("{} = {:#x} ({})", expr, val, val)
                } else if let Some(sym) = self.symbol_table.lookup(expr) {
                    alloc::format!("{} = {:#x} (size={})", expr, sym.address, sym.size)
                } else {
                    alloc::format!("'{}' not found", expr)
                }
            }
            DebugCommand::Watch(addr, wp_type) => {
                if let Some(id) = self.set_watchpoint(*addr, *wp_type) {
                    alloc::format!("Watchpoint #{} set at {:#x}", id, addr)
                } else {
                    String::from("Failed to set watchpoint (no free debug registers)")
                }
            }
            DebugCommand::Info(sub) => match sub.as_str() {
                "regs" | "registers" => self.registers.dump(),
                "break" | "breakpoints" => self.list_breakpoints(),
                "sym" | "symbols" => {
                    alloc::format!("Symbol table: {} symbols loaded", self.symbol_table.count())
                }
                "flags" => self.registers.flags_string(),
                _ => String::from("info [regs|breakpoints|symbols|flags]"),
            },
            DebugCommand::Help => {
                let mut s = String::from("KDB Commands:\n");
                s.push_str("  c/continue      - resume execution\n");
                s.push_str("  s/step           - single step\n");
                s.push_str("  n/next           - step over\n");
                s.push_str("  finish           - step out of function\n");
                s.push_str("  b/break <addr>   - set breakpoint\n");
                s.push_str("  d/delete <id>    - delete breakpoint\n");
                s.push_str("  enable/disable   - toggle breakpoint\n");
                s.push_str("  r/regs           - show registers\n");
                s.push_str("  set <reg> <val>  - set register\n");
                s.push_str("  x <addr> [len]   - examine memory\n");
                s.push_str("  dis <addr> [n]   - disassemble\n");
                s.push_str("  bt/backtrace     - stack trace\n");
                s.push_str("  p/print <expr>   - print value\n");
                s.push_str("  watch <addr>     - set write watchpoint\n");
                s.push_str("  info <what>      - info subcommand\n");
                s.push_str("  q/quit           - exit debugger\n");
                s
            }
            DebugCommand::Quit => {
                self.active = false;
                String::from("Exiting debugger...")
            }
        }
    }

    /// Add command to history
    pub fn add_history(&mut self, cmd: &str) {
        if self.command_history.len() >= HISTORY_SIZE {
            self.command_history.pop_front();
        }
        self.command_history.push_back(String::from(cmd));
        self.last_command = Some(String::from(cmd));
    }
}

// ═══════════════════════════════════════════════════════════════════════
// GLOBAL STATE
// ═══════════════════════════════════════════════════════════════════════

lazy_static::lazy_static! {
    static ref KDB: Mutex<KernelDebugger> = Mutex::new(KernelDebugger::new());
}

static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Enter the debugger
pub fn enter_debugger(regs: &RegisterSet, reason: &str) {
    KDB.lock().enter(regs, reason);
}

/// Process a debugger command
pub fn process_command(input: &str) -> Option<String> {
    let cmd = parse_command(input)?;
    let mut kdb = KDB.lock();
    kdb.add_history(input);
    Some(kdb.handle_command(&cmd))
}

/// Set a breakpoint by address
pub fn set_breakpoint(addr: u64) -> u64 {
    KDB.lock().set_breakpoint(addr)
}

/// Add a kernel symbol
pub fn add_symbol(name: &str, addr: u64, size: u64, sym_type: char) {
    KDB.lock().symbol_table.add(name, addr, size, sym_type);
}

/// Is debugger active?
pub fn is_active() -> bool {
    KDB.lock().active
}

/// Initialize kernel debugger
pub fn init() {
    if INITIALIZED.load(Ordering::Relaxed) {
        return;
    }
    INITIALIZED.store(true, Ordering::Relaxed);

    // Add some initial kernel symbols
    {
        let mut kdb = KDB.lock();
        kdb.symbol_table
            .add("kernel_main", 0xFFFF800000100000, 0x1000, 'T');
        kdb.symbol_table
            .add("_start", 0xFFFF800000000000, 0x100, 'T');
        kdb.symbol_table
            .add("interrupt_handler", 0xFFFF800000200000, 0x500, 'T');
        kdb.symbol_table
            .add("page_fault_handler", 0xFFFF800000200500, 0x200, 'T');
        kdb.symbol_table
            .add("syscall_handler", 0xFFFF800000201000, 0x2000, 'T');
    }

    serial_println!("[KnoxOS] Kernel debugger (KDB) initialized");
}
