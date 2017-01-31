extern crate crc;
#[macro_use]
extern crate log;
extern crate num;
#[macro_use]
extern crate num_derive;

mod bitstream;
pub mod deflate;
pub mod gzip;
pub mod huffman;
pub mod zip;
mod util;

