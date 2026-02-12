## Changelog

### v3.8 — February 12, 2026

**System Monitor: Network Tab** (`wasm_apps/system_monitor/src/main.rs` — ENHANCED):

New Network tab added to System Monitor (5 tabs total: Overview, Performance, Processes, Network, Scheduler):

- **Protocol Statistics** — Live display of TCP, UDP, ICMP, and DNS counters via `guestlib::netstats_protocol()`
- **Network Interfaces** — Interface listing (eth0, lo, etc.) with status via `guestlib::netstats_interfaces()`
- **Active Connections** — Real-time connection table showing state (ESTABLISHED=green, LISTEN=yellow) via `guestlib::netstats_connections()`
- **Auto-refresh** — Network stats cached and refreshed every 2 seconds for efficient polling
- **Color-coded display** — Protocol names highlighted in blue, interfaces in green, connection states color-coded

**Settings: Power Management Tab** (`wasm_apps/settings/src/main.rs` — ENHANCED):

New Power tab added to Settings (7 tabs total: About, Appearance, System, Security, Keyboard, Apps, Power):

- **System Uptime** — Live uptime counter formatted as hours/minutes/seconds
- **Sync Filesystems** — Button to flush all pending writes to FAT32 disk via `guestlib::sync_filesystems()`
- **Restart System** — Graceful reboot button (blue, calls `guestlib::request_reboot()`) with hover highlighting
- **Shut Down** — Graceful shutdown button (red, calls `guestlib::request_shutdown()`) with hover highlighting
- **System Information** — Power subsystem details (ACPI S5 shutdown, keyboard controller reset, FAT32 persistence, auto-save intervals)
- **Interactive buttons** — Click-to-activate with visual hover feedback using `draw_rounded_rect`

### v3.7 — February 12, 2026

**TLS 1.2 Crypto Engine** (`kernel/src/tls.rs` — NEW, 700+ lines):

Complete TLS 1.2 client implementation with from-scratch cryptographic primitives, no external crates:

- **SHA-256** hash function — Full FIPS-180 implementation with padding, 64-round compression
- **HMAC-SHA256** — RFC 2104 compliant keyed-hash message authentication
- **AES-128** cipher — Complete AES with:
  - Key expansion (16-byte key → 176-byte round keys)
  - Block encrypt/decrypt (10 rounds: SubBytes, ShiftRows, MixColumns, AddRoundKey)
  - GF(2^8) multiplication for MixColumns/InvMixColumns
  - Full S-Box and inverse S-Box lookup tables
  - CBC mode encryption/decryption with PKCS#7 padding
- **TLS 1.2 Record Layer** — Content type parsing (Handshake, ChangeCipherSpec, Alert, ApplicationData)
- **TLS 1.2 Handshake Protocol**:
  - ClientHello builder with version, cipher suites, SNI extension, signature algorithms
  - ServerHello/Certificate/ServerHelloDone processing
  - ClientKeyExchange builder with pre-master secret
  - ChangeCipherSpec and Finished message handling
  - Verify data computation for handshake integrity
- **TLS PRF (P_SHA256)** — Key derivation function producing arbitrary-length key material
- **Key Derivation** — master_secret from pre_master_secret + randoms, key block expansion to:
  - Client/server MAC keys (32 bytes each, SHA-256)
  - Client/server write keys (16 bytes each, AES-128)
  - Client/server IVs (16 bytes each)
- **Record Encryption/Decryption** — AES-128-CBC with HMAC-SHA256 MAC, sequence number tracking
- **Session Management** — TlsSession struct with full handshake state machine
- **Cipher Suite** — TLS_RSA_WITH_AES_128_CBC_SHA256 (0x003C)
- Terminal curl/wget updated to report TLS engine availability for HTTPS URLs

**WASI Filesystem VFS Integration** (`kernel/src/wasm/mod.rs` — ENHANCED):

WASM applications can now transparently access both ramfs and FAT32 persistent storage through the WASI filesystem API:

- **path_open VFS routing** — Paths starting with `/disk/` are automatically routed through the VFS layer:
  - Files on FAT32 are loaded into ramfs for fd-based access (read/write/seek)
  - File creation (`O_CREAT`) writes through VFS to FAT32
  - Directory opens handle VFS stat verification
- **path_create_directory VFS routing** — `mkdir` on `/disk/` paths creates directories on FAT32
- **path_filestat_get VFS routing** — `stat` on `/disk/` paths queries FAT32 metadata via VFS
- **path_remove_directory VFS routing** — `rmdir` on `/disk/` paths removes FAT32 directories
- **path_unlink_file VFS routing** — `unlink` on `/disk/` paths deletes FAT32 files
- **New WASI functions implemented**:
  - `fd_tell` — Returns current file cursor position (seek offset=0, whence=CUR)
  - `fd_advise` — Advisory file access pattern hint (no-op success)
  - `fd_allocate` — Pre-allocate file space (extends file if offset+len > current size)

**USB/NVMe/AHCI Block Device I/O Layer** (`kernel/src/hardware.rs` — ENHANCED, 300+ lines added):

Added a unified block device abstraction and actual I/O command paths for all three storage controller types:

- **BlockDevice trait** — Unified interface with:
  - `read_sectors(lba, count, buf)` — Read sectors from device
  - `write_sectors(lba, count, buf)` — Write sectors to device
  - `sector_count()` — Total device capacity
  - `sector_size()` — Sector size (default 512)
  - `device_name()` — Human-readable device identifier
  - `flush()` — Write cache flush
- **Block Device Registry** — `BLOCK_DEVICES` global with `BlockDeviceEntry` records:
  - Device name, type (NVMe/AHCI/VirtIO), total sectors, sector size
  - `block_device_summary()` — Human-readable device listing
- **NVMe Block I/O** — `NvmeController::read_block()` and `write_block()`:
  - Builds NVMe Read/Write commands via `nvme_read_cmd`/`nvme_write_cmd`
  - Buffer size validation, controller state checks
  - Ready for I/O queue pair submission (admin queue operational)
- **AHCI Block I/O** — `AhciController::read_block()`, `write_block()`, and `flush_cache()`:
  - Builds FIS for READ DMA EXT (0x25) and WRITE DMA EXT (0x35)
  - Port busy check (BSY/DRQ bits in Task File Data register)
  - FLUSH CACHE EXT (0xEA) command for write-back
  - Per-port device validation and command engine status check
- **Driver Registration** — `init_full_drivers()` now registers discovered devices:
  - NVMe namespaces registered as block devices after identify
  - AHCI SATA devices registered per-port after command engine start
- **Block Device Types** — `BlockDeviceType` enum: NvmeNamespace, AhciSata, VirtioBlock

**Proportional Font Rendering** (ALREADY COMPLETE — `applib/src/drawing/proportional.rs`):

The proportional (variable-width) font rendering system was already fully implemented in a previous version. STATUS.md "remaining" entry was stale. Capabilities include:

- `ProportionalMetrics` — Per-font advance widths, left bearings, kerning pairs
- `from_font()` — Automatic metrics analysis from bitmap font data
- `char_width()` / `text_width()` — Character and string width measurement
- `kern()` — Kerning pair lookup for tight letter spacing
- `draw_str_proportional()` — Variable-width text rendering with kerning
- `compute_proportional_bbox()` — Bounding box calculation for layout
- `draw_proportional_in_rect()` — Bounded proportional text rendering
- `get_metrics()` — Cached metrics lookup per font instance
- Integrated with the TrueType rasterizer (`kernel/src/truetype.rs`, 1182 lines)

### v3.6 — February 12, 2026

**Real HTTP/1.1 Client** (`wasm_apps/terminal/src/shell/commands_net.rs` — REWRITTEN):

The `curl` and `wget` commands previously only performed DNS lookups and displayed a "Full HTTP client not yet available" message. They have been completely rewritten with a real HTTP/1.1 client using the kernel TCP stack and DNS resolver.

- `http_get(host, port, path)` — Real HTTP/1.1 GET request engine:
  - DNS resolution via `guestlib::dns_resolve()`
  - TCP connection via `guestlib::tcp_connect()`
  - Proper HTTP/1.1 request with `Host`, `User-Agent`, `Accept`, `Connection: close` headers
  - Response parsing: status line, headers, body extraction
  - Chunked transfer encoding decoder (`decode_chunked()`)
  - Timeout handling (10-second deadline per request)
  - Returns `(status_code, headers, body)` tuple
- `parse_url(url)` — URL parser supporting `http://host[:port][/path]` format
- `decode_chunked(data)` — Chunked transfer encoding decoder per RFC 7230

**curl — Enhanced** (`wasm_apps/terminal/src/shell/commands_net.rs`):

- `curl <url>` — Fetch and display HTTP response body
- `-v` / `--verbose` — Show request and response headers
- `-I` / `--head` — Show response headers only (HEAD-like)
- `-s` / `--silent` — Suppress progress/status output
- `-o <file>` / `--output <file>` — Save response body to file
- `-L` / `--location` — Follow HTTP redirects (up to 5 hops via Location header)
- Error handling: connection failures, DNS resolution errors, timeouts

**wget — Enhanced** (`wasm_apps/terminal/src/shell/commands_net.rs`):

- `wget <url>` — Download file from URL and save to local filesystem
- Auto-generates filename from URL path (defaults to `index.html`)
- `-q` / `--quiet` — Suppress progress output
- `-O <file>` / `--output-document <file>` — Specify output filename
- Displays download size and save path

**New Network Commands** (`wasm_apps/terminal/src/shell/commands_net.rs`):

- `traceroute <host>` — Trace network route to host
  - DNS resolution + 30-hop simulation with realistic RTT values
  - Displays hop number, hostname/IP, and three RTT measurements
- `nc` / `netcat` — TCP connection and port scanning tool
  - `nc <host> <port>` — Open TCP connection and report success/failure
  - `nc -z <host> <port_start>[-<port_end>]` — Port scan (range supported)
  - DNS resolution, TCP connect probe, connection timing
- `dig <hostname>` — DNS lookup utility (dig-style output)
  - Displays query, answer section with A record, query time, server info
  - Formatted like standard `dig` output with section headers
- `route` — Display kernel routing table
  - Shows destination, gateway, netmask, flags, metric, interface
  - Default route (0.0.0.0) and local subnet entries
- `arp` — Display ARP table
  - Shows hostname, IP address, MAC/HW address, interface
  - Gateway and local entry display

**AWK Text Processing** (`wasm_apps/terminal/src/shell/commands_text.rs` — NEW):

Full `awk` implementation with pattern scanning and text processing:

- Field splitting: whitespace (default) or custom separator via `-F`
- Pattern types: `BEGIN`, `END`, `/regex/`, conditions (`NR>2`, `$1=="foo"`)
- Built-in variables: `$0` (whole line), `$1`..`$N` (fields), `NR` (line number), `NF` (field count), `FS`, `OFS`, `ORS`
- Statements: `print`, variable assignment (`=`, `+=`, `-=`, `++`)
- Built-in functions: `length()`, `toupper()`, `tolower()`
- Arithmetic expressions with `+`, `-` operators
- Multiple pattern-action blocks with semicolon statement separators
- Examples: `awk '{print $1}' file.txt`, `awk -F: '{print $1, $3}' /etc/passwd`, `awk 'BEGIN{sum=0} {sum+=$1} END{print sum}' nums.txt`

**Manual Pages System** (`wasm_apps/terminal/src/shell/commands_text.rs` — NEW):

Built-in `man` command with Unix-style manual pages:

- `man <command>` — Display formatted manual page with NAME, SYNOPSIS, DESCRIPTION, OPTIONS, EXAMPLES, SEE ALSO sections
- `man -k <keyword>` — Search manual page names and descriptions (apropos)
- `man -l` — List all available manual pages organized by section
- Manual sections: 1 (User Commands), 7 (Miscellaneous), 8 (System Administration)
- 60+ manual page entries covering all major commands
- Detailed pages for: curl, awk, grep, ls, vim, ping, shutdown, systemctl, man
- ANSI formatting: bold titles, underlined parameters, section headers

**Tab Completion** (`wasm_apps/terminal/src/shell/completion.rs`):

- Added 12 new commands: awk, gawk, nawk, man, info, traceroute, tracepath, nc, netcat, dig, route, arp

**Shell Dispatch** (`wasm_apps/terminal/src/shell/mod.rs`):

- Added command routing for all new commands:
  - `awk` | `gawk` | `nawk` → `cmd_awk()`
  - `man` | `info` → `cmd_man()`
  - `traceroute` | `tracepath` → `cmd_traceroute()`
  - `nc` | `netcat` → `cmd_nc()`
  - `dig` → `cmd_dig()`
  - `route` → `cmd_route()`
  - `arp` → `cmd_arp()`

### v3.5 — February 12, 2026

**Timezone Manager** (`kernel/src/timezone.rs` — NEW):

- `TimezoneManager` with UTC offset-based timezone support
- 22 named timezone presets covering all major world regions
- `set_by_name()` — Set timezone by name (e.g., "America/New_York", "Europe/London", "Asia/Tokyo")
- `set_offset()` — Set timezone by raw UTC offset in minutes
- `apply_offset()` — Apply timezone offset to a `chrono::DateTime<Utc>` value
- `format_offset()` — Human-readable UTC offset string (e.g., "UTC-05:00")
- `format_list()` — Formatted list of all available timezones with offsets
- Persistence via `crate::preferences::PREFERENCES` — timezone saved and restored across reboots
- `init()` — Loads saved timezone preference on boot
- Global `TIMEZONE: Mutex<TimezoneManager>` with module-level convenience functions

**Clipboard History Ring** (`kernel/src/clipboard.rs` — ENHANCED):

- Added `VecDeque<String>` history ring buffer (10 entries max)
- `set_text()` now automatically pushes to history (with deduplication of top entry)
- `history_count()` — Number of entries in the ring
- `history_get(index)` — Retrieve specific entry (0 = most recent)
- `history_select(index)` — Restore a historical entry to the active clipboard
- `clear_history()` — Clear all history entries
- `format_history()` — Formatted display with truncated previews (50 chars)

**Real-Time Clock API for WASM Apps** (`kernel/src/wasm/mod.rs`):

- `host_get_datetime(buf_ptr)` — Read kernel RTC with timezone applied; writes 7×i32 [year, month, day, hour, minute, second, tz_offset_minutes]
- `host_get_timezone(buf_ptr, buf_len)` — Get current timezone name string
- `host_set_timezone(name_ptr, name_len)` — Set timezone (restricted to terminal/settings apps)
- `host_timezone_list(buf_ptr, buf_len)` — Get formatted list of all available timezones

**Clipboard History API** (`kernel/src/wasm/mod.rs`):

- `host_clipboard_history_count()` — Number of clipboard history entries
- `host_clipboard_history_get(index, buf_ptr, buf_len)` — Get specific history entry
- `host_clipboard_history_select(index)` — Restore history entry to active clipboard
- `host_clipboard_history_list(buf_ptr, buf_len)` — Formatted clipboard history listing

**Process Priority API** (`kernel/src/wasm/mod.rs`):

- `host_set_priority(name_ptr, name_len, priority)` — Set scheduling priority for a named process (Low/Normal/High)
- `scheduler::set_priority()` updated to return `bool` for success/failure reporting

**WASI Stability Fixes** (`kernel/src/wasm/mod.rs`):

- `path_filestat_set_times` — Changed from panicking stub to graceful `ERRNO_NOTSUP` (76) return
- `fd_fdstat_set_flags` — Changed from panicking stub to graceful `ERRNO_NOTSUP` (76) return
- Prevents WASM app crashes when Rust std library touches file metadata timestamps

**Guestlib Wrappers** (`guestlib/src/lib.rs`):

- `DateTimeInfo` struct with year/month/day/hour/minute/second/tz_offset fields
  - `format()` — "YYYY-MM-DD HH:MM:SS TZ" display
  - `format_date()` / `format_time()` — Date-only and time-only formatters
  - `weekday()` — Day of week (Tomohiko Sakamoto's algorithm)
  - `weekday_name()` / `month_name()` / `month_name_full()` — Human-readable names
  - `days_in_month()` / `is_leap_year()` — Calendar arithmetic
- `get_datetime()` → `Option<DateTimeInfo>` — Get RTC date/time with timezone
- `get_timezone()` → `String` — Get current timezone name
- `set_timezone(name)` → `Result<(), &str>` — Set timezone
- `timezone_list()` → `String` — List available timezones
- `clipboard_history_count()` → `i32`
- `clipboard_history_get(index)` → `Option<String>`
- `clipboard_history_select(index)` → `bool`
- `clipboard_history_list()` → `String`
- `set_priority(name, priority)` → `Result<(), &str>` — Set process scheduling priority

**Terminal Commands — Fixed** (`wasm_apps/terminal/src/shell/`):

- `date` — Now shows real RTC date/time with timezone (e.g., "Wednesday Jan 15 14:30:22 America/New_York 2026") instead of boot-relative uptime
- `env` — Now shows kernel environment variables from `guestlib::env_list()` plus local shell variables instead of hardcoded values
- `cal` — Now renders a real calendar for the current month from RTC with today highlighted (reverse video)
- `neofetch` — Updated version display from "Knox OS 3.2" to "Knox OS 3.5"

**Terminal Commands — New** (`wasm_apps/terminal/src/shell/`):

- `tz` / `timezone` / `timedatectl` — Timezone management
  - (no args) — Show current timezone and local time
  - `list` — List all 22 available timezones with UTC offsets
  - `set <name>` — Set timezone (e.g., `tz set America/New_York`)
- `nice` / `renice` — Process priority management
  - `<process> <priority>` — Set priority (low/normal/high or 0/1/2)
  - Examples: `nice terminal high`, `renice file_manager low`
- `clipring` / `cliphistory` / `cliphist` — Clipboard history ring
  - (no args) / `list` — Show clipboard history with entry indices
  - `count` — Show number of history entries
  - `get <N>` — Show specific history entry
  - `select <N>` — Restore history entry to active clipboard

**Tab Completion** (`wasm_apps/terminal/src/shell/completion.rs`):

- Added 8 new commands: tz, timezone, timedatectl, nice, renice, clipring, cliphistory, cliphist

**Kernel Integration** (`kernel/src/main.rs`):

- Module declaration: `timezone` (with `#[allow(dead_code)]`)
- Init sequence: `timezone::init()` after `mounts::init()`

### v3.4 — February 12, 2026

**Shared Memory IPC** (`kernel/src/shmem.rs` — NEW):

- `SharedMemoryManager` with `BTreeMap<String, SharedRegion>` for named shared memory regions
- Configurable region size: 4 KB (MIN) to 16 MB (MAX), 64 MB total system limit
- `ShmAccess::ReadOnly` / `ShmAccess::ReadWrite` per-attachment access modes
- Reference-counted attachments: multiple apps attach/detach independently
- Owner-based permission model: only creator can destroy region
- Operations: `create()`, `destroy()`, `attach()`, `detach()`, `read()`, `write()`
- Per-region statistics: read/write counts, bytes transferred, attachment count
- `format_list()` — ipcs-style formatted output for terminal display
- Global `SHM_MANAGER: Mutex<SharedMemoryManager>` with module-level convenience functions

**Resource Limits** (`kernel/src/rlimits.rs` — NEW):

- `RlimitManager` with per-app `AppLimits` isolation via `BTreeMap`
- 8 `Resource` types: Memory, CpuTime, OpenFiles, IpcMessages, Sockets, Pipes, SharedMemory, StackSize
- Soft/hard limit pairs (`Rlimit` struct) — soft limits can be raised up to hard limit
- Default limits: Memory 32MB/64MB, CpuTime 25ms/50ms, OpenFiles 64/256, Sockets 8/16, etc.
- `set_soft()` / `set_hard()` with validation (soft ≤ hard)
- `would_exceed()` for pre-check before resource acquisition
- `format_limits()` — ulimit -a style per-app output
- `format_summary()` — overview of all apps' resource usage
- Global `RLIMIT_MANAGER: Mutex<RlimitManager>` with module-level convenience functions

**Service Manager** (`kernel/src/services.rs` — NEW):

- `ServiceManager` with `BTreeMap<String, Service>` for systemd-like service tracking
- `ServiceState` enum: Starting, Running, Stopping, Stopped, Failed, Disabled
- `ServiceType` enum: Kernel, App, Timer
- Dependency tracking via `add_dependency()` — service won't start if dependencies aren't running
- Auto-restart with configurable max retries (default: 3)
- 15 built-in kernel services (memory, filesystem, network, display, audio, etc.) auto-started at boot
- 6 app services (terminal, text_editor, web_browser, file_manager, system_monitor, settings)
- `format_list()` — systemctl list-units style output with color-coded states
- `format_status()` — systemctl status style detailed view with uptime
- Global `SERVICE_MANAGER: Mutex<ServiceManager>` with module-level convenience functions

**Virtual /proc Filesystem** (`kernel/src/procfs.rs` — NEW):

- `ProcFs` with 18 `ProcGenerator` entries for dynamic content generation
- Entries: version, uptime, loadavg, meminfo, cpuinfo, stat, mounts, filesystems, net/dev, net/tcp, net/udp, interrupts, cmdline, hostname, modules, swaps, diskstats, partitions
- Each entry generates real data from kernel state (allocator stats, timer ticks, network info, etc.)
- `read(path)` → `Option<String>` to read any /proc entry
- `list_entries()` — returns (name, description) pairs for directory listing
- `format_directory()` — ls -la style /proc directory display
- Global `PROC_FS: Mutex<ProcFs>` with module-level convenience functions

**Mount Subsystem** (`kernel/src/mounts.rs` — NEW):

- `MountTable` with `BTreeMap<String, MountEntry>` for filesystem mount tracking
- `mount()` / `unmount()` operations with validation (no duplicate mounts, can't unmount /)
- Auto-populated at boot: ramfs→/, FAT32→/disk (if persistent storage available), proc→/proc, tmpfs→/tmp
- `fstype_for_path()` — resolve filesystem type for any path (longest prefix match)
- `format_mounts()` — /etc/mtab style mount listing
- `format_fstab()` — /etc/fstab style static table
- `format_df()` — df command output with actual usage from allocator/FAT32
- Global `MOUNT_TABLE: Mutex<MountTable>` with module-level convenience functions

**WASM Host APIs** (`kernel/src/wasm/mod.rs`):

- `host_shm_create(name_ptr, name_len, size)` — Create shared memory region (returns 0/-1/-2/-3/-4)
- `host_shm_destroy(name_ptr, name_len)` — Destroy shared memory region
- `host_shm_attach(name_ptr, name_len, mode)` — Attach to region (mode: 0=RO, 1=RW)
- `host_shm_write(name_ptr, name_len, offset, data_ptr, data_len)` — Write to shared memory
- `host_shm_read(name_ptr, name_len, offset, buf_ptr, buf_len)` — Read from shared memory
- `host_shm_list(buf_ptr, buf_len)` — List all shared memory regions (formatted)
- `host_shm_count()` — Get shared memory region count
- `host_rlimit_get(buf_ptr, buf_len)` — Get resource limits for calling app (formatted)
- `host_rlimit_set(resource_id, value)` — Set soft resource limit (0-7 resource IDs)
- `host_rlimit_summary(buf_ptr, buf_len)` — Get resource limits summary for all apps
- `host_service_list(buf_ptr, buf_len)` — List all services (systemctl format)
- `host_service_status(name_ptr, name_len, buf_ptr, buf_len)` — Get service status
- `host_service_control(name_ptr, name_len, action)` — Control service (0=start/1=stop/2=restart/3=enable/4=disable)
- `host_service_count()` — Get total service count
- `host_procfs_read(path_ptr, path_len, buf_ptr, buf_len)` — Read /proc entry
- `host_procfs_list(buf_ptr, buf_len)` — List /proc entries
- `host_mount_list(buf_ptr, buf_len)` — Get mount table
- `host_mount_df(buf_ptr, buf_len)` — Get df-style disk usage
- `host_mount_count()` — Get mount count

**Guestlib Wrappers** (`guestlib/src/lib.rs`):

- `shm_create(name, size)` → `Result<(), &str>` — Create shared memory region
- `shm_destroy(name)` → `Result<(), &str>` — Destroy shared memory region
- `shm_attach(name, read_write)` → `Result<(), &str>` — Attach to region
- `shm_write(name, offset, data)` → `Result<usize, &str>` — Write data to shared memory
- `shm_read(name, offset, max_len)` → `Result<Vec<u8>, &str>` — Read data from shared memory
- `shm_list()` → `String` — List shared memory regions (8KB buffer)
- `shm_count()` → `i32`
- `rlimit_get()` → `String` — Get resource limits for calling app (4KB buffer)
- `rlimit_set(resource_id, value)` → `Result<(), &str>` — Set soft resource limit
- `rlimit_summary()` → `String` — Resource limits summary for all apps (8KB buffer)
- `service_list()` → `String` — List all services (8KB buffer)
- `service_status(name)` → `String` — Service status (4KB buffer)
- `service_control(name, action)` → `Result<(), &str>` — Control service
- `service_start/stop/restart/enable/disable(name)` — Convenience wrappers
- `service_count()` → `i32`
- `procfs_read(path)` → `Option<String>` — Read /proc entry (8KB buffer)
- `procfs_list()` → `String` — List /proc directory (4KB buffer)
- `mount_list()` → `String` — Mount table (4KB buffer)
- `mount_df()` → `String` — Disk usage output (4KB buffer)
- `mount_count()` → `i32`

**Terminal Commands** (`wasm_apps/terminal/src/shell/`):

- `systemctl` / `service` — Service management (systemd-like)
  - `list` — Show all services with state/type/uptime
  - `status <name>` — Detailed service status
  - `start/stop/restart <name>` — Control service state
  - `enable/disable <name>` — Toggle auto-start
  - Color-coded output: green=running, yellow=stopped, red=failed
- `ipcs` / `shmem` — Shared memory segment listing (ipcs-style)
- `ulimit` / `rlimit` — Resource limit display and management
  - `-a` — Show all limits (default)
  - `--summary` / `-s` — Summary of all apps
  - `-m/-t/-n/-i/-s/-p <value>` — Set specific limit
- `proc` / `procfs` — Virtual /proc filesystem browser
  - `ls` / (no args) — List /proc entries
  - `<path>` — Read specific entry (e.g., `proc meminfo`, `proc cpuinfo`)
- `mount` / `mounts` — Enhanced with real mount subsystem data
- `df` — Enhanced with real filesystem usage from mount subsystem

**Tab Completion** (`wasm_apps/terminal/src/shell/completion.rs`):

- Added 8 new commands: systemctl, service, ipcs, shmem, ulimit, rlimit, proc, procfs

**Kernel Integration** (`kernel/src/main.rs`):

- Module declarations: `shmem`, `rlimits`, `services`, `procfs`, `mounts` (with `#[allow(dead_code)]`)
- Init sequence after `netstats::init()`: `shmem::init()` → `rlimits::init()` → `procfs::init()` → `mounts::init()`
- App registration: `rlimits::register_app()` for each loaded WASM app
- Service manager: `services::init(clock.time())` after app registration

**Test Suite** (`kernel/src/tests.rs`):

- `test_shmem()` — 14 tests: create/destroy, attach/detach, read/write, readonly permissions, size validation, format output
- `test_rlimits()` — 13 tests: register, defaults, from_id, set_soft, hard limit enforcement, usage tracking, check_limit, format output
- `test_services()` — 11 tests: register, duplicate prevention, start/stop, already-running detection, enable/disable, nonexistent service, format output
- `test_procfs()` — 11 tests: entry count, read version/cpuinfo/meminfo/uptime/mounts/hostname, nonexistent entry, format directory
- `test_mounts()` — 12 tests: mount/unmount, duplicate prevention, get entry, fstype resolution, root unmount prevention, format output

### v3.3 — February 12, 2026

**Environment Variable Manager** (`kernel/src/env.rs` — NEW):

- `EnvironmentManager` with per-app `AppEnvironment` isolation via `BTreeMap`
- 15 system defaults per app: SHELL, HOME, TERM, LANG, PATH, USER, LOGNAME, HOSTNAME, OSTYPE, MACHTYPE, PWD, RUST_BACKTRACE, APP_NAME, XDG_RUNTIME_DIR, LC_ALL
- `set()` with validation (alphanumeric + underscore names, MAX_VARS=128, MAX_DATA=16KB)
- `get()`, `unset()`, `list()`, `count()`, `format_all()` methods on AppEnvironment
- System-wide overrides via `set_system_override()` — propagates to all apps
- Read-only variable protection, inherited vs user-set tracking
- Global `ENV_MANAGER: Mutex<EnvironmentManager>` with module-level convenience functions

**Process Table** (`kernel/src/proc_table.rs` — NEW):

- `ProcessTable` with `BTreeMap<u32, ProcessInfo>` + name-to-PID index
- `ProcessState` enum: Running, Sleeping, Stopped, Zombie, Starting
- PID 0 = `[kernel/idle]`, PID 1 = `[kernel/init]`, user apps start at PID 100
- Per-process tracking: CPU time (ms), last step duration, memory bytes, open FDs, sockets, IPC counters, nice value, TTY
- `register()` / `set_state()` / `record_step()` / `set_memory()` / `mark_exited()` / `reap_zombies()` lifecycle
- `format_ps()` — `ps aux` style table output
- `format_pstree()` — hierarchical tree view with parent-child connectors
- Global `PROCESS_TABLE: Mutex<ProcessTable>` with module-level convenience functions

**System Event Log** (`kernel/src/event_log.rs` — NEW):

- `EventLog` with 1024-entry `VecDeque` ring buffer
- 6 categories: AUTH, PROC, NET, FS, SYS, SEC (`EventCategory` enum)
- 5 severity levels: INFO, NOTICE, WARN, ERROR, CRIT (`EventSeverity` enum with Ord)
- Monotonic sequence numbers + millisecond timestamps via `SystemClock::elapsed_ms()`
- `record()` with timestamp/category/severity/source/message
- Query: `tail(n)`, `by_category()`, `by_severity()`, `by_source()`
- Format: `format_last()`, `format_category()`, `summary()` per-category counts
- Convenience functions: `auth_event()`, `process_event()`, `network_event()`, `fs_event()`, `system_event()`, `security_event()`
- Global `EVENT_LOG: Mutex<EventLog>` with `try_lock()` to avoid deadlocks

**Network Statistics Manager** (`kernel/src/netstats.rs` — NEW):

- `NetStatsManager` with per-interface `InterfaceStats` (rx/tx bytes/packets/errors/dropped, MAC, IPv4/IPv6, MTU)
- `ConnectionEntry` for active connection tracking (protocol, local/remote addr, state, app name)
- `ProtocolStats`: TCP (connections/active/segments/retransmits), UDP (datagrams), ICMP, DNS (queries/cache hits/misses), firewall (allowed/dropped)
- `ThroughputSample` history (120 samples at 1/sec) for historical bandwidth tracking
- Default interfaces: eth0 (VirtIO) + lo (loopback)
- `record_tcp_connect()` / `record_tcp_close()` / `record_icmp_sent()` / `record_dns_query()` event hooks
- Format: `format_stats()` (protocol summary), `format_interfaces()` (ifconfig-style), `format_connections()` (ss/netstat-style)
- Global `NET_STATS: Mutex<NetStatsManager>` with module-level convenience functions

**WASM Host APIs** (`kernel/src/wasm/mod.rs`):

- `host_env_get(key_ptr, key_len, buf_ptr, buf_len)` — Get environment variable for calling app
- `host_env_set(key_ptr, key_len, val_ptr, val_len)` — Set environment variable (returns 0/-1)
- `host_env_list(buf_ptr, buf_len)` — List all env vars as KEY=VALUE lines
- `host_env_count()` — Count of environment variables for calling app
- `host_proc_list(buf_ptr, buf_len)` — Get ps-formatted process list
- `host_proc_tree(buf_ptr, buf_len)` — Get pstree-formatted process tree
- `host_proc_count()` — Total process count
- `host_proc_getpid()` — Get PID of calling app
- `host_event_log_read(buf_ptr, buf_len, count, category)` — Read journal entries (category -1=all, 0-5=specific)
- `host_event_log_count()` — Total event log entry count
- `host_netstats(buf_ptr, buf_len, mode)` — Network stats (mode 0=protocol, 1=interfaces, 2=connections)

**Guestlib Wrappers** (`guestlib/src/lib.rs`):

- `env_get(key)` → `Option<String>` (8KB buffer)
- `env_set(key, value)` → `Result<(), &str>`
- `env_list()` → `String` (16KB buffer, KEY=VALUE lines)
- `env_count()` → `i32`
- `proc_list()` → `String` (16KB buffer, ps-formatted)
- `proc_tree()` → `String` (16KB buffer, pstree-formatted)
- `proc_count()` → `i32`
- `proc_getpid()` → `i32`
- `event_log_read(count, category)` → `String` (16KB buffer, journalctl-formatted)
- `event_log_count()` → `i32`
- `netstats_protocol()` → `String` (protocol stats)
- `netstats_interfaces()` → `String` (interface list)
- `netstats_connections()` → `String` (connection table)

**Terminal Commands** (`wasm_apps/terminal/src/shell/`):

- `journalctl` / `journal` — Read system event journal with filtering
  - `-n N` — Show last N entries (default 20)
  - `-u CATEGORY` — Filter by category (auth/proc/net/fs/sys/sec)
  - `--disk-usage` — Show event count and summary
  - `-f` — Simulated follow mode (shows recent entries)
  - Colorized output: red=CRIT/ERROR, yellow=WARN, cyan=NOTICE, default=INFO
- `pstree` — Display process hierarchy tree with PID and state
- `lsof` — List open file descriptors per process
- `last` / `lastlog` — Show login history and session events
- `getpid` / `pidof` — Display PID of the calling terminal app

**Time Module Enhancement** (`kernel/src/time.rs`):

- Added `SystemClock::elapsed_ms()` — static method using rdtsc with ~2GHz approximation
- Used by EventLog for accurate timestamps without requiring SystemClock instance

**Kernel Integration** (`kernel/src/main.rs`):

- Module declarations: `env`, `event_log`, `netstats`, `proc_table` (with `#[allow(dead_code)]`)
- Init sequence after `cron::init()`: `env::init()` → `proc_table::init()` → `event_log::init()` → `netstats::init()`
- App registration loop: `env::register_app()` and `proc_table::register_app()` for each loaded WASM app
- Boot complete event: `event_log::system_event("Knox OS kernel boot complete")`

**Tab Completion** (`wasm_apps/terminal/src/shell/completion.rs`):

- Added 8 new commands: journalctl, journal, pstree, lsof, last, lastlog, getpid, pidof

**Test Suite** (`kernel/src/tests.rs`):

- `test_env()` — 12 tests: registration, defaults, set/get/overwrite/unset, list, format, count
- `test_proc_table()` — 16 tests: initial count, register, PID lookup, state, record_step, ps/pstree format, zombie lifecycle
- `test_event_log()` — 14 tests: add/read, category/severity filters, tail, format, summary, clear
- `test_netstats()` — 16 tests: protocol counters, TCP connect/close, ICMP, DNS, interface counters, format outputs

### v3.2 — February 12, 2026

**Kernel Log Ring Buffer** (`kernel/src/klog.rs` — NEW):

- 512-entry ring buffer (`VecDeque<LogEntry>`) capturing all kernel log messages
- Each entry stores: timestamp (ms since boot), log level (1-5), module name, message text
- `push()` / `tail(n)` / `filtered(count, max_level)` / `format_last(n)` methods
- Global `KERNEL_LOG: Mutex<KernelLog>` with `try_lock()` to prevent deadlocks during logging
- Integrated with `SerialLogger::log()` so every log message is captured automatically
- Real `dmesg` command now reads actual kernel boot/runtime messages

**Hostname Management** (`kernel/src/hostname.rs` — NEW):

- `HostnameManager` with `get()` / `set()` backed by persistent preferences
- Validation: 1-64 characters, alphanumeric + hyphens + dots, no leading/trailing hyphens
- Default hostname: "knox"; persists across reboots via preferences system
- Host APIs: `host_get_hostname`, `host_set_hostname` (returns 0/success, -1/invalid, -2/too long)

**Load Average Tracking** (`kernel/src/loadavg.rs` — NEW):

- Exponential weighted moving average (EWMA) for 1/5/15-minute load averages
- Decay constants: 0.9835 (1min), 0.9967 (5min), 0.9989 (15min) — matches Unix semantics
- Sampled every 1000ms from scheduler task count
- Global `LOAD_TRACKER: Mutex<LoadTracker>`, updated in kernel main loop
- Host API: `host_get_loadavg` returns 3×i32 (load × 100 for fixed-point transport)

**Cron-like Task Scheduler** (`kernel/src/cron.rs` — NEW):

- `CronScheduler` with periodic kernel-level tasks (not user-configurable yet)
- 5 built-in tasks:
  - `fs_sync` (30s) — flush filesystem buffers to disk
  - `dns_cleanup` (60s) — expire stale DNS cache entries
  - `load_sample` (1s) — sample CPU load for averages
  - `pref_autosave` (5min) — auto-save user preferences
  - `log_rotate` (2min) — trim kernel log ring buffer
- Global `CRON: Mutex<CronScheduler>`, ticked from kernel main loop

**WASM Host APIs** (`kernel/src/wasm/mod.rs`):

- `host_dmesg(buf, len, count, level)` — Read kernel log entries filtered by level
- `host_dmesg_count()` — Total kernel log entry count
- `host_dmesg_clear()` — Clear log buffer (terminal/settings apps only)
- `host_get_hostname(buf, len)` — Get system hostname
- `host_set_hostname(name, len)` — Set hostname with validation
- `host_get_loadavg(buf)` — Get 1/5/15-minute load averages
- `host_cron_task_count()` — Number of active cron tasks

**Guestlib Wrappers** (`guestlib/src/lib.rs`):

- `dmesg(count, max_level)` → `String` (16KB buffer, formatted output)
- `dmesg_count()` → `i32`
- `dmesg_clear()` → `bool`
- `get_hostname()` → `String`
- `set_hostname(name)` → `Result<(), &str>`
- `get_loadavg()` → `LoadAverage { load_1min, load_5min, load_15min: f64 }`
- `cron_task_count()` → `i32`

**Terminal Commands** (`wasm_apps/terminal/src/shell/`):

- `dmesg` — Now reads **real kernel log** from ring buffer with colorized output (red=error, yellow=warn, gray=debug)
- `hostname [name]` — Show or set system hostname (with validation feedback)
- `top` / `htop` — Process/task monitor with scheduler stats (PID, priority, CPU time, preemptions, quantum)
- `lsblk` — List block devices (VirtIO Block + FAT32)
- `lsmod` — List loaded kernel modules (virtio_gpu, virtio_net, wasmi, smoltcp, etc.)
- `crontab [-l]` — List scheduled kernel tasks with intervals
- `sysctl [-a] [key]` — View kernel parameters (hostname, ostype, loadavg, nproc, display, etc.)
- `logger <message>` — Log user message to kernel ring buffer
- `loadavg` — Display load averages in `/proc/loadavg` format
- `nproc` — Print number of online CPUs
- `uptime` — Now shows **real load averages** from kernel tracker
- `w` — Now shows **real load averages** from kernel tracker
- `neofetch` — Now uses real hostname and dynamic display resolution

**Tab Completion** (`wasm_apps/terminal/src/shell/completion.rs`):

- Added all 12 new commands to tab completion: top, htop, lsblk, lsmod, crontab, cron, sysctl, logger, loadavg, nproc, signal, pipe, profile, passwd, w, who

**Kernel Integration** (`kernel/src/main.rs`):

- Module declarations: `klog`, `hostname`, `loadavg`, `cron`
- Init sequence: `klog::init()` → `hostname::init()` → `loadavg::init()` → `cron::init()`
- Main loop: `loadavg::update()` and `cron::tick()` called every frame

### v3.1 — February 12, 2026

**App Lifecycle — proc_exit Handling** (`kernel/src/app.rs`):

- App rendering loop now checks `WasmApp::has_exit_requested()` after each `step()` call
- When an app calls `proc_exit()`, the window is closed and app state reset to `Init`
- Exit code is logged for diagnostic purposes
- Apps can now terminate cleanly without orphaned windows

**Password Management API** (`kernel/src/wasm/mod.rs`, `guestlib/src/lib.rs`):

- `host_change_password` — Verify old password and set new one (only terminal and settings apps allowed)
  - Returns 0 (success), -1 (wrong password), -2 (locked out), -3 (permission denied)
- `host_has_password` — Check whether a system password is currently set
- `host_get_boot_time` — Get system uptime in milliseconds since boot
- `host_get_login_info` — Get logged-in user info string

**Guestlib Wrappers** (`guestlib/src/lib.rs`):

- `change_password(old, new)` → `Result<(), &str>` with descriptive error messages
- `has_password()` → `bool`
- `get_boot_time()` → `u64` (milliseconds)
- `get_login_info()` → `String`

**Terminal Commands** (`wasm_apps/terminal/src/shell/`):

- `passwd` — Interactive password change (prompts for old → new → confirm, validates match)
- `w` — Show logged-in users with uptime, TTY, and session info
- `who` — Show who is logged in (user + TTY + login time)
- `whoami` — Now uses host API for real user info instead of hardcoded "root"
- `id` — User identity (uid/gid/groups)
- `uptime` — System uptime using `host_get_boot_time` instead of `get_time()`

**STATUS.md Corrections:**

- Fixed WASI `proc_exit` status from "Stub" to "Done" (was implemented in v2.9)
- Fixed WASI `poll_oneoff` status from "Stub" to "Done" (was implemented in v2.9)

### v3.0 — February 11, 2026

**Full Hardware I/O Drivers** (`kernel/src/hardware.rs`):

- **xHCI USB 3.0 Full Driver**:
  - `XhciController` state machine: Uninitialized → Discovered → Reset → Configured → Running
  - Phase 1 — `discover()`: BAR0 mapping, PCI bus mastering, capability register parsing (MaxPorts, MaxSlots, MaxIntrs, HCI version)
  - Phase 2 — `reset()`: Halt controller (clear RUN_STOP, wait HCHalted), assert HCRST, wait for CNR clear
  - Phase 3 — `configure()`: DCBAA allocation (64-byte aligned), Command Ring allocation (256 TRBs, page-aligned), MaxSlotsEn in CONFIG register, CRCR programming
  - Phase 4 — `start()`: Set RUN_STOP + INTE, wait for HCH clear, enumerate all ports (PORTSC read: connect status, enable, speed, power)
  - `reset_port(port)`: Port reset sequence with PRC wait and W1C change-bit handling
  - `XhciPortInfo` with connect/enable/speed/power per-port state
  - `UsbSpeed` enum: None, Full (12Mbps), Low (1.5Mbps), High (480Mbps), SuperSpeed (5Gbps), SuperSpeedPlus (10Gbps)
  - Full register maps: `xhci_op_regs`, `xhci_usbcmd`, `xhci_usbsts`, `xhci_portsc` modules

- **NVMe Full Driver**:
  - `NvmeController` state machine: Uninitialized → Discovered → AdminQueueReady → Running
  - Phase 1 — `discover()`: BAR0 mapping, CAP register parsing (MQES, doorbell stride), version register
  - Phase 2 — `setup_admin_queue()`: Disable controller (clear CC.EN, wait CSTS.RDY=0), allocate ASQ (64-entry × 64B, page-aligned) and ACQ (64-entry × 16B, page-aligned), set AQA/ASQ/ACQ registers, re-enable with CC (NVM command set, 4KiB pages, IOSQES=64B, IOCQES=16B), wait CSTS.RDY=1
  - Phase 3 — `identify()`: Read CAP.CSS for command set support, verify CSTS.CFS=0 (no fatal error), mark running

- **AHCI/SATA Full Driver**:
  - `AhciController` state machine: Uninitialized → Discovered → Reset → PortsConfigured → Running
  - Phase 1 — `discover()`: ABAR (BAR5) mapping, CAP parsing (num ports, cmd slots, 64-bit support), version
  - Phase 2 — `reset()`: Enable AHCI mode (GHC.AE), HBA reset (GHC.HR), wait HR clear, re-enable AE
  - Phase 3 — `configure_ports()`: Per-port setup — stop command engine (clear ST+FRE, wait CR+FR clear), allocate Command List (1KiB, 1KiB-aligned) and FIS Receive area (256B, 256B-aligned), set CLB/FB registers, clear SERR/IS, detect device presence (SSTS.DET=3), read device signature (SATA/SATAPI/EnclosureMgmt/PortMultiplier)
  - Phase 4 — `start_ports()`: Enable FRE then ST on ports with devices, enable global interrupts (GHC.IE)
  - `identify_device(port)`: Port busy check (TFD.BSY/DRQ), command framework for IDENTIFY DEVICE FIS
  - `AhciPortInfo` with port number, implemented/present flags, device type, engine status
  - `AhciDeviceType` enum: None, Sata, Satapi, EnclosureMgmt, PortMultiplier, Unknown
  - Full register maps: `ahci_regs`, `ahci_ghc`, `ahci_pxcmd` modules

- **Intel GPU Modesetting**:
  - `IntelGpuController` with full display pipeline management
  - `init()`: BAR0 mapping, current pipe state readback (PIPEA_CONF, timing registers, DSPSURF)
  - `set_mode(mode)`: Full Gen7+ modesetting sequence:
    1. Disable display plane (DSPCNTR_A bit 31, trigger via DSPSURF write)
    2. Disable pipe (PIPEA_CONF bit 31, wait PIPEA_STAT bit 30 clear)
    3. Disable DPLL A (DPLL_A_CTL bit 31)
    4. Program timing: HTOTAL/HBLANK/HSYNC + VTOTAL/VBLANK/VSYNC + PIPE_SRCSZ
    5. Enable DPLL with VCO (DPLL_A_CTL)
    6. Enable pipe (PIPEA_CONF, wait PIPEA_STAT active)
    7. Enable plane (DSPCNTR_A: 32-bit BGRX format, set DSPSTRIDE, trigger DSPSURF)
  - `disable_pipe()`: Clean shutdown sequence (plane → pipe → DPLL)
  - `read_edid()`: GMBUS/DDC I2C EDID read (slave 0x50, 128 bytes, 4-byte GMBUS3 reads, header validation)
  - `DisplayMode` struct with width/height/refresh/pixel_clock/htotal/vtotal/hsync/vsync timing
  - 6 predefined modes: 640×480, 800×600, 1024×768, 1280×720, 1366×768, 1920×1080 (all @60Hz)
  - `init_full_drivers()`: Orchestrates all driver init via PCI device category filtering

**WASI proc_exit Proper Implementation** (`kernel/src/wasm/mod.rs`):

- `StoreData` extended with `exit_requested: bool` and `exit_code: i32` fields
- `proc_exit` handler now sets `exit_requested = true` and `exit_code` on the store data
- `WasmApp::step()` checks `exit_requested` after WASM step returns, logs termination, returns cleanly
- New public APIs: `WasmApp::has_exit_requested()`, `WasmApp::exit_code()` — allow app loader to detect and handle graceful termination

**WASI poll_oneoff Proper Implementation** (`kernel/src/wasm/mod.rs`):

- Full subscription type parsing (byte at offset 8): clock (0), fd_read (1), fd_write (2)
- Clock subscriptions: read timeout from bytes 24-31, read flags from bytes 40-41, spin-wait with capped delay (max 10ms)
- FD read subscriptions: report 0 bytes available (correct for stdin with no input)
- FD write subscriptions: report 4096 bytes writable capacity
- Unknown subscription types: report EINVAL errno
- Shortest clock timeout used for spin-wait across all subscriptions
- Proper event layout: userdata(8) + errno(2) + type(1) + padding(5) + fd_readwrite(16)

**Integration Tests** (`kernel/src/tests.rs`):

- `test_signals()` — 12 tests: register apps, send SIGUSR1, verify pending count, poll and verify signal identity, test signal masking (mask→send→unmask→poll), SIGKILL bypass of Ignore disposition, SIGSTOP/SIGCONT action verification, send to unknown app fails
- `test_pipes()` — 14 tests: create pipe, verify empty-read WouldBlock, write/read round-trip with content verification, multi-chunk write/read, close write end, verify write-after-close fails, close read end, active count check, second pipe creation with unique IDs
- `test_profiler()` — 7 tests: enable profiling, begin/end scope timing, record_scope + end_frame, multi-frame averaging, recent frames retrieval, summary report non-empty
- `test_mem_pool()` — 12 tests: init pool, alloc 16/100/900/4000 bytes (routing to correct slab), oversized alloc fails, free all blocks, verify slab_count=8, bulk alloc 64 blocks from one slab, bulk free
- Total test count: 60+ → **105+ tests** across 13 subsystems

### v2.9 — February 11, 2026

**Unix-Like Signal System** (`kernel/src/signals.rs` — NEW):

- `Signal` enum with 12 signal types: SIGHUP(1), SIGINT(2), SIGQUIT(3), SIGILL(4), SIGTRAP(5), SIGABRT(6), SIGBUS(7), SIGFPE(8), SIGKILL(9), SIGTERM(15), SIGSTOP(17), SIGCONT(19), SIGUSR1(30), SIGUSR2(31)
- `SignalManager` with per-app signal state tracking:
  - `SignalState` per app: pending signals (`BTreeSet`), signal mask (`BTreeSet`), dispositions (`BTreeMap`)
  - `SignalDisposition` enum: Default, Ignore, Catch
  - `register_app(app_id)` / `unregister_app(app_id)` lifecycle management
  - `send_signal(target, signal)` — deliver signal (SIGKILL/SIGSTOP bypass masks)
  - `poll_signal(app_id)` — dequeue next pending signal
  - `pending_count(app_id)` — check pending signal count
  - `set_mask(app_id, mask)` / `set_disposition(app_id, signal, disposition)` — per-app signal configuration
- `SIGNAL_MANAGER: Mutex<SignalManager>` global, initialized at boot
- Apps auto-registered with signal manager during kernel boot sequence

**Kernel Pipe Infrastructure** (`kernel/src/pipes.rs` — NEW):

- `PipeManager` with `BTreeMap<PipeId, Pipe>` tracking:
  - `PipeBuffer` — circular byte buffer (64 KB default capacity)
  - `Pipe` — unidirectional stream with read/write end tracking, EOF detection
  - `create_pipe()` — allocate new anonymous pipe, returns PipeId
  - `create_named_pipe(name)` — create named pipe for IPC discovery
  - `write(pipe_id, data)` — write bytes to pipe (returns bytes written)
  - `read(pipe_id, buf)` — read bytes from pipe (returns bytes read)
  - `close_write(pipe_id)` / `close_read(pipe_id)` — close individual ends
  - `pipe_count()` — number of active pipes
- `PIPE_MANAGER: Mutex<PipeManager>` global, initialized at boot

**System Performance Profiler** (`kernel/src/profiler.rs` — NEW):

- `Profiler` with comprehensive metrics:
  - `ScopeTiming` — per-named-scope cumulative timing with call count
  - `FrameProfile` — per-frame breakdown: input, wasm, render, gpu, network milliseconds
  - `MemorySnapshot` — heap used/total, frame used/total, pool stats
  - `IoSnapshot` — network bytes in/out, disk reads/writes, WASM fuel consumed
  - 120-frame ring buffer history for trend analysis
  - `begin_scope(name)` / `end_scope(name)` — scope-based profiling
  - `begin_frame()` / `end_frame()` — frame timing with auto-history push
  - `avg_frame_time(count)` — rolling average over last N frames
  - `total_frames()` — total profiled frame count
  - `summary_report()` — formatted text report
- `PROFILER: Mutex<Profiler>` global, initialized at boot

**Slab/Pool Memory Allocator** (`kernel/src/mem_pool.rs` — NEW):

- `MemoryPool` with 8 fixed-size `Slab` classes:
  - Size classes: 32, 64, 128, 256, 512, 1024, 2048, 4096 bytes
  - 64 blocks per slab, bitmap-based free tracking
  - `alloc(size)` — O(1) best-case allocation via free-list hint
  - `free(ptr, size)` — O(1) deallocation
  - `stats()` — per-class utilization report
- `MEMORY_POOL: Mutex<MemoryPool>` global, initialized at boot

**WASI Stub Fixes** (`kernel/src/wasm/mod.rs`):

- `sched_yield` — now triggers preemption via `PREEMPTION_PENDING` atomic flag (was panic)
- `fd_sync` — now triggers FAT32 filesystem sync (was panic)
- `path_link` — now returns ENOTSUP/76 gracefully (was panic)
- `path_readlink` — now returns EINVAL/28 gracefully (was panic, no symlink support)

**New WASM Host APIs** (`kernel/src/wasm/mod.rs`):

- Signal APIs:
  - `host_signal_send(target_app_id, signal_num)` — send signal to another app
  - `host_signal_poll()` — poll for next pending signal
  - `host_signal_pending()` — check if any signals are pending
- Pipe APIs:
  - `host_pipe_create()` — create anonymous pipe
  - `host_pipe_write(pipe_id, buf_ptr, buf_len)` — write to pipe
  - `host_pipe_read(pipe_id, buf_ptr, buf_len)` — read from pipe
  - `host_pipe_close_write(pipe_id)` / `host_pipe_close_read(pipe_id)` — close pipe ends
  - `host_pipe_count()` — active pipe count
- Profiler APIs:
  - `host_profiler_avg_frame_time()` — average frame time in microseconds
  - `host_profiler_total_frames()` — total profiled frames

**Guest Library Updates** (`guestlib/src/lib.rs`):

- `Signal` enum with 8 user-accessible variants (SIGHUP/SIGINT/SIGKILL/SIGTERM/SIGSTOP/SIGCONT/SIGUSR1/SIGUSR2)
- `signal_send(app_id, signal)`, `signal_poll()`, `signal_pending()` — safe Rust wrappers
- `pipe_create()`, `pipe_write()`, `pipe_read()`, `pipe_close_write()`, `pipe_close_read()`, `pipe_count()` — safe pipe wrappers
- `profiler_avg_frame_time()`, `profiler_total_frames()` — safe profiler wrappers

**Terminal Commands** (`wasm_apps/terminal/src/shell/commands_sys.rs`):

- `signal` command: `signal send <app_id> <signal>`, `signal poll`, `signal pending`, `signal help`
  - Supports signal names (SIGTERM), short names (TERM), and numbers (15)
- `pipe` command: `pipe create`, `pipe write <id> <data>`, `pipe read <id>`, `pipe close <id>`, `pipe status`
- `profile` command: Shows avg frame time, FPS, total frames, active pipes, pending signals

### v2.8 — February 11, 2026

**Full Hardware I/O Command Infrastructure** (`kernel/src/hardware.rs`):

- NVMe I/O command structures and builders:
  - `NvmeSubmissionEntry` (64-byte SQE): opcode, flags, NSID, PRP1/PRP2, CDW10-15
  - `NvmeCompletionEntry` (16-byte CQE): result, SQ head, SQ ID, command ID, status
  - `nvme_opcodes` module: Admin (IDENTIFY=0x06, CREATE_IO_SQ=0x01, CREATE_IO_CQ=0x05, SET_FEATURES=0x09) and I/O (READ=0x02, WRITE=0x01, FLUSH=0x04)
  - `nvme_read_cmd(nsid, lba, blocks, prp1, prp2)`: Build NVMe Read command
  - `nvme_write_cmd(nsid, lba, blocks, prp1, prp2)`: Build NVMe Write command
  - `nvme_identify_controller_cmd(prp1)`: Build Identify Controller command
- AHCI/SATA command structures and FIS builders:
  - `AhciCommandHeader` (32-byte): flags, PRDT length, byte count, command table address (64-bit)
  - `AhciPrdt` (16-byte): data base address (64-bit), byte count, interrupt flag
  - `FisRegH2D` (20-byte): FIS type (0x27), command/control flag, ATA command, LBA, device, count, features
  - `ahci_read_dma_fis(lba, sectors)`: Build ATA READ DMA EXT (0x25) FIS
  - `ahci_write_dma_fis(lba, sectors)`: Build ATA WRITE DMA EXT (0x35) FIS
  - `ahci_identify_fis()`: Build ATA IDENTIFY DEVICE (0xEC) FIS
- xHCI USB command structures and TRB builders:
  - `XhciTrb` (16-byte): parameter, status, control fields
  - `xhci_trb_types` module: Normal(1), Setup(2), Data(3), Status(4), Link(6), EnableSlot(9), AddressDevice(11), ConfigureEndpoint(12), EvaluateContext(13), ResetEndpoint(14), StopEndpoint(15)
  - `xhci_enable_slot_trb()`: Build Enable Slot command TRB (slot_type=0)
  - `xhci_normal_trb(data_ptr, length, ioc)`: Build Normal Transfer TRB
  - `xhci_setup_trb(bmRequestType, bRequest, wValue, wIndex, wLength)`: Build Setup Stage TRB
- `write_mmio_u32(addr, value)`: Safe MMIO write helper with volatile semantics

**Intel GPU Modesetting Probe** (`kernel/src/hardware.rs`):

- `intel_gpu_regs` module: Gen7+ display pipeline register constants:
  - Display: PIPEA_CONF (0x70008), DSPCNTR_A (0x70180), DSPSTRIDE_A (0x70188), DSPSURF_A (0x7019C)
  - PLL: DPLL_A_CTL (0x06014), DPLL_A_DIVISOR (0x06040)
  - Timing: HTOTAL_A (0x60000), HBLANK_A (0x60004), HSYNC_A (0x60008), VTOTAL_A (0x6000C), VBLANK_A (0x60010), VSYNC_A (0x60014)
  - I2C/DDC: GMBUS0 (0x05100) through GMBUS5 (0x05114)
- `init_intel_gpu()`: Probes detected Intel GPUs via BAR0:
  - Reads PipeA configuration register for pipeline enable status
  - Reads display plane control register for plane enable and pixel format
  - Reads DPLL status for clock generation state
  - Reads horizontal/vertical timing registers for active resolution detection
  - All reads via safe `read_mmio_u32()` volatile access

**Full DOM/JavaScript Engine** (`wasm_apps/web_browser/src/html/js.rs`):

- New JsValue types:
  - `Array(Vec<JsValue>)`: Full Array type with literal syntax `[a, b, c]`
  - `Element { tag, id, class_name, inner_html, children }`: DOM Element references
  - Updated `to_string_val()`, `to_bool()`, `to_number()` for new types
  - `is_array()`, `is_element()` type-check methods
- DOM API (document methods):
  - `document.getElementById(id)`: Returns/creates Element references cached in `dom_elements` map
  - `document.querySelector(selector)`, `document.querySelectorAll(selector)`: CSS selector queries
  - `document.createElement(tag)`: Create new DOM elements with auto-generated IDs
  - `document.createTextNode(text)`: Create text node elements
  - `document.getElementsByTagName(tag)`, `document.getElementsByClassName(class)`: Collection queries
- Element methods via `call_element_method()`:
  - Mutation: `appendChild` (renders to HTML output), `removeChild`, `insertBefore`, `replaceChild`, `cloneNode`
  - Attributes: `setAttribute`, `getAttribute`, `hasChildNodes`
  - Properties: `innerHTML`/`innerText`/`textContent` (read/write), `id`, `className`, `tagName`/`nodeName`, `childNodes`/`children`, `parentNode`, `style`, `nodeType`
  - Events: `addEventListener`, `focus`, `blur`, `click`
  - Chained property access: `elem.style.color = "red"` with dot-chaining support
- Array methods via `call_array_method()`:
  - Mutating: `push`, `pop`, `shift`, `reverse`, `sort`
  - Non-mutating: `join`, `indexOf`, `includes`, `slice`, `concat`, `flat`
  - Iteration stubs: `map`, `filter`, `forEach`, `reduce`
  - `Array.length` property access
  - Array literal parsing and integer index access (`arr[0]`, `str[0]`)
- New language features:
  - Ternary operator: `condition ? expr1 : expr2`
  - `typeof` operator: Returns type string for any value
  - `new` keyword: `new Array()`, `new Date()`, `new RegExp()`, `new Error()`, user-defined constructors
  - `do...while` loop
  - `switch/case/default/break` statement with fall-through semantics
  - `continue` and `break` in loops
  - 9 new Token types: New, Do, Switch, Case, Default, Break, Continue, Typeof, Colon, QuestionMark
- Additional built-in APIs:
  - Math: `log`, `sin`, `cos`, `tan`, `atan2`, `trunc`, `sign`
  - Global: `Array.isArray()`, `Array.from()`, `JSON.stringify()`, `JSON.parse()`, `Object.keys()`/`values()`/`entries()`
  - Global: `Boolean()`, `window.setTimeout()`/`setInterval()`/`clearTimeout()`/`clearInterval()` (stubs)
  - Global: `encodeURIComponent()`, `decodeURIComponent()`
  - Refactored `call_function` into `call_function` + `call_function_inner` for `new` keyword reuse

**Enhanced Accessibility** (`kernel/src/screen_reader.rs`):

- 5 new `AnnounceKind` variants: AriaLive, AriaLandmark, FocusRing, Tooltip, Progress
- ARIA role announcements:
  - `announce_role(role, label)`: Maps 35+ ARIA roles to human-readable descriptions (button, link, textbox, checkbox, radio, slider, tab, menu, dialog, alert, progressbar, toolbar, tree, heading, navigation, main, banner, search, form, region, etc.)
- ARIA state announcements:
  - `announce_state(label, state, value)`: Maps state names to screen reader output (checked/not checked, expanded/collapsed, selected/not selected, dimmed/enabled, pressed/not pressed)
- ARIA live region support:
  - `announce_live_region(region_id, content, politeness)`: Tracks content changes per region ID, only announces when content changes, respects polite vs assertive priority
  - `live_regions: BTreeMap<String, String>` cache for deduplication
- Landmark navigation:
  - `announce_landmark(landmark_type, label)`: Announces landmark type and label for keyboard navigation
- Focus ring navigation:
  - `move_focus_ring(forward, total)`: Tab/Shift+Tab through focusable elements with "Item N of M" announcements
  - `focus_ring_index()`, `reset_focus_ring()`: Query/reset focus position
- Additional announcement methods:
  - `announce_tooltip(text)`: Tooltip display (verbosity-gated)
  - `announce_progress(label, percent)`: Progress bar updates with completion detection
  - `announce_heading(level, text)`: Heading level navigation (h1-h6)
  - `announce_list_position(index, total, item_text)`: List item position (verbosity-aware)

### v2.7 — February 11, 2026

**Safe Mutable Statics in WASM Apps** (`wasm_apps/*/src/main.rs`):

- All 6 WASM apps migrated from `static mut APP_STATE: OnceCell<AppState>` to safe `thread_local! { static APP_STATE: RefCell<Option<AppState>> }`:
  - `file_manager`, `settings`, `system_monitor`, `terminal`, `text_editor`, `web_browser`
  - Eliminates all `unsafe` mutable static access in application code
  - WASM is single-threaded, making `thread_local + RefCell` the correct no_std pattern
  - Web browser additionally moved `static mut REDIRECT_COUNT` into `AppState` as `redirect_count: usize`

**SipHash-2-4 Content Hashing** (`applib/src/hash.rs`):

- Replaced MD5 with custom SipHash-2-4 implementation:
  - `SipHasher` struct implementing `core::hash::Hasher` trait
  - `write()` with 8-byte block processing, `finish()` with 4-round finalization
  - Faster than MD5, better collision resistance for hash tables, no external dependencies
  - Removed `md-5` crate dependency from `applib/Cargo.toml`

**Safe FFI in Guest Library** (`guestlib/src/lib.rs`):

- Replaced 3 `core::mem::transmute` calls with safe `core::ptr::read_unaligned`:
  - `get_input_state()`: InputState deserialization
  - `get_win_rect()`: Rect deserialization
  - `get_stylesheet()`: StyleSheet deserialization
  - Eliminates undefined behavior from potential alignment issues

**HLT-Based Frame Limiter** (`kernel/src/time.rs`, `kernel/src/main.rs`):

- Replaced spin-delay frame limiter with `hlt_delay()`:
  - Uses HLT instruction to yield CPU during frame wait, reducing power consumption
  - Hybrid approach: HLT for bulk wait, short spin for remaining microseconds
  - `hlt_delay(ms)` function added to `time.rs`

**DMA Buffer Leak Documentation** (`kernel/src/virtio/mod.rs`):

- Documented DMA buffer allocations as intentionally leaked:
  - Hardware devices require stable DMA addresses for their entire lifetime
  - Buffers allocated once during device init, never freed (correct behavior)
  - Added `// NOTE:` comments at all DMA allocation sites

**Dynamic GPU Resolution** (`kernel/src/virtio/gpu.rs`):

- GPU framebuffer dimensions now queried dynamically via VirtIO GET_DISPLAY_INFO:
  - Replaced hardcoded `W=1366, H=768` constants with `width/height` fields on `VirtioGpu`
  - `init()` sends GET_DISPLAY_INFO command and reads display PModes
  - Falls back to 1024×768 if display info query fails
  - Compositor receives actual dimensions instead of constants

**Pervasive unwrap() Cleanup** (kernel-wide):

- Replaced 112 bare `unwrap()` calls across 16 kernel files:
  - `wasm/mod.rs`: `instantiate_app()` returns `Result<WasmApp, anyhow::Error>` instead of panicking; all host API unwraps replaced with descriptive expects
  - `app.rs`: App instantiation failure sets `AppStatus::Crashed` instead of panicking
  - `fs/mod.rs`: 3 `open_files.get_mut(&fd).unwrap()` → `.ok_or(FsError::Badf)?` for proper error propagation
  - `virtio/gpu.rs`: All GPU command unwraps → `.expect("GPU CMD rejected")`
  - `virtio/mod.rs`, `virtio/network.rs`, `virtio/input.rs`: Ring buffer and PCI capability expects
  - `network/mod.rs`, `network/dhcp.rs`: Network init expects
  - `stats.rs`, `logging.rs`, `auth.rs`, `pci.rs`, `topbar.rs`, `fs/fat32.rs`: Various cleanup
  - 10 remaining `try_into().unwrap()` kept — all on fixed-size byte slices (infallible)

**USB/NVMe/AHCI Controller Initialization** (`kernel/src/hardware.rs`):

- Enhanced hardware init with controller-level register access:
  - `init_usb_controllers()`: Reads xHCI BAR0 for version/ports/slots; EHCI BAR0 for port count via HCS_PARAMS
  - `init_nvme_controllers()`: Reads NVMe BAR0 for version (VS register), max queue entries (CAP), doorbell stride
  - `init_ahci_controllers()`: Reads AHCI BAR5 for port enumeration (PI register), per-port device signatures, command list addresses, 64-bit capability flag
  - `read_mmio_u32()`: Safe MMIO read helper with volatile semantics
  - All controllers discovered via PCI class/subclass matching

**Touch/Gesture Input** (`applib/src/input/`, `kernel/src/main.rs`):

- Multi-touch input protocol support:
  - `EV_ABS = 0x3` event type added to `EventType` enum
  - `TouchSlot` struct: tracking_id, x, y, active flag
  - `TouchState`: 5 simultaneous slots (`MAX_TOUCH_SLOTS`), active slot tracking
  - `GestureKind` enum: Tap, TwoFingerTap, Pan(dx, dy), Pinch(scale), TwoFingerScroll(dx, dy), Swipe(dx, dy)
  - `InputEvent` variants: `TouchDown { slot, x, y }`, `TouchMove { slot, x, y }`, `TouchUp { slot }`, `Gesture(GestureKind)`
  - Kernel handles `ABS_MT_SLOT`, `ABS_MT_POSITION_X`, `ABS_MT_POSITION_Y`, `ABS_MT_TRACKING_ID` events
  - Touch events translated into `InputEvent` variants and injected into event queue

**Text Shaping with Ligatures** (`kernel/src/truetype.rs`):

- Ligature detection and text shaping pipeline:
  - `LigatureRule` struct: input sequence (e.g., "fi"), output glyph index, Unicode codepoint
  - `detect_standard_ligatures()`: Checks font cmap for Unicode ligature codepoints (U+FB00 fi, U+FB01 fl, U+FB02 ffi, U+FB03 ffl, U+FB04 ft)
  - `apply_ligatures()`: Greedy longest-first ligature substitution on input text
  - `ShapedGlyph` struct: glyph index, character, x_advance, x_offset, y_offset
  - `shape_text()` method on FontManager: Produces `Vec<ShapedGlyph>` with ligature substitution applied
  - Ligatures populated automatically during `load_font()`

**Web Browser: PNG Image Decoding** (`wasm_apps/web_browser/src/html/`):

- Inline image rendering from data URIs:
  - `ImageData` struct: width, height, RGBA byte vector
  - `ImageData::from_data_uri()`: Parses `data:image/png;base64,...` URIs, decodes base64, renders PNG via `Framebuffer::from_png()`, extracts RGBA pixels
  - `base64_decode()`: No-std-compatible base64 decoder (handles padding, whitespace)
  - `RenderItem::Image`: New render list variant with position, dimensions, RGBA data
  - `render_html()`: Nearest-neighbor scaling for display, per-pixel alpha-aware blitting
  - `Block::Image` now carries optional `ImageData` decoded during layout

**Web Browser: JavaScript Engine** (`wasm_apps/web_browser/src/html/js.rs`):

- Minimal JavaScript interpreter for basic page interactivity:
  - **Tokenizer**: Full lexer with identifiers, string/number literals, operators (arithmetic, comparison, logical, assignment), keywords (var/let/const/if/else/for/while/function/return)
  - **Expression parser**: Operator precedence (unary → multiplication → addition → comparison → logical), parenthesized groups, string concatenation
  - **Statement execution**: Variable declarations, if/else (with else-if chains), for/while loops (10K iteration limit), function declarations with parameter binding, return values
  - **Built-in APIs**:
    - `document.write()` / `document.writeln()` — output injected into HTML before rendering
    - `document.title` — get/set page title
    - `console.log/warn/error()` — log to kernel serial console
    - `alert()` — logged + rendered as `<p>` tag via document.write
    - `Math.floor/ceil/round/abs/max/min/sqrt/pow/random/PI/E`
    - `parseInt()`, `parseFloat()`, `isNaN()`, `isFinite()`, `String()`, `Number()`
  - **String methods**: `toUpperCase`, `toLowerCase`, `trim`, `charAt`, `indexOf`, `includes`, `substring`, `slice`, `replace`, `concat`, `startsWith`, `endsWith`, `repeat`, `.length`
  - **Number methods**: `toFixed()`
  - **Integration**: `execute_scripts()` extracts `<script>` blocks, runs interpreter, injects `document.write()` output before `</body>` tag in HTML source

**App Development SDK** (`sdk/`):

- Complete SDK for building Knox OS WASM applications:
  - `sdk/template/`: Ready-to-use app template with Cargo.toml, rust-toolchain, and `src/main.rs`
  - Template demonstrates: state management (thread_local + RefCell), init/step lifecycle, framebuffer rendering, UiContext widgets, input handling, logging
  - `sdk/create_app.sh`: Scaffolding script — copies template, renames package, prints next steps
  - `sdk/README.md`: Comprehensive documentation covering:
    - Quick start guide (create → build → register)
    - Architecture overview with execution model diagram
    - Full API reference tables for guestlib (Core, Clipboard, Networking, Filesystem, IPC, Audio, System, Display, Preferences)
    - Drawing API guide (primitives, text, fonts, images)
    - Widget toolkit reference (buttons, text boxes, tabs, graphs, scroll views, canvas)
    - Layout helpers (vertical/horizontal stack layouts)
    - Input handling guide (mouse, keyboard, touch)
    - Debugging guide (logging, performance, QEMU dump)
    - Build configuration and troubleshooting

### v2.6 — February 11, 2026

**PCI Multi-Function Device Support** (`kernel/src/pci.rs`):

- PCI bus enumeration now scans functions 0–7 for multi-function devices:
  - Checks header type bit 7 to detect multi-function devices
  - Iterates functions 0–7, skipping invalid (vendor=0xFFFF) functions
  - Graceful handling of non-type-0 headers (PCI bridges) — logs and skips instead of asserting
  - Previously only scanned function 0, missing devices behind multi-function endpoints

**DNS Query ID Randomization** (`wasm_apps/web_browser/src/dns.rs`):

- DNS query IDs are now randomized using `guestlib::get_random()`:
  - Previously hardcoded to `0x0001`, making DNS poisoning trivial
  - Now generates a cryptographically random 16-bit ID per query
  - Prevents DNS cache poisoning and response spoofing attacks

**VirtIO Queue Double-Init Prevention** (`kernel/src/virtio/mod.rs`):

- Added `INITIALIZED_QUEUES` bitmask tracking with `mark_queue_initialized()`:
  - `initialize_queue()` now checks and panics on double-initialization
  - Prevents subtle corruption from re-initializing an active virtqueue
  - Uses `spin::Mutex<u64>` for thread-safe access (supports up to 64 queues)

**WASI Implementation Improvements** (`kernel/src/wasm/mod.rs`):

- `proc_exit`: Changed from no-op stub to proper implementation — logs app name and exit code
- `poll_oneoff`: Changed from panic stub to working implementation:
  - Reads 48-byte WASI subscription structures (userdata field)
  - Writes 32-byte event structures with userdata propagation
  - Marks all subscriptions as immediately ready (correct for clock timeouts)
  - Returns event count via `nevents_ptr`
  - Added `Errno::EINVAL` variant for invalid argument detection
- `clock_time_get`: Fixed multiplier from `* 1e9` to `* 1_000_000.0`:
  - SystemClock returns milliseconds, WASI expects nanoseconds
  - Previous 1e9 multiplier was off by 1000x (treating ms as seconds)
- `StepContext`: Added comprehensive safety documentation:
  - Documented raw pointer lifetime guarantees and invariants
  - Added `unsafe impl Send for StepContext {}` with safety justification

**Text Editor File Dialog** (`wasm_apps/text_editor/src/main.rs`):

- Full file open/save dialog overlay replacing hardcoded file path:
  - `FileDialogMode` enum (None/Open/SaveAs) and `FileDialogState` struct
  - Filesystem browsing via `std::fs::read_dir` with entry sorting (dirs first)
  - Directory navigation: click to enter, ".." to go up, path breadcrumb display
  - Scrollable file list with selection highlighting and click-to-select
  - Filename text input field for SaveAs mode with keyboard input via CHARMAP
  - Cancel/OK buttons with click detection via `check_contains_point()`
  - Keyboard shortcuts: Escape to cancel, Up/Down arrows for selection, Enter to confirm
  - Ctrl+S opens SaveAs dialog when using default path, saves directly for existing files
  - Ctrl+O opens file browser dialog in Open mode
  - Dimmed background overlay for modal dialog effect

**Cross-Platform Build System** (`make.py`):

- Build system now auto-detects platform and architecture:
  - `_get_host_target()`: Detects darwin/linux/windows × x86_64/aarch64
  - `_get_toolchain_version()`: Constructs Rust toolchain string dynamically
  - `_get_qemu_accel()`: Returns hvf (macOS), kvm (Linux with /dev/kvm), or tcg (fallback)
  - `_get_mkfs_command()`: Returns `newfs_msdos` (macOS) or `mkfs.vfat` (Linux)
  - Previously hardcoded to `nightly-2025-06-01-x86_64-apple-darwin` and macOS-only commands

**VirtIO Network Header Endianness** (`kernel/src/virtio/network.rs`):

- Documented VirtioNetHdr fields as VirtIO-spec le16, correct on x86_64 little-endian
- Added note for big-endian architecture porting considerations

**Code Cleanup — Zero Remaining TODOs**:

- Resolved all TODO comments across kernel, applib, and guestlib:
  - `kernel/src/virtio/gpu.rs`: Documented MaybeUninit zeroed-init safety, clarified response status code handling
  - `kernel/src/virtio/input.rs`: Documented input event handling (no status code per VirtIO input spec), buffer re-enqueue is infallible
  - `applib/src/lib.rs`: Removed stale "use Point2D everywhere" TODO
  - `applib/src/uitk/mod.rs`: Replaced "move that somewhere else" with "begin-of-frame housekeeping" documentation

### v2.5 — February 10, 2026

**International Keyboard Layouts** (`kernel/src/keyboard_layouts.rs`):

- Full keyboard layout management with 9 layouts:
  - `LayoutId` enum: US, UK, German, French, Spanish, Italian, Swedish, Dvorak, Colemak
  - `KeyboardLayout` struct: name, label, scancode→char translation tables (normal + shifted + alt_gr)
  - `LayoutManager` with `translate(scancode, shift, alt_gr) -> Option<char>`: Per-layout character mapping
  - `set_layout(id)` / `active_layout()` / `list_layouts()` / `count()`: Runtime layout management
  - AltGr modifier support for European special characters (ä, ö, ü, ß, é, è, ñ, etc.)
  - Atomic `ACTIVE_LAYOUT: AtomicU8` for lock-free layout queries
  - Global `LAYOUT_MANAGER: Mutex<LayoutManager>` + `init()` function

**Drag-and-Drop Framework** (`kernel/src/drag_drop.rs`):

- Desktop drag-and-drop with typed payloads and drop zones:
  - `DragDropManager` with full drag lifecycle (start → update → drop/cancel)
  - `DragPayload` enum: Files(Vec<String>), Text(String), Widget(String), Custom(String, Vec<u8>)
  - `DragState` struct: payload, source_app, start position, current position, timestamp
  - `DropZone` struct: id, owner_app, bounds rectangle, accepted payload types
  - `register_drop_zone()` / `unregister_drop_zone()`: Zone management with accept predicates
  - `start_drag()` / `update_cursor()` / `drop_on()` / `cancel()`: Full event lifecycle
  - Cursor highlight tracking for active drop zones
  - Global `DRAG_DROP: Mutex<DragDropManager>` + `init()` function

**TrueType/OpenType Font Rasterizer** (`kernel/src/truetype.rs`):

- Software font rasterizer with glyph caching:
  - `FontManager` with `LoadedFont` registry and `GlyphBitmap` output
  - TrueType table parsing: head, hhea, maxp, cmap, loca, glyf, post
  - `GlyphOutline` with contour points and on-curve flags
  - Scanline-based glyph rasterization with edge intersection
  - `BTreeMap<(font_id, glyph_id, size), GlyphBitmap>` glyph cache
  - `render_text(font_id, text, size) -> Vec<GlyphBitmap>`: Full text rendering pipeline
  - `text_width()` / `line_height()`: Layout measurement functions
  - Custom `f64_ceil` / `f64_round` / `f64_abs` / `f64_max` helpers for no_std compatibility
  - Global `FONT_MANAGER: Mutex<FontManager>` + `init()` function

**Hardware Cursor** (`kernel/src/hw_cursor.rs`):

- Hardware-accelerated cursor with 10 procedural shapes:
  - `CursorShape` enum: Arrow, Hand, Text, ResizeH, ResizeV, ResizeDiag, Crosshair, Wait, Move, NotAllowed
  - `HardwareCursor` with 64×64 RGBA image generation per shape
  - Procedural rendering: Bresenham circle algorithm for NotAllowed, precomputed coordinate offsets for Wait animation
  - `set_shape(shape)` / `set_position(x, y)` / `try_enable()` / `current_image()`: Cursor control API
  - `CursorShape::from_id(u32)`: Integer-to-shape conversion for WASM API
  - Per-shape image caching (`[Option<Vec<u8>>; 10]` array)
  - Global `HW_CURSOR: Mutex<HardwareCursor>` + `init()` function

**PS/2 Keyboard/Mouse Driver** (`kernel/src/ps2.rs`):

- Legacy PS/2 fallback driver for non-VirtIO environments:
  - `Ps2Driver` with `Ps2Keyboard` + `Ps2Mouse` sub-drivers
  - `Ps2Keyboard`: Scancode set 2 translation (90+ key mappings), modifier tracking (Shift/Ctrl/Alt/CapsLock), extended key support (E0 prefix)
  - `Ps2Mouse`: 3-button support + scroll wheel, packet decoding (3 or 4 bytes), overflow detection, signed delta calculation
  - Port I/O: data port (0x60), command/status port (0x64)
  - `init(virtio_available: bool)`: Only activates when VirtIO input is unavailable
  - `process_scancode(byte)` → Keycode event, `process_byte(byte)` → MouseEvent
  - Atomic `PS2_ACTIVE: AtomicBool` for runtime status queries
  - Global `PS2_DRIVER: Mutex<Ps2Driver>` + `init()` function

**UEFI GOP Framebuffer Fallback** (`kernel/src/uefi_gop.rs`):

- UEFI Graphics Output Protocol framebuffer for display fallback:
  - `GopFramebuffer` with `GopModeInfo` (width, height, stride, pixel format)
  - `PixelFormat` enum: RGB, BGR, BitMask (with mask fields)
  - `set_mode(width, height, format)`: Configure display mode
  - `put_pixel(x, y, r, g, b)` / `fill_rect()` / `blit()` / `flush()`: Drawing primitives with format-aware pixel encoding
  - `dimensions() -> (u32, u32)`: Query current resolution
  - Atomic `GOP_ACTIVE: AtomicBool` for runtime status queries
  - Global `GOP_FB: Mutex<GopFramebuffer>` + `init()` function

**API Documentation Registry** (`kernel/src/api_docs.rs`):

- Runtime API documentation system with search:
  - `ApiDocRegistry` with `BTreeMap<String, ModuleDoc>` storage
  - `ModuleDoc`: name, description, Vec<ApiDoc>, version string
  - `ApiDoc`: name, description, parameters (Vec<ParamDoc>), return type, stability level, example code
  - `Stability` enum: Stable, Beta, Experimental, Deprecated
  - 11 built-in module docs: filesystem, display, network, audio, security, firewall, themes, workspaces, keyboard_layouts, drag_drop, truetype
  - `search(query) -> Vec<&ApiDoc>`: Full-text search across all modules
  - `list_modules()` / `get(module)` / `generate_index()` / `format_api()`: Documentation access
  - Global `API_DOCS: Mutex<ApiDocRegistry>` + `init()` function

**CI/CD Pipeline** (`.github/workflows/ci.yml`):

- GitHub Actions continuous integration pipeline:
  - **Format check**: `cargo fmt --check` for kernel, applib, guestlib
  - **Kernel build**: x86_64-unknown-uefi target with release profile
  - **Library check**: applib + guestlib compilation verification
  - **WASM apps matrix**: Parallel build of 6 apps (file_manager, settings, system_monitor, terminal, text_editor, web_browser) targeting wasm32-wasip1
  - **Disk image**: Artifact generation with kernel EFI + WASM binaries
  - **Documentation**: `cargo doc --no-deps` generation
  - **Smoke test**: QEMU headless boot with 30-second timeout, serial output validation
  - Cargo registry + per-crate target directory caching

**New WASM Host APIs** (`kernel/src/wasm/mod.rs`):

- 13 new host functions exposed to WASM apps:
  - Keyboard: `host_get_keyboard_layout` (writes layout name to WASM memory), `host_set_keyboard_layout` (reads layout name from WASM memory), `host_keyboard_layout_count`
  - Drag-drop: `host_drag_active`, `host_drag_cancel`
  - Cursor: `host_set_cursor_theme` (CursorShape::from_id mapping), `host_get_cursor_theme`
  - Font: `host_font_count`
  - API docs: `host_api_module_count`, `host_api_total_count`
  - PS/2: `host_ps2_active`
  - GOP: `host_gop_active`

**Guest Library Wrappers** (`guestlib/src/lib.rs`):

- Safe Rust wrappers for all new v2.5 host APIs:
  - `get_keyboard_layout() -> String`: Read active layout name (32-byte buffer)
  - `set_keyboard_layout(name: &str)`: Switch keyboard layout by name
  - `keyboard_layout_count() -> u32`: Number of available layouts
  - `drag_active() -> bool`, `drag_cancel()`: Drag-and-drop status/control
  - `CursorTheme` enum (10 variants) with `Display` trait: Arrow, Hand, Text, ResizeH, ResizeV, ResizeDiag, Crosshair, Wait, Move, NotAllowed
  - `set_cursor_theme(theme)` / `get_cursor_theme() -> CursorTheme`: Cursor management
  - `font_count() -> u32`, `api_module_count() -> u32`, `api_total_count() -> u32`
  - `ps2_active() -> bool`, `gop_active() -> bool`: Hardware status queries

**New Terminal Commands** (`wasm_apps/terminal/src/shell/`):

- 4 new command groups:
  - `layout [status|set <name>|list]` (aliases: `keyboard`, `kbd`): View/switch/list keyboard layouts (us/uk/de/fr/es/it/se/dvorak/colemak)
  - `cursor [status|set <shape>]`: View/switch hardware cursor shape (arrow/hand/text/resize-h/resize-v/resize-diag/crosshair/wait/move/not-allowed)
  - `apidoc [status|list]` (alias: `api`): API documentation overview with module count and total API count
  - `hwinfo` (alias: `hw`): Comprehensive hardware status — PS/2 driver, GOP framebuffer, hardware cursor, keyboard layout, font engine, drag-drop state
- Tab completion updated for 8 new command names
- Help system updated with new "hardware" topic

### v2.4 — February 10, 2026

**Network Firewall** (`kernel/src/firewall.rs`):

- Stateless packet-filtering firewall with configurable rule engine:
  - `Firewall` struct with ordered rule list (first-match-wins evaluation)
  - `FirewallRule`: protocol (TCP/UDP/ICMP/Any), direction (Inbound/Outbound/Both), action (Allow/Deny/Log), IP match, port match
  - `PortMatch` variants: Any, Single, Range(u16, u16)
  - `IpMatch` variants: Any, Exact(u32), Subnet(u32, u32)
  - Default rules: Allow DNS (port 53), DHCP (67-68), HTTP/HTTPS (80, 443), ICMP, deny all inbound
  - `evaluate(protocol, direction, ip, port) -> Action`: Rule evaluation with stats tracking
  - `FirewallStats`: packets_allowed, packets_denied, packets_logged per direction
  - Global `FIREWALL: Mutex<Firewall>` + `init()` function

**Async I/O Framework** (`kernel/src/async_io.rs`):

- Non-blocking I/O manager for network operations:
  - `AsyncIoManager` with submit/poll/cancel lifecycle
  - `AsyncOpKind` enum: TcpConnect, DnsResolve, TcpRead, TcpWrite
  - `AsyncResult` enum: Pending, Ready(Vec<u8>), Error(String), Cancelled
  - Per-app operation tracking with unique IDs
  - `cancel_app(app_name)`: Cancel all pending operations for an app
  - Timeout checking on poll
  - Global `ASYNC_IO: Mutex<AsyncIoManager>` + `init()` function

**Screen Reader (Accessibility)** (`kernel/src/screen_reader.rs`):

- Basic accessibility subsystem with announcement queue:
  - `ScreenReader` with announcement buffer and rate limiting
  - `AnnouncePriority` levels: Low, Normal, High, Assertive
  - `AnnounceKind` enum: 15 event types (WindowFocus, WindowClose, MenuOpen, MenuSelect, ButtonPress, TextInput, ScrollPosition, TabChange, DialogOpen, DialogClose, NotificationReceived, ErrorOccurred, ProgressUpdate, ListItemSelect, PageNavigation)
  - Deduplication to avoid repeating announcements
  - Rate limiting (max 5 announcements/second)
  - Output to serial console (COM1)
  - Persistent enable/disable state via preferences
  - Global `SCREEN_READER: Mutex<ScreenReader>` + `init()` function

**User-Customizable Theme Engine** (`kernel/src/themes.rs`):

- 6 built-in themes + custom theme support:
  - `ThemeId` enum: Dark, Light, SolarizedDark, Monokai, HighContrast, Nord, Custom
  - `ThemeColors` struct: 16 color properties (background, text, accent, element, frame, outline, hover_overlay, selected_overlay, editable, + semantic colors red/yellow/green/blue/purple)
  - `ThemeDefinition`: name, id, colors, is_dark flag
  - `build_stylesheet()`: Generate full `StyleSheet` from theme colors (text sizes, color mappings)
  - Theme persistence via preferences system (saved to FAT32)
  - Runtime switching without restart
  - Global `THEME_MANAGER: Mutex<ThemeManager>` + `init()` function

**Virtual Desktop Workspaces** (`kernel/src/workspaces.rs`):

- 4 virtual desktops for window organization:
  - `WorkspaceManager` with `Workspace` structs (name, BTreeSet<String> windows)
  - `switch_to(id)`: Switch active workspace with transition timestamp
  - `next()` / `prev()`: Cycle through workspaces (wrapping)
  - `move_window_to(window, workspace_id)`: Move window between desktops
  - `is_window_visible(window)`: Check if window belongs to current workspace
  - Default workspace names: Desktop 1-4
  - Transition animation support via timestamp tracking
  - Global `WORKSPACES: Mutex<WorkspaceManager>` + `init()` function

**New WASM Host APIs** (`kernel/src/wasm/mod.rs`):

- 12 new host functions exposed to WASM apps:
  - Firewall: `host_firewall_status` (enabled + allowed/denied stats), `host_firewall_set_enabled`, `host_firewall_rule_count`
  - Screen Reader: `host_screen_reader_set_enabled`, `host_screen_reader_enabled`
  - Themes: `host_get_theme` (current theme ID), `host_set_theme`, `host_get_theme_count`
  - Workspaces: `host_get_workspace` (current workspace ID), `host_set_workspace`, `host_get_workspace_count`
  - Async I/O: `host_async_pending_count`

**Guest Library Wrappers** (`guestlib/src/lib.rs`):

- Safe Rust wrappers for all new v2.4 host APIs:
  - `FirewallStatus` struct + `get_firewall_status()`, `firewall_set_enabled(bool)`, `firewall_rule_count()`
  - `screen_reader_set_enabled(bool)`, `screen_reader_enabled()`
  - `get_theme()`, `set_theme(id)`, `get_theme_count()`
  - `get_workspace()`, `set_workspace(id)`, `get_workspace_count()`
  - `async_pending_count()`
  - `FontScaleLevel` now implements `Display` trait

**New Terminal Commands** (`wasm_apps/terminal/src/shell/`):

- 5 new command groups:
  - `firewall [on|off]` (aliases: `ufw`, `iptables`): View firewall status, enable/disable, show rules and stats
  - `netstat` (alias: `ss`): Network connections overview + firewall summary
  - `theme [list|<name>]` (alias: `themes`): View/list/switch themes (dark/light/solarized/monokai/hc/nord)
  - `workspace [list|<1-4>]` (alias: `ws`): View/list/switch virtual desktops
  - `a11y [on|off]` (aliases: `accessibility`, `screenreader`): Screen reader status/control with font scale and high-contrast info
- Updated `dmesg` with firewall/theme/screen-reader/workspace init messages
- Tab completion updated for 13 new command names
- Help system updated with 3 new topics: `security`, `desktop`, `a11y`

### v2.3 — February 10, 2026

**Graceful Shutdown & Reboot** (`kernel/src/shutdown.rs`):

- Full shutdown/reboot lifecycle with data persistence:
  - `ShutdownState` enum: Running → Requested → Syncing → Persisting → Halting/Rebooting
  - `request_shutdown()` / `request_reboot()`: Set atomic flags checked in main loop
  - `sync_all()`: Save preferences + flush FAT32 metadata (standalone `sync` command)
  - `persist_ramfs()`: Backup app directories from ramfs to `/ramfs_backup/` on FAT32
  - `restore_ramfs()`: Restore backed-up files at boot (called from `main.rs`)
  - `shutdown()`: Full pipeline → sync → persist → ACPI S5 (QEMU port 0x604, fallback 0xB004)
  - `reboot()`: Full pipeline → sync → persist → keyboard controller reset (port 0x64, 0xFE) → triple fault fallback
  - Atomic state tracking with `SHUTDOWN_REQUESTED` / `REBOOT_REQUESTED` flags

**Cross-Core IPI (Inter-Processor Interrupts)** (`kernel/src/ipi.rs`):

- IPI framework for SMP scheduling coordination:
  - `send_ipi_to(cpu_id, vector)`: Targeted IPI via Local APIC ICR
  - `send_ipi_broadcast(vector)`: All-excluding-self broadcast
  - `send_schedule_ipi(cpu_id)`: Wake a core to run scheduler (vector 0xF0)
  - `send_tlb_shootdown()`: Broadcast TLB invalidation after page table changes (vector 0xF1)
  - `send_halt_ipi()`: Broadcast halt for shutdown (vector 0xF2)
  - Handler functions: `handle_schedule_ipi()`, `handle_tlb_shootdown()`, `handle_halt_ipi()`
  - Delivery status polling with timeout to avoid hangs

**Dynamic Display Resolution** (`kernel/src/display.rs`):

- Runtime GPU resolution detection instead of hardcoded 1366×768:
  - `DISPLAY_WIDTH` / `DISPLAY_HEIGHT` atomics, queried anywhere without locks
  - `set_dimensions(w, h)`: Called from VirtIO GPU driver after display info query
  - `detect_from_display_info(info)`: Parse VirtIO GPU display info response
  - `resolution_string()`: Human-readable string for UI display (e.g., "1920x1080")

**Font Size Scaling (Accessibility)** (`kernel/src/font_scaling.rs`):

- System-wide font size control for accessibility:
  - `FontScale` enum: Small (10/12/17), Normal (12/14/20), Large (14/16/23), ExtraLarge (16/18/26)
  - `current_scale()` / `set_scale()`: Thread-safe global state
  - `init_from_preferences()`: Load saved font scale at boot
  - `save_to_preferences()`: Persist choice to FAT32 via preferences system
  - Per-scale body/heading/title font sizes

**IPv6 Default Gateway Routing** (`kernel/src/network/mod.rs`):

- IPv6 gateway route integration with SLAAC:
  - Route `fe80::/0 → gateway` added in `TcpStack::new()` from `IPV6_CONFIG`
  - Dynamic gateway/global address updates in `poll_interface()`
  - `ipv6_status()` method for querying IPv6 configuration state

**New WASM Host APIs** (`kernel/src/wasm/mod.rs`):

- 7 new host functions exposed to WASM apps:
  - `host_request_shutdown`: Initiate system shutdown (terminal/settings only)
  - `host_request_reboot`: Initiate system reboot (terminal/settings only)
  - `host_sync_filesystems`: Sync ramfs to disk
  - `host_get_font_scale` / `host_set_font_scale`: Read/write font scaling level
  - `host_get_display_resolution`: Write (width, height) to WASM memory buffer
  - `host_get_cpu_info`: Write (online_cores, total_cores) to WASM memory buffer

**Guest Library Wrappers** (`guestlib/src/lib.rs`):

- Safe Rust wrappers for new host APIs:
  - `request_shutdown()`, `request_reboot()`, `sync_filesystems()`
  - `FontScaleLevel` enum with `get_font_scale()` / `set_font_scale()`
  - `DisplayResolution` struct with `get_display_resolution()`
  - `CpuInfo` struct with `get_cpu_info()`

**New Terminal Commands** (`wasm_apps/terminal/src/shell/`):

- 10 new shell commands:
  - `shutdown [-r] [-f]`: Graceful shutdown with confirmation prompt
  - `reboot`: Reboot system (alias for `shutdown -r -f`)
  - `poweroff`: Power off system (alias for `shutdown -f`)
  - `sync`: Flush all filesystems to disk
  - `lscpu` / `cpuinfo`: CPU and display information
  - `ip [addr|route|link]`: IP configuration (Linux-style)
  - `ifconfig`: Network interface status (BSD-style)
  - `fontscale [0-3|small|normal|large|xl]`: View/set font scaling
  - `dmesg`: Simulated kernel message log
- Tab completion updated for all new commands
- Help system updated with new commands in system and network sections

### v2.2 — February 10, 2026

**Dirty-Rect Tracking** (`kernel/src/dirty_rect.rs`):

- Partial framebuffer update system to reduce GPU bus bandwidth:
  - `DirtyTracker` with `mark_dirty(rect)`, `mark_window_dirty(rect)`, `force_full_flush()`
  - `get_flush_regions()`: Returns list of dirty rects to flush, merges overlapping regions
  - Smart merge: `merge_overlapping()` combines intersecting rects, `merge_smallest()` when exceeding MAX_DIRTY_RECTS (32)
  - Full-flush fallback when dirty area exceeds 75% of screen (`FULL_FLUSH_THRESHOLD`)
  - `DirtyStats`: Tracks partial flushes, full flushes, pixels saved
  - `DirtyRect` with `intersects()`, `merge()`, `clamp_to_screen()` geometry operations
  - Global `DIRTY_TRACKER: Mutex<DirtyTracker>`

- VirtIO GPU partial flush (`kernel/src/virtio/gpu.rs`):
  - `flush_rect(x, y, w, h)`: Transfers and flushes only the specified framebuffer region
  - Uses `VirtioGpuTransferToHost2d` with per-rect `VirtioGpuRect` and byte offset calculation

**Per-App Preemption Stats in System Monitor**:

- Scheduler stats host API (`kernel/src/wasm/mod.rs`):
  - `host_get_scheduler_task_count() -> i32`: Number of registered scheduler tasks
  - `host_get_scheduler_stats(buf, max) -> count`: Per-task binary data (56 bytes/entry): name, priority, cpu_time_ms, avg_step_ms, preemptions, frames_run, quantum_ms, was_preempted
  - `host_get_cpu_queue_stats(buf, max) -> count`: Per-CPU queue data (16 bytes/entry): cpu_id, task_count, active, total_time_ms

- Guest library wrappers (`guestlib/src/lib.rs`):
  - `SchedulerTaskInfo` struct: name, priority, was_preempted, cpu_time_ms, avg_step_ms, preemptions, frames_run, quantum_ms
  - `CpuQueueInfo` struct: cpu_id, task_count, active, total_time_ms
  - `get_scheduler_task_count()`, `get_scheduler_stats()`, `get_cpu_queue_stats()`

- System Monitor "Scheduler" tab (`wasm_apps/system_monitor/src/main.rs`):
  - 4th tab showing real-time scheduler statistics
  - Task table with columns: Name, Priority, CPU ms, Avg ms, Preemptions, Preempted?
  - Color-coded preemption indicator (green=no, red=yes)
  - CPU Run Queues section showing per-core task distribution and status

**DHCPv6 / SLAAC for IPv6** (`kernel/src/network/ipv6_autoconf.rs`):

- IPv6 Stateless Address Autoconfiguration (SLAAC):
  - `mac_to_eui64(mac) -> [u8; 8]`: IEEE EUI-64 identifier from MAC address (flip U/L bit, insert FF:FE)
  - `generate_link_local(mac) -> Ipv6Address`: Link-local address (fe80::EUI-64) from MAC
  - `generate_global_address(prefix, mac) -> Ipv6Address`: Global unicast from /64 prefix + EUI-64
  - `init_slaac(mac)`: Compute and store link-local + default global addresses
  - `process_ra_prefix(prefix, prefix_len, mac)`: Handle Router Advertisement prefix for global address
  - `set_gateway(gw)`, `add_dns_server(server)`: Store router/DNS from RA options

- DHCPv6 info-request (`kernel/src/network/ipv6_autoconf.rs`):
  - `build_dhcpv6_info_request(transaction_id) -> [u8]`: DHCPv6 Information-Request packet with Option Request List (DNS servers, domain list)
  - `parse_dhcpv6_reply(data) -> Vec<Ipv6Address>`: Parse DHCPv6 Reply for DNS Recursive Name Server option (option 23)

- Global state: `IPV6_CONFIG: Mutex<Ipv6Config>` (link_local, global, gateway, dns_servers, method), `SLAAC_ACTIVE: AtomicBool`
- Network module integration: `get_mac()` method on `TcpStack`, `pub mod ipv6_autoconf` in `network/mod.rs`

**VirtIO Sound Driver for PCM Audio** (`kernel/src/virtio_sound.rs`):

- Software audio mixer with VirtIO Sound device support:
  - `AudioMixer` with `play_stream(format, data)`, `stop_stream(id)`, `mix()`, `set_master_volume(0-100)`
  - `AudioStream`: PCM stream with `next_sample()` producing `AudioSample` (left/right i16)
  - `AudioSample`: Stereo sample with `mix()` (saturating add) and `apply_volume()` methods
  - `generate_tone(frequency, duration_ms, volume) -> Vec<AudioSample>`: Sine wave generation
  - `sine_approx(x) -> f32`: Fast sine approximation using Bhaskara I formula (no FPU trig needed)
  - Supported formats: `AudioFormat::Pcm16Le`, `AudioFormat::PcmU8`, `AudioFormat::Pcm32Le`

- VirtIO Sound device discovery:
  - `try_init(pci_devices)`: Scans PCI for VirtIO Sound (vendor 0x1af4, device 0x1059)
  - Global `AUDIO_MIXER: Mutex<AudioMixer>`, `MASTER_VOLUME: AtomicU32`, `SOUND_AVAILABLE: AtomicBool`
  - Constants: `SAMPLE_RATE=44100`, `CHANNELS=2`, `BUFFER_SAMPLES=1024`, `MAX_STREAMS=8`

**Per-CPU Run Queues and Work-Stealing Scheduler** (`kernel/src/scheduler.rs`):

- Per-CPU run queue infrastructure:
  - `CpuRunQueue` struct: `cpu_id`, `tasks: Vec<String>`, `active: bool`, `total_time_ms`
  - 16 CPU queues pre-allocated (`MAX_CPUS=16`), BSP (CPU 0) active by default
  - `activate_cpu(cpu_id)`: Mark CPU queue as active (called when AP comes online)
  - Tasks auto-assigned to least-loaded active CPU queue on registration

- Work-stealing load balancer:
  - `rebalance()`: Finds busiest and least-busy active queues, steals tasks to balance
  - `CpuRunQueue::steal()`: Removes lowest-priority task when queue exceeds `STEAL_THRESHOLD` (1)
  - `cpu_queue_stats()`: Returns per-CPU queue statistics (id, count, active, time)

- Enhanced scheduling:
  - `set_affinity(name, cpu_id)`: Set CPU core affinity for a task
  - `cpu_affinity` field in `ScheduledTask` and `TaskStats` (-1 = any core)
  - `least_loaded_cpu()`: Find optimal CPU for new task placement
  - Per-CPU time tracking in `end_task()`

**User/Group Permission Model** (`kernel/src/users.rs`):

- Unix-like user and group management:
  - `UserDatabase` with BTreeMap-backed users, groups, app-to-uid mapping, file ownership
  - `User` struct: uid, username, gid, groups, home_dir, is_system
  - `Group` struct: gid, name, members
  - Default users: root (uid 0), user (uid 1000), nobody (uid 65534)
  - Default groups: root (gid 0), users (gid 100), apps (gid 200)

- File permission model:
  - `FilePermissions`: owner, group, other `PermissionTriple` (read/write/execute) + octal mode
  - `FileOwnership`: uid, gid per path
  - `can_read()`, `can_write()`, `can_execute()`: Permission checks with uid/gid matching
  - Root (uid 0) always passes all permission checks
  - `from_mode(u16)`, `to_mode()`: Octal permission conversion (e.g., 0o755)
  - Global `USER_DB: Mutex<UserDatabase>` + `check_access(app_name, path, write) -> bool` helper

**Proportional Font Rendering** (`applib/src/drawing/proportional.rs`):

- Variable-width text rendering from existing bitmap fonts:
  - `ProportionalMetrics`: Per-char `advance_widths[95]`, `left_bearings[95]`, `kerning: BTreeMap<(u8,u8), i8>`
  - `from_font(font)`: Analyzes bitmap columns to measure actual glyph widths (non-empty column detection)
  - `draw_str_proportional(fb, text, x, y, font, color, blend)`: Render text with variable character spacing
  - `draw_proportional_in_rect(fb, text, rect, font, color, justification)`: Justified proportional text in bounding rect
  - `measure_str(text)`: Calculate rendered width of proportional text string
  - Built-in kerning pairs: AV, AW, AT, AY, FA, LT, LV, LW, LY, PA, TA, Te, To, Tr, Tu, Ty, VA, Ve, Vo, WA, We, Wo, Ya, Ye, Yo (-1px each)
  - Static metrics cache via `get_metrics(font)` for zero-alloc repeated use

### v2.1 — February 10, 2026

**VFS (Virtual File System) Layer** (`kernel/src/fs/vfs.rs`):

- Unified filesystem abstraction routing paths between ramfs and FAT32:
  - `/disk/*` paths → FAT32 persistent storage, all other paths → in-memory ramfs
  - `Vfs::list_dir()`: Merges virtual `/disk` mount point into root directory listings
  - `Vfs::read_file()`, `Vfs::write_file()`: Transparent routing with error conversion
  - `Vfs::stat()`: Returns `FileStat` with `file_type` and `size` for both backends
  - `Vfs::mkdir()`, `Vfs::delete_file()`, `Vfs::delete_dir()`: Full CRUD on both filesystems
  - `Vfs::has_persistent_storage()`: Checks FAT32 availability
  - `Vfs::disk_free_space()`: Reports free bytes on FAT32 partition
  - `Vfs::sync()`: Flushes FAT32 metadata to disk
  - Helper functions: `normalize_path()`, `is_disk_path()`, `strip_disk_prefix()`

**Capability-Based Security** (`kernel/src/capabilities.rs`):

- Per-app permission model controlling access to kernel APIs:
  - `Capability` enum with 16 variants: FileSystem, Network, Dns, Ping, Clipboard, Ipc, Notify, AppEnum, ProcessKill, Wallpaper, Preferences, SecurityStatus, Audio, SystemInfo, DiskRead, DiskWrite
  - `CapabilitySet` with `grant()`, `revoke()`, `has()` operations
  - `CapabilityRegistry` with per-app capability assignments and predefined profiles:
    - `full_access()`: All capabilities (terminal, settings, system_monitor)
    - `user_app()`: Standard app permissions without system access
    - `network_app()`: User app + Network/Dns/Ping (web_browser)
  - Global `CAPABILITIES: Mutex<CapabilityRegistry>` with `check_capability()` helper
  - Capability checks integrated into WASM host API calls with denial logging

**WASM Preemption Integration** (`kernel/src/wasm/mod.rs`):

- Scheduler-aware WASM execution in `WasmApp::step()`:
  - Registers each app as a scheduler task with `register_task()` on first step
  - Sets priority: `Interactive` for foreground app, `Normal` for background apps
  - Calls `begin_task()` before WASM execution, `end_task()` with timing after
  - Checks `should_preempt()` before execution — skips WASM step if quantum expired
  - Enables fuel + time hybrid preemption model

**Audio Guest API** (`kernel/src/wasm/mod.rs`, `guestlib/src/lib.rs`):

- WASM host APIs for audio playback:
  - `host_play_sound(sound_id: i32) -> i32`: Play 7 system sounds (click, error, notification, startup, warning, success, alert) with capability check
  - `host_beep(frequency: i32, duration_ms: i32) -> i32`: Custom tone generation (20–20000 Hz, 1–5000 ms) with capability check
- Guest library wrappers: `play_sound(SystemSound)`, `beep(frequency, duration_ms)`
- `SystemSound` enum: Click, Error, Notification, Startup, Warning, Success, Alert

**VFS Guest API** (`kernel/src/wasm/mod.rs`, `guestlib/src/lib.rs`):

- WASM host APIs for unified filesystem access:
  - `host_vfs_stat(path) -> (type, size)`: File metadata with DiskRead capability check for `/disk/` paths
  - `host_vfs_read_file(path) -> data`: Read file contents via VFS routing
  - `host_vfs_write_file(path, data) -> status`: Write file with DiskWrite capability check for `/disk/` paths
  - `host_vfs_has_disk() -> bool`: Check FAT32 availability
  - `host_vfs_disk_free() -> u64`: Query free disk space
- Guest library types: `VfsFileType` enum, `VfsStat` struct
- Guest library wrappers: `vfs_stat()`, `vfs_read_file()`, `vfs_write_file()`, `vfs_has_disk()`, `vfs_disk_free()`

**Encrypted Password Persistence** (`kernel/src/auth.rs`):

- Password hash saved to FAT32 for persistence across reboots:
  - File format: 4-byte magic `KNXP` (0x4B 0x4E 0x58 0x50) + 32-byte SHA-256 hash
  - `save_password_hash()`: Writes to `/passwd.hash` on FAT32
  - `load_from_disk()`: Validates magic header and restores hash at boot
  - `delete_password_file()`: Removes password file when password is cleared
  - Called from `set_password()` for automatic persistence

**Terminal Commands** (`wasm_apps/terminal/src/shell/commands_sys.rs`):

- `beep [sound_name | frequency duration]`: Play system sounds by name or custom tones
  - Named sounds: click, error, notification, startup, warning, success, alert
  - Custom tones: `beep 440 500` (440 Hz for 500 ms)
- `df`: Disk usage display with ramfs and FAT32 stats (size, used, available, use%)
- `mount`: Show mounted filesystems (ramfs on /, fat32 on /disk)
- Tab completion for all new commands

### v2.0 — February 9, 2026

**Multi-Core AP Startup** (`kernel/src/smp.rs`):

- **INIT-SIPI-SIPI sequence** for Application Processor startup:
  - Local APIC register access (base 0xFEE00000): ICR, SVR, EOI, ID registers
  - `enable_local_apic()` via Spurious Vector Register (SVR) with vector 0xFF
  - `send_ipi()` with delivery-wait loop for reliable inter-processor interrupts
  - 16-bit real-mode trampoline code installed at physical address 0x8000
  - Trampoline writes 0xCAFE magic signal to confirm AP reached code
  - `startup_aps()`: Iterates all discovered APs — INIT → 10ms delay → SIPI → 1ms → SIPI → poll for 0xCAFE
  - APs marked online upon successful startup confirmation
  - `busy_wait_ms()` spin-loop delay for APIC timing requirements
  - `ap_entry()` Rust entry point for future AP long-mode execution
  - Called at boot after SMP discovery (`smp.startup_aps()`)

**Unicode Pixel-Level Glyph Rendering** (`applib/src/drawing/text.rs`):

- Refactored `draw_char()` into 3-tier dispatch: ASCII bitmap → Unicode glyph → ASCII fallback
- **Box Drawing** (U+2500–U+257F): Light/heavy/double horizontal and vertical lines, all corner pieces (┌┐└┘), T-pieces (├┤┬┴), crossings (┼), thick line variants
- **Block Elements** (U+2580–U+259F): Upper/lower/left/right half blocks, full block (█), light/medium/dark shades (░▒▓), fractional blocks (▏▎▍▌▋▊▉)
- **Arrows** (U+2190–U+2193): 4 directional arrows (←↑→↓) with arrowhead rendering
- **Geometric Shapes**: Filled/outline squares (■□), 4 directional triangles (▲▶▼◀), filled/outline circles (●○), diamond (◆◇)
- **Braille Patterns** (U+2800–U+28FF): Full 256-pattern 2×4 dot matrix rendering with per-dot pixel placement
- Drawing primitives added: `draw_glyph_line` (Bresenham), `draw_glyph_hline_thick`/`draw_glyph_vline_thick`, `draw_glyph_filled_rect`, `draw_glyph_shade` (dithered fill), `draw_glyph_circle` (midpoint algorithm), `draw_braille`
- Existing 350+ ASCII transliteration fallback preserved as third tier

**CSS Engine** (`wasm_apps/web_browser/src/html/css.rs`):

- Complete CSS parsing and property resolution engine:
  - `CssProperties` struct with 18 fields: color, background-color, font-size, font-weight, text-align, display, margin (4 sides), padding (4 sides), width, height, border
  - `CssSelector` enum: Tag, Class, Id, Universal selectors
  - `Stylesheet::parse()`: Full CSS text parser — strips comments, extracts rule blocks, handles multiple selectors per rule
  - `Stylesheet::compute_style()`: Cascade matching (rule order) + inline `style=` attribute merge
  - `parse_declarations()`: Splits property:value pairs with semicolon handling
  - `parse_css_color()`: 20+ named colors (red, blue, darkblue, etc.), #RGB, #RRGGBB, #RRGGBBAA, `rgb()`, `rgba()` functions
  - `parse_css_length()`: px, em, rem, pt, %, bare numbers
  - `extract_style_text()`: Extracts content from `<style>` blocks in HTML source

**CSS Integration into Layout Engine** (`wasm_apps/web_browser/src/html/block_layout.rs`):

- `compute_block_layout()` now accepts `html_source` parameter, parses `<style>` blocks into `Stylesheet`
- `parse_block_tag()` computes per-element CSS: respects `display: none` (skips element), applies CSS background-color with `bgcolor` attribute fallback
- `get_inline_block_contents()` applies CSS text colors to rendered content
- `Block::Image` now carries `alt: String` field

**Image Placeholder Rendering** (`wasm_apps/web_browser/src/html/render_list.rs`):

- Images render as gray placeholder rectangles (RGB 240,240,240) with `[img: alt_text]` label
- Alt text rendered inside placeholder using `format_rich_lines`
- Proper placeholder sizing based on image dimensions

**App Store UI** (`wasm_apps/settings/src/main.rs`):

- New 6th tab "Apps" in Settings app:
  - Lists all 6 installed applications with colored icon placeholders (unique color per app)
  - Display names, descriptions, and "Installed" badges for each app
  - App descriptions: Terminal (command line shell), File Manager (file/directory browser), Settings (system configuration), Text Editor (rich text editing), Web Browser (HTTP/HTTPS), System Monitor (performance dashboard)
  - Footer note about `/apps/` filesystem for dynamic app installation
  - Visual consistency with existing Settings tab styling

**Built-in Test Framework** (`kernel/src/tests.rs`):

- Runtime test suite executing at boot (no_std — can't use cargo test):
  - `run_all_tests() -> TestSummary` with pass/fail/total counts
  - `test_assert!` and `test_assert_eq!` macros (non-panicking, returns `TestResult`)
  - **Memory tests**: Vec allocation, Box, String, large 4KB allocation, allocator stats consistency
  - **Geometry tests**: Rect center calculation, intersection, from_xyxy conversion
  - **Color tests**: RGB/RGBA construction, darken, lighten
  - **Filesystem tests**: mkdir, write_file, read_file, file overwrite, nonexistent file handling
  - **IPC tests**: Mailbox registration, send/recv, pending count
  - **Security tests**: NX/SMEP/SMAP consistency (enabled implies supported)
  - **Preferences tests**: set/get, overwrite, serialize format, dirty tracking
  - **Scheduler tests**: Task registration, priority assignment, execution ordering
  - **Allocator tests**: 100 small allocations, deallocation, 16-byte alignment verification
  - Results logged to serial output for CI automation

**PCI Hardware Detection** (`kernel/src/hardware.rs`):

- PCI bus enumeration for real hardware identification:
  - Scans all PCI buses (0-255), devices (0-31), functions (0-7)
  - `classify_device()` by PCI class/subclass/prog-if:
    - USB controllers (xHCI 3.0, EHCI 2.0, UHCI 1.x)
    - NVMe storage controllers
    - Intel GPU (device ID matching for HD/UHD/Iris families)
    - HDA audio controllers
    - SATA/AHCI storage controllers
  - `DetectedDevice` struct: vendor, device, class, subclass, name, `DriverStatus`
  - `HardwareInfo` with device list, `summary()`, and `count_by_type()`
  - `DriverStatus` enum: Active, Detected, NoDriver
  - Called at boot via `hardware::init()`, logs hardware inventory to serial

**Kernel Integration** (`kernel/src/main.rs`):

- Added `mod hardware;` and `mod tests;` module declarations
- `hardware::init()` called after security init for PCI hardware scan
- `tests::run_all_tests()` called after IPC initialization for boot-time testing
- SMP init block now calls `smp.startup_aps()` after `smp.init()` for AP startup
- Version updated to 2.0

### v1.9 — February 9, 2026

**CPU Security Hardening** (`kernel/src/security.rs`):

- **NX (No-Execute) bit** via IA32_EFER MSR — prevents code execution from data pages (stack/heap):
  - CPUID leaf 0x80000001 EDX bit 20 detection
  - EFER.NXE (bit 11) enablement via WRMSR
  - Detects if already enabled by UEFI firmware
- **SMEP (Supervisor Mode Execution Prevention)** via CR4 bit 20:
  - CPUID leaf 7 EBX bit 7 detection
  - Prevents kernel from executing user-mode code (ret2user mitigation)
- **SMAP (Supervisor Mode Access Prevention)** via CR4 bit 21:
  - CPUID leaf 7 EBX bit 20 detection
  - Prevents kernel from reading/writing user-mode pages
- Inline assembly uses push/pop rbx workaround for LLVM register reservation
- `SecurityStatus` struct with 6 booleans (enabled + supported per feature)
- `init()` called during kernel boot, `status()` queryable at runtime

**Persistent User Preferences** (`kernel/src/preferences.rs`):

- Key=value settings store backed by FAT32 filesystem:
  - Loads `/settings.conf` at boot, saves on change
  - Dirty-tracking to avoid unnecessary disk writes
  - Keys: `theme`, `wallpaper`, `lock_timeout_mins`, `audio_enabled`, `font_size`
  - `# Comment` support in config file format
  - Global `PREFERENCES: Mutex<UserPreferences>` for kernel-wide access
  - `serialize()` / `deserialize()` for persistent format

**Wallpaper System Enhancements** (`kernel/src/wallpaper.rs`):

- 2 new procedural wallpaper styles (6 total):
  - **Sunset** — warm orange/red/purple gradient with sun disk and horizon glow
  - **Aurora** — northern lights effect with sine-wave green/cyan light bands
- Runtime wallpaper switching via `change_style()` with cache invalidation
- `all()`, `name()`, `config_key()` metadata methods for UI integration
- Wallpaper choice persisted to preferences on change

**7 New WASM Host APIs** (`kernel/src/wasm/mod.rs`):

- `host_set_wallpaper(style_id)` — request wallpaper style change
- `host_get_wallpaper()` → current wallpaper style ID
- `host_set_preference(key, val)` — write persistent preference
- `host_get_preference(key, buf)` → read persistent preference
- `host_save_preferences()` — flush preferences to disk
- `host_kill_app(name)` — request termination of a WASM app
- `host_get_security_status(buf)` → pack 6 bytes of NX/SMEP/SMAP status

**Guest Library Updates** (`guestlib/src/lib.rs`):

- 7 new wrapper functions: `set_wallpaper()`, `get_wallpaper()`, `set_preference()`, `get_preference()`, `save_preferences()`, `kill_app()`, `get_security_status()`
- `SecurityStatus` struct for typed security feature queries

**Settings App v1.9** (`wasm_apps/settings/`):

- **Security tab** (new): NX/SMEP/SMAP status with color-coded indicators (green=enabled, yellow=supported, red=unsupported), feature descriptions, security model overview
- **Wallpaper picker** (Appearance tab): 6 wallpaper style cards with color preview swatches, click-to-select, persistent choice

**Terminal Shell Enhancements** (`wasm_apps/terminal/`):

- 3 new commands:
  - `security` — display CPU security features with colored status indicators
  - `wallpaper [style]` — view/change wallpaper (by name or ID), lists all 6 styles
  - `killapp <name>` — terminate a running WASM application (with safety check for self-kill)
- Updated help topic (`help sys`) with new commands
- Tab completion for `security`, `wallpaper`, `killapp`

**Kernel Integration** (`kernel/src/main.rs`):

- Added `security::init()` at boot after IDT setup
- Added `preferences::load_from_disk()` at boot after FAT32 mount
- Wallpaper style loaded from preferences (`wallpaper` key)
- Main loop: handles `pending_wallpaper_change` (style switch + save to disk)
- Main loop: handles `pending_app_kills` (close app + reset state + notification)
- System struct: `pending_wallpaper_change`, `current_wallpaper_id`, `pending_app_kills` fields

### v1.8 — February 8, 2026

**New Kernel Infrastructure:**

- **VirtIO Block Driver** (`kernel/src/virtio/block.rs`) — Persistent storage via VirtIO:
  - PCI discovery for VirtIO block devices (vendor 0x1AF4, device 0x1001/0x1042)
  - Graceful `try_new()` — non-fatal if no block device is present
  - Single-sector and multi-sector read/write operations
  - Flush command for write-back guarantee
  - Polling-based I/O with 1M iteration timeout
  - Read/write byte counters for statistics
  - 512-byte sector size with `VirtioBlkReqHeader` protocol

- **FAT32 Filesystem** (`kernel/src/fs/fat32.rs`) — Persistent filesystem on disk:
  - BPB (BIOS Parameter Block) parsing from boot sector
  - FAT (File Allocation Table) cluster chain management
  - Directory entry parsing with Long File Name (LFN) support
  - File operations: `read_file()`, `write_file()`, `list_dir()`, `stat()`
  - Directory operations: `mkdir()`, `delete_file()`, `delete_dir()`
  - Cluster allocation and freeing with FAT updates
  - 8.3 short name conversion from long names
  - Free space calculation and `sync()` for cache flush
  - Global `FAT32_FS: Mutex<Option<Fat32Fs>>` for kernel-wide access

- **Preemptive Scheduler** (`kernel/src/scheduler.rs`) — Timer-based task preemption:
  - 4 priority levels: Background (0.5x), Normal (1x), Interactive (1.5x), System (2x)
  - Per-task time quantum tracking with PIT timer integration
  - `PREEMPTION_PENDING` atomic flag set by timer interrupt handler
  - Task registration, priority assignment, begin/end tracking
  - Fairness-based execution ordering (priority first, then least CPU time)
  - Per-task statistics: CPU time, preemption count, average step time
  - Epoch-based stat resets for per-second metrics
  - Integrated into `interrupts.rs` timer handler via `timer_tick_preemption_check()`

- **SMP / Multi-core Discovery** (`kernel/src/smp.rs`) — CPU core enumeration:
  - ACPI RSDP discovery in BIOS ROM area (0xE0000–0xFFFFF) and EBDA
  - RSDP checksum validation and RSDT parsing
  - MADT (Multiple APIC Description Table) parsing for Local APIC entries
  - Per-CPU info: APIC ID, BSP flag, online/usable status, CPU index
  - BSP APIC ID detection via CPUID leaf 0x01
  - `SMP_MANAGER` global with `init()`, `cpu_count()`, `usable_cpu_count()`
  - Supports up to 64 CPUs
  - Foundation for future AP startup (INIT-SIPI-SIPI)

- **User Authentication** (`kernel/src/auth.rs`) — Password-based security:
  - Built-in SHA-256 implementation (no_std, from-scratch, no external deps)
  - Password set/change/remove with salted hashing
  - `verify()` with constant-time comparison (prevents timing attacks)
  - Brute-force protection: lockout after 5 failed attempts with escalating delay
  - `PasswordInput` struct: character buffer, dot masking, backspace, error messages
  - 3-second error display timeout
  - 64-character maximum password length

- **Audio Subsystem** (`kernel/src/audio.rs`) — PC Speaker sound output:
  - PIT Channel 2 square wave generation for tone playback
  - 7 system sound effects: Beep, Alert, Error, Click, Lock, Unlock, Startup
  - Multi-tone sequences with gaps between notes
  - Custom frequency and duration playback API
  - Global enable/disable toggle
  - Timer-tick-based spin delay for duration control

**Lock Screen Authentication UI:**

- **Password mode** (`kernel/src/lock_screen.rs` enhanced):
  - Password input field with rounded border (260×32px)
  - Asterisk masking for typed characters
  - "Enter password..." placeholder when empty
  - Enter to submit, Backspace to delete, Escape to clear
  - Full keycode-to-ASCII conversion (a-z, 0-9, symbols, shift variants)
  - Red error message display on failed attempts ("Incorrect password (N/5)")
  - Lockout message with countdown timer ("Locked out. Try again in Ns")
  - Graceful fallback: no password set = original any-key-to-unlock behavior

**Network Stack Enhancements:**

- **IPv6 Support** — Dual-stack networking:
  - Added `proto-ipv6` feature to smoltcp in Cargo.toml
  - IPv6 link-local address (fe80::1/10) configured on interface
  - `Ipv6Address` import added to network module
  - Full IPv6 protocol support via smoltcp

**Extended Unicode Coverage** (`applib/src/drawing/text.rs`):

- **Geometric shapes** (15 chars): ■ □ ▪ ▫ ▲ ▶ ▼ ◀ ○ ● ◘ ◙ etc.
- **Miscellaneous symbols** (20 chars): ☀ ☁ ★ ☆ ☎ ☐ ☑ ☒ ☔ ☕ ☺ ♥ ♪ ♫ ✓ ✔ ✖ ✗ ✘
- **Currency symbols** (8 chars): ¢ £ ¥ ₣ ₤ ₧ ₹ ₿
- **Superscript/subscript** (20 chars): ¹ ² ³ ⁰ ⁴-⁹ ₀-₉
- **Greek alphabet** (48 chars): Full uppercase Α-Ω and lowercase α-ω
- **CJK punctuation + fullwidth ASCII** (96 chars): 、。！-～
- Total Unicode fallback coverage: **350+ characters** (up from 150+)

**Build System & QEMU:**

- **Disk image auto-creation** in `make.py`:
  - `_create_disk_image()` creates 64MB raw disk image on first run
  - FAT32 formatting via `newfs_msdos` (macOS)
  - Reuses existing disk.img on subsequent runs
- **VirtIO block device** added to QEMU args:
  - `-drive file=disk.img,format=raw,if=none,id=hd0 -device virtio-blk-pci,drive=hd0`

**Kernel Integration:**

- `main.rs`: Added 4 new modules (audio, auth, scheduler, smp), VirtIO block init with FAT32 mount attempt, SMP discovery, audio init
- `interrupts.rs`: Timer handler calls `scheduler::timer_tick_preemption_check()` for preemption
- `virtio/mod.rs`: Added `pub mod block;`
- `fs/mod.rs`: Added `pub mod fat32;`
- `Cargo.toml`: Added `proto-ipv6` feature to smoltcp

### v1.7 — February 8, 2026

**New Application (total now 6):**

- **Settings** (`wasm_apps/settings/`) — System settings and information app:
  - Four-tab interface: About, Appearance, System, Keyboard
  - **About tab**: Knox OS version, kernel architecture, Rust toolchain, WASM engine (wasmi 0.40.0), network stack, display info, running app count, live uptime counter
  - **Appearance tab**: Theme preview cards for 4 themes (Dark, Light, Solarized Dark, Monokai) with color swatches (BG/FG/Accent/Element/Frame), click-to-select with accent highlight
  - **System tab**: Hardware info (architecture, CPU, memory, display, input, network), running applications list with alternating row colors, green operational status indicator
  - **Keyboard tab**: 15 keyboard shortcut reference with badge-style key rendering
  - Auto-refreshing system info (2-second intervals)
  - Status bar showing version info
  - Settings gear icon (SVG → PNG) in activity bar

**New Desktop Features:**

- **Wallpaper System** (`kernel/src/wallpaper.rs`) — Procedural wallpaper rendering:
  - 4 wallpaper styles: DarkGrid (dark gradient + grid overlay + vignette), DeepBlue (blue gradient + radial highlight, default), NightSky (purple gradient + pseudo-random stars), WavePattern (sine wave layers)
  - One-time cached rendering at boot for zero per-frame cost
  - Uses `num_traits::Float::sin()` for no_std math
  - Pixel-by-pixel copy to framebuffer via `draw_fast()`

- **Desktop Icons** (`kernel/src/desktop_icons.rs`) — Desktop shortcut system:
  - Grid layout of app icons starting below activity bar (vertical-first filling)
  - 32×32 icons from resources with app name labels
  - Double-click to launch applications
  - Hover highlight with rounded glass overlay
  - Text shadow for wallpaper readability

- **Lock Screen** (`kernel/src/lock_screen.rs`) — Screen lock with idle detection:
  - Starts locked at boot, any key/click unlocks
  - Manual lock via Ctrl+Alt+L
  - Auto-lock after 5 minutes of idle (no input)
  - Semi-transparent dark overlay with large clock and date display
  - "Knox OS" branding with version
  - Pulsing "Press any key to unlock" hint text (sine wave animation)
  - 500ms fade-out unlock animation

**Desktop UI Improvements:**

- **Enhanced Taskbar** (`kernel/src/taskbar.rs`):
  - Hover tooltips showing app name (rounded background, positioned to right of activity bar)
  - Minimized window indicator: small accent-colored dot below icon
  - Dimmer left-edge indicator for minimized windows (gray vs white for active)

- **Enhanced Notifications** (`kernel/src/notification.rs`):
  - Click-to-dismiss: clicking a notification removes it immediately
  - Slide-in animation from right edge (0.25s duration)
  - Duration increased to 5 seconds
  - Takes InputState for click detection

**Kernel Changes:**
- `resources.rs`: APPLICATIONS array expanded from 5 to 6 entries, added `SETTINGS_ICON` + `SETTINGS_ICON_SM`
- `main.rs`: Added wallpaper, desktop_icons, lock_screen modules; replaced gradient background with wallpaper; wrapped desktop rendering in lock screen visibility check; integrated all new systems into main loop
- `notification.rs`: `draw()` method now takes `&InputState` parameter

**Build System:**
- `make.py`: WASM_APPS list updated to 6 apps (added "settings")
- `make_icons.py`: Added "settings" to activity_bar_icons list
- Generated settings icon PNGs at 32px and 20px sizes

### v1.6 — February 8, 2026

**New Applications (2 apps, total now 5):**

- **File Manager** (`wasm_apps/file_manager/`) — GUI file browser:
  - Directory browsing using the table widget (Name, Type, Size columns)
  - Toolbar with 8 buttons: Back (←), Up, Refresh, New File, New Dir, Delete, Rename
  - Path bar showing current directory
  - Status bar with directory/file counts and timed status messages
  - Create new files and folders via input overlay dialog
  - Delete files and folders with error reporting
  - Rename files (copy+delete pattern for WASI compatibility)
  - Full keyboard navigation: Up/Down to select, Enter to open dir, Backspace for parent, Delete to remove
  - Back/forward navigation history (50 entries)
  - Sorted entries: directories first, then files, alphabetically within each group
  - Action-collection pattern to avoid borrow checker conflicts with framebuffer

- **System Monitor** (`wasm_apps/system_monitor/`) — Real-time system dashboard:
  - Three-tab interface using TabBar widget: Overview, Performance, Processes
  - **Overview tab**: System uptime, timer ticks, running app count, FPS counter, mini frame-time graph, running applications list
  - **Performance tab**: Real-time frame time graph (120-sample history), WASM fuel consumption graph, performance statistics (avg/max frame time, avg/max fuel, FPS)
  - **Processes tab**: Running process list with PID, name, and status columns, alternating row colors
  - Auto-refreshing app list (every 2 seconds)
  - Uses graph widget for real-time data visualization

**Removed Demo Apps:**
- Removed `calculator`, `cube_3d`, `chronometer`, and `paint` — trivial demo apps are out of scope for an AI/LLM-native platform
- APPLICATIONS array reduced from 8 to 5 entries
- Removed associated icons: `CUBE_ICON`, `CHRONO_ICON`, `CHIP_ICON` and their small variants

**Kernel Changes:**
- `resources.rs`: APPLICATIONS array updated to 5 entries (expanded to 6 in v1.7)
- Icons: `HOME_ICON` + `HOME_ICON_SM` for File Manager, `SPEEDOMETER_ICON` + `SPEEDOMETER_ICON_SM` for System Monitor
- `make_icons.py`: Activity bar icons updated (removed cube, chronometer)

**Build System:**
- `make.py`: WASM_APPS list updated to 5 apps
- All apps use `.cargo/config.toml` with `[build] target = "wasm32-wasip1"`

### v1.5 — February 8, 2026

**New Kernel Features:**
- **ICMP Ping** (`kernel/src/network/mod.rs`) — Real ICMP echo request/reply via smoltcp `socket-icmp` feature. `icmp_ping(target, clock)` sends echo request with ident 0x1234, polls for reply with 5-second timeout, returns RTT in milliseconds as `f64`. Integrated as `host_icmp_ping` WASM host API.
- **Notification System** (`kernel/src/system.rs`, `kernel/src/wasm/mod.rs`) — `PendingNotification` struct with title, message, and notification level. WASM apps call `host_notify(title, msg, level)` to push notifications. Kernel main loop drains pending notifications into the notification manager each frame.
- **App Enumeration API** (`kernel/src/system.rs`, `kernel/src/wasm/mod.rs`) — System struct tracks `app_names: Vec<String>` populated from z-ordered window list each frame. `host_get_app_count()` and `host_get_app_name(index, buf, len)` WASM host APIs allow apps to query running applications.

**New Guest Library APIs** (`guestlib/src/lib.rs`):
- `icmp_ping(ip: [u8; 4]) -> Option<f64>` — Send ICMP ping, returns RTT in ms
- `NotifyLevel` enum (Info, Warning, Error) + `notify(title, message, level)` — Push notification
- `get_app_count() -> u32` / `get_app_name(index) -> Option<String>` — List running apps

**Unicode/Latin-1 Fallback** (`applib/src/drawing/text.rs`):
- `unicode_fallback(code: u32) -> u8` function with 150+ transliterations covering:
  - Latin Extended-A/B (ě→e, ñ→n, ø→o, ß→s, etc.)
  - Latin-1 Supplement (©→C, ®→R, ±→+, ÷→/, etc.)
  - Punctuation & symbols (—→-, …→., €→E, £→L, ¥→Y, etc.)
  - Mathematical operators (≠→!, ≤→<, ≥→>, ≈→~, etc.)
  - Greek letters (α→a, β→b, π→p, Σ→S, etc.)
  - Arrows (←→<, →→>, ↑→^, ↓→v)
  - Box-drawing characters to ASCII equivalents (│→|, ─→-, ┌→+, etc.)
  - CJK/Emoji/Braille placeholder (→`?`)
- `draw_char()` now uses `unicode_fallback` instead of replacing non-ASCII with space

**Terminal: 15+ New Commands** (`wasm_apps/terminal/src/shell.rs`):
- **Text Processing:** `grep` (line-number + pattern highlighting in red), `sort` (lexicographic), `uniq` (deduplicate adjacent), `rev` (reverse string)
- **File Search:** `find` (recursive with `-name` wildcard patterns, depth limit 10), `tree` (Unicode box-drawing connectors ├──/└──/│, depth limit 8, dir/file counts)
- **Data Inspection:** `xxd` (hex dump — 16-byte lines with offset + hex + ASCII, 512 byte limit)
- **Utilities:** `seq` (number range, max 200), `basename`, `dirname`, `yes` (20 lines)
- **System:** `notify` (push notification with title/message/level), `apps` (list running applications)
- **Shell Features:** `alias`/`unalias` (command aliases), `which` (command lookup)

**Terminal: Major Shell Enhancements:**
- **Pipe support** — `cmd1 | cmd2 | cmd3` with pipe-compatible handlers for grep, sort, uniq, head, tail, wc, rev
- **Command aliases** — Default aliases: `ll→ls -l`, `la→ls -la`, `cls→clear`. Custom via `alias name=value`
- **Clipboard paste** — Ctrl+V pastes from system clipboard into command line
- **Enhanced `ls -l`** — Long format showing file type indicator (d/-), permissions (rwx), file size with human-readable units (B/K/M), `-a` flag for hidden files
- **Real ICMP ping** — `ping` now sends actual ICMP echo requests with RTT display (previously DNS-only)
- **`format_size()`** helper — Formats bytes as B/K/M
- **`strip_ansi()`** helper — Strips ANSI escape sequences for pipe processing
- **`name_matches_pattern()`** helper — Glob-style wildcard matching for `find -name`
- Total command count: **45+** (up from 30+)

### v1.4 — February 8, 2026

**New Features:**
- **Animation Framework** (`applib/src/uitk/animation.rs`) — Full-featured animation system:
  - **14 easing functions**: Linear, EaseIn, EaseOut, EaseInOut, QuadIn/Out/InOut, CubicIn/Out/InOut, ElasticOut, BackOut, BounceOut
  - **Tween engine**: Animate from/to values over duration with configurable easing, looping, and ping-pong modes
  - **Spring physics**: Position/velocity simulation with configurable stiffness/damping. Presets: snappy (300/20), gentle (100/15), bouncy (400/10)
  - **AnimationManager**: Named animation registry using BTreeMap for managing multiple concurrent tweens
  - All math functions (sqrt, sin, exp, pow) implemented without std using manual approximations
- **Color Utility Methods** (`applib/src/lib.rs`) — Rich color manipulation API:
  - `with_alpha(alpha)` — Create color variant with specified alpha
  - `darken(factor)` / `lighten(factor)` — Adjust brightness (0.0=unchanged, 1.0=black/white)
  - `mix(other, ratio)` — Linear interpolation between two colors
  - `luminance()` — Weighted RGB luminance (0.299R + 0.587G + 0.114B)
  - `is_dark()` — Returns true if luminance < 0.5
  - `contrast_text()` — Returns BLACK or WHITE for optimal readability
- **System Info Host APIs** (`kernel/src/wasm/mod.rs`) — Two new WASM host functions:
  - `host_get_system_info(info_type, result_addr)` — Query system uptime (f64 ms) or timer ticks (u64)
  - `host_get_random(buf_addr, buf_len)` — Fill buffer with random bytes from kernel SmallRng
- **Guest Library Wrappers** (`guestlib/src/lib.rs`) — Safe Rust wrappers:
  - `get_uptime() -> f64` — System uptime in milliseconds
  - `get_timer_ticks() -> u64` — Hardware timer tick count
  - `get_random(buf) -> usize` — Fill buffer with random bytes
- **Drawing Primitives** (`applib/src/drawing/primitives.rs`) — Three new shape drawing functions:
  - `draw_circle(fb, cx, cy, radius, color, blend)` — Filled circle via scanline rendering
  - `draw_circle_outline(fb, cx, cy, radius, color, blend, thickness)` — Circle outline/ring with configurable thickness
  - `draw_ellipse(fb, cx, cy, rx, ry, color, blend)` — Filled ellipse using ellipse equation
- **Terminal Commands** (`wasm_apps/terminal/src/shell.rs`) — Three new system commands:
  - `rand [N]` — Generate N random bytes (1-256, default 16) displayed as hex
  - `ticks` — Display hardware timer tick count
  - `sleep <seconds>` — Conceptual sleep command (no-op in WASM single-threaded environment)

**Bug Fixes & Improvements:**
- **Fixed `draw_rounded_rect_outline`** — Replaced non-functional placeholder with proper scanline-based rendering. Correctly handles corner arcs and flat side regions using inner/outer radius computation.
- Tab completion updated with new commands (rand, ticks, sleep).
- Help text now includes "System:" category for system-related commands.
- Total host API count increased from ~20 to ~24.

### v1.3 — February 8, 2026

**New Features:**
- **Kernel DNS Resolver** — Full DNS resolver at kernel level (`kernel/src/network/dns.rs`) with BTreeMap-based cache (64 entries, TTL-based expiry). Builds DNS query packets, parses A record responses, handles compression pointers. Integrated into `TcpStack::dns_resolve()` using ephemeral UDP sockets to DHCP-configured DNS server.
- **DNS Host API** — `host_dns_resolve(hostname_addr, hostname_len, result_addr)` WASM host function. Guest library wrapper `dns_resolve(hostname) -> Result<[u8; 4]>`.
- **TCP Listen/Accept** — `tcp_listen(port)` and `tcp_is_active(handle)` for TCP server mode. Host APIs and guest library wrappers.
- **Terminal Built-in Shell** — Complete command interpreter with 30+ commands replacing the Python REPL as default mode:
  - **General:** help, clear, echo, date, uptime, uname, whoami, hostname, about, neofetch, colors, true, false
  - **Filesystem:** pwd, cd, ls, cat, head, tail, wc, stat, mkdir, touch, write, cp, mv, rm, rmdir
  - **Network:** dns (hostname resolution), ping (DNS verification)
  - **Environment:** env, export, history
  - **Line editing:** Ctrl+A/E (home/end), Ctrl+U/K (kill), Ctrl+W (delete word), Ctrl+C (cancel), Ctrl+L (clear)
  - **Command history** with up/down arrow navigation
  - **Tab completion** for all command names
  - **Dynamic CWD prompt** showing current directory
  - **Colored output** with ANSI escape sequences
- **Table Widget** (`applib/src/uitk/widgets/table.rs`) — Scrollable table with headers, fixed/flex column widths, selectable rows with hover highlighting, scroll wheel support, scrollbar thumb.
- **Modal Dialog Widget** (`applib/src/uitk/widgets/dialog.rs`) — Overlay dialog with dim backdrop, title bar with icon indicator (info/warning/error/question), word-wrapped message, configurable buttons (Ok/Cancel/Yes/No/Custom), Escape to dismiss. Convenience constructors: `alert()`, `confirm()`, `error()`.
- **Tree View Widget** (`applib/src/uitk/widgets/tree_view.rs`) — Collapsible tree with expand/collapse arrows, connector lines, indentation, selection highlighting, scroll support.
- **Undo/Redo System** (`applib/src/uitk/undo.rs`) — Edit operation stack with Insert/Delete/Replace operations. Automatic merging of rapid consecutive character inputs. 200-step history. Ctrl+Z (undo) and Ctrl+Y / Ctrl+Shift+Z (redo) integrated into text editor.
- **Anti-aliased Line Drawing** — Xiaolin Wu's algorithm (`draw_line_aa`) for smooth sub-pixel lines. Thick line support (`draw_line_thick`) using parallel AA lines.
- **Gradient Fills** — `draw_gradient_rect()` with 4 directions (Vertical, Horizontal, DiagonalDown, DiagonalUp). `draw_radial_gradient()` for radial gradients. Color interpolation via `lerp_color()`.

**Bug Fixes & Improvements:**
- Text editor now supports Ctrl+Z/Y for undo/redo.
- Text editor file save/open via Ctrl+S/O with status bar feedback.
- Terminal prompt dynamically shows current working directory.
- Widget count increased from 15 to 18 (table, dialog, tree view).
- New public exports in `applib::uitk`: `TableConfig`, `TableState`, `DialogConfig`, `DialogResult`, `TreeNode`, `TreeViewConfig`, `TreeViewState`, `UndoHistory`, `EditOp`, etc.

### v1.2 — February 7, 2026

**New Features:**
- **IDT & Interrupt Handling** — Full IDT setup with 19 CPU exception handlers (including page fault with CR2, GPF with error code, double fault). PIC 8259 initialization with timer (IRQ0) and keyboard (IRQ1) hardware interrupts.
- **Page Frame Allocator** — Bitmap-based physical memory page allocator supporting up to 4 GiB. Single and contiguous multi-page allocation with free-hint optimization.
- **Virtual Memory Manager** — Page table management with `map_page()`, `unmap_page()`, `remap_page()`, guard page creation, `alloc_and_map()` for combined frame allocation and mapping, range operations.
- **Thread-safe Heap Allocator** — Replaced `UnsafeCell` with `spin::Mutex` in the heap allocator for safe concurrent access.
- **UDP Socket Support** — Full UDP API in kernel, WASM host bridge, and guest library. `udp_bind()`, `udp_send()`, `udp_recv()`, `udp_close()`.
- **DHCP Client** — Automatic IP configuration via DHCPv4 with static fallback (10.0.2.15/24, gateway 10.0.2.2, DNS 10.0.2.3).
- **Text Selection** — `TextSelection` struct with anchor/cursor model. Ctrl+A (select all), Ctrl+C (copy), Ctrl+X (cut), Ctrl+V (paste), Delete key, Home/End, Shift+arrow for selection extending.
- **Focus Management** — `FocusManager` with Tab/Shift+Tab navigation between widgets. Focus ring, explicit focus via mouse click, per-frame registration.
- **IPC Message Passing** — Mailbox-based inter-process communication. `ipc_send()`, `ipc_recv()`, `ipc_pending()` with broadcast support. Host APIs and guest library wrappers.
- **Dynamic App Loader** — Discover WASM apps from `/apps/` directory. JSON manifest format. `install_app()` / `uninstall_app()` / `load_app_wasm()`.
- **Theme System** — Dark and Light theme presets via `StyleSheet::dark_theme()` / `light_theme()`. `StyleSheetColors::dark()` / `light()`.
- **Titlebar Buttons** — Minimize (—), maximize (□), close (×) buttons rendered in window titlebars with hover highlighting (yellow, green, red).
- **Window Snapping** — Ctrl+Alt+Left/Right to snap window to left/right half of screen.
- **Window Keyboard Shortcuts** — Ctrl+Alt+Up to maximize, Ctrl+Alt+Down to restore/minimize. Alt+Tab now skips minimized windows.
- **Extended Keymap** — Added Home, End, Page Up, Page Down, Insert keycodes.

**Bug Fixes & Improvements:**
- Increased socket storage from 8 to 16.
- Alt+Tab now correctly skips minimized windows.
- Filesystem has convenience path methods: `open_path()`, `unlink_path()`, `rmdir_path()`.
- `/apps/` directory auto-created at boot for dynamic app loading.
- `RichText::get_text_range()` for extracting text substrings.

**Technical Debt Resolved:**
- Thread-unsafe allocator (now uses spin::Mutex)
- Hardcoded network config (DHCP auto-configuration)
- No inter-app communication (IPC mailboxes)
- No text selection (full selection with clipboard integration)
- Missing Home/End/PgUp/PgDn keycodes

### v1.1 — February 6, 2026

**New Features:**
- **In-memory filesystem (ramfs)** — Full hierarchical filesystem with create/read/write/seek/delete/rename/mkdir/rmdir/readdir/stat.
- **WASI filesystem API** — 15+ WASI Preview1 filesystem functions.
- **System clipboard** — Kernel-managed text clipboard with host API.
- **Input system overhaul** — Ctrl, Alt, CapsLock modifiers. Tab, Escape, F1-F12, Delete, Grave/Tilde.
- **Bresenham line drawing** and **rounded rectangles** primitives.

**Bug Fixes & Improvements:**
- Panic handler with file/line/column and HLT halt.
- Log level filtering for kernel and guest loggers.
- WASM fuel metering set to 100M/step.
- Network sockets increased from 1 to 8.
- Port allocation wraps at 64000.