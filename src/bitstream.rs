use std::io::{self, Read};

pub type Bits = u16;

pub struct BitReader<'a, R: Read + 'a> {
    buf: &'a mut R,
    bits: u8,
    acc: u32,
}

pub fn reverse(a: Bits, n: u8) -> Bits {
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
    v >> (16 - n)
}

impl<'a, R: Read> BitReader<'a, R> {
    pub fn new(buf: &'a mut R) -> BitReader<R> {
        BitReader { buf: buf, bits: 0, acc: 0 }
    }

    //order: true for LSB and false for MSB (Huffman codes)
    pub fn read_bits(&mut self, n: u8, order: bool) -> Result<Bits, io::Error> {
        assert!(n <= 16);
        let mut bytes: [u8; 1] = [0; 1];
        while self.bits < n {
            try!(self.buf.read_exact(&mut bytes));
            let byte = bytes[0];
            self.acc |= (byte as u32) << self.bits;
            self.bits += 8;
        }
        let res = self.acc & ((1 << n) - 1);
        self.acc >>= n;
        self.bits -= n;
        if order {
            Ok(res as Bits)
        } else {
            Ok(reverse(res as Bits, n))
        }
    }
}

#[derive(Default)]
pub struct BitWriter {
    bits: u8,
    acc: u32
}

impl BitWriter {
    pub fn new() -> BitWriter {
        BitWriter { bits: 0, acc: 0 }
    }

    pub fn write_bits(&mut self, b: Bits, n: u8) -> Vec<u8> {
        assert!(n <= 16);
        assert!(b <= 1 << n);
        let c = b;//if order { b } else { reverse(b, n) };
        self.acc |= (c as u32) << self. bits;
        self.bits += n;
        let nb = self.bits / 8;
        let mut bytes = Vec::<u8>::with_capacity(nb as usize);
        if nb > 0 {
            bytes.reserve(nb as usize);
            for _ in 0..nb {
                bytes.push((self.acc & 0xFF) as u8);
                self.acc >>= 8;
            }
            //let nc = try!(self.buf.write(&bytes as &[u8]));
            //assert!(nc == nb as usize);
            self.bits -= nb * 8;
        }
        bytes
    }

    pub fn flush(&mut self) -> Option<u8> {
        if self.bits > 0 {
            assert!(self.bits < 8);
            //let bytes: [u8; 1] = [self.acc as u8; 1];
            //try!(self.buf.write(&bytes));
            //try!(self.buf.flush());
            self.bits = 0;
            let byte = self.acc as u8;
            self.acc = 0;
            Some(byte)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::io::{BufReader, Cursor};

    #[test]
    fn basics() {
        let mut writer = BitWriter::new();
        let mut vec = writer.write_bits(0x5A5A, 15);
        assert!(vec.len() == 1);
        vec.extend(writer.write_bits(0x3AA5, 15).iter());
        assert!(vec.len() == 3);
        writer.flush().map(|c| { vec.push(c); });
        assert!(vec.len() == 4);
        let mut input = BufReader::new(Cursor::new(vec));
        let mut reader = BitReader::new(&mut input);
        let first = reader.read_bits(15, false).unwrap();
        assert_eq!(first, 0x2D2D);
        let second = reader.read_bits(15, true).unwrap();
        assert_eq!(second, 0x3AA5);
    }
}
