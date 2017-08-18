use std::fmt;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Error, ErrorKind};
use std::io::SeekFrom::{Current, Start};
use std::io::prelude::*;
use std::str;
use std::string::String;
use std::vec::Vec;

use crc::crc32::{Digest, Hasher32, IEEE};
use num::FromPrimitive;

use deflate::*;
use util::*;

#[repr(u32)]
#[derive(FromPrimitive)]
enum Signature {
    LFH = 0x0403_4b50,
    AED = 0x0806_4b50,
    CFH = 0x0201_4b50,
    DS = 0x0505_4b50,
    ECDR64 = 0x0606_4b50,
    ECDL64 = 0x0706_4b50,
    ECDR = 0x0605_4b50,
}

#[repr(u8)]
#[derive(FromPrimitive)]
enum Compat {
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

impl fmt::Display for Compat {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Compat::FAT => write!(f, "FAT/VFAT/FAT32"),
            Compat::Amiga => write!(f, "Amiga"),
            Compat::OpenVMS => write!(f, "OpenVMS"),
            Compat::UNIX => write!(f, "UNIX"),
            Compat::VMCMS => write!(f, "VM/CMS"),
            Compat::AtariST => write!(f, "Atari ST"),
            Compat::HPFS => write!(f, "OS/2 HPFS"),
            Compat::Macintosh => write!(f, "Macintosh"),
            Compat::ZSystem => write!(f, "Z-System"),
            Compat::CPM => write!(f, "CP/M"),
            Compat::NTFS => write!(f, "Windows NTFS"),
            Compat::MVS => write!(f, "MVS (OS/390 -Z/OS)"),
            Compat::VSE => write!(f, "VSE"),
            Compat::AcornRisc => write!(f, "Acron Risc"),
            Compat::VFAT => write!(f, "VFAT"),
            Compat::AlternateMVS => write!(f, "alterate MVS"),
            Compat::BeOS => write!(f, "BeOS"),
            Compat::Tandem => write!(f, "Tandem"),
            Compat::OS400 => write!(f, "OS400"),
            Compat::OSX => write!(f, "OSX"),
        }
    }
}

struct Version {
    compatibility: Compat,
    major: u8,
    minor: u8,
}

impl Version {
    pub fn from_word(a: &[u8; 2]) -> Option<Version> {
        Compat::from_u8(a[1]).map(|x|
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
enum CompMethod {
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
    TerseOld = 10,
    Reserved11 = 11,
    BZIP2 = 12,
    Reserved13 = 13,
    LZMA = 14,
    Reserved15 = 15,
    Reserved16 = 16,
    Reserved17 = 17,
    TerseNew = 18,
    LZ77z = 19,
    WavPack = 97,
    PPMd = 98,
}

impl fmt::Display for CompMethod {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            CompMethod::Store => write!(f, "Store"),
            CompMethod::Shrink => write!(f, "Shrink"),
            CompMethod::ReduceFactor1 =>
                write!(f, "Reduce with Compression Factor 1"),
            CompMethod::ReduceFactor2 =>
                write!(f, "Reduce with Compression Factor 2"),
            CompMethod::ReduceFactor3 =>
                write!(f, "Reduce with Compression Factor 3"),
            CompMethod::ReduceFactor4 =>
                write!(f, "Reduce with Compression Factor 4"),
            CompMethod::Implode => write!(f, "Implode"),
            CompMethod::Tokenize => write!(f, "Tokenize"),
            CompMethod::Deflate => write!(f, "Deflate"),
            CompMethod::Deflate64 => write!(f, "Deflate64"),
            CompMethod::TerseOld => write!(f, "IBM TERSE (old)"),
            CompMethod::Reserved11 => write!(f, "Reserved11"),
            CompMethod::BZIP2 => write!(f, "BZIP2"),
            CompMethod::Reserved13 => write!(f, "Reserved13"),
            CompMethod::LZMA => write!(f, "LZMA"),
            CompMethod::Reserved15 => write!(f, "Reserved15"),
            CompMethod::Reserved16 => write!(f, "Reserved16"),
            CompMethod::Reserved17 => write!(f, "Reserved17"),
            CompMethod::TerseNew => write!(f, "IBM TERSE (new)"),
            CompMethod::LZ77z => write!(f, "IBM LZ77 z Architecture"),
            CompMethod::WavPack => write!(f, "WavPack"),
            CompMethod::PPMd => write!(f, "PPMd"),
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
enum CompOption {
    Implode {
        dictionary_size: bool,
        trees: bool,
    },
    Deflate(DeflateOption),
    LZMA(bool),
}

impl CompOption {
    fn new(a: u8, method: &CompMethod) -> Option<CompOption> {
        match *method {
            CompMethod::Implode => Some(CompOption::Implode{
                dictionary_size: a & 2 == 2, trees: a & 1 == 1 }),
            CompMethod::Deflate | CompMethod::Deflate64 =>
                DeflateOption::from_u8(a).map(CompOption::Deflate),
            CompMethod::LZMA => Some(
                CompOption::LZMA(a & 1 == 1)),
            _ => None
        }
    }
}

impl fmt::Display for CompOption {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            CompOption::Implode { dictionary_size, trees } =>
                write!(f, "{}-{}", dictionary_size, trees),
            CompOption::Deflate(ref option) => write!(f, "{}", option),
            CompOption::LZMA(option) => write!(f, "{}", option),
        }
    }
}

struct GPBF {
    encrypted: bool,
    compression_option: Option<CompOption>,
    crc: bool,
    enhanced_deflating: bool,
    patched_data: bool,
    strong_encryption: bool,
    utf8: bool,
    enhanced_compression: bool,
    masked: bool,
}

impl GPBF {
    fn new(a: &[u8], method: &CompMethod) -> GPBF {
        let option = CompOption::new(a[0] >> 1, method);
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
pub struct LocalFileHeader {
    file_name: String,
    version_needed_to_extract: Version,
    general_purpose_bit_flag: GPBF,
    compression_method: CompMethod,
    compressed_size: u32,
    uncompressed_size: u32,
    crc: u32,
    last_mod_file_time: u16,
    last_mod_file_date: u16,
    file_name_length: u16,
    extra_field_length: u16,
    offset: u64
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

const LFH_SIZE: usize = 2 * 5 + 4 * 3 + 2 * 2;

fn read_lfh(a: [u8; LFH_SIZE]) -> Result<LocalFileHeader, Error> {
    let mut reader = BufReader::new(&a[..]);
    let mut word: [u8; 2] = [0; 2];
    let mut dword: [u8; 4] = [0; 4];
    try!(reader.read_exact(&mut word));
    let version = match Version::from_word(&word) {
        Some(x) => x,
        None => return Err(Error::new(ErrorKind::Other, "Bad version in LFH"))
    };
    let _ = reader.read_exact(&mut word);
    let tmp = word;
    let _ = reader.read_exact(&mut word);
    let method = match CompMethod::from_u16(trans16(word)) {
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
    Ok(LocalFileHeader {
        file_name: String::new(),
        version_needed_to_extract: version,
        general_purpose_bit_flag: gpbf,
        compression_method: method,
        compressed_size: compressed_size,
        uncompressed_size: uncompressed_size,
        crc: crc,
        last_mod_file_time: time,
        last_mod_file_date: date,
        file_name_length: file_name_length,
        extra_field_length: extra_field_length,
        offset: 0})
}

/// Parse a zip file
///
/// # Example
///
/// ```no_run
/// use zip::zip;
///
/// let v = zip::parse("my.zip");
/// assert!(v.is_ok());
/// ```
pub fn parse(file_name: &str) -> Result<Vec<LocalFileHeader>, Error> {
    let file = try!(File::open(file_name));
    let mut reader = BufReader::new(file);
    let mut word: [u8; 2] = [0; 2];
    let mut dword: [u8; 4] = [0; 4];
    let mut qword: [u8; 8] = [0; 8];
    let mut lfh_array: [u8; LFH_SIZE] = [0; LFH_SIZE];
    let mut lfh_counter = 0;
    let mut cfh_counter = 0;
    let mut lfhs = Vec::<LocalFileHeader>::new();
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
                lfh.offset = try!(reader.seek(Current(0)));
                try!(reader.seek(Current(lfh.compressed_size as i64)));
                debug!("{}", lfh);
                lfhs.push(lfh);
            }
            Some(Signature::CFH) => {
                cfh_counter += 1;
                debug!("central file header {}", cfh_counter);
                try!(reader.read_exact(&mut word));
                let version_made_by = match Version::from_word(&word) {
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
    Ok(lfhs)
}

pub fn extract(file_name: &str, lfh: &LocalFileHeader) -> Result<(), Error> {
    debug!("{}", file_name);
    let file = try!(File::open(file_name));
    let mut reader = BufReader::new(file);
    try!(reader.seek(Start(lfh.offset)));
    debug!("{}", lfh.file_name);
    if lfh.file_name.ends_with('/') {
        debug!("Directory");
        try!(fs::create_dir_all(&lfh.file_name));
        return Ok(());
    }
    debug!("File");
    let out = try!(File::create(&lfh.file_name));
    let mut writer = BufWriter::new(out);
    match lfh.compression_method {
        CompMethod::Store => {
            let mut out = Vec::<u8>::new();
            out.resize(64 * 1024, 0);
            let mut copied = 0;
            let mut hasher = Digest::new(IEEE);
            while copied < lfh.uncompressed_size {
                let to_copy = (lfh.uncompressed_size - copied) as usize;
                if to_copy < out.len() {
                    out.resize(to_copy, 0);
                }
                try!(reader.read_exact(&mut out));
                try!(writer.write_all(&out));
                copied += out.len() as u32;
                hasher.write(&out);
            }
            assert_eq!(hasher.sum32(), lfh.crc);
        }
        CompMethod::Deflate => {
            let (decompressed_size, checksum) = try!(inflate(&mut reader, &mut writer));
            assert_eq!(decompressed_size, lfh.uncompressed_size);
            assert_eq!(checksum, lfh.crc);
        }
        _ => return Err(Error::new(ErrorKind::Other, "Unsupported compression method")),
    }
    try!(writer.flush());
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn store() {
        assert!(parse("test/store.zip").is_ok());
    }

    #[test]
    fn fixed_huffman() {
        assert!(parse("test/fixed_huffman.zip").is_ok());
    }

    #[test]
    fn dynamic_huffman() {
        assert!(parse("test/dynamic_huffman.zip").is_ok());
    }
}

