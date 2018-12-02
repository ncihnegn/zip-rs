use std::cmp::{Ordering, PartialOrd};
use std::collections::BinaryHeap;
use std::io::{Error, ErrorKind, Read};
use std::u16;

use crate::bitstream::*;
use crate::constant::*;

lazy_static! {
    pub static ref FIXED_LITERAL_DEC: HuffmanDec = HuffmanDec::fixed_literal_dec();
    pub static ref FIXED_LITERAL_ENC: Vec<(Bits, u8)> = HuffmanEnc::fixed_literal_enc();
}

#[derive(Debug, Eq, PartialEq)]
struct Char {
    val: u16,
    freq: usize,
    left: Option<Box<Char>>,
    right: Option<Box<Char>>,
}

impl Ord for Char {
    fn cmp(&self, other: &Char) -> Ordering {
        // Note that we flip the ordering here
        other.freq.cmp(&self.freq)
    }
}

impl PartialOrd for Char {
    fn partial_cmp(&self, other: &Char) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Default)]
pub struct HuffmanDec {
    count: Vec<u16>,
    symbol: Vec<u16>,
}

impl HuffmanDec {
    pub fn new() -> HuffmanDec {
        HuffmanDec {
            count: Vec::new(),
            symbol: Vec::new(),
        }
    }

    pub fn fixed_literal_dec() -> HuffmanDec {
        let count = vec![
            0 as u16,
            0,
            0,
            0,
            0,
            0,
            0,
            280 - 256,
            144 + 288 - 280,
            256 - 144,
        ];
        let mut symbol: Vec<u16> = (256..280).collect();
        let mut len8 = (0..144).collect();
        symbol.append(&mut len8);
        let mut len8a = (280..288).collect();
        symbol.append(&mut len8a);
        let mut len9 = (144..256 as u16).collect();
        symbol.append(&mut len9);
        HuffmanDec { count, symbol }
    }
}

pub struct HuffmanEnc {}

impl HuffmanEnc {
    pub fn fixed_literal_enc() -> Vec<(Bits, u8)> {
        let mut lit_lens = vec![8 as u8; MAX_NUM_LIT];
        for l in lit_lens.iter_mut().take(256).skip(144) {
            *l = 9;
        }
        for l in lit_lens.iter_mut().take(280).skip(256) {
            *l = 7;
        }
        gen_huffman_enc(&lit_lens)
    }
}

/// Assign lengths based on frequencies
pub fn assign_lengths(v: &[usize]) -> Vec<u8> {
    if v.is_empty() {
        return Vec::<u8>::new();
    }
    const NONLEAF: u16 = u16::MAX;
    let mut heap = BinaryHeap::new();
    // Build a min-heap
    for (c, f) in v.iter().enumerate() {
        if *f > 0 {
            heap.push(Char {
                val: c as u16,
                freq: *f,
                left: None,
                right: None,
            });
        }
    }
    while heap.len() > 1 {
        let l = heap.pop().unwrap();
        let r = heap.pop().unwrap();
        heap.push(Char {
            val: NONLEAF,
            freq: l.freq + r.freq,
            left: Some(Box::new(l)),
            right: Some(Box::new(r)),
        });
    }
    let root = heap.pop().unwrap();
    let mut todo = Vec::new();
    todo.push(root);
    let mut level: u8 = 0;
    let mut lengths = vec![0 as u8; v.len()];
    info!("{:?}", todo);
    while !todo.is_empty() {
        let mut next = Vec::new();
        for c in todo {
            if let Some(l) = c.left {
                next.push(*l);
            }
            if let Some(r) = c.right {
                next.push(*r);
            }
            if c.val != NONLEAF {
                info!("val: {}, level: {}", c.val, level);
                lengths[c.val as usize] = if level > 0 { level } else { 1 };
            }
        }
        todo = next;
        level += 1;
    }
    lengths
}

/// Generate a canonical Huffman encoding table with lengths
pub fn gen_huffman_enc(v: &[u8]) -> Vec<(Bits, u8)> {
    let max_bits = *v.iter().max().unwrap() as usize;
    let mut bl_count = vec![0 as Bits; max_bits + 1];
    for i in v {
        bl_count[*i as usize] += 1;
    }
    let mut next_code = vec![0 as Bits; max_bits + 1];
    let mut code: Bits = 0;
    bl_count[0] = 0;
    for (bits, bl) in bl_count.iter().enumerate().take(max_bits) {
        code = (code + bl) << 1;
        next_code[bits + 1] = code;
    }
    let max_code = v.len() - 1;
    let mut enc = vec![(0 as Bits, 0 as u8); max_code + 1];
    for (n, l) in v.iter().enumerate().take(max_code + 1) {
        let len = *l as usize;
        if len != 0 {
            enc[n] = (reverse(next_code[len], *l), *l);
            next_code[len] += 1;
        }
    }
    enc
}

pub fn gen_huffman_dec(lengths: &[u8], n: u16) -> HuffmanDec {
    let max_bits = *lengths.iter().max().unwrap() as usize;
    assert!(max_bits <= MAX_NUM_BITS);
    let mut count = vec![0 as u16; max_bits + 1];
    for i in lengths {
        if *i != 0 {
            count[*i as usize] += 1;
        }
    }
    let mut offsets = vec![0 as u16; max_bits + 1];
    for (i, c) in count.iter().enumerate().take(max_bits).skip(1) {
        offsets[i + 1] = offsets[i] + *c;
    }
    //let n = offsets[max_bits+1];//total number of symbols
    let mut symbol = vec![0 as u16; n as usize];
    for (sym, l) in lengths.iter().enumerate().take(n as usize) {
        let len = *l as usize;
        if len > 0 {
            symbol[offsets[len] as usize] = sym as u16;
            offsets[len] += 1;
        }
    }
    HuffmanDec { count, symbol }
}

pub fn read_code<R: Read>(reader: &mut BitReader<R>, dec: &HuffmanDec) -> Result<u16, Error> {
    let mut b = 0;
    let mut bits: Bits = 0;
    let mut index = 0;
    let mut first = 0;
    debug_assert_ne!(dec.count.len(), 1);
    while b < dec.count.len() {
        let mut e = 1;
        b += 1;
        while dec.count[b] == 0 {
            e += 1;
            b += 1;
        }
        bits <<= e;
        first <<= e;
        debug!("read {} bits", e);
        bits |= r#try!(reader.read_bits(e, false));
        let ct = dec.count[b];
        debug!("bits: {}", bits);
        debug!("first: {} ct: {}", first, ct);
        if bits >= first && bits < first + ct {
            debug_assert!(index + bits - first < dec.symbol.len() as u16);
            return Ok(dec.symbol[(index + bits - first) as usize]);
        }
        index += ct;
        first += ct;
    }
    Err(Error::new(ErrorKind::Other, "Illegal Huffman code"))
}

#[cfg(test)]
mod test {
    use super::*;
    use std::io::{BufReader, Cursor};

    #[test]
    fn fixed_huffman_literal() {
        let ref enc = FIXED_LITERAL_ENC;
        assert_eq!(enc[0].0, reverse(0b00110000, 8));
        assert_eq!(enc[144].0, reverse(0b110010000, 9));
        assert_eq!(enc[256].0, reverse(0b0000000, 7));
        assert_eq!(enc[280].0, reverse(0b11000000, 8));
        let ref dec = FIXED_LITERAL_DEC;
        assert_eq!(dec.count[7], 24);
        assert_eq!(dec.count[8], 152);
        assert_eq!(dec.count[9], 112);
        assert_eq!(dec.symbol[0], 256);
        assert_eq!(dec.symbol[24], 0);
        assert_eq!(dec.symbol[176], 144);
    }

    #[test]
    fn dynamic_huffman_codelen() {
        let code_lens = vec![2, 6, 6, 4, 5, 2, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 4, 4, 0];
        let enc = gen_huffman_enc(&code_lens);
        assert_eq!(enc[0].0, 0b00);
        assert_eq!(enc[1].0, reverse(62, code_lens[1]));
        assert_eq!(enc[3].0, reverse(12, code_lens[3]));
        assert_eq!(enc[4].0, reverse(30, code_lens[4]));
        assert_eq!(enc[5].0, reverse(1, code_lens[5]));
        assert_eq!(enc[17].0, reverse(13, code_lens[17]));
    }

    #[test]
    fn assign_lengths_test() {
        // Introduction to Algorithms, Third Edition, Figure 16.5
        let mut v = vec![0; 'f' as usize + 1];
        v['f' as usize] = 5;
        v['e' as usize] = 9;
        v['c' as usize] = 12;
        v['b' as usize] = 13;
        v['d' as usize] = 16;
        v['a' as usize] = 45;
        let l = assign_lengths(&v);
        assert_eq!(l['f' as usize], 4);
        assert_eq!(l['e' as usize], 4);
        assert_eq!(l['c' as usize], 3);
        assert_eq!(l['b' as usize], 3);
        assert_eq!(l['d' as usize], 3);
        assert_eq!(l['a' as usize], 1);
    }

    #[test]
    fn assign_lengths_re() {
        let mut v = vec![0; 6];
        v[5] = 2;
        let l = assign_lengths(&v);
        assert_eq!(l[5] as usize, 1);
    }

    #[test]
    fn single_symbol() {
        let code_lens = vec![1];
        //let enc = gen_huffman_enc(&code_lens);
        let dec = gen_huffman_dec(&code_lens, 1);
        //error!("{:?}", enc);
        //error!("{:?}", dec);

        let mut writer = BitWriter::new();
        let mut vec = writer.write_bits(0x0, 1);
        writer.flush().map(|c| {
            vec.push(c);
        });
        let mut input = BufReader::new(Cursor::new(vec));
        let mut reader = BitReader::new(&mut input);
        let _ = read_code(&mut reader, &dec);
    }
}
