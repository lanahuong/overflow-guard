use std::env;
use std::ffi::OsString;
use std::os::unix::ffi::OsStrExt;

fn main() {
    let mut args: Vec<OsString> = env::args_os().into_iter().collect();
    let first_arg: OsString = args.remove(1);
    let input_bytes: &[u8] = first_arg.as_bytes();
    let mut buffer: [u8; 16] = [0; 16];

    unsafe { std::ptr::copy(input_bytes.as_ptr(), buffer.as_mut_ptr(), input_bytes.len()) }

    println!("{}", std::str::from_utf8(&buffer).unwrap())
}
