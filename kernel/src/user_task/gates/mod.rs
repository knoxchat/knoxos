/// Scheduled Ring 3 gate demonstrations and the start-menu launcher.
mod display;
mod enosys;
mod fs;
mod identity;
mod io;
pub mod markers;
mod net;
mod process;

pub use markers::*;

use crate::context::DESKTOP_PID;
use crate::process::Pid;
use crate::serial_println;
use crate::user_task::spawn::{SpawnOutcome, spawn_elf};

use display::*;
use enosys::*;
use fs::*;
use identity::*;
use io::*;
use markers::*;
use net::*;
use process::*;

/// Switch to `pid` and keep running user tasks until they all block or exit.
///
/// A single `switch_to` is not enough: `wait4` parks the parent and
/// `resume_next_or_return` may hand the CPU back to the desktop *before*
/// the child has run. Drain the user run queue so Gate B4's parent actually
/// finishes (and can be reaped) before the next demo starts.
pub(super) unsafe fn run_until_desktop(pid: Pid) {
    crate::context::switch_to(pid);
    loop {
        let next = {
            let mut sched = crate::scheduler::SCHEDULER.lock();
            sched.clear_current();
            sched.schedule()
        };
        match next {
            Some(n)
                if n > DESKTOP_PID
                    && !crate::context::is_ap_idle_pid(n)
                    && crate::context::has_runnable_context(n) =>
            {
                crate::context::switch_to(n);
            }
            _ => break,
        }
    }
}

pub(super) fn spawn_or_log(elf: &[u8], name: &str) -> Option<Pid> {
    match spawn_elf(elf, name, &[name], &[], None) {
        SpawnOutcome::Spawned(pid) => Some(pid),
        SpawnOutcome::NoElf => {
            serial_println!("[user_task] {}: ELF invalid", name);
            None
        }
        SpawnOutcome::NoMemory(what) => {
            serial_println!("[user_task] {}: no memory for {}", name, what);
            None
        }
    }
}

pub(super) fn reap_child(pid: Pid) -> bool {
    let reaped = crate::process::PROCESS_TABLE.lock().waitpid(pid);
    if reaped.is_some() {
        crate::signals::destroy_process_signals(pid);
        crate::context::destroy_process_context(pid);
        return true;
    }
    crate::process::PROCESS_TABLE
        .lock()
        .get_process(pid)
        .is_none()
}

/// Run the scheduled-userspace demonstrations; returns when they complete.
pub fn run_gate_demos() {
    if !crate::vmm::ready() {
        serial_println!("[user_task] Gate B3+ skipped: VMM not ready");
        return;
    }
    serial_println!(
        "[user_task] ── Gate B3–B8 + D1 + F1–F4 + J1 + J4 + K1 + K3 + L1 + L4 + M2–M4 + N2–N4 + O2–O4 + P2–P4: scheduled Ring 3 ──"
    );
    unsafe {
        crate::context::run_in_desktop_context(gate_boot_body);
    }
    serial_println!(
        "[user_task] ── Gate B3–B8 + D1 + F1–F4 + J1 + J4 + K1 + K3 + L1 + L4 + M2–M4 + N2–N4 + O2–O4 + P2–P4 + Q2–Q4 + R2–R4 + S2–S4 + T2–T4 + U2–U4 + V2–V4 + W2–W4 + X2–X4 + Y2–Y4 + Z2–Z4: done ──"
    );
}

extern "C" fn gate_boot_body() {
    run_gate_b3();
    run_gate_b4();
    run_gate_b5();
    run_gate_b6();
    run_gate_d1();
    run_gate_b7();
    run_gate_b8();
    run_gate_i3();
    run_gate_f1();
    run_gate_f3();
    run_gate_f4();
    run_gate_j1();
    run_gate_j4();
    run_gate_k1();
    run_gate_k3();
    run_gate_l1();
    run_gate_l4();
    run_gate_m2();
    run_gate_m3();
    run_gate_m4();
    run_gate_n2();
    run_gate_n3();
    run_gate_n4();
    run_gate_o2();
    run_gate_o3();
    run_gate_o4();
    run_gate_p2();
    run_gate_p3();
    run_gate_p4();
    run_gate_q2();
    run_gate_q3();
    run_gate_q4();
    run_gate_r2();
    run_gate_r3();
    run_gate_r4();
    run_gate_s2();
    run_gate_s3();
    run_gate_s4();
    run_gate_t2();
    run_gate_t3();
    run_gate_t4();
    run_gate_u2();
    run_gate_u3();
    run_gate_u4();
    run_gate_v2();
    run_gate_v3();
    run_gate_v4();
    run_gate_w2();
    run_gate_w3();
    run_gate_w4();
    run_gate_x2();
    run_gate_x3();
    run_gate_x4();
    run_gate_y2();
    run_gate_y3();
    run_gate_y4();
    run_gate_z2();
    run_gate_z3();
    run_gate_z4();
    run_gate_aa2();
    run_gate_aa3();
    run_gate_aa4();
    run_gate_ab2();
    run_gate_ab3();
    run_gate_ab4();
    run_gate_ac2();
    run_gate_ac3();
    run_gate_ac4();
    run_gate_ad2();
    run_gate_ad3();
    run_gate_ad4();
    run_gate_ae2();
    run_gate_ae3();
    run_gate_ae4();
    run_gate_af2();
    run_gate_af3();
    run_gate_af4();
    run_gate_ag2();
    run_gate_ag3();
    run_gate_ag4();
    run_gate_ah2();
    run_gate_ah3();
    run_gate_ah4();
    run_gate_ai2();
    run_gate_ai3();
    run_gate_ai4();
    run_gate_aj2();
    run_gate_aj3();
    run_gate_aj4();
    run_gate_ak2();
    run_gate_ak3();
    run_gate_ak4();
    run_gate_al2();
    run_gate_al3();
    run_gate_al4();
    run_gate_am2();
    run_gate_am3();
    run_gate_am4();
    run_gate_an2();
    run_gate_an3();
    run_gate_an4();
    run_gate_ao2();
    run_gate_ao3();
    run_gate_ao4();
    run_gate_ap2();
    run_gate_ap3();
    run_gate_ap4();
    run_gate_aq2();
    run_gate_aq3();
    run_gate_aq4();
    run_gate_ar2();
    run_gate_ar3();
    run_gate_ar4();
    run_gate_as2();
    run_gate_as3();
    run_gate_as4();
    run_gate_at2();
    run_gate_at3();
    run_gate_at4();
    run_gate_au2();
    run_gate_au3();
    run_gate_au4();
    run_gate_av2();
    run_gate_av3();
    run_gate_av4();
    run_gate_aw2();
    run_gate_aw3();
    run_gate_aw4();
    run_gate_ax2();
    run_gate_ax3();
    run_gate_ax4();
    run_gate_ay2();
    run_gate_ay3();
    run_gate_ay4();
    run_gate_az2();
    run_gate_az3();
    run_gate_az4();
    run_gate_ba2();
    run_gate_ba3();
    run_gate_ba4();
    run_gate_bb2();
    run_gate_bb3();
    run_gate_bb4();
}

/// Spawn a Ring 3 SHM launcher without waiting (start-menu clicks).
pub fn spawn_launcher_app(name: &str) {
    let elf = crate::init::launcher_client_elf_data();
    let _ = spawn_or_log(&elf, name);
}
