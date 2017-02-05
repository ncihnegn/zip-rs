use std::env;

extern crate env_logger;
extern crate zip;
use zip::zip::*;

fn main() {
    env_logger::init().unwrap();
    let args: Vec<String> = env::args().collect();
    match args.len() {
        1 => {
            println!("Usage: unzip myfile.zip");
        }
        2 => {
            let file_name = args[1].as_str();
            let v = parse(&file_name).unwrap();
            for f in v {
                match extract(&file_name, &f) {
                    Ok(()) => println!("{} is extracted successfully", file_name),
                    Err(e) => println!("{:?}", e),
                }
            }
        }
        _ => {
            println!("Usage: unzip myfile.zip");
        }
    }
}

