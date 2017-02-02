use std::fmt;
use std::fs::File;
use std::io::{BufReader, BufWriter, Error, ErrorKind};
use std::io::prelude::*;

use crc::crc32::checksum_ieee;
use num::FromPrimitive;

use deflate::*;
use util::*;

struct Flags {
    ftext: bool,
    fhcrc: bool,
    fextra: bool,
    fname: bool,
    fcomment: bool,
}

#[repr(u8)]
#[derive(FromPrimitive)]
enum ExtraFlags {
    Ignored = 0,
    Maximum = 2,
    Fastest = 4,
}

#[repr(u8)]
#[derive(FromPrimitive)]
enum OS {
    FAT = 0,
    Amiga = 1,
    VMS = 2,
    UNIX = 3,
    VMCMS = 4,
    AtariTOS = 5,
    HPFS = 6,
    Macintosh = 7,
    ZSystem = 8,
    CPM = 9,
    TOPS20 = 10,
    NTFS = 11,
    QDOS = 12,
    AcornRISCOS = 13,
    Unknown = 255,
}

impl fmt::Display for OS {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            OS::FAT => write!(f, "FAT"),
            OS::Amiga => write!(f, "Amiga"),
            OS::VMS => write!(f, "VMS"),
            OS::UNIX => write!(f, "UNIX"),
            OS::VMCMS => write!(f, "VM/CMS"),
            OS::AtariTOS => write!(f, "Atari TOS"),
            OS::HPFS => write!(f, "HPFS"),
            OS::Macintosh => write!(f, "Macintosh"),
            OS::ZSystem => write!(f, "Z-System"),
            OS::CPM => write!(f, "CP/M"),
            OS::TOPS20 => write!(f, "TOPS-20"),
            OS::NTFS => write!(f, "NTFS"),
            OS::QDOS => write!(f, "QDOS"),
            OS::AcornRISCOS => write!(f, "Acron RISCOS"),
            _ => write!(f, "Unknown"),
        }
    }
}

#[allow(dead_code)]
struct GzipMember {
    flg: Flags,
    xfl: ExtraFlags,
    mtime: u32,
    os: OS,
    crc16: u16,
    crc32: u32,
    isize: u32,
}

pub fn parse(file_name: &str) -> Result<(), Error> {
    let file = try!(File::open(file_name));
    let mut reader = BufReader::new(file);
    let mut byte: [u8; 1] = [0; 1];
    let mut word: [u8; 2] = [0; 2];
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
    let mtime = trans32(dword);
    try!(reader.read_exact(&mut byte));
    let xfl = match ExtraFlags::from_u8(byte[0]) {
        Some(x) => x,
        None => return Err(Error::new(ErrorKind::Other, "Bad XFL")),
    };

    try!(reader.read_exact(&mut byte));
    let os = match OS::from_u8(byte[0]) {
        Some(x) => x,
        None => return Err(Error::new(ErrorKind::Other, "Bad XFL")),
    };
    let mut extra = Vec::<u8>::new();
    if flg.fextra {
        try!(reader.read_exact(&mut word));
        let xlen = trans16(word);
        extra.resize(xlen as usize, 0);
        try!(reader.read_exact(&mut extra as &mut [u8]));
    }
    let mut file_name = Vec::<u8>::new();
    if flg.fname {
        try!(reader.read_until(0, &mut file_name));
    }
    debug!("File name: {:?}", String::from_utf8(file_name));
    let mut file_comment = Vec::<u8>::new();
    if flg.fcomment {
        try!(reader.read_until(0, &mut file_comment));
        debug!("File comment: {:?}", String::from_utf8(file_comment));
    }
    let mut crc16: u16 = 0;
    if flg.fhcrc {
        try!(reader.read_exact(&mut word));
        crc16 = trans16(word);
    }
    let out = Vec::<u8>::new();
    let mut writer = BufWriter::new(out);
    let ret = try!(inflate(&mut reader, &mut writer));
    try!(reader.read_exact(&mut dword));
    let out = match writer.into_inner() {
        Ok(x) => x,
        Err(_) => return Err(Error::new(ErrorKind::Other, "Can't get the inner output")),
    };
    let crc32: u32 = trans32(dword);
    try!(reader.read_exact(&mut dword));
    let isize: u32 = trans32(dword);
    assert_eq!(ret, isize as usize);
    debug!("{:08x} {:08x}", checksum_ieee(&out), crc32);
    assert_eq!(checksum_ieee(&out), crc32);
    debug!("{:?}", String::from_utf8(out));

    let _ = GzipMember { flg: flg, mtime: mtime, xfl: xfl, os: os, crc16: crc16, crc32: crc32, isize: isize };
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn basic() {
        assert!(parse("Cargo.toml.gz").is_ok());
    }
}
