// SPDX-License-Identifier: MIT
//! Docker / VM Test Image Generation
//!
//! Creates pre-built test images for CI/CD:
//! 1. QEMU disk images with KnoxOS pre-installed
//! 2. Docker/OCI container images for testing
//! 3. Automated test harness images
//! 4. Cloud-init compatible VM images

extern crate alloc;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use lazy_static::lazy_static;
use spin::Mutex;

// ─── Image Types ────────────────────────────────────────────────────

/// Type of test image to generate
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestImageType {
    /// Raw disk image for QEMU (qcow2-compatible layout)
    QemuDisk,
    /// OCI container image (Docker-compatible)
    OciContainer,
    /// ISO with auto-test boot
    TestIso,
    /// Cloud-init enabled VM image
    CloudInit,
    /// Minimal kernel-only image for unit tests
    KernelOnly,
}

/// Image configuration
#[derive(Debug, Clone)]
pub struct TestImageConfig {
    pub image_type: TestImageType,
    pub name: String,
    pub version: String,
    pub size_mb: u64,
    pub include_desktop: bool,
    pub include_network: bool,
    pub include_test_suite: bool,
    pub auto_shutdown: bool,
    pub serial_console: bool,
    pub memory_mb: u32,
    pub cpus: u32,
}

impl TestImageConfig {
    pub fn default_qemu() -> Self {
        Self {
            image_type: TestImageType::QemuDisk,
            name: String::from("knoxos-test"),
            version: String::from("0.2.1"),
            size_mb: 2048,
            include_desktop: true,
            include_network: true,
            include_test_suite: true,
            auto_shutdown: true,
            serial_console: true,
            memory_mb: 2048,
            cpus: 2,
        }
    }

    pub fn default_oci() -> Self {
        Self {
            image_type: TestImageType::OciContainer,
            name: String::from("knoxos/test"),
            version: String::from("0.2.1"),
            size_mb: 512,
            include_desktop: false,
            include_network: true,
            include_test_suite: true,
            auto_shutdown: true,
            serial_console: true,
            memory_mb: 512,
            cpus: 1,
        }
    }

    pub fn kernel_only() -> Self {
        Self {
            image_type: TestImageType::KernelOnly,
            name: String::from("knoxos-kernel-test"),
            version: String::from("0.2.1"),
            size_mb: 64,
            include_desktop: false,
            include_network: false,
            include_test_suite: true,
            auto_shutdown: true,
            serial_console: true,
            memory_mb: 256,
            cpus: 1,
        }
    }
}

// ─── QCOW2 Image Format ────────────────────────────────────────────

/// QCOW2 header (version 3)
#[repr(C)]
#[derive(Debug, Clone)]
pub struct Qcow2Header {
    pub magic: u32,   // 0x514649FB ('QFI\xFB')
    pub version: u32, // 3
    pub backing_file_offset: u64,
    pub backing_file_size: u32,
    pub cluster_bits: u32, // 16 = 64KB clusters
    pub size: u64,         // Virtual disk size
    pub crypt_method: u32,
    pub l1_size: u32,
    pub l1_table_offset: u64,
    pub refcount_table_offset: u64,
    pub refcount_table_clusters: u32,
    pub nb_snapshots: u32,
    pub snapshots_offset: u64,
    // v3 additional fields
    pub incompatible_features: u64,
    pub compatible_features: u64,
    pub autoclear_features: u64,
    pub refcount_order: u32, // 4 = 16-bit refcounts
    pub header_length: u32,
}

/// Build a QCOW2 image header
pub fn build_qcow2_header(virtual_size: u64) -> Vec<u8> {
    let cluster_bits: u32 = 16; // 64KB clusters
    let cluster_size: u64 = 1u64 << cluster_bits;
    let l2_entries = cluster_size / 8;
    let l1_entries = virtual_size.div_ceil(l2_entries * cluster_size);

    let mut header = vec![0u8; cluster_size as usize]; // Header occupies first cluster

    // Magic
    header[0..4].copy_from_slice(&0x514649FBu32.to_be_bytes());
    // Version 3
    header[4..8].copy_from_slice(&3u32.to_be_bytes());
    // Cluster bits
    header[20..24].copy_from_slice(&cluster_bits.to_be_bytes());
    // Virtual size
    header[24..32].copy_from_slice(&virtual_size.to_be_bytes());
    // L1 size
    header[36..40].copy_from_slice(&(l1_entries as u32).to_be_bytes());
    // L1 table offset (after header, at cluster 1)
    header[40..48].copy_from_slice(&cluster_size.to_be_bytes());
    // Refcount table offset (at cluster 2)
    header[48..56].copy_from_slice(&(cluster_size * 2).to_be_bytes());
    // Refcount table clusters
    header[56..60].copy_from_slice(&1u32.to_be_bytes());
    // Refcount order (4 = 16-bit)
    header[92..96].copy_from_slice(&4u32.to_be_bytes());
    // Header length
    header[100..104].copy_from_slice(&104u32.to_be_bytes());

    header
}

// ─── OCI Container Image ────────────────────────────────────────────

/// OCI Image manifest
#[derive(Debug, Clone)]
pub struct OciManifest {
    pub schema_version: u32,
    pub media_type: String,
    pub config_digest: String,
    pub layers: Vec<OciLayer>,
}

#[derive(Debug, Clone)]
pub struct OciLayer {
    pub media_type: String,
    pub digest: String,
    pub size: u64,
}

/// Build OCI container image manifest
pub fn build_oci_manifest(config: &TestImageConfig) -> String {
    format!(
        r#"{{
  "schemaVersion": 2,
  "mediaType": "application/vnd.oci.image.manifest.v1+json",
  "config": {{
    "mediaType": "application/vnd.oci.image.config.v1+json",
    "digest": "sha256:{}",
    "size": 1024
  }},
  "layers": [
    {{
      "mediaType": "application/vnd.oci.image.layer.v1.tar+gzip",
      "digest": "sha256:{}",
      "size": {}
    }}
  ]
}}"#,
        "a".repeat(64), // Config digest placeholder
        "b".repeat(64), // Layer digest placeholder
        config.size_mb * 1024 * 1024,
    )
}

/// Build OCI image config
pub fn build_oci_config(config: &TestImageConfig) -> String {
    format!(
        r#"{{
  "architecture": "amd64",
  "os": "knoxos",
  "config": {{
    "Env": [
      "PATH=/usr/bin:/bin",
      "TERM=xterm-256color"
    ],
    "Cmd": ["/bin/sh"],
    "Labels": {{
      "org.opencontainers.image.title": "{}",
      "org.opencontainers.image.version": "{}",
      "org.opencontainers.image.description": "KnoxOS test image"
    }}
  }},
  "rootfs": {{
    "type": "layers",
    "diff_ids": ["sha256:{}"]
  }}
}}"#,
        config.name,
        config.version,
        "c".repeat(64),
    )
}

// ─── Dockerfile Generation ──────────────────────────────────────────

/// Generate a Dockerfile for building a KnoxOS test container
pub fn generate_dockerfile(config: &TestImageConfig) -> String {
    let mut dockerfile = String::from("# KnoxOS Test Container\n");
    dockerfile.push_str("# Auto-generated by knoxos test_images module\n\n");
    dockerfile.push_str("FROM scratch\n\n");
    dockerfile.push_str("LABEL maintainer=\"knoxos@knoxos.dev\"\n");
    dockerfile.push_str(&format!("LABEL version=\"{}\"\n\n", config.version));

    // Copy kernel image
    dockerfile.push_str("COPY knoxos-bios.img /boot/knoxos-bios.img\n");
    dockerfile.push_str("COPY knoxos-uefi.img /boot/knoxos-uefi.img\n\n");

    // Add QEMU wrapper script
    dockerfile.push_str("COPY run_test.sh /run_test.sh\n");
    dockerfile.push_str("RUN chmod +x /run_test.sh\n\n");

    if config.include_test_suite {
        dockerfile.push_str("COPY tests/ /tests/\n");
    }

    dockerfile.push_str("ENTRYPOINT [\"/run_test.sh\"]\n");
    dockerfile
}

/// Generate the QEMU test runner script
pub fn generate_test_runner_script(config: &TestImageConfig) -> String {
    let mut script = String::from("#!/bin/sh\n");
    script.push_str("# KnoxOS QEMU Test Runner\n\n");
    script.push_str("set -e\n\n");

    script.push_str(&format!("MEMORY={}M\n", config.memory_mb));
    script.push_str(&format!("CPUS={}\n", config.cpus));
    script.push_str("TIMEOUT=120\n\n");

    script.push_str("exec timeout $TIMEOUT qemu-system-x86_64 \\\n");
    script.push_str("  -machine q35 \\\n");
    script.push_str("  -m $MEMORY \\\n");
    script.push_str("  -smp $CPUS \\\n");
    script.push_str("  -drive format=raw,file=/boot/knoxos-bios.img \\\n");
    script.push_str("  -device isa-debug-exit,iobase=0xf4,iosize=0x04 \\\n");
    script.push_str("  -serial stdio \\\n");
    script.push_str("  -display none \\\n");
    script.push_str("  -no-reboot \\\n");

    if config.include_network {
        script.push_str("  -netdev user,id=net0 \\\n");
        script.push_str("  -device virtio-net-pci,netdev=net0 \\\n");
    }

    script.push_str("  -nographic\n\n");

    script.push_str("EXIT_CODE=$?\n");
    script.push_str("if [ $EXIT_CODE -eq 33 ]; then\n");
    script.push_str("  echo \"TESTS PASSED\"\n");
    script.push_str("  exit 0\n");
    script.push_str("else\n");
    script.push_str("  echo \"TESTS FAILED (exit code: $EXIT_CODE)\"\n");
    script.push_str("  exit 1\n");
    script.push_str("fi\n");

    script
}

// ─── Cloud-Init Image ───────────────────────────────────────────────

/// Generate cloud-init user-data for VM provisioning
pub fn generate_cloud_init_userdata(hostname: &str) -> String {
    format!(
        r#"#cloud-config
hostname: {}
manage_etc_hosts: true
users:
  - name: knoxos
    groups: sudo
    shell: /bin/sh
    sudo: ALL=(ALL) NOPASSWD:ALL
    ssh_authorized_keys: []
packages:
  - qemu-guest-agent
runcmd:
  - systemctl enable qemu-guest-agent
  - systemctl start qemu-guest-agent
final_message: "KnoxOS VM ready after $UPTIME seconds"
"#,
        hostname
    )
}

/// Generate cloud-init meta-data
pub fn generate_cloud_init_metadata(instance_id: &str) -> String {
    format!(
        r#"instance-id: {}
local-hostname: knoxos-test
"#,
        instance_id
    )
}

// ─── GitHub Actions CI Integration ──────────────────────────────────

/// Generate a GitHub Actions workflow for testing with our images
pub fn generate_ci_workflow() -> String {
    String::from(
        r#"name: KnoxOS Integration Tests
on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  build-and-test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust nightly
        uses: dtolnay/rust-toolchain@nightly
        with:
          components: rust-src, llvm-tools-preview

      - name: Install QEMU
        run: sudo apt-get install -y qemu-system-x86

      - name: Build kernel
        run: make build-kernel

      - name: Build boot images
        run: make build-boot

      - name: Run unit tests
        run: make test

      - name: Run integration tests
        run: |
          timeout 120 ./tests/run_integration.sh || true
          if grep -q "ALL_TESTS_PASSED" /tmp/knoxos-test.log; then
            echo "Integration tests passed!"
          else
            echo "Integration tests failed!"
            exit 1
          fi

      - name: Build Docker test image
        run: |
          docker build -t knoxos/test:${{ github.sha }} -f test.Dockerfile .

      - name: Upload artifacts
        uses: actions/upload-artifact@v4
        with:
          name: knoxos-images
          path: |
            boot/target/release/knoxos-bios.img
            boot/target/release/knoxos-uefi.img
"#,
    )
}

/// Generate a test Dockerfile for CI
pub fn generate_test_dockerfile() -> String {
    String::from(
        r#"FROM ubuntu:22.04
RUN apt-get update && apt-get install -y qemu-system-x86 && rm -rf /var/lib/apt/lists/*
COPY boot/target/release/knoxos-bios.img /boot/knoxos-bios.img
COPY boot/target/release/knoxos-uefi.img /boot/knoxos-uefi.img
COPY tests/ /tests/
COPY run_test.sh /run_test.sh
RUN chmod +x /run_test.sh
ENTRYPOINT ["/run_test.sh"]
"#,
    )
}

// ─── Image Builder ──────────────────────────────────────────────────

/// Build result
#[derive(Debug)]
pub struct BuildResult {
    pub success: bool,
    pub image_path: String,
    pub image_size: u64,
    pub build_time_ms: u64,
    pub artifacts: Vec<String>,
}

/// Build a test image
pub fn build_test_image(config: &TestImageConfig) -> BuildResult {
    crate::serial_println!(
        "[test_images] Building {:?} image: {}",
        config.image_type,
        config.name
    );

    let mut artifacts = Vec::new();

    match config.image_type {
        TestImageType::QemuDisk => {
            let header = build_qcow2_header(config.size_mb * 1024 * 1024);
            let path = format!("/var/lib/knoxos/{}.qcow2", config.name);
            crate::vfs::create_file_dispatch(&path, &header);
            artifacts.push(path.clone());
            crate::serial_println!("[test_images] QCOW2 image: {}", path);

            // Also generate QEMU launch script
            let script = generate_test_runner_script(config);
            let script_path = format!("/var/lib/knoxos/run_{}.sh", config.name);
            crate::vfs::create_file_dispatch(&script_path, script.as_bytes());
            artifacts.push(script_path);
        }
        TestImageType::OciContainer => {
            let manifest = build_oci_manifest(config);
            let oci_config = build_oci_config(config);
            let dockerfile = generate_dockerfile(config);

            let manifest_path = format!("/var/lib/knoxos/oci/{}/manifest.json", config.name);
            let config_path = format!("/var/lib/knoxos/oci/{}/config.json", config.name);
            let dockerfile_path = format!("/var/lib/knoxos/oci/{}/Dockerfile", config.name);

            crate::vfs::ensure_directory(&format!("/var/lib/knoxos/oci/{}", config.name));
            crate::vfs::create_file_dispatch(&manifest_path, manifest.as_bytes());
            crate::vfs::create_file_dispatch(&config_path, oci_config.as_bytes());
            crate::vfs::create_file_dispatch(&dockerfile_path, dockerfile.as_bytes());

            artifacts.push(manifest_path);
            artifacts.push(config_path);
            artifacts.push(dockerfile_path);

            crate::serial_println!("[test_images] OCI image: {}", config.name);
        }
        TestImageType::CloudInit => {
            let userdata = generate_cloud_init_userdata("knoxos-test");
            let metadata = generate_cloud_init_metadata("knoxos-test-001");

            let ud_path = String::from("/var/lib/knoxos/cloud-init/user-data");
            let md_path = String::from("/var/lib/knoxos/cloud-init/meta-data");

            crate::vfs::ensure_directory("/var/lib/knoxos/cloud-init");
            crate::vfs::create_file_dispatch(&ud_path, userdata.as_bytes());
            crate::vfs::create_file_dispatch(&md_path, metadata.as_bytes());

            artifacts.push(ud_path);
            artifacts.push(md_path);
        }
        TestImageType::KernelOnly | TestImageType::TestIso => {
            let path = format!("/var/lib/knoxos/{}.img", config.name);
            crate::vfs::create_file_dispatch(&path, b"[kernel test image]");
            artifacts.push(path);
        }
    }

    // Generate CI workflow
    let ci_workflow = generate_ci_workflow();
    let ci_path = String::from("/var/lib/knoxos/.github/workflows/test.yml");
    crate::vfs::ensure_directory("/var/lib/knoxos/.github/workflows");
    crate::vfs::create_file_dispatch(&ci_path, ci_workflow.as_bytes());
    artifacts.push(ci_path);

    // Generate test Dockerfile
    let test_df = generate_test_dockerfile();
    let df_path = String::from("/var/lib/knoxos/test.Dockerfile");
    crate::vfs::create_file_dispatch(&df_path, test_df.as_bytes());
    artifacts.push(df_path);

    BuildResult {
        success: true,
        image_path: format!("/var/lib/knoxos/{}", config.name),
        image_size: config.size_mb * 1024 * 1024,
        build_time_ms: 0,
        artifacts,
    }
}

// ─── Init ───────────────────────────────────────────────────────────

static INITIALIZED: AtomicBool = AtomicBool::new(false);

pub fn init() {
    if INITIALIZED.swap(true, Ordering::SeqCst) {
        return;
    }

    crate::vfs::ensure_directory("/var/lib/knoxos");
    crate::vfs::ensure_directory("/var/lib/knoxos/oci");

    // Build default test images
    let qemu_result = build_test_image(&TestImageConfig::default_qemu());
    let oci_result = build_test_image(&TestImageConfig::default_oci());

    crate::serial_println!("[test_images] Test image subsystem initialized");
    crate::serial_println!(
        "[test_images] QEMU image: {} ({} artifacts)",
        qemu_result.image_path,
        qemu_result.artifacts.len()
    );
    crate::serial_println!(
        "[test_images] OCI image: {} ({} artifacts)",
        oci_result.image_path,
        oci_result.artifacts.len()
    );
}
