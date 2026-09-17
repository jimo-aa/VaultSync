//! C ABI 导出（P0-4：FFI 骨架）。
//!
//! 约定：
//! - 返回字符串一律 UTF-8 C 指针，调用方用完必须以 `vault_core_free_string` 归还；
//! - 后续涉及二进制载荷的接口将采用 (ptr, len) + 调用方提供的输出缓冲区，
//!   永不返回指向引擎内部状态的指针。
use std::ffi::{c_char, CString};

/// 引擎自检：0 = OK，非 0 = 引擎不可用。
#[no_mangle]
pub extern "C" fn vault_core_hello() -> i32 {
    if crate::self_check() {
        0
    } else {
        1
    }
}

/// 引擎版本号（UTF-8，SemVer，与 docs/10 版本规范对应）。
///
/// # Safety
/// 返回的指针必须传给 [`vault_core_free_string`] 释放。
#[no_mangle]
pub unsafe extern "C" fn vault_core_version() -> *const c_char {
    // env! 编译期常量，CString::new 不可能失败；P1 后此接口下沉为更完整的引擎信息。
    let s = CString::new(env!("CARGO_PKG_VERSION")).expect("version has no NUL");
    s.into_raw()
}

/// 释放 `vault_core_version` 返回的字符串。
///
/// # Safety
/// `ptr` 必须是本 crate 导出函数返回、且尚未释放过的指针。
#[no_mangle]
pub unsafe extern "C" fn vault_core_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        drop(unsafe { CString::from_raw(ptr) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;

    #[test]
    fn hello_returns_zero_when_self_check_passes() {
        assert_eq!(vault_core_hello(), 0);
    }

    #[test]
    fn version_roundtrip_and_free() {
        unsafe {
            let ptr = vault_core_version();
            let s = CStr::from_ptr(ptr).to_str().expect("utf8").to_owned();
            vault_core_free_string(ptr as *mut c_char);
            assert_eq!(s, env!("CARGO_PKG_VERSION"));
        }
    }
}
