// ═══════════════════════════════════════════════════════════════════════
// BIT READER UTILITY
// ═══════════════════════════════════════════════════════════════════════

pub(super) struct BitReader<'a> {
    data: &'a [u8],
    byte_offset: usize,
    bit_offset: u8,
}

impl<'a> BitReader<'a> {
    pub(super) fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            byte_offset: 0,
            bit_offset: 0,
        }
    }

    pub(super) fn read_bits(&mut self, n: u8) -> u32 {
        let mut result = 0u32;
        for _ in 0..n {
            if self.byte_offset >= self.data.len() {
                return result;
            }
            let bit = (self.data[self.byte_offset] >> (7 - self.bit_offset)) & 1;
            result = (result << 1) | (bit as u32);
            self.bit_offset += 1;
            if self.bit_offset >= 8 {
                self.bit_offset = 0;
                self.byte_offset += 1;
            }
        }
        result
    }

    pub(super) fn read_exp_golomb(&mut self) -> u32 {
        let mut leading_zeros = 0u32;
        while self.read_bits(1) == 0 {
            leading_zeros += 1;
            if leading_zeros > 31 {
                return 0;
            }
        }
        if leading_zeros == 0 {
            return 0;
        }
        let suffix = self.read_bits(leading_zeros as u8);
        (1 << leading_zeros) - 1 + suffix
    }

    pub(super) fn read_signed_exp_golomb(&mut self) -> i32 {
        let val = self.read_exp_golomb();
        if val == 0 {
            return 0;
        }
        let sign = if val & 1 == 0 { -1 } else { 1 };
        sign * val.div_ceil(2) as i32
    }
}

pub(super) fn read_leb128(data: &[u8]) -> (u64, usize) {
    let mut result = 0u64;
    let mut shift = 0u32;
    let mut i = 0;
    loop {
        if i >= data.len() {
            break;
        }
        let byte = data[i];
        result |= ((byte & 0x7F) as u64) << shift;
        i += 1;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
        if shift >= 56 {
            break;
        }
    }
    (result, i)
}
