macro_rules! trans_bytes {
    ($bytes:expr) => {
        unsafe { transmute($bytes) }
    };
}

pub fn to_hex_string(bytes: &[u8]) -> String {
    let strs: Vec<String> = bytes.iter().map(|b| format!("{:02x}", b)).collect();
    strs.join(" ")
}

pub fn trans24(a: &[u8]) -> usize {
    ((a[2] as usize) << 16) | ((a[1] as usize) << 8) | (a[0] as usize)
}
