#![feature(proc_macro)]

extern crate num;
#[macro_use]
extern crate num_derive;

pub mod bitstream;
pub mod deflate;
pub mod huffman;
pub mod zip;

use zip::*;

fn main() {
    let _ = parse("fixed_huffman.zip");
}
