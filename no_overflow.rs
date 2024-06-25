use std::env;
use std::ffi::OsString;
use std::os::unix::ffi::OsStrExt;

fn main() {
    //let mut args: Vec<OsString> = env::args_os().into_iter().collect();
    //let first_arg: OsString = args.remove(1);
    //let input_bytes: &[u8] = first_arg.as_bytes();
    let input_bytes: &[u8] = "more than 16 characters".as_bytes();
    let mut buffer: [u8; 16] = [0; 16];
    
    unsafe {
        std::ptr::copy(
            input_bytes.as_ptr(),
            buffer.as_mut_ptr(),
            16,
        )
    }

    for c in buffer {
        print!("{}", c as char);
    }
}
