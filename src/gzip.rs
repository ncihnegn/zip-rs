use std::fmt;
use std::fs::File;
use std::io::{BufReader, BufWriter, Error, ErrorKind, Read, Seek, SeekFrom};
use std::io::prelude::*;
use std::str;

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
#[cfg_attr(feature = "cargo-clippy", allow(enum_variant_names))]
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
pub struct GzipMember {
    flg: Flags,
    xfl: ExtraFlags,
    mtime: u32,
    os: OS,
    crc16: u16,
    offset: u64,
    crc32: u32,
    isize: u32,
    file_name: String,
    file_comment: String,
}

pub fn parse(file_name: &str) -> Result<Vec<GzipMember>, Error> {
    let file = try!(File::open(file_name));
    let mut reader = BufReader::new(file);
    let mut byte: [u8; 1] = [0; 1];
    let mut word: [u8; 2] = [0; 2];
    let mut dword: [u8; 4] = [0; 4];
    let mut members = Vec::new();
    //let current = reader.seek(SeekFrom::Current(0)).unwrap();
    let end = reader.seek(SeekFrom::End(0)).unwrap();
    //assert_eq!(current, reader.seek(SeekFrom::Start(current)).unwrap());
    let _ = reader.seek(SeekFrom::Start(0));
    while reader.seek(SeekFrom::Current(0)).unwrap() != end {
        try!(reader.read_exact(&mut byte));
        assert_eq!(byte[0], 0x1F);
        try!(reader.read_exact(&mut byte));
        assert_eq!(byte[0], 0x8B);
        try!(reader.read_exact(&mut byte));
        assert_eq!(byte[0], 8);//Deflate Only
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
        let file_name = if flg.fname {
            let mut v = Vec::<u8>::new();
            try!(reader.read_until(0, &mut v));
            v.pop();//Remove trailing '\0'
            String::from_utf8(v).unwrap()
        } else {
            let mut tmp = String::from(file_name);
            if tmp.ends_with(".gz") {
                let len = tmp.len() - 3;
                tmp.truncate(len);
            }
            if !members.is_empty() {
                tmp += &format!(".{}", members.len());
            }
            tmp
        };
        debug!("File name: {}", file_name);
        let mut file_comment = String::new();
        if flg.fcomment {
            let mut v = Vec::<u8>::new();
            try!(reader.read_until(0, &mut v));
            v.pop();
            file_comment = String::from_utf8(v).unwrap();
            debug!("File comment: {}", file_comment);
        }
        let crc16: u16 = if flg.fhcrc {
            try!(reader.read_exact(&mut word));
            trans16(word) } else { 0 };
        let offset = reader.seek(SeekFrom::Current(0)).unwrap();
        let out = Vec::<u8>::new();
        let mut writer = BufWriter::new(out);
        let (decompressed_size, crc) = try!(inflate(&mut reader, &mut writer));
        try!(reader.read_exact(&mut dword));
        let out = match writer.into_inner() {
            Ok(x) => x,
            Err(_) => return Err(Error::new(ErrorKind::Other, "Can't get the inner output")),
        };
        let crc32: u32 = trans32(dword);
        try!(reader.read_exact(&mut dword));
        let isize: u32 = trans32(dword);
        debug!("{}({:08x}), expected {}({:08x})", decompressed_size, crc, isize, crc32);
        assert_eq!(decompressed_size, isize);
        assert_eq!(crc, crc32);
        debug!("\n{}", str::from_utf8(&out).unwrap());

        let mem = GzipMember { flg: flg, mtime: mtime, xfl: xfl, os: os, crc16: crc16, crc32: crc32, isize: isize, offset: offset, file_name: file_name, file_comment: file_comment };
        members.push(mem);
    }
    Ok(members)
}

pub fn extract(file_name: &str, member: &GzipMember) -> Result<(), Error> {
    let input = try!(File::open(file_name));
    let mut reader = BufReader::new(input);
    try!(reader.seek(SeekFrom::Start(member.offset)));
    let output = try!(File::create(&member.file_name));
    let mut writer = BufWriter::new(output);
    let (decompressed_size, crc) = try!(inflate(&mut reader, &mut writer));
    assert_eq!(decompressed_size, member.isize);
    assert_eq!(crc, member.crc32);
    try!(writer.flush());
    Ok(())
}


#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn basic() {
        let file_name = "test/dynamic_huffman.gz";
        assert!(parse(&file_name).is_ok());
    }

    #[test]
    fn multiple() {
        let file_name = "test/multiple.gz";
        assert!(parse(&file_name).is_ok());
    }
}
