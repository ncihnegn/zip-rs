use std::cmp::{Ordering, PartialOrd};
use std::collections::BinaryHeap;
use std::io::{Error, ErrorKind, Read};
use std::u16;

use bitstream::*;

const MAXBITS: usize = 15;
const MAXLITERAL: u16 = 287;

lazy_static! {
    pub static ref FIXED_LITERAL_DEC: HuffmanDec = HuffmanDec::fixed_literal_dec();
    pub static ref FIXED_LITERAL_ENC: Vec<(Bits, u8)> = HuffmanEnc::fixed_literal_enc();
}

#[derive(Eq, PartialEq)]
struct Char {
    val: u16,
    freq: usize,
    left: Option<Box<Char>>,
    right: Option<Box<Char>>
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

#[derive(Debug)]
pub struct HuffmanDec {
    count: Vec<u16>,
    symbol: Vec<u16>,
}

impl HuffmanDec {
    pub fn new() -> HuffmanDec {
        HuffmanDec { count: Vec::new(), symbol: Vec::new() }
    }

    pub fn fixed_literal_dec() -> HuffmanDec {
        let count: Vec<u16> = vec![0,0,0,0,0,0,0,280-256,144+288-280,256-144];
        let mut symbol: Vec<u16> = (256..280).collect();
        let mut len8 = (0..144).collect();
        symbol.append(&mut len8);
        let mut len8a: Vec<u16> = (280..288).collect();
        symbol.append(&mut len8a);
        let mut len9: Vec<u16> = (144..256).collect();
        symbol.append(&mut len9);
        HuffmanDec { count: count, symbol: symbol }
    }
}

pub struct HuffmanEnc {
    
}
impl HuffmanEnc {
    pub fn fixed_literal_enc() -> Vec<(Bits, u8)> {
        let mut lit_lens = Vec::<u8>::with_capacity(MAXLITERAL as usize + 1);
        lit_lens.resize(MAXLITERAL as usize + 1, 8);
        for s in 144..256 {
            lit_lens[s] = 9;
        }
        for s in 256..280 {
            lit_lens[s] = 7;
        }
        gen_huffman_enc(&lit_lens)
    }
}

/// Assign lengths based on frequencies
pub fn assign_lengths(v: &Vec<usize>) -> Vec<u8> {
    const NONLEAF: u16 = u16::MAX;
    let mut heap = BinaryHeap::new();
    // Build a min-heap
    for c in 0..v.len() {
        if v[c] > 0 {
            heap.push(Char { val: c as u16, freq: v[c], left: None, right: None});
        }
    }
    while heap.len() > 1 {
        let l = heap.pop().unwrap();
        let r = heap.pop().unwrap();
        heap.push(Char { val: NONLEAF, freq: l.freq + r.freq,
                         left: Some(Box::new(l)), right: Some(Box::new(r)) });
    }
    let root = heap.pop().unwrap();
    let mut todo = Vec::new();
    todo.push(root);
    let mut level: u8 = 0;
    let mut lengths = Vec::<u8>::with_capacity(v.len());
    lengths.resize(v.len(), 0);
    while !todo.is_empty() {
        let mut next = Vec::new();
        for c in todo {
            c.left.map(|l| { next.push(*l); });
            c.right.map(|r| { next.push(*r); });
            if c.val != NONLEAF {
                lengths[c.val as usize] = level;
            }
        }
        todo = next;
        level += 1;
    }
    lengths
}

/// Generate a canonical Huffman encoding table with lengths
pub fn gen_huffman_enc(v: &Vec<u8>) -> Vec<(Bits, u8)> {
    let mut bl_count = Vec::<Bits>::new();
    let max_bits = v.iter().max().unwrap().clone() as usize;
    bl_count.resize(max_bits+1, 0);
    for i in v {
        bl_count[*i as usize] += 1;
    }
    let mut next_code = Vec::<Bits>::new();
    next_code.resize(max_bits+1, 0);
    let mut code: Bits = 0;
    bl_count[0] = 0;
    for bits in 1..max_bits+1 {
        code = (code + bl_count[bits-1]) << 1;
        next_code[bits] = code;
    }
    let max_code = v.len()-1;
    let mut enc = Vec::<(Bits, u8)>::new();
    enc.resize(max_code+1, (0, 0));
    for n in 0..max_code+1 {
        let len = v[n] as usize;
        if len != 0 {
            enc[n] = (next_code[len], v[n]);
            next_code[len] += 1;
        }
    }
    enc
}

pub fn gen_huffman_dec(lengths: &Vec<u8>, n: u16) -> HuffmanDec {
    let mut count: Vec<u16> = Vec::new();
    let max_bits = lengths.iter().max().unwrap().clone() as usize;
    assert!(max_bits <= MAXBITS);
    count.resize(max_bits+1, 0);
    for i in lengths {
        if *i != 0 {
            count[*i as usize] += 1;
        }
    }
    let mut offsets: Vec<u16> = Vec::new();
    offsets.resize(max_bits+1, 0);
    for i in 1..max_bits {
        offsets[i+1] = offsets[i] + count[i];
    }
    //let n = offsets[max_bits+1];//total number of symbols
    let mut symbol: Vec<u16> = Vec::new();
    symbol.resize(n as usize, 0);
    for sym in 0..n {
        let len = lengths[sym as usize] as usize;
        if len > 0 {
            symbol[offsets[len] as usize] = sym;
            offsets[len] += 1;
        }
    }
    HuffmanDec { count: count, symbol: symbol }
}

pub fn read_code<R: Read>(reader: &mut BitReader<R>, dec: &HuffmanDec) -> Result<u16, Error> {
    let mut b = 0;
    let mut bits: Bits = 0;
    let mut index = 0;
    let mut first = 0;
    while b < dec.count.len() {
        let mut e = 1;
        b += 1;
        while dec.count[b] == 0 {
            e += 1;
            b += 1;
        }
        bits <<= e;
        first <<= e;
        bits |= try!(reader.read_bits(e, false));
        let count = dec.count[b];
        debug!("bits: {}", bits);
        if bits >= first && bits < first + count {
            assert!(index + bits - first < dec.symbol.len() as u16);
            return Ok(dec.symbol[(index + bits - first) as usize]);
        }
        index += count;
        first += count;
    }
    Err(Error::new(ErrorKind::Other, "Illegal Huffman code"))
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn fixed_huffman_literal() {
        let ref enc = FIXED_LITERAL_ENC;
        assert!(enc[0].0 == 0b00110000);
        assert!(enc[144].0 == 0b110010000);
        assert!(enc[256].0 == 0b0000000);
        assert!(enc[280].0 == 0b11000000);
        let ref dec = FIXED_LITERAL_DEC;
        assert!(dec.count[7] == 24);
        assert!(dec.count[8] == 152);
        assert!(dec.count[9] == 112);
        assert!(dec.symbol[0] == 256);
        assert!(dec.symbol[24] == 0);
        assert!(dec.symbol[176] == 144);
    }

    #[test]
    fn dynamic_huffman_codelen() {
        let code_lens = vec![2, 6, 6, 4, 5, 2, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 4, 4, 0];
        let enc = gen_huffman_enc(&code_lens);
        assert!(enc[0].0 == 0b00);
        assert!(enc[1].0 == 62);
        assert!(enc[3].0 == 12);
        assert!(enc[4].0 == 30);
        assert!(enc[5].0 == 1);
        assert!(enc[17].0 == 13);
    }

    #[test]
    fn assign_lengths_test() {
        // Introduction to Algorithms, Third Edition, Figure 16.5
        let mut v = Vec::new();
        v.resize('f' as usize + 1, 0);
        v['f' as usize] =  5;
        v['e' as usize] =  9;
        v['c' as usize] =  12;
        v['b' as usize] =  13;
        v['d' as usize] =  16;
        v['a' as usize] =  45;
        let l = assign_lengths(&v);
        assert!(l['f' as usize] == 4);
        assert!(l['e' as usize] == 4);
        assert!(l['c' as usize] == 3);
        assert!(l['b' as usize] == 3);
        assert!(l['d' as usize] == 3);
        assert!(l['a' as usize] == 1);
    }
}
