use std::collections::HashMap;
use std::fmt;
use std::fs::File;
use std::io::{BufReader, BufWriter, Error, ErrorKind};
use std::io::SeekFrom::{Current, Start};
use std::io::prelude::*;
use std::string::String;
use std::vec::Vec;

use crc::crc32::checksum_ieee;
use num::FromPrimitive;

use deflate::*;
use util::*;

#[repr(u32)]
#[derive(FromPrimitive)]
enum Signature {
    LFH = 0x04034b50,
    AED = 0x08064b50,
    CFH = 0x02014b50,
    DS = 0x05054b50,
    ECDR64 = 0x06064b50,
    ECDL64 = 0x07064b50,
    ECDR = 0x06054b50,
}

#[repr(u8)]
#[derive(FromPrimitive)]
enum Compatibility {
    FAT = 0,
    Amiga = 1,
    OpenVMS = 2,
    UNIX = 3,
    VMCMS = 4,
    AtariST = 5,
    HPFS = 6,
    Macintosh = 7,
    ZSystem = 8,
    CPM = 9,
    NTFS = 10,
    MVS = 11,
    VSE = 12,
    AcornRisc = 13,
    VFAT = 14,
    AlternateMVS = 15,
    BeOS = 16,
    Tandem = 17,
    OS400 = 18,
    OSX = 19,
}

impl fmt::Display for Compatibility {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Compatibility::FAT => write!(f, "FAT/VFAT/FAT32"),
            Compatibility::Amiga => write!(f, "Amiga"),
            Compatibility::OpenVMS => write!(f, "OpenVMS"),
            Compatibility::UNIX => write!(f, "UNIX"),
            Compatibility::VMCMS => write!(f, "VM/CMS"),
            Compatibility::AtariST => write!(f, "Atari ST"),
            Compatibility::HPFS => write!(f, "OS/2 HPFS"),
            Compatibility::Macintosh => write!(f, "Macintosh"),
            Compatibility::ZSystem => write!(f, "Z-System"),
            Compatibility::CPM => write!(f, "CP/M"),
            Compatibility::NTFS => write!(f, "Windows NTFS"),
            Compatibility::MVS => write!(f, "MVS (OS/390 -Z/OS)"),
            Compatibility::VSE => write!(f, "VSE"),
            Compatibility::AcornRisc => write!(f, "Acron Risc"),
            Compatibility::VFAT => write!(f, "VFAT"),
            Compatibility::AlternateMVS => write!(f, "alterate MVS"),
            Compatibility::BeOS => write!(f, "BeOS"),
            Compatibility::Tandem => write!(f, "Tandem"),
            Compatibility::OS400 => write!(f, "OS400"),
            Compatibility::OSX => write!(f, "OSX"),
        }
    }
}

struct Version {
    compatibility: Compatibility,
    major: u8,
    minor: u8,
}

impl Version {
    pub fn from(a: &[u8]) -> Option<Version> {
        Compatibility::from_u8(a[1]).map(|x|
            Version { compatibility: x, major: a[0] % (1 << 4),
                                 minor: a[0] >> 4 })
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}({}.{})", self.compatibility, self.major, self.minor)
    }
}

#[repr(u16)]
#[derive(FromPrimitive)]
enum CompressionMethod {
    Store = 0,
    Shrink = 1,
    ReduceFactor1 = 2,
    ReduceFactor2 = 3,
    ReduceFactor3 = 4,
    ReduceFactor4 = 5,
    Implode = 6,
    Tokenize = 7,
    Deflate = 8,
    Deflate64 = 9,
    TERSEOld = 10,
    Reserved11 = 11,
    BZIP2 = 12,
    Reserved13 = 13,
    LZMA = 14,
    Reserved15 = 15,
    Reserved16 = 16,
    Reserved17 = 17,
    TERSENew = 18,
    LZ77z = 19,
    WavPack = 97,
    PPMd = 98,
}

impl fmt::Display for CompressionMethod {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            CompressionMethod::Store => write!(f, "Store"),
            CompressionMethod::Shrink => write!(f, "Shrink"),
            CompressionMethod::ReduceFactor1 =>
                write!(f, "Reduce with Compression Factor 1"),
            CompressionMethod::ReduceFactor2 =>
                write!(f, "Reduce with Compression Factor 2"),
            CompressionMethod::ReduceFactor3 =>
                write!(f, "Reduce with Compression Factor 3"),
            CompressionMethod::ReduceFactor4 =>
                write!(f, "Reduce with Compression Factor 4"),
            CompressionMethod::Implode => write!(f, "Implode"),
            CompressionMethod::Tokenize => write!(f, "Tokenize"),
            CompressionMethod::Deflate => write!(f, "Deflate"),
            CompressionMethod::Deflate64 => write!(f, "Deflate64"),
            CompressionMethod::TERSEOld => write!(f, "IBM TERSE (old)"),
            CompressionMethod::Reserved11 => write!(f, "Reserved11"),
            CompressionMethod::BZIP2 => write!(f, "BZIP2"),
            CompressionMethod::Reserved13 => write!(f, "Reserved13"),
            CompressionMethod::LZMA => write!(f, "LZMA"),
            CompressionMethod::Reserved15 => write!(f, "Reserved15"),
            CompressionMethod::Reserved16 => write!(f, "Reserved16"),
            CompressionMethod::Reserved17 => write!(f, "Reserved17"),
            CompressionMethod::TERSENew => write!(f, "IBM TERSE (new)"),
            CompressionMethod::LZ77z => write!(f, "IBM LZ77 z Architecture"),
            CompressionMethod::WavPack => write!(f, "WavPack"),
            CompressionMethod::PPMd => write!(f, "PPMd"),
        }
    }
}

#[derive(Debug, FromPrimitive)]
enum DeflateOption {
    Normal = 0,
    Maximum = 1,
    Fast = 2,
    SuperFast = 3,
}

impl fmt::Display for DeflateOption {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            DeflateOption::Normal => write!(f, "Normal"),
            DeflateOption::Maximum => write!(f, "Maximum"),
            DeflateOption::Fast => write!(f, "Fast"),
            DeflateOption::SuperFast => write!(f, "SuperFast"),
        }
    }
}

#[derive(Debug)]
enum CompressionOption {
    Implode {
        dictionary_size: bool,
        trees: bool,
    },
    Deflate(DeflateOption),
    LZMA(bool),
}

impl CompressionOption {
    fn new(a: u8, method: &CompressionMethod) -> Option<CompressionOption> {
        match *method {
            CompressionMethod::Implode => Some(CompressionOption::Implode{
                dictionary_size: a & 2 == 2, trees: a & 1 == 1 }),
            CompressionMethod::Deflate | CompressionMethod::Deflate64 =>
                DeflateOption::from_u8(a).map(|x| CompressionOption::Deflate(x)),
            CompressionMethod::LZMA => Some(
                CompressionOption::LZMA(a & 1 == 1)),
            _ => None
        }
    }
}

impl fmt::Display for CompressionOption {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            CompressionOption::Implode { dictionary_size, trees } =>
                write!(f, "{}-{}", dictionary_size, trees),
            CompressionOption::Deflate(ref option) => write!(f, "{}", option),
            CompressionOption::LZMA(option) => write!(f, "{}", option),
        }
    }
}

struct GPBF {
    encrypted: bool,
    compression_option: Option<CompressionOption>,
    crc: bool,
    enhanced_deflating: bool,
    patched_data: bool,
    strong_encryption: bool,
    utf8: bool,
    enhanced_compression: bool,
    masked: bool,
}

impl GPBF {
    fn new(a: &[u8], method: &CompressionMethod) -> GPBF {
        let option = CompressionOption::new(a[0] >> 1, method);
        GPBF { encrypted: a[0] == 1, compression_option: option,
               crc: a[0] & (1 << 3) == 1 << 3,
               enhanced_deflating: a[0] & (1 << 4) == 1 << 4,
               patched_data: a[0] & (1 << 5) == 1 << 5,
               strong_encryption: a[0] & (1 << 6) == 1 << 6,
               utf8: a[1] & (1 << (11-8)) == 1 << (11-8),
               enhanced_compression: a[1] & (1 << (12-8)) == 1 << (12-8),
               masked: a[1] & (1 << (13-8)) == 1 << (13-8) }
    }
}

impl fmt::Display for GPBF {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "encrypted: {} {} {} {} {} {} {} {}", self.encrypted,
               self.crc, self.enhanced_deflating, self.patched_data,
               self.strong_encryption, self.utf8, self.enhanced_compression,
               self.masked)
    }
}

#[allow(dead_code)]
struct LocalFileHeader {
    file_name: String,
    version_needed_to_extract: Version,
    general_purpose_bit_flag: GPBF,
    compression_method: CompressionMethod,
    compressed_size: u32,
    uncompressed_size: u32,
    crc: u32,
    last_mod_file_time: u16,
    last_mod_file_date: u16,
    file_name_length: u16,
    extra_field_length: u16,
}

impl fmt::Display for LocalFileHeader {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} (0x{:08x}) {} {} {:?} {}->{}", self.file_name,
               self.crc, self.version_needed_to_extract,
               self.compression_method,
               self.general_purpose_bit_flag.compression_option,
               self.compressed_size, self.uncompressed_size)
    }
}

#[allow(dead_code)]
struct CentralFileHeader {
    lfh: LocalFileHeader,
    version_made_by: Version,
    disk_number_start: u16,
    internal_file_attributes: u16,
    external_file_attributes: u32,
    relative_offset_of_local_header: u32,
}

impl fmt::Display for CentralFileHeader {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} (0x{:08x}) {} {} {:?} {}->{}", self.lfh.file_name,
               self.lfh.crc, self.lfh.version_needed_to_extract,
               self.lfh.compression_method,
               self.lfh.general_purpose_bit_flag.compression_option,
               self.lfh.compressed_size, self.lfh.uncompressed_size)
    }
}

#[allow(dead_code)]
struct EndCentralDirectoryRecord {
    number_of_this_disk: u16,
    file_comment_length: u16,
    file_comment: String,
}

#[allow(dead_code)]
struct HuffmanCode {
    map: HashMap<u16, u8>,
}

const LFH_SIZE: usize = 2 * 5 + 4 * 3 + 2 * 2;

fn read_lfh(a: [u8; LFH_SIZE]) -> Result<LocalFileHeader, Error> {
    let mut reader = BufReader::new(&a[..]);
    let mut word: [u8; 2] = [0; 2];
    let mut dword: [u8; 4] = [0; 4];
    try!(reader.read_exact(&mut word));
    let version = match Version::from(&word) {
        Some(x) => x,
        None => return Err(Error::new(ErrorKind::Other, "Bad version in LFH"))
    };
    let _ = reader.read_exact(&mut word);
    let tmp = word.clone();
    let _ = reader.read_exact(&mut word);
    let method = match CompressionMethod::from_u16(trans16(word)) {
        Some(x) => x,
        None => return Err(Error::new(ErrorKind::Other, "Bad compression method in LFH"))
    };
    let gpbf = GPBF::new(&tmp, &method);
    let _ = reader.read_exact(&mut word);
    let time: u16 = trans16(word);
    let _ = reader.read_exact(&mut word);
    let date: u16 = trans16(word);
    let _ = reader.read_exact(&mut dword);
    let crc: u32 = trans32(dword);
    let _ = reader.read_exact(&mut dword);
    let compressed_size: u32 = trans32(dword);
    let _ = reader.read_exact(&mut dword);
    let uncompressed_size: u32 = trans32(dword);
    let _ = reader.read_exact(&mut word);
    let file_name_length: u16 = trans16(word);
    let _ = reader.read_exact(&mut word);
    let extra_field_length: u16 = trans16(word);
    Ok(LocalFileHeader { file_name: String::new(),
                      version_needed_to_extract: version,
                      general_purpose_bit_flag: gpbf,
                      compression_method: method,
                      compressed_size: compressed_size,
                      uncompressed_size: uncompressed_size,
                      crc: crc,
                      last_mod_file_time: time,
                      last_mod_file_date: date,
                      file_name_length: file_name_length,
                      extra_field_length: extra_field_length })
}

pub fn parse(file_name: &str) -> Result<(), Error> {
    let file = try!(File::open(file_name));
    let mut reader = BufReader::new(file);
    let mut word: [u8; 2] = [0; 2];
    let mut dword: [u8; 4] = [0; 4];
    let mut qword: [u8; 8] = [0; 8];
    let mut lfh_array: [u8; LFH_SIZE] = [0; LFH_SIZE];
    let mut lfh_counter = 0;
    let mut cfh_counter = 0;
    let mut lfhs: HashMap<u64, LocalFileHeader> = HashMap::new();
    while reader.read_exact(&mut dword).is_ok() {
        let signature = Signature::from_u32(trans32(dword));
        match signature {
            Some(Signature::LFH) => {
                lfh_counter += 1;
                debug!("local file header {} ", lfh_counter);
                try!(reader.read_exact(&mut lfh_array));
                let mut lfh = try!(read_lfh(lfh_array));
                let mut v = Vec::<u8>::new();
                v.resize(lfh.file_name_length as usize, 0);
                try!(reader.read_exact(&mut v as &mut [u8]));
                lfh.file_name = String::from_utf8(v).unwrap();
                try!(reader.seek(Current(lfh.extra_field_length as i64)));
                let position = try!(reader.seek(Current(0)));
                try!(reader.seek(Current(lfh.compressed_size as i64)));
                debug!("0x{:08x}", position);
                debug!("{}", lfh);
                lfhs.insert(position, lfh);
            }
            Some(Signature::CFH) => {
                cfh_counter += 1;
                debug!("central file header {}", cfh_counter);
                try!(reader.read_exact(&mut word));
                let version_made_by = match Version::from(&word) {
                    Some(x) => x,
                    None => return Err(Error::new(ErrorKind::Other, "Bad version made by"))
                };
                try!(reader.read_exact(&mut lfh_array));
                let mut lfh = try!(read_lfh(lfh_array));
                try!(reader.read_exact(&mut word));
                let file_comment_length: u16 = trans16(word);
                try!(reader.read_exact(&mut word));
                let disk_number = trans16(word);
                try!(reader.read_exact(&mut word));
                let internal = trans16(word);
                try!(reader.read_exact(&mut dword));
                let external = trans32(dword);
                try!(reader.read_exact(&mut dword));
                let offset = trans32(dword);
                let mut v = Vec::<u8>::new();
                v.resize(lfh.file_name_length as usize, 0);
                try!(reader.read_exact(&mut v as &mut [u8]));
                lfh.file_name = String::from_utf8(v).unwrap();
                try!(reader.seek(Current(lfh.extra_field_length as i64)));
                try!(reader.seek(Current(file_comment_length as i64)));
                let cfh = CentralFileHeader {
                    version_made_by: version_made_by,
                    disk_number_start: disk_number,
                    internal_file_attributes: internal,
                    external_file_attributes: external,
                    relative_offset_of_local_header: offset,
                    lfh: lfh };
                debug!("{}", cfh);
            }
            Some(Signature::ECDR64) => {
                debug!("Zip64 end of central directory record");
                try!(reader.read_exact(&mut qword));
                let size: u64 = trans64(qword);
                try!(reader.seek(Current(size as i64)));
            }
            Some(Signature::ECDL64) => {
                debug!("Zip64 end of central directory locator");
                try!(reader.seek(Current(4 + 8 + 4)));
            }
            Some(Signature::ECDR) => {
                debug!("end of central directory record");
                try!(reader.seek(Current(2 * 4 + 4 * 2)));
                try!(reader.read_exact(&mut word));
                let file_comment_length: u16 = trans16(word);
                try!(reader.seek(Current(file_comment_length as i64)));
            }
            _ => {
                return Err(Error::new(ErrorKind::Other, "Bad signature"));
            }
        }
    }

    for (position, lfh) in lfhs {
        try!(reader.seek(Start(position)));
        let out = Vec::<u8>::new();
        let mut writer = BufWriter::new(out);
        try!(inflate(&mut reader, &mut writer));
        let out = match writer.into_inner() {
            Ok(x) => x,
            Err(_) => return Err(Error::new(ErrorKind::Other, "Can't get the inner output")),
        };
        assert_eq!(checksum_ieee(&out), lfh.crc);
        debug!("{:?}", String::from_utf8(out));
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn fixed_huffman() {
        assert!(parse("fixed_huffman.zip").is_ok());
    }

    #[test]
    fn dynamic_huffman() {
        assert!(parse("dynamic_huffman.zip").is_ok());
    }
}

