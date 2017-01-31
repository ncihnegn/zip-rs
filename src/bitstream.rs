use std::io::Read;

pub type Bits = u16;

pub struct BitReader<'a, R: Read + 'a> {
    buf: &'a mut R,
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

impl<'a, R: Read> BitReader<'a, R> {
    pub fn new(buf: &'a mut R) -> BitReader<R> {
        BitReader { buf: buf, bits: 0, acc: 0 }
    }

    //order: true for LSB and false for MSB (Huffman codes)
    pub fn read_bits(&mut self, n: u8, order: bool) -> Option<Bits> {
        assert!(n <= 16);
        let mut bytes: [u8; 1] = [0; 1];
        while self.bits < n {
            let _ = self.buf.read_exact(&mut bytes);
            let byte = bytes[0];
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
