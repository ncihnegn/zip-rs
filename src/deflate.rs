use std::io::{BufReader, BufWriter, Error, ErrorKind, Read, Write};
use std::iter::FromIterator;

use bitstream::*;
use huffman::*;
use util::*;

pub const NUM_LITERAL: u16 = 288;
pub const MAXIMUM_DISTANCE: usize = 32 * 1024;
pub const MAXIMUM_LENGTH: usize = 258;

//static fixed_lit_count: Vec<u16> = vec!(0,0,0,0,0,0,280-256,144+288-280,256-244);
lazy_static! {
    pub static ref FIXED_LITERAL_DEC: HuffmanDec = HuffmanDec::fixed_literal_dec();
}

fn read_length<R: Read>(lit: u16, reader: &mut BitReader<R>) -> Result<u16, Error> {
    let mut len = lit - 257;
    if len < 8 {
        len += 3;
    } else {
        let extra_bits = (len - 4) / 4;
        let extra = try!(reader.read_bits(extra_bits as u8, true));
        len = 7 + ((1 << (extra_bits + 1)) - 2) * 4 + ((len - 8) % 4) * (1 << extra_bits) + extra;
        debug!("Code: {} Extra Bits: {} Extra Value: {} Length: {}", lit, extra_bits, extra, len);
    }
    Ok(len)
}

fn read_distance<R: Read>(dist_code: u16, reader: &mut BitReader<R>) -> Result<u16, Error> {
    assert!(dist_code < 30);
    let mut distance = dist_code;
    if dist_code > 3 {
        let extra_bits = (dist_code - 2) / 2;
        let extra = try!(reader.read_bits(extra_bits as u8, true));
        distance = (1 << extra_bits) * (2 + (dist_code % 2)) + extra;
    }
    Ok(distance + 1)
}

pub fn read_codelens<R: Read>(reader: &mut BitReader<R>, clen_dec: &HuffmanDec, n: usize) -> Result<Vec<u8>, Error> {
    let mut lens = Vec::<u8>::new();
    lens.resize(n, 0);
    let mut index = 0;
    while index < n {
        let s = try!(read_code(reader, &clen_dec)) as u8;
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
                count = try!(reader.read_bits(2, true)) + 3;
            }
            17 => {
                count = try!(reader.read_bits(3, true)) + 3;
            }
            18 => {
                count = try!(reader.read_bits(7, true)) + 11;
            }
            _ => {
                return Err(Error::new(ErrorKind::Other, "Bad code length"));
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
    Ok(lens)
}

pub fn read_code_table<R: Read>(reader: &mut BitReader<R>) -> Result<(HuffmanDec, HuffmanDec), Error> {
    let hlit = try!(reader.read_bits(5, true)) as usize + 257;
    let hdist = try!(reader.read_bits(5, true)) as usize + 1;
    let hclen = try!(reader.read_bits(4, true)) as usize + 4;
    let mut hclen_len = Vec::<u8>::new();
    let hclen_order = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
    let max_hclen = hclen_order.len();
    hclen_len.resize(max_hclen, 0);
    assert!(hlit <= 286 && hclen <= max_hclen && hdist <= 32);
    for i in 0..hclen {
        hclen_len[hclen_order[i]] = try!(reader.read_bits(3, true)) as u8;
    }
    let clen_dec = gen_huffman_dec(&hclen_len, max_hclen as u16);
    let hlit_len = try!(read_codelens(reader, &clen_dec, hlit));
    let hdist_len = try!(read_codelens(reader, &clen_dec, hdist));
    Ok((gen_huffman_dec(&hlit_len, hlit as u16), gen_huffman_dec(&hdist_len, hdist as u16)))
}

pub fn read_fixed_literal<R: Read>(reader: &mut BitReader<R>) -> u16 {
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

pub fn inflate<R: Read, W: Write>(input: &mut BufReader<R>, output: &mut BufWriter<W>) -> Result<usize, Error> {
    let mut decompressed_size = 0;
    let mut reader = BitReader::new(input);
    let last_block_bit = try!(reader.read_bits(1, true));
    if last_block_bit == 1 {
        debug!("Last Block bit is set");
    }
    let block_type = try!(reader.read_bits(2, true));
    let mut fixed_huffman = false;
    match block_type {
        0 => debug!("Block is stored"),
        1 => {
            debug!("Fixed Huffman codes");
            fixed_huffman = true;
        }
        2 => debug!("Dynamic Huffman codes"),
        _ => {
            return Err(Error::new(ErrorKind::Other, "Bad block type"));
        }
    }
    let mut window = Vec::<u8>::with_capacity(MAXIMUM_DISTANCE + MAXIMUM_LENGTH);
    let (lit_dec, dist_dec) = if fixed_huffman { (HuffmanDec::new(), HuffmanDec::new()) } else { try!(read_code_table(&mut reader)) };
    loop {
        let lit = if fixed_huffman { try!(read_code(&mut reader, &FIXED_LITERAL_DEC)) } else { try!(read_code(&mut reader, &lit_dec)) };
        match lit {
            0...255 => {
                let byte = lit as u8;
                if window.len() == MAXIMUM_DISTANCE {
                    let byte: [u8; 1] = [window.remove(0); 1];
                    try!(output.write(&byte));
                }
                window.push(byte);
                debug!("lit: {:02x}", lit);
                decompressed_size += 1;
            }
            256 => break,
            257...285 => {
                let len = try!(read_length(lit, &mut reader)) as usize;
                assert!(len <= MAXIMUM_LENGTH);
                let dist_code = if fixed_huffman { try!(reader.read_bits(5, false)) } else { try!(read_code(&mut reader, &dist_dec)) };
                let dist = try!(read_distance(dist_code, &mut reader)) as usize;
                debug!("{}: {}", decompressed_size, to_hex_string(&window));
                debug!("{}({}), {} {}", dist, dist_code, len, window.len());
                assert!(dist > 0 && dist < MAXIMUM_DISTANCE);
                assert!(dist <= window.len());
                if window.len() + len > window.capacity() {
                    let to_write = window.len() + len - window.capacity();
                    try!(output.write(&window[0..to_write]));
                    window.drain(0..to_write);
                }
                //Fix the case len > dist
                let mut cur_len = len;
                if len > dist {
                    cur_len = dist;
                }
                let mut copied = 0;
                let first = window.len() - dist;
                let mut seg = Vec::from_iter(window[first..first + cur_len]
                                             .iter().cloned());
                while copied + cur_len <= len {
                    window.extend_from_slice(&seg);
                    copied += cur_len;
                }
                if copied < len {
                    cur_len = len - copied;
                    seg.resize(cur_len, 0);
                    window.extend_from_slice(&seg);
                }
                decompressed_size += len;
            }
            _ => {
                return Err(Error::new(ErrorKind::Other, "Bad literal"));
            }
        }
    }
    try!(output.write(window.as_slice()));
    Ok(decompressed_size)
}

