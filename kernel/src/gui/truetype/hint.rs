//! Tiny TrueType instruction interpreter (SVTCA, PUSHB, MDAP).
use alloc::vec::Vec;

/// Round a 26.6 fixed-point coordinate to the pixel grid.
pub fn round_26_6(v: i32) -> i32 {
    if v >= 0 {
        (v + 32) & !63
    } else {
        -(((-v) + 32) & !63)
    }
}

/// Interpret a tiny TrueType instruction stream (SVTCA, PUSHB, MDAP).
pub fn hint_glyph(points: &mut [(i32, i32)], program: &[u8]) -> bool {
    let mut stack: Vec<i32> = Vec::new();
    let mut fv_x = true;
    let mut i = 0;
    while i < program.len() {
        let op = program[i];
        i += 1;
        match op {
            0x00 => fv_x = false, // SVTCA[y]
            0x01 => fv_x = true,  // SVTCA[x]
            0xB0..=0xB7 => {
                let n = (op - 0xB0 + 1) as usize;
                for _ in 0..n {
                    if i >= program.len() {
                        return false;
                    }
                    stack.push(program[i] as i32);
                    i += 1;
                }
            }
            0x2E | 0x2F => {
                let Some(p) = stack.pop() else {
                    return false;
                };
                let idx = p as usize;
                if idx >= points.len() {
                    return false;
                }
                if op == 0x2F {
                    if fv_x {
                        points[idx].0 = round_26_6(points[idx].0);
                    } else {
                        points[idx].1 = round_26_6(points[idx].1);
                    }
                }
            }
            _ => return false,
        }
    }
    true
}

pub const GATE_TTF_HINT_MARKER: &str = "GATE_TTF hint";

pub fn hint_self_test() -> bool {
    // Point 0 at 33.3125 px; MDAP[rnd] along x snaps to 33.0
    let mut pts = [(33 * 64 + 20, 10 * 64)];
    let program = [0x01u8, 0xB0, 0x00, 0x2F];
    let ran = hint_glyph(&mut pts, &program);
    let ok = ran && pts[0].0 == 33 * 64 && pts[0].1 == 10 * 64;
    if ok {
        crate::serial_println!("[fonts] {} (MDAP round-to-grid x=33)", GATE_TTF_HINT_MARKER);
    } else {
        crate::serial_println!(
            "[fonts] GATE_TTF FAILED ran={} x={} y={}",
            ran,
            pts[0].0,
            pts[0].1
        );
    }
    ok
}
