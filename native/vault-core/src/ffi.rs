//! C ABI 导出（P0-4 骨架 + P1 密钥体系）。
//!
//! 约定：
//! - 状态码：0=OK，1=密码错误(out_wait_ms=新增冷却)，2=冷却中(out_wait_ms=剩余)，
//!   3=IO，4=安全存储不可用，5=未绑定生物识别，6=格式错误，7=参数非法，8=内部错误；
//! - 会话以不透明句柄（`*mut Session`）跨越 FFI，MK 永不出引擎；
//! - 字符串一律 UTF-8 C 指针；调用方用 `vault_core_free_string` 归还引擎分配的字符串。
use std::ffi::{c_char, CStr, CString};

use crate::platform_store::{open_os_store, SecureStore};
use crate::service::{self, CoreError};
use crate::session::Session;

pub const OK: i32 = 0;
pub const ERR_WRONG_PASSWORD: i32 = 1;
pub const ERR_COOLDOWN: i32 = 2;
pub const ERR_IO: i32 = 3;
pub const ERR_BIO_UNAVAILABLE: i32 = 4;
pub const ERR_BIO_NOT_BOUND: i32 = 5;
pub const ERR_FORMAT: i32 = 6;
pub const ERR_INVALID_ARG: i32 = 7;
pub const ERR_INTERNAL: i32 = 8;

fn map_err(e: &CoreError) -> i32 {
    match e {
        CoreError::WrongPassword(_) => ERR_WRONG_PASSWORD,
        CoreError::Cooldown(_) => ERR_COOLDOWN,
        CoreError::Io(_) => ERR_IO,
        CoreError::BioUnavailable => ERR_BIO_UNAVAILABLE,
        CoreError::BioNotBound => ERR_BIO_NOT_BOUND,
        CoreError::Format(_) => ERR_FORMAT,
        CoreError::Internal(_) => ERR_INTERNAL,
    }
}

fn write_wait(out: *mut u64, ms: u64) {
    if !out.is_null() {
        unsafe { out.write(ms) };
    }
}

/// # Safety
/// `p` 必须为合法 UTF-8 NUL 结尾 C 字符串指针，或为 null（→ Err）。
unsafe fn cstr<'a>(p: *const c_char) -> Result<&'a str, i32> {
    if p.is_null() {
        return Err(ERR_INVALID_ARG);
    }
    unsafe { CStr::from_ptr(p) }
        .to_str()
        .map_err(|_| ERR_INVALID_ARG)
}

/// # Safety
/// `path` 同上；`handle_out` / `wait_out` 可为 null（handle_out 为 null 时返回 INVALID_ARG）。
unsafe fn path_of(p: *const c_char) -> Result<std::path::PathBuf, i32> {
    Ok(std::path::PathBuf::from(unsafe { cstr(p) }?))
}

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
    let s =
        CString::new(env!("CARGO_PKG_VERSION")).unwrap_or_else(|_| panic!("version has no NUL"));
    s.into_raw()
}

/// 释放引擎返回的字符串。
///
/// # Safety
/// `ptr` 必须是本 crate 导出函数返回、且尚未释放过的指针。
#[no_mangle]
pub unsafe extern "C" fn vault_core_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        drop(unsafe { CString::from_raw(ptr) });
    }
}

/// 保险箱文件是否存在。1 = 存在，0 = 不存在，负值 = 参数非法。
///
/// # Safety
/// `path` 必须为合法 UTF-8 C 字符串。
#[no_mangle]
pub unsafe extern "C" fn vault_core_vault_exists(path: *const c_char) -> i32 {
    match unsafe { path_of(path) } {
        Ok(p) => i32::from(service::vault_exists(&p)),
        Err(e) => e,
    }
}

/// 保险箱是否已绑定生物识别。1/0，或状态码。
///
/// # Safety
/// `path` 必须为合法 UTF-8 C 字符串。
#[no_mangle]
pub unsafe extern "C" fn vault_core_bio_bound(path: *const c_char) -> i32 {
    match unsafe { path_of(path) }.and_then(|p| service::bio_bound(&p).map_err(|e| map_err(&e))) {
        Ok(b) => i32::from(b),
        Err(e) => e,
    }
}

/// 创建保险箱（首次运行 / 伪空间第二空间均用此接口，文件路径不同即相互独立）。
///
/// # Safety
/// `path`、`password` 必须为合法 UTF-8 C 字符串。
#[no_mangle]
pub unsafe extern "C" fn vault_core_create_vault(
    path: *const c_char,
    password: *const c_char,
    bind_bio: i32,
) -> i32 {
    let run = || -> Result<(), i32> {
        let p = unsafe { path_of(path) }?;
        let pwd = unsafe { cstr(password) }?;
        let store = open_os_store();
        service::create_vault(
            &p,
            pwd,
            bind_bio != 0,
            store.as_ref().map(|s| s as &dyn SecureStore),
        )
        .map_err(|e| map_err(&e))
    };
    run().err().unwrap_or(OK)
}

/// 主密码解锁。
///
/// 成功时向 `handle_out` 写入会话句柄；密码错误 / 冷却时向 `wait_out` 写毫秒数。
///
/// # Safety
/// `path`、`password` 必须为合法 UTF-8 C 字符串；`handle_out` 非空；
/// 会话句柄只能经 [`vault_core_lock`] 释放。
#[no_mangle]
pub unsafe extern "C" fn vault_core_unlock(
    path: *const c_char,
    password: *const c_char,
    disguise: i32,
    handle_out: *mut *mut Session,
    wait_out: *mut u64,
) -> i32 {
    let run = || -> Result<(), i32> {
        if handle_out.is_null() {
            return Err(ERR_INVALID_ARG);
        }
        let p = unsafe { path_of(path) }?;
        let pwd = unsafe { cstr(password) }?;
        match service::unlock(&p, pwd, disguise != 0) {
            Ok(s) => {
                unsafe { handle_out.write(Box::into_raw(Box::new(s))) };
                Ok(())
            }
            Err(CoreError::WrongPassword(w)) => {
                write_wait(wait_out, w);
                Err(ERR_WRONG_PASSWORD)
            }
            Err(CoreError::Cooldown(ms)) => {
                write_wait(wait_out, ms);
                Err(ERR_COOLDOWN)
            }
            Err(e) => Err(map_err(&e)),
        }
    };
    run().err().unwrap_or(OK)
}

/// 生物识别解锁（F-04）。语义同 [`vault_core_unlock`]。
///
/// # Safety
/// 同 [`vault_core_unlock`]。
#[no_mangle]
pub unsafe extern "C" fn vault_core_unlock_bio(
    path: *const c_char,
    handle_out: *mut *mut Session,
    wait_out: *mut u64,
) -> i32 {
    let run = || -> Result<(), i32> {
        if handle_out.is_null() {
            return Err(ERR_INVALID_ARG);
        }
        let p = unsafe { path_of(path) }?;
        let store = open_os_store().ok_or(ERR_BIO_UNAVAILABLE)?;
        match service::unlock_biometric(&p, &store) {
            Ok(s) => {
                unsafe { handle_out.write(Box::into_raw(Box::new(s))) };
                Ok(())
            }
            Err(CoreError::WrongPassword(w)) => {
                write_wait(wait_out, w);
                Err(ERR_WRONG_PASSWORD)
            }
            Err(CoreError::Cooldown(ms)) => {
                write_wait(wait_out, ms);
                Err(ERR_COOLDOWN)
            }
            Err(e) => Err(map_err(&e)),
        }
    };
    run().err().unwrap_or(OK)
}

/// 为会话所属保险箱绑定生物识别（F-03）。主空间会话专用。
///
/// # Safety
/// `handle` 必须是未释放的有效会话句柄。
#[no_mangle]
pub unsafe extern "C" fn vault_core_bind_bio(handle: *mut Session) -> i32 {
    if handle.is_null() {
        return ERR_INVALID_ARG;
    }
    let store = match open_os_store() {
        Some(s) => s,
        None => return ERR_BIO_UNAVAILABLE,
    };
    let session = unsafe { &*handle };
    service::bind_biometric(session, &store)
        .map_err(|e| map_err(&e))
        .err()
        .unwrap_or(OK)
}

/// 解除生物识别绑定。
///
/// # Safety
/// `handle` 必须是未释放的有效会话句柄。
#[no_mangle]
pub unsafe extern "C" fn vault_core_unbind_bio(handle: *mut Session) -> i32 {
    if handle.is_null() {
        return ERR_INVALID_ARG;
    }
    let store = match open_os_store() {
        Some(s) => s,
        None => return ERR_BIO_UNAVAILABLE,
    };
    let session = unsafe { &*handle };
    service::unbind_biometric(session, &store)
        .map_err(|e| map_err(&e))
        .err()
        .unwrap_or(OK)
}

/// 修改主密码（F-07）：仅重包装 MK，数据零重加密。主空间会话专用。
///
/// # Safety
/// `handle` 必须是未释放的有效会话句柄；`old_password`/`new_password` 为合法 UTF-8。
#[no_mangle]
pub unsafe extern "C" fn vault_core_change_password(
    handle: *mut Session,
    old_password: *const c_char,
    new_password: *const c_char,
) -> i32 {
    let run = || -> Result<(), i32> {
        if handle.is_null() {
            return Err(ERR_INVALID_ARG);
        }
        let old = unsafe { cstr(old_password) }?;
        let new = unsafe { cstr(new_password) }?;
        let session = unsafe { &*handle };
        service::change_password(session, old, new).map_err(|e| map_err(&e))
    };
    run().err().unwrap_or(OK)
}

/// 锁定：销毁会话句柄并擦除 MK。句柄自此失效。
///
/// # Safety
/// `handle` 必须是未释放的有效会话句柄；调用后不得复用。
#[no_mangle]
pub unsafe extern "C" fn vault_core_lock(handle: *mut Session) {
    if !handle.is_null() {
        drop(unsafe { Box::from_raw(handle) });
    }
}

/// 查询当前冷却剩余毫秒（便于 UI 轮询），无冷却写 0。
///
/// # Safety
/// `path` 合法；`wait_out` 非空。
#[no_mangle]
pub unsafe extern "C" fn vault_core_cooldown_remaining_ms(
    path: *const c_char,
    wait_out: *mut u64,
) -> i32 {
    let run = || -> Result<(), i32> {
        if wait_out.is_null() {
            return Err(ERR_INVALID_ARG);
        }
        let p = unsafe { path_of(path) }?;
        let ms = service::cooldown_remaining_ms(&p).unwrap_or(0);
        write_wait(wait_out, ms);
        Ok(())
    };
    run().err().unwrap_or(OK)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::ffi::CString;
    use std::ptr;

    fn cs(s: &str) -> CString {
        CString::new(s).expect("no NUL")
    }

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

    #[test]
    fn full_flow_via_ffi_with_mock_store() {
        // 注意：create/unlock 走 OsSecureStore；本用例只测纯密码路径。
        let dir = tempfile::tempdir().expect("tmp");
        let vault = dir.path().join("t.vsvb");
        let cp = cs(vault.to_str().expect("utf8"));
        let pw = cs("pw-1234");

        unsafe {
            assert!(vault_core_vault_exists(cp.as_ptr()) == 0);
            assert_eq!(vault_core_create_vault(cp.as_ptr(), pw.as_ptr(), 0), OK);
            assert!(vault_core_vault_exists(cp.as_ptr()) == 1);

            let mut h: *mut Session = ptr::null_mut();
            let mut wait: u64 = 0;
            assert_eq!(
                vault_core_unlock(cp.as_ptr(), cs("wrong").as_ptr(), 0, &mut h, &mut wait),
                ERR_WRONG_PASSWORD
            );
            assert_eq!(
                vault_core_unlock(cp.as_ptr(), pw.as_ptr(), 0, &mut h, &mut wait),
                OK
            );
            assert!(!h.is_null());

            assert_eq!(
                vault_core_change_password(h, pw.as_ptr(), cs("new-pw").as_ptr()),
                OK
            );
            assert_eq!(vault_core_lock(h), ());
            // 句柄已失效，锁定后重新解锁需新密码
            let mut h2: *mut Session = ptr::null_mut();
            assert_eq!(
                vault_core_unlock(cp.as_ptr(), cs("new-pw").as_ptr(), 0, &mut h2, &mut wait),
                OK
            );
            vault_core_lock(h2);
        }
    }

    #[test]
    fn null_args_rejected() {
        unsafe {
            let mut h: *mut Session = ptr::null_mut();
            assert_eq!(
                vault_core_unlock(ptr::null(), ptr::null(), 0, &mut h, ptr::null_mut()),
                ERR_INVALID_ARG
            );
            assert_eq!(vault_core_lock(ptr::null_mut()), ());
        }
    }

    /// 真实平台安全存储联测（Windows/macOS 本机运行）：
    /// cargo test -p vault-core -- --ignored --nocapture
    #[test]
    #[ignore]
    fn bio_end_to_end_with_os_store() {
        let dir = tempfile::tempdir().expect("tmp");
        let vault = dir.path().join("bio.vsvb");
        let vp = cs(vault.to_str().expect("utf8"));
        let pw = cs("pw-bio");
        unsafe {
            assert_eq!(vault_core_create_vault(vp.as_ptr(), pw.as_ptr(), 1), OK);

            let mut h: *mut Session = ptr::null_mut();
            let mut wait: u64 = 0;
            let status = vault_core_unlock_bio(vp.as_ptr(), &mut h, &mut wait);
            println!("unlock_bio status={status} handle={h:?}");
            assert_eq!(status, OK, "unlock_bio 应成功");
            assert!(!h.is_null(), "句柄必须非空");
            vault_core_lock(h);

            // 解绑后回到 BIO_NOT_BOUND
            assert_eq!(
                vault_core_unlock(vp.as_ptr(), pw.as_ptr(), 0, &mut h, &mut wait),
                OK
            );
            assert_eq!(vault_core_unbind_bio(h), OK);
            vault_core_lock(h);
            let status = vault_core_unlock_bio(vp.as_ptr(), &mut h, &mut wait);
            assert_eq!(status, ERR_BIO_NOT_BOUND);
            assert!(h.is_null());
        }
    }
}
