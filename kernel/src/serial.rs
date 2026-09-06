/// Serial port output for debugging (visible in QEMU console)
use lazy_static::lazy_static;
use spin::Mutex;

/// Minimal MMIO UART writer for non-x86 platforms.
#[cfg(not(target_arch = "x86_64"))]
pub struct MmioSerialPort {
    base: usize,
}

#[cfg(not(target_arch = "x86_64"))]
impl MmioSerialPort {
    pub const unsafe fn new(base: usize) -> Self {
        Self { base }
    }
    pub fn init(&mut self) {
        // Minimal 8250/16550 init via MMIO
        unsafe {
            let base = self.base as *mut u8;
            base.add(1).write_volatile(0x00); // Disable interrupts
            base.add(3).write_volatile(0x80); // DLAB on
            base.add(0).write_volatile(0x01); // Divisor low (115200 baud)
            base.add(1).write_volatile(0x00); // Divisor high
            base.add(3).write_volatile(0x03); // 8N1, DLAB off
            base.add(2).write_volatile(0xC7); // Enable FIFO
        }
    }
    fn write_byte(&mut self, byte: u8) {
        unsafe {
            let base = self.base as *mut u8;
            // Wait for transmit holding register empty
            while base.add(5).read_volatile() & 0x20 == 0 {}
            base.add(0).write_volatile(byte);
        }
    }
}

#[cfg(not(target_arch = "x86_64"))]
impl core::fmt::Write for MmioSerialPort {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for byte in s.bytes() {
            self.write_byte(byte);
        }
        Ok(())
    }
}

/// COM1 wrapper around uart_16550 0.8 (SerialPort was removed in 0.5).
#[cfg(target_arch = "x86_64")]
pub struct SerialPort {
    inner: uart_16550::Uart16550<uart_16550::backend::PioBackend>,
}

#[cfg(target_arch = "x86_64")]
impl SerialPort {
    pub unsafe fn new(port: u16) -> Self {
        let mut inner = uart_16550::Uart16550::new_port(port).expect("COM1 port must be valid");
        let _ = inner.init(uart_16550::Config::default());
        Self { inner }
    }
}

#[cfg(target_arch = "x86_64")]
impl core::fmt::Write for SerialPort {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        self.inner.send_bytes_exact(s.as_bytes());
        Ok(())
    }
}

#[cfg(target_arch = "x86_64")]
lazy_static! {
    pub static ref SERIAL1: Mutex<SerialPort> = Mutex::new(unsafe { SerialPort::new(0x3F8) });
}

#[cfg(target_arch = "aarch64")]
lazy_static! {
    pub static ref SERIAL1: Mutex<MmioSerialPort> = {
        // QEMU virt machine PL011 UART base
        let mut serial_port = unsafe { MmioSerialPort::new(0x0900_0000) };
        serial_port.init();
        Mutex::new(serial_port)
    };
}

#[cfg(target_arch = "riscv64")]
lazy_static! {
    pub static ref SERIAL1: Mutex<MmioSerialPort> = {
        // QEMU virt machine UART base
        let mut serial_port = unsafe { MmioSerialPort::new(0x1000_0000) };
        serial_port.init();
        Mutex::new(serial_port)
    };
}

#[doc(hidden)]
pub fn _print(args: ::core::fmt::Arguments) {
    use core::fmt::Write;

    #[cfg(target_arch = "x86_64")]
    {
        #[cfg(not(target_arch = "x86_64"))]
        use crate::arch_compat::instructions::interrupts;
        #[cfg(target_arch = "x86_64")]
        use x86_64::instructions::interrupts;
        interrupts::without_interrupts(|| {
            SERIAL1
                .lock()
                .write_fmt(args)
                .expect("Printing to serial failed");
        });
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        SERIAL1
            .lock()
            .write_fmt(args)
            .expect("Printing to serial failed");
    }
}

/// Print to the host through the serial interface.
#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => {
        $crate::serial::_print(format_args!($($arg)*))
    };
}

/// Print to the host through the serial interface, with newline.
#[macro_export]
macro_rules! serial_println {
    () => ($crate::serial_print!("\n"));
    ($fmt:expr) => ($crate::serial_print!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => ($crate::serial_print!(
        concat!($fmt, "\n"), $($arg)*));
}
