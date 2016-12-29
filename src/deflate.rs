use std::iter::FromIterator;

use bitstream::*;
use huffman::*;

pub fn inflate(reader: &mut BitReader) -> Vec<u8> {
    let mut v = Vec::<u8>::new();
    loop {
        let lit = read_fixed_literal(reader);
        if lit < 256 {
            v.push(lit as u8);
            //print!("{}", lit as u8 as char);
        } else if lit == 256 {
            break;
        } else {
            assert!(lit <= 285);
            let len = read_fixed_length(lit, reader) as usize;
            let dist = read_fixed_distance(reader) as usize;
            //print!("({}, {})", dist, len);
            let seg = Vec::from_iter(v[v.len()-1 - dist ..
                                       (v.len()-1 - dist + len)]
                                     .iter().cloned());
            v.extend_from_slice(&seg);
        }
    }
    v
}
