// Printf formatting engine (stdio.h)
use alloc::vec::Vec;

/// Internal printf formatting — processes a format string and variadic args.
/// This is a simplified but functional implementation supporting:
///   %d, %i, %u, %x, %X, %o, %s, %c, %p, %ld, %lu, %lx, %lld, %llu, %llx,
///   %f (simplified), %%, %zu, %zd, width, precision, padding, left-align.
pub struct PrintfFormatter {
    output: Vec<u8>,
    max_len: Option<usize>,
}

impl PrintfFormatter {
    pub fn new(max_len: Option<usize>) -> Self {
        Self {
            output: Vec::new(),
            max_len,
        }
    }

    pub fn push(&mut self, b: u8) {
        if let Some(max) = self.max_len {
            if self.output.len() >= max {
                return;
            }
        }
        self.output.push(b);
    }

    pub fn push_str(&mut self, s: &[u8]) {
        for &b in s {
            self.push(b);
        }
    }

    pub fn finish(self) -> Vec<u8> {
        self.output
    }

    pub fn len(&self) -> usize {
        self.output.len()
    }

    pub fn is_empty(&self) -> bool {
        self.output.is_empty()
    }

    /// Format an integer as decimal
    pub fn format_signed(&mut self, val: i64, width: usize, zero_pad: bool, left_align: bool) {
        let mut buf = [0u8; 24];
        let negative = val < 0;
        let mut v = if negative {
            (val as i128).unsigned_abs() as u64
        } else {
            val as u64
        };
        let mut pos = buf.len();
        if v == 0 {
            pos -= 1;
            buf[pos] = b'0';
        } else {
            while v > 0 {
                pos -= 1;
                buf[pos] = b'0' + (v % 10) as u8;
                v /= 10;
            }
        }
        if negative {
            pos -= 1;
            buf[pos] = b'-';
        }
        let digits = &buf[pos..];
        let dlen = digits.len();
        if !left_align && dlen < width {
            let pad = if zero_pad { b'0' } else { b' ' };
            for _ in 0..(width - dlen) {
                self.push(pad);
            }
        }
        self.push_str(digits);
        if left_align && dlen < width {
            for _ in 0..(width - dlen) {
                self.push(b' ');
            }
        }
    }

    /// Format an unsigned integer
    pub fn format_unsigned(&mut self, val: u64, width: usize, zero_pad: bool, left_align: bool) {
        let mut buf = [0u8; 22];
        let mut v = val;
        let mut pos = buf.len();
        if v == 0 {
            pos -= 1;
            buf[pos] = b'0';
        } else {
            while v > 0 {
                pos -= 1;
                buf[pos] = b'0' + (v % 10) as u8;
                v /= 10;
            }
        }
        let digits = &buf[pos..];
        let dlen = digits.len();
        if !left_align && dlen < width {
            let pad = if zero_pad { b'0' } else { b' ' };
            for _ in 0..(width - dlen) {
                self.push(pad);
            }
        }
        self.push_str(digits);
        if left_align && dlen < width {
            for _ in 0..(width - dlen) {
                self.push(b' ');
            }
        }
    }

    /// Format unsigned as hex
    pub fn format_hex(
        &mut self,
        val: u64,
        upper: bool,
        width: usize,
        zero_pad: bool,
        left_align: bool,
        prefix: bool,
    ) {
        let mut buf = [0u8; 20];
        let mut v = val;
        let mut pos = buf.len();
        let hex_chars = if upper {
            b"0123456789ABCDEF"
        } else {
            b"0123456789abcdef"
        };
        if v == 0 {
            pos -= 1;
            buf[pos] = b'0';
        } else {
            while v > 0 {
                pos -= 1;
                buf[pos] = hex_chars[(v & 0xF) as usize];
                v >>= 4;
            }
        }
        if prefix {
            pos -= 1;
            buf[pos] = if upper { b'X' } else { b'x' };
            pos -= 1;
            buf[pos] = b'0';
        }
        let digits = &buf[pos..];
        let dlen = digits.len();
        if !left_align && dlen < width {
            let pad = if zero_pad { b'0' } else { b' ' };
            for _ in 0..(width - dlen) {
                self.push(pad);
            }
        }
        self.push_str(digits);
        if left_align && dlen < width {
            for _ in 0..(width - dlen) {
                self.push(b' ');
            }
        }
    }

    /// Format unsigned as octal
    pub fn format_octal(&mut self, val: u64, width: usize, zero_pad: bool, left_align: bool) {
        let mut buf = [0u8; 24];
        let mut v = val;
        let mut pos = buf.len();
        if v == 0 {
            pos -= 1;
            buf[pos] = b'0';
        } else {
            while v > 0 {
                pos -= 1;
                buf[pos] = b'0' + (v & 7) as u8;
                v >>= 3;
            }
        }
        let digits = &buf[pos..];
        let dlen = digits.len();
        if !left_align && dlen < width {
            let pad = if zero_pad { b'0' } else { b' ' };
            for _ in 0..(width - dlen) {
                self.push(pad);
            }
        }
        self.push_str(digits);
        if left_align && dlen < width {
            for _ in 0..(width - dlen) {
                self.push(b' ');
            }
        }
    }
}

/// Process a printf format string with a raw va_list-style argument pointer.
///
/// This is the core engine used by printf, fprintf, snprintf, etc.
/// `args` is a pointer to the first variadic argument (treated as u64 slots
/// on x86_64 System V ABI).
///
/// Returns the formatted byte vector.
pub unsafe fn printf_engine(
    fmt: *const u8,
    mut args: *const u64,
    max_len: Option<usize>,
) -> Vec<u8> {
    let mut f = PrintfFormatter::new(max_len);
    let mut i = 0;

    loop {
        let c = *fmt.add(i);
        if c == 0 {
            break;
        }
        i += 1;

        if c != b'%' {
            f.push(c);
            continue;
        }

        // Parse format specifier
        let mut flags_left = false;
        let mut flags_zero = false;
        let mut flags_hash = false;
        let mut flags_plus = false;
        let mut flags_space = false;

        // Parse flags
        loop {
            let fc = *fmt.add(i);
            match fc {
                b'-' => {
                    flags_left = true;
                    i += 1;
                }
                b'0' => {
                    flags_zero = true;
                    i += 1;
                }
                b'#' => {
                    flags_hash = true;
                    i += 1;
                }
                b'+' => {
                    flags_plus = true;
                    i += 1;
                }
                b' ' => {
                    flags_space = true;
                    i += 1;
                }
                _ => break,
            }
        }

        // Parse width
        let mut width: usize = 0;
        if *fmt.add(i) == b'*' {
            width = *args as usize;
            args = args.add(1);
            i += 1;
        } else {
            while (*fmt.add(i)).is_ascii_digit() {
                width = width * 10 + (*fmt.add(i) - b'0') as usize;
                i += 1;
            }
        }

        // Parse precision
        let mut precision: Option<usize> = None;
        if *fmt.add(i) == b'.' {
            i += 1;
            let mut prec = 0usize;
            if *fmt.add(i) == b'*' {
                prec = *args as usize;
                args = args.add(1);
                i += 1;
            } else {
                while (*fmt.add(i)).is_ascii_digit() {
                    prec = prec * 10 + (*fmt.add(i) - b'0') as usize;
                    i += 1;
                }
            }
            precision = Some(prec);
        }

        // Parse length modifier
        let mut long_count = 0u8; // 1 = l, 2 = ll
        let mut size_t_mod = false;
        match *fmt.add(i) {
            b'l' => {
                long_count = 1;
                i += 1;
                if *fmt.add(i) == b'l' {
                    long_count = 2;
                    i += 1;
                }
            }
            b'h' => {
                i += 1;
                if *fmt.add(i) == b'h' {
                    i += 1;
                }
            }
            b'z' => {
                size_t_mod = true;
                i += 1;
            }
            b'j' | b't' => {
                i += 1;
            }
            _ => {}
        }

        // Parse conversion specifier
        let spec = *fmt.add(i);
        i += 1;

        match spec {
            b'd' | b'i' => {
                let val = *args as i64;
                args = args.add(1);
                f.format_signed(val, width, flags_zero && !flags_left, flags_left);
            }
            b'u' => {
                let val = *args;
                args = args.add(1);
                f.format_unsigned(val, width, flags_zero && !flags_left, flags_left);
            }
            b'x' => {
                let val = *args;
                args = args.add(1);
                f.format_hex(
                    val,
                    false,
                    width,
                    flags_zero && !flags_left,
                    flags_left,
                    flags_hash,
                );
            }
            b'X' => {
                let val = *args;
                args = args.add(1);
                f.format_hex(
                    val,
                    true,
                    width,
                    flags_zero && !flags_left,
                    flags_left,
                    flags_hash,
                );
            }
            b'o' => {
                let val = *args;
                args = args.add(1);
                f.format_octal(val, width, flags_zero && !flags_left, flags_left);
            }
            b's' => {
                let ptr = *args as *const u8;
                args = args.add(1);
                if ptr.is_null() {
                    let s = b"(null)";
                    let len = if let Some(p) = precision {
                        p.min(s.len())
                    } else {
                        s.len()
                    };
                    if !flags_left && len < width {
                        for _ in 0..(width - len) {
                            f.push(b' ');
                        }
                    }
                    f.push_str(&s[..len]);
                    if flags_left && len < width {
                        for _ in 0..(width - len) {
                            f.push(b' ');
                        }
                    }
                } else {
                    let mut slen = 0;
                    while *ptr.add(slen) != 0 {
                        slen += 1;
                        if slen > 65536 {
                            break;
                        }
                    }
                    let len = if let Some(p) = precision {
                        p.min(slen)
                    } else {
                        slen
                    };
                    if !flags_left && len < width {
                        for _ in 0..(width - len) {
                            f.push(b' ');
                        }
                    }
                    let slice = core::slice::from_raw_parts(ptr, len);
                    f.push_str(slice);
                    if flags_left && len < width {
                        for _ in 0..(width - len) {
                            f.push(b' ');
                        }
                    }
                }
            }
            b'c' => {
                let val = *args as u8;
                args = args.add(1);
                if !flags_left && width > 1 {
                    for _ in 0..(width - 1) {
                        f.push(b' ');
                    }
                }
                f.push(val);
                if flags_left && width > 1 {
                    for _ in 0..(width - 1) {
                        f.push(b' ');
                    }
                }
            }
            b'p' => {
                let val = *args;
                args = args.add(1);
                f.push_str(b"0x");
                f.format_hex(val, false, 0, false, false, false);
            }
            b'f' | b'F' | b'e' | b'E' | b'g' | b'G' => {
                // Simplified float: print integer part + 6 decimals
                let val = f64::from_bits(*args);
                args = args.add(1);
                let prec = precision.unwrap_or(6);
                let negative = val < 0.0;
                let abs_val = if negative { -val } else { val };
                let int_part = abs_val as u64;
                let frac_mult = 10u64.pow(prec as u32);
                let frac_part = ((abs_val - int_part as f64) * frac_mult as f64) as u64;
                if negative {
                    f.push(b'-');
                }
                f.format_unsigned(int_part, 0, false, false);
                if prec > 0 {
                    f.push(b'.');
                    f.format_unsigned(frac_part, prec, true, false);
                }
            }
            b'n' => {
                // Store number of characters written so far
                let ptr = *args as *mut i32;
                args = args.add(1);
                if !ptr.is_null() {
                    *ptr = f.len() as i32;
                }
            }
            b'%' => {
                f.push(b'%');
            }
            _ => {
                // Unknown specifier — output as-is
                f.push(b'%');
                f.push(spec);
            }
        }
    }

    f.finish()
}
