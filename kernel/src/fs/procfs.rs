/// ProcFS - /proc pseudo-filesystem implementation
/// Provides process information and kernel tunables via filesystem interface
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::process::{PROCESS_TABLE, Pid};
use crate::serial_println;

/// Read a /proc file and return its dynamic content
pub fn read_proc_file(path: &str) -> Option<Vec<u8>> {
    let path = path.trim_start_matches("/proc");
    let path = path.trim_start_matches('/');

    if path.is_empty() {
        return Some(b"proc filesystem\n".to_vec());
    }

    // Split into components
    let parts: Vec<&str> = path.split('/').collect();

    match parts[0] {
        // /proc/version
        "version" => Some(
            format!(
                "KnoxOS version 0.2.6 (rustc {}) #1 SMP PREEMPT_DYNAMIC\n",
                "nightly-2025"
            )
            .into_bytes(),
        ),

        // /proc/uptime
        "uptime" => {
            let ticks = crate::interrupts::get_ticks();
            let seconds = ticks as f64 / 18.2; // PIT frequency
            Some(format!("{:.2} {:.2}\n", seconds, seconds).into_bytes())
        }

        // /proc/meminfo
        "meminfo" => {
            let heap_size_kb = crate::allocator::HEAP_SIZE / 1024;
            let total_kb = {
                let detected = crate::multiboot::total_memory() / 1024;
                if detected > 0 {
                    detected as usize
                } else {
                    heap_size_kb
                }
            };
            Some(
                format!(
                    "MemTotal:       {} kB\n\
                 MemFree:        {} kB\n\
                 MemAvailable:   {} kB\n\
                 Buffers:        0 kB\n\
                 Cached:         0 kB\n\
                 SwapTotal:      0 kB\n\
                 SwapFree:       0 kB\n\
                 Shmem:          0 kB\n\
                 KernelStack:    32 kB\n\
                 PageTables:     0 kB\n\
                 VmallocTotal:   {} kB\n",
                    total_kb,
                    total_kb / 2, // Approximate
                    total_kb / 2,
                    total_kb,
                )
                .into_bytes(),
            )
        }

        // /proc/cpuinfo
        "cpuinfo" => {
            let cpuid = crate::arch_compat::raw_cpuid::CpuId::new();
            let vendor = cpuid
                .get_vendor_info()
                .map(|v| String::from(v.as_str()))
                .unwrap_or_else(|| String::from("Unknown"));
            let brand = cpuid
                .get_processor_brand_string()
                .map(|b| String::from(b.as_str()))
                .unwrap_or_else(|| String::from("Unknown CPU"));

            let mut flags = String::new();
            if let Some(features) = cpuid.get_feature_info() {
                if features.has_sse() {
                    flags.push_str("sse ");
                }
                if features.has_sse2() {
                    flags.push_str("sse2 ");
                }
                if features.has_fpu() {
                    flags.push_str("fpu ");
                }
                if features.has_apic() {
                    flags.push_str("apic ");
                }
                if features.has_pse() {
                    flags.push_str("pse ");
                }
                if features.has_msr() {
                    flags.push_str("msr ");
                }
                if features.has_pae() {
                    flags.push_str("pae ");
                }
            }

            Some(
                format!(
                    "processor\t: 0\n\
                 vendor_id\t: {}\n\
                 model name\t: {}\n\
                 cpu MHz\t\t: 0.000\n\
                 cache size\t: 0 KB\n\
                 physical id\t: 0\n\
                 siblings\t: 1\n\
                 core id\t\t: 0\n\
                 cpu cores\t: 1\n\
                 bogomips\t: 0.00\n\
                 flags\t\t: {}\n\
                 address sizes\t: 48 bits physical, 48 bits virtual\n",
                    vendor,
                    brand,
                    flags.trim()
                )
                .into_bytes(),
            )
        }

        // /proc/stat
        "stat" => {
            let ticks = crate::interrupts::get_ticks();
            Some(
                format!(
                    "cpu  {} 0 {} 0 0 0 0 0 0 0\n\
                 cpu0 {} 0 {} 0 0 0 0 0 0 0\n\
                 intr {}\n\
                 ctxt 0\n\
                 btime 0\n\
                 processes 3\n\
                 procs_running 1\n\
                 procs_blocked 0\n",
                    ticks / 2,
                    ticks / 2,
                    ticks / 2,
                    ticks / 2,
                    ticks
                )
                .into_bytes(),
            )
        }

        // /proc/filesystems
        "filesystems" => Some(
            b"nodev\tproc\n\
                   nodev\tsysfs\n\
                   nodev\tdevtmpfs\n\
                   nodev\ttmpfs\n\
                   \tknoxosfs\n"
                .to_vec(),
        ),

        // /proc/mounts (or /proc/self/mounts)
        "mounts" => Some(
            b"knoxosfs / knoxosfs rw,relatime 0 0\n\
                   proc /proc proc rw,nosuid,nodev,noexec 0 0\n\
                   sysfs /sys sysfs rw,nosuid,nodev,noexec 0 0\n\
                   devtmpfs /dev devtmpfs rw,nosuid 0 0\n\
                   tmpfs /tmp tmpfs rw,nosuid,nodev 0 0\n"
                .to_vec(),
        ),

        // /proc/loadavg
        "loadavg" => {
            let table = PROCESS_TABLE.lock();
            let nr_running = table.count();
            Some(format!("0.00 0.00 0.00 1/{} 3\n", nr_running).into_bytes())
        }

        // /proc/cmdline
        "cmdline" => Some(b"knoxos root=/dev/ram0 console=ttyS0\n".to_vec()),

        // /proc/hostname
        "hostname" | "sys/kernel/hostname" => Some(b"knoxos\n".to_vec()),

        // /proc/sys/kernel/... — Chromium sandbox probes
        "sys" => match parts.get(1..).map(|p| p.join("/")).as_deref() {
            Some("kernel/unprivileged_userns_clone") => Some(b"1\n".to_vec()),
            Some("kernel/core_pattern") => Some(b"core.%p\n".to_vec()),
            Some("kernel/yama/ptrace_scope") => Some(b"1\n".to_vec()),
            Some("kernel/hostname") => Some(b"knoxos\n".to_vec()),
            Some("kernel/ostype") => Some(b"Linux\n".to_vec()),
            Some("kernel/osrelease") => Some(b"6.1.0-knoxos\n".to_vec()),
            _ => None,
        },

        // /proc/self - symlink to current process
        "self" => {
            if parts.len() > 1 {
                // /proc/self/xxx -> /proc/<current_pid>/xxx
                let current_pid = crate::scheduler::current_pid().unwrap_or(1);
                let sub_path = parts[1..].join("/");
                read_proc_pid(current_pid, &sub_path)
            } else {
                Some(b"self\n".to_vec())
            }
        }

        // /proc/<pid>/... - per-process info
        pid_str => {
            if let Ok(pid) = pid_str.parse::<Pid>() {
                if parts.len() > 1 {
                    read_proc_pid(pid, &parts[1..].join("/"))
                } else {
                    // /proc/<pid> - list available entries
                    Some(b"cmdline\nstatus\nstat\nmaps\nfd\ncwd\nexe\nenviron\n".to_vec())
                }
            } else {
                None
            }
        }
    }
}

/// Read per-process /proc/<pid>/xxx files
fn read_proc_pid(pid: Pid, entry: &str) -> Option<Vec<u8>> {
    let table = PROCESS_TABLE.lock();
    let process = table.get_process(pid);
    let name = process
        .map(|p| p.name.clone())
        .unwrap_or_else(|| String::from("chrome"));
    let state = process
        .map(|p| format!("{:?}", p.state))
        .unwrap_or_else(|| String::from("R"));
    let ppid = process.map(|p| p.ppid).unwrap_or(0);
    let uid = process.map(|p| p.uid).unwrap_or(0);
    let gid = process.map(|p| p.gid).unwrap_or(0);
    let cwd = process
        .map(|p| p.cwd.clone())
        .unwrap_or_else(|| String::from("/"));
    let priority = process.map(|p| p.priority).unwrap_or(0);

    match entry {
        // /proc/<pid>/status
        "status" => {
            Some(format!(
                "Name:\t{}\n\
                 Umask:\t0022\n\
                 State:\t{}\n\
                 Tgid:\t{}\n\
                 Pid:\t{}\n\
                 PPid:\t{}\n\
                 Uid:\t{}\t{}\t{}\t{}\n\
                 Gid:\t{}\t{}\t{}\t{}\n\
                 Threads:\t1\n\
                 VmSize:\t0 kB\n\
                 VmRSS:\t0 kB\n",
                name,
                state,
                pid, pid,
                ppid,
                uid, uid, uid, uid,
                gid, gid, gid, gid,
            ).into_bytes())
        }

        // /proc/<pid>/stat (single line, like Linux)
        "stat" => {
            Some(format!(
                "{} ({}) {} {} {} 0 0 0 0 0 0 0 0 0 0 {} 0 1 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0\n",
                pid, name, state, ppid, pid, priority
            ).into_bytes())
        }

        // /proc/<pid>/cmdline
        "cmdline" => {
            Some(name.as_bytes().to_vec())
        }

        // /proc/<pid>/cwd
        "cwd" => {
            Some(format!("{}\n", cwd).into_bytes())
        }

        // /proc/<pid>/environ
        "environ" => {
            Some("HOME=/home/user\x00PATH=/bin:/usr/bin\x00USER=user\x00SHELL=/bin/sh\x00TERM=linux\x00".as_bytes().to_vec())
        }

        // /proc/<pid>/maps (memory map - placeholder)
        "maps" => {
            Some(format!(
                "00400000-00401000 r-xp 00000000 00:00 0  {}\n\
                 7fff00000000-7fff00001000 rw-p 00000000 00:00 0  [stack]\n\
                 7fff40000000-7fff40001000 r--p 00000000 00:00 0  [vvar]\n\
                 7fff40001000-7fff40002000 r-xp 00000000 00:00 0  [vdso]\n",
                name
            ).into_bytes())
        }

        // /proc/<pid>/fd - list open fds
        "fd" => {
            let fds = crate::fd::PROCESS_FD_TABLES.lock();
            if let Some(fd_table) = fds.get(&pid) {
                let fd_list: Vec<String> = fd_table.list_fds().iter()
                    .map(|fd| format!("{}", fd))
                    .collect();
                Some(fd_list.join("\n").into_bytes())
            } else {
                Some(b"0\n1\n2\n".to_vec())
            }
        }

        // /proc/<pid>/exe
        "exe" => {
            Some(format!("/bin/{}\n", name).into_bytes())
        }

        "smaps" => Some(
            format!(
                "00400000-00401000 r-xp 00000000 00:00 0  {}\n\
                 Size:                  4 kB\n\
                 Rss:                   4 kB\n\
                 Pss:                   4 kB\n\
                 Shared_Clean:          0 kB\n\
                 Private_Dirty:         4 kB\n",
                name
            )
            .into_bytes(),
        ),

        "statm" => Some(b"1 1 0 1 0 1 0\n".to_vec()),

        "oom_score_adj" => Some(b"0\n".to_vec()),

        "cgroup" => Some(b"0::/\n".to_vec()),

        "mountinfo" => Some(
            b"1 0 0:1 / / rw - knoxosfs knoxosfs rw\n\
              2 1 0:2 / /proc rw - proc proc rw\n\
              3 1 0:3 / /sys rw - sysfs sysfs rw\n\
              4 1 0:4 / /dev rw - devtmpfs devtmpfs rw\n"
                .to_vec(),
        ),

        "limits" => Some(
            b"Limit                     Soft Limit           Hard Limit           Units     \n\
              Max cpu time              unlimited            unlimited            seconds   \n\
              Max file size             unlimited            unlimited            bytes     \n\
              Max data size             unlimited            unlimited            bytes     \n\
              Max stack size            8388608              unlimited            bytes     \n\
              Max core file size        0                    unlimited            bytes     \n\
              Max resident set          unlimited            unlimited            bytes     \n\
              Max processes             256                  256                  processes \n\
              Max open files            1024                 1024                 files     \n\
              Max locked memory         65536                65536                bytes     \n\
              Max address space         unlimited            unlimited            bytes     \n\
              Max file locks            unlimited            unlimited            locks     \n\
              Max pending signals       256                  256                  signals   \n\
              Max msgqueue size         819200               819200               bytes     \n\
              Max nice priority         0                    0                    \n\
              Max realtime priority     0                    0                    \n\
              Max realtime timeout      unlimited            unlimited            us        \n"
                .to_vec(),
        ),

        "ns" => Some(b"cgroup\nipc\nmnt\nnet\npid\nuser\nuts\n".to_vec()),

        _ => None,
    }
}

/// Read a /sys file
pub fn read_sys_file(path: &str) -> Option<Vec<u8>> {
    let path = path.trim_start_matches("/sys");
    let path = path.trim_start_matches('/');

    match path {
        "" => {
            Some(b"block\nbus\nclass\ndev\ndevices\nfirmware\nfs\nkernel\nmodule\npower\n".to_vec())
        }

        "kernel/hostname" => Some(b"knoxos\n".to_vec()),
        "kernel/ostype" => Some(b"KnoxOS\n".to_vec()),
        "kernel/osrelease" => Some(b"0.2.6\n".to_vec()),
        "kernel/version" => Some(b"#1 SMP PREEMPT_DYNAMIC\n".to_vec()),

        "devices/system/cpu/online" => Some(b"0\n".to_vec()),
        "devices/system/cpu/possible" => Some(b"0\n".to_vec()),

        "class/graphics/fb0/virtual_size" => {
            let (w, h) = crate::gui::screen_size();
            Some(format!("{},{}\n", w, h).into_bytes())
        }
        "class/graphics/fb0/bits_per_pixel" => Some(b"32\n".to_vec()),

        "fs/knoxosfs/features" => Some(b"in_memory\n".to_vec()),

        _ => None,
    }
}

/// List /proc directory entries
pub fn list_proc() -> Vec<String> {
    let mut entries = alloc::vec![
        String::from("version"),
        String::from("uptime"),
        String::from("meminfo"),
        String::from("cpuinfo"),
        String::from("stat"),
        String::from("filesystems"),
        String::from("mounts"),
        String::from("loadavg"),
        String::from("cmdline"),
        String::from("self"),
    ];

    // Add per-process directories
    let table = PROCESS_TABLE.lock();
    for pid in table.list_pids() {
        entries.push(format!("{}", pid));
    }

    entries
}

/// Initialize procfs
pub fn init() {
    serial_println!("[KnoxOS] /proc filesystem initialized");
    serial_println!("[KnoxOS] /sys filesystem initialized");
}
