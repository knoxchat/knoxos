/// Security & misc builtins — lscgroup, getenforce, sestatus, gpu_info
use alloc::string::String;
use core::fmt::Write;

use crate::shell::types::ShellResult;

pub fn lscgroup() -> ShellResult {
    let groups = crate::cgroups::list_all();
    let mut output = String::new();
    for g in &groups {
        writeln!(output, "{}", g).unwrap();
    }
    ShellResult::ok(&output)
}

pub fn getenforce() -> ShellResult {
    let mode = crate::security::get_mode();
    ShellResult::ok(&alloc::format!("{}\n", mode))
}

pub fn sestatus() -> ShellResult {
    let status = crate::security::status();
    ShellResult::ok(&alloc::format!("{}\n", status))
}

pub fn gpu_info() -> ShellResult {
    let info = crate::gpu::drm_info();
    ShellResult::ok(&alloc::format!("{}\n", info))
}

/// xrandr — query and set display resolution via BGA
pub fn xrandr(args: &[String]) -> ShellResult {
    let (cur_w, cur_h) = crate::gui::screen_size();

    // No arguments: list supported modes
    if args.is_empty() || (args.len() == 1 && args[0] == "xrandr") {
        let mut output = String::new();
        writeln!(output, "Screen 0: {}x{} current", cur_w, cur_h).unwrap();
        writeln!(output, "VGA-1 connected {}x{}+0+0", cur_w, cur_h).unwrap();
        for &(rw, rh, label) in crate::gui::RESOLUTIONS {
            let active = if rw == cur_w && rh == cur_h {
                " *+"
            } else {
                "   "
            };
            writeln!(output, "   {:<24}{}", label, active).unwrap();
        }
        return ShellResult::ok(&output);
    }

    // Parse --mode WIDTHxHEIGHT  or  -s WIDTHxHEIGHT  or just WIDTHxHEIGHT
    let mut mode_str: Option<&str> = None;
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        match a {
            "--mode" | "-s" | "--size" | "--output" => {
                // skip --output VGA-1, but grab next arg for --mode / -s
                if a == "--output" {
                    i += 1; // skip the output name
                } else if i + 1 < args.len() {
                    mode_str = Some(args[i + 1].as_str());
                    i += 1;
                }
            }
            s if s.contains('x') && s.chars().next().is_some_and(|c| c.is_ascii_digit()) => {
                mode_str = Some(s);
            }
            _ => {}
        }
        i += 1;
    }

    if let Some(m) = mode_str {
        // Parse WIDTHxHEIGHT
        let parts: alloc::vec::Vec<&str> = m.split('x').collect();
        if parts.len() == 2 {
            if let (Ok(w), Ok(h)) = (parts[0].parse::<usize>(), parts[1].parse::<usize>()) {
                if w >= 640 && h >= 480 && w <= 7680 && h <= 4320 {
                    if crate::gui::change_resolution(w, h) {
                        return ShellResult::ok(&alloc::format!(
                            "Resolution changed to {}x{}\n",
                            w,
                            h
                        ));
                    } else {
                        return ShellResult::err(&alloc::format!(
                            "Failed to set resolution {}x{}\n",
                            w,
                            h
                        ));
                    }
                }
            }
        }
        return ShellResult::err(&alloc::format!(
            "Invalid mode: '{}'. Use WIDTHxHEIGHT (e.g. 1920x1080)\n",
            m
        ));
    }

    ShellResult::err("Usage: xrandr [--mode WIDTHxHEIGHT] [-s WIDTHxHEIGHT]\n")
}
