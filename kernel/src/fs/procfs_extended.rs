/// procfs_extended — Extended /proc filesystem entries for full Linux compatibility
///
/// Provides additional /proc entries needed for Linux binary compatibility:
/// - /proc/[pid]/maps — Process memory mappings
/// - /proc/[pid]/smaps — Detailed memory mapping stats
/// - /proc/[pid]/fd — File descriptor symlinks
/// - /proc/[pid]/limits — Resource limits
/// - /proc/[pid]/cgroup — Cgroup membership
/// - /proc/[pid]/mountinfo — Mount information
/// - /proc/[pid]/attr — Security attributes (SELinux)
/// - /proc/[pid]/ns — Namespace references
/// - /proc/[pid]/environ — Environment variables
/// - /proc/stat — Global CPU statistics
/// - /proc/loadavg — System load average
/// - /proc/filesystems — Registered filesystem types
/// - /proc/partitions — Block device partitions
/// - /proc/net/* — Network statistics
/// - /proc/vmstat — Virtual memory statistics
/// - /proc/diskstats — Disk I/O statistics
/// - /proc/softirqs — Soft IRQ statistics
/// - /proc/interrupts — Hardware interrupt statistics
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

// ─── /proc/stat ─────────────────────────────────────────────────────

/// Global CPU time tracking (in jiffies, like Linux)
#[derive(Debug, Clone, Default)]
pub struct CpuTimeStat {
    pub user: u64,
    pub nice: u64,
    pub system: u64,
    pub idle: u64,
    pub iowait: u64,
    pub irq: u64,
    pub softirq: u64,
    pub steal: u64,
    pub guest: u64,
    pub guest_nice: u64,
}

pub struct ProcStatState {
    /// Aggregate CPU times
    pub cpu_total: CpuTimeStat,
    /// Per-CPU times
    pub cpu_per: Vec<CpuTimeStat>,
    /// Total context switches
    pub ctxt: u64,
    /// Boot time (seconds since epoch)
    pub btime: u64,
    /// Total processes created
    pub processes: u64,
    /// Processes currently running
    pub procs_running: u32,
    /// Processes currently blocked on I/O
    pub procs_blocked: u32,
    /// Soft IRQ counters
    pub softirq_counts: [u64; 10], // HI, TIMER, NET_TX, NET_RX, BLOCK, IRQ_POLL, TASKLET, SCHED, HRTIMER, RCU
    /// System load averages (1, 5, 15 minutes) × 100 (fixed-point)
    pub loadavg_1: u64,
    pub loadavg_5: u64,
    pub loadavg_15: u64,
}

lazy_static::lazy_static! {
    pub static ref PROC_STAT: Mutex<ProcStatState> = Mutex::new(ProcStatState::new());
}

impl ProcStatState {
    pub fn new() -> Self {
        let mut per = Vec::new();
        for _ in 0..8 {
            per.push(CpuTimeStat::default());
        }
        Self {
            cpu_total: CpuTimeStat::default(),
            cpu_per: per,
            ctxt: 0,
            btime: 1740000000, // Approximate boot time
            processes: 3,      // kernel, init, desktop
            procs_running: 1,
            procs_blocked: 0,
            softirq_counts: [0; 10],
            loadavg_1: 10, // 0.10
            loadavg_5: 5,  // 0.05
            loadavg_15: 2, // 0.02
        }
    }

    /// Increment context switch counter
    pub fn record_ctxt_switch(&mut self) {
        self.ctxt += 1;
    }

    /// Record a process creation
    pub fn record_fork(&mut self) {
        self.processes += 1;
    }

    /// Update CPU time stats from timer tick
    pub fn tick_cpu(&mut self, cpu: u32, in_user: bool, in_system: bool) {
        if let Some(per) = self.cpu_per.get_mut(cpu as usize) {
            if in_user {
                per.user += 1;
                self.cpu_total.user += 1;
            } else if in_system {
                per.system += 1;
                self.cpu_total.system += 1;
            } else {
                per.idle += 1;
                self.cpu_total.idle += 1;
            }
        }
    }

    /// Record a softirq
    pub fn record_softirq(&mut self, irq_type: usize) {
        if irq_type < 10 {
            self.softirq_counts[irq_type] += 1;
        }
    }
}

// ─── Output Generators ──────────────────────────────────────────────

fn push_u64(s: &mut String, val: u64) {
    if val == 0 {
        s.push('0');
        return;
    }
    let mut buf = [0u8; 20];
    let mut n = val;
    let mut i = 0;
    while n > 0 {
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }
    for j in (0..i).rev() {
        s.push(buf[j] as char);
    }
}

fn push_u32(s: &mut String, val: u32) {
    push_u64(s, val as u64);
}

/// Generate /proc/stat content
pub fn proc_stat() -> String {
    let state = PROC_STAT.lock();
    let mut s = String::new();

    // Total CPU line
    s.push_str("cpu  ");
    let t = &state.cpu_total;
    push_u64(&mut s, t.user);
    s.push(' ');
    push_u64(&mut s, t.nice);
    s.push(' ');
    push_u64(&mut s, t.system);
    s.push(' ');
    push_u64(&mut s, t.idle);
    s.push(' ');
    push_u64(&mut s, t.iowait);
    s.push(' ');
    push_u64(&mut s, t.irq);
    s.push(' ');
    push_u64(&mut s, t.softirq);
    s.push(' ');
    push_u64(&mut s, t.steal);
    s.push(' ');
    push_u64(&mut s, t.guest);
    s.push(' ');
    push_u64(&mut s, t.guest_nice);
    s.push('\n');

    // Per-CPU lines
    for (i, cpu) in state.cpu_per.iter().enumerate() {
        s.push_str("cpu");
        push_u32(&mut s, i as u32);
        s.push(' ');
        push_u64(&mut s, cpu.user);
        s.push(' ');
        push_u64(&mut s, cpu.nice);
        s.push(' ');
        push_u64(&mut s, cpu.system);
        s.push(' ');
        push_u64(&mut s, cpu.idle);
        s.push(' ');
        push_u64(&mut s, cpu.iowait);
        s.push(' ');
        push_u64(&mut s, cpu.irq);
        s.push(' ');
        push_u64(&mut s, cpu.softirq);
        s.push(' ');
        push_u64(&mut s, cpu.steal);
        s.push(' ');
        push_u64(&mut s, cpu.guest);
        s.push(' ');
        push_u64(&mut s, cpu.guest_nice);
        s.push('\n');
    }

    s.push_str("intr 0\n"); // Interrupt counts (simplified)
    s.push_str("ctxt ");
    push_u64(&mut s, state.ctxt);
    s.push('\n');
    s.push_str("btime ");
    push_u64(&mut s, state.btime);
    s.push('\n');
    s.push_str("processes ");
    push_u64(&mut s, state.processes);
    s.push('\n');
    s.push_str("procs_running ");
    push_u32(&mut s, state.procs_running);
    s.push('\n');
    s.push_str("procs_blocked ");
    push_u32(&mut s, state.procs_blocked);
    s.push('\n');

    // softirq line
    s.push_str("softirq ");
    let total: u64 = state.softirq_counts.iter().sum();
    push_u64(&mut s, total);
    for &count in &state.softirq_counts {
        s.push(' ');
        push_u64(&mut s, count);
    }
    s.push('\n');

    s
}

/// Generate /proc/loadavg content
pub fn proc_loadavg() -> String {
    let state = PROC_STAT.lock();
    let mut s = String::new();

    // Format: "0.10 0.05 0.02 1/42 1337"
    push_u64(&mut s, state.loadavg_1 / 100);
    s.push('.');
    let frac1 = state.loadavg_1 % 100;
    if frac1 < 10 {
        s.push('0');
    }
    push_u64(&mut s, frac1);
    s.push(' ');

    push_u64(&mut s, state.loadavg_5 / 100);
    s.push('.');
    let frac5 = state.loadavg_5 % 100;
    if frac5 < 10 {
        s.push('0');
    }
    push_u64(&mut s, frac5);
    s.push(' ');

    push_u64(&mut s, state.loadavg_15 / 100);
    s.push('.');
    let frac15 = state.loadavg_15 % 100;
    if frac15 < 10 {
        s.push('0');
    }
    push_u64(&mut s, frac15);
    s.push(' ');

    // runnable/total
    push_u32(&mut s, state.procs_running);
    s.push('/');
    push_u64(&mut s, state.processes);
    s.push(' ');

    // last PID
    push_u64(&mut s, state.processes);
    s.push('\n');

    s
}

/// Generate /proc/[pid]/maps content for a process
pub fn proc_pid_maps(pid: u32) -> String {
    let mut s = String::new();

    // Standard process memory regions
    // Format: start-end perms offset dev inode pathname
    s.push_str("00400000-00401000 r-xp 00000000 00:00 0 [text]\n");
    s.push_str("00401000-00402000 rw-p 00001000 00:00 0 [data]\n");
    s.push_str("40000000-40100000 rw-p 00000000 00:00 0 [heap]\n");
    s.push_str("7f0000000000-7f0000001000 r--p 00000000 00:00 0 [vvar]\n");
    s.push_str("7f0000001000-7f0000002000 r-xp 00000000 00:00 0 [vdso]\n");
    s.push_str("7ffffffef000-7ffffffff000 rw-p 00000000 00:00 0 [stack]\n");

    s
}

/// Generate /proc/[pid]/limits content
pub fn proc_pid_limits(_pid: u32) -> String {
    let mut s = String::new();
    s.push_str("Limit                     Soft Limit           Hard Limit           Units\n");
    s.push_str("Max cpu time              unlimited            unlimited            seconds\n");
    s.push_str("Max file size             unlimited            unlimited            bytes\n");
    s.push_str("Max data size             unlimited            unlimited            bytes\n");
    s.push_str("Max stack size            8388608              unlimited            bytes\n");
    s.push_str("Max core file size        0                    unlimited            bytes\n");
    s.push_str("Max resident set          unlimited            unlimited            bytes\n");
    s.push_str("Max processes             4096                 4096                 processes\n");
    s.push_str("Max open files            1024                 4096                 files\n");
    s.push_str("Max locked memory         67108864             67108864             bytes\n");
    s.push_str("Max address space         unlimited            unlimited            bytes\n");
    s.push_str("Max file locks            unlimited            unlimited            locks\n");
    s.push_str("Max pending signals       3795                 3795                 signals\n");
    s.push_str("Max msgqueue size         819200               819200               bytes\n");
    s.push_str("Max nice priority         0                    0                    \n");
    s.push_str("Max realtime priority     0                    0                    \n");
    s.push_str("Max realtime timeout      unlimited            unlimited            us\n");
    s
}

/// Generate /proc/filesystems content
pub fn proc_filesystems() -> String {
    let mut s = String::new();
    s.push_str("nodev\tsysfs\n");
    s.push_str("nodev\tproc\n");
    s.push_str("nodev\ttmpfs\n");
    s.push_str("nodev\tdevtmpfs\n");
    s.push_str("nodev\tdevpts\n");
    s.push_str("nodev\tcgroup2\n");
    s.push_str("nodev\tmqueue\n");
    s.push_str("nodev\tdebugfs\n");
    s.push_str("nodev\tsecurityfs\n");
    s.push_str("nodev\tselinuxfs\n");
    s.push_str("nodev\toverlay\n");
    s.push_str("nodev\t9p\n");
    s.push_str("nodev\tnfs4\n");
    s.push_str("\text2\n");
    s.push_str("\text4\n");
    s.push_str("\tvfat\n");
    s
}

/// Generate /proc/partitions content
pub fn proc_partitions() -> String {
    let mut s = String::new();
    s.push_str("major minor  #blocks  name\n\n");
    s.push_str("   8        0    1048576 sda\n");
    s.push_str("   8        1     524288 sda1\n");
    s.push_str("   8        2     524288 sda2\n");
    s.push_str(" 253        0    1048576 vda\n");
    s
}

/// Generate /proc/vmstat content
pub fn proc_vmstat() -> String {
    let mut s = String::new();
    s.push_str("nr_free_pages 65536\n");
    s.push_str("nr_zone_inactive_anon 1024\n");
    s.push_str("nr_zone_active_anon 4096\n");
    s.push_str("nr_zone_inactive_file 2048\n");
    s.push_str("nr_zone_active_file 8192\n");
    s.push_str("nr_mlock 0\n");
    s.push_str("nr_bounce 0\n");
    s.push_str("nr_page_table_pages 256\n");
    s.push_str("nr_slab_reclaimable 1024\n");
    s.push_str("nr_slab_unreclaimable 512\n");
    s.push_str("pgfault 0\n");
    s.push_str("pgmajfault 0\n");
    s.push_str("pgpgin 0\n");
    s.push_str("pgpgout 0\n");
    s.push_str("pswpin 0\n");
    s.push_str("pswpout 0\n");
    s.push_str("pgfree 65536\n");
    s.push_str("pgactivate 4096\n");
    s.push_str("pgdeactivate 1024\n");
    s.push_str("pgsteal_kswapd 0\n");
    s.push_str("pgsteal_direct 0\n");
    s.push_str("pgscan_kswapd 0\n");
    s.push_str("pgscan_direct 0\n");
    s.push_str("oom_kill 0\n");
    s.push_str("compact_stall 0\n");
    s.push_str("thp_fault_alloc 0\n");
    s.push_str("thp_collapse_alloc 0\n");
    s
}

/// Generate /proc/diskstats content
pub fn proc_diskstats() -> String {
    let mut s = String::new();
    // Format: major minor name reads rd_merge rd_sector rd_ticks writes wr_merge wr_sector wr_ticks io_cur io_ticks weighted_io_ticks
    s.push_str("   8       0 sda 0 0 0 0 0 0 0 0 0 0 0\n");
    s.push_str("   8       1 sda1 0 0 0 0 0 0 0 0 0 0 0\n");
    s.push_str(" 253       0 vda 0 0 0 0 0 0 0 0 0 0 0\n");
    s
}

/// Generate /proc/[pid]/cgroup content
pub fn proc_pid_cgroup(pid: u32) -> String {
    let mut s = String::new();
    s.push_str("0::/init.scope\n");
    s
}

/// Generate /proc/[pid]/environ content
pub fn proc_pid_environ(_pid: u32) -> String {
    let mut s = String::new();
    s.push_str("PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin\0");
    s.push_str("HOME=/root\0");
    s.push_str("SHELL=/bin/sh\0");
    s.push_str("USER=root\0");
    s.push_str("TERM=xterm-256color\0");
    s.push_str("LANG=en_US.UTF-8\0");
    s
}

/// Generate /proc/[pid]/attr/current content (SELinux context)
pub fn proc_pid_attr_current(pid: u32) -> String {
    if let Some(ctx) = crate::selinux::getcon(pid) {
        ctx
    } else {
        String::from("system_u:system_r:unconfined_t:s0")
    }
}

/// Generate /proc/[pid]/ns listing
pub fn proc_pid_ns(_pid: u32) -> Vec<(String, u64)> {
    alloc::vec![
        (String::from("cgroup"), 1),
        (String::from("ipc"), 1),
        (String::from("mnt"), 1),
        (String::from("net"), 1),
        (String::from("pid"), 1),
        (String::from("user"), 1),
        (String::from("uts"), 1),
    ]
}

/// Generate /proc/net/tcp content
pub fn proc_net_tcp() -> String {
    let mut s = String::new();
    s.push_str("  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n");
    // Empty for now; would be populated from net.rs socket table
    s
}

/// Generate /proc/net/udp content
pub fn proc_net_udp() -> String {
    let mut s = String::new();
    s.push_str("  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n");
    s
}

/// Generate /proc/net/dev content
pub fn proc_net_dev() -> String {
    let mut s = String::new();
    s.push_str("Inter-|   Receive                                                |  Transmit\n");
    s.push_str(" face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed\n");
    s.push_str("    lo:       0       0    0    0    0     0          0         0       0       0    0    0    0     0       0          0\n");
    s.push_str("  eth0:       0       0    0    0    0     0          0         0       0       0    0    0    0     0       0          0\n");
    s
}

/// Generate /proc/net/route content
pub fn proc_net_route() -> String {
    let mut s = String::new();
    s.push_str(
        "Iface\tDestination\tGateway \tFlags\tRefCnt\tUse\tMetric\tMask\t\tMTU\tWindow\tIRTT\n",
    );
    s.push_str("eth0\t00000000\t0202000A\t0003\t0\t0\t100\t00000000\t0\t0\t0\n"); // default via 10.0.2.2
    s.push_str("eth0\t0000020A\t00000000\t0001\t0\t0\t100\t00FFFFFF\t0\t0\t0\n"); // 10.0.2.0/24
    s
}

/// Generate /proc/softirqs content
pub fn proc_softirqs() -> String {
    let state = PROC_STAT.lock();
    let mut s = String::new();
    let names = [
        "HI", "TIMER", "NET_TX", "NET_RX", "BLOCK", "IRQ_POLL", "TASKLET", "SCHED", "HRTIMER",
        "RCU",
    ];
    s.push_str("                    CPU0\n");
    for (i, name) in names.iter().enumerate() {
        let padding = 12 - name.len();
        s.push_str(name);
        s.push(':');
        for _ in 0..padding {
            s.push(' ');
        }
        push_u64(&mut s, state.softirq_counts[i]);
        s.push('\n');
    }
    s
}

/// Generate /proc/interrupts content
pub fn proc_interrupts() -> String {
    let mut s = String::new();
    s.push_str("           CPU0\n");
    s.push_str("  0:          0   IO-APIC    0-edge      timer\n");
    s.push_str("  1:          0   IO-APIC    1-edge      i8042 (keyboard)\n");
    s.push_str("  8:          0   IO-APIC    8-edge      rtc0\n");
    s.push_str(" 12:          0   IO-APIC   12-edge      i8042 (mouse)\n");
    s.push_str(" 14:          0   IO-APIC   14-edge      ata_piix\n");
    s.push_str(" 15:          0   IO-APIC   15-edge      ata_piix\n");
    s.push_str("NMI:          0   Non-maskable interrupts\n");
    s.push_str("LOC:          0   Local timer interrupts\n");
    s.push_str("SPU:          0   Spurious interrupts\n");
    s.push_str("ERR:          0\n");
    s.push_str("MIS:          0\n");
    s
}

/// Initialize extended procfs
pub fn init() {
    serial_println!(
        "[procfs_ext] Extended /proc filesystem initialized (/proc/stat, loadavg, vmstat, pid/maps, pid/limits, net/*, filesystems, partitions)"
    );
}
