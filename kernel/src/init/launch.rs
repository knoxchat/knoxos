use crate::process::Pid;
use crate::serial_println;
use alloc::vec::Vec;

use super::elf::hello_userspace_elf_data;

/// Paths to try for init in order
const INIT_PATHS: &[&str] = &["/init", "/sbin/init", "/bin/init", "/etc/init", "/bin/sh"];

/// Built-in minimal /init program (x86_64 machine code)
/// This is a tiny statically-linked ELF that:
///   1. Writes "KnoxOS init (PID 1) started\n" to stdout (syscall write)
///   2. Loops on wait4() so PID 1 never exits
///
/// Loaded at the Gate B2 high vaddr so it does not share L4[0] with kernel
/// identity maps (0x401000 is present-but-NX in cloned kernel tables).
fn builtin_init_elf() -> Vec<u8> {
    builtin_init_elf_data()
}

/// Public entry point for other modules to get the init ELF data.
/// Instruction bytes: `userspace/apps/init/`.
pub fn builtin_init_elf_data() -> Vec<u8> {
    include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../userspace/apps/init/init.elf"
    ))
    .to_vec()
}

fn launch_hello_userspace() -> Result<(), &'static str> {
    let elf = hello_userspace_elf_data();
    if !crate::elf::is_elf(&elf) {
        return Err("hello ELF is invalid");
    }
    serial_println!(
        "[init] Mapping static hello ELF ({} bytes) into current page tables",
        elf.len()
    );
    let (entry, rsp) = crate::vmm::map_static_elf_into_current(&elf)?;
    serial_println!("[init] hello mapped: entry={:#x} rsp={:#x}", entry, rsp);
    // SAFETY: kernel-mode exclusive access; operands are owned, mapped, or architecture-defined for this path.
    unsafe {
        crate::usermode::run_userspace_once(entry, rsp);
    }
    Ok(())
}

/// Try to load /init from the filesystem or initramfs
fn find_init_binary() -> Option<Vec<u8>> {
    // Try initramfs (cpio) first
    for path in INIT_PATHS {
        if let Some(data) = crate::cpio::read_file(path) {
            serial_println!("[init] Found {} in initramfs ({} bytes)", path, data.len());
            return Some(data);
        }
    }

    // Try VFS
    for path in INIT_PATHS {
        let vfs = crate::vfs::VFS.lock();
        if let Some(ino) = vfs.resolve_path(path) {
            if let Some(inode) = vfs.get_inode(ino) {
                if !inode.data.is_empty() && crate::elf::is_elf(&inode.data) {
                    serial_println!("[init] Found {} in VFS ({} bytes)", path, inode.data.len());
                    return Some(inode.data.clone());
                }
            }
        }
    }

    None
}

/// Start the first user-space program.
///
/// Gate B2: map a static hello ELF into the current page tables, `iretq` to
/// Ring 3, `sys_write` to serial, `sys_exit` back to the kernel.
/// Gate B3–B6: scheduled tasks with their own CR3 — `execve`+`waitpid`,
/// `fork`+child, SIGKILL / SIGSEGV / PTY SIGINT, then `/bin/sh` on a PTY.
/// Gate B7: custom SIGINT handler + live `rt_sigreturn`.
/// Gate B8: timer preempts a spinning Ring 3 program that never syscalls.
/// Gate D1: loopback `send`/`recv` from a Ring 3 UDP pair.
/// Gate F1/F2: Ring 3 SHM client presents a buffer the compositor scans out.
/// Gate K1: `clone(CLONE_THREAD)` + live `thread_join`.
/// Gate K3: `/sbin/init` `iretq`s, writes its banner, and parks in `wait4`.
pub fn start_init() -> Option<Pid> {
    serial_println!("[init] Starting first userspace (Gate B2 hello)...");

    match launch_hello_userspace() {
        Ok(()) => {
            serial_println!("[init] Ring 3 hello returned to kernel");
        }
        Err(e) => {
            serial_println!("[init] Ring 3 hello failed: {}", e);
        }
    }

    crate::user_task::run_gate_demos();

    None
}

/// Initialize init process module
pub fn init() {
    serial_println!("[KnoxOS] Init process launcher ready");
}
