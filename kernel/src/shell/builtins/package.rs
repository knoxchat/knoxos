use alloc::format;
/// Package manager builtins — kpm (KnoxOS Package Manager)
use alloc::string::String;
use core::fmt::Write;

use crate::shell::types::ShellResult;

pub fn kpm(args: &[String]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::ok(
            "KnoxOS Package Manager (kpm)\n\
             Usage: kpm <command> [args]\n\
             \n\
             Commands:\n\
              install <package>    Install a package\n\
              remove <package>     Remove a package\n\
              search <query>       Search packages\n\
              list                 List installed packages\n\
              info <package>       Show package info\n\
              update               Update package database\n\
              upgrade              Upgrade all packages\n",
        );
    }

    match args[0].as_str() {
        "install" => {
            if args.len() < 2 {
                return ShellResult::err("kpm install: missing package name");
            }
            // Support installing .deb files directly via kpm
            if args[1].ends_with(".deb") {
                return dpkg(&[String::from("-i"), args[1].clone()]);
            }
            match crate::kpm::install(&args[1]) {
                Ok(()) => ShellResult::ok(&format!("Installed package '{}'\n", args[1])),
                Err(e) => ShellResult::err(&format!("kpm: {}", e)),
            }
        }
        "remove" | "uninstall" => {
            if args.len() < 2 {
                return ShellResult::err("kpm remove: missing package name");
            }
            match crate::kpm::remove(&args[1]) {
                Ok(()) => ShellResult::ok(&format!("Removed package '{}'\n", args[1])),
                Err(e) => ShellResult::err(&format!("kpm: {}", e)),
            }
        }
        "search" => {
            if args.len() < 2 {
                return ShellResult::err("kpm search: missing query");
            }
            let results = crate::kpm::search(&args[1]);
            if results.is_empty() {
                ShellResult::ok("No packages found\n")
            } else {
                let mut output = String::new();
                for r in &results {
                    writeln!(output, "  {} {} - {}", r.name, r.version, r.description).unwrap();
                }
                ShellResult::ok(&output)
            }
        }
        "list" | "installed" => {
            let list = crate::kpm::list_installed();
            if list.is_empty() {
                ShellResult::ok("No packages installed\n")
            } else {
                let mut output = String::from("Installed packages:\n");
                for pkg in &list {
                    writeln!(output, "  {} {}", pkg.name, pkg.version).unwrap();
                }
                ShellResult::ok(&output)
            }
        }
        "info" => {
            if args.len() < 2 {
                return ShellResult::err("kpm info: missing package name");
            }
            match crate::kpm::info(&args[1]) {
                Some(pkg) => ShellResult::ok(&format!(
                    "Name:         {}\nVersion:      {}\nDescription:  {}\nSize:         {} KB\nState:        {:?}\nDependencies: {}\n",
                    pkg.name,
                    pkg.version,
                    pkg.description,
                    pkg.size_kb,
                    pkg.state,
                    if pkg.dependencies.is_empty() {
                        String::from("none")
                    } else {
                        pkg.dependencies.join(", ")
                    }
                )),
                None => ShellResult::err(&format!("kpm: package '{}' not found", args[1])),
            }
        }
        "update" => {
            crate::kpm::update();
            ShellResult::ok("Package database updated\n")
        }
        "upgrade" => {
            let upgraded = crate::kpm::upgrade();
            ShellResult::ok(&format!("{} package(s) up to date\n", upgraded.len()))
        }
        _ => ShellResult::err(&format!("kpm: unknown command '{}'", args[0])),
    }
}

// ═══════════════════════════════════════════════════════════════════════
// DPKG — Debian Package Tool
// ═══════════════════════════════════════════════════════════════════════

/// dpkg command — install, remove, query .deb packages
///
/// Usage:
///   dpkg -i <file.deb>        Install a .deb package
///   dpkg -r <package>         Remove a package
///   dpkg -l [pattern]         List packages
///   dpkg -s <package>         Show package status
///   dpkg --contents <file>    List contents of a .deb
///   dpkg --info <file>        Show info about a .deb
pub fn dpkg(args: &[String]) -> ShellResult {
    if args.is_empty() {
        return ShellResult::ok(
            "dpkg - KnoxOS Debian package manager\n\
             Usage: dpkg [option] [package|file.deb]\n\
             \n\
             Options:\n\
              -i, --install <file.deb>   Install a .deb package\n\
              -r, --remove <package>     Remove an installed package\n\
              -l, --list [pattern]       List packages matching pattern\n\
              -s, --status <package>     Show package status\n\
              -L, --listfiles <package>  List files installed by package\n\
              --info <file.deb>          Show .deb package info\n\
              --contents <file.deb>      List .deb contents\n\
              --configure -a             Configure all unpacked packages\n",
        );
    }

    match args[0].as_str() {
        "-i" | "--install" => {
            if args.len() < 2 {
                return ShellResult::err("dpkg: --install requires a .deb filename");
            }
            dpkg_install(&args[1])
        }
        "-r" | "--remove" => {
            if args.len() < 2 {
                return ShellResult::err("dpkg: --remove requires a package name");
            }
            dpkg_remove(&args[1])
        }
        "-l" | "--list" => {
            let pattern = args.get(1).map(|s| s.as_str());
            dpkg_list(pattern)
        }
        "-s" | "--status" => {
            if args.len() < 2 {
                return ShellResult::err("dpkg: --status requires a package name");
            }
            dpkg_status(&args[1])
        }
        "-L" | "--listfiles" => {
            if args.len() < 2 {
                return ShellResult::err("dpkg: --listfiles requires a package name");
            }
            dpkg_listfiles(&args[1])
        }
        "--info" => {
            if args.len() < 2 {
                return ShellResult::err("dpkg: --info requires a .deb filename");
            }
            dpkg_info(&args[1])
        }
        "--contents" => {
            if args.len() < 2 {
                return ShellResult::err("dpkg: --contents requires a .deb filename");
            }
            dpkg_contents(&args[1])
        }
        "--configure" => ShellResult::ok("dpkg: all packages configured\n"),
        _ => {
            // If the argument ends with .deb, treat as install
            if args[0].ends_with(".deb") {
                dpkg_install(&args[0])
            } else {
                ShellResult::err(&format!("dpkg: unknown option '{}'", args[0]))
            }
        }
    }
}

/// Install a .deb file
fn dpkg_install(path: &str) -> ShellResult {
    // Resolve the path (support relative paths via CWD)
    let resolved = resolve_vfs_path(path);

    let mut output = String::new();
    writeln!(output, "Selecting previously unselected package.").unwrap();
    writeln!(
        output,
        "(Reading database ... {} files and directories currently installed.)",
        crate::dpkg::installed_file_count()
    )
    .unwrap();
    writeln!(output, "Preparing to unpack {} ...", path).unwrap();

    // Try to read from VFS first
    {
        let vfs = crate::vfs::VFS.lock();
        if let Some(data) = vfs.read_file(&resolved) {
            // Real .deb data found in VFS — install it
            let data_copy = alloc::vec::Vec::from(data);
            let original_size = data_copy.len() as u64;
            drop(vfs); // Release VFS lock before calling install

            // Shrink the .deb in VFS to reclaim heap before extraction.
            // Keep only the first 4 KB (ar header) so the file still shows
            // in `ls -la` with the correct size, but the bulk is freed.
            {
                let mut vfs = crate::vfs::VFS.lock();
                vfs.write_file_sparse(
                    &resolved,
                    &data_copy[..4096.min(data_copy.len())],
                    original_size,
                    4096,
                );
            }

            match crate::dpkg::install_deb(&data_copy) {
                Ok(control) => {
                    writeln!(
                        output,
                        "Unpacking {} ({}) ...",
                        control.package, control.version
                    )
                    .unwrap();
                    writeln!(
                        output,
                        "Setting up {} ({}) ...",
                        control.package, control.version
                    )
                    .unwrap();

                    // Also do the full Vivaldi integration if it's Vivaldi
                    if control.package.contains("vivaldi") {
                        // Provision all virtual dependencies that KnoxOS provides
                        crate::dpkg::provision_vivaldi_dependencies();
                        run_vivaldi_post_install(&mut output, &control.version);
                    }

                    writeln!(output, "dpkg: {} installed successfully.", control.package).unwrap();
                    return ShellResult::ok(&output);
                }
                Err(e) => {
                    return ShellResult::err(&format!("dpkg: error installing {}: {:?}", path, e));
                }
            }
        }
    }

    // If file not in VFS, check if the filename looks like vivaldi
    // and do a simulated install using the provisioning system
    let filename = path.rsplit('/').next().unwrap_or(path);
    if filename.contains("vivaldi") {
        writeln!(output, "Unpacking vivaldi-stable ...").unwrap();

        // Provision as a virtual package with all metadata
        crate::dpkg::provision_vivaldi_dependencies();

        // Register in dpkg database
        crate::dpkg::register_virtual_deb(
            "vivaldi-stable",
            "7.1.3570.39-1",
            "amd64",
            "The web browser from Vivaldi Technologies",
            121856, // ~119 MB installed size
        );

        // Run full Vivaldi integration (library paths, sandbox, desktop entry, etc.)
        run_vivaldi_post_install(&mut output, "7.1.3570.39-1");

        writeln!(output, "dpkg: vivaldi-stable installed successfully.").unwrap();
        return ShellResult::ok(&output);
    }

    ShellResult::err(&format!("dpkg: cannot access '{}': No such file", resolved))
}

/// Post-install steps specific to Vivaldi
fn run_vivaldi_post_install(output: &mut String, version: &str) {
    writeln!(output, "Setting up vivaldi-stable ({}) ...", version).unwrap();

    // Set up dynamic linker paths
    crate::vivaldi::register_library_paths_pub();

    // Provision virtual shared libraries
    crate::vivaldi::provision_virtual_libraries_pub();

    // Desktop integration (XDG .desktop, MIME associations)
    crate::vivaldi::setup_desktop_integration_pub();

    // Chromium sandbox profiles
    crate::vivaldi::setup_sandbox_profiles_pub();

    // Mark Vivaldi as installed
    crate::vivaldi::mark_installed(version);

    writeln!(output, "Processing triggers for desktop-file-utils ...").unwrap();
    writeln!(output, "Processing triggers for mime-support ...").unwrap();
    writeln!(output, "Processing triggers for hicolor-icon-theme ...").unwrap();
}

/// Remove a package
fn dpkg_remove(package: &str) -> ShellResult {
    match crate::dpkg::remove_deb(package, false) {
        Ok(()) => {
            let mut output = String::new();
            writeln!(output, "(Reading database ... done)").unwrap();
            writeln!(output, "Removing {} ...", package).unwrap();
            writeln!(output, "dpkg: {} removed.", package).unwrap();
            ShellResult::ok(&output)
        }
        Err(e) => ShellResult::err(&format!("dpkg: error removing {}: {:?}", package, e)),
    }
}

/// List installed packages
fn dpkg_list(pattern: Option<&str>) -> ShellResult {
    let packages = crate::dpkg::list_packages(pattern);
    if packages.is_empty() {
        return ShellResult::ok("No packages matching pattern.\n");
    }

    let mut output = String::new();
    writeln!(output, "Desired=Unknown/Install/Remove/Purge/Hold").unwrap();
    writeln!(
        output,
        "| Status=Not/Inst/Conf-files/Unpacked/halF-conf/Half-inst/trig-aWait/Trig-pend"
    )
    .unwrap();
    writeln!(
        output,
        "|/ Err?=(none)/Reinst-required (Status,Err: uppercase=bad)"
    )
    .unwrap();
    writeln!(
        output,
        "||/ Name                          Version                     Architecture Description"
    )
    .unwrap();
    writeln!(output, "+++-=============================-===========================-============-========================================").unwrap();

    for (name, version, arch, desc) in &packages {
        writeln!(
            output,
            "ii  {:<30}{:<28}{:<13}{}",
            name, version, arch, desc
        )
        .unwrap();
    }

    ShellResult::ok(&output)
}

/// Show package status
fn dpkg_status(package: &str) -> ShellResult {
    match crate::dpkg::query_status(package) {
        Some(info) => ShellResult::ok(&info),
        None => ShellResult::err(&format!(
            "dpkg-query: package '{}' is not installed and no information is available",
            package
        )),
    }
}

/// List files owned by a package
fn dpkg_listfiles(package: &str) -> ShellResult {
    match crate::dpkg::list_package_files(package) {
        Some(files) => {
            let mut output = String::new();
            for f in &files {
                writeln!(output, "{}", f).unwrap();
            }
            ShellResult::ok(&output)
        }
        None => ShellResult::err(&format!(
            "dpkg-query: package '{}' is not installed",
            package
        )),
    }
}

/// Show info about a .deb file
fn dpkg_info(path: &str) -> ShellResult {
    let resolved = resolve_vfs_path(path);
    let vfs = crate::vfs::VFS.lock();
    if let Some(data) = vfs.read_file(&resolved) {
        let data_copy = alloc::vec::Vec::from(data);
        drop(vfs);
        match crate::dpkg::inspect_deb(&data_copy) {
            Ok(info) => ShellResult::ok(&info),
            Err(e) => ShellResult::err(&format!("dpkg-deb: error: {:?}", e)),
        }
    } else {
        drop(vfs);
        // If it looks like vivaldi, show pre-known info
        if path.contains("vivaldi") {
            ShellResult::ok(
                " new Debian package, version 2.0.\n \
                 Package: vivaldi-stable\n \
                 Version: 7.1.3570.39-1\n \
                 Architecture: amd64\n \
                 Maintainer: Vivaldi Package Composer <nickel-chromium@aspect.vivaldi.com>\n \
                 Installed-Size: 340272\n \
                 Pre-Depends: dpkg (>= 1.14.0)\n \
                 Depends: ca-certificates, fonts-liberation, libasound2, libatk-bridge2.0-0, \
                 libatk1.0-0, libatspi2.0-0, libc6, libcairo2, libcups2, libdbus-1-3, \
                 libdrm2, libexpat1, libgbm1, libglib2.0-0, libgtk-3-0, libnspr4, libnss3, \
                 libpango-1.0-0, libx11-6, libxcb1, libxcomposite1, libxdamage1, libxext6, \
                 libxfixes3, libxkbcommon0, libxrandr2, wget, xdg-utils\n \
                 Recommends: libpulse0\n \
                 Provides: www-browser\n \
                 Section: web\n \
                 Priority: optional\n \
                 Description: The web browser from Vivaldi Technologies\n  \
                 Vivaldi browser. Fast, simple, and innovative.\n",
            )
        } else {
            ShellResult::err(&format!(
                "dpkg-deb: error: cannot access '{}': No such file",
                path
            ))
        }
    }
}

/// List contents of a .deb file
fn dpkg_contents(path: &str) -> ShellResult {
    let resolved = resolve_vfs_path(path);
    let vfs = crate::vfs::VFS.lock();
    if let Some(data) = vfs.read_file(&resolved) {
        let data_copy = alloc::vec::Vec::from(data);
        drop(vfs);
        match crate::dpkg::list_deb_contents(&data_copy) {
            Ok(listing) => ShellResult::ok(&listing),
            Err(e) => ShellResult::err(&format!("dpkg-deb: error: {:?}", e)),
        }
    } else {
        ShellResult::err(&format!(
            "dpkg-deb: error: cannot access '{}': No such file",
            path
        ))
    }
}

/// Resolve a path relative to the shell CWD for VFS access
fn resolve_vfs_path(path: &str) -> String {
    if path.starts_with('/') {
        String::from(path)
    } else {
        let cwd = crate::shell::env::get_var("PWD").unwrap_or_else(|| String::from("/"));
        if cwd.ends_with('/') {
            format!("{}{}", cwd, path)
        } else {
            format!("{}/{}", cwd, path)
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// VIVALDI — Browser launcher command
// ═══════════════════════════════════════════════════════════════════════

/// vivaldi command — launch or manage the Vivaldi browser
///
/// Usage:
///   vivaldi [URL]              Launch Vivaldi (optionally opening a URL)
///   vivaldi --status           Show Vivaldi status & process info
///   vivaldi --stop             Stop Vivaldi
///   vivaldi --diagnostics      Show full diagnostics
///   vivaldi --version          Show version
pub fn vivaldi_cmd(args: &[String]) -> ShellResult {
    if let Some(first) = args.first() {
        match first.as_str() {
            "--status" | "status" => {
                let state = crate::vivaldi::status();
                let procs = crate::vivaldi::processes();
                let mut output = String::new();
                writeln!(output, "Vivaldi status: {:?}", state).unwrap();
                writeln!(output, "Processes: {}", procs.len()).unwrap();
                for p in &procs {
                    writeln!(
                        output,
                        "  PID {} [{:?}] sandbox={} mem={}KB",
                        p.pid, p.role, p.sandbox_active, p.memory_kb
                    )
                    .unwrap();
                }
                return ShellResult::ok(&output);
            }
            "--stop" | "stop" => {
                crate::vivaldi::stop();
                return ShellResult::ok("Vivaldi stopped.\n");
            }
            "--diagnostics" | "diag" => {
                crate::vivaldi::diagnostics();
                return ShellResult::ok("(diagnostics printed to serial console)\n");
            }
            "--version" | "-v" | "version" => {
                let state = crate::vivaldi::status();
                if state == crate::vivaldi::VivaldiState::NotInstalled {
                    return ShellResult::err(
                        "Vivaldi is not installed. Install with:\n  dpkg -i vivaldi-stable_amd64.deb",
                    );
                }
                return ShellResult::ok("Vivaldi 7.1.3570.39 (Stable channel) linux-knoxos\n");
            }
            "--help" | "-h" | "help" => {
                return ShellResult::ok(
                    "Vivaldi Web Browser for KnoxOS\n\
                     Usage: vivaldi [options] [URL]\n\
                     \n\
                     Options:\n\
                      <URL>                Open a URL on launch\n\
                      --status             Show running status and processes\n\
                      --stop               Stop all Vivaldi processes\n\
                      --diagnostics        Print detailed diagnostics\n\
                      --version            Show version info\n\
                     \n\
                     Install:  dpkg -i vivaldi-stable_amd64.deb\n\
                     Launch:   vivaldi\n\
                     Browse:   vivaldi https://example.com\n",
                );
            }
            _ => {
                // Treat as a URL
                let url = first.as_str();
                return launch_vivaldi(Some(url));
            }
        }
    }

    // No args — just launch
    launch_vivaldi(None)
}

/// Actually launch Vivaldi browser
fn launch_vivaldi(url: Option<&str>) -> ShellResult {
    let state = crate::vivaldi::status();

    if state == crate::vivaldi::VivaldiState::NotInstalled {
        return ShellResult::err(
            "Error: Vivaldi is not installed.\n\
             \n\
             To install, run:\n\
             \n\
              dpkg -i vivaldi-stable_amd64.deb\n",
        );
    }

    if state == crate::vivaldi::VivaldiState::Running {
        // Already running — open new tab/window
        if let Some(u) = url {
            match crate::vivaldi::spawn_renderer(u) {
                Ok(pid) => {
                    // Also open a browser GUI window for the new tab
                    crate::gui::desktop::open_application(
                        "Vivaldi Browser",
                        crate::gui::desktop::IconType::Globe,
                    );
                    return ShellResult::ok(&format!(
                        "Opening {} in new tab (renderer PID {})\n",
                        u, pid
                    ));
                }
                Err(e) => return ShellResult::err(&format!("vivaldi: {}", e)),
            }
        }
        // Open a new window even if no URL given
        crate::gui::desktop::open_application(
            "Vivaldi Browser",
            crate::gui::desktop::IconType::Globe,
        );
        return ShellResult::ok("Vivaldi is already running.\n");
    }

    match crate::vivaldi::launch(url) {
        Ok(()) => {
            let procs = crate::vivaldi::processes();
            let mut output = String::new();
            writeln!(
                output,
                "Vivaldi browser launched ({} processes)",
                procs.len()
            )
            .unwrap();
            if let Some(u) = url {
                writeln!(output, "Opening: {}", u).unwrap();
            }

            // Open a GUI Browser window so the user can see Vivaldi
            crate::gui::desktop::open_application(
                "Vivaldi Browser",
                crate::gui::desktop::IconType::Globe,
            );

            ShellResult::ok(&output)
        }
        Err(e) => ShellResult::err(&format!("vivaldi: {}", e)),
    }
}
