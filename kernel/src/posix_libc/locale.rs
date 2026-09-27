// Locale (locale.h)

/// struct lconv (simplified)
#[repr(C)]
pub struct Lconv {
    pub decimal_point: *const u8,
    pub thousands_sep: *const u8,
    pub grouping: *const u8,
    pub int_curr_symbol: *const u8,
    pub currency_symbol: *const u8,
    pub mon_decimal_point: *const u8,
    pub mon_thousands_sep: *const u8,
    pub mon_grouping: *const u8,
    pub positive_sign: *const u8,
    pub negative_sign: *const u8,
}

static DECIMAL_POINT: [u8; 2] = [b'.', 0];
static EMPTY_STRING: [u8; 1] = [0];
static NEGATIVE_SIGN: [u8; 2] = [b'-', 0];

static mut DEFAULT_LCONV: Lconv = Lconv {
    decimal_point: DECIMAL_POINT.as_ptr(),
    thousands_sep: EMPTY_STRING.as_ptr(),
    grouping: EMPTY_STRING.as_ptr(),
    int_curr_symbol: EMPTY_STRING.as_ptr(),
    currency_symbol: EMPTY_STRING.as_ptr(),
    mon_decimal_point: EMPTY_STRING.as_ptr(),
    mon_thousands_sep: EMPTY_STRING.as_ptr(),
    mon_grouping: EMPTY_STRING.as_ptr(),
    positive_sign: EMPTY_STRING.as_ptr(),
    negative_sign: NEGATIVE_SIGN.as_ptr(),
};

static LC_ALL_NAME: [u8; 12] = *b"en_US.UTF-8\0";

/// setlocale — set locale
#[unsafe(no_mangle)]
pub unsafe extern "C" fn setlocale(_category: i32, locale: *const u8) -> *const u8 {
    if locale.is_null() || *locale == 0 {
        return LC_ALL_NAME.as_ptr();
    }
    LC_ALL_NAME.as_ptr()
}

/// localeconv — get locale formatting parameters
#[unsafe(no_mangle)]
pub unsafe extern "C" fn localeconv() -> *mut Lconv {
    &raw mut DEFAULT_LCONV
}
