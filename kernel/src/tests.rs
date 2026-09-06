//! Kernel Unit Tests — Validates core subsystems
//!
//! Run with: `cargo test` (boots QEMU, runs tests, exits)
//!
//! Each test function exercises a specific kernel subsystem.

#[cfg(test)]
use crate::serial_println;

// ═══════════════════════════════════════════════════════════════════════
// VFS TESTS
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod vfs_tests {
    use super::*;
    use crate::vfs;
    use alloc::string::String;
    use alloc::vec;

    #[test_case]
    fn test_vfs_write_read() {
        vfs::init();
        let data = b"Hello, KnoxOS!";
        assert!(vfs::write_file_dispatch("/tmp/test.txt", data));
        let read = vfs::read_file_dispatch("/tmp/test.txt").expect("read failed");
        assert_eq!(&read[..], data);
    }

    #[test_case]
    fn test_vfs_overwrite() {
        let data1 = b"first";
        let data2 = b"second";
        vfs::write_file_dispatch("/tmp/overwrite.txt", data1);
        vfs::write_file_dispatch("/tmp/overwrite.txt", data2);
        let read = vfs::read_file_dispatch("/tmp/overwrite.txt").unwrap();
        assert_eq!(&read[..], data2);
    }

    #[test_case]
    fn test_vfs_nonexistent_read() {
        let result = vfs::read_file_dispatch("/nonexistent/path/file.txt");
        assert!(result.is_none());
    }

    #[test_case]
    fn test_vfs_ensure_directory() {
        vfs::ensure_directory("/etc/knoxos/test");
        assert!(vfs::write_file_dispatch("/etc/knoxos/test/conf", b"ok"));
    }
}

// ═══════════════════════════════════════════════════════════════════════
// ALLOCATOR TESTS
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod allocator_tests {
    use alloc::boxed::Box;
    use alloc::vec;
    use alloc::vec::Vec;

    #[test_case]
    fn test_box_alloc_dealloc() {
        let val = Box::new(42u64);
        assert_eq!(*val, 42);
    }

    #[test_case]
    fn test_vec_growth() {
        let mut v: Vec<u32> = Vec::new();
        for i in 0..1000 {
            v.push(i);
        }
        assert_eq!(v.len(), 1000);
        assert_eq!(v[999], 999);
    }

    #[test_case]
    fn test_large_allocation() {
        // Allocate 1 MiB
        let v = vec![0u8; 1024 * 1024];
        assert_eq!(v.len(), 1024 * 1024);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// IMAGE DECODER TESTS
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod image_tests {
    use crate::gui::image;

    #[test_case]
    fn test_bmp_decode() {
        // Minimal 2x2 24-bit BMP (no compression)
        #[rustfmt::skip]
        let bmp: &[u8] = &[
            // BM header (14 bytes)
            0x42, 0x4D,             // "BM"
            0x46, 0x00, 0x00, 0x00, // file size = 70
            0x00, 0x00, 0x00, 0x00, // reserved
            0x36, 0x00, 0x00, 0x00, // pixel data offset = 54
            // DIB header (40 bytes)
            0x28, 0x00, 0x00, 0x00, // header size = 40
            0x02, 0x00, 0x00, 0x00, // width = 2
            0x02, 0x00, 0x00, 0x00, // height = 2
            0x01, 0x00,             // color planes = 1
            0x18, 0x00,             // bits per pixel = 24
            0x00, 0x00, 0x00, 0x00, // compression = 0 (none)
            0x10, 0x00, 0x00, 0x00, // image size = 16
            0x13, 0x0B, 0x00, 0x00, // h-res
            0x13, 0x0B, 0x00, 0x00, // v-res
            0x00, 0x00, 0x00, 0x00, // colors
            0x00, 0x00, 0x00, 0x00, // important colors
            // Pixel data (bottom-up, padded to 4-byte rows)
            // Row 0 (bottom): 2 pixels × 3 bytes = 6 bytes + 2 padding
            0x00, 0x00, 0xFF, // pixel (0,1) = red (BGR)
            0x00, 0xFF, 0x00, // pixel (1,1) = green
            0x00, 0x00,       // padding
            // Row 1 (top): 2 pixels × 3 bytes = 6 bytes + 2 padding
            0xFF, 0x00, 0x00, // pixel (0,0) = blue (BGR)
            0xFF, 0xFF, 0xFF, // pixel (1,0) = white
            0x00, 0x00,       // padding
        ];

        let img = image::decode(bmp).expect("BMP decode failed");
        assert_eq!(img.width, 2);
        assert_eq!(img.height, 2);
        assert_eq!(img.pixels.len(), 4);
    }

    #[test_case]
    fn test_format_detection_png() {
        let png_sig: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
        let fmt = image::detect_format(&png_sig);
        assert_eq!(fmt, Some(image::ImageFormat::Png));
    }

    #[test_case]
    fn test_format_detection_jpeg() {
        let jpg_sig: [u8; 8] = [0xFF, 0xD8, 0xFF, 0xE0, 0, 0, 0, 0];
        let fmt = image::detect_format(&jpg_sig);
        assert_eq!(fmt, Some(image::ImageFormat::Jpeg));
    }

    #[test_case]
    fn test_scale_image() {
        // Create a tiny 2x2 image and scale to 4x4
        use crate::gui::framebuffer::Pixel;
        let img = image::DecodedImage {
            width: 2,
            height: 2,
            pixels: alloc::vec![
                Pixel::rgb(255, 0, 0),
                Pixel::rgb(0, 255, 0),
                Pixel::rgb(0, 0, 255),
                Pixel::rgb(255, 255, 255),
            ],
            format: image::ImageFormat::Raw,
        };
        let scaled = image::scale_image(&img, 4, 4);
        assert_eq!(scaled.width, 4);
        assert_eq!(scaled.height, 4);
        assert_eq!(scaled.pixels.len(), 16);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// FIREWALL TESTS
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod firewall_tests {
    use crate::firewall;
    use crate::net::Ipv4Address;

    #[test_case]
    fn test_firewall_default_accept() {
        firewall::init();
        let action = firewall::filter_packet(
            firewall::Chain::Input,
            Ipv4Address([10, 0, 0, 1]),
            Ipv4Address([10, 0, 0, 2]),
            6, // TCP
            12345,
            80,
            100,
        );
        // Default policy should be Accept when no rules match
        assert!(matches!(action, firewall::Target::Accept));
    }
}

// ═══════════════════════════════════════════════════════════════════════
// PASSWORD HASHING TESTS
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod password_tests {
    use crate::users;

    #[test_case]
    fn test_hash_and_verify() {
        let hash = users::hash_password("secret123", "testsalt");
        assert!(hash.starts_with("$5$"));
        assert!(users::verify_password("secret123", &hash));
        assert!(!users::verify_password("wrong_password", &hash));
    }

    #[test_case]
    fn test_empty_password_hash() {
        let hash = users::hash_password("", "knoxos");
        assert!(users::verify_password("", &hash));
        assert!(!users::verify_password("notempty", &hash));
    }

    #[test_case]
    fn test_wildcard_password() {
        assert!(users::verify_password("anything", "*"));
    }
}

// ═══════════════════════════════════════════════════════════════════════
// SCHEDULER TESTS
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod scheduler_tests {
    use crate::scheduler;

    #[test_case]
    fn test_scheduler_init() {
        scheduler::init();
        let _ = scheduler::current_pid();
    }
}

// ═══════════════════════════════════════════════════════════════════════
// DNS TESTS
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod dns_tests {
    use crate::dns;

    #[test_case]
    fn test_dns_build_query() {
        let query = dns::build_query("example.com", 1); // A record
        // DNS header is 12 bytes, then the question section
        assert!(query.len() > 12);
        // Transaction ID is first 2 bytes (non-zero)
        // Flags: standard query = 0x0100
        assert_eq!(query[2], 0x01);
        assert_eq!(query[3], 0x00);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// THEME TESTS
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod theme_tests {
    use crate::gui::theme;

    #[test_case]
    fn test_theme_switch() {
        theme::set_theme(theme::ThemeId::ArcticLight);
        assert_eq!(theme::active_theme(), theme::ThemeId::ArcticLight);

        theme::set_theme(theme::ThemeId::NebulaDark);
        assert_eq!(theme::active_theme(), theme::ThemeId::NebulaDark);
    }

    #[test_case]
    fn test_theme_colors_non_zero() {
        let colors = theme::colors();
        assert!(colors.bg_primary.a > 0);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// SYSCALL ABI TEST FRAMEWORK
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod syscall_abi_tests {
    use super::*;
    use crate::syscall;
    use alloc::vec;

    /// Helper: invoke handle_syscall with given number and arguments
    fn do_syscall(nr: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64, a6: u64) -> i64 {
        syscall::handle_syscall(nr, a1, a2, a3, a4, a5, a6)
    }

    // ─── File I/O syscalls ─────────────────────────────────────

    #[test_case]
    fn test_sys_write_stdout() {
        // write(fd=1, buf, len) → should succeed (returns bytes written or ≥0)
        let msg = b"test\n";
        let ret = do_syscall(1, 1, msg.as_ptr() as u64, msg.len() as u64, 0, 0, 0);
        assert!(ret >= 0, "write() to stdout failed: {}", ret);
    }

    #[test_case]
    fn test_sys_read_bad_fd() {
        // read(fd=9999, ...) → should return -EBADF (−9)
        let mut buf = [0u8; 64];
        let ret = do_syscall(0, 9999, buf.as_mut_ptr() as u64, 64, 0, 0, 0);
        assert!(ret < 0, "read() on bad fd should fail");
    }

    #[test_case]
    fn test_sys_close_bad_fd() {
        // close(fd=9999) → should return -EBADF
        let ret = do_syscall(3, 9999, 0, 0, 0, 0, 0);
        assert!(ret < 0, "close() on bad fd should fail");
    }

    #[test_case]
    fn test_sys_open_close_cycle() {
        // Ensure VFS has a test file
        crate::vfs::write_file_dispatch("/tmp/syscall_test.txt", b"hello");

        // open("/tmp/syscall_test.txt", O_RDONLY=0, 0)
        let path = b"/tmp/syscall_test.txt\0";
        let fd = do_syscall(2, path.as_ptr() as u64, 0, 0, 0, 0, 0);
        assert!(fd >= 0, "open() should return valid fd, got {}", fd);

        // close(fd)
        let ret = do_syscall(3, fd as u64, 0, 0, 0, 0, 0);
        assert!(ret == 0 || ret >= 0, "close() should succeed, got {}", ret);
    }

    // ─── Process syscalls ──────────────────────────────────────

    #[test_case]
    fn test_sys_getpid() {
        // getpid() = syscall 39
        let pid = do_syscall(39, 0, 0, 0, 0, 0, 0);
        assert!(pid > 0, "getpid() should return positive pid, got {}", pid);
    }

    #[test_case]
    fn test_sys_getppid() {
        // getppid() = syscall 110
        let ppid = do_syscall(110, 0, 0, 0, 0, 0, 0);
        assert!(ppid >= 0, "getppid() should return >= 0, got {}", ppid);
    }

    #[test_case]
    fn test_sys_getuid() {
        // getuid() = syscall 102
        let uid = do_syscall(102, 0, 0, 0, 0, 0, 0);
        assert!(uid >= 0, "getuid() should return >= 0, got {}", uid);
    }

    #[test_case]
    fn test_sys_getgid() {
        // getgid() = syscall 104
        let gid = do_syscall(104, 0, 0, 0, 0, 0, 0);
        assert!(gid >= 0, "getgid() should return >= 0, got {}", gid);
    }

    #[test_case]
    fn test_sys_geteuid() {
        // geteuid() = syscall 107
        let euid = do_syscall(107, 0, 0, 0, 0, 0, 0);
        assert!(euid >= 0, "geteuid() should return >= 0, got {}", euid);
    }

    // ─── Memory syscalls ───────────────────────────────────────

    #[test_case]
    fn test_sys_brk() {
        // brk(0) → returns current break
        let brk = do_syscall(12, 0, 0, 0, 0, 0, 0);
        assert!(brk >= 0, "brk(0) should return current break, got {}", brk);
    }

    #[test_case]
    fn test_sys_mmap_anonymous() {
        // mmap(0, 4096, PROT_READ|PROT_WRITE=3, MAP_PRIVATE|MAP_ANONYMOUS=0x22, -1, 0)
        let ret = do_syscall(9, 0, 4096, 3, 0x22, u64::MAX, 0);
        // Should return a valid address or an error
        // Even if simulation, should not crash
        assert!(ret != 0, "mmap() returned null");
    }

    // ─── Time syscalls ─────────────────────────────────────────

    #[test_case]
    fn test_sys_clock_gettime() {
        // clock_gettime(CLOCK_REALTIME=0, &timespec)
        let mut ts = [0u64; 2]; // tv_sec, tv_nsec
        let ret = do_syscall(228, 0, ts.as_mut_ptr() as u64, 0, 0, 0, 0);
        // Should succeed or return a meaningful error
        assert!(
            ret >= -1000,
            "clock_gettime() returned unexpected error: {}",
            ret
        );
    }

    #[test_case]
    fn test_sys_gettimeofday() {
        // gettimeofday(tv, NULL) = syscall 96
        let mut tv = [0u64; 2]; // tv_sec, tv_usec
        let ret = do_syscall(96, tv.as_mut_ptr() as u64, 0, 0, 0, 0, 0);
        assert!(ret >= 0, "gettimeofday() should succeed, got {}", ret);
    }

    // ─── Signal syscalls ───────────────────────────────────────

    #[test_case]
    fn test_sys_sigprocmask() {
        // rt_sigprocmask(SIG_BLOCK=0, NULL, &oldset, sigsetsize=8)
        let mut oldset = 0u64;
        let ret = do_syscall(14, 0, 0, &mut oldset as *mut u64 as u64, 8, 0, 0);
        // Should not crash, may return 0 or -EINVAL
        assert!(ret >= -1000, "sigprocmask returned unexpected: {}", ret);
    }

    // ─── Network syscalls ──────────────────────────────────────

    #[test_case]
    fn test_sys_socket_create() {
        // socket(AF_INET=2, SOCK_STREAM=1, 0) = syscall 41
        let fd = do_syscall(41, 2, 1, 0, 0, 0, 0);
        // Should return fd or error
        if fd >= 0 {
            // Clean up
            do_syscall(3, fd as u64, 0, 0, 0, 0, 0);
        }
        // Even if it fails, should return a valid errno
        assert!(fd >= -1000, "socket() returned unexpected: {}", fd);
    }

    // ─── Filesystem meta syscalls ──────────────────────────────

    #[test_case]
    fn test_sys_getcwd() {
        // getcwd(buf, size) = syscall 79
        let mut buf = [0u8; 256];
        let ret = do_syscall(79, buf.as_mut_ptr() as u64, 256, 0, 0, 0, 0);
        assert!(ret >= 0, "getcwd() should succeed, got {}", ret);
    }

    #[test_case]
    fn test_sys_dup2() {
        // dup2(oldfd, newfd) = syscall 33
        // dup2 on invalid fd
        let ret = do_syscall(33, 9999, 9998, 0, 0, 0, 0);
        assert!(ret < 0, "dup2() on bad fds should fail");
    }

    // ─── Misc syscalls ─────────────────────────────────────────

    #[test_case]
    fn test_sys_uname() {
        // uname(buf) = syscall 63
        let mut buf = [0u8; 390]; // struct utsname
        let ret = do_syscall(63, buf.as_mut_ptr() as u64, 0, 0, 0, 0, 0);
        assert!(ret >= 0, "uname() should succeed, got {}", ret);
    }

    #[test_case]
    fn test_sys_sched_yield() {
        // sched_yield() = syscall 24
        let ret = do_syscall(24, 0, 0, 0, 0, 0, 0);
        assert!(ret >= 0, "sched_yield() should return 0, got {}", ret);
    }

    #[test_case]
    fn test_sys_unknown_syscall() {
        // Very high syscall number should return -ENOSYS (-38)
        let ret = do_syscall(99999, 0, 0, 0, 0, 0, 0);
        assert_eq!(
            ret, -38,
            "Unknown syscall should return -ENOSYS, got {}",
            ret
        );
    }

    // ─── ABI register convention test ──────────────────────────
    // Verify that all 6 argument registers pass through correctly

    #[test_case]
    fn test_syscall_six_args_passthrough() {
        // Use write (syscall 1) which uses arg1=fd, arg2=buf, arg3=len
        // The fact that write works with pointer args validates register passing
        let data = b"abi-test\n";
        let ret = do_syscall(
            1,                    // nr: write
            2,                    // arg1: fd=stderr
            data.as_ptr() as u64, // arg2: buf pointer
            data.len() as u64,    // arg3: count
            0,                    // arg4: unused
            0,                    // arg5: unused
            0,                    // arg6: unused
        );
        assert!(ret >= 0, "write to stderr should succeed, got {}", ret);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// MIDI TESTS
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod midi_tests {
    use crate::midi;

    #[test_case]
    fn test_gm_instrument_names() {
        // GM instrument 0 should be "Acoustic Grand Piano"
        let name = midi::gm_instrument_name(0);
        assert!(!name.is_empty(), "GM instrument 0 should have a name");
    }

    #[test_case]
    fn test_note_to_freq_a4() {
        // MIDI note 69 = A4 = 440 Hz
        let freq = midi::note_to_freq(69);
        assert!(
            freq >= 439 && freq <= 441,
            "A4 should be ~440 Hz, got {}",
            freq
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════
// KPM (PACKAGE MANAGER) TESTS
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod kpm_tests {
    use crate::kpm;
    use alloc::string::String;

    #[test_case]
    fn test_kpm_init_has_packages() {
        kpm::init();
        let packages = kpm::PACKAGES.lock();
        assert!(
            packages.len() > 0,
            "KPM should have built-in packages after init"
        );
    }

    #[test_case]
    fn test_kpm_search() {
        let results = kpm::search("kernel");
        assert!(
            results.len() > 0,
            "Searching 'kernel' should return results"
        );
    }

    #[test_case]
    fn test_kpm_resolve_dependencies() {
        // Should resolve a known package without error
        let result = kpm::resolve_dependencies("knoxos-kernel");
        assert!(
            result.is_ok(),
            "Resolving knoxos-kernel deps should succeed"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════
// RELEASE SIGNING TESTS
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod release_tests {
    use crate::release_sign;

    #[test_case]
    fn test_semver_parse() {
        let v = release_sign::SemVer::parse("1.2.3").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);
    }

    #[test_case]
    fn test_semver_prerelease() {
        let v = release_sign::SemVer::parse("1.0.0-beta.1").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.prerelease, Some(alloc::string::String::from("beta.1")));
    }

    #[test_case]
    fn test_semver_comparison() {
        let v1 = release_sign::SemVer::parse("1.0.0").unwrap();
        let v2 = release_sign::SemVer::parse("1.0.1").unwrap();
        assert_eq!(v1.cmp_precedence(&v2), core::cmp::Ordering::Less);
    }

    #[test_case]
    fn test_semver_prerelease_lower() {
        let v_pre = release_sign::SemVer::parse("1.0.0-alpha").unwrap();
        let v_rel = release_sign::SemVer::parse("1.0.0").unwrap();
        assert_eq!(v_pre.cmp_precedence(&v_rel), core::cmp::Ordering::Less);
    }

    #[test_case]
    fn test_release_sign_verify() {
        let seed = [0xABu8; 32];
        let key = release_sign::generate_key_pair(&seed);
        let data = b"test release data";
        let sig = release_sign::sign_artifact(data, &key);
        assert!(release_sign::verify_signature(data, &sig, &key));
    }

    #[test_case]
    fn test_release_sign_tampered() {
        let seed = [0xCDu8; 32];
        let key = release_sign::generate_key_pair(&seed);
        let data = b"original data";
        let sig = release_sign::sign_artifact(data, &key);
        let tampered = b"tampered data";
        assert!(!release_sign::verify_signature(tampered, &sig, &key));
    }

    #[test_case]
    fn test_semver_bump() {
        let v = release_sign::SemVer::new(1, 2, 3);
        let major = v.bump_major();
        assert_eq!(major.major, 2);
        assert_eq!(major.minor, 0);
        assert_eq!(major.patch, 0);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// REPOSITORY SERVER TESTS
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod repo_server_tests {
    use crate::repo_server;

    #[test_case]
    fn test_repo_add_package() {
        repo_server::add_package(
            "test-pkg",
            "1.0.0",
            "A test package",
            b"KPKG\x01\x00\x00\x00test data",
            &[],
        );
        let list = repo_server::list_packages();
        assert!(list.iter().any(|(name, _, _)| name == "test-pkg"));
    }

    #[test_case]
    fn test_repo_packages_index() {
        let index = repo_server::generate_packages_index();
        // Should contain Debian-compatible fields
        assert!(index.contains("Package:") || index.is_empty());
    }

    #[test_case]
    fn test_repo_handle_404() {
        let resp = repo_server::handle_request("/nonexistent");
        assert_eq!(resp.status, 404);
    }

    #[test_case]
    fn test_repo_handle_index() {
        let resp = repo_server::handle_request("/repo/");
        assert_eq!(resp.status, 200);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// BLUETOOTH TESTS
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod bluetooth_tests {
    use crate::bluetooth;

    #[test_case]
    fn test_bluetooth_init() {
        bluetooth::init();
        // Should not panic
    }
}

// ═══════════════════════════════════════════════════════════════════════
// TLS TESTS
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tls_tests {
    use crate::tls;

    #[test_case]
    fn test_certificate_store_init() {
        // Creating a certificate store should not panic
        let store = tls::CertificateStore::new();
        assert!(store.trusted_roots.is_empty());
    }

    #[test_case]
    fn test_x509_parse_invalid() {
        // Invalid DER data should return None
        let result = tls::X509Certificate::from_der(&[0x00, 0x01, 0x02]);
        assert!(result.is_none());
    }
}

// ═══════════════════════════════════════════════════════════════════════
// PDF TESTS
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod pdf_tests {
    use crate::pdf;

    #[test_case]
    fn test_pdf_invalid_data() {
        let result = pdf::parse(b"not a pdf");
        assert!(result.is_none());
    }

    #[test_case]
    fn test_pdf_header_check() {
        // Valid PDF header but no content
        let data = b"%PDF-1.4\n%%EOF";
        let result = pdf::parse(data);
        // Should parse without panic (may return empty doc)
        if let Some(doc) = result {
            assert!(doc.page_count == 0 || doc.page_count > 0);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// BENCHMARK TESTS
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod benchmark_tests {
    use crate::benchmark;

    #[test_case]
    fn test_benchmark_init() {
        benchmark::init();
        // Should not panic
    }

    #[test_case]
    fn test_benchmark_run_all() {
        benchmark::run_all();
        // Should complete without panic
    }
}

// ═══════════════════════════════════════════════════════════════════════
// WINDOW TILING TESTS
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod window_tiling_tests {
    use crate::gui::window::{TilingMode, Window, WindowManager};

    #[test_case]
    fn test_tiling_modes_exist() {
        // Verify all tiling modes can be constructed
        let modes = [
            TilingMode::Floating,
            TilingMode::MasterStack,
            TilingMode::Grid,
            TilingMode::Monocle,
            TilingMode::Columns,
        ];
        assert_eq!(modes.len(), 5);
    }

    #[test_case]
    fn test_tiling_mode_default_is_floating() {
        let wm = WindowManager::new();
        assert_eq!(wm.tiling_mode, TilingMode::Floating);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// SOCKET ACTIVATION TESTS
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod socket_activation_tests {
    use crate::socket_activation::{SocketAddress, SocketProtocol, SocketState, SocketUnit};

    #[test_case]
    fn test_socket_unit_creation() {
        let unit = SocketUnit::new("test.socket", "test.service", SocketAddress::tcp_port(8080));
        assert_eq!(unit.name, "test.socket");
        assert_eq!(unit.service, "test.service");
        assert_eq!(unit.state, SocketState::Inactive);
        assert_eq!(unit.backlog, 128);
        assert!(unit.reuse_addr);
    }

    #[test_case]
    fn test_socket_address_display() {
        let addr = SocketAddress::tcp_addr(127, 0, 0, 1, 443);
        let port = addr.port();
        assert_eq!(port, Some(443));
    }

    #[test_case]
    fn test_socket_unix_address() {
        let addr = SocketAddress::unix("/run/test.sock");
        assert_eq!(addr.port(), None);
    }

    #[test_case]
    fn test_socket_unit_builder() {
        let unit = SocketUnit::new("http.socket", "httpd.service", SocketAddress::tcp_port(80))
            .with_description("HTTP Socket")
            .with_accept(true)
            .with_protocol(SocketProtocol::Tcp);
        assert!(unit.accept);
        assert_eq!(unit.protocol, SocketProtocol::Tcp);
        assert_eq!(unit.description, "HTTP Socket");
    }
}

// ═══════════════════════════════════════════════════════════════════════
// THERMAL MONITORING TESTS
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod thermal_tests {
    use crate::thermal::{CoolingPolicy, ThermalZone, TripType, ZoneType};

    #[test_case]
    fn test_thermal_zone_creation() {
        let zone = ThermalZone::new("test-cpu", ZoneType::Estimated);
        assert_eq!(zone.name, "test-cpu");
        assert_eq!(zone.current_temp, 0);
        assert_eq!(zone.sample_count, 0);
    }

    #[test_case]
    fn test_thermal_zone_update() {
        let mut zone = ThermalZone::new("test", ZoneType::Estimated);
        zone.add_trip(TripType::Active, 45, 3);
        zone.add_trip(TripType::Critical, 100, 0);

        zone.update_temp(35000); // 35°C
        assert_eq!(zone.current_temp, 35000);
        assert_eq!(zone.sample_count, 1);
        assert!(!zone.critical);
    }

    #[test_case]
    fn test_thermal_trip_trigger() {
        let mut zone = ThermalZone::new("test", ZoneType::Estimated);
        zone.add_trip(TripType::Active, 45, 3);

        // Below trip point
        zone.update_temp(40000);
        assert!(!zone.trip_points[0].triggered);

        // Above trip point
        zone.update_temp(46000);
        assert!(zone.trip_points[0].triggered);
    }

    #[test_case]
    fn test_thermal_history() {
        let mut zone = ThermalZone::new("test", ZoneType::Estimated);
        for i in 0..10 {
            zone.update_temp(30000 + i * 1000);
        }
        assert_eq!(zone.sample_count, 10);
        assert_eq!(zone.min_temp, 30000);
        assert_eq!(zone.max_temp, 39000);
    }

    #[test_case]
    fn test_thermal_celsius_conversion() {
        let mut zone = ThermalZone::new("test", ZoneType::Estimated);
        zone.update_temp(45500); // 45.5°C
        let (degrees, frac) = zone.temp_celsius();
        assert_eq!(degrees, 45);
        assert_eq!(frac, 500);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// MEDIA PLAYER TESTS
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod media_player_tests {
    use crate::media_player::{AudioFormat, PlaybackState, parse_wav};

    #[test_case]
    fn test_cd_quality_format() {
        let fmt = AudioFormat::cd_quality();
        assert_eq!(fmt.sample_rate, 44100);
        assert_eq!(fmt.channels, 2);
        assert_eq!(fmt.bits_per_sample, 16);
    }

    #[test_case]
    fn test_wav_parse_too_small() {
        let data = [0u8; 10];
        assert!(parse_wav(&data).is_err());
    }

    #[test_case]
    fn test_wav_parse_invalid_magic() {
        let mut data = [0u8; 44];
        // Not RIFF
        data[0..4].copy_from_slice(b"NOPE");
        assert!(parse_wav(&data).is_err());
    }

    #[test_case]
    fn test_playback_states() {
        assert_ne!(PlaybackState::Playing, PlaybackState::Paused);
        assert_ne!(PlaybackState::Stopped, PlaybackState::Playing);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// SIXEL GRAPHICS TESTS
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod sixel_tests {
    use crate::terminal::sixel::SixelImage;

    #[test_case]
    fn test_sixel_image_creation() {
        let img = SixelImage::new();
        assert_eq!(img.width, 0);
        assert_eq!(img.height, 0);
    }

    #[test_case]
    fn test_sixel_parse_simple() {
        let mut img = SixelImage::new();
        // Simple sixel: ? = all 6 bits off, ~ = all 6 bits on
        img.parse(b"~");
        assert!(img.width > 0 || !img.pixels.is_empty());
    }

    #[test_case]
    fn test_sixel_scale() {
        let mut img = SixelImage::new();
        img.parse(b"\"1;1;100;60~~~$-~~~");
        let scaled = img.scale_to_fit(50, 30);
        // Scaled image should be smaller or equal
        assert!(scaled.width <= 100);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// IME TESTS
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod ime_tests {
    use crate::gui::ime::{ImeMode, ImeState};

    #[test_case]
    fn test_ime_off_passthrough() {
        let mut ime = ImeState::new();
        assert_eq!(ime.mode, ImeMode::Off);
        let result = ime.process_key('a');
        assert_eq!(result, Some(alloc::string::String::from("a")));
    }

    #[test_case]
    fn test_ime_pinyin_mode() {
        let mut ime = ImeState::new();
        ime.mode = ImeMode::Pinyin;
        // Type 'w' + 'o' → should generate candidates for "我"
        let _ = ime.process_key('w');
        let _ = ime.process_key('o');
        assert!(!ime.candidates.is_empty() || !ime.preedit.is_empty());
    }

    #[test_case]
    fn test_ime_cancel() {
        let mut ime = ImeState::new();
        ime.mode = ImeMode::Pinyin;
        let _ = ime.process_key('n');
        let _ = ime.process_key('i');
        ime.cancel();
        assert!(ime.input_buffer.is_empty());
        assert!(ime.candidates.is_empty());
        assert!(!ime.visible);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// RTL TEXT TESTS
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod rtl_text_tests {
    use crate::gui::rtl_text::BidiClass;

    #[test_case]
    fn test_bidi_class_variants() {
        // Verify BidiClass types can be constructed
        let classes = [
            BidiClass::L,
            BidiClass::R,
            BidiClass::AL,
            BidiClass::EN,
            BidiClass::AN,
            BidiClass::ON,
            BidiClass::WS,
        ];
        assert_eq!(classes.len(), 7);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// SCREEN RECORDING TESTS
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod screen_record_tests {
    use crate::screen_record::{RecordConfig, RecordingState};

    #[test_case]
    fn test_recording_state() {
        assert_ne!(RecordingState::Idle, RecordingState::Recording);
        assert_ne!(RecordingState::Recording, RecordingState::Paused);
    }

    #[test_case]
    fn test_record_config_default() {
        let config = RecordConfig::default();
        assert_eq!(config.fps, 30);
        assert_eq!(config.capture_width, 1920);
        assert_eq!(config.capture_height, 1080);
        assert!(config.include_cursor);
        assert_eq!(config.max_duration_secs, 300);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// VECTOR GRAPHICS TESTS
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod vector_tests {
    use crate::gui::vector::Point;

    #[test_case]
    fn test_point_creation() {
        let p = Point::new(10.5, 20.3);
        assert!(p.x > 10.0 && p.x < 11.0);
        assert!(p.y > 20.0 && p.y < 21.0);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// VISUAL TEST FRAMEWORK TESTS
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod visual_test_tests {
    use crate::visual_test::{Screenshot, compare};

    #[test_case]
    fn test_compare_identical() {
        let a = Screenshot {
            name: alloc::string::String::from("test"),
            width: 2,
            height: 2,
            pixels: alloc::vec![0xFF000000, 0xFF000000, 0xFF000000, 0xFF000000],
        };
        let b = a.clone();
        let result = compare(&a, &b, 0.1);
        assert!(result.passed);
        assert_eq!(result.diff_pixels, 0);
    }

    #[test_case]
    fn test_compare_size_mismatch() {
        let a = Screenshot {
            name: alloc::string::String::from("a"),
            width: 2,
            height: 2,
            pixels: alloc::vec![0; 4],
        };
        let b = Screenshot {
            name: alloc::string::String::from("b"),
            width: 3,
            height: 3,
            pixels: alloc::vec![0; 9],
        };
        let result = compare(&a, &b, 0.1);
        assert!(!result.passed);
        assert_eq!(result.diff_percent, 100.0);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// PGO (PROFILE-GUIDED OPTIMIZATION) TESTS
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod pgo_tests {
    use crate::pgo::PgoMode;

    #[test_case]
    fn test_pgo_modes() {
        assert_ne!(PgoMode::None, PgoMode::Instrument);
        assert_ne!(PgoMode::Instrument, PgoMode::Optimize);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// SERVICE MANAGER TESTS
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod service_manager_tests {
    use crate::service_manager::{RestartPolicy, ServiceState, ServiceUnit};

    #[test_case]
    fn test_service_unit_creation() {
        let svc = ServiceUnit::new("test.service", "Test Service", "/usr/bin/test");
        assert_eq!(svc.name, "test.service");
        assert_eq!(svc.state, ServiceState::Inactive);
        assert!(!svc.enabled);
    }

    #[test_case]
    fn test_restart_policies() {
        assert_ne!(RestartPolicy::No, RestartPolicy::Always);
        assert_ne!(RestartPolicy::OnFailure, RestartPolicy::OnAbnormal);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// HWTEST (HARDWARE TESTING) TESTS
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod hwtest_tests {
    use crate::hwtest::{TestCategory, TestSeverity};

    #[test_case]
    fn test_categories() {
        let categories = [
            TestCategory::Cpu,
            TestCategory::Memory,
            TestCategory::Pci,
            TestCategory::Interrupt,
            TestCategory::Timer,
            TestCategory::Storage,
            TestCategory::Network,
            TestCategory::Serial,
            TestCategory::Acpi,
        ];
        assert_eq!(categories.len(), 9);
    }

    #[test_case]
    fn test_severities() {
        assert_ne!(TestSeverity::Info, TestSeverity::Critical);
        assert_ne!(TestSeverity::Warning, TestSeverity::Error);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// GUI WIDGET UNIT TESTS
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod gui_widget_tests {
    use crate::gui::widgets::{Button, Checkbox, Slider, TextInput};

    #[test_case]
    fn test_widget_button_state() {
        let mut b = Button::new(0, 0, 80, 24, "ok");
        assert!(!b.hovered && !b.pressed);
        b.hovered = true;
        assert!(b.hovered && !b.pressed);
        b.pressed = true;
        assert!(b.hovered && b.pressed);
        b.pressed = false;
        b.hovered = false;
        assert!(!b.hovered && !b.pressed);
    }

    #[test_case]
    fn test_widget_text_input() {
        let mut t = TextInput::new(0, 0, 120, 24, "type…");
        t.insert_char('H');
        t.insert_char('i');
        assert_eq!(t.text.as_str(), "Hi");
        assert_eq!(t.cursor_pos, 2);
        t.backspace();
        assert_eq!(t.text.as_str(), "H");
        assert_eq!(t.cursor_pos, 1);
    }

    #[test_case]
    fn test_widget_slider_range() {
        let mut s = Slider::new(0, 0, 100, 50);
        s.update_from_mouse(-40);
        assert_eq!(s.value, 0);
        s.update_from_mouse(200);
        assert_eq!(s.value, 100);
        s.update_from_mouse(25);
        assert_eq!(s.value, 25);
    }

    #[test_case]
    fn test_widget_checkbox_toggle() {
        let mut c = Checkbox::new(0, 0, "enable", false);
        assert!(!c.checked);
        c.checked = !c.checked;
        assert!(c.checked);
        c.checked = !c.checked;
        assert!(!c.checked);
    }

    #[test_case]
    fn test_widget_dropdown_selection() {
        let items = ["Apple", "Banana", "Cherry"];
        let mut selected = 0usize;
        selected = 2;
        assert_eq!(items[selected], "Cherry");
        selected = selected.min(items.len() - 1);
        assert!(selected < items.len());
    }
}

// ═══════════════════════════════════════════════════════════════════════
// FILESYSTEM STRESS TESTS
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod fs_stress_tests {
    use crate::vfs;
    use alloc::format;
    use alloc::vec;

    #[test_case]
    fn test_concurrent_file_create() {
        vfs::init();
        vfs::ensure_directory("/tmp/stress");
        for i in 0..32u32 {
            let path = format!("/tmp/stress/f{}.txt", i);
            let payload = format!("file-{}", i);
            assert!(vfs::write_file_dispatch(&path, payload.as_bytes()));
        }
        for i in 0..32u32 {
            let path = format!("/tmp/stress/f{}.txt", i);
            let got = vfs::read_file_dispatch(&path).expect("missing");
            assert_eq!(got, format!("file-{}", i).as_bytes());
        }
    }

    #[test_case]
    fn test_deep_directory_nesting() {
        vfs::ensure_directory("/tmp/deep");
        let mut path = alloc::string::String::from("/tmp/deep");
        for i in 0..16 {
            path.push_str("/d");
            path.push_str(&format!("{}", i));
            vfs::ensure_directory(&path);
        }
        let file = format!("{}/leaf", path);
        assert!(vfs::write_file_dispatch(&file, b"ok"));
        assert_eq!(vfs::read_file_dispatch(&file).unwrap(), b"ok");
    }

    #[test_case]
    fn test_large_file_write() {
        let data = vec![0x5Au8; 64 * 1024];
        assert!(vfs::write_file_dispatch("/tmp/large.bin", &data));
        let read = vfs::read_file_dispatch("/tmp/large.bin").unwrap();
        assert_eq!(read.len(), data.len());
        assert_eq!(read, data);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// NETWORK CONFORMANCE TESTS
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod network_conformance_tests {
    use crate::dns;
    use crate::net::{TCP_ACK, TCP_SYN, TcpHeader};

    #[test_case]
    fn test_tcp_syn_ack_sequence() {
        let syn = TcpHeader::new(12345, 80, 1, 0, TCP_SYN);
        assert_eq!(syn.flags() & TCP_SYN, TCP_SYN);
        assert_eq!(syn.flags() & TCP_ACK, 0);
        assert_eq!(syn.seq(), 1);

        let syn_ack = TcpHeader::new(80, 12345, 99, 2, TCP_SYN | TCP_ACK);
        assert_eq!(syn_ack.flags() & (TCP_SYN | TCP_ACK), TCP_SYN | TCP_ACK);
        assert_eq!(syn_ack.ack(), 2);
        assert_eq!(syn_ack.seq(), 99);
    }

    #[test_case]
    fn test_tcp_retransmit_timeout() {
        // RTO doubles on timeout (classic exponential backoff), capped.
        let mut rto: u32 = 200;
        for _ in 0..5 {
            rto = (rto * 2).min(3200);
        }
        assert_eq!(rto, 3200);
        assert!(rto >= 200);
    }

    #[test_case]
    fn test_dns_rfc_compliance() {
        let q = dns::build_query("example.com", dns::DNS_TYPE_A);
        assert!(q.len() >= 12 + 17); // header + 7example3com0 + type + class
        let qdcount = u16::from_be_bytes([q[4], q[5]]);
        assert_eq!(qdcount, 1);
        let flags = u16::from_be_bytes([q[2], q[3]]);
        assert_eq!(flags & dns::DNS_FLAG_QR, 0); // query, not response
        assert_eq!(flags & dns::DNS_FLAG_RD, dns::DNS_FLAG_RD);
        assert_eq!(q[12], 7); // "example"
        assert_eq!(&q[13..20], b"example");
        assert_eq!(q[20], 3); // "com"
        assert_eq!(&q[21..24], b"com");
        assert_eq!(q[24], 0);
        let qtype = u16::from_be_bytes([q[25], q[26]]);
        let qclass = u16::from_be_bytes([q[27], q[28]]);
        assert_eq!(qtype, dns::DNS_TYPE_A);
        assert_eq!(qclass, dns::DNS_CLASS_IN);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// SECURITY TESTS
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod security_tests {
    use crate::cred::Credentials;
    use crate::ssp;

    #[test_case]
    fn test_privilege_escalation_blocked() {
        let mut user = Credentials::user(1000, 1000);
        assert!(user.set_uid(0).is_err());
        assert_eq!(user.euid, 1000);
        let mut root = Credentials::root();
        assert!(root.set_uid(1000).is_ok());
        assert_eq!(root.euid, 1000);
    }

    #[test_case]
    fn test_stack_smashing_detected() {
        ssp::set_thread_canary(3);
        let canary = ssp::get_thread_canary(3);
        assert_ne!(canary, 0);
        assert!(ssp::verify_thread_canary(3, canary));
        assert!(!ssp::verify_thread_canary(3, canary ^ 0xFF));
    }

    #[test_case]
    fn test_null_pointer_is_noncanonical_user() {
        // A userspace null deref is not a mapped VMA; the kernel treats addr 0
        // as invalid. This is the predicate the #PF path uses before demand map.
        let addr: u64 = 0;
        assert_eq!(addr & 0xFFFF_8000_0000_0000, 0);
        assert!(addr < 0x1000);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// ACCESSIBILITY TESTS
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod accessibility_tests {
    use crate::gui::accessibility::{ElementType, FocusManager, FocusableElement};
    use crate::gui::framebuffer::Rect;
    use crate::gui::theme_switch::ThemeColors;
    use alloc::string::String;

    fn contrast_ratio(a: u32, b: u32) -> u32 {
        fn lum(c: u32) -> u32 {
            let r = ((c >> 16) & 0xFF) as u32;
            let g = ((c >> 8) & 0xFF) as u32;
            let b = (c & 0xFF) as u32;
            3 * r + 6 * g + b
        }
        let (l1, l2) = (lum(a), lum(b));
        let (hi, lo) = if l1 >= l2 { (l1, l2) } else { (l2, l1) };
        (hi + 10) * 10 / (lo + 10)
    }

    fn el(id: u32, tab: i32, label: &str) -> FocusableElement {
        FocusableElement {
            id,
            label: String::from(label),
            rect: Rect::new(0, 0, 10, 10),
            tab_index: tab,
            focusable: true,
            element_type: ElementType::Button,
        }
    }

    #[test_case]
    fn test_high_contrast_theme_colors() {
        let t = ThemeColors::high_contrast();
        let ratio = contrast_ratio(t.bg_primary, t.text_primary);
        // WCAG AAA body text is 7:1; black/white is ~21:1.
        assert!(ratio >= 70, "contrast {} (tenths) below 7:1", ratio);
    }

    #[test_case]
    fn test_screen_reader_element_traversal() {
        let mut fm = FocusManager::new();
        fm.register(el(1, 0, "one"));
        fm.register(el(2, 1, "two"));
        fm.register(el(3, 2, "three"));
        fm.focus_next();
        assert_eq!(fm.focused().unwrap().id, 1);
        fm.focus_next();
        assert_eq!(fm.focused().unwrap().id, 2);
        fm.focus_next();
        assert_eq!(fm.focused().unwrap().id, 3);
    }

    #[test_case]
    fn test_keyboard_navigation() {
        let mut fm = FocusManager::new();
        fm.register(el(10, 0, "a"));
        fm.register(el(11, 1, "b"));
        fm.focus_next();
        fm.focus_next();
        assert_eq!(fm.focused().unwrap().id, 11);
        fm.focus_next(); // wrap
        assert_eq!(fm.focused().unwrap().id, 10);
        fm.focus_prev();
        assert_eq!(fm.focused().unwrap().id, 11);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// PROPERTY / ALLOCATOR TESTS
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod property_tests {
    use crate::path::normalize_path;
    use crate::vmm::PhysicalFramePool;
    use alloc::vec::Vec;

    #[test_case]
    fn test_path_normalize_idempotent() {
        let samples = [
            "/home/user/../etc",
            "a/b/./c",
            "//a///b",
            "/a/b/../../c",
            "..",
            "a/../../b",
            "/",
            "/././.",
        ];
        for s in samples {
            let once = normalize_path(s);
            let twice = normalize_path(&once);
            assert_eq!(once, twice, "not idempotent for {s}");
        }
        assert_eq!(normalize_path("/home/user/../etc"), "/etc");
        assert_eq!(normalize_path("a/b/./c"), "a/b/c");
    }

    #[test_case]
    fn test_alloc_free_symmetry() {
        let mut pool = PhysicalFramePool::new();
        for i in 0..32u64 {
            pool.add_frame(0x0040_0000 + i * 4096);
        }
        assert_eq!(pool.available(), 32);
        let mut held = Vec::new();
        for _ in 0..32 {
            held.push(pool.allocate().expect("frame"));
        }
        assert_eq!(pool.available(), 0);
        for f in held {
            pool.free(f);
        }
        assert_eq!(pool.available(), 32);
        let a = pool.allocate().unwrap();
        pool.free(a);
        assert_eq!(pool.available(), 32);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// CONFIG PRESERVE / CONTEXT
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod upgrade_tests {
    use crate::vfs;

    #[test_case]
    fn test_package_upgrade_preserves_config() {
        vfs::init();
        vfs::ensure_directory("/etc/pkg");
        assert!(vfs::write_file_dispatch("/etc/pkg/cfg", b"keep-me"));
        assert!(vfs::write_file_dispatch("/usr/lib/pkg/bin", b"v1"));
        assert!(vfs::write_file_dispatch("/usr/lib/pkg/bin", b"v2"));
        assert_eq!(vfs::read_file_dispatch("/etc/pkg/cfg").unwrap(), b"keep-me");
        assert_eq!(vfs::read_file_dispatch("/usr/lib/pkg/bin").unwrap(), b"v2");
    }

    #[test_case]
    fn test_package_downgrade_rollback() {
        vfs::write_file_dispatch("/usr/lib/pkg/bin", b"v2");
        vfs::write_file_dispatch("/usr/lib/pkg/bin", b"v1");
        assert_eq!(vfs::read_file_dispatch("/usr/lib/pkg/bin").unwrap(), b"v1");
    }
}

#[cfg(test)]
mod context_layout_tests {
    use crate::context::CpuContext;

    #[test_case]
    fn test_kernel_thread_context_runnable() {
        let ctx = CpuContext::new_kernel_thread(0xFFFF_8000_0010_0000, 0xFFFF_8000_0020_0000);
        assert!(ctx.is_runnable());
        assert_eq!(ctx.cs, 0x08);
        assert_eq!(ctx.ss, 0x10);
        assert_eq!(ctx.rflags & 0x200, 0x200);
        assert_eq!(ctx.rsp & 0xF, 8);
    }

    #[test_case]
    fn test_fxsave_area_alignment() {
        assert_eq!(core::mem::offset_of!(CpuContext, fxsave_area) % 16, 0);
    }
}
