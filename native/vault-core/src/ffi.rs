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
use vault_vault::vault::Vault;

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
                s.audit(
                    "session",
                    if s.disguise {
                        "unlock disguise"
                    } else {
                        "unlock"
                    },
                );
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
                s.audit("session", "unlock biometric");
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
    let rc = service::bind_biometric(session, &store)
        .map_err(|e| map_err(&e))
        .err()
        .unwrap_or(OK);
    if rc == OK {
        session.audit("security", "bind biometric");
    }
    rc
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
    let rc = service::unbind_biometric(session, &store)
        .map_err(|e| map_err(&e))
        .err()
        .unwrap_or(OK);
    if rc == OK {
        session.audit("security", "unbind biometric");
    }
    rc
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
        let rc = service::change_password(session, old, new).map_err(|e| map_err(&e));
        if rc.is_ok() {
            session.audit("session", "change password (re-wrap MK only)");
        }
        rc
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
        let session = unsafe { &*handle };
        session.audit("session", "lock");
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

// ==== P2 保险箱操作（docs/05-02）====
// JSON 输出以引擎分配的 CString 返回，调用方用 vault_core_free_string 释放；
// 出错返回空指针（错误语义与状态码约定一致的部分走 i32 接口）。

fn vault_op<T>(
    handle: *mut Session,
    f: impl FnOnce(&mut Vault, &Session) -> Result<T, CoreError>,
) -> Result<T, i32> {
    if handle.is_null() {
        return Err(ERR_INVALID_ARG);
    }
    let session = unsafe { &*handle };
    session
        .with_vault(|v| f(v, session))
        .map_err(|e| map_err(&e))
}

/// 敏感操作成功后追加审计条目（P5-1，docs/05-06 §3.2 记录范围）。
///
/// # Safety
/// `handle` 为空时静默返回；审计写入失败不阻断业务（见 `Session::audit` 的决策说明）。
/// 同步摘要压成一行（审计 detail 用；与 Dart 侧的摘要卡同字段）。
fn summary_line(v: &serde_json::Value) -> String {
    let n = |k: &str| {
        v.get(k)
            .and_then(|x| x.as_array())
            .map(Vec::len)
            .unwrap_or(0)
    };
    format!(
        "pushed={} pulled={} deleted={} conflicts={}",
        n("pushed"),
        n("pulled"),
        n("deleted"),
        n("conflicts")
    )
}

unsafe fn audit_op(handle: *mut Session, kind: &str, detail: &str) {
    if handle.is_null() {
        return;
    }
    let session = unsafe { &*handle };
    session.audit(kind, detail);
}

fn json_out(v: serde_json::Value) -> *mut c_char {
    match serde_json::to_string(&v)
        .ok()
        .and_then(|s| CString::new(s).ok())
    {
        Some(c) => c.into_raw(),
        None => std::ptr::null_mut(),
    }
}

/// 创建文件夹，返回新文件夹 id（out 参数）。
///
/// # Safety
/// `handle` 有效；`name` 合法 UTF-8；`folder_id_out` 非空。
#[no_mangle]
pub unsafe extern "C" fn vault_core_vault_mkdir(
    handle: *mut Session,
    parent: i64,
    name: *const c_char,
    folder_id_out: *mut u64,
) -> i32 {
    let run = || -> Result<(), i32> {
        if folder_id_out.is_null() {
            return Err(ERR_INVALID_ARG);
        }
        let name = unsafe { cstr(name) }?;
        let id = vault_op(handle, |v, _| {
            v.mkdir(parent as u64, name).map_err(CoreError::Internal)
        })?;
        unsafe { folder_id_out.write(id) };
        unsafe {
            audit_op(
                handle,
                "vault",
                &format!("mkdir parent={parent} name={name}"),
            )
        };
        Ok(())
    };
    run().err().unwrap_or(OK)
}

/// 列出子项 JSON。成功返回 JSON 字符串（需 free），失败返回空指针。
///
/// # Safety
/// `handle` 有效；`folder` 为文件夹 id。
#[no_mangle]
pub unsafe extern "C" fn vault_core_vault_list(handle: *mut Session, folder: i64) -> *mut c_char {
    match vault_op(handle, |v, _| {
        v.list_children(folder as u64).map_err(CoreError::Internal)
    }) {
        Ok(json) => match CString::new(json) {
            Ok(c) => c.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
        Err(_) => std::ptr::null_mut(),
    }
}

/// 全文检索，返回 JSON 字符串（需 free）。
///
/// # Safety
/// `handle` 有效；`query` 合法 UTF-8。
#[no_mangle]
pub unsafe extern "C" fn vault_core_vault_search(
    handle: *mut Session,
    query: *const c_char,
) -> *mut c_char {
    let run = || -> Result<*mut c_char, i32> {
        let q = unsafe { cstr(query) }?;
        let json = vault_op(handle, |v, _| Ok(v.search(q)))?;
        let c = CString::new(json).map_err(|_| ERR_INTERNAL)?;
        Ok(c.into_raw())
    };
    run().unwrap_or(std::ptr::null_mut())
}

/// 导入文件，返回文件 id（out 参数）。
///
/// # Safety
/// `handle` 有效；`src` 路径合法；`file_id_out` 非空。
#[no_mangle]
pub unsafe extern "C" fn vault_core_vault_import(
    handle: *mut Session,
    src: *const c_char,
    folder: i64,
    file_id_out: *mut u64,
) -> i32 {
    if handle.is_null() || file_id_out.is_null() {
        return ERR_INVALID_ARG;
    }
    let session = unsafe { &*handle };
    let src = match unsafe { cstr(src) } {
        Ok(s) => s,
        Err(e) => return e,
    };
    match session.with_vault(|v| {
        v.import_file(std::path::Path::new(src), folder as u64)
            .map_err(CoreError::Internal)
    }) {
        Ok(id) => {
            unsafe { file_id_out.write(id) };
            unsafe {
                audit_op(
                    handle,
                    "vault",
                    &format!("import src={src} folder={folder} id={id}"),
                )
            };
            OK
        }
        Err(e) => map_err(&e),
    }
}

/// 导出解密文件。
///
/// # Safety
/// `handle` 有效；`dest` 合法。
#[no_mangle]
pub unsafe extern "C" fn vault_core_vault_export(
    handle: *mut Session,
    file_id: i64,
    dest: *const c_char,
) -> i32 {
    let run = || -> Result<(), i32> {
        let dest = unsafe { cstr(dest) }?;
        vault_op(handle, |v, _| {
            v.export_file(file_id as u64, std::path::Path::new(dest))
                .map_err(CoreError::Internal)
        })
    };
    let rc = run().err().unwrap_or(OK);
    if rc == OK {
        unsafe { audit_op(handle, "vault", &format!("export id={file_id}")) };
    }
    rc
}

/// 重命名文件。
///
/// # Safety
/// `handle` 有效；`name` 合法。
#[no_mangle]
pub unsafe extern "C" fn vault_core_vault_rename_file(
    handle: *mut Session,
    file_id: i64,
    name: *const c_char,
) -> i32 {
    let run = || -> Result<(), i32> {
        let name = unsafe { cstr(name) }?;
        vault_op(handle, |v, _| {
            v.rename_file(file_id as u64, name)
                .map_err(CoreError::Internal)
        })
    };
    let rc = run().err().unwrap_or(OK);
    if rc == OK {
        unsafe { audit_op(handle, "vault", &format!("rename file={file_id}")) };
    }
    rc
}

/// 重命名文件夹（根目录不可改名）。
///
/// # Safety
/// `handle` 有效；`name` 合法。
#[no_mangle]
pub unsafe extern "C" fn vault_core_vault_rename_folder(
    handle: *mut Session,
    folder_id: i64,
    name: *const c_char,
) -> i32 {
    let run = || -> Result<(), i32> {
        let name = unsafe { cstr(name) }?;
        vault_op(handle, |v, _| {
            v.rename_folder(folder_id as u64, name)
                .map_err(CoreError::Internal)
        })
    };
    let rc = run().err().unwrap_or(OK);
    if rc == OK {
        unsafe { audit_op(handle, "vault", &format!("rename folder={folder_id}")) };
    }
    rc
}

/// 删除文件（secure=1 时先单次覆写再删除）。
///
/// # Safety
/// `handle` 有效。
#[no_mangle]
pub unsafe extern "C" fn vault_core_vault_delete_file(
    handle: *mut Session,
    file_id: i64,
    secure: i32,
) -> i32 {
    let rc = vault_op(handle, |v, _| {
        v.delete_file(file_id as u64, secure != 0)
            .map_err(CoreError::Internal)
    })
    .err()
    .unwrap_or(OK);
    if rc == OK {
        unsafe {
            audit_op(
                handle,
                "vault",
                &format!("wipe file={file_id} secure={secure}"),
            )
        };
    }
    rc
}

/// 递归删除文件夹（secure=1 时逐个先单次覆写再删除），返回删除的文件数；失败返回负状态码。
///
/// # Safety
/// `handle` 有效。
#[no_mangle]
pub unsafe extern "C" fn vault_core_vault_delete_folder(
    handle: *mut Session,
    folder_id: i64,
    secure: i32,
) -> i32 {
    let r = vault_op(handle, |v, _| {
        v.delete_folder(folder_id as u64, secure != 0)
            .map_err(CoreError::Internal)
    });
    // 成功返回删除的文件数（≥0）；失败返回负状态码（-1..-8 不会与计数混淆）。
    match r {
        Ok(n) => {
            unsafe {
                audit_op(
                    handle,
                    "vault",
                    &format!("wipe folder={folder_id} files={n} secure={secure}"),
                )
            };
            i32::try_from(n).unwrap_or(i32::MAX)
        }
        Err(code) => -code,
    }
}

/// 设置标签（逗号分隔）。
///
/// # Safety
/// `handle` 有效。
#[no_mangle]
pub unsafe extern "C" fn vault_core_vault_set_tags(
    handle: *mut Session,
    file_id: i64,
    tags_csv: *const c_char,
) -> i32 {
    let run = || -> Result<(), i32> {
        let csv = unsafe { cstr(tags_csv) }?;
        let tags: Vec<String> = if csv.is_empty() {
            Vec::new()
        } else {
            csv.split(',').map(str::to_string).collect()
        };
        vault_op(handle, |v, _| {
            v.set_tags(file_id as u64, tags)
                .map_err(CoreError::Internal)
        })
    };
    let rc = run().err().unwrap_or(OK);
    if rc == OK {
        unsafe { audit_op(handle, "vault", &format!("tags id={file_id}")) };
    }
    rc
}

/// 创建阅后即焚分享，返回 JSON {"shareId":u64,"token":"hex"}。
///
/// # Safety
/// `handle` 有效。
#[no_mangle]
pub unsafe extern "C" fn vault_core_vault_share_create(
    handle: *mut Session,
    file_id: i64,
    ttl_secs: u64,
    max_opens: u32,
) -> *mut c_char {
    let run = || -> Result<serde_json::Value, i32> {
        vault_op(handle, |v, _| {
            v.create_share(file_id as u64, ttl_secs, max_opens)
                .map(|(id, token)| serde_json::json!({"shareId": id, "token": token}))
                .map_err(CoreError::Internal)
        })
    };
    match run() {
        Ok(v) => {
            unsafe { audit_op(handle, "security", &format!("share create id={file_id}")) };
            json_out(v)
        }
        Err(_) => std::ptr::null_mut(),
    }
}

/// 通过分享导出（消耗一次打开机会）。
///
/// # Safety
/// `handle` 有效；`token`、`dest` 合法。
#[no_mangle]
pub unsafe extern "C" fn vault_core_vault_share_open(
    handle: *mut Session,
    share_id: i64,
    token: *const c_char,
    dest: *const c_char,
) -> i32 {
    let run = || -> Result<(), i32> {
        let token = unsafe { cstr(token) }?;
        let dest = unsafe { cstr(dest) }?;
        vault_op(handle, |v, _| {
            v.open_share(share_id as u64, token, std::path::Path::new(dest))
                .map_err(CoreError::Internal)
        })
    };
    let rc = run().err().unwrap_or(OK);
    if rc == OK {
        unsafe { audit_op(handle, "security", &format!("share open id={share_id}")) };
    }
    rc
}

// ==== P5-3 隐写术（docs/05-05）====
// 隐写载荷 = AEAD(HKDF(MK,"stego-payload"), [name‖data])，再按位写进 PNG 的 LSB。
// 因此「密文在图片内不可直接读」，且提取必须持有同一保险箱的 MK（本机隐蔽辅助，
// 不作跨设备传输通道——跨设备走 P2P/中继）。

/// 隐写引擎是否已启用（1=启用，0=未启用）。
///
/// # Safety
/// `handle` 必须是未释放的有效会话句柄。
#[no_mangle]
pub unsafe extern "C" fn vault_core_stego_status(handle: *mut Session) -> i32 {
    if handle.is_null() {
        return ERR_INVALID_ARG;
    }
    let session = unsafe { &*handle };
    i32::from(
        session
            .stego_enabled
            .load(std::sync::atomic::Ordering::Relaxed),
    )
}

/// 启用 / 停用隐写引擎（docs/05-06 §七）；状态变更记审计。
///
/// # Safety
/// `handle` 必须是未释放的有效会话句柄。
#[no_mangle]
pub unsafe extern "C" fn vault_core_stego_set_enabled(handle: *mut Session, on: i32) -> i32 {
    if handle.is_null() {
        return ERR_INVALID_ARG;
    }
    let session = unsafe { &*handle };
    let enabled = on != 0;
    session
        .stego_enabled
        .store(enabled, std::sync::atomic::Ordering::Relaxed);
    session.audit(
        "security",
        if enabled {
            "stego enable"
        } else {
            "stego disable"
        },
    );
    OK
}

/// 图片容量（可嵌入载荷字节数，已扣除长度前缀）。失败返回负错误码。
///
/// # Safety
/// `image_path` 为合法路径。
#[no_mangle]
pub unsafe extern "C" fn vault_core_stego_capacity(
    handle: *mut Session,
    image_path: *const c_char,
) -> i64 {
    if handle.is_null() {
        return -i64::from(ERR_INVALID_ARG);
    }
    let path = match unsafe { cstr(image_path) } {
        Ok(p) => p,
        Err(e) => return -i64::from(e),
    };
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(_) => return -i64::from(ERR_IO),
    };
    match vault_stego::png_size(&bytes) {
        Ok((w, h)) => i64::try_from(vault_stego::capacity_bytes(w, h)).unwrap_or(i64::MAX),
        Err(_) => -i64::from(ERR_FORMAT),
    }
}

/// 隐写嵌入：把保险箱内某文件的**明文**加密后嵌入图片，输出到 `out_path`。
///
/// # Safety
/// `handle` 有效；`image_path`、`out_path` 合法。
#[no_mangle]
pub unsafe extern "C" fn vault_core_stego_embed(
    handle: *mut Session,
    file_id: i64,
    image_path: *const c_char,
    out_path: *const c_char,
) -> i32 {
    let run = || -> Result<(), i32> {
        if handle.is_null() {
            return Err(ERR_INVALID_ARG);
        }
        let image_path = unsafe { cstr(image_path) }?;
        let out_path = unsafe { cstr(out_path) }?;
        let session = unsafe { &*handle };
        if !session
            .stego_enabled
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            return Err(ERR_INVALID_ARG);
        }
        let image = std::fs::read(image_path).map_err(|_| ERR_IO)?;

        // 1) 导出明文到临时文件（复用既有导出路径，避免在 FFI 层重写容器解析）
        let tmp = session.data_dir().join(format!("stego-{file_id}.tmp"));
        session
            .with_vault(|v| {
                v.export_file(file_id as u64, &tmp)
                    .map_err(CoreError::Internal)
            })
            .map_err(|e| map_err(&e))?;
        let plain = std::fs::read(&tmp).map_err(|_| ERR_IO)?;
        let _ = std::fs::remove_file(&tmp);

        // 2) 取原文件名（用于提取端还原）
        let name = session
            .with_vault(|v| v.file_name(file_id as u64).map_err(CoreError::Internal))
            .unwrap_or_else(|_| format!("file-{file_id}"));

        // 3) 载荷 = AEAD(HKDF(MK,"stego-payload"), [len(name)‖name‖data])，随机 nonce
        let key = session.stego_key();
        let nonce_v = vault_crypto::random::random_bytes(vault_crypto::AES_GCM_NONCE_LEN);
        let mut nonce = [0u8; vault_crypto::AES_GCM_NONCE_LEN];
        nonce.copy_from_slice(&nonce_v);
        let mut inner = Vec::with_capacity(4 + name.len() + plain.len());
        inner.extend_from_slice(&(name.len() as u32).to_be_bytes());
        inner.extend_from_slice(name.as_bytes());
        inner.extend_from_slice(&plain);
        let sealed =
            vault_crypto::aead::aead_encrypt(&key, &nonce, &inner).map_err(|_| ERR_INTERNAL)?;

        // 4) LSB 嵌入并写出
        let out = vault_stego::embed(&image, &sealed).map_err(|_| ERR_FORMAT)?;
        std::fs::write(out_path, out).map_err(|_| ERR_IO)?;
        session.audit("security", &format!("stego embed id={file_id}"));
        Ok(())
    };
    run().err().unwrap_or(OK)
}

/// 隐写提取：从图片取回载荷、解密并写出明文到 `dest_path`。
/// 返回 JSON {"name":…,"size":N}（供 UI 提示还原出的文件名），失败返回 null。
///
/// # Safety
/// `handle` 有效；`image_path`、`dest_path` 合法。
#[no_mangle]
pub unsafe extern "C" fn vault_core_stego_extract(
    handle: *mut Session,
    image_path: *const c_char,
    dest_path: *const c_char,
) -> *mut c_char {
    let run = || -> Result<serde_json::Value, i32> {
        if handle.is_null() {
            return Err(ERR_INVALID_ARG);
        }
        let image_path = unsafe { cstr(image_path) }?;
        let dest_path = unsafe { cstr(dest_path) }?;
        let session = unsafe { &*handle };
        if !session
            .stego_enabled
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            return Err(ERR_INVALID_ARG);
        }
        let image = std::fs::read(image_path).map_err(|_| ERR_IO)?;
        let sealed = vault_stego::extract(&image).map_err(|_| ERR_FORMAT)?;
        let key = session.stego_key();
        let inner = vault_crypto::aead::aead_decrypt(&key, &sealed).ok_or(ERR_WRONG_PASSWORD)?;
        if inner.len() < 4 {
            return Err(ERR_FORMAT);
        }
        let name_len = u32::from_be_bytes(inner[0..4].try_into().map_err(|_| ERR_FORMAT)?) as usize;
        if inner.len() < 4 + name_len {
            return Err(ERR_FORMAT);
        }
        let name = String::from_utf8_lossy(&inner[4..4 + name_len]).to_string();
        let data = &inner[4 + name_len..];
        std::fs::write(dest_path, data).map_err(|_| ERR_IO)?;
        session.audit("security", "stego extract");
        Ok(serde_json::json!({ "name": name, "size": data.len() }))
    };
    match run() {
        Ok(v) => json_out(v),
        Err(_) => std::ptr::null_mut(),
    }
}

// ==== P5-1 审计日志 / P5-5 紧急销毁（docs/05-06）====
// 审计日志密钥派生自 MK，随会话打开/销毁；失败不阻断业务（见 Session::audit）。

/// 审计条目列表。返回 JSON {"count","head","entries":[{seq,tsMs,kind,detail,prevHash,hash}]}。
///
/// # Safety
/// `handle` 必须是未释放的有效会话句柄。
#[no_mangle]
pub unsafe extern "C" fn vault_core_audit_list(handle: *mut Session) -> *mut c_char {
    let run = || -> Result<serde_json::Value, i32> {
        if handle.is_null() {
            return Err(ERR_INVALID_ARG);
        }
        let session = unsafe { &*handle };
        session
            .with_audit(|log| {
                serde_json::json!({
                    "count": log.len(),
                    "head": log.head_hash(),
                    "entries": log.entries().iter().map(|e| serde_json::json!({
                        "seq": e.seq,
                        "tsMs": e.ts_ms,
                        "kind": e.kind,
                        "detail": e.detail,
                        "prevHash": e.prev_hash,
                        "hash": e.hash,
                    })).collect::<Vec<_>>(),
                })
            })
            .ok_or(ERR_INTERNAL)
    };
    match run() {
        Ok(v) => json_out(v),
        Err(_) => std::ptr::null_mut(),
    }
}

/// 链式校验。返回 JSON {"ok","checked","brokenAt":u64|null,"reason":string|null}。
///
/// # Safety
/// `handle` 必须是未释放的有效会话句柄。
#[no_mangle]
pub unsafe extern "C" fn vault_core_audit_verify(handle: *mut Session) -> *mut c_char {
    let run = || -> Result<serde_json::Value, i32> {
        if handle.is_null() {
            return Err(ERR_INVALID_ARG);
        }
        let session = unsafe { &*handle };
        session
            .with_audit(|log| {
                let v = log.verify();
                serde_json::json!({
                    "ok": v.ok,
                    "checked": v.checked,
                    "brokenAt": v.broken_at,
                    "reason": v.reason,
                })
            })
            .ok_or(ERR_INTERNAL)
    };
    match run() {
        Ok(v) => json_out(v),
        Err(_) => std::ptr::null_mut(),
    }
}

/// 加密导出审计日志到 `dest`（专用导出密钥 = HKDF(MK,"audit-export")）。
/// 导出事件本身也记入审计（docs/05-06 §3.3）。
///
/// # Safety
/// `handle` 有效；`dest` 合法路径。
#[no_mangle]
pub unsafe extern "C" fn vault_core_audit_export(handle: *mut Session, dest: *const c_char) -> i32 {
    let run = || -> Result<(), i32> {
        if handle.is_null() {
            return Err(ERR_INVALID_ARG);
        }
        let dest = unsafe { cstr(dest) }?;
        let session = unsafe { &*handle };
        let key = session.audit_export_key();
        session
            .with_audit(|log| log.export_encrypted(std::path::Path::new(dest), &key))
            .ok_or(ERR_INTERNAL)?
            .map_err(|_| ERR_IO)?;
        session.audit("security", "audit export");
        Ok(())
    };
    run().err().unwrap_or(OK)
}

/// 紧急销毁 · 本机（P5-5，docs/05-06 §五）：加密擦除本机全部可解封材料
/// （保险箱头部 + 数据目录：密文容器 / 加密索引 / P2P 身份 / 审计日志）。
/// `secure=1` 时先对每个文件做单次覆写（介质兜底语义）。
/// 调用成功后该句柄对应的保险箱已不存在，调用方应立即 `vault_core_lock`。
///
/// # Safety
/// `handle` 必须是未释放的有效会话句柄。
#[no_mangle]
pub unsafe extern "C" fn vault_core_destroy_local(handle: *mut Session, secure: i32) -> i32 {
    let run = || -> Result<(), i32> {
        if handle.is_null() {
            return Err(ERR_INVALID_ARG);
        }
        let session = unsafe { &*handle };
        // 尽力写入最后一条审计（docs/05-06 §五「同时尽力写入最终审计记录」）；
        // 若未先导出，该条目会随数据目录一并销毁——UI 提供「先导出」选项。
        session.audit(
            "security",
            if secure != 0 {
                "destroy local (secure overwrite)"
            } else {
                "destroy local"
            },
        );
        crate::service::wipe_local(&session.vault_path, secure != 0).map_err(|_| ERR_IO)
    };
    run().err().unwrap_or(OK)
}

// ==== P3 P2P 同步（docs/05-03、docs/08）====
// 全部接口仅主空间会话可用；JSON 输出需 vault_core_free_string 释放。
// 同步为阻塞调用（完成或超时返回）；远程销毁在引擎监听线程异步执行。

/// 生成一次性配对邀请码。返回 JSON {"code","fingerprint","deviceId","port"}。
///
/// # Safety
/// `handle` 必须是未释放的有效会话句柄。
#[no_mangle]
pub unsafe extern "C" fn vault_core_p2p_pair_begin(handle: *mut Session) -> *mut c_char {
    let run = || -> Result<serde_json::Value, i32> {
        let session = unsafe { &*handle };
        let engine = crate::p2p_service::p2p_engine(session).map_err(|e| map_err(&e))?;
        engine.pair_begin().map_err(|_| ERR_INTERNAL)
    };
    match run() {
        Ok(v) => json_out(v),
        Err(_) => std::ptr::null_mut(),
    }
}

/// 连接新设备完成配对（PSK = 邀请码派生）。返回 JSON {"peerId","name"}。
///
/// # Safety
/// `handle` 有效；`addr`、`code` 合法 UTF-8。
#[no_mangle]
pub unsafe extern "C" fn vault_core_p2p_pair_join(
    handle: *mut Session,
    addr: *const c_char,
    code: *const c_char,
) -> *mut c_char {
    let run = || -> Result<serde_json::Value, i32> {
        let session = unsafe { &*handle };
        let a = unsafe { cstr(addr) }?;
        let code = unsafe { cstr(code) }?;
        let engine = crate::p2p_service::p2p_engine(session).map_err(|e| map_err(&e))?;
        engine.pair_join(a, code).map_err(|_| ERR_IO)
    };
    match run() {
        Ok(v) => json_out(v),
        Err(_) => std::ptr::null_mut(),
    }
}

/// 发起一次增量同步（阻塞直至完成）。返回摘要 JSON。
///
/// # Safety
/// `handle` 有效；`addr` 合法。
#[no_mangle]
pub unsafe extern "C" fn vault_core_p2p_sync(
    handle: *mut Session,
    addr: *const c_char,
) -> *mut c_char {
    let run = || -> Result<serde_json::Value, i32> {
        let session = unsafe { &*handle };
        let a = unsafe { cstr(addr) }?;
        let engine = crate::p2p_service::p2p_engine(session).map_err(|e| map_err(&e))?;
        engine.sync_with(a).map_err(|_| ERR_IO)
    };
    match run() {
        Ok(v) => json_out(v),
        Err(_) => std::ptr::null_mut(),
    }
}

/// 发起一次经中继的增量同步（发起端）。返回摘要 JSON。
///
/// # Safety
/// `handle` 有效；`relay`、`room` 合法。
#[no_mangle]
pub unsafe extern "C" fn vault_core_p2p_sync_relay(
    handle: *mut Session,
    relay: *const c_char,
    room: *const c_char,
) -> *mut c_char {
    let run = || -> Result<serde_json::Value, i32> {
        let session = unsafe { &*handle };
        let r = unsafe { cstr(relay) }?;
        let room = unsafe { cstr(room) }?;
        let engine = crate::p2p_service::p2p_engine(session).map_err(|e| map_err(&e))?;
        let v = engine.sync_via_relay(r, room).map_err(|_| ERR_IO)?;
        session.audit("device", &format!("sync relay {}", summary_line(&v)));
        Ok(v)
    };
    match run() {
        Ok(v) => json_out(v),
        Err(_) => std::ptr::null_mut(),
    }
}

/// 作为响应端经中继接入一次（阻塞至本轮同步结束；冒烟/无直连场景用）。
///
/// # Safety
/// `handle` 有效；`relay`、`room` 合法。
#[no_mangle]
pub unsafe extern "C" fn vault_core_p2p_serve_relay(
    handle: *mut Session,
    relay: *const c_char,
    room: *const c_char,
) -> i32 {
    let run = || -> Result<(), i32> {
        let session = unsafe { &*handle };
        let r = unsafe { cstr(relay) }?;
        let room = unsafe { cstr(room) }?;
        let engine = crate::p2p_service::p2p_engine(session).map_err(|e| map_err(&e))?;
        engine.serve_relay_connection(r, room).map_err(|_| ERR_IO)
    };
    run().err().unwrap_or(OK)
}

/// P2P 状态（本机身份 / 对端 / 事件 / 武装的销毁）。返回 JSON。
///
/// # Safety
/// `handle` 有效。
#[no_mangle]
pub unsafe extern "C" fn vault_core_p2p_status(handle: *mut Session) -> *mut c_char {
    let run = || -> Result<serde_json::Value, i32> {
        let session = unsafe { &*handle };
        let engine = crate::p2p_service::p2p_engine(session).map_err(|e| map_err(&e))?;
        Ok(engine.status())
    };
    match run() {
        Ok(v) => json_out(v),
        Err(_) => std::ptr::null_mut(),
    }
}

/// 解除与指定设备的配对（从对端登记表移除）。0 = OK（本来就不存在亦返回 0，幂等）。
///
/// # Safety
/// `handle` 有效；`device_id` 合法 UTF-8。
#[no_mangle]
pub unsafe extern "C" fn vault_core_p2p_unpair(
    handle: *mut Session,
    device_id: *const c_char,
) -> i32 {
    let run = || -> Result<(), i32> {
        if handle.is_null() {
            return Err(ERR_INVALID_ARG);
        }
        let session = unsafe { &*handle };
        let id = unsafe { cstr(device_id) }?;
        let engine = crate::p2p_service::p2p_engine(session).map_err(|e| map_err(&e))?;
        let removed = engine.unpair(id).map_err(|_| ERR_INTERNAL)?;
        session.audit("device", &format!("unpair peer={id} removed={removed}"));
        Ok(())
    };
    run().err().unwrap_or(OK)
}

/// 向对端发送远程销毁指令（签名 + 延迟秒数；0 = 到达即执行）。
///
/// # Safety
/// `handle` 有效；`addr`、`target` 合法。
#[no_mangle]
pub unsafe extern "C" fn vault_core_p2p_destroy_arm(
    handle: *mut Session,
    addr: *const c_char,
    target: *const c_char,
    delay_secs: u64,
) -> *mut c_char {
    let run = || -> Result<serde_json::Value, i32> {
        let session = unsafe { &*handle };
        let a = unsafe { cstr(addr) }?;
        let t = unsafe { cstr(target) }?;
        let engine = crate::p2p_service::p2p_engine(session).map_err(|e| map_err(&e))?;
        let v = engine
            .destroy_arm_remote(a, t, delay_secs)
            .map_err(|_| ERR_IO)?;
        session.audit(
            "security",
            &format!("destroy arm target={t} delay={delay_secs}s"),
        );
        Ok(v)
    };
    match run() {
        Ok(v) => json_out(v),
        Err(_) => std::ptr::null_mut(),
    }
}

/// 取消本机已武装的延迟销毁。1 = 有任务被取消，0 = 无。
///
/// # Safety
/// `handle` 有效。
#[no_mangle]
pub unsafe extern "C" fn vault_core_p2p_destroy_cancel(handle: *mut Session) -> i32 {
    if handle.is_null() {
        return ERR_INVALID_ARG;
    }
    let session = unsafe { &*handle };
    match crate::p2p_service::p2p_engine(session) {
        Ok(e) => {
            let cancelled = e.destroy_cancel();
            if cancelled {
                session.audit("security", "destroy cancel");
            }
            i32::from(cancelled)
        }
        Err(e) => map_err(&e),
    }
}

/// 冲突解决：保留 keep_id，删除 drop_id（secure=1 先覆写）。
///
/// # Safety
/// `handle` 有效。
#[no_mangle]
pub unsafe extern "C" fn vault_core_p2p_conflict_resolve(
    handle: *mut Session,
    keep_id: i64,
    drop_id: i64,
    secure: i32,
) -> i32 {
    let run = || -> Result<(), i32> {
        let session = unsafe { &*handle };
        session
            .with_vault(|v| {
                v.delete_file(drop_id as u64, secure != 0)
                    .map_err(CoreError::Internal)
            })
            .map_err(|e| map_err(&e))?;
        let _ = keep_id;
        unsafe {
            audit_op(
                handle,
                "vault",
                &format!("conflict resolve keep={keep_id} drop={drop_id}"),
            )
        };
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
