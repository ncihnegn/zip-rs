use bitstream::*;

type Literal = u16;
type Length = u16;
type Distance = u16;

pub fn read_fixed_literal(reader: &mut BitReader) -> Literal {
    let mut lit = reader.read_bits(7, false).unwrap();
    //println!("0b{:07b}", lit);
    if lit <= 0b0010111 {
        lit += 256;
    } else {
        let b = reader.read_bits(1, false).unwrap();
        lit <<= 1;
        lit |= b;
        //println!("0b{:08b}", lit);
        if lit <= 0b10111111 {
            lit -= 0b00110000;
        } else if lit <= 0b11000111 {
            lit -= 0b11000000;
            lit += 280;
        } else {
            let b = reader.read_bits(1, false).unwrap();
            lit <<= 1;
            lit |= b;
            //println!("0b{:09b}", lit);
            lit -= 0b110010000;
            lit += 144;
        }
    }
    lit
}

pub fn read_fixed_length(lit: Literal, reader: &mut BitReader) -> Length {
    let mut len = lit - 257;
    if len < 8 {
        len += 3;
    } else {
        let s = (len - 8) / 4;
        len = 10 + ((1 << (s + 1)) - 2) * 4 + ((len - 8) % 4) * (1 << s);
        len += reader.read_bits(s as u8, false).unwrap();
    }
    len
}

pub fn read_fixed_distance(reader: &mut BitReader) -> Distance {
    let mut dist = reader.read_bits(5, false).unwrap();
    assert!(dist < 30);
    if dist > 3 {
        let extra_bits = (dist - 2) / 2;
        let extra = reader.read_bits(extra_bits as u8, true).unwrap();
        dist = (1 << extra_bits) * (2 + (dist % 2)) + extra;
    } else {
        dist += 1;
    }
    dist
}

pub struct CodeLength {
    hlit: Vec<u8>,
    hdist: Vec<u8>,
}

pub fn gen_huffman_table(v: &Vec<u8>) -> Vec<Bits>{
    let mut bl_count = Vec::<Bits>::new();
    let max_bits = v.iter().max().unwrap().clone() as usize;
    bl_count.resize(max_bits+1, 0);
    for i in v {
        bl_count[*i as usize] += 1;
    }
    let mut next_code = Vec::<Bits>::with_capacity(max_bits+1);
    next_code.push(0);
    let mut code: Bits = 0;
    bl_count[0] = 0;
    for bits in 1..max_bits+1 {
        code = (code + bl_count[bits-1]) << 1;
        next_code.push(code);
    }
    let max_code = v.len()-1;
    let mut code_table = Vec::<Bits>::new();
    code_table.resize(max_code+1, 0);
    for n in 1..max_code+1 {
        let len = v[n] as usize;
        if len != 0 {
            code_table[n] = next_code[len];
            next_code[len] += 1;
        }
    }
    code_table
}

pub fn read_code_tree(reader: &mut BitReader) -> CodeLength {
    let hlit = reader.read_bits(5, false).unwrap();
    let hdist = reader.read_bits(5, false).unwrap();
    let hclen = reader.read_bits(5, false).unwrap();
    let mut hclen_len = Vec::<u8>::new();
    hclen_len.resize((hclen+4) as usize, 0);
    let seq = [16,17,18,0,8,7,9,6,10,5,11,4,12,3,13,2,14,1,15];
    for i in seq.iter() {
        hclen_len[*i] = reader.read_bits(3, false).unwrap() as u8;
    }
    let mut code_len = CodeLength { hlit: Vec::<u8>::new(),
                                    hdist: Vec::<u8>::new() };
    for i in 0..hlit as usize+257 {
        code_len.hlit[i] = reader.read_bits(3, false).unwrap() as u8;
    }
    code_len
}

