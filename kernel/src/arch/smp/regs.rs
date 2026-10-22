// ─── APIC Constants ─────────────────────────────────────────────────────

/// Default Local APIC base address (memory-mapped)
pub const LAPIC_BASE: u64 = 0xFEE0_0000;

/// Local APIC register offsets
pub const LAPIC_ID: u32 = 0x020; // APIC ID
pub const LAPIC_VERSION: u32 = 0x030; // APIC Version
pub const LAPIC_TPR: u32 = 0x080; // Task Priority Register
pub const LAPIC_EOI: u32 = 0x0B0; // End of Interrupt
pub const LAPIC_SVR: u32 = 0x0F0; // Spurious Interrupt Vector
pub const LAPIC_ESR: u32 = 0x280; // Error Status Register
pub const LAPIC_ICR_LO: u32 = 0x300; // Interrupt Command Register (low)
pub const LAPIC_ICR_HI: u32 = 0x310; // Interrupt Command Register (high)
pub const LAPIC_TIMER: u32 = 0x320; // Timer LVT
pub const LAPIC_LINT0: u32 = 0x350; // Local Interrupt 0
pub const LAPIC_LINT1: u32 = 0x360; // Local Interrupt 1
pub const LAPIC_ERROR_LVT: u32 = 0x370; // Error LVT
pub const LAPIC_TIMER_INIT: u32 = 0x380; // Timer Initial Count
pub const LAPIC_TIMER_CURRENT: u32 = 0x390; // Timer Current Count
pub const LAPIC_TIMER_DIVIDE: u32 = 0x3E0; // Timer Divide Configuration

/// APIC SVR bit: APIC enabled
pub const LAPIC_SVR_ENABLE: u32 = 0x100;
/// Spurious vector number
pub const SPURIOUS_VECTOR: u32 = 0xFF;

/// APIC Timer modes
pub const TIMER_PERIODIC: u32 = 0x20000; // Periodic mode
pub const TIMER_ONE_SHOT: u32 = 0x00000; // One-shot mode
pub const TIMER_MASKED: u32 = 0x10000; // Masked (disabled)

/// Timer divide values
pub const TIMER_DIVIDE_1: u32 = 0xB;
pub const TIMER_DIVIDE_2: u32 = 0x0;
pub const TIMER_DIVIDE_4: u32 = 0x1;
pub const TIMER_DIVIDE_8: u32 = 0x2;
pub const TIMER_DIVIDE_16: u32 = 0x3;
pub const TIMER_DIVIDE_32: u32 = 0x8;
pub const TIMER_DIVIDE_64: u32 = 0x9;
pub const TIMER_DIVIDE_128: u32 = 0xA;

/// Timer interrupt vector
pub const TIMER_VECTOR: u32 = 0x20; // Same as PIT for compatibility

/// IPI delivery modes
pub const IPI_INIT: u32 = 0x500; // INIT IPI
pub const IPI_STARTUP: u32 = 0x600; // Startup IPI (SIPI)
pub const IPI_FIXED: u32 = 0x000; // Fixed delivery
pub const IPI_ALL_EXCL: u32 = 0xC0000; // All excluding self

/// I/O APIC base address
pub const IOAPIC_BASE: u64 = 0xFEC0_0000;

/// I/O APIC registers
pub const IOAPIC_REG_SELECT: u32 = 0x00;
pub const IOAPIC_REG_DATA: u32 = 0x10;
pub const IOAPIC_REG_ID: u32 = 0x00;
pub const IOAPIC_REG_VERSION: u32 = 0x01;
pub const IOAPIC_RED_TABLE_BASE: u32 = 0x10; // Redirection table starts here

/// MSR addresses
pub const MSR_APIC_BASE: u32 = 0x1B;
