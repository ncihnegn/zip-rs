use std::io::{self, Read, Write};

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

#[allow(dead_code)]
pub struct BitWriter<'a, W: Write + 'a> {
    buf: &'a mut W,
    bits: u8,
    acc: u32,
}

#[allow(dead_code)]
impl<'a, W: Write> BitWriter<'a, W> {
    pub fn new(buf: &'a mut W) -> BitWriter<W> {
        BitWriter { buf: buf, bits: 0, acc: 0 }
    }

    //order: true for LSB and false for MSB (Huffman codes)
    pub fn write_bits(&mut self, b: Bits, n: u8, order: bool) -> Result<u8, io::Error> {
        assert!(n <= 16);
        assert!(b <= 1 << n);
        let c = if order { b } else { reverse(b, n) };
        self.acc |= (c as u32) << self. bits;
        self.bits += n;
        let nb = self.bits / 8;
        if nb > 0 {
            let mut bytes = Vec::<u8>::new();
            bytes.reserve(nb as usize);
            for _ in 0..nb {
                bytes.push((self.acc & 0xFF) as u8);
                self.acc >>= 8;
            }
            let nc = try!(self.buf.write(&bytes as &[u8]));
            assert!(nc == nb as usize);
            self.bits -= nb * 8;
        }
        Ok(nb)
    }

    pub fn flush(&mut self) -> Result<u8, io::Error> {
        if self.bits > 0 {
            assert!(self.bits < 8);
            let bytes: [u8; 1] = [self.acc as u8; 1];
            try!(self.buf.write(&bytes));
            try!(self.buf.flush());
            self.bits = 0;
            self.acc = 0;
            Ok(1)
        } else {
            Ok(0)
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::fs::{self, File};
    use std::io::{BufReader, BufWriter};

    #[test]
    fn basics() {
        let file_name = "test/buf_read_write_test";
        {
            let file = File::create(file_name).unwrap();
            let mut output = BufWriter::new(file);
            let mut writer = BitWriter::new(&mut output);
            let mut counter = writer.write_bits(0x5A5A, 15, false).unwrap();
            assert!(counter == 1);
            counter += writer.write_bits(0x3AA5, 15, true).unwrap();
            assert!(counter == 3);
            counter += writer.flush().unwrap();
            assert!(counter == 4);
        }
        {
            let file = File::open(file_name).unwrap();
            let mut input = BufReader::new(file);
            let mut reader = BitReader::new(&mut input);
            let first = reader.read_bits(15, false).unwrap();
            assert!(first == 0x5A5A);
            assert!(reader.read_bits(15, true).unwrap() == 0x3AA5);
        }
        let _ = fs::remove_file(file_name);
    }
}
