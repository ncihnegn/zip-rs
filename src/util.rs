use std::mem::transmute;

pub fn trans16(a: [u8; 2]) -> u16 {
    //((a[1] as u16) << 8) | (a[0] as u16)
    unsafe { transmute(a) }
}

pub fn trans32(a: [u8; 4]) -> u32 {
    //((a[3] as u32) << 24) | ((a[2] as u32) << 16) | ((a[1] as u32) << 8) |
    //(a[0] as u32)
    unsafe { transmute(a) }
}

pub fn trans64(a: [u8; 8]) -> u64 {
    //((a[7] as u64) << 56) | ((a[6] as u64) << 48) | ((a[5] as u64) << 40) |
    //((a[4] as u64) << 32) | ((a[3] as u64) << 24) | ((a[2] as u64) << 16) |
    //((a[1] as u64) << 8) | (a[0] as u64)
    unsafe { transmute(a) }
}

pub fn to_hex_string(bytes: &Vec<u8>) -> String {
    let strs: Vec<String> = bytes.iter().map(|b| format!("{:02x}", b)).collect();
    strs.join(" ")
}
