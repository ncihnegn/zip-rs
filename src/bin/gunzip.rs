use std::env;

extern crate env_logger;
extern crate zip;
use zip::gzip::*;

fn main() {
    env_logger::init().unwrap();
    let args: Vec<String> = env::args().collect();
    match args.len() {
        1 => {
            println!("Usage: gunzip myfile.zip");
        }
        2 => {
            let _ = parse(args[1].as_str());
        }
        _ => {
            println!("Usage: gunzip myfile.zip");
        }
    }
}

