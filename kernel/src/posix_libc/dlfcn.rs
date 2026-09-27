// Dynamic loading (dlfcn.h) — bridges to dynlink.rs
use core::ffi::c_void;

/// dlopen — open a shared library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dlopen_c(filename: *const u8, flags: i32) -> *mut c_void {
    let name = if filename.is_null() {
        None
    } else {
        let mut len = 0;
        while *filename.add(len) != 0 {
            len += 1;
        }
        Some(core::str::from_utf8_unchecked(core::slice::from_raw_parts(
            filename, len,
        )))
    };
    let handle = crate::dynlink::dlopen(name, flags);
    if handle == 0 {
        core::ptr::null_mut()
    } else {
        handle as *mut c_void
    }
}

/// dlsym — look up a symbol
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dlsym_c(handle: *mut c_void, symbol: *const u8) -> *mut c_void {
    if symbol.is_null() {
        return core::ptr::null_mut();
    }
    let mut len = 0;
    while *symbol.add(len) != 0 {
        len += 1;
    }
    let sym_name = core::str::from_utf8_unchecked(core::slice::from_raw_parts(symbol, len));
    crate::dynlink::dlsym(handle as u64, sym_name)
}

/// dlclose — close a shared library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dlclose_c(handle: *mut c_void) -> i32 {
    crate::dynlink::dlclose(handle as u64)
}

/// dlerror — get error message
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dlerror_c() -> *const u8 {
    // Returns a pointer to a static error message
    static mut ERROR_BUF: [u8; 256] = [0; 256];
    match crate::dynlink::dlerror() {
        Some(msg) => {
            let bytes = msg.as_bytes();
            let len = bytes.len().min(255);
            let buf_ptr = core::ptr::addr_of_mut!(ERROR_BUF) as *mut u8;
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), buf_ptr, len);
            *buf_ptr.add(len) = 0;
            buf_ptr as *const u8
        }
        None => core::ptr::null(),
    }
}
