use super::{SyscallError, SyscallResult, read_user_string};
use crate::serial_println;
/// Syscall implementations — KnoxOS AI system calls
/// ai_load_model, ai_infer, ai_query
use alloc::string::String;

pub fn sys_ai_load_model(path: u64, _size: u64) -> SyscallResult {
    let p = unsafe { read_user_string(path) }.unwrap_or_else(|| String::from("default"));
    serial_println!("[KnoxOS AI] Loading model: {}", p);
    let model_id = crate::ai::load_model(&p);
    Ok(model_id)
}

pub fn sys_ai_infer(handle: u64, input: u64, output: u64) -> SyscallResult {
    serial_println!("[KnoxOS AI] Inference on handle {}", handle);
    let input_data = unsafe {
        let len = 64;
        core::slice::from_raw_parts(input as *const f32, len)
    };
    match crate::ai::infer(handle, input_data) {
        Ok(result) => {
            let out_ptr = output as *mut f32;
            for (i, &val) in result.iter().enumerate() {
                unsafe {
                    out_ptr.add(i).write(val);
                }
            }
            Ok(result.len() as u64)
        }
        Err(_) => Err(SyscallError::InvalidArgument),
    }
}

pub fn sys_ai_query(prompt_ptr: u64, prompt_len: u64, response_ptr: u64) -> SyscallResult {
    let prompt = unsafe {
        let b = core::slice::from_raw_parts(prompt_ptr as *const u8, prompt_len as usize);
        core::str::from_utf8(b).map_err(|_| SyscallError::InvalidArgument)?
    };
    serial_println!("[KnoxOS AI] Query: {}", prompt);
    let response = crate::ai::query(prompt);
    let resp_bytes = response.as_bytes();
    unsafe {
        core::ptr::copy_nonoverlapping(
            resp_bytes.as_ptr(),
            response_ptr as *mut u8,
            resp_bytes.len(),
        );
    }
    unsafe {
        *(response_ptr as *mut u8).add(resp_bytes.len()) = 0;
    }
    Ok(resp_bytes.len() as u64)
}
