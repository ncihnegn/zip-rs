use std::fs::File;
use std::io::{self, BufReader};
use std::io::prelude::*;

use num::FromPrimitive;

//use bitstream::*;

struct Flags {
    ftext: bool,
    fhcrc: bool,
    fextra: bool,
    fname: bool,
    fcomment: bool,
}

#[allow(dead_code)]
#[repr(u8)]
#[derive(FromPrimitive)]
enum ExtraFlags {
    MAXIMUM = 2,
    Fastest = 4,
}

pub fn parse(file_name: &str) -> Result<(), io::Error> {
    let file = try!(File::open(file_name));
    let mut reader = BufReader::new(file);
    let mut byte: [u8; 1] = [0; 1];
    let mut dword: [u8; 4] = [0; 4];
    try!(reader.read_exact(&mut byte));
    assert!(byte[0] == 0x1F);
    try!(reader.read_exact(&mut byte));
    assert!(byte[0] == 0x8B);
    try!(reader.read_exact(&mut byte));
    assert!(byte[0] == 8);//Deflate Only
    try!(reader.read_exact(&mut byte));
    let mut flg = Flags { ftext: false, fhcrc: false, fextra: false, fname: false, fcomment: false };
    if byte[0] & 1 == 1 {
        flg.ftext = true;
    }
    if byte[0] & 2 == 2 {
        flg.fhcrc = true;
    }
    if byte[0] & 4 == 4 {
        flg.fextra = true;
    }
    if byte[0] & 8 == 8 {
        flg.fname = true;
    }
    if byte[0] & 16 == 16 {
        flg.fcomment = true;
    }
    try!(reader.read_exact(&mut dword));
    let mtime = dword;
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn basic() {
        let _ = parse("Cargo.zip.gz");
    }
}
