pub type Bits = u16;

pub struct BitReader<'a> {
    buf: &'a[u8],
    bits: u8,
    acc: u32,
}

fn reverse(a: Bits, n: u8) -> Bits {
    let mut v = a;
    if n == 1 {
        return v;
    }
    // swap odd and even bits
    v = ((v >> 1) & 0x5555) | ((v & 0x5555) << 1);
    if n == 2 {
        return v;
    }
    // swap consecutive pairs
    v = ((v >> 2) & 0x3333) | ((v & 0x3333) << 2);
    if n <= 4 {
        return v >> (4 - n);
    }
    // swap nibbles ...
    v = ((v >> 4) & 0x0F0F) | ((v & 0x0F0F) << 4);
    if n <= 8 {
        return v >> (8 - n);
    }
    // swap bytes
    v = ((v >> 8) & 0x00FF) | ((v & 0x00FF) << 8);
    return v >> (16 - n);
}

impl<'a> BitReader<'a> {
    pub fn new(buf: &'a [u8]) -> BitReader<'a> {
        BitReader { buf: buf, bits: 0, acc: 0 }
    }

    //order: true for LSB and false for MSB (Huffman codes)
    pub fn read_bits(&mut self, n: u8, order: bool) -> Option<Bits> {
        assert!(n <= 16);
        while self.bits < n {
            let byte = if self.buf.len() > 0 {
                let byte = self.buf[0];
                self.buf = &self.buf[1..];
                byte
            } else {
                return None
            };
            self.acc |= (byte as u32) << { self.bits };
            self.bits += 8;
        }
        let res = self.acc & ((1 << n) - 1);
        self.acc >>= n;
        self.bits -= n;
        if order {
            Some(res as u16)
        } else {
            Some(reverse(res as u16, n))
        }
    }
}
