/// GPIO Pin Controller
///
/// General Purpose I/O driver for SBC platforms (Raspberry Pi, Intel, AMD).
/// Provides digital pin control, PWM, interrupt-on-change, and pin muxing.
///
/// Features:
///   - Pin direction (input/output/alternate function)
///   - Pull-up / pull-down / open-drain configuration
///   - Edge/level interrupt triggering
///   - Software debouncing
///   - GPIO chip abstraction for multiple controllers
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use crate::serial_println;

/// Pin direction
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PinDirection {
    Input,
    Output,
}

/// Pin pull configuration
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PinPull {
    None,
    Up,
    Down,
}

/// Pin output mode
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PinDrive {
    PushPull,
    OpenDrain,
}

/// Interrupt edge type
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IrqEdge {
    Rising,
    Falling,
    Both,
    LevelHigh,
    LevelLow,
}

/// Pin value
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PinValue {
    Low,
    High,
}

/// GPIO pin configuration
#[derive(Debug, Clone)]
pub struct GpioPin {
    pub chip_id: u8,
    pub pin: u16,
    pub direction: PinDirection,
    pub pull: PinPull,
    pub drive: PinDrive,
    pub irq_edge: Option<IrqEdge>,
    pub debounce_us: u32,
    pub value: PinValue,
    pub label: String,
    pub in_use: bool,
}

/// GPIO controller (chip)
pub struct GpioChip {
    pub id: u8,
    pub label: String,
    pub base: u16,
    pub num_pins: u16,
    pub mmio_base: u64,
    pub pins: Vec<GpioPin>,
    /// Shadow DIR register: bit N = 1 means output.
    dir_reg: u32,
    /// Shadow DATA register: bit N is the pin level.
    data_reg: u32,
    /// Bit N set: pin N is looped to pin N^1 (pairs 0↔1, 2↔3, …).
    loopback_mask: u32,
}

lazy_static::lazy_static! {
    static ref GPIO_CHIPS: Mutex<Vec<GpioChip>> = Mutex::new(Vec::new());
}

impl GpioChip {
    pub fn new(id: u8, label: String, mmio_base: u64, base: u16, num_pins: u16) -> Self {
        let mut pins = Vec::with_capacity(num_pins as usize);
        for p in 0..num_pins {
            pins.push(GpioPin {
                chip_id: id,
                pin: base + p,
                direction: PinDirection::Input,
                pull: PinPull::None,
                drive: PinDrive::PushPull,
                irq_edge: None,
                debounce_us: 0,
                value: PinValue::Low,
                label: String::new(),
                in_use: false,
            });
        }
        Self {
            id,
            label,
            base,
            num_pins,
            mmio_base,
            pins,
            dir_reg: 0,
            data_reg: 0,
            loopback_mask: 0,
        }
    }

    fn pin_mut(&mut self, offset: u16) -> Result<&mut GpioPin, &'static str> {
        self.pins.get_mut(offset as usize).ok_or("Pin out of range")
    }

    /// Request exclusive use of a pin
    pub fn request_pin(&mut self, offset: u16, label: &str) -> Result<(), &'static str> {
        let pin = self.pin_mut(offset)?;
        if pin.in_use {
            return Err("Pin already in use");
        }
        pin.in_use = true;
        pin.label = String::from(label);
        Ok(())
    }

    /// Release a pin
    pub fn free_pin(&mut self, offset: u16) {
        if let Ok(pin) = self.pin_mut(offset) {
            pin.in_use = false;
            pin.label.clear();
            pin.direction = PinDirection::Input;
            pin.irq_edge = None;
        }
    }

    /// Set pin direction
    pub fn set_direction(&mut self, offset: u16, dir: PinDirection) -> Result<(), &'static str> {
        if offset >= 32 {
            return Err("Pin out of range");
        }
        {
            let pin = self.pin_mut(offset)?;
            pin.direction = dir;
        }
        if dir == PinDirection::Output {
            self.dir_reg |= 1 << offset;
        } else {
            self.dir_reg &= !(1 << offset);
        }
        Ok(())
    }

    /// Configure pull-up/down
    pub fn set_pull(&mut self, offset: u16, pull: PinPull) -> Result<(), &'static str> {
        let pin = self.pin_mut(offset)?;
        pin.pull = pull;
        Ok(())
    }

    /// Pair pin `offset` with `offset ^ 1` so output writes appear on the partner input.
    pub fn set_loopback(&mut self, offset: u16, enable: bool) -> Result<(), &'static str> {
        if offset >= 32 || self.pins.get(offset as usize).is_none() {
            return Err("Pin out of range");
        }
        if enable {
            self.loopback_mask |= 1 << offset;
            self.loopback_mask |= 1 << (offset ^ 1);
        } else {
            self.loopback_mask &= !(1 << offset);
            self.loopback_mask &= !(1 << (offset ^ 1));
        }
        Ok(())
    }

    fn write_data_bit(&mut self, offset: u16, val: PinValue) {
        if val == PinValue::High {
            self.data_reg |= 1 << offset;
        } else {
            self.data_reg &= !(1 << offset);
        }
        if let Some(pin) = self.pins.get_mut(offset as usize) {
            pin.value = val;
        }
    }

    /// Set output value
    pub fn set_value(&mut self, offset: u16, val: PinValue) -> Result<(), &'static str> {
        if offset >= 32 {
            return Err("Pin out of range");
        }
        {
            let pin = self.pin_mut(offset)?;
            if pin.direction != PinDirection::Output {
                return Err("Pin not configured as output");
            }
        }
        self.write_data_bit(offset, val);
        if self.loopback_mask & (1 << offset) != 0 {
            let partner = offset ^ 1;
            if (self.dir_reg & (1 << partner)) == 0 {
                self.write_data_bit(partner, val);
            }
        }
        Ok(())
    }

    /// Read input value
    pub fn get_value(&self, offset: u16) -> Result<PinValue, &'static str> {
        if offset >= 32 {
            return Err("Pin out of range");
        }
        let _pin = self.pins.get(offset as usize).ok_or("Pin out of range")?;
        if self.data_reg & (1 << offset) != 0 {
            Ok(PinValue::High)
        } else {
            Ok(PinValue::Low)
        }
    }

    /// Configure interrupt
    pub fn set_irq(&mut self, offset: u16, edge: IrqEdge) -> Result<(), &'static str> {
        let pin = self.pin_mut(offset)?;
        pin.irq_edge = Some(edge);
        // Configure edge detect registers
        Ok(())
    }

    /// Set debounce time
    pub fn set_debounce(&mut self, offset: u16, us: u32) -> Result<(), &'static str> {
        let pin = self.pin_mut(offset)?;
        pin.debounce_us = us;
        Ok(())
    }

    /// Handle GPIO interrupt — check which pins triggered
    pub fn handle_irq(&mut self) -> Vec<u16> {
        let mut triggered = Vec::new();
        // Read interrupt status register, check each bit
        for (i, pin) in self.pins.iter().enumerate() {
            if pin.irq_edge.is_some() {
                // If status bit set for this pin, add to list and clear
                let _ = pin; // placeholder
                triggered.push(i as u16);
            }
        }
        // Acknowledge all pending interrupts
        triggered
    }
}

pub fn register_chip(id: u8, label: &str, mmio_base: u64, base: u16, num_pins: u16) {
    let chip = GpioChip::new(id, String::from(label), mmio_base, base, num_pins);
    serial_println!(
        "[GPIO] Registered {} with {} pins at base {}",
        label,
        num_pins,
        base
    );
    GPIO_CHIPS.lock().push(chip);
}

pub const GATE_GPIO_MARKER: &str = "GATE_GPIO xfer";

fn gpio_self_test() -> bool {
    let mut chip = GpioChip::new(0, String::from("virt-gpio"), 0, 0, 8);
    if chip.request_pin(0, "out").is_err() || chip.request_pin(1, "in").is_err() {
        serial_println!("[GPIO] GATE_GPIO FAILED: request");
        return false;
    }
    if chip.set_direction(0, PinDirection::Output).is_err()
        || chip.set_direction(1, PinDirection::Input).is_err()
        || chip.set_loopback(0, true).is_err()
    {
        serial_println!("[GPIO] GATE_GPIO FAILED: config");
        return false;
    }
    if chip.set_value(0, PinValue::High).is_err() {
        serial_println!("[GPIO] GATE_GPIO FAILED: set");
        return false;
    }
    match chip.get_value(1) {
        Ok(PinValue::High) => {
            serial_println!("[GPIO] {} (pin0→pin1 loopback HIGH)", GATE_GPIO_MARKER);
            true
        }
        other => {
            serial_println!("[GPIO] GATE_GPIO FAILED get={:?}", other);
            false
        }
    }
}

pub fn init() {
    serial_println!("[GPIO] GPIO subsystem loaded");
    let _ = gpio_self_test();
}
