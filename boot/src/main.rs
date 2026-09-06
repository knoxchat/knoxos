/// KnoxOS Boot Image Builder
/// Creates UEFI and BIOS bootable disk images from the kernel ELF binary
use std::path::PathBuf;

fn main() {
    let kernel_path = {
        let mut args = std::env::args().skip(1);
        let path = args.next().unwrap_or_else(|| {
            eprintln!("Usage: knoxos-boot <KERNEL_ELF_PATH>");
            eprintln!("  Creates UEFI and BIOS bootable disk images from the kernel ELF");
            std::process::exit(1);
        });
        PathBuf::from(path)
    };

    if !kernel_path.exists() {
        eprintln!(
            "Error: Kernel binary not found at: {}",
            kernel_path.display()
        );
        std::process::exit(1);
    }

    let kernel_dir = kernel_path.parent().unwrap();

    // Create UEFI disk image
    let uefi_path = kernel_dir.join("knoxos-uefi.img");
    println!("Creating UEFI disk image: {}", uefi_path.display());
    bootloader::UefiBoot::new(&kernel_path)
        .create_disk_image(&uefi_path)
        .expect("Failed to create UEFI disk image");
    println!("  ✓ UEFI image created successfully");

    // Create BIOS disk image
    let bios_path = kernel_dir.join("knoxos-bios.img");
    println!("Creating BIOS disk image: {}", bios_path.display());
    bootloader::BiosBoot::new(&kernel_path)
        .create_disk_image(&bios_path)
        .expect("Failed to create BIOS disk image");
    println!("  ✓ BIOS image created successfully");

    println!();
    println!("Boot images ready:");
    println!("  UEFI: {}", uefi_path.display());
    println!("  BIOS: {}", bios_path.display());
    println!();
    println!("Run with QEMU:");
    println!(
        "  UEFI: qemu-system-x86_64 -bios /usr/share/ovmf/OVMF.fd -drive format=raw,file={}",
        uefi_path.display()
    );
    println!(
        "  BIOS: qemu-system-x86_64 -drive format=raw,file={}",
        bios_path.display()
    );
}
