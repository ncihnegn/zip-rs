use std::io::{BufReader, BufWriter, Error, ErrorKind, Read, Write};
use std::iter::FromIterator;

use crc::crc32::{Digest, Hasher32, IEEE};
use num::FromPrimitive;

use bitstream::*;
use huffman::*;
use util::*;

const NUM_LITERAL: u16 = 288;
const MAXIMUM_DISTANCE: usize = 32 * 1024;
const MAXIMUM_LENGTH: usize = 258;
const HCLEN_ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];

#[repr(u16)]
#[derive(FromPrimitive)]
enum BlockType {
    Store = 0,
    FixedHuffman = 1,
    DynamicHuffman = 2,
}

//static fixed_lit_count: Vec<u16> = vec!(0,0,0,0,0,0,280-256,144+288-280,256-244);

fn read_length<R: Read>(lit: u16, reader: &mut BitReader<R>) -> Result<u16, Error> {
    let mut len = lit - 257;
    if len < 8 {
        len += 3;
    } else {
        let extra_bits = (len - 4) / 4;
        let extra = try!(reader.read_bits(extra_bits as u8, true));
        len = (1 << extra_bits) * 4 + 3 + ((len - 8) % 4) * (1 << extra_bits) + extra;
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

fn read_codelens<R: Read>(reader: &mut BitReader<R>, clen_dec: &HuffmanDec, n: usize) -> Result<Vec<u8>, Error> {
    debug!("To read {} code lengths", n);
    let mut lens = Vec::<u8>::with_capacity(n);
    lens.resize(n, 0);
    let mut index = 0;
    while index < n {
        let s = try!(read_code(reader, &clen_dec)) as u8;
        let mut count = 0;
        let mut len: u8 = 0;
        debug!("code len {}", s);
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
        debug!("index {}", index);
    }
    Ok(lens)
}

fn read_code_table<R: Read>(reader: &mut BitReader<R>) -> Result<(HuffmanDec, HuffmanDec), Error> {
    let hlit = try!(reader.read_bits(5, true)) as usize + 257;
    let hdist = try!(reader.read_bits(5, true)) as usize + 1;
    let hclen = try!(reader.read_bits(4, true)) as usize + 4;
    let max_hclen = HCLEN_ORDER.len();
    let mut hclen_len = Vec::<u8>::with_capacity(max_hclen);
    hclen_len.resize(max_hclen, 0);
    assert!(hlit <= 286 && hclen <= max_hclen && hdist <= 32);
    for i in 0..hclen {
        hclen_len[HCLEN_ORDER[i]] = try!(reader.read_bits(3, true)) as u8;
    }
    let clen_dec = gen_huffman_dec(&hclen_len, max_hclen as u16);
    let hlit_len = try!(read_codelens(reader, &clen_dec, hlit));
    let hdist_len = try!(read_codelens(reader, &clen_dec, hdist));
    debug!("Read code table done");
    Ok((gen_huffman_dec(&hlit_len, hlit as u16), gen_huffman_dec(&hdist_len, hdist as u16)))
}

fn encode_codelens(clen: &Vec<u8>) -> Vec<(u8, u8)> {
    let mut v = Vec::<(u8, u8)>::new();
    let len = clen.len();
    debug!("clen {} {:?}", len, clen);
    let mut i = 0;
    while i < len {
        if i == len-1 {
            v.push((clen[i], 0));
            break;
        }
        for j in (i+1)..len {
            let mut repeat = j - 1 - i;
            if clen[i] == 0 {
                repeat += 1;
            }
            if clen[j] != clen[i] || (clen[i] == 0 && repeat == 138) || (clen[i] != 0 && repeat == 6) {
                if repeat >= 3 {
                    repeat -= 3;
                    if clen[i] == 0 {
                        match repeat {
                            0...7 => {
                                v.push((17, repeat as u8));
                                debug!("({}, {})", 17, repeat);
                            }
                            8...135 => {
                                v.push((18, (repeat - 8) as u8));
                                debug!("({}, {})", 18, repeat - 8);
                            }
                            _ => panic!("Illegal Huffman code length")
                        }
                    } else {
                        v.push((clen[i], 0));
                        debug!("({}, {})", clen[i], 0);
                        v.push((16, repeat as u8));
                        debug!("({}, {})", 16, repeat);
                    }
                } else {
                    v.push((clen[i], 0));
                    debug!("({}, {})", clen[i], 0);
                    if clen[i] == 0 && repeat > 0 {
                        repeat -= 1;
                    }
                    for _ in 0..repeat {
                        v.push((clen[i], 0));
                        debug!("({}, {})", clen[i], 0);
                    }
                }
                i = j;
                debug!("currently {}", i);
                break;
            }
        }
    }
    debug!("{:?}", v);
    return v;
}

fn write_code_table(writer: &mut BitWriter, code_len: &Vec<u8>) -> Vec<u8> {
    let hlit = code_len.len() - 257;
    let mut v = writer.write_bits(hlit as u16, 5, true);
    let hdist = 1-1;
    v.extend(writer.write_bits(hdist as u16, 5, true).iter());
    let cclen = encode_codelens(&code_len);
    let mut freq = Vec::<usize>::with_capacity(HCLEN_ORDER.len());
    freq.resize(HCLEN_ORDER.len(), 0);
    freq[0] = 1;//dist 0
    for (c, _) in cclen.clone() {
        let cs = c as usize;
        debug_assert!(cs < HCLEN_ORDER.len());
        freq[cs] += 1;
    }
    while freq.len() > 4 && *(freq.last().unwrap()) == 0 {
        let _ = freq.pop();
    }
    let hclen = freq.len();
    v.extend(writer.write_bits((hclen - 4) as u16, 4, true).iter());
    let clen = assign_lengths(&freq);
    for i in HCLEN_ORDER.iter() {
        if *i < hclen {
            debug!("{}", *i);
            v.extend(writer.write_bits(clen[*i] as u16, 3, true).iter());
        }
    }
    let enc = gen_huffman_enc(&clen);
    for (c, r) in cclen {
        let (bits, bit_len) = enc[c as usize];
        v.extend(writer.write_bits(bits, bit_len, false).iter());
        match c {
            0...15 => {}
            16 => v.extend(writer.write_bits(r as u16, 2, true)),
            17 => v.extend(writer.write_bits(r as u16, 3, true)),
            18 => v.extend(writer.write_bits(r as u16, 7, true)),
            _ => panic!("Illegal code length Huffman code")
        }
    }
    //dist
    let (bits, bit_len) = enc[0];
    v.extend(writer.write_bits(bits, bit_len, false).iter());
    return v;
}

// Not being used
#[allow(dead_code)]
fn read_fixed_literal<R: Read>(reader: &mut BitReader<R>) -> u16 {
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

pub fn inflate<R: Read, W: Write>(input: &mut BufReader<R>, output: &mut BufWriter<W>) -> Result<(u32, u32), Error> {
    let mut decompressed_size: u32 = 0;
    let mut reader = BitReader::new(input);
    let last_block_bit = try!(reader.read_bits(1, true));
    if last_block_bit == 1 {
        debug!("Last Block");
    } else {
        debug!("Not last block");
    }
    let block_type = BlockType::from_u8(try!(reader.read_bits(2, true)) as u8);
    let mut hasher = Digest::new(IEEE);
    let mut dec = (HuffmanDec::new(), HuffmanDec::new());
    match block_type {
        Some(BlockType::Store) => debug!("Store"),
        Some(BlockType::FixedHuffman) => debug!("Fixed Huffman codes"),
        Some(BlockType::DynamicHuffman) => {
            debug!("Dynamic Huffman codes");
            dec = try!(read_code_table(&mut reader));
        }
        _ => return Err(Error::new(ErrorKind::Other, "Bad block type"))
    }
    let block_type = block_type.unwrap();
    let mut window = Vec::<u8>::with_capacity(MAXIMUM_DISTANCE + MAXIMUM_LENGTH);
    loop {
        let lit = match block_type {
            BlockType::Store => {
                try!(reader.read_bits(8, false)) as u16
            }
            BlockType::FixedHuffman => try!(read_code(&mut reader, &FIXED_LITERAL_DEC)),
            BlockType::DynamicHuffman => try!(read_code(&mut reader, &dec.0))
        };
        match lit {
            0...255 => {
                let byte = lit as u8;
                if window.len() == MAXIMUM_DISTANCE {
                    let byte: [u8; 1] = [window.remove(0); 1];
                    try!(output.write(&byte));
                    hasher.write(&byte);
                }
                window.push(byte);
                debug!("lit {}: {:02x}", decompressed_size, lit);
                decompressed_size += 1;
            }
            256 => {
                debug!("end of block");
                break;
            }
            257...285 => {
                let len = try!(read_length(lit, &mut reader)) as usize;
                assert!(len <= MAXIMUM_LENGTH);

                let dist_code = match block_type {
                    BlockType::FixedHuffman => try!(reader.read_bits(5, false)),
                    BlockType::DynamicHuffman => try!(read_code(&mut reader, &dec.1)),
                    _ => return Err(Error::new(ErrorKind::Other, "Bad block type; Shouldn't reach here"))
                };
                let dist = try!(read_distance(dist_code, &mut reader)) as usize;
                debug!("{}: {}", decompressed_size, to_hex_string(&window));
                debug!("{}({}), {} {}", dist, dist_code, len, window.len());
                assert!(dist > 0 && dist < MAXIMUM_DISTANCE);
                assert!(dist <= window.len());
                if window.len() + len > window.capacity() {
                    let to_write = window.len() + len - window.capacity();
                    try!(output.write(&window[0..to_write]));
                    hasher.write(&window[0..to_write]);
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
                decompressed_size += len as u32;
            }
            _ => {
                return Err(Error::new(ErrorKind::Other, "Bad literal"));
            }
        }
    }
    try!(output.write(window.as_slice()));
    hasher.write(window.as_slice());
    Ok((decompressed_size, hasher.sum32()))
}

pub fn deflate<R: Read, W: Write>(input: &mut BufReader<R>, output: &mut BufWriter<W>) -> Result<(u32, u32), Error> {
    let mut window = Vec::<u8>::new();
    let mut bytes = [0 as u8; MAXIMUM_LENGTH];
    let mut data = Vec::<u8>::new();
    let mut hasher = Digest::new(IEEE);
    let mut writer = BitWriter::new();
    writer.write_bits(1, 1, true);
    writer.write_bits(BlockType::DynamicHuffman as u16, 2, true);
    let mut freq = Vec::<usize>::with_capacity(NUM_LITERAL as usize);
    freq.resize(257, 0);
    freq[256] = 1;
    let mut read_len = 0;
    loop {
        let len = input.read(&mut bytes).unwrap();
        read_len += len;
        for i in 0..len {
            freq[bytes[i] as usize] += 1;
        }

        if len == 0 {
            break;
        }
        data.extend(&bytes[0..len]);
    }
    debug!("read len {}", read_len);
    let code_len = assign_lengths(&freq);
    debug!("window {:?}", window);
    let v = write_code_table(&mut writer, &code_len);
    window.extend(v.iter());
    debug!("window {:?}", window);
    let enc = gen_huffman_enc(&code_len);
    for b in data {
        let (bits, bits_len) = enc[b as usize];
        debug!("byte {:02x}->{} {}", b, bits, bits_len);
        let v = writer.write_bits(bits, bits_len, false);
        window.extend(v.iter());
        //debug!("window {:?}", window);
    }
    let (bits, bits_len) = enc[256];//end
    let v = writer.write_bits(bits, bits_len, false);
    window.extend(v.iter());
    writer.flush().map(|c| { window.push(c); });
    debug!("window {:?}", window);
    try!(output.write(&window[0..window.len()]));
    hasher.write(&window[0..window.len()]);
    let compressed_size = window.len() as u32;
    debug!("compressed size: {}", compressed_size);
    Ok((compressed_size, hasher.sum32()))
}

#[cfg(test)]
mod test {
    use super::*;

    use env_logger;
    use rand::{self, Rng};

    #[test]
    fn huffman_literals() {
        //env_logger::init().unwrap();
        let uncompressed_len = 128;//rand::random::<u16>() as usize;
        debug!("uncompressed length: {}", uncompressed_len);
        let mut uncompressed = Vec::<u8>::with_capacity(uncompressed_len);
        uncompressed.resize(uncompressed_len, 0);
        let mut rng = rand::thread_rng();
        rng.fill_bytes(&mut uncompressed);
        debug!("uncompressed : {:?}", uncompressed);
        let mut hasher = Digest::new(IEEE);
        hasher.write(&uncompressed);
        let crc = hasher.sum32();
        let mut compressed = Vec::<u8>::new();
        {
            let mut reader = BufReader::new(&uncompressed as &[u8]);
            let mut writer = BufWriter::new(&mut compressed);
            let (compressed_len, ccrc) = deflate(&mut reader, &mut writer).unwrap();
            println!("{} {}", compressed_len, ccrc);
            let _ = writer.flush();
        }
        let mut reader = BufReader::new(&compressed as &[u8]);
        let mut decompressed = Vec::<u8>::new();
        let mut writer = BufWriter::new(&mut decompressed);
        let (decompressed_len, dcrc) = inflate(&mut reader, &mut writer).unwrap();
        assert_eq!(uncompressed_len, decompressed_len as usize);
        assert_eq!(crc, dcrc);
    }

    #[test]
    fn codelen_alphabet() {
        let len = rand::random::<u16>() as usize;
        let mut v = Vec::with_capacity(len);
        v.resize(len, 0);
        let mut rng = rand::thread_rng();
        for i in 0..len {
            v[i] = rng.gen_range(0, 16);//[0,16)
        }
        let clens = encode_codelens(&v);
        let mut d = Vec::<u8>::with_capacity(len);
        for (c, r) in clens {
            match c {
                0...15 => d.push(c),
                16 => {
                    let c = d.pop().unwrap();
                    d.push(c);
                    for _ in 0..(r+3) {
                        d.push(c);
                    }
                }
                17...18 => {
                    let rep = if c == 17 { r + 3 } else { r + 11 };
                    for _ in 0..rep {
                        d.push(0);
                    }
                }
                _ => panic!("Illegal clen character")
            }
        }
        assert_eq!(v.len(), d.len());
        assert_eq!(v, d);
    }

    #[test]
    fn codelen_huffman() {
        let len = rand::random::<u16>() as usize;
        let mut v = Vec::with_capacity(len);
        v.resize(len, 0);
        let mut rng = rand::thread_rng();
        for i in 0..len {
            v[i] = rng.gen_range(0, 16);//[0,16)
        }
        let clens = encode_codelens(&v);
        let mut freq = Vec::<usize>::with_capacity(HCLEN_ORDER.len());
        freq.resize(HCLEN_ORDER.len(), 0);
        for (c, _) in clens {
            let cs = c as usize;
            debug_assert!(cs < HCLEN_ORDER.len());
            freq[cs] += 1;
        }
        while freq.len() > 4 && *(freq.last().unwrap()) == 0 {
            let _ = freq.pop();
        }
        let hclen = freq.len();
        let mut writer = BitWriter::new();
        let mut encoded = Vec::new();
        {
            let clens = assign_lengths(&freq);
            for i in HCLEN_ORDER.iter() {
                if *i < hclen {
                    debug!("{}", *i);
                    encoded.extend(writer.write_bits(clens[*i] as u16, 3, true).iter());
                }
            }
            let enc = gen_huffman_enc(&clens);
            for (c, r) in clens {
                let (bits, bit_len) = enc[c as usize];
                encoded.extend(writer.write_bits(bits, bit_len, false).iter());
                match c {
                    0...15 => {}
                    16 => encoded.extend(writer.write_bits(r as u16, 2, true)),
                    17 => encoded.extend(writer.write_bits(r as u16, 3, true)),
                    18 => encoded.extend(writer.write_bits(r as u16, 7, true)),
                    _ => panic!("Illegal code length Huffman code")
                }
            }
            encoded.extend(writer.flush());
        }
        let mut d = Vec::<u8>::with_capacity(len);
        let clens = Vec::<(u8, u8)>::new();
        for (c, r) in clens {
            match c {
                0...15 => d.push(c),
                16 => {
                    let c = d.pop().unwrap();
                    d.push(c);
                    for _ in 0..(r+3) {
                        d.push(c);
                    }
                }
                17...18 => {
                    let rep = if c == 17 { r + 3 } else { r + 11 };
                    for _ in 0..rep {
                        d.push(0);
                    }
                }
                _ => panic!("Illegal clen character")
            }
        }
        assert_eq!(v.len(), d.len());
        assert_eq!(v, d);
    }
}
