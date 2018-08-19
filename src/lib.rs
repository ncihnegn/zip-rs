extern crate crc;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
extern crate num;
#[macro_use]
extern crate num_derive;

mod bitstream;
mod constant;
pub mod deflate;
pub mod huffman;
#[macro_use]
mod util;
pub mod gzip;
pub mod zip;

#[cfg(test)]
extern crate env_logger;
#[cfg(test)]
extern crate rand;
