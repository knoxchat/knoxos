use crate::serial_println;
/// prctl, arch_prctl, personality
use crate::syscall::{SyscallError, SyscallResult, read_user_string};

// ── prctl ───────────────────────────────────────────────────────────

pub fn sys_prctl(option: i32, arg2: u64, arg3: u64, arg4: u64, arg5: u64) -> SyscallResult {
    let pid = crate::scheduler::current_pid().unwrap_or(1);
    match option {
        1 => {
            // PR_SET_PDEATHSIG
            Ok(0)
        }
        2 => {
            // PR_GET_PDEATHSIG
            if arg2 != 0 {
                unsafe {
                    *(arg2 as *mut i32) = 0;
                }
            }
            Ok(0)
        }
        4 => {
            // PR_GET_UNALIGN
            Ok(0)
        }
        6 => {
            // PR_GET_FPEMU
            Ok(0)
        }
        9 => {
            // PR_GET_KEEPCAPS
            Ok(0)
        }
        15 => {
            // PR_SET_NAME — set process/thread name
            let name = unsafe { read_user_string(arg2) }.unwrap_or_default();
            let mut table = crate::process::PROCESS_TABLE.lock();
            if let Some(proc) = table.get_process_mut(pid) {
                let truncated = if name.len() > 15 { &name[..15] } else { &name };
                proc.name = alloc::string::String::from(truncated);
            }
            Ok(0)
        }
        16 => {
            // PR_GET_NAME — get process/thread name
            let table = crate::process::PROCESS_TABLE.lock();
            if let Some(proc) = table.get_process(pid) {
                if arg2 != 0 {
                    let bytes = proc.name.as_bytes();
                    let len = bytes.len().min(16);
                    unsafe {
                        core::ptr::copy_nonoverlapping(bytes.as_ptr(), arg2 as *mut u8, len);
                        *((arg2 as usize + len) as *mut u8) = 0;
                    }
                }
            }
            Ok(0)
        }
        22 => {
            // PR_SET_SECCOMP
            if arg2 == 1 {
                crate::seccomp::seccomp_set_mode_strict(pid)
                    .map(|_| 0u64)
                    .map_err(|_| SyscallError::InvalidArgument)
            } else {
                Ok(0)
            }
        }
        28 => {
            // PR_SET_NO_NEW_PRIVS
            Ok(0) // Accept and ignore
        }
        35 => {
            // PR_GET_NO_NEW_PRIVS
            Ok(0)
        }
        36 => {
            // PR_GET_THP_DISABLE
            Ok(0)
        }
        38 => {
            // PR_SET_CHILD_SUBREAPER
            Ok(0)
        }
        40 | 41 => {
            // PR_CAP_AMBIENT
            Ok(0)
        }
        _ => {
            serial_println!("[KnoxOS] prctl: unhandled option {}", option);
            let _ = (arg3, arg4, arg5);
            Ok(0)
        }
    }
}

// ── arch_prctl ──────────────────────────────────────────────────────

pub fn sys_arch_prctl(code: i32, addr: u64) -> SyscallResult {
    match code {
        0x1001 => {
            // ARCH_SET_GS
            crate::arch_compat::registers::model_specific::GsBase::write(
                crate::arch_compat::structures::paging::VirtAddr::new(addr),
            );
            Ok(0)
        }
        0x1002 => {
            // ARCH_SET_FS
            crate::usermode::program_fs_base(addr);
            if let Some(pid) = crate::scheduler::current_pid() {
                crate::context::set_user_fs_base(pid, addr);
            }
            Ok(0)
        }
        0x1003 => {
            // ARCH_GET_FS
            let fs = crate::arch_compat::registers::model_specific::FsBase::read();
            if addr != 0 {
                unsafe {
                    *(addr as *mut u64) = fs.as_u64();
                }
            }
            Ok(fs.as_u64())
        }
        0x1004 => {
            // ARCH_GET_GS
            let gs = crate::arch_compat::registers::model_specific::GsBase::read();
            if addr != 0 {
                unsafe {
                    *(addr as *mut u64) = gs.as_u64();
                }
            }
            Ok(gs.as_u64())
        }
        _ => Err(SyscallError::InvalidArgument),
    }
}

// ── personality ─────────────────────────────────────────────────────

pub fn sys_personality(persona: u64) -> SyscallResult {
    if persona == 0xFFFFFFFF {
        // Query current personality
        Ok(0) // PER_LINUX
    } else {
        Ok(0) // Accept, return old personality
    }
}
