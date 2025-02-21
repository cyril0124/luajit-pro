use luajit_pro_helper::*;

use std::ffi::{CStr, CString};

const CARGO_PATH: &'static str = env!("CARGO_MANIFEST_DIR");

#[test]
fn test_lua() {
    let file_path = format!("{CARGO_PATH}/tests/main.lua");

    let ret_code = transform_lua(CString::new(file_path.as_str()).unwrap().as_ptr());
    let ret_code = unsafe {
        CStr::from_ptr(ret_code)
            .to_str()
            .unwrap_or("Not a valid UTF-8 string")
            .to_string()
    };

    println!("{}", ret_code);
}

#[test]
fn test_teal() {
    let file_path = format!("{CARGO_PATH}/tests/main.tl");

    let ret_code = transform_lua(CString::new(file_path.as_str()).unwrap().as_ptr());
    let ret_code = unsafe {
        CStr::from_ptr(ret_code)
            .to_str()
            .unwrap_or("Not a valid UTF-8 string")
            .to_string()
    };

    println!("{}", ret_code);
}
