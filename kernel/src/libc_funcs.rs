//! C Standard Library Core Functions (libc compatibility)
//!
//! Implements fundamental C library functions needed for running
//! Linux ELF binaries. These functions use raw pointers and C-style
//! patterns intentionally for ABI compatibility.
#![allow(
    clippy::missing_safety_doc,
    clippy::not_unsafe_ptr_arg_deref,
    clippy::manual_memcpy,
    clippy::needless_range_loop,
    clippy::manual_pattern_char_comparison,
    clippy::len_without_is_empty,
    clippy::manual_is_multiple_of,
    clippy::manual_div_ceil,
    clippy::collapsible_if,
    clippy::single_match,
    clippy::needless_return,
    clippy::needless_bool_assign,
    clippy::clone_on_copy,
    clippy::get_first,
    clippy::explicit_auto_deref,
    clippy::single_char_add_str,
    clippy::is_digit_ascii_radix,
    clippy::needless_borrow,
    clippy::let_and_return,
    clippy::redundant_closure,
    clippy::if_same_then_else,
    clippy::excessive_precision,
    clippy::type_complexity,
    clippy::too_many_arguments,
    clippy::trim_split_whitespace,
    clippy::manual_map
)]

use core::ffi::c_void;

use crate::serial_println;

// ─── String Functions (string.h) ────────────────────────────────────

/// Copy n bytes from src to dest (non-overlapping)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memcpy(dest: *mut c_void, src: *const c_void, n: usize) -> *mut c_void {
    let d = dest as *mut u8;
    let s = src as *const u8;

    // Fast path: aligned 8-byte copies
    if n >= 8 && (d as usize) % 8 == 0 && (s as usize) % 8 == 0 {
        let qwords = n / 8;
        let d64 = d as *mut u64;
        let s64 = s as *const u64;
        for i in 0..qwords {
            *d64.add(i) = *s64.add(i);
        }
        let remainder = qwords * 8;
        for i in remainder..n {
            *d.add(i) = *s.add(i);
        }
    } else {
        for i in 0..n {
            *d.add(i) = *s.add(i);
        }
    }

    dest
}

/// Copy n bytes from src to dest (handles overlapping regions)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memmove(dest: *mut c_void, src: *const c_void, n: usize) -> *mut c_void {
    let d = dest as *mut u8;
    let s = src as *const u8;

    if d < s as *mut u8 || d >= s.add(n) as *mut u8 {
        // Non-overlapping or dest < src: copy forward
        for i in 0..n {
            *d.add(i) = *s.add(i);
        }
    } else {
        // Overlapping with dest > src: copy backward
        for i in (0..n).rev() {
            *d.add(i) = *s.add(i);
        }
    }

    dest
}

/// Fill n bytes of memory with constant byte c
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memset(dest: *mut c_void, c: i32, n: usize) -> *mut c_void {
    let d = dest as *mut u8;
    let byte = c as u8;

    // Fast path: aligned 8-byte fills
    if n >= 8 && (d as usize) % 8 == 0 {
        let fill: u64 = (byte as u64) * 0x0101010101010101;
        let qwords = n / 8;
        let d64 = d as *mut u64;
        for i in 0..qwords {
            *d64.add(i) = fill;
        }
        let remainder = qwords * 8;
        for i in remainder..n {
            *d.add(i) = byte;
        }
    } else {
        for i in 0..n {
            *d.add(i) = byte;
        }
    }

    dest
}

/// Compare n bytes of memory
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memcmp(s1: *const c_void, s2: *const c_void, n: usize) -> i32 {
    let a = s1 as *const u8;
    let b = s2 as *const u8;

    for i in 0..n {
        let diff = *a.add(i) as i32 - *b.add(i) as i32;
        if diff != 0 {
            return diff;
        }
    }
    0
}

/// Find byte c in n bytes of memory
#[unsafe(no_mangle)]
pub unsafe extern "C" fn memchr(s: *const c_void, c: i32, n: usize) -> *mut c_void {
    let p = s as *const u8;
    let byte = c as u8;

    for i in 0..n {
        if *p.add(i) == byte {
            return p.add(i) as *mut c_void;
        }
    }
    core::ptr::null_mut()
}

/// Calculate string length
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strlen(s: *const u8) -> usize {
    let mut len = 0;
    while *s.add(len) != 0 {
        len += 1;
    }
    len
}

/// Calculate string length with maximum
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strnlen(s: *const u8, maxlen: usize) -> usize {
    let mut len = 0;
    while len < maxlen && *s.add(len) != 0 {
        len += 1;
    }
    len
}

/// Copy string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strcpy(dest: *mut u8, src: *const u8) -> *mut u8 {
    let mut i = 0;
    loop {
        *dest.add(i) = *src.add(i);
        if *src.add(i) == 0 {
            break;
        }
        i += 1;
    }
    dest
}

/// Copy string with length limit
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strncpy(dest: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    let mut i = 0;
    while i < n && *src.add(i) != 0 {
        *dest.add(i) = *src.add(i);
        i += 1;
    }
    while i < n {
        *dest.add(i) = 0;
        i += 1;
    }
    dest
}

/// Compare two strings
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strcmp(s1: *const u8, s2: *const u8) -> i32 {
    let mut i = 0;
    loop {
        let a = *s1.add(i);
        let b = *s2.add(i);
        if a != b || a == 0 {
            return a as i32 - b as i32;
        }
        i += 1;
    }
}

/// Compare two strings with length limit
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strncmp(s1: *const u8, s2: *const u8, n: usize) -> i32 {
    for i in 0..n {
        let a = *s1.add(i);
        let b = *s2.add(i);
        if a != b || a == 0 {
            return a as i32 - b as i32;
        }
    }
    0
}

/// Concatenate strings
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strcat(dest: *mut u8, src: *const u8) -> *mut u8 {
    let dest_len = strlen(dest);
    strcpy(dest.add(dest_len), src);
    dest
}

/// Concatenate strings with length limit
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strncat(dest: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    let dest_len = strlen(dest);
    let mut i = 0;
    while i < n && *src.add(i) != 0 {
        *dest.add(dest_len + i) = *src.add(i);
        i += 1;
    }
    *dest.add(dest_len + i) = 0;
    dest
}

/// Find character in string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strchr(s: *const u8, c: i32) -> *mut u8 {
    let byte = c as u8;
    let mut i = 0;
    loop {
        if *s.add(i) == byte {
            return s.add(i) as *mut u8;
        }
        if *s.add(i) == 0 {
            return core::ptr::null_mut();
        }
        i += 1;
    }
}

/// Find last occurrence of character in string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strrchr(s: *const u8, c: i32) -> *mut u8 {
    let byte = c as u8;
    let len = strlen(s);
    let mut i = len;
    loop {
        if *s.add(i) == byte {
            return s.add(i) as *mut u8;
        }
        if i == 0 {
            return core::ptr::null_mut();
        }
        i -= 1;
    }
}

/// Find substring in string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strstr(haystack: *const u8, needle: *const u8) -> *mut u8 {
    if *needle == 0 {
        return haystack as *mut u8;
    }

    let needle_len = strlen(needle);
    let haystack_len = strlen(haystack);

    if needle_len > haystack_len {
        return core::ptr::null_mut();
    }

    for i in 0..=(haystack_len - needle_len) {
        if strncmp(haystack.add(i), needle, needle_len) == 0 {
            return haystack.add(i) as *mut u8;
        }
    }

    core::ptr::null_mut()
}

/// Span of characters in set
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strspn(s: *const u8, accept: *const u8) -> usize {
    let mut count = 0;
    while *s.add(count) != 0 {
        if strchr(accept, *s.add(count) as i32).is_null() {
            break;
        }
        count += 1;
    }
    count
}

/// Span of characters not in set
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strcspn(s: *const u8, reject: *const u8) -> usize {
    let mut count = 0;
    while *s.add(count) != 0 {
        if !strchr(reject, *s.add(count) as i32).is_null() {
            break;
        }
        count += 1;
    }
    count
}

/// Duplicate a string (allocates memory)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strdup(s: *const u8) -> *mut u8 {
    let len = strlen(s) + 1;
    let layout = alloc::alloc::Layout::from_size_align_unchecked(len, 1);
    let dest = alloc::alloc::alloc(layout);
    if !dest.is_null() {
        memcpy(dest as *mut c_void, s as *const c_void, len);
    }
    dest
}

// ─── ctype Functions (ctype.h) ──────────────────────────────────────

#[unsafe(no_mangle)]
pub extern "C" fn isalpha(c: i32) -> i32 {
    if (c >= b'a' as i32 && c <= b'z' as i32) || (c >= b'A' as i32 && c <= b'Z' as i32) {
        1
    } else {
        0
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn isdigit(c: i32) -> i32 {
    if c >= b'0' as i32 && c <= b'9' as i32 {
        1
    } else {
        0
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn isalnum(c: i32) -> i32 {
    if isalpha(c) != 0 || isdigit(c) != 0 {
        1
    } else {
        0
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn isspace(c: i32) -> i32 {
    if c == b' ' as i32
        || c == b'\t' as i32
        || c == b'\n' as i32
        || c == b'\r' as i32
        || c == 0x0B
        || c == 0x0C
    {
        1
    } else {
        0
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn isupper(c: i32) -> i32 {
    if c >= b'A' as i32 && c <= b'Z' as i32 {
        1
    } else {
        0
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn islower(c: i32) -> i32 {
    if c >= b'a' as i32 && c <= b'z' as i32 {
        1
    } else {
        0
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn toupper(c: i32) -> i32 {
    if islower(c) != 0 { c - 32 } else { c }
}

#[unsafe(no_mangle)]
pub extern "C" fn tolower(c: i32) -> i32 {
    if isupper(c) != 0 { c + 32 } else { c }
}

#[unsafe(no_mangle)]
pub extern "C" fn isprint(c: i32) -> i32 {
    if (0x20..=0x7E).contains(&c) { 1 } else { 0 }
}

#[unsafe(no_mangle)]
pub extern "C" fn isxdigit(c: i32) -> i32 {
    if isdigit(c) != 0
        || (c >= b'a' as i32 && c <= b'f' as i32)
        || (c >= b'A' as i32 && c <= b'F' as i32)
    {
        1
    } else {
        0
    }
}

// ─── stdlib Functions (stdlib.h) ────────────────────────────────────

/// Convert string to integer
#[unsafe(no_mangle)]
pub unsafe extern "C" fn atoi(s: *const u8) -> i32 {
    let mut i = 0;
    let mut sign = 1i32;
    let mut result = 0i32;

    // Skip whitespace
    while isspace(*s.add(i) as i32) != 0 {
        i += 1;
    }

    // Handle sign
    if *s.add(i) == b'-' {
        sign = -1;
        i += 1;
    } else if *s.add(i) == b'+' {
        i += 1;
    }

    // Parse digits
    while *s.add(i) >= b'0' && *s.add(i) <= b'9' {
        result = result * 10 + (*s.add(i) - b'0') as i32;
        i += 1;
    }

    sign * result
}

/// Convert string to long
#[unsafe(no_mangle)]
pub unsafe extern "C" fn atol(s: *const u8) -> i64 {
    let mut i = 0;
    let mut sign = 1i64;
    let mut result = 0i64;

    while isspace(*s.add(i) as i32) != 0 {
        i += 1;
    }

    if *s.add(i) == b'-' {
        sign = -1;
        i += 1;
    } else if *s.add(i) == b'+' {
        i += 1;
    }

    while *s.add(i) >= b'0' && *s.add(i) <= b'9' {
        result = result * 10 + (*s.add(i) - b'0') as i64;
        i += 1;
    }

    sign * result
}

/// Convert string to long with base and end pointer
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strtol(s: *const u8, endptr: *mut *mut u8, base: i32) -> i64 {
    let mut i = 0;
    let mut sign = 1i64;
    let mut result = 0i64;
    let mut actual_base = base;

    // Skip whitespace
    while isspace(*s.add(i) as i32) != 0 {
        i += 1;
    }

    // Handle sign
    if *s.add(i) == b'-' {
        sign = -1;
        i += 1;
    } else if *s.add(i) == b'+' {
        i += 1;
    }

    // Auto-detect base
    if actual_base == 0 {
        if *s.add(i) == b'0' {
            if *s.add(i + 1) == b'x' || *s.add(i + 1) == b'X' {
                actual_base = 16;
                i += 2;
            } else {
                actual_base = 8;
                i += 1;
            }
        } else {
            actual_base = 10;
        }
    } else if actual_base == 16
        && *s.add(i) == b'0'
        && (*s.add(i + 1) == b'x' || *s.add(i + 1) == b'X')
    {
        i += 2;
    }

    // Parse digits
    loop {
        let c = *s.add(i);
        let digit = if c.is_ascii_digit() {
            (c - b'0') as i32
        } else if c.is_ascii_lowercase() {
            (c - b'a' + 10) as i32
        } else if c.is_ascii_uppercase() {
            (c - b'A' + 10) as i32
        } else {
            break;
        };

        if digit >= actual_base {
            break;
        }

        result = result * actual_base as i64 + digit as i64;
        i += 1;
    }

    if !endptr.is_null() {
        *endptr = s.add(i) as *mut u8;
    }

    sign * result
}

/// Convert string to unsigned long
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strtoul(s: *const u8, endptr: *mut *mut u8, base: i32) -> u64 {
    strtol(s, endptr, base) as u64
}

/// Absolute value
#[unsafe(no_mangle)]
pub extern "C" fn abs(x: i32) -> i32 {
    if x < 0 { -x } else { x }
}

/// Long absolute value
#[unsafe(no_mangle)]
pub extern "C" fn labs(x: i64) -> i64 {
    if x < 0 { -x } else { x }
}

/// Allocate zeroed memory
#[unsafe(no_mangle)]
pub unsafe extern "C" fn calloc(nmemb: usize, size: usize) -> *mut c_void {
    let total = nmemb.saturating_mul(size);
    if total == 0 {
        return core::ptr::null_mut();
    }
    let layout = alloc::alloc::Layout::from_size_align_unchecked(total, 8);
    let ptr = alloc::alloc::alloc_zeroed(layout) as *mut c_void;
    ptr
}

/// Allocate memory
#[unsafe(no_mangle)]
pub unsafe extern "C" fn malloc(size: usize) -> *mut c_void {
    if size == 0 {
        return core::ptr::null_mut();
    }
    let layout = alloc::alloc::Layout::from_size_align_unchecked(size, 8);
    alloc::alloc::alloc(layout) as *mut c_void
}

/// Free memory
#[unsafe(no_mangle)]
pub unsafe extern "C" fn free(ptr: *mut c_void) {
    if !ptr.is_null() {
        // In a real implementation, we'd track the allocation size
        // For now, we use a minimal dealloc
        let layout = alloc::alloc::Layout::from_size_align_unchecked(1, 1);
        alloc::alloc::dealloc(ptr as *mut u8, layout);
    }
}

/// Reallocate memory
#[unsafe(no_mangle)]
pub unsafe extern "C" fn realloc(ptr: *mut c_void, size: usize) -> *mut c_void {
    if ptr.is_null() {
        return malloc(size);
    }
    if size == 0 {
        free(ptr);
        return core::ptr::null_mut();
    }
    // Simplified: allocate new, copy, free old
    let new_ptr = malloc(size);
    if !new_ptr.is_null() {
        memcpy(new_ptr, ptr, size);
        free(ptr);
    }
    new_ptr
}

// ─── stdio Functions (stdio.h) – kernel-level ──────────────────────

/// Write to file descriptor 1 (stdout → serial)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn puts(s: *const u8) -> i32 {
    let mut i = 0;
    while *s.add(i) != 0 {
        i += 1;
    }
    let slice = core::slice::from_raw_parts(s, i);
    if let Ok(str) = core::str::from_utf8(slice) {
        crate::serial_println!("{}", str);
    }
    i as i32 + 1
}

/// Write character to stdout
#[unsafe(no_mangle)]
pub extern "C" fn putchar(c: i32) -> i32 {
    // Use serial_print macro for output
    crate::serial_print!("{}", c as u8 as char);
    c
}

/// Write n characters from buf to fd
#[unsafe(no_mangle)]
pub unsafe extern "C" fn write_libc(fd: i32, buf: *const c_void, count: usize) -> isize {
    if fd == 1 || fd == 2 {
        // stdout or stderr → serial
        let bytes = core::slice::from_raw_parts(buf as *const u8, count);
        if let Ok(str) = core::str::from_utf8(bytes) {
            crate::serial_print!("{}", str);
        }
        count as isize
    } else {
        -1 // EBADF
    }
}

// ─── errno Support ──────────────────────────────────────────────────

static mut ERRNO_VALUE: i32 = 0;

/// Get pointer to errno
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __errno_location() -> *mut i32 {
    &raw mut ERRNO_VALUE
}

// ─── Program Control ────────────────────────────────────────────────

/// Exit the current process
#[unsafe(no_mangle)]
pub extern "C" fn exit(status: i32) -> ! {
    serial_println!("[libc] exit({})", status);
    // Kill current process via scheduler
    if let Some(pid) = crate::scheduler::current_pid() {
        crate::process::kill(pid);
    }
    loop {
        crate::arch_compat::instructions::interrupts::hlt();
    }
}

/// Abort the current process
#[unsafe(no_mangle)]
pub extern "C" fn abort() -> ! {
    serial_println!("[libc] abort()");
    if let Some(pid) = crate::scheduler::current_pid() {
        crate::process::kill(pid);
    }
    loop {
        crate::arch_compat::instructions::interrupts::hlt();
    }
}

// ─── Math Helpers ───────────────────────────────────────────────────

/// Integer division and remainder
#[repr(C)]
pub struct DivResult {
    pub quot: i32,
    pub rem: i32,
}

#[unsafe(no_mangle)]
pub extern "C" fn div(numer: i32, denom: i32) -> DivResult {
    DivResult {
        quot: numer / denom,
        rem: numer % denom,
    }
}

// ─── Environment ────────────────────────────────────────────────────

static mut ENV_EMPTY: [*const u8; 1] = [core::ptr::null()];

/// Get environment variable (stub)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getenv(_name: *const u8) -> *const u8 {
    core::ptr::null()
}

/// Get environ pointer
#[unsafe(no_mangle)]
pub unsafe extern "C" fn get_environ() -> *mut *const u8 {
    (&raw mut ENV_EMPTY) as *mut *const u8
}

// ─── Init ───────────────────────────────────────────────────────────

pub fn init() {
    serial_println!("[KnoxOS] C standard library functions initialized");
    serial_println!(
        "[KnoxOS]   string.h: memcpy, memmove, memset, memcmp, memchr, strlen, strcmp, strcpy, strcat, strstr, ..."
    );
    serial_println!(
        "[KnoxOS]   stdlib.h: atoi, strtol, malloc, calloc, realloc, free, exit, abort, ..."
    );
    serial_println!("[KnoxOS]   ctype.h: isalpha, isdigit, isspace, toupper, tolower, ...");
    serial_println!("[KnoxOS]   stdio.h: puts, putchar, write (kernel-level)");
}
