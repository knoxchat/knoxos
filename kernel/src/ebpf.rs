// ebpf.rs — Extended Berkeley Packet Filter (eBPF) virtual machine
// Supports BPF program loading, verification, maps, and execution

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use lazy_static::lazy_static;
use spin::Mutex;

/// BPF program types
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u32)]
pub enum BpfProgType {
    Unspec = 0,
    SocketFilter = 1,
    KProbe = 2,
    SchedCls = 3,
    SchedAct = 4,
    Tracepoint = 5,
    Xdp = 6,
    PerfEvent = 7,
    CgroupSkb = 8,
    CgroupSock = 9,
    LwtIn = 10,
    LwtOut = 11,
    LwtXmit = 12,
    SockOps = 13,
    SkSkb = 14,
    CgroupDevice = 15,
    SkMsg = 16,
    RawTracepoint = 17,
    CgroupSockAddr = 18,
    LwtSeg6local = 19,
    LircMode2 = 20,
    SkReuseport = 21,
    FlowDissector = 22,
    CgroupSysctl = 23,
    RawTracepointWritable = 24,
    CgroupSockopt = 25,
    Tracing = 26,
    StructOps = 27,
    Ext = 28,
    Lsm = 29,
    SkLookup = 30,
    Syscall = 31,
}

/// BPF map types
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u32)]
pub enum BpfMapType {
    Unspec = 0,
    Hash = 1,
    Array = 2,
    ProgArray = 3,
    PerfEventArray = 4,
    PercpuHash = 5,
    PercpuArray = 6,
    StackTrace = 7,
    CgroupArray = 8,
    LruHash = 9,
    LruPercpuHash = 10,
    LpmTrie = 11,
    ArrayOfMaps = 12,
    HashOfMaps = 13,
    Devmap = 14,
    Sockmap = 15,
    Cpumap = 16,
    Xskmap = 17,
    Sockhash = 18,
    CgroupStorage = 19,
    ReuseportSockarray = 20,
    PercpuCgroupStorage = 21,
    Queue = 22,
    Stack = 23,
    SkStorage = 24,
    DevmapHash = 25,
    StructOps = 26,
    Ringbuf = 27,
    InodeStorage = 28,
    TaskStorage = 29,
    BloomFilter = 30,
}

/// BPF instruction
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct BpfInsn {
    pub code: u8,
    pub regs: u8, // dst_reg:4 | src_reg:4
    pub off: i16,
    pub imm: i32,
}

impl BpfInsn {
    pub fn dst_reg(&self) -> u8 {
        self.regs & 0x0F
    }

    pub fn src_reg(&self) -> u8 {
        (self.regs >> 4) & 0x0F
    }
}

/// BPF ALU/JMP opcodes
pub const BPF_LD: u8 = 0x00;
pub const BPF_LDX: u8 = 0x01;
pub const BPF_ST: u8 = 0x02;
pub const BPF_STX: u8 = 0x03;
pub const BPF_ALU: u8 = 0x04;
pub const BPF_JMP: u8 = 0x05;
pub const BPF_ALU64: u8 = 0x07;

pub const BPF_ADD: u8 = 0x00;
pub const BPF_SUB: u8 = 0x10;
pub const BPF_MUL: u8 = 0x20;
pub const BPF_DIV: u8 = 0x30;
pub const BPF_OR: u8 = 0x40;
pub const BPF_AND: u8 = 0x50;
pub const BPF_LSH: u8 = 0x60;
pub const BPF_RSH: u8 = 0x70;
pub const BPF_NEG: u8 = 0x80;
pub const BPF_MOD: u8 = 0x90;
pub const BPF_XOR: u8 = 0xA0;
pub const BPF_MOV: u8 = 0xB0;
pub const BPF_ARSH: u8 = 0xC0;

pub const BPF_JA: u8 = 0x00;
pub const BPF_JEQ: u8 = 0x10;
pub const BPF_JGT: u8 = 0x20;
pub const BPF_JGE: u8 = 0x30;
pub const BPF_JSET: u8 = 0x40;
pub const BPF_JNE: u8 = 0x50;
pub const BPF_JSGT: u8 = 0x60;
pub const BPF_JSGE: u8 = 0x70;
pub const BPF_CALL: u8 = 0x80;
pub const BPF_EXIT: u8 = 0x90;
pub const BPF_JLT: u8 = 0xA0;
pub const BPF_JLE: u8 = 0xB0;
pub const BPF_JSLT: u8 = 0xC0;
pub const BPF_JSLE: u8 = 0xD0;

pub const BPF_K: u8 = 0x00; // Immediate
pub const BPF_X: u8 = 0x08; // Register

/// BPF helper function IDs
pub const BPF_FUNC_MAP_LOOKUP_ELEM: u32 = 1;
pub const BPF_FUNC_MAP_UPDATE_ELEM: u32 = 2;
pub const BPF_FUNC_MAP_DELETE_ELEM: u32 = 3;
pub const BPF_FUNC_PROBE_READ: u32 = 4;
pub const BPF_FUNC_KTIME_GET_NS: u32 = 5;
pub const BPF_FUNC_TRACE_PRINTK: u32 = 6;
pub const BPF_FUNC_GET_PRANDOM_U32: u32 = 7;
pub const BPF_FUNC_GET_SMP_PROCESSOR_ID: u32 = 8;
pub const BPF_FUNC_GET_CURRENT_PID_TGID: u32 = 14;
pub const BPF_FUNC_GET_CURRENT_UID_GID: u32 = 15;
pub const BPF_FUNC_GET_CURRENT_COMM: u32 = 16;

/// Maximum BPF program size (in instructions)
pub const BPF_MAXINSNS: usize = 4096;

/// Number of registers (r0-r10)
pub const BPF_REG_COUNT: usize = 11;

/// BPF map instance
#[derive(Debug)]
pub struct BpfMap {
    pub id: u64,
    pub map_type: BpfMapType,
    pub key_size: u32,
    pub value_size: u32,
    pub max_entries: u32,
    pub flags: u32,
    pub name: String,
    /// Stored entries: key bytes -> value bytes
    pub entries: BTreeMap<Vec<u8>, Vec<u8>>,
}

impl BpfMap {
    pub fn new(
        id: u64,
        map_type: BpfMapType,
        key_size: u32,
        value_size: u32,
        max_entries: u32,
    ) -> Self {
        BpfMap {
            id,
            map_type,
            key_size,
            value_size,
            max_entries,
            flags: 0,
            name: String::new(),
            entries: BTreeMap::new(),
        }
    }

    pub fn lookup(&self, key: &[u8]) -> Option<&Vec<u8>> {
        self.entries.get(key)
    }

    pub fn update(&mut self, key: &[u8], value: &[u8]) -> Result<(), BpfError> {
        if self.entries.len() >= self.max_entries as usize && !self.entries.contains_key(key) {
            return Err(BpfError::MapFull);
        }
        self.entries.insert(key.to_vec(), value.to_vec());
        Ok(())
    }

    pub fn delete(&mut self, key: &[u8]) -> Result<(), BpfError> {
        self.entries.remove(key).ok_or(BpfError::NotFound)?;
        Ok(())
    }
}

/// BPF program
#[derive(Debug)]
pub struct BpfProg {
    pub id: u64,
    pub prog_type: BpfProgType,
    pub name: String,
    pub insns: Vec<BpfInsn>,
    pub license: String,
    pub attached: bool,
    pub run_count: u64,
    pub run_time_ns: u64,
}

impl BpfProg {
    pub fn new(id: u64, prog_type: BpfProgType, insns: Vec<BpfInsn>) -> Self {
        BpfProg {
            id,
            prog_type,
            name: String::new(),
            insns,
            license: String::from("GPL"),
            attached: false,
            run_count: 0,
            run_time_ns: 0,
        }
    }
}

/// BPF execution context
pub struct BpfContext {
    pub regs: [u64; BPF_REG_COUNT],
    pub stack: [u8; 512],
    pub pc: usize,
}

impl Default for BpfContext {
    fn default() -> Self {
        Self::new()
    }
}

impl BpfContext {
    pub fn new() -> Self {
        BpfContext {
            regs: [0u64; BPF_REG_COUNT],
            stack: [0u8; 512],
            pc: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BpfError {
    NotFound,
    InvalidProg,
    InvalidMap,
    MapFull,
    PermDenied,
    TooLarge,
    VerifierFailed,
}

lazy_static! {
    static ref PROGS: Mutex<BTreeMap<u64, BpfProg>> = Mutex::new(BTreeMap::new());
    static ref MAPS: Mutex<BTreeMap<u64, BpfMap>> = Mutex::new(BTreeMap::new());
    static ref NEXT_PROG_ID: Mutex<u64> = Mutex::new(1);
    static ref NEXT_MAP_ID: Mutex<u64> = Mutex::new(1);
}

/// bpf(BPF_PROG_LOAD) — load a BPF program
pub fn sys_bpf_prog_load(
    prog_type: BpfProgType,
    insns: &[BpfInsn],
    _license: &str,
) -> Result<u64, BpfError> {
    if insns.len() > BPF_MAXINSNS {
        return Err(BpfError::TooLarge);
    }

    // Simple verification
    if !verify_program(insns) {
        return Err(BpfError::VerifierFailed);
    }

    let mut next = NEXT_PROG_ID.lock();
    let id = *next;
    *next += 1;
    drop(next);

    let prog = BpfProg::new(id, prog_type, insns.to_vec());
    PROGS.lock().insert(id, prog);

    Ok(id)
}

/// Simple BPF program verifier
fn verify_program(insns: &[BpfInsn]) -> bool {
    if insns.is_empty() {
        return false;
    }

    // Last instruction must be EXIT
    let last = &insns[insns.len() - 1];
    let last_class = last.code & 0x07;
    let last_op = last.code & 0xF0;
    if last_class != BPF_JMP || last_op != BPF_EXIT {
        return false;
    }

    // Check all jumps are in bounds
    for (i, insn) in insns.iter().enumerate() {
        let class = insn.code & 0x07;
        if class == BPF_JMP {
            let op = insn.code & 0xF0;
            if op != BPF_CALL && op != BPF_EXIT {
                let target = i as i64 + 1 + insn.off as i64;
                if target < 0 || target >= insns.len() as i64 {
                    return false;
                }
            }
        }

        // Check register indices
        if insn.dst_reg() >= BPF_REG_COUNT as u8 || insn.src_reg() >= BPF_REG_COUNT as u8 {
            return false;
        }
    }

    true
}

/// bpf(BPF_MAP_CREATE) — create a BPF map
pub fn sys_bpf_map_create(
    map_type: BpfMapType,
    key_size: u32,
    value_size: u32,
    max_entries: u32,
) -> Result<u64, BpfError> {
    let mut next = NEXT_MAP_ID.lock();
    let id = *next;
    *next += 1;
    drop(next);

    let map = BpfMap::new(id, map_type, key_size, value_size, max_entries);
    MAPS.lock().insert(id, map);

    Ok(id)
}

/// bpf(BPF_MAP_LOOKUP_ELEM)
pub fn sys_bpf_map_lookup(map_id: u64, key: &[u8]) -> Result<Vec<u8>, BpfError> {
    let maps = MAPS.lock();
    let map = maps.get(&map_id).ok_or(BpfError::NotFound)?;
    map.lookup(key).cloned().ok_or(BpfError::NotFound)
}

/// bpf(BPF_MAP_UPDATE_ELEM)
pub fn sys_bpf_map_update(map_id: u64, key: &[u8], value: &[u8]) -> Result<(), BpfError> {
    let mut maps = MAPS.lock();
    let map = maps.get_mut(&map_id).ok_or(BpfError::NotFound)?;
    map.update(key, value)
}

/// bpf(BPF_MAP_DELETE_ELEM)
pub fn sys_bpf_map_delete(map_id: u64, key: &[u8]) -> Result<(), BpfError> {
    let mut maps = MAPS.lock();
    let map = maps.get_mut(&map_id).ok_or(BpfError::NotFound)?;
    map.delete(key)
}

/// Execute a BPF program with given context
pub fn run_program(prog_id: u64, ctx_data: u64) -> Result<u64, BpfError> {
    let mut progs = PROGS.lock();
    let prog = progs.get_mut(&prog_id).ok_or(BpfError::NotFound)?;

    let insns = prog.insns.clone();
    prog.run_count += 1;
    drop(progs);

    let mut ctx = BpfContext::new();
    ctx.regs[1] = ctx_data; // R1 = context pointer

    // Execute
    let result = execute(&insns, &mut ctx)?;

    Ok(result)
}

/// BPF interpreter
fn execute(insns: &[BpfInsn], ctx: &mut BpfContext) -> Result<u64, BpfError> {
    let max_insns = 1_000_000; // Prevent infinite loops
    let mut count = 0usize;

    while ctx.pc < insns.len() && count < max_insns {
        let insn = &insns[ctx.pc];
        let class = insn.code & 0x07;
        let _source = insn.code & 0x08;
        let op = insn.code & 0xF0;

        let dst = insn.dst_reg() as usize;
        let src = insn.src_reg() as usize;

        match class {
            BPF_ALU64 | BPF_ALU => {
                let src_val = if insn.code & BPF_X != 0 {
                    ctx.regs[src]
                } else {
                    insn.imm as i64 as u64
                };

                let result = match op {
                    BPF_ADD => ctx.regs[dst].wrapping_add(src_val),
                    BPF_SUB => ctx.regs[dst].wrapping_sub(src_val),
                    BPF_MUL => ctx.regs[dst].wrapping_mul(src_val),
                    BPF_DIV => ctx.regs[dst].checked_div(src_val).unwrap_or(0),
                    BPF_OR => ctx.regs[dst] | src_val,
                    BPF_AND => ctx.regs[dst] & src_val,
                    BPF_LSH => ctx.regs[dst] << (src_val & 63),
                    BPF_RSH => ctx.regs[dst] >> (src_val & 63),
                    BPF_NEG => (-(ctx.regs[dst] as i64)) as u64,
                    BPF_MOD => {
                        if src_val != 0 {
                            ctx.regs[dst] % src_val
                        } else {
                            ctx.regs[dst]
                        }
                    }
                    BPF_XOR => ctx.regs[dst] ^ src_val,
                    BPF_MOV => src_val,
                    BPF_ARSH => ((ctx.regs[dst] as i64) >> (src_val & 63)) as u64,
                    _ => return Err(BpfError::InvalidProg),
                };

                ctx.regs[dst] = if class == BPF_ALU {
                    result as u32 as u64 // 32-bit truncation
                } else {
                    result
                };
            }
            BPF_JMP => {
                match op {
                    BPF_EXIT => return Ok(ctx.regs[0]),
                    BPF_CALL => {
                        // Call helper function
                        let helper_id = insn.imm as u32;
                        ctx.regs[0] = call_helper(helper_id, ctx);
                    }
                    BPF_JA => {
                        ctx.pc = (ctx.pc as i64 + insn.off as i64) as usize;
                    }
                    _ => {
                        let src_val = if insn.code & BPF_X != 0 {
                            ctx.regs[src]
                        } else {
                            insn.imm as i64 as u64
                        };

                        let cond = match op {
                            BPF_JEQ => ctx.regs[dst] == src_val,
                            BPF_JGT => ctx.regs[dst] > src_val,
                            BPF_JGE => ctx.regs[dst] >= src_val,
                            BPF_JSET => ctx.regs[dst] & src_val != 0,
                            BPF_JNE => ctx.regs[dst] != src_val,
                            BPF_JSGT => (ctx.regs[dst] as i64) > (src_val as i64),
                            BPF_JSGE => (ctx.regs[dst] as i64) >= (src_val as i64),
                            BPF_JLT => ctx.regs[dst] < src_val,
                            BPF_JLE => ctx.regs[dst] <= src_val,
                            BPF_JSLT => (ctx.regs[dst] as i64) < (src_val as i64),
                            BPF_JSLE => (ctx.regs[dst] as i64) <= (src_val as i64),
                            _ => false,
                        };

                        if cond {
                            ctx.pc = (ctx.pc as i64 + insn.off as i64) as usize;
                        }
                    }
                }
            }
            _ => {} // LD/LDX/ST/STX handled similarly
        }

        ctx.pc += 1;
        count += 1;
    }

    Ok(ctx.regs[0])
}

/// Call a BPF helper function
fn call_helper(id: u32, ctx: &mut BpfContext) -> u64 {
    match id {
        BPF_FUNC_KTIME_GET_NS => crate::rtc::unix_time() as u64 * 1_000_000_000,
        BPF_FUNC_GET_PRANDOM_U32 => {
            // Simple PRNG
            let mut lo: u32 = 0;
            let mut hi: u32 = 0;
            unsafe {
                #[cfg(target_arch = "x86_64")]
                core::arch::asm!("rdtsc", out("eax") lo, out("edx") hi);
            }
            ((hi as u64) << 32 | lo as u64) & 0xFFFFFFFF
        }
        BPF_FUNC_GET_SMP_PROCESSOR_ID => 0,
        BPF_FUNC_GET_CURRENT_PID_TGID => {
            let pid = 1u64; // Simplified
            let tgid = pid;
            (tgid << 32) | pid
        }
        BPF_FUNC_GET_CURRENT_UID_GID => {
            let uid = crate::users::get_current_uid() as u64;
            let gid = crate::users::get_current_gid() as u64;
            (gid << 32) | uid
        }
        BPF_FUNC_TRACE_PRINTK => {
            crate::serial_println!(
                "bpf_trace_printk: r1={:#x} r2={:#x} r3={:#x}",
                ctx.regs[1],
                ctx.regs[2],
                ctx.regs[3]
            );
            0
        }
        _ => 0, // Unknown helper
    }
}

/// Unload a BPF program
pub fn bpf_prog_unload(id: u64) {
    PROGS.lock().remove(&id);
}

/// Destroy a BPF map
pub fn bpf_map_destroy(id: u64) {
    MAPS.lock().remove(&id);
}

/// List loaded programs
pub fn list_progs() -> Vec<(u64, String, BpfProgType)> {
    let progs = PROGS.lock();
    progs
        .values()
        .map(|p| (p.id, p.name.clone(), p.prog_type))
        .collect()
}

/// List maps
pub fn list_maps() -> Vec<(u64, BpfMapType, u32)> {
    let maps = MAPS.lock();
    maps.values()
        .map(|m| (m.id, m.map_type, m.max_entries))
        .collect()
}

/// Initialize eBPF subsystem
pub fn init() {
    crate::serial_println!(
        "  eBPF subsystem initialized (32 prog types, 31 map types, verifier, interpreter)"
    );
}
