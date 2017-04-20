use std::io::{Error, ErrorKind, Read, Write};

use bitstream::*;

const MAXBITS: usize = 15;

lazy_static! {
    pub static ref FIXED_LITERAL_DEC: HuffmanDec = HuffmanDec::fixed_literal_dec();
    pub static ref FIXED_LITERAL_ENC: Vec<(u8, Bits)> = HuffmanEnc::fixed_literal_enc();
}

#[allow(dead_code)]
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

#[allow(dead_code)]
pub struct HuffmanEnc {
    
}
impl HuffmanEnc {
    pub fn fixed_literal_enc() -> Vec<(u8, Bits)> {
        let lit: u16 = 288;
        let mut lit_lens: Vec<u8> = Vec::new();
        lit_lens.resize(lit as usize, 8);
        for s in 144..256 {
            lit_lens[s] = 9;
        }
        for s in 256..280 {
            lit_lens[s] = 7;
        }
        return gen_huffman_enc(&lit_lens);
    }
}

pub fn gen_huffman_enc(v: &Vec<u8>) -> Vec<(u8, Bits)> {
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
    let mut enc = Vec::<(u8, Bits)>::new();
    enc.resize(max_code+1, (0, 0));
    for n in 0..max_code+1 {
        let len = v[n] as usize;
        if len != 0 {
            enc[n] = (v[n], next_code[len]);
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
        if bits >= first && bits < first + count {
            return Ok(dec.symbol[(index + bits - first) as usize]);
        }
        index += count;
        first += count;
    }
    Err(Error::new(ErrorKind::Other, "Illegal Huffman code"))
}

pub fn write_code<W: Write>(writer: &mut BitWriter<W>, data: u16, enc: Vec<(u8, Bits)>) -> Result<u8, Error> {
    if enc.len() > data as usize {
        return Err(Error::new(ErrorKind::Other, "Data not in the table"));
    }
    let (n, bits) = enc[data as usize];
    return Ok(try!(writer.write_bits(bits, n, false)));
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn fixed_huffman_literal() {

        let ref enc = FIXED_LITERAL_ENC;
        assert!(enc[0].1 == 0b00110000);
        assert!(enc[144].1 == 0b110010000);
        assert!(enc[256].1 == 0b0000000);
        assert!(enc[280].1 == 0b11000000);
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
        assert!(enc[0].1 == 0b00);
        assert!(enc[1].1 == 62);
        assert!(enc[3].1 == 12);
        assert!(enc[4].1 == 30);
        assert!(enc[5].1 == 1);
        assert!(enc[17].1 == 13);
    }
}
