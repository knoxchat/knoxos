/// PS/2 mouse controller initialization (x86_64 port I/O)

/// Initialize PS/2 mouse (x86_64 only — uses port I/O)
pub fn init_mouse() {
    #[cfg(target_arch = "x86_64")]
    {
        #[cfg(target_arch = "x86_64")]
        use crate::arch_compat::instructions::port::Port;
        #[cfg(not(target_arch = "x86_64"))]
        use crate::arch_compat::instructions::port::Port;

        // Disable interrupts during init so ACK bytes don't go into the queue
        crate::arch_compat::instructions::interrupts::without_interrupts(|| {
            unsafe {
                let mut cmd_port = Port::<u8>::new(0x64);
                let mut data_port = Port::<u8>::new(0x60);

                // Enable auxiliary device
                wait_write();
                cmd_port.write(0xA8);

                // Enable interrupts on PS/2 controller
                wait_write();
                cmd_port.write(0x20);
                wait_read();
                let status = data_port.read() | 0x02;
                wait_write();
                cmd_port.write(0x60);
                wait_write();
                data_port.write(status);

                // Use default settings
                write_mouse(0xF6);
                read_mouse(); // consume ACK

                // ── Enable IntelliMouse protocol (4-byte packets with scroll wheel) ──
                // Magic sequence: set sample rate 200, 100, 80, then request device ID
                write_mouse(0xF3);
                read_mouse(); // set sample rate command, ACK
                write_mouse(200);
                read_mouse(); // rate = 200, ACK
                write_mouse(0xF3);
                read_mouse(); // set sample rate command, ACK
                write_mouse(100);
                read_mouse(); // rate = 100, ACK
                write_mouse(0xF3);
                read_mouse(); // set sample rate command, ACK
                write_mouse(80);
                read_mouse(); // rate = 80, ACK

                // Read device ID — should be 3 if IntelliMouse mode activated (was 0)
                write_mouse(0xF2); // Get Device ID
                read_mouse(); // ACK
                let device_id = read_mouse();
                let has_scroll = device_id == 3 || device_id == 4;

                if has_scroll {
                    super::mouse::MOUSE.lock().intellimouse = true;
                    crate::serial_println!(
                        "[KnoxOS] PS/2 Mouse: IntelliMouse mode (scroll wheel enabled, ID={})",
                        device_id
                    );
                } else {
                    crate::serial_println!(
                        "[KnoxOS] PS/2 Mouse: Standard mode (no scroll wheel, ID={})",
                        device_id
                    );
                }

                // Enable mouse data reporting
                write_mouse(0xF4);
                read_mouse(); // consume ACK

                // Flush any remaining bytes from the data port
                for _ in 0..16 {
                    let mut status_port = Port::<u8>::new(0x64);
                    if status_port.read() & 0x01 != 0 {
                        let _ = data_port.read();
                    } else {
                        break;
                    }
                }
            }
        });

        // Drain any bytes that leaked into our queue during init
        if let Ok(queue) = super::mouse::MOUSE_QUEUE.try_get() {
            while queue.pop().is_some() {}
        }

        crate::serial_println!("[KnoxOS] PS/2 Mouse initialized");
    }

    #[cfg(not(target_arch = "x86_64"))]
    {
        // On aarch64/riscv64, mouse input comes from VirtIO input or device tree
        crate::serial_println!("[KnoxOS] PS/2 not available on this arch — using VirtIO/DT input");
    }
}

#[cfg(target_arch = "x86_64")]
unsafe fn wait_write() {
    let mut port = crate::arch_compat::instructions::port::Port::<u8>::new(0x64);
    for _ in 0..10000 {
        if port.read() & 0x02 == 0 {
            return;
        }
    }
}

#[cfg(target_arch = "x86_64")]
unsafe fn wait_read() {
    let mut port = crate::arch_compat::instructions::port::Port::<u8>::new(0x64);
    for _ in 0..10000 {
        if port.read() & 0x01 != 0 {
            return;
        }
    }
}

#[cfg(target_arch = "x86_64")]
unsafe fn write_mouse(byte: u8) {
    let mut cmd_port = crate::arch_compat::instructions::port::Port::<u8>::new(0x64);
    let mut data_port = crate::arch_compat::instructions::port::Port::<u8>::new(0x60);
    wait_write();
    cmd_port.write(0xD4);
    wait_write();
    data_port.write(byte);
}

#[cfg(target_arch = "x86_64")]
unsafe fn read_mouse() -> u8 {
    let mut data_port = crate::arch_compat::instructions::port::Port::<u8>::new(0x60);
    wait_read();
    data_port.read()
}
