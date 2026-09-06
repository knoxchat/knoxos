/// Kernel & system management builtins — dmesg, lsmod, modinfo, insmod, rmmod, lsblk, mount, umount, poweroff, reboot
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

use crate::shell::types::ShellResult;

pub fn dmesg(args: &[String]) -> ShellResult {
    let count = if args.len() >= 2 && args[0] == "-n" {
        args[1].parse::<usize>().unwrap_or(0)
    } else {
        0
    };
    let clear = args.iter().any(|a| a == "-c" || a == "--clear");

    let entries = if count > 0 {
        crate::syslog::dmesg_tail(count)
    } else {
        crate::syslog::dmesg()
    };

    let mut output = String::new();
    for entry in &entries {
        writeln!(output, "{}", crate::syslog::format_entry(entry)).unwrap();
    }

    if clear {
        crate::syslog::clear();
    }

    ShellResult::ok(&output)
}

pub fn lsmod() -> ShellResult {
    let info = crate::modules::format_modules();
    ShellResult::ok(&info)
}

pub fn modinfo(args: &[String]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::err("modinfo: missing module name");
    }
    match crate::modules::module_info(&args[0]) {
        Some(m) => {
            let mut output = String::new();
            writeln!(output, "name:        {}", m.name).unwrap();
            writeln!(output, "description: {}", m.description).unwrap();
            writeln!(output, "version:     {}", m.version).unwrap();
            writeln!(output, "license:     {}", m.license).unwrap();
            writeln!(output, "state:       {:?}", m.state).unwrap();
            writeln!(output, "ref_count:   {}", m.ref_count).unwrap();
            if !m.dependencies.is_empty() {
                let dep_names: Vec<&str> = m.dependencies.iter().map(|(n, _)| n.as_str()).collect();
                writeln!(output, "depends:     {}", dep_names.join(",")).unwrap();
            }
            ShellResult::ok(&output)
        }
        None => ShellResult::err(&alloc::format!("modinfo: module '{}' not found", args[0])),
    }
}

pub fn insmod(args: &[String]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::err("insmod: missing module name");
    }
    let module = crate::modules::KernelModule {
        name: args[0].clone(),
        description: String::from("User-loaded module"),
        author: String::from("user"),
        version: String::from("0.1.0"),
        license: String::from("GPL"),
        state: crate::modules::ModuleState::Live,
        size_bytes: 0,
        dependencies: Vec::new(),
        ref_count: 0,
        parameters: BTreeMap::new(),
    };
    match crate::modules::insert_module(module) {
        Ok(()) => ShellResult::ok(&alloc::format!("Module '{}' loaded", args[0])),
        Err(e) => ShellResult::err(&alloc::format!("insmod: {}", e)),
    }
}

pub fn rmmod(args: &[String]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::err("rmmod: missing module name");
    }
    match crate::modules::remove_module(&args[0]) {
        Ok(()) => ShellResult::ok(&alloc::format!("Module '{}' unloaded", args[0])),
        Err(e) => ShellResult::err(&alloc::format!("rmmod: {}", e)),
    }
}

pub fn lsblk() -> ShellResult {
    let devices = crate::block::list_devices();
    if devices.is_empty() {
        ShellResult::ok("No block devices found\n")
    } else {
        let mut output = String::from("NAME          SIZE       TYPE       MODEL\n");
        for dev in &devices {
            let size_mb = (dev.total_blocks * dev.block_size as u64) / (1024 * 1024);
            writeln!(
                output,
                "{:<13} {:<10} {:?}{:<5} {}",
                dev.name,
                alloc::format!("{}MB", size_mb),
                dev.device_type,
                "",
                dev.model
            )
            .unwrap();
        }
        ShellResult::ok(&output)
    }
}

pub fn mount(args: &[String]) -> ShellResult {
    if args.is_empty() {
        // Show currently mounted filesystems
        let mounts = crate::ext2::list_mounts();
        if mounts.is_empty() {
            ShellResult::ok("No filesystems mounted\n")
        } else {
            let mut output = String::new();
            for m in &mounts {
                writeln!(output, "{}", m).unwrap();
            }
            ShellResult::ok(&output)
        }
    } else if args.len() >= 2 {
        let dev_idx = args[0]
            .trim_start_matches("/dev/sd")
            .chars()
            .next()
            .map(|c| (c as u8 - b'a') as usize)
            .unwrap_or(0);
        match crate::ext2::mount(dev_idx) {
            Ok(_) => ShellResult::ok(&alloc::format!("Mounted {} on {}\n", args[0], args[1])),
            Err(_) => ShellResult::err(&alloc::format!("mount: failed to mount {}", args[0])),
        }
    } else {
        ShellResult::err("mount: usage: mount <device> <mountpoint>")
    }
}

pub fn umount(args: &[String]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::err("umount: missing mountpoint");
    }
    ShellResult::ok(&alloc::format!("Unmounted {}\n", args[0]))
}

pub fn poweroff() -> ShellResult {
    crate::serial_println!("[KnoxOS] System powering off...");
    crate::sound::play_event(crate::sound::SoundEvent::Notification);
    crate::acpi::shutdown();
}

pub fn reboot() -> ShellResult {
    crate::serial_println!("[KnoxOS] System rebooting...");
    crate::acpi::reboot();
}
