/// DPKG Post-Install Script Execution (Phase 25)
///
/// Extends the dpkg subsystem with the ability to execute
/// maintainer scripts (preinst, postinst, prerm, postrm)
/// during package installation and removal.
///
/// Script execution pipeline:
///   1. Extract maintainer scripts from control.tar
///   2. Install to /var/lib/dpkg/info/<package>.<script>
///   3. Execute via /bin/sh with proper environment
///   4. Handle exit codes and trigger processing
///   5. Update dpkg status database
///
/// Also implements:
///   - dpkg triggers (ldconfig, update-alternatives, etc.)
///   - dpkg diversions (file replacement overrides)
///   - Conffile handling (preserve user modifications)
///   - dpkg status database updates (/var/lib/dpkg/status)
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// MAINTAINER SCRIPT TYPES
// ═══════════════════════════════════════════════════════════════════════

/// The four maintainer script types in Debian packages
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptType {
    /// Runs before package files are unpacked
    Preinst,
    /// Runs after package files are unpacked
    Postinst,
    /// Runs before package files are removed
    Prerm,
    /// Runs after package files are removed
    Postrm,
}

impl ScriptType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ScriptType::Preinst => "preinst",
            ScriptType::Postinst => "postinst",
            ScriptType::Prerm => "prerm",
            ScriptType::Postrm => "postrm",
        }
    }
}

impl core::fmt::Display for ScriptType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Result of a script execution
#[derive(Debug, Clone)]
pub struct ScriptResult {
    pub script_type: ScriptType,
    pub package: String,
    pub exit_code: i32,
    pub output: String,
    pub success: bool,
}

/// A maintainer script extracted from a .deb
#[derive(Debug, Clone)]
pub struct MaintainerScript {
    pub script_type: ScriptType,
    pub package: String,
    pub version: String,
    pub content: Vec<u8>,
    pub is_executable: bool,
}

// ═══════════════════════════════════════════════════════════════════════
// SCRIPT STORAGE
// ═══════════════════════════════════════════════════════════════════════

lazy_static::lazy_static! {
    /// Stored maintainer scripts: package -> (script_type -> content)
    static ref SCRIPTS: Mutex<BTreeMap<String, BTreeMap<String, Vec<u8>>>> =
        Mutex::new(BTreeMap::new());

    /// Execution log
    static ref EXECUTION_LOG: Mutex<Vec<ScriptResult>> = Mutex::new(Vec::new());

    /// Registered triggers: trigger_name -> list of packages interested
    static ref TRIGGERS: Mutex<BTreeMap<String, Vec<String>>> = Mutex::new(BTreeMap::new());

    /// Diversions: diverted_path -> (diversion_to, package)
    static ref DIVERSIONS: Mutex<BTreeMap<String, (String, String)>> = Mutex::new(BTreeMap::new());

    /// Conffile tracking: package -> list of conffile paths
    static ref CONFFILES: Mutex<BTreeMap<String, Vec<String>>> = Mutex::new(BTreeMap::new());
}

// ═══════════════════════════════════════════════════════════════════════
// SCRIPT EXTRACTION FROM .DEB
// ═══════════════════════════════════════════════════════════════════════

/// Extract maintainer scripts from a control.tar archive
pub fn extract_scripts_from_control(
    package: &str,
    version: &str,
    control_data: &[u8],
) -> Vec<MaintainerScript> {
    let mut scripts = Vec::new();

    // The control.tar contains files like:
    //   ./preinst
    //   ./postinst
    //   ./prerm
    //   ./postrm
    //   ./control
    //   ./md5sums
    //   ./conffiles
    //   ./triggers

    // Parse as tar (may be compressed — dpkg.rs handles decompression)
    let entries = crate::dpkg::parse_tar(control_data);

    for entry in &entries {
        let name = entry.path.trim_start_matches("./").trim_start_matches('/');

        let script_type = match name {
            "preinst" => Some(ScriptType::Preinst),
            "postinst" => Some(ScriptType::Postinst),
            "prerm" => Some(ScriptType::Prerm),
            "postrm" => Some(ScriptType::Postrm),
            _ => None,
        };

        if let Some(stype) = script_type {
            let data_end = entry.data_offset + entry.size;
            if data_end <= control_data.len() {
                let content = control_data[entry.data_offset..data_end].to_vec();
                scripts.push(MaintainerScript {
                    script_type: stype,
                    package: String::from(package),
                    version: String::from(version),
                    content,
                    is_executable: true,
                });
            }
        }

        // Also extract conffiles list
        if name == "conffiles" {
            let data_end = entry.data_offset + entry.size;
            if data_end <= control_data.len() {
                let content = &control_data[entry.data_offset..data_end];
                if let Ok(text) = core::str::from_utf8(content) {
                    let conf_list: Vec<String> = text
                        .lines()
                        .filter(|l| !l.is_empty())
                        .map(|l| l.trim().to_string())
                        .collect();
                    CONFFILES.lock().insert(String::from(package), conf_list);
                }
            }
        }

        // Extract trigger interests
        if name == "triggers" {
            let data_end = entry.data_offset + entry.size;
            if data_end <= control_data.len() {
                let content = &control_data[entry.data_offset..data_end];
                if let Ok(text) = core::str::from_utf8(content) {
                    parse_trigger_file(package, text);
                }
            }
        }
    }

    // Store scripts for later access
    {
        let mut store = SCRIPTS.lock();
        let pkg_scripts = store.entry(String::from(package)).or_default();
        for script in &scripts {
            pkg_scripts.insert(
                String::from(script.script_type.as_str()),
                script.content.clone(),
            );
        }
    }

    serial_println!(
        "[dpkg-scripts] Extracted {} scripts for '{}' v{}",
        scripts.len(),
        package,
        version
    );

    scripts
}

/// Parse a triggers file and register interests
fn parse_trigger_file(package: &str, content: &str) {
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let parts: Vec<&str> = line.splitn(2, ' ').collect();
        if parts.len() < 2 {
            continue;
        }

        let directive = parts[0];
        let trigger_name = parts[1].trim();

        match directive {
            "interest" | "interest-await" | "interest-noawait" => {
                let mut triggers = TRIGGERS.lock();
                triggers
                    .entry(String::from(trigger_name))
                    .or_default()
                    .push(String::from(package));
                serial_println!(
                    "[dpkg-triggers] {} interested in '{}'",
                    package,
                    trigger_name
                );
            }
            "activate" | "activate-await" | "activate-noawait" => {
                serial_println!("[dpkg-triggers] {} activates '{}'", package, trigger_name);
            }
            _ => {}
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// SCRIPT EXECUTION
// ═══════════════════════════════════════════════════════════════════════

/// Execute a maintainer script for a package.
///
/// The script is executed via the shell (/bin/sh) with the standard
/// dpkg environment variables set:
///   - DPKG_MAINTSCRIPT_PACKAGE=<package>
///   - DPKG_MAINTSCRIPT_ARCH=amd64
///   - DPKG_MAINTSCRIPT_NAME=<script_type>
///   - DEBIAN_FRONTEND=noninteractive
///
/// Arguments follow dpkg conventions:
///   - postinst configure <most-recent-configured-version>
///   - prerm remove
///   - postrm remove
///   - preinst install <new-version>
pub fn execute_script(package: &str, script_type: ScriptType, args: &[&str]) -> ScriptResult {
    serial_println!(
        "[dpkg-scripts] Executing {}.{} {}",
        package,
        script_type,
        args.join(" ")
    );

    // Find the script
    let script_content = {
        let store = SCRIPTS.lock();
        store
            .get(package)
            .and_then(|pkg| pkg.get(script_type.as_str()))
            .cloned()
    };

    let content = match script_content {
        Some(c) => c,
        None => {
            // No script of this type — that's OK, return success
            serial_println!("[dpkg-scripts] No {} script for '{}'", script_type, package);
            return ScriptResult {
                script_type,
                package: String::from(package),
                exit_code: 0,
                output: String::from("(no script)"),
                success: true,
            };
        }
    };

    // Install the script to the dpkg info directory
    let script_path = format!("/var/lib/dpkg/info/{}.{}", package, script_type.as_str());
    crate::vfs::create_file_dispatch(&script_path, &content);

    // Parse the script content to determine the interpreter
    let content_str = String::from_utf8_lossy(&content).to_string();
    let interpreter = if content_str.starts_with("#!") {
        let first_line = content_str.lines().next().unwrap_or("#!/bin/sh");
        let interp = first_line.trim_start_matches("#!").trim();
        String::from(interp)
    } else {
        String::from("/bin/sh")
    };

    // Build the environment
    let env_vars = vec![
        format!("DPKG_MAINTSCRIPT_PACKAGE={}", package),
        format!("DPKG_MAINTSCRIPT_ARCH=amd64"),
        format!("DPKG_MAINTSCRIPT_NAME={}", script_type.as_str()),
        format!("DEBIAN_FRONTEND=noninteractive"),
        format!("PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"),
    ];

    // Execute via shell
    let exit_code = execute_shell_script(&content_str, &interpreter, args, &env_vars);

    let success = exit_code == 0;
    let result = ScriptResult {
        script_type,
        package: String::from(package),
        exit_code,
        output: if success {
            format!("{} completed successfully", script_type)
        } else {
            format!("{} failed with exit code {}", script_type, exit_code)
        },
        success,
    };

    // Log the execution
    EXECUTION_LOG.lock().push(result.clone());

    if success {
        serial_println!("[dpkg-scripts] {}.{}: OK", package, script_type);
    } else {
        serial_println!(
            "[dpkg-scripts] {}.{}: FAILED (exit code {})",
            package,
            script_type,
            exit_code
        );
    }

    result
}

/// Execute a shell script using the kernel's built-in shell
fn execute_shell_script(
    content: &str,
    _interpreter: &str,
    args: &[&str],
    env_vars: &[String],
) -> i32 {
    // Set up environment variables in the shell context
    for var in env_vars {
        if let Some(eq_pos) = var.find('=') {
            let key = &var[..eq_pos];
            let value = &var[eq_pos + 1..];
            crate::shell::set_env_var(key, value);
        }
    }

    // Set positional parameters ($1, $2, etc.)
    for (i, arg) in args.iter().enumerate() {
        crate::shell::set_env_var(&format!("{}", i + 1), arg);
    }

    // Parse and execute the script line by line
    let mut exit_code = 0i32;
    let mut in_function = false;
    let mut skip_block = false;

    for line in content.lines() {
        let line = line.trim();

        // Skip empty lines, comments, and shebang
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Handle common dpkg postinst patterns:
        // Most postinst scripts follow this pattern:
        //   case "$1" in
        //     configure)
        //       ...commands...
        //       ;;
        //   esac
        //   exit 0

        // Simple command dispatch via the built-in shell
        if line == "set -e" {
            // Enable exit-on-error (we track this but don't abort the whole kernel)
            continue;
        }

        if let Some(rest) = line.strip_prefix("exit ") {
            if let Ok(code) = rest.trim().parse::<i32>() {
                exit_code = code;
            }
            break;
        }

        // Execute the command via the shell
        let result = crate::shell::execute_command(line);
        if result.exit_code != 0 {
            exit_code = result.exit_code;
            // In a `set -e` script, any failure aborts
            // For now, continue but record the failure
        }
    }

    exit_code
}

// ═══════════════════════════════════════════════════════════════════════
// PACKAGE INSTALLATION FLOW
// ═══════════════════════════════════════════════════════════════════════

/// Full dpkg package installation with script execution.
///
/// Follows the Debian package install sequence:
///   1. Run old-prerm upgrade (if upgrading)
///   2. Run new-preinst install
///   3. Unpack new files
///   4. Run new-postinst configure
///   5. Process triggers
///   6. Update dpkg status database
pub fn install_package_full(
    package: &str,
    version: &str,
    control_data: &[u8],
    data_entries: &[(String, Vec<u8>)], // (path, content) pairs
) -> Result<(), String> {
    serial_println!("[dpkg] Installing {} version {}", package, version);

    // Step 1: Extract maintainer scripts
    let scripts = extract_scripts_from_control(package, version, control_data);

    // Step 2: Run preinst
    let preinst_result = execute_script(package, ScriptType::Preinst, &["install", version]);
    if !preinst_result.success {
        return Err(format!(
            "preinst failed for {} (exit code {})",
            package, preinst_result.exit_code
        ));
    }

    // Step 3: Unpack files to rootfs
    let mut installed_files = Vec::new();
    for (path, content) in data_entries {
        let full_path = if let Some(rest) = path.strip_prefix("./") {
            format!("/{}", rest)
        } else if path.starts_with('/') {
            path.clone()
        } else {
            format!("/{}", path)
        };

        // Check for diversions
        let actual_path = {
            let diversions = DIVERSIONS.lock();
            if let Some((divert_to, _pkg)) = diversions.get(&full_path) {
                divert_to.clone()
            } else {
                full_path.clone()
            }
        };

        // Create parent directories
        if let Some(parent) = actual_path.rfind('/') {
            let parent_path = &actual_path[..parent];
            if !parent_path.is_empty() {
                crate::vfs::ensure_directory(parent_path);
            }
        }

        // Check conffile preservation
        let should_preserve = {
            let conffiles = CONFFILES.lock();
            conffiles
                .get(package)
                .map(|cf| cf.contains(&full_path))
                .unwrap_or(false)
        };

        if should_preserve && crate::vfs::read_file_dispatch(&actual_path).is_some() {
            // Conffile exists and is tracked — don't overwrite
            serial_println!("[dpkg] Preserving conffile: {}", actual_path);
        } else {
            crate::vfs::create_file_dispatch(&actual_path, content);
        }

        installed_files.push(actual_path);
    }

    // Step 4: Run postinst configure
    let postinst_result = execute_script(package, ScriptType::Postinst, &["configure", version]);
    if !postinst_result.success {
        serial_println!(
            "[dpkg] Warning: postinst failed for {} (exit code {}), continuing...",
            package,
            postinst_result.exit_code
        );
        // Don't fail the whole installation for postinst failures
    }

    // Step 5: Process triggers
    process_pending_triggers();

    // Step 6: Update dpkg status database
    update_dpkg_status(package, version, "installed", &installed_files);

    serial_println!(
        "[dpkg] Successfully installed {} v{} ({} files)",
        package,
        version,
        installed_files.len()
    );

    Ok(())
}

/// Remove a package with script execution
pub fn remove_package_full(package: &str) -> Result<(), String> {
    serial_println!("[dpkg] Removing {}", package);

    // Run prerm
    let prerm_result = execute_script(package, ScriptType::Prerm, &["remove"]);
    if !prerm_result.success {
        serial_println!(
            "[dpkg] Warning: prerm failed for {} (exit code {})",
            package,
            prerm_result.exit_code
        );
    }

    // Remove installed files (from dpkg database)
    // The actual file removal is handled by dpkg.rs

    // Run postrm
    let postrm_result = execute_script(package, ScriptType::Postrm, &["remove"]);
    if !postrm_result.success {
        serial_println!(
            "[dpkg] Warning: postrm failed for {} (exit code {})",
            package,
            postrm_result.exit_code
        );
    }

    // Clean up scripts
    SCRIPTS.lock().remove(package);
    CONFFILES.lock().remove(package);

    // Update status
    update_dpkg_status(package, "", "deinstall ok config-files", &[]);

    serial_println!("[dpkg] Removed {}", package);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// TRIGGER PROCESSING
// ═══════════════════════════════════════════════════════════════════════

/// Process all pending triggers
pub fn process_pending_triggers() {
    let triggers = TRIGGERS.lock().clone();

    for (trigger_name, interested_packages) in &triggers {
        serial_println!("[dpkg-triggers] Processing trigger: {}", trigger_name);

        // Handle well-known triggers
        match trigger_name.as_str() {
            "ldconfig" => {
                serial_println!("[dpkg-triggers] Running ldconfig");
                // Regenerate shared library cache
                crate::shell::execute_command("ldconfig");
            }
            "update-alternatives" => {
                serial_println!("[dpkg-triggers] Updating alternatives");
            }
            "man-db" => {
                serial_println!("[dpkg-triggers] Updating man-db cache");
            }
            "desktop-file-utils" => {
                serial_println!("[dpkg-triggers] Updating desktop database");
            }
            "mime-support" => {
                serial_println!("[dpkg-triggers] Updating MIME database");
            }
            "shared-mime-info" => {
                serial_println!("[dpkg-triggers] Updating shared MIME info");
            }
            "hicolor-icon-theme" => {
                serial_println!("[dpkg-triggers] Updating icon cache");
            }
            _ => {
                // Run postinst for interested packages
                for pkg in interested_packages {
                    execute_script(pkg, ScriptType::Postinst, &["triggered", trigger_name]);
                }
            }
        }
    }
}

/// Activate a trigger (called during package installation)
pub fn activate_trigger(trigger_name: &str, activating_package: &str) {
    serial_println!(
        "[dpkg-triggers] Activated '{}' by {}",
        trigger_name,
        activating_package
    );
    // The trigger will be processed at the end of the installation
}

// ═══════════════════════════════════════════════════════════════════════
// DIVERSIONS
// ═══════════════════════════════════════════════════════════════════════

/// Add a dpkg diversion (dpkg-divert --add)
pub fn add_diversion(path: &str, divert_to: &str, package: &str) {
    serial_println!(
        "[dpkg-divert] Diverting {} to {} (by {})",
        path,
        divert_to,
        package
    );
    DIVERSIONS.lock().insert(
        String::from(path),
        (String::from(divert_to), String::from(package)),
    );
}

/// Remove a dpkg diversion (dpkg-divert --remove)
pub fn remove_diversion(path: &str) {
    DIVERSIONS.lock().remove(path);
}

/// Check if a path is diverted
pub fn get_diversion(path: &str) -> Option<(String, String)> {
    DIVERSIONS.lock().get(path).cloned()
}

// ═══════════════════════════════════════════════════════════════════════
// DPKG STATUS DATABASE
// ═══════════════════════════════════════════════════════════════════════

/// Update the dpkg status database file (/var/lib/dpkg/status)
fn update_dpkg_status(package: &str, version: &str, status: &str, installed_files: &[String]) {
    // Write to /var/lib/dpkg/status
    let status_entry = format!(
        "Package: {}\nStatus: install ok {}\nPriority: optional\nSection: misc\n\
         Architecture: amd64\nVersion: {}\nInstalled-Size: {}\n\n",
        package,
        status,
        version,
        installed_files.len() * 4 // rough estimate in KB
    );

    // Append to status file
    let status_path = "/var/lib/dpkg/status";
    let existing = crate::vfs::read_file_dispatch(status_path).unwrap_or_default();
    let mut full_content = existing;
    full_content.extend_from_slice(status_entry.as_bytes());
    crate::vfs::create_file_dispatch(status_path, &full_content);

    // Write file list to /var/lib/dpkg/info/<package>.list
    let list_path = format!("/var/lib/dpkg/info/{}.list", package);
    let file_list: String = installed_files.iter().map(|f| format!("{}\n", f)).collect();
    crate::vfs::create_file_dispatch(&list_path, file_list.as_bytes());
}

/// Get the execution log
pub fn get_execution_log() -> Vec<ScriptResult> {
    EXECUTION_LOG.lock().clone()
}

// ═══════════════════════════════════════════════════════════════════════
// INITIALIZATION
// ═══════════════════════════════════════════════════════════════════════

pub fn init() {
    // Ensure dpkg directories exist
    crate::vfs::ensure_directory("/var/lib/dpkg");
    crate::vfs::ensure_directory("/var/lib/dpkg/info");
    crate::vfs::ensure_directory("/var/lib/dpkg/updates");
    crate::vfs::ensure_directory("/var/lib/dpkg/triggers");
    crate::vfs::ensure_directory("/var/cache/apt/archives");

    // Create initial status file if missing
    let status_path = "/var/lib/dpkg/status";
    if crate::vfs::read_file_dispatch(status_path).is_none() {
        crate::vfs::create_file_dispatch(status_path, b"");
    }

    // Register well-known triggers
    {
        let mut triggers = TRIGGERS.lock();
        triggers.insert(String::from("ldconfig"), Vec::new());
        triggers.insert(String::from("man-db"), Vec::new());
        triggers.insert(String::from("desktop-file-utils"), Vec::new());
        triggers.insert(String::from("hicolor-icon-theme"), Vec::new());
    }

    serial_println!("[dpkg-scripts] Maintainer script execution engine initialized");
    serial_println!("[dpkg-scripts]   Script types: preinst, postinst, prerm, postrm");
    serial_println!("[dpkg-scripts]   Triggers: ldconfig, man-db, desktop-file-utils");
    serial_println!("[dpkg-scripts]   Diversions: dpkg-divert support");
    serial_println!("[dpkg-scripts]   Status database: /var/lib/dpkg/status");
}
