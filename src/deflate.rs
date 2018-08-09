use std::collections::HashMap;
use std::io::{BufReader, BufWriter, Error, ErrorKind, Read, Write};
use std::iter::FromIterator;
use std::u16;

use crc::crc32::{Digest, Hasher32, IEEE};
use num::FromPrimitive;

use bitstream::*;
use constant::*;
use huffman::*;
use util::*;

#[repr(u16)]
#[derive(FromPrimitive)]
enum BlockType {
    Store = 0,
    FixedHuffman = 1,
    DynamicHuffman = 2,
}

#[derive(Clone, Debug)]
enum CodeLength {
    Single(u8),
    Repeat { code: u8, repeat: u8 },
}

pub enum LZ77 {
    Literal(u16),
    Copy { len: usize, dist: usize },
}

//static fixed_lit_count: Vec<u16> = vec!(0,0,0,0,0,0,280-256,144+288-280,256-244);

fn read_length<R: Read>(lit: u16, reader: &mut BitReader<R>) -> Result<u16, Error> {
    let mut len = lit - (END_OF_BLOCK + 1);
    if len < 8 {
        len += 3;
    } else {
        let extra_bits = (len - 4) / 4;
        let extra = try!(reader.read_bits(extra_bits as u8, true));
        len = (1 << extra_bits) * 4 + 3 + ((len - 8) % 4) * (1 << extra_bits) + extra;
        debug!(
            "Code: {} Extra Bits: {} Extra Value: {} Length: {}",
            lit, extra_bits, extra, len
        );
    }
    Ok(len)
}

fn length_code(len: usize) -> Result<(usize, u8), Error> {
    //let bits = ((len - 10) as f32).log2().ceil() - 2;
    match len {
        3...10 => Ok((len + 254, 0)),
        11...18 => Ok((260 + ((len + 1) >> 1), 1)),
        19...34 => Ok((264 + ((len + 1) >> 2), 2)),
        35...66 => Ok((269 + ((len - 3) >> 3), 3)),
        67...130 => Ok((273 + ((len - 3) >> 4), 4)),
        131...257 => Ok((277 + ((len - 3) >> 5), 5)),
        258 => Ok((285, 6)),
        _ => Err(Error::new(ErrorKind::Other, "Incorrect length")),
    }
}

fn dist_code(dist: usize) -> Result<(usize, u8), Error> {
    let dm5 = dist - 5;
    let bits = (dm5 as f32).log2().floor() as usize;
    match dist {
        1...4 => Ok((dm5 + 4, bits as u8)),
        5...32_768 => Ok(((dm5 >> bits) + 4, bits as u8)),
        _ => Err(Error::new(ErrorKind::Other, "Wrong distance")),
    }
}

fn read_distance<R: Read>(dcode: u16, reader: &mut BitReader<R>) -> Result<u16, Error> {
    let distance = if dcode > 3 {
        let extra_bits = (dcode - 2) / 2;
        let extra = try!(reader.read_bits(extra_bits as u8, true));
        (1 << extra_bits) * (2 + (dcode % 2)) + extra
    } else {
        dcode
    };
    Ok(distance + 1)
}

fn read_code_lengths<R: Read>(
    reader: &mut BitReader<R>,
    clen_dec: &HuffmanDec,
    n: usize,
) -> Result<Vec<u8>, Error> {
    debug!("To read {} code lengths", n);
    let mut lens = Vec::<u8>::with_capacity(n);
    lens.resize(n, 0);
    let mut index = 0;
    while index < n {
        let s = try!(read_code(reader, clen_dec)) as u8;
        let mut count = 0;
        let mut len: u8 = 0;
        debug!("code len {}", s);
        match s {
            0...15 => {
                lens[index] = s;
                index += 1;
            }
            16 => {
                debug_assert!(!lens.is_empty());
                len = lens[index - 1];
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
            for l in lens.iter_mut().skip(index).take(count as usize) {
                *l = len;
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
    for i in HCLEN_ORDER.iter().take(hclen) {
        hclen_len[*i] = try!(reader.read_bits(3, true)) as u8;
    }
    let clen_dec = gen_huffman_dec(&hclen_len, max_hclen as u16);
    let hlit_len = try!(read_code_lengths(reader, &clen_dec, hlit));
    let hdist_len = try!(read_code_lengths(reader, &clen_dec, hdist));
    debug!("Read code table done");
    info!("hlit_len: {} {:?}", hlit, hlit_len);
    info!("hdist_len: {} {:?}", hdist, hdist_len);
    Ok((
        gen_huffman_dec(&hlit_len, hlit as u16),
        gen_huffman_dec(&hdist_len, hdist as u16),
    ))
}

fn encode_code_lengths(clen: &[u8]) -> Vec<CodeLength> {
    let mut v = Vec::<CodeLength>::new();
    let len = clen.len();
    if len == 0 {
        debug!("Empty code lengths");
        return v;
    }
    let mut repeat = 0;
    let mut prev = 0; //implicitly add one for repeat zeros
    for (i, cur) in clen.iter().enumerate().take(len) {
        let cur_dump = if *cur == prev && (repeat < 6 || *cur == 0) && repeat < 138 {
            repeat += 1;
            if i != len - 1 {
                continue;
            }
            false
        } else {
            true
        };
        match repeat {
            0 => {
                if i > 0 {
                    v.push(CodeLength::Single(prev));
                }
            }
            1...2 => {
                if prev != 0 {
                    repeat += 1;
                }
                for _ in 0..repeat {
                    v.push(CodeLength::Single(prev));
                }
            }
            3...10 => {
                if prev != 0 {
                    v.push(CodeLength::Single(prev));
                }
                v.push(CodeLength::Repeat {
                    code: if prev == 0 { 17 } else { 16 },
                    repeat: repeat - 3,
                });
            }
            11...138 => v.push(CodeLength::Repeat {
                code: 18,
                repeat: repeat - 11,
            }),
            _ => panic!("Illegal repeat"),
        }
        if cur_dump && i == len - 1 {
            v.push(CodeLength::Single(*cur));
        }
        if *cur != 0 {
            repeat = 0;
        } else {
            repeat = 1;
        }
        prev = *cur;
    }
    v
}

fn update_freq(freq: &mut Vec<usize>, eclens: &[CodeLength]) {
    for cl in eclens.iter() {
        match *cl {
            CodeLength::Single(c) | CodeLength::Repeat { code: c, .. } => freq[c as usize] += 1,
        }
    }
}

fn reordered_code_lengths(clens: &[u8]) -> Vec<u8> {
    let mut mapped_clens = Vec::with_capacity(HCLEN_ORDER.len());
    mapped_clens.resize(HCLEN_ORDER.len(), 0);
    for (i, o) in HCLEN_ORDER.iter().enumerate().take(clens.len()) {
        mapped_clens[i] = clens[*o as usize];
    }
    while mapped_clens.len() > 4 && *(mapped_clens.last().unwrap()) == 0 {
        let _ = mapped_clens.pop();
    }
    mapped_clens
}

fn write_code_table(writer: &mut BitWriter, lit_clens: &[u8], dist_clens: &[u8]) -> Vec<u8> {
    let hlit = lit_clens.len() - 257;
    let mut v = writer.write_bits(hlit as u16, 5);
    let hdist = dist_clens.len() - 1;
    v.extend(writer.write_bits(hdist as u16, 5).iter());
    let lit_eclens = encode_code_lengths(lit_clens);
    let dist_eclens = encode_code_lengths(dist_clens);
    let mut freq = Vec::<usize>::with_capacity(HCLEN_ORDER.len());
    freq.resize(HCLEN_ORDER.len(), 0);
    update_freq(&mut freq, &lit_eclens);
    update_freq(&mut freq, &dist_eclens);
    let clen = assign_lengths(&freq);
    let mapped_clens = reordered_code_lengths(&clen);
    let hclen = mapped_clens.len();
    v.extend(writer.write_bits((hclen - 4) as u16, 4).iter());
    debug!("Write clen codes");
    for i in mapped_clens {
        v.extend(writer.write_bits(u16::from(i), 3).iter());
        //debug!("{}->{}", HCLEN_ORDER[i], mapped_clens[i]);
    }
    let enc = gen_huffman_enc(&clen);
    debug!("Write lit code lengths");
    v.extend(write_code_lengths(writer, &lit_eclens, &enc).iter());
    v.extend(write_code_lengths(writer, &dist_eclens, &enc).iter());
    v
}

fn write_code_lengths(
    writer: &mut BitWriter,
    eclens: &[CodeLength],
    enc: &[(Bits, u8)],
) -> Vec<u8> {
    let mut v = Vec::new();
    for cl in eclens {
        match *cl {
            CodeLength::Single(c) => {
                let (bits, bit_len) = enc[c as usize];
                v.extend(writer.write_bits(bits, bit_len).iter());
            }
            CodeLength::Repeat { code: l, repeat: r } => {
                let (bits, bit_len) = enc[l as usize];
                v.extend(writer.write_bits(bits, bit_len).iter());
                match l {
                    16 | 17 => v.extend(writer.write_bits(u16::from(r), l - 14)),
                    18 => v.extend(writer.write_bits(u16::from(r), 7)),
                    _ => panic!("Illegal code length Huffman code"),
                }
            }
        };
    }
    v
}

// Not being used
#[allow(dead_code)]
fn read_fixed_literal<R: Read>(reader: &mut BitReader<R>) -> u16 {
    let mut lit = reader.read_bits(7, false).unwrap();
    if lit <= 0b001_0111 {
        lit += 256;
    } else {
        let b = reader.read_bits(1, false).unwrap();
        lit <<= 1;
        lit |= b;
        if lit <= 0b1011_1111 {
            lit -= 0b0011_0000;
        } else if lit <= 0b1100_0111 {
            lit -= 0b1100_0000;
            lit += 280;
        } else {
            let b = reader.read_bits(1, false).unwrap();
            lit <<= 1;
            lit |= b;
            lit -= 0b1_1001_0000;
            lit += 144;
        }
    }
    lit
}

pub fn inflate<R: Read, W: Write>(
    input: &mut BufReader<R>,
    output: &mut BufWriter<W>,
) -> Result<(u32, u32), Error> {
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
        _ => return Err(Error::new(ErrorKind::Other, "Bad block type")),
    }
    info!("Dec {:?}", dec);
    let block_type = block_type.unwrap();
    let mut window = Vec::<u8>::with_capacity(MAX_DIST + MAX_LEN);
    loop {
        let lit = match block_type {
            BlockType::Store => try!(reader.read_bits(8, false)) as u16,
            BlockType::FixedHuffman => try!(read_code(&mut reader, &FIXED_LITERAL_DEC)),
            BlockType::DynamicHuffman => try!(read_code(&mut reader, &dec.0)),
        };
        match lit {
            0...255 => {
                let byte = lit as u8;
                debug!("byte {}", byte);
                if window.len() == MAX_DIST {
                    let mut b: [u8; 1] = [0; 1];
                    b[0] = window.remove(0); //workaround clippy bug
                    debug!("write");
                    let _ = try!(output.write(&b));
                    debug!("hasher");
                    hasher.write(&b);
                }
                window.push(byte);
                debug!("inflate lit {:02x}", lit);
                decompressed_size += 1;
            }
            END_OF_BLOCK => {
                debug!("end of block");
                break;
            }
            257...285 => {
                let len = try!(read_length(lit, &mut reader)) as usize;
                assert!(len <= MAX_LEN);

                let dcode = match block_type {
                    BlockType::FixedHuffman => try!(reader.read_bits(5, false)),
                    BlockType::DynamicHuffman => try!(read_code(&mut reader, &dec.1)),
                    _ => {
                        return Err(Error::new(
                            ErrorKind::Other,
                            "Bad block type; Shouldn't reach here",
                        ))
                    }
                };
                assert!(dcode < NUM_DIST_CODE);
                let dist = try!(read_distance(dcode, &mut reader)) as usize;
                debug!("{}: {}", decompressed_size, to_hex_string(&window));
                info!("inflate copy {} {}", dist, len);
                assert!(dist > 0 && dist < MAX_DIST);
                assert!(dist <= window.len());
                if window.len() + len > window.capacity() {
                    let to_write = window.len() + len - window.capacity();
                    let _ = try!(output.write(&window[0..to_write]));
                    hasher.write(&window[0..to_write]);
                    window.drain(0..to_write);
                }
                //Fix the case len > dist
                let mut cur_len = if len > dist { dist } else { len };
                let mut copied = 0;
                let first = window.len() - dist;
                let seg = Vec::from_iter(window[first..first + cur_len].iter().cloned());
                while copied + cur_len <= len {
                    window.extend_from_slice(&seg);
                    copied += cur_len;
                }
                if copied < len {
                    cur_len = len - copied;
                    window.extend_from_slice(&seg[0..cur_len]);
                }
                decompressed_size += len as u32;
            }
            _ => {
                return Err(Error::new(ErrorKind::Other, "Bad literal"));
            }
        }
    }
    let _ = try!(output.write(window.as_slice()));
    hasher.write(window.as_slice());
    Ok((decompressed_size, hasher.sum32()))
}

fn compare(bytes: &[u8], i: usize, j: usize) -> usize {
    let mut len = 0;
    while j + len < bytes.len() && bytes[i + len] == bytes[j + len] {
        len += 1;
    }
    len
}

pub fn deflate<R: Read, W: Write>(
    input: &mut BufReader<R>,
    output: &mut BufWriter<W>,
) -> Result<(u32, u32), Error> {
    let mut window = Vec::<u8>::new();
    let mut bytes = [0 as u8; u16::MAX as usize];
    let mut vlz = Vec::<LZ77>::new();
    let mut hasher = Digest::new(IEEE);
    let mut writer = BitWriter::new();

    let mut lfreq = Vec::<usize>::with_capacity(MAX_NUM_LIT);
    lfreq.resize(MAX_NUM_LIT, 0);
    let mut dfreq = Vec::<usize>::with_capacity(MAX_DIST);
    dfreq.resize(MAX_DIST, 0);
    let mut read_len = 0;

    loop {
        let len = input.read(&mut bytes).unwrap();
        if len == 0 {
            if read_len == 0 {
                return Ok((0, 0));
            } else {
                break;
            }
        } else if read_len == 0 {
            writer.write_bits(1, 1);
            writer.write_bits(BlockType::DynamicHuffman as u16, 2);
        }
        let mut head = HashMap::<usize, usize>::new();
        read_len += len;
        if len >= MIN_LEN {
            let mut prev = Vec::<usize>::with_capacity(len - (MIN_LEN - 1));
            prev.resize(len - (MIN_LEN - 1), len);
            for (i, b) in bytes.windows(MIN_LEN).enumerate().take(len - (MIN_LEN - 1)) {
                let hash = trans24(b);
                prev[i] = *(head.get(&hash).unwrap_or(&len));
                let _ = head.insert(hash, i);
                let mut next = prev[i];
                let mut max_len: usize = 0;
                let mut max_dist: usize = 0;
                while next != len && i - next < MAX_DIST {
                    let len = compare(&bytes, i, next);
                    if len > max_len {
                        max_dist = i - next;
                        max_len = len;
                    }
                    next = prev[next];
                }
                if max_len >= MIN_LEN {
                    lfreq[length_code(max_len).unwrap().0] += 1;
                    dfreq[dist_code(max_dist).unwrap().0] += 1;
                    info!("deflate copy {} {}", max_dist, max_len);
                    vlz.push(LZ77::Copy {
                        len: max_len,
                        dist: max_dist,
                    });
                } else {
                    lfreq[b[0] as usize] += 1;
                    info!("deflate lit {:02x}", b[0]);
                    vlz.push(LZ77::Literal(u16::from(b[0])));
                }
            }
        }

        let begin = if len >= MIN_LEN {
            len - (MIN_LEN - 1)
        } else {
            0
        };
        for b in bytes.iter().take(len).skip(begin) {
            lfreq[*b as usize] += 1;
            info!("deflate lit {:02x}", *b);
            vlz.push(LZ77::Literal(u16::from(*b)));
        }
    }
    while lfreq.len() > MIN_NUM_LIT && *(lfreq.last().unwrap()) == 0 {
        lfreq.pop(); //lfreq.resize(257, 0);//literals only
    }
    while !dfreq.is_empty() && *(dfreq.last().unwrap()) == 0 {
        dfreq.pop();
    }
    vlz.push(LZ77::Literal(END_OF_BLOCK));
    lfreq[END_OF_BLOCK as usize] += 1;
    debug!("read len {}", read_len);
    let lit_clens = assign_lengths(&lfreq);
    debug!("window {:?}", window);

    info!("dfreq {:?}", dfreq);
    let mut dist_clens = assign_lengths(&dfreq);
    info!("dist_clens {:?}", dist_clens);
    if dist_clens.is_empty() {
        // No copy at all
        dist_clens.push(0);
    }
    window.extend(write_code_table(&mut writer, &lit_clens, &dist_clens).iter());
    debug!("window {:?}", window);
    let lenc = gen_huffman_enc(&lit_clens);
    let denc = gen_huffman_enc(&dist_clens);
    info!("denc len {}", denc.len());
    let vhuff = dehuffman(&vlz, &lenc, &denc);
    for (bits, bits_len) in vhuff {
        let v = writer.write_bits(bits, bits_len);
        window.extend(v.iter());
        //debug!("window {:?}", window);
    }
    if let Some(c) = writer.flush() {
        window.push(c);
    }
    debug!("window {:?}", window);
    try!(output.write_all(&window));
    hasher.write(&window[0..window.len()]);
    let compressed_size = window.len();
    debug!("compressed size: {}", compressed_size);
    Ok((compressed_size as u32, hasher.sum32()))
}

fn dehuffman(vlz: &[LZ77], lenc: &[(Bits, u8)], denc: &[(Bits, u8)]) -> Vec<(Bits, u8)> {
    let mut vhuff = Vec::new();
    for b in vlz {
        match *b {
            LZ77::Literal(l) => vhuff.push(lenc[l as usize]),
            LZ77::Copy { len: l, dist: d } => {
                let lc = length_code(l).unwrap();
                vhuff.push(lenc[lc.0]);
                vhuff.push(((lc.0 & ((1 << lc.1) - 1)) as u16, lc.1));
                let dc = dist_code(d).unwrap();
                vhuff.push(denc[dc.0]);
                vhuff.push(((dc.0 & ((1 << dc.1) - 1)) as u16, dc.1));
            }
        }
    }
    vhuff
}

#[cfg(test)]
mod test {
    use super::*;

    use env_logger;
    use rand::{self, Rng, RngCore};

    fn end_to_end_test(uncompressed_len: usize) {
        let mut rng = rand::thread_rng();
        info!("uncompressed length: {}", uncompressed_len);
        let mut uncompressed = Vec::<u8>::with_capacity(uncompressed_len);
        uncompressed.resize(uncompressed_len, 0);
        rng.fill_bytes(&mut uncompressed);
        info!("uncompressed : {:?}", uncompressed);
        let mut hasher = Digest::new(IEEE);
        hasher.write(&uncompressed);
        let crc = hasher.sum32();
        let mut compressed = Vec::<u8>::new();
        {
            let mut reader = BufReader::new(&uncompressed as &[u8]);
            let mut writer = BufWriter::new(&mut compressed);
            let (compressed_len, ccrc) = deflate(&mut reader, &mut writer).unwrap();
            debug!("compressed {} {}", compressed_len, ccrc);
            let _ = writer.flush();
        }
        if !compressed.is_empty() {
            let mut reader = BufReader::new(&compressed as &[u8]);
            let mut decompressed = Vec::<u8>::new();
            let mut writer = BufWriter::new(&mut decompressed);
            let (decompressed_len, dcrc) = inflate(&mut reader, &mut writer).unwrap();
            assert_eq!(uncompressed_len, decompressed_len as usize);
            assert_eq!(crc, dcrc);
        }
    }

    #[test]
    fn huffman_short() {
        for uncompressed_len in 0..(MIN_LEN + 1) {
            end_to_end_test(uncompressed_len);
        }
    }

    #[test]
    fn huffman_long() {
        for uncompressed_len in (MIN_LEN + 1)..(u16::MAX as usize) {
            end_to_end_test(uncompressed_len);
        }
    }

    #[test]
    fn codelen_alphabet() {
        env_logger::init();
        let len = rand::random::<u16>() as usize;
        let mut v = Vec::with_capacity(len);
        v.resize(len, 0);
        let mut rng = rand::thread_rng();
        for i in 0..len {
            v[i] = rng.gen_range(0, 16); //[0,16)
        }
        debug!("{:?}", v);
        let clens = encode_code_lengths(&v);
        debug!("{:?}", clens);
        let mut d = Vec::<u8>::with_capacity(len);
        for cl in clens {
            match cl {
                CodeLength::Single(c) => d.push(c),
                CodeLength::Repeat { code: l, repeat: r } => {
                    if l == 16 {
                        let c = d.pop().unwrap();
                        d.push(c);
                        for _ in 0..(r + 3) {
                            d.push(c);
                        }
                    } else {
                        let rep = if l == 17 { r + 3 } else { r + 11 };
                        for _ in 0..rep {
                            d.push(0);
                        }
                    }
                }
            }
        }
        debug!("{:?}", d);
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
            v[i] = rng.gen_range(0, 16); //[0,16)
        }
        let eclens = encode_code_lengths(&v);
        let mut freq = Vec::<usize>::with_capacity(HCLEN_ORDER.len());
        freq.resize(HCLEN_ORDER.len(), 0);
        update_freq(&mut freq, &eclens);
        let clens = assign_lengths(&freq);
        let mapped_clens = reordered_code_lengths(&clens);
        let hclen = mapped_clens.len();
        let mut writer = BitWriter::new();
        let mut encoded = Vec::new();
        {
            encoded.extend(writer.write_bits(hclen as u16 - 4, 4).iter());
            debug!("{:?}", clens);
            for i in 0..hclen {
                encoded.extend(writer.write_bits(mapped_clens[i] as u16, 3).iter());
                debug!("{}->{}", HCLEN_ORDER[i], mapped_clens[i]);
            }
            let enc = gen_huffman_enc(&clens);
            encoded.extend(write_code_lengths(&mut writer, &eclens, &enc));
            encoded.extend(writer.flush());
        }
        let mut input = BufReader::new(&encoded as &[u8]);
        let mut reader = BitReader::new(&mut input);
        let hclen = reader.read_bits(4, true).unwrap() as usize + 4;
        let max_hclen = HCLEN_ORDER.len();
        let mut hclen_len = Vec::<u8>::with_capacity(max_hclen);
        hclen_len.resize(max_hclen, 0);
        for i in 0..hclen {
            hclen_len[HCLEN_ORDER[i]] = reader.read_bits(3, true).unwrap() as u8;
            debug!("{}->{}", i, hclen_len[HCLEN_ORDER[i]]);
        }
        debug!("{:?}", hclen_len);
        let clen_dec = gen_huffman_dec(&hclen_len, max_hclen as u16);
        debug!("{:?}", clen_dec);
        let hlit_len = read_code_lengths(&mut reader, &clen_dec, len).unwrap();
        assert_eq!(v.len(), hlit_len.len());
        assert_eq!(v, hlit_len);
    }
}
