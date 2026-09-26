use alloc::string::String;

/// Helper: read a null-terminated string from user pointer
pub unsafe fn read_user_string(ptr: u64) -> Option<String> {
    if ptr == 0 {
        return None;
    }
    let mut len = 0usize;
    let base = ptr as *const u8;
    while *base.add(len) != 0 && len < 4096 {
        len += 1;
    }
    let bytes = core::slice::from_raw_parts(base, len);
    core::str::from_utf8(bytes).ok().map(String::from)
}
