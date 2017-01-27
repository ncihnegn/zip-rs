use std::iter::FromIterator;

use bitstream::*;
use huffman::*;

pub const NUM_LITERAL: u16 = 288;

//static fixed_lit_count: Vec<u16> = vec!(0,0,0,0,0,0,280-256,144+288-280,256-244);

fn read_length(lit: u16, reader: &mut BitReader) -> u16 {
    let mut len = lit - 257;
    if len < 8 {
        len += 3;
    } else {
        let extra_bits = (len - 4) / 4;
        let extra = reader.read_bits(extra_bits as u8, true).unwrap();
        len = 10 + ((1 << (extra_bits + 1)) - 2) * 4 + ((len - 8) % 4) * (1 << extra_bits) + extra;
        debug!("Code: {} Extra Bits: {} Extra Value: {} Length: {}", lit, extra_bits, extra, len);
    }
    len
}

fn read_distance(dist_code: u16, reader: &mut BitReader) -> u16 {
    assert!(dist_code < 30);
    let mut distance = dist_code;
    if dist_code > 3 {
        let extra_bits = (dist_code - 2) / 2;
        let extra = reader.read_bits(extra_bits as u8, true).unwrap();
        distance = (1 << extra_bits) * (2 + (dist_code % 2)) + extra;
    }
    distance + 1
}

pub fn read_codelens(reader: &mut BitReader, clen_dec: &HuffmanDec, n: usize) -> Vec<u8> {
    let mut lens = Vec::<u8>::new();
    lens.resize(n, 0);
    let mut index = 0;
    while index < n {
        let s = read_code(reader, &clen_dec).unwrap() as u8;
        let mut count = 0;
        let mut len: u8 = 0;
        match s {
            0...15 => {
                lens[index] = s;
                index += 1;
            }
            16 => {
                assert!(!lens.is_empty());
                len = lens[index-1];
                count = reader.read_bits(2, true).unwrap() + 3;
            }
            17 => {
                count = reader.read_bits(3, true).unwrap() + 3;
            }
            18 => {
                count = reader.read_bits(7, true).unwrap() + 11;
            }
            _ => {
                panic!("Unknown code length");
            }
        }
        if s > 15 && s < 19 {
            assert!(index + count as usize <= n);
            for i in 0..count {
                lens[index + i as usize] = len;
            }
            index += count as usize;
        }
    }
    lens
}

pub fn read_code_table(reader: &mut BitReader) -> (HuffmanDec, HuffmanDec) {
    let hlit = reader.read_bits(5, true).unwrap() as usize + 257;
    let hdist = reader.read_bits(5, true).unwrap() as usize + 1;
    let hclen = reader.read_bits(4, true).unwrap() as usize + 4;
    let mut hclen_len = Vec::<u8>::new();
    let hclen_order = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
    let max_hclen = hclen_order.len();
    hclen_len.resize(max_hclen, 0);
    assert!(hlit <= 286 && hclen <= max_hclen && hdist <= 32);
    for i in 0..hclen {
        hclen_len[hclen_order[i]] = reader.read_bits(3, true).unwrap() as u8;
    }
    let clen_dec = gen_huffman_dec(&hclen_len, max_hclen as u16);
    let hlit_len = read_codelens(reader, &clen_dec, hlit);
    let hdist_len = read_codelens(reader, &clen_dec, hdist);
    (gen_huffman_dec(&hlit_len, hlit as u16), gen_huffman_dec(&hdist_len, hdist as u16))
}

pub fn read_fixed_literal(reader: &mut BitReader) -> u16 {
    let mut lit = reader.read_bits(7, false).unwrap();
    if lit <= 0b0010111 {
        lit += 256;
    } else {
        let b = reader.read_bits(1, false).unwrap();
        lit <<= 1;
        lit |= b;
        if lit <= 0b10111111 {
            lit -= 0b00110000;
        } else if lit <= 0b11000111 {
            lit -= 0b11000000;
            lit += 280;
        } else {
            let b = reader.read_bits(1, false).unwrap();
            lit <<= 1;
            lit |= b;
            lit -= 0b110010000;
            lit += 144;
        }
    }
    lit
}

pub fn inflate(reader: &mut BitReader, fixed: bool) -> Vec<u8> {
    let mut v = Vec::<u8>::new();
    let  (lit_dec, dist_dec) = if fixed { (HuffmanDec::fixed_literal(), HuffmanDec::new()) } else { read_code_table(reader) };
    loop {
        let lit = read_code(reader, &lit_dec).unwrap();
        match lit {
            0...255 => {
                v.push(lit as u8);
                debug!("lit: {:02x}", lit);
            }
            256 => break,
            257...285 => {
                let len = read_length(lit, reader) as usize;
                let dist_code = if fixed { reader.read_bits(5, false).unwrap() } else { read_code(reader, &dist_dec).unwrap() };
                let dist = read_distance(dist_code, reader) as usize;
                assert!(dist > 0);
                debug!("{}({}),{} {}", dist, dist_code, len, v.len());
                debug!("{:?}", v);
                assert!(dist <= v.len());
                //Fix the case len > dist
                let mut cur_len = len;
                if len > dist {
                    cur_len = dist;
                }
                let mut copied = 0;
                let mut seg = Vec::from_iter(v[v.len() - dist ..
                                           v.len() - dist + cur_len]
                                         .iter().cloned());
                while copied + cur_len <= len {
                    v.extend_from_slice(&seg);
                    copied += cur_len;
                }
                if copied < len {
                    cur_len = len - copied;
                    seg.resize(cur_len, 0);
                    v.extend_from_slice(&seg);
                }
            }
            _ => panic!("Out-of-range literal"),
        }
    }
    v
}


