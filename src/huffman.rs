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
        len += reader.read_bits(s as u8, true).unwrap();
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
