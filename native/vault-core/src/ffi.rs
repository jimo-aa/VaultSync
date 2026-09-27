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
// V2.0 扩展码（docs/v2.0/01 §6.7）
pub const ERR_LEASE_BUSY: i32 = 9;
/// 维护态（P7-3 轮换会话接线；契约位先占）
#[allow(dead_code)]
pub const ERR_MAINTENANCE: i32 = 10;
/// 需要恢复（P7-3 接线；契约位先占）
#[allow(dead_code)]
pub const ERR_NEEDS_RECOVERY: i32 = 11;
pub const ERR_CANCELLED: i32 = 12;
pub const ERR_CAPABILITY: i32 = 13;

fn map_err(e: &CoreError) -> i32 {
    match e {
        CoreError::NeedsRecovery(_) => ERR_NEEDS_RECOVERY,
        CoreError::Maintenance => ERR_MAINTENANCE,
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
    // P7-8：CAP_PQ_HYBRID 随本引擎永久置位 → 本端接受 suite 2 容器（读门控，
    // docs/v2.0/11 行 171）。写路径在 T2 前恒写 suite 1（偏差见 LOG 2026-09-22）。
    vault_vault::container::set_pq_suite_accepted(true);
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
                s.attach_lease();
                // P7-3/P7-4 自动续做：先消费对端轮换通知（如有），再续做自有轮换
                // （均失败不阻塞解锁，如实记诊断）
                if let Err(e) = service::consume_rotation_notice(&s, pwd) {
                    if !matches!(e, CoreError::Format("rotation notice mk unwrap failed")) {
                        // 解封失败保持沉默（对端伪造/不同源属正常拒绝路径，已在对端记审计）
                    }
                    eprintln!("vsync rotation inbound consume: {:?}", e);
                }
                if s.is_maintenance() {
                    match service::rotate_mk_resume(&s, pwd) {
                        Ok(_) => {}
                        Err(e) => eprintln!("vsync rotation auto-resume: {:?}", e),
                    }
                }
                unsafe { handle_out.write(Box::into_raw(Box::new(s))) };
                crate::contract::publish_simple(crate::contract::EV_ENGINE_READY);
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
                s.attach_lease();
                unsafe { handle_out.write(Box::into_raw(Box::new(s))) };
                crate::contract::publish_simple(crate::contract::EV_ENGINE_READY);
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
        unsafe { ensure_writable(handle) }?;
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
        crate::contract::publish_simple(crate::contract::EV_LOCKED);
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

/// 只读降级门禁（P6-2/§3.5 只读矩阵）：只读会话上一切「改库内字节」的操作返回 9。
unsafe fn ensure_writable(handle: *mut Session) -> Result<(), i32> {
    if handle.is_null() {
        return Err(ERR_INVALID_ARG);
    }
    let session = unsafe { &*handle };
    if session.is_readonly() {
        return Err(ERR_LEASE_BUSY);
    }
    Ok(())
}

unsafe fn audit_op(handle: *mut Session, kind: &str, detail: &str) {
    if handle.is_null() {
        return;
    }
    let session = unsafe { &*handle };
    session.audit(kind, detail);
    // 既有审计点 = 保险箱变更点：同步发布 VAULT_CHANGED（P6-6 事件流）
    crate::contract::publish(
        crate::contract::EV_VAULT_CHANGED,
        0,
        serde_json::json!({"kind": kind, "detail": detail}),
    );
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
        unsafe { ensure_writable(handle) }?;
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
        unsafe { ensure_writable(handle) }?;
        let q = unsafe { cstr(query) }?;
        let json = vault_op(handle, |v, _| Ok(v.search(q)))?;
        let c = CString::new(json).map_err(|_| ERR_INTERNAL)?;
        Ok(c.into_raw())
    };
    run().unwrap_or(std::ptr::null_mut())
}

/// 检索 V2（P7-9，docs/v2.0/01 §六.13）：`vault_core_vault_search_v2(handle, query,
/// opts_json, out_json)`。opts_json 可为 null（默认 limit=32 / rankVer=1）；载荷
/// `{"schema":1,"total":N,"rankVer":1,"hits":[…]}`。content 类命中 spans 为空
/// （片段重算需明文，归外壳按需补齐——诚实边界）。只读会话可用（检索在只读矩阵）。
///
/// # Safety
/// `handle` 有效；`query` / `opts_json` 合法 UTF-8（可 null）；`out_json` 非空，
/// 成功时写入引擎分配的 CString（用 `vault_core_free_string` 归还）。
#[no_mangle]
pub unsafe extern "C" fn vault_core_vault_search_v2(
    handle: *mut Session,
    query: *const c_char,
    opts_json: *const c_char,
    out_json: *mut *mut c_char,
) -> i32 {
    if handle.is_null() || out_json.is_null() {
        return ERR_INVALID_ARG;
    }
    let run = || -> Result<(), i32> {
        let q = unsafe { cstr(query) }?;
        let opts: vault_vault::index::SearchV2Opts = if opts_json.is_null() {
            Default::default()
        } else {
            let raw = unsafe { cstr(opts_json) }?;
            if raw.trim().is_empty() {
                Default::default()
            } else {
                serde_json::from_str(raw).map_err(|_| ERR_INVALID_ARG)?
            }
        };
        let json = vault_op(handle, |v, _| {
            serde_json::to_string(&v.search_v2(q, &opts))
                .map_err(|_| CoreError::Internal("search v2 serialize"))
        })?;
        let c = CString::new(json).map_err(|_| ERR_INTERNAL)?;
        unsafe { out_json.write(c.into_raw()) };
        Ok(())
    };
    run().err().unwrap_or(OK)
}

/// 擦除强度分级探测（P7-5，docs/v2.0/02 §6.2①）：`vault_core_erase_class_probe(path)`
/// → JSON `{mediaKind, eraseClass, methodBits, degradations[], secureEraseClaim}`。
/// 探测失败绝不失败（unknown + crypto_only + 降级说明，原则 7 的如实降级）。
/// `secureEraseClaim` = 「安全擦除」字样的唯一判据（当前恒 false，TRIM 未接线）。
///
/// # Safety
/// `path` 必须为合法 UTF-8 NUL 结尾 C 字符串指针。
#[no_mangle]
pub unsafe extern "C" fn vault_core_erase_class_probe(path: *const c_char) -> *mut c_char {
    let run = || -> Result<*mut c_char, i32> {
        let p = unsafe { path_of(path) }?;
        let report = vault_vault::erase::probe(&p);
        let claim = vault_vault::erase::SECURE_ERASE_CLAIM_ALLOWED(&report);
        Ok(json_out(serde_json::json!({
            "mediaKind": report.media_kind,
            "eraseClass": report.erase_class,
            "methodBits": report.method_bits,
            "degradations": report.degradations,
            "secureEraseClaim": claim,
        })))
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
        unsafe { ensure_writable(handle) }?;
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
        unsafe { ensure_writable(handle) }?;
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
        unsafe { ensure_writable(handle) }?;
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
        unsafe { ensure_writable(handle) }?;
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
        unsafe { ensure_writable(handle) }?;
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
    // P8-10：开关落盘 `settings.enc`（VSSG 族之外的单文件 AEAD，key HKDF(MK,"settings")）——
    // 解锁后引擎自读，不再依赖 UI 每次同步（docs/05-05 §5.3）。**读改写**：
    // 同一载体还承载中继候选等其它设置（P8-5 收尾），整存整取会抹掉它们。
    let mut s = crate::settings::Settings::load(&session.data_dir(), &session.mk);
    s.schema = 1;
    s.stego_enabled = enabled;
    let persisted = s.save(&session.data_dir(), &session.mk).is_ok();
    session.audit(
        "security",
        if enabled {
            "stego enable"
        } else {
            "stego disable"
        },
    );
    // 落盘失败如实报 IO（内存开关已改：本次会话仍生效，重启后丢失）
    if persisted {
        OK
    } else {
        ERR_IO
    }
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

/// P8-8：原地编辑——以**同一 `file_id`** 提交新版本（docs/05-03 §6.4）。
/// `opts_json` = `{"keepRevs":1}`（默认 1 = 只保留上一版）。旧版本容器保留为
/// `files/<id>.r<rev>.vse`（三方合并基线 + `keepRevs` 清理）。
/// 返回 0；7 = 参数（含 opts 解析失败）；3 = 源不可读 / IO；9 = 只读；10 = 维护态。
/// 新 `rev` 经 `vault_list` / 同步清单的 `rev` 字段可见。
///
/// # Safety
/// `handle` 有效；`src` 合法路径；`opts_json` 可为 null。
#[no_mangle]
pub unsafe extern "C" fn vault_core_vault_apply_edit(
    handle: *mut Session,
    file_id: i64,
    src: *const c_char,
    opts_json: *const c_char,
) -> i32 {
    let run = || -> Result<(), i32> {
        unsafe { ensure_writable(handle) }?;
        let session = unsafe { &*handle };
        if session.is_maintenance() {
            return Err(ERR_MAINTENANCE);
        }
        let src = unsafe { cstr(src) }?;
        let keep = if opts_json.is_null() {
            1u32
        } else {
            let opts: serde_json::Value =
                serde_json::from_str(unsafe { cstr(opts_json) }?).map_err(|_| ERR_INVALID_ARG)?;
            opts["keepRevs"].as_u64().unwrap_or(1).min(64) as u32
        };
        let rev = session
            .with_vault(|v| {
                v.apply_edit(file_id as u64, std::path::Path::new(src), keep)
                    .map_err(CoreError::Internal)
            })
            .map_err(|e| map_err(&e))?;
        session.audit("vault", &format!("apply_edit id={file_id} rev={rev}"));
        Ok(())
    };
    run().err().unwrap_or(OK)
}

// ==== P8-10 隐写跨图分割与容量协商（docs/05-05 §三）====

/// 容量协商的纯计算部分（与 FFI 解耦，便于单测）。
/// `opts`：`{fillRatio, disperse, shuffle, codec, maxBytes}`。
fn stego_plan_value(
    file_len: u64,
    name: &str,
    images: &[String],
    opts: &serde_json::Value,
) -> serde_json::Value {
    let fill = opts["fillRatio"].as_f64().unwrap_or(0.8).clamp(0.05, 1.0);
    let max_bytes = opts["maxBytes"]
        .as_u64()
        .unwrap_or(vault_stego::multi::DEFAULT_MAX_BYTES);
    let dispersed = opts["disperse"].as_bool().unwrap_or(true);
    let mut notes: Vec<String> = Vec::new();
    if opts["codec"].as_str() == Some("auto") {
        // JPEG 鲁棒路线不做：`auto` 一律解析为 png 并如实标注（05-05 §3.4）
        notes.push("codec_fallback=png".into());
    }
    if !dispersed {
        notes.push("disperse=off".into());
    }
    if opts["shuffle"].as_bool().unwrap_or(false) {
        notes.push("shuffle=on".into());
    }
    if file_len > max_bytes {
        notes.push(format!("file exceeds maxBytes({max_bytes})"));
    }

    let first_overhead = (vault_stego::multi::SHARD_HEADER_LEN + 2 + name.len() + 32) as u64;
    let mut per = Vec::new();
    let mut net: Vec<u64> = Vec::new();
    let mut read_error = false;
    for (i, p) in images.iter().enumerate() {
        match std::fs::read(p)
            .ok()
            .and_then(|b| vault_stego::png_size(&b).ok())
        {
            Some((w, h)) => {
                let cap = vault_stego::capacity_bytes(w, h) as u64;
                let usable = ((cap as f64) * fill).floor() as u64;
                let overhead = if i == 0 {
                    first_overhead
                } else {
                    vault_stego::multi::SHARD_HEADER_LEN as u64
                };
                let shard_bytes = usable.saturating_sub(overhead);
                net.push(shard_bytes);
                per.push(serde_json::json!({
                    "index": i, "width": w, "height": h,
                    "capacity": cap, "usable": usable, "shardBytes": shard_bytes,
                }));
            }
            None => {
                read_error = true;
                notes.push(format!("image {i} unreadable"));
                per.push(serde_json::json!({"index": i, "error": "unreadable image"}));
            }
        }
    }
    // 最少图数：贪心前缀和；已给图不够时按平均净容量外推「还需要几张」
    let mut acc = 0u64;
    let mut needed = 0usize;
    for n in &net {
        acc += n;
        needed += 1;
        if acc >= file_len {
            break;
        }
    }
    if acc < file_len && !net.is_empty() {
        let per = net.iter().sum::<u64>() / (net.len() as u64);
        if per > 0 {
            needed += (file_len - acc).div_ceil(per) as usize;
        }
    }
    let given = images.len();
    let fits = !read_error && file_len <= max_bytes && acc >= file_len;
    let shortfall = file_len.saturating_sub(acc);
    serde_json::json!({
        "schema": 1,
        "fileLen": file_len,
        "imagesGiven": given,
        "imagesNeeded": needed,
        "fits": fits,
        "shortfallBytes": shortfall,
        "fillRatio": fill,
        "disperse": dispersed,
        "perImage": per,
        "notes": notes,
    })
}

/// `embed_multi` 的输出路径（**不覆盖源图**：同目录 `<stem>.stego.png`）。
fn stego_out_path(p: &str) -> String {
    let path = std::path::Path::new(p);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("image");
    let dir = path.parent().unwrap_or(std::path::Path::new("."));
    dir.join(format!("{stem}.stego.png"))
        .to_string_lossy()
        .to_string()
}

/// P8-10：隐写容量协商（**容量的唯一真值来源**，结果不缓存；docs/05-05 §3.3）。
/// `images_json` = 图片路径数组；`opts_json` = `{fillRatio, disperse, shuffle, codec, maxBytes}`；
/// `out_json` = `{schema,fileLen,imagesGiven,imagesNeeded,fits,shortfallBytes,perImage[],notes[]}`。
/// 返回 0；7 = 未启用 / 参数非法（含空图集）；3 = 文件信息读取失败。
/// `fits=false` 是**正常结果**（如实拒绝），不是错误码。
///
/// # Safety
/// `handle` 有效；`images_json`、`out_json` 非空；`opts_json` 可为 null。
#[no_mangle]
pub unsafe extern "C" fn vault_core_stego_plan(
    handle: *mut Session,
    file_id: i64,
    images_json: *const c_char,
    opts_json: *const c_char,
    out_json: *mut *mut c_char,
) -> i32 {
    if handle.is_null() || images_json.is_null() || out_json.is_null() {
        return ERR_INVALID_ARG;
    }
    let run = || -> Result<serde_json::Value, i32> {
        let session = unsafe { &*handle };
        if !session
            .stego_enabled
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            return Err(ERR_INVALID_ARG);
        }
        let images: Vec<String> =
            serde_json::from_str(unsafe { cstr(images_json) }?).map_err(|_| ERR_INVALID_ARG)?;
        if images.is_empty() {
            return Err(ERR_INVALID_ARG);
        }
        let opts: serde_json::Value = if opts_json.is_null() {
            serde_json::json!({})
        } else {
            serde_json::from_str(unsafe { cstr(opts_json) }?).map_err(|_| ERR_INVALID_ARG)?
        };
        let file_len = session
            .with_vault(|v| v.file_size(file_id as u64).map_err(CoreError::Internal))
            .map_err(|e| map_err(&e))?;
        let name = session
            .with_vault(|v| v.file_name(file_id as u64).map_err(CoreError::Internal))
            .unwrap_or_else(|_| format!("file-{file_id}"));
        Ok(stego_plan_value(file_len, &name, &images, &opts))
    };
    match run() {
        Ok(v) => {
            let Ok(c) = CString::new(v.to_string()) else {
                return ERR_INTERNAL;
            };
            unsafe { out_json.write(c.into_raw()) };
            OK
        }
        Err(e) => e,
    }
}

/// P8-10：跨图嵌入（每图一片；bit1 单图模式即 <1 图> = 单图）。
/// 图片**不覆盖源文件**，产出同目录 `<stem>.stego.png`（`out_json.images[]` 给出实际路径）。
/// 走后台任务（`out_task_id`）：`task_cancel` 在图片边界收敛为 `12`。
/// `out_json` 立即返回 `{taskId, images:[…]}`（产物路径可预知）。
///
/// # Safety
/// `handle` 有效；`images_json`、`out_json` 非空；`opts_json` 可为 null；`out_task_id` 可为 null。
#[no_mangle]
pub unsafe extern "C" fn vault_core_stego_embed_multi(
    handle: *mut Session,
    file_id: i64,
    images_json: *const c_char,
    opts_json: *const c_char,
    out_json: *mut *mut c_char,
    out_task_id: *mut u32,
) -> i32 {
    if handle.is_null() || images_json.is_null() || out_json.is_null() {
        return ERR_INVALID_ARG;
    }
    let run = || -> Result<(serde_json::Value, u32), i32> {
        unsafe { ensure_writable(handle) }?;
        let session = unsafe { &*handle };
        if session.is_maintenance() {
            return Err(ERR_MAINTENANCE);
        }
        if !session
            .stego_enabled
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            return Err(ERR_INVALID_ARG);
        }
        let images: Vec<String> =
            serde_json::from_str(unsafe { cstr(images_json) }?).map_err(|_| ERR_INVALID_ARG)?;
        if images.is_empty() {
            return Err(ERR_INVALID_ARG);
        }
        let opts: serde_json::Value = if opts_json.is_null() {
            serde_json::json!({})
        } else {
            serde_json::from_str(unsafe { cstr(opts_json) }?).map_err(|_| ERR_INVALID_ARG)?
        };
        let dispersed = opts["disperse"].as_bool().unwrap_or(true);
        let max_bytes = opts["maxBytes"]
            .as_u64()
            .unwrap_or(vault_stego::multi::DEFAULT_MAX_BYTES);

        let (plain, name) = read_plain_for_stego(session, file_id)?;
        if plain.len() as u64 > max_bytes {
            return Err(ERR_INVALID_ARG);
        }
        // 逐图净容量（与 `_plan` 同口径：唯一真值来源是同一段计算）
        let plan = stego_plan_value(plain.len() as u64, &name, &images, &opts);
        if plan["fits"] != serde_json::json!(true) {
            return Err(ERR_INVALID_ARG);
        }
        // 逐图净容量（与 `_plan` 同口径：唯一真值来源是同一段计算）；
        // `split_and_encode` 收到的是**可用容量**（它自己再扣分片头开销）
        let mut caps: Vec<usize> = Vec::new();
        for entry in plan["perImage"].as_array().into_iter().flatten() {
            let usable = entry["usable"].as_u64().unwrap_or(0).min(usize::MAX as u64);
            caps.push(usable as usize);
        }
        let frames = vault_stego::multi::split_and_encode(&name, &plain, &caps, dispersed)
            .map_err(|_| ERR_INVALID_ARG)?;
        let outs: Vec<String> = images.iter().map(|p| stego_out_path(p)).collect();
        let key = session.stego_key();

        // 后台任务：逐图加密 + LSB 嵌入（图片边界检查点 → 可 task_cancel）
        let images_moved = images.clone();
        let outs_moved = outs.clone();
        let task_id = crate::contract::spawn_task("stego", "interactive", move |ctl, _p| {
            for (i, frame) in frames.iter().enumerate() {
                if ctl.checkpoint().is_err() {
                    return Err("cancelled".to_string());
                }
                let nonce_v = vault_crypto::random::random_bytes(vault_crypto::AES_GCM_NONCE_LEN);
                let mut nonce = [0u8; vault_crypto::AES_GCM_NONCE_LEN];
                nonce.copy_from_slice(&nonce_v);
                let sealed = vault_crypto::aead::aead_encrypt(&key, &nonce, frame)
                    .map_err(|e| e.to_string())?;
                let image = std::fs::read(&images_moved[i]).map_err(|e| e.to_string())?;
                let out = vault_stego::embed_spread(&image, &sealed, dispersed)
                    .map_err(|e| e.to_string())?;
                std::fs::write(&outs_moved[i], out).map_err(|e| e.to_string())?;
            }
            Ok(serde_json::json!({"images": outs_moved}))
        });
        session.audit(
            "security",
            &format!("stego embed multi id={file_id} shards={}", images.len()),
        );
        Ok((
            serde_json::json!({"taskId": task_id, "images": outs, "shards": images.len()}),
            task_id,
        ))
    };
    match run() {
        Ok((v, id)) => {
            let Ok(c) = CString::new(v.to_string()) else {
                return ERR_INTERNAL;
            };
            unsafe { out_json.write(c.into_raw()) };
            if !out_task_id.is_null() {
                unsafe { out_task_id.write(id) };
            }
            OK
        }
        Err(e) => e,
    }
}

/// P8-10：跨图提取（乱序可、重复索引拒绝、缺片报 `missing[]`）。
/// V1.0 单图载荷（首字节 `0x00`）仍可提取（**单向兼容**）。
/// `out_json` = `{name, size, shards}`；失败时写 `{"error":…,"missing":[…]}`（若有结构信息）并返回码。
/// **终验在写盘之前完成**，故不存在「半成品目标文件」需要删除（比「先写后删」更强）。
///
/// # Safety
/// `handle` 有效；`images_json`、`dest`、`out_json` 非空。
#[no_mangle]
pub unsafe extern "C" fn vault_core_stego_extract_multi(
    handle: *mut Session,
    images_json: *const c_char,
    dest: *const c_char,
    out_json: *mut *mut c_char,
) -> i32 {
    if handle.is_null() || images_json.is_null() || dest.is_null() || out_json.is_null() {
        return ERR_INVALID_ARG;
    }
    let mut diag: Option<serde_json::Value> = None;
    let mut run = || -> Result<serde_json::Value, i32> {
        let session = unsafe { &*handle };
        if !session
            .stego_enabled
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            return Err(ERR_INVALID_ARG);
        }
        let images: Vec<String> =
            serde_json::from_str(unsafe { cstr(images_json) }?).map_err(|_| ERR_INVALID_ARG)?;
        if images.is_empty() {
            return Err(ERR_INVALID_ARG);
        }
        let dest = unsafe { cstr(dest) }?;
        let key = session.stego_key();
        let mut shards: Vec<vault_stego::multi::Shard> = Vec::new();
        for p in &images {
            let image = std::fs::read(p).map_err(|_| ERR_IO)?;
            // V2 分片载荷恒为分散布局；解不开则按 V1 单图载荷处理
            let sealed = vault_stego::extract_spread(&image, true).map_err(|_| ERR_FORMAT)?;
            let plain =
                vault_crypto::aead::aead_decrypt(&key, &sealed).ok_or(ERR_WRONG_PASSWORD)?;
            match plain.first() {
                Some(&vault_stego::multi::PAYLOAD_VER_V2) => {
                    let s = vault_stego::multi::parse_shard(&plain, u64::MAX)
                        .map_err(|_| ERR_FORMAT)?;
                    shards.push(s);
                }
                Some(&0x00) => {
                    // V1 布局：[u32 name_len][name][data]（单图，单向兼容）
                    if plain.len() < 4 {
                        return Err(ERR_FORMAT);
                    }
                    let name_len =
                        u32::from_be_bytes(plain[0..4].try_into().map_err(|_| ERR_FORMAT)?)
                            as usize;
                    if plain.len() < 4 + name_len {
                        return Err(ERR_FORMAT);
                    }
                    let name = String::from_utf8_lossy(&plain[4..4 + name_len]).to_string();
                    let data = &plain[4 + name_len..];
                    std::fs::write(dest, data).map_err(|_| ERR_IO)?;
                    session.audit("security", "stego extract (v1 payload)");
                    return Ok(serde_json::json!({
                        "name": name, "size": data.len(), "shards": 1,
                        "payloadVer": 1,
                    }));
                }
                _ => return Err(ERR_FORMAT),
            }
        }
        let total = shards.first().map(|s| s.total).unwrap_or(0);
        match vault_stego::multi::reassemble(&shards) {
            Ok((hdr, data)) => {
                std::fs::write(dest, &data).map_err(|_| ERR_IO)?;
                session.audit("security", "stego extract multi");
                Ok(serde_json::json!({
                    "name": hdr.name, "size": data.len(), "shards": total,
                    "payloadVer": 2,
                }))
            }
            Err(e) => {
                let mut v = serde_json::json!({"error": e.as_str()});
                if let vault_stego::multi::ReassembleError::Missing(m) = &e {
                    v["missing"] = serde_json::json!(m);
                }
                diag = Some(v);
                Err(ERR_FORMAT)
            }
        }
    };
    match run() {
        Ok(v) => {
            let Ok(c) = CString::new(v.to_string()) else {
                return ERR_INTERNAL;
            };
            unsafe { out_json.write(c.into_raw()) };
            OK
        }
        Err(e) => {
            // 结构化失败（缺片 / 重复）把诊断写进 out_json，便于 UI 如实呈现
            if let Some(d) = diag {
                if let Ok(c) = CString::new(d.to_string()) {
                    unsafe { out_json.write(c.into_raw()) };
                }
            }
            e
        }
    }
}

/// 导出保险箱文件明文 + 原文件名（隐写入口共用；临时明文即读即删）。
fn read_plain_for_stego(session: &Session, file_id: i64) -> Result<(Vec<u8>, String), i32> {
    let tmp = session.data_dir().join(format!("stego-{file_id}.tmp"));
    session
        .with_vault(|v| {
            v.export_file(file_id as u64, &tmp)
                .map_err(CoreError::Internal)
        })
        .map_err(|e| map_err(&e))?;
    let plain = std::fs::read(&tmp).map_err(|_| ERR_IO)?;
    let _ = std::fs::remove_file(&tmp);
    let name = session
        .with_vault(|v| v.file_name(file_id as u64).map_err(CoreError::Internal))
        .unwrap_or_else(|_| format!("file-{file_id}"));
    Ok((plain, name))
}

// ==== P5-5 全设备联动销毁：离线设备高优先级信令队列（docs/05-06 §5.1）====
/// 为**每一个**已配对设备各签发一条销毁指令并入队（离线设备下次上线时投递）。
/// 返回 JSON {"queued":N,"peers":["vd-…"]}。
///
/// # Safety
/// `handle` 有效。
#[no_mangle]
pub unsafe extern "C" fn vault_core_p2p_destroy_queue_all(
    handle: *mut Session,
    delay_secs: u64,
) -> *mut c_char {
    let run = || -> Result<serde_json::Value, i32> {
        unsafe { ensure_writable(handle) }?;
        let session = unsafe { &*handle };
        let engine = crate::p2p_service::p2p_engine(session).map_err(|e| map_err(&e))?;
        let v = engine
            .destroy_queue_all(delay_secs)
            .map_err(|_| ERR_INTERNAL)?;
        let n = v.get("queued").and_then(|x| x.as_u64()).unwrap_or(0);
        session.audit(
            "security",
            &format!("destroy queue all peers={n} delay={delay_secs}s"),
        );
        Ok(v)
    };
    match run() {
        Ok(v) => json_out(v),
        Err(_) => std::ptr::null_mut(),
    }
}

/// 队列快照 JSON {"count":N,"items":[{peer,issuedMs,delaySecs}]}。
///
/// # Safety
/// `handle` 有效。
#[no_mangle]
pub unsafe extern "C" fn vault_core_p2p_pending_orders(handle: *mut Session) -> *mut c_char {
    let run = || -> Result<serde_json::Value, i32> {
        let session = unsafe { &*handle };
        let engine = crate::p2p_service::p2p_engine(session).map_err(|e| map_err(&e))?;
        Ok(engine.pending_orders())
    };
    match run() {
        Ok(v) => json_out(v),
        Err(_) => std::ptr::null_mut(),
    }
}

/// 清空待投递队列，返回清除条数。
///
/// # Safety
/// `handle` 有效。
#[no_mangle]
pub unsafe extern "C" fn vault_core_p2p_cancel_orders(handle: *mut Session) -> i32 {
    let run = || -> Result<i32, i32> {
        unsafe { ensure_writable(handle) }?;
        let session = unsafe { &*handle };
        let engine = crate::p2p_service::p2p_engine(session).map_err(|e| map_err(&e))?;
        let n = engine.cancel_pending_orders();
        if n > 0 {
            session.audit("security", &format!("destroy queue cleared orders={n}"));
        }
        i32::try_from(n).map_err(|_| ERR_INTERNAL)
    };
    run().unwrap_or_else(|e| -e)
}

/// 记录 / 更新对端监听地址（配对时自动记录；此接口是手填兜底）。
///
/// # Safety
/// `handle` 有效；`device_id`、`addr` 为合法 UTF-8。
#[no_mangle]
pub unsafe extern "C" fn vault_core_p2p_remember_addr(
    handle: *mut Session,
    device_id: *const c_char,
    addr: *const c_char,
) -> i32 {
    let run = || -> Result<(), i32> {
        let id = unsafe { cstr(device_id) }?;
        let a = unsafe { cstr(addr) }?;
        let session = unsafe { &*handle };
        let engine = crate::p2p_service::p2p_engine(session).map_err(|e| map_err(&e))?;
        engine
            .remember_peer_addr(id, a)
            .map_err(|_| ERR_INVALID_ARG)?;
        session.audit("device", &format!("peer addr updated peer={id}"));
        Ok(())
    };
    run().err().unwrap_or(OK)
}

// ==== P5-4 密钥轮换（docs/07）====

/// 单文件 FSKey 轮换：以全新随机 FSKey 重写该文件容器（其余文件不受影响）。
///
/// # Safety
/// `handle` 有效。
#[no_mangle]
pub unsafe extern "C" fn vault_core_vault_rotate_file_key(
    handle: *mut Session,
    file_id: i64,
) -> i32 {
    let run = || -> Result<(), i32> {
        unsafe { ensure_writable(handle) }?;
        if handle.is_null() {
            return Err(ERR_INVALID_ARG);
        }
        let session = unsafe { &*handle };
        session
            .with_vault(|v| {
                v.rotate_file_key(file_id as u64)
                    .map(|_| ())
                    .map_err(CoreError::Internal)
            })
            .map_err(|e| map_err(&e))?;
        unsafe { audit_op(handle, "security", &format!("rotate file key id={file_id}")) };
        Ok(())
    };
    run().err().unwrap_or(OK)
}

/// 文件夹 FSK 轮换：新 FSK 覆盖 + 重写该文件夹直属文件的容器。
///
/// # Safety
/// `handle` 有效。
#[no_mangle]
pub unsafe extern "C" fn vault_core_vault_rotate_folder_keys(
    handle: *mut Session,
    folder_id: i64,
) -> i32 {
    let run = || -> Result<(), i32> {
        unsafe { ensure_writable(handle) }?;
        if handle.is_null() {
            return Err(ERR_INVALID_ARG);
        }
        let session = unsafe { &*handle };
        let n = session
            .with_vault(|v| {
                v.rotate_folder_keys(folder_id as u64)
                    .map_err(CoreError::Internal)
            })
            .map_err(|e| map_err(&e))?;
        unsafe {
            audit_op(
                handle,
                "security",
                &format!("rotate folder keys folder={folder_id} files={n}"),
            )
        };
        Ok(())
    };
    run().err().unwrap_or(OK)
}

/// MK 全库轮换（强确认：需重新输入主密码）。成功后**必须**立即 lock 并重新解锁
/// （会话内的 MK 副本已陈旧）。既有阅后即焚分享一并作废。
///
/// # Safety
/// `handle` 有效；`password` 为合法 UTF-8。
#[no_mangle]
pub unsafe extern "C" fn vault_core_rotate_mk(
    handle: *mut Session,
    password: *const c_char,
) -> i32 {
    let run = || -> Result<(), i32> {
        unsafe { ensure_writable(handle) }?;
        if handle.is_null() {
            return Err(ERR_INVALID_ARG);
        }
        let pwd = unsafe { cstr(password) }?;
        let session = unsafe { &*handle };
        let store = open_os_store();
        let store_ref: Option<&dyn crate::platform_store::SecureStore> = store
            .as_ref()
            .map(|s| s as &dyn crate::platform_store::SecureStore);
        service::rotate_mk(session, pwd, store_ref).map_err(|e| map_err(&e))?;
        Ok(())
    };
    run().err().unwrap_or(OK)
}

// ==== P5-7 更新清单验签与安装包哈希（客户端分发通道；发布方离线签名）====

/// 校验更新清单签名：`manifest` 为**清单原始字节**（服务端下发的那份），
/// `sig_hex` 为其 Ed25519 签名，`pubkey_hex` 为客户端内置的发布公钥。
/// 返回 JSON `{"ok":true}` / `{"ok":false}`（不区分格式错与签名错，
/// 客户端对两者都必须按「不可信」处理）。
///
/// # Safety
/// 三个参数均为合法 UTF-8 C 字符串。
#[no_mangle]
pub unsafe extern "C" fn vault_core_verify_update_manifest(
    manifest: *const c_char,
    sig_hex: *const c_char,
    pubkey_hex: *const c_char,
) -> *mut c_char {
    let run = || -> Result<serde_json::Value, i32> {
        let msg = unsafe { cstr(manifest) }?;
        let sig = unsafe { cstr(sig_hex) }?;
        let pk = unsafe { cstr(pubkey_hex) }?;
        let ok = vault_crypto::sig::verify_hex(pk, msg.as_bytes(), sig);
        Ok(serde_json::json!({ "ok": ok }))
    };
    match run() {
        Ok(v) => json_out(v),
        Err(_) => std::ptr::null_mut(),
    }
}

/// 计算文件 SHA-256（hex）：下载完成后校验安装包与清单里声明的 sha256。
/// 失败返回 null。
///
/// # Safety
/// `path` 为合法 UTF-8 C 字符串。
#[no_mangle]
pub unsafe extern "C" fn vault_core_file_sha256(path: *const c_char) -> *mut c_char {
    let run = || -> Result<String, i32> {
        let p = unsafe { cstr(path) }?;
        vault_crypto::sig::sha256_file_hex(std::path::Path::new(p)).map_err(|_| ERR_IO)
    };
    match run() {
        Ok(hex) => match std::ffi::CString::new(hex) {
            Ok(c) => c.into_raw(),
            Err(_) => std::ptr::null_mut(),
        },
        Err(_) => std::ptr::null_mut(),
    }
}

// ==== P5-2 全库完整性校验（安全中心「文件完整性」的真实数据源）====

/// 全库完整性校验：逐个容器重算块哈希并与密文清单比对（含 GCM 认证）。
/// 返回 JSON `{"files":N,"checked":M,"failed":[{"id","name","reason"}]}`。
/// 无失败项时也返回结果（UI 据此显示「正常 · M / N 校验通过」）。
///
/// 注意：逐块重算，耗时与数据量成正比；调用方应放在后台 Isolate 并提示扫描中。
///
/// # Safety
/// `handle` 必须是未释放的有效会话句柄。
#[no_mangle]
pub unsafe extern "C" fn vault_core_vault_verify_all(handle: *mut Session) -> *mut c_char {
    let run = || -> Result<serde_json::Value, i32> {
        if handle.is_null() {
            return Err(ERR_INVALID_ARG);
        }
        let session = unsafe { &*handle };
        let manifest = session
            .with_vault(|v| v.sync_manifest().map_err(CoreError::Internal))
            .map_err(|e| map_err(&e))?;
        let items: Vec<serde_json::Value> =
            serde_json::from_str(&manifest).map_err(|_| ERR_INTERNAL)?;
        let mut checked = 0usize;
        let mut failed: Vec<serde_json::Value> = Vec::new();
        for it in items {
            // 墓碑条目（deletedMs）没有容器，跳过
            if it.get("deletedMs").is_some() {
                continue;
            }
            let id = it.get("id").and_then(|x| x.as_u64()).unwrap_or(0);
            let name = it
                .get("name")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            match session.with_vault(|v| v.verify_file(id).map_err(CoreError::Internal)) {
                Ok(()) => checked += 1,
                Err(e) => failed.push(serde_json::json!({
                    "id": id,
                    "name": name,
                    // CoreError 已 derive(Debug)，内部原因（块哈希不匹配 / GCM 失败）原样带出
                    "reason": format!("{e:?}"),
                })),
            }
        }
        let files = checked + failed.len();
        if !failed.is_empty() {
            session.audit(
                "security",
                &format!("integrity scan failed={}/{}", failed.len(), files),
            );
        }
        Ok(serde_json::json!({
            "files": files,
            "checked": checked,
            "failed": failed,
        }))
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
        unsafe { ensure_writable(handle) }?;
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
        unsafe { ensure_writable(handle) }?;
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
        unsafe { ensure_writable(handle) }?;
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
        unsafe { ensure_writable(handle) }?;
        let session = unsafe { &*handle };
        let a = unsafe { cstr(addr) }?;
        let engine = crate::p2p_service::p2p_engine(session).map_err(|e| map_err(&e))?;
        apply_sync_policy(session, &engine);
        let v = engine.sync_with(a).map_err(|_| ERR_IO)?;
        publish_transfer_queue("done", 0, Some(&engine));
        Ok(v)
    };
    match run() {
        Ok(v) => json_out(v),
        Err(_) => std::ptr::null_mut(),
    }
}

/// 发起一次增量同步（P8-1 入队语义）：作业入后台队列（优先级 background），
/// 立即返回 task_id；摘要 JSON 经 TASK_DONE 的 `resultJson` / `task_status` 获取。
/// 作业在文件与块检查点上响应 `task_pause` / `task_cancel`。
/// 返回 0；7 = 参数非法；9 = 只读；13 = 能力未置位。
///
/// # Safety
/// `handle` 有效；`addr` 合法 UTF-8；`out_task_id` 可为 null。
#[no_mangle]
pub unsafe extern "C" fn vault_core_p2p_sync_task(
    handle: *mut Session,
    addr: *const c_char,
    out_task_id: *mut u32,
) -> i32 {
    if crate::contract::capability_bits() & crate::contract::CAP_TASKS == 0 {
        return ERR_CAPABILITY;
    }
    if handle.is_null() || addr.is_null() {
        return ERR_INVALID_ARG;
    }
    let run = || -> Result<u32, i32> {
        unsafe { ensure_writable(handle) }?;
        let session = unsafe { &*handle };
        let a = unsafe { cstr(addr) }?.to_string();
        let engine = crate::p2p_service::p2p_engine(session).map_err(|e| map_err(&e))?;
        apply_sync_policy(session, &engine);
        let eng_task = std::sync::Arc::clone(&engine);
        let id = crate::contract::spawn_task("sync", "background", move |ctl, _progress| {
            // 桥接注册表控制面 → 引擎文件/块检查点（task_pause/cancel 即时生效）
            let sync_ctl = vault_p2p::engine::SyncCtl {
                cancel: ctl.cancel_flag(),
                pause: ctl.pause_flag(),
                gate: ctl.gate(),
            };
            eng_task.set_sync_ctl(Some(sync_ctl));
            let result = eng_task.sync_with(&a);
            eng_task.set_sync_ctl(None);
            // P8-2/P8-7：完成事件（含被跳过条目）
            publish_transfer_queue("done", 0, Some(&eng_task));
            result
        });
        // P8-2：入队事件（TRANSFER_QUEUE(11)）
        publish_transfer_queue("queued", id, Some(&engine));
        Ok(id)
    };
    match run() {
        Ok(id) => {
            if !out_task_id.is_null() {
                unsafe { out_task_id.write(id) };
            }
            OK
        }
        Err(e) => e,
    }
}

/// P8-7：选择性同步策略读取（`sync-policy.enc`）。返回 JSON（`sync_policy` 载荷
/// + `fromDefault`：读取失败/缺失 → 默认全同步并如实置真）。
///
/// # Safety
/// `handle` 有效。
#[no_mangle]
pub unsafe extern "C" fn vault_core_sync_policy_get(handle: *mut Session) -> *mut c_char {
    let run = || -> Result<serde_json::Value, i32> {
        let session = unsafe { &*handle };
        let (p, from_default) = crate::settings::SyncPolicy::load(&session.data_dir(), &session.mk);
        let mut v = serde_json::to_value(&p).map_err(|_| ERR_INTERNAL)?;
        if let Some(o) = v.as_object_mut() {
            o.insert("fromDefault".into(), serde_json::json!(from_default));
        }
        Ok(v)
    };
    match run() {
        Ok(v) => json_out(v),
        Err(_) => std::ptr::null_mut(),
    }
}

/// P8-7：写入选择性同步策略。0 = OK；7 = 非法（**原子拒绝**，旧策略不受影响）；
/// 9 = 只读降级；10 = 维护态（策略属业务写入）；3 = 落盘失败。
/// 成功即注入已创建的 P2P 引擎（下一次同步立即生效）并记审计 `sync.policy`。
///
/// # Safety
/// `handle` 有效；`json` 合法 UTF-8。
#[no_mangle]
pub unsafe extern "C" fn vault_core_sync_policy_set(
    handle: *mut Session,
    json: *const c_char,
) -> i32 {
    let run = || -> Result<(), i32> {
        unsafe { ensure_writable(handle) }?;
        let session = unsafe { &*handle };
        if session.is_maintenance() {
            return Err(ERR_MAINTENANCE);
        }
        let body = unsafe { cstr(json) }?;
        let p: crate::settings::SyncPolicy =
            serde_json::from_str(body).map_err(|_| ERR_INVALID_ARG)?;
        p.validate().map_err(|_| ERR_INVALID_ARG)?;
        p.save(&session.data_dir(), &session.mk)
            .map_err(|_| ERR_IO)?;
        session.audit("sync", "policy updated");
        if let Some(e) = session
            .p2p
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            e.set_sync_filter(p.to_engine_filter());
        }
        Ok(())
    };
    run().err().unwrap_or(OK)
}

/// P8-7：外壳上报同步上下文（时段 / 当前网络类型），供引擎的第二处生效点复核。
/// JSON `{"inWindow":true,"netType":"wifi"}`；未知 netType 一律按「未上报」放行。
/// 0 = OK；7 = 参数非法。
///
/// # Safety
/// `handle` 有效；`json` 合法 UTF-8。
#[no_mangle]
pub unsafe extern "C" fn vault_core_set_sync_context(
    handle: *mut Session,
    json: *const c_char,
) -> i32 {
    let run = || -> Result<(), i32> {
        let session = unsafe { &*handle };
        let body = unsafe { cstr(json) }?;
        let v: serde_json::Value = serde_json::from_str(body).map_err(|_| ERR_INVALID_ARG)?;
        let ctx = vault_p2p::policy::SyncContext {
            in_window: v["inWindow"].as_bool().unwrap_or(true),
            net_type: v["netType"].as_str().unwrap_or("").to_string(),
        };
        *session.sync_ctx.lock().unwrap_or_else(|e| e.into_inner()) = ctx.clone();
        if let Some(e) = session
            .p2p
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            e.set_sync_context(ctx);
        }
        Ok(())
    };
    run().err().unwrap_or(OK)
}

/// P8-7/P8-8：把策略与上下文注入引擎（每次同步前调用——策略改动无需重建引擎），
/// 并按 `conflict.keepWindowDays` 顺带清理超期冲突副本。
fn apply_sync_policy(session: &Session, engine: &vault_p2p::P2pEngine) {
    let (p, _from_default) = crate::settings::SyncPolicy::load(&session.data_dir(), &session.mk);
    engine.set_sync_filter(p.to_engine_filter());
    let ctx = session
        .sync_ctx
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    engine.set_sync_context(ctx);
    let _ = engine.sweep_expired_conflicts(p.conflict.keep_window_days);
}

/// P8-2/P8-7：传输队列事件（`TRANSFER_QUEUE`(11)）——排队/完成计数 + 被跳过条目。
fn publish_transfer_queue(phase: &str, task_id: u32, engine: Option<&vault_p2p::P2pEngine>) {
    let skipped = engine.map(|e| e.skipped()).unwrap_or(serde_json::json!([]));
    let payload = match phase {
        "queued" => serde_json::json!({"phase": "queued", "queued": 1, "running": 0, "done": 0}),
        _ => {
            serde_json::json!({"phase": "done", "queued": 0, "running": 0, "done": 1, "skipped": skipped})
        }
    };
    crate::contract::publish(crate::contract::EV_TRANSFER_QUEUE, task_id, payload);
}

/// 发起一次经中继的增量同步（发起端，P8-5 VSR2）。返回摘要 JSON。
/// 失败：7 = 参数（房间名 / token 不合规，本地前置拦截）；3 = 网络 / 鉴权
/// （中继 R 码已映射为本机码，可读诊断进引擎事件日志，**不把 R 码原样上抛**）；
/// 13 = 中继受限 / 不可用——同时发布 `PATH_DEGRADED`(13) 契约事件
/// （reason ∈ relay_limit | relay_unavailable，docs/08 §3.4 映射纪律）。
///
/// # Safety
/// `handle` 有效；`relay`、`room`、`token` 合法 UTF-8。
#[no_mangle]
pub unsafe extern "C" fn vault_core_p2p_sync_relay(
    handle: *mut Session,
    relay: *const c_char,
    room: *const c_char,
    token: *const c_char,
) -> *mut c_char {
    let run = || -> Result<serde_json::Value, i32> {
        unsafe { ensure_writable(handle) }?;
        let session = unsafe { &*handle };
        let r = unsafe { cstr(relay) }?;
        let room = unsafe { cstr(room) }?;
        let token = unsafe { cstr(token) }?;
        let engine = crate::p2p_service::p2p_engine(session).map_err(|e| map_err(&e))?;
        apply_sync_policy(session, &engine);
        let v = engine.sync_via_relay(r, room, token).map_err(|f| {
            if let Some(reason) = f.degrade_reason {
                crate::contract::publish(
                    crate::contract::EV_PATH_DEGRADED,
                    0,
                    serde_json::json!({"reason": reason, "diag": f.diag}),
                );
            }
            f.local_code
        })?;
        session.audit("device", &format!("sync relay {}", summary_line(&v)));
        publish_transfer_queue("done", 0, Some(&engine));
        Ok(v)
    };
    match run() {
        Ok(v) => json_out(v),
        Err(_) => std::ptr::null_mut(),
    }
}

/// **P8-4 打洞探针**（矩阵 / 诊断驱动用；`docs/v2.0/14-打洞NAT矩阵验收手册.md` §七）。
///
/// 走真实路径：VSR2 接入（顺带从 `OK` 行取观测端点）→ Noise 握手 → `hello_ext` 候选交换
/// → 映射观测 → 打洞（**同步**等结果）。**不要求已配对**、不读写保险箱、不做清单交换。
/// 两端各调一次（`role` = `0` 发起端 / 其它 响应端；`code` 两端需一致，空串 = 不用 PSK）。
///
/// 返回 JSON（`schema` / `role` / `peerDeviceId` / `ok` / `reason` / `symmetricNat` /
/// `localCandidates` / `remoteCandidates` / `kind` / `degradedSteps` / `mappedPort` /
/// `peerObservedPort` / `elapsedMs`）——**只含计数、原因与端口，无 IP**（隐私纪律）；
/// 结果同时落进 `path_status` 的 `punch` 段（与后台自动尝试共用同一套记账）。
/// 失败（中继接入 / 握手 / Hello）→ null，可读诊断进引擎事件日志 + 审计（同 `sync_relay`）。
///
/// # Safety
/// `handle` 有效；`relay_addr` / `room` / `token` / `code` 合法 UTF-8（`code` 可空串）。
#[no_mangle]
pub unsafe extern "C" fn vault_core_p2p_punch_probe(
    handle: *mut Session,
    relay_addr: *const c_char,
    room: *const c_char,
    token: *const c_char,
    code: *const c_char,
    role: u32,
    timeout_ms: u64,
) -> *mut c_char {
    let run = || -> Result<serde_json::Value, i32> {
        // 探针是只读诊断动作：不走 ensure_writable（维护态也应可诊断）
        if handle.is_null() {
            return Err(ERR_INVALID_ARG);
        }
        let session = unsafe { &*handle };
        let relay_addr = unsafe { cstr(relay_addr) }?;
        let room = unsafe { cstr(room) }?;
        let token = unsafe { cstr(token) }?;
        let code = unsafe { cstr(code) }?;
        let engine = crate::p2p_service::p2p_engine(session).map_err(|e| map_err(&e))?;
        apply_sync_policy(session, &engine);
        let ro = vault_p2p::engine::ProbeRole::from_code(role);
        let shared = if code.is_empty() { None } else { Some(code) };
        let v = engine
            .punch_probe(relay_addr, room, token, shared, ro, timeout_ms.max(1000))
            .map_err(|f| {
                if let Some(reason) = f.degrade_reason {
                    crate::contract::publish(
                        crate::contract::EV_PATH_DEGRADED,
                        0,
                        serde_json::json!({"reason": reason, "diag": f.diag}),
                    );
                }
                f.local_code
            })?;
        // 审计只记判定与计数（**不含 IP**）
        session.audit(
            "device",
            &format!(
                "punch probe role={} ok={} reason={} symmetricNat={} local={} remote={} kind={} elapsedMs={}",
                v["role"].as_str().unwrap_or("?"),
                v["ok"],
                v["reason"].as_str().unwrap_or("none"),
                v["symmetricNat"],
                v["localCandidates"],
                v["remoteCandidates"],
                v["kind"].as_str().unwrap_or("?"),
                v["elapsedMs"],
            ),
        );
        Ok(v)
    };
    match run() {
        Ok(v) => json_out(v),
        Err(_) => std::ptr::null_mut(),
    }
}

/// 作为响应端经中继接入一次（阻塞至本轮同步结束；冒烟/无直连场景用）。
/// 失败语义同 `vault_core_p2p_sync_relay`（含 PATH_DEGRADED 发布）。
///
/// # Safety
/// `handle` 有效；`relay`、`room`、`token` 合法 UTF-8。
#[no_mangle]
pub unsafe extern "C" fn vault_core_p2p_serve_relay(
    handle: *mut Session,
    relay: *const c_char,
    room: *const c_char,
    token: *const c_char,
) -> i32 {
    let run = || -> Result<(), i32> {
        unsafe { ensure_writable(handle) }?;
        let session = unsafe { &*handle };
        let r = unsafe { cstr(relay) }?;
        let room = unsafe { cstr(room) }?;
        let token = unsafe { cstr(token) }?;
        let engine = crate::p2p_service::p2p_engine(session).map_err(|e| map_err(&e))?;
        apply_sync_policy(session, &engine);
        engine.serve_relay_connection(r, room, token).map_err(|f| {
            if let Some(reason) = f.degrade_reason {
                crate::contract::publish(
                    crate::contract::EV_PATH_DEGRADED,
                    0,
                    serde_json::json!({"reason": reason, "diag": f.diag}),
                );
            }
            f.local_code
        })?;
        publish_transfer_queue("done", 0, Some(&engine));
        Ok(())
    };
    run().err().unwrap_or(OK)
}

// ==== 中继设置与多中继候选（P8-5 收尾，docs/v2.0/08 §3.6）====

/// 中继设置读取（settings.enc 载体）：
/// `{allowPublicRelay, relays: [{addr, token, public}]}`。token 是用户显式配置的
/// 接入凭证（非密钥材料），设置页需要回显，故原样返回。
///
/// # Safety
/// `handle` 必须是未释放的有效会话句柄。
#[no_mangle]
pub unsafe extern "C" fn vault_core_relay_settings_get(handle: *mut Session) -> *mut c_char {
    let run = || -> Result<serde_json::Value, i32> {
        if handle.is_null() {
            return Err(ERR_INVALID_ARG);
        }
        let session = unsafe { &*handle };
        let s = crate::settings::Settings::load(&session.data_dir(), &session.mk);
        Ok(serde_json::json!({
            "allowPublicRelay": s.allow_public_relay,
            "relays": s
                .relays
                .iter()
                .map(|r| serde_json::json!({"addr": r.addr, "token": r.token, "public": r.public}))
                .collect::<Vec<_>>(),
        }))
    };
    match run() {
        Ok(v) => json_out(v),
        Err(_) => std::ptr::null_mut(),
    }
}

/// 中继设置写入（**原子拒绝**：任一条不合法整体拒绝，旧设置完好）。审计只记
/// 开关与条数，**不含 token**。失败：7 = JSON 不可解析 / 校验不过。
///
/// # Safety
/// `handle` 必须是未释放的有效会话句柄；`json` 为合法 UTF-8 JSON 文本。
#[no_mangle]
pub unsafe extern "C" fn vault_core_relay_settings_set(
    handle: *mut Session,
    json: *const c_char,
) -> i32 {
    let run = || -> Result<(), i32> {
        if handle.is_null() {
            return Err(ERR_INVALID_ARG);
        }
        let session = unsafe { &*handle };
        let body = unsafe { cstr(json) }?;
        let v: serde_json::Value = serde_json::from_str(body).map_err(|_| ERR_INVALID_ARG)?;
        let allow = v
            .get("allowPublicRelay")
            .and_then(|x| x.as_bool())
            .ok_or(ERR_INVALID_ARG)?;
        let mut relays = Vec::new();
        for r in v
            .get("relays")
            .and_then(|x| x.as_array())
            .ok_or(ERR_INVALID_ARG)?
        {
            let addr = r
                .get("addr")
                .and_then(|x| x.as_str())
                .ok_or(ERR_INVALID_ARG)?
                .to_string();
            let token = r
                .get("token")
                .and_then(|x| x.as_str())
                .ok_or(ERR_INVALID_ARG)?
                .to_string();
            let public = r.get("public").and_then(|x| x.as_bool()).unwrap_or(false);
            relays.push(crate::settings::RelayEndpoint {
                addr,
                token,
                public,
            });
        }
        // 读改写：stego 开关与中继候选同载体，整存整取会抹掉对方字段
        let mut s = crate::settings::Settings::load(&session.data_dir(), &session.mk);
        s.schema = 1;
        s.allow_public_relay = allow;
        s.relays = relays;
        s.validate().map_err(|_| ERR_INVALID_ARG)?;
        s.save(&session.data_dir(), &session.mk)
            .map_err(|_| ERR_IO)?;
        session.audit(
            "device",
            &format!(
                "relay settings set allowPublicRelay={} relays={}",
                allow,
                s.relays.len()
            ),
        );
        Ok(())
    };
    run().err().unwrap_or(OK)
}

/// 多中继候选顺序同步（发起端，P8-5 收尾）：候选 = 设置里的中继列表（保序；
/// `allowPublicRelay = false` 时官方条目**在拨号前**被剔除）。按序逐个尝试，
/// 全部失败 → 最后一跳的本机码；发布 `PATH_DEGRADED`(13)（候选全部失败时
/// 只发一次）。候选为空 → 13 + `relay_unavailable`，不发起任何连接。
///
/// # Safety
/// `handle` 必须是未释放的有效会话句柄；`room` 合法 UTF-8。
#[no_mangle]
pub unsafe extern "C" fn vault_core_p2p_sync_relay_candidates(
    handle: *mut Session,
    room: *const c_char,
) -> *mut c_char {
    let run = || -> Result<serde_json::Value, i32> {
        unsafe { ensure_writable(handle) }?;
        let session = unsafe { &*handle };
        let room = unsafe { cstr(room) }?;
        let engine = crate::p2p_service::p2p_engine(session).map_err(|e| map_err(&e))?;
        apply_sync_policy(session, &engine);
        let s = crate::settings::Settings::load(&session.data_dir(), &session.mk);
        let cands = s.relay_candidates();
        let v = engine
            .sync_via_relay_candidates(&cands, room)
            .map_err(|f| {
                if let Some(reason) = f.degrade_reason {
                    crate::contract::publish(
                        crate::contract::EV_PATH_DEGRADED,
                        0,
                        serde_json::json!({
                            "reason": reason,
                            "diag": f.diag,
                            "candidates": cands.len(),
                        }),
                    );
                }
                f.local_code
            })?;
        session.audit(
            "device",
            &format!(
                "sync relay candidates n={} {}",
                cands.len(),
                summary_line(&v)
            ),
        );
        publish_transfer_queue("done", 0, Some(&engine));
        Ok(v)
    };
    match run() {
        Ok(v) => json_out(v),
        Err(_) => std::ptr::null_mut(),
    }
}

/// 响应端的多中继候选顺序接入（语义同 `vault_core_p2p_sync_relay_candidates`）。
///
/// # Safety
/// `handle` 必须是未释放的有效会话句柄；`room` 合法 UTF-8。
#[no_mangle]
pub unsafe extern "C" fn vault_core_p2p_serve_relay_candidates(
    handle: *mut Session,
    room: *const c_char,
) -> i32 {
    let run = || -> Result<(), i32> {
        unsafe { ensure_writable(handle) }?;
        let session = unsafe { &*handle };
        let room = unsafe { cstr(room) }?;
        let engine = crate::p2p_service::p2p_engine(session).map_err(|e| map_err(&e))?;
        apply_sync_policy(session, &engine);
        let s = crate::settings::Settings::load(&session.data_dir(), &session.mk);
        let cands = s.relay_candidates();
        engine.serve_relay_candidates(&cands, room).map_err(|f| {
            if let Some(reason) = f.degrade_reason {
                crate::contract::publish(
                    crate::contract::EV_PATH_DEGRADED,
                    0,
                    serde_json::json!({"reason": reason, "diag": f.diag}),
                );
            }
            f.local_code
        })?;
        publish_transfer_queue("done", 0, Some(&engine));
        Ok(())
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

/// 局域网设备发现快照（P8-4，docs/05-03 §3.4；CAP_DISCOVERY）。
/// 返回 JSON：`{schema, tsMs, enabled, self{deviceId,fingerprint,port},
/// matchCode, matchCodeExpiresMs, candidates:[{deviceId,name,fingerprint,
/// addrs[{addr,transport,reachable,rttMs}],paired}]}`。
/// 未配对设备 deviceId/name 为 null；发现不构成身份证明（配对仍须带外要素）。
/// mDNS 不可用 → `enabled=false`（不报错，如实降级；不得显示「未发现设备」）。
///
/// # Safety
/// `handle` 有效；`out_json` 非空（成功时写入引擎分配的 CString）。
#[no_mangle]
pub unsafe extern "C" fn vault_core_p2p_discover(
    handle: *mut Session,
    out_json: *mut *mut c_char,
) -> i32 {
    if crate::contract::capability_bits() & crate::contract::CAP_DISCOVERY == 0 {
        return ERR_CAPABILITY;
    }
    if handle.is_null() || out_json.is_null() {
        return ERR_INVALID_ARG;
    }
    let run = || -> Result<(), i32> {
        let session = unsafe { &*handle };
        let engine = crate::p2p_service::p2p_engine(session).map_err(|e| map_err(&e))?;
        let c = CString::new(serde_json::to_string(&engine.discover()).map_err(|_| ERR_INTERNAL)?)
            .map_err(|_| ERR_INTERNAL)?;
        unsafe { out_json.write(c.into_raw()) };
        Ok(())
    };
    run().err().unwrap_or(OK)
}

/// 路径与填充诊断（P8-6/P8-4/P8-5，docs/05-03 §4.4、05-04 F-07）：
/// `{"schema":1,"padding":{tier,legacyPeer,frameLenHistogram[4],overheadRatio},
/// "lastPathFailure":{reason,atMs}|null,"peers":[{peerId,path,pathSinceMs,
/// attempts{direct,relay},lastFailure,bytesIn,bytesOut,paddingTier,
/// punch{attempts,ok,reason,symmetricNat,localCandidates,remoteCandidates,
/// degradedSteps,kind}}]}`。
/// 只输出本进程可观测字段；rtt / 对端候选数等未观测项**不输出**（原则 7，
/// 不编造）。P8-4：`punch` 段只给**计数与原因**，不给 IP（隐私纪律）；
/// `path` 仍为 `direct|relay`——打通后数据仍走本会话承载，**不谎称** `hole_punch`。
///
/// # Safety
/// `handle` 有效。
#[no_mangle]
pub unsafe extern "C" fn vault_core_p2p_path_status(handle: *mut Session) -> *mut c_char {
    let run = || -> Result<serde_json::Value, i32> {
        let session = unsafe { &*handle };
        let engine = crate::p2p_service::p2p_engine(session).map_err(|e| map_err(&e))?;
        let st = engine.status();
        let mut ps = engine.path_stats();
        if let Some(obj) = ps.as_object_mut() {
            obj.insert("padding".into(), st["padding"].clone());
            obj.insert("lastPathFailure".into(), st["lastPathFailure"].clone());
        }
        Ok(ps)
    };
    match run() {
        Ok(v) => json_out(v),
        Err(_) => std::ptr::null_mut(),
    }
}

// ==== P8-9 阅后即焚（docs/v2.0/05-04 §五；CAP_BURN_SHARE）====

/// 创建一次性票据：`{"burnId","code","room","expiresMs","openTimeoutSecs"}`。
/// `code` **仅此一次返回**（秘密不落盘，重启即失效）。
///
/// # Safety
/// `handle` 有效。
#[no_mangle]
pub unsafe extern "C" fn vault_core_burn_create(
    handle: *mut Session,
    file_id: u64,
    ttl_secs: u64,
    open_timeout_secs: u64,
) -> *mut c_char {
    let run = || -> Result<serde_json::Value, i32> {
        unsafe { ensure_writable(handle) }?;
        if crate::contract::capability_bits() & crate::contract::CAP_BURN_SHARE == 0 {
            return Err(ERR_CAPABILITY);
        }
        let session = unsafe { &*handle };
        let engine = crate::p2p_service::p2p_engine(session).map_err(|e| map_err(&e))?;
        let v = engine
            .burn_create(file_id, ttl_secs, open_timeout_secs.max(1))
            .map_err(|_| ERR_INTERNAL)?;
        session.audit("burn", &format!("create file=#{file_id} ttl={ttl_secs}s"));
        Ok(v)
    };
    match run() {
        Ok(v) => json_out(v),
        Err(_) => std::ptr::null_mut(),
    }
}

/// 接收端登记票据（凭邀请码；秘密仅内存）。`relay_addr`/`relay_token` 为空串 = 直连模式。
///
/// # Safety
/// `handle` 有效；三个入参均合法 UTF-8。
#[no_mangle]
pub unsafe extern "C" fn vault_core_burn_receive(
    handle: *mut Session,
    code: *const c_char,
    relay_addr: *const c_char,
    relay_token: *const c_char,
) -> *mut c_char {
    let run = || -> Result<serde_json::Value, i32> {
        unsafe { ensure_writable(handle) }?;
        let session = unsafe { &*handle };
        let code = unsafe { cstr(code) }?;
        let ra = unsafe { cstr(relay_addr) }?;
        let rt = unsafe { cstr(relay_token) }?;
        let engine = crate::p2p_service::p2p_engine(session).map_err(|e| map_err(&e))?;
        let relay = if ra.is_empty() { None } else { Some((ra, rt)) };
        let v = engine
            .burn_receive(code, relay)
            .map_err(|_| ERR_INVALID_ARG)?;
        session.audit("burn", "joined room (code accepted, secret kept in memory)");
        Ok(v)
    };
    match run() {
        Ok(v) => json_out(v),
        Err(_) => std::ptr::null_mut(),
    }
}

/// 发送端投递：`addr` 非空 = 直连；否则用 `relay_addr` + `relay_token`（房间由票据派生）。
/// 返回 `{"delivered","bytes","peer"}`；已交付票据复用 → 7（参数）。
///
/// # Safety
/// `handle` 有效；三个入参均合法 UTF-8。
#[no_mangle]
pub unsafe extern "C" fn vault_core_burn_send(
    handle: *mut Session,
    burn_id: u64,
    addr: *const c_char,
    relay_addr: *const c_char,
    relay_token: *const c_char,
) -> *mut c_char {
    let run = || -> Result<serde_json::Value, i32> {
        unsafe { ensure_writable(handle) }?;
        let session = unsafe { &*handle };
        let addr = unsafe { cstr(addr) }?;
        let ra = unsafe { cstr(relay_addr) }?;
        let rt = unsafe { cstr(relay_token) }?;
        let engine = crate::p2p_service::p2p_engine(session).map_err(|e| map_err(&e))?;
        let v = if !addr.is_empty() {
            engine.burn_send_direct(burn_id, addr)
        } else {
            engine.burn_send_relay(burn_id, ra, rt)
        }
        .map_err(|e| {
            // 秘密与文件名绝不进日志/错误串；只记短码
            session.audit("burn", &format!("deliver failed id={burn_id} err={e}"));
            ERR_INVALID_ARG
        })?;
        session.audit(
            "burn",
            &format!("delivered id={burn_id} bytes={}", v["bytes"]),
        );
        Ok(v)
    };
    match run() {
        Ok(v) => json_out(v),
        Err(_) => std::ptr::null_mut(),
    }
}

/// 「打开」：解密为受控临时明文（Unix 0600）并起自动擦除定时器。
/// 返回 `{"path","name","size","openTimeoutSecs"}`。
///
/// # Safety
/// `handle` 有效。
#[no_mangle]
pub unsafe extern "C" fn vault_core_burn_open(handle: *mut Session, burn_id: u64) -> *mut c_char {
    let run = || -> Result<serde_json::Value, i32> {
        unsafe { ensure_writable(handle) }?;
        let session = unsafe { &*handle };
        let engine = crate::p2p_service::p2p_engine(session).map_err(|e| map_err(&e))?;
        let v = engine.burn_open(burn_id).map_err(|_| ERR_INVALID_ARG)?;
        session.audit("burn", &format!("opened id={burn_id}"));
        Ok(v)
    };
    match run() {
        Ok(v) => json_out(v),
        Err(_) => std::ptr::null_mut(),
    }
}

/// 显式「另存」：写用户路径并放弃该次自动擦除。0 = OK。
///
/// # Safety
/// `handle` 有效；`dest` 合法 UTF-8。
#[no_mangle]
pub unsafe extern "C" fn vault_core_burn_save_as(
    handle: *mut Session,
    burn_id: u64,
    dest: *const c_char,
) -> i32 {
    let run = || -> Result<(), i32> {
        unsafe { ensure_writable(handle) }?;
        let session = unsafe { &*handle };
        let dest = unsafe { cstr(dest) }?;
        let engine = crate::p2p_service::p2p_engine(session).map_err(|e| map_err(&e))?;
        engine
            .burn_save_as(burn_id, dest)
            .map_err(|_| ERR_INVALID_ARG)?;
        session.audit("burn", &format!("saved_as id={burn_id}"));
        Ok(())
    };
    run().err().unwrap_or(OK)
}

/// 擦除（临时明文 + 容器 + 内存密钥；重试 1/5/30 s ×3）。
/// 返回 `{"erased","attempts"}`——`erased=false` 时上层须告警。
///
/// # Safety
/// `handle` 有效。
#[no_mangle]
pub unsafe extern "C" fn vault_core_burn_erase(handle: *mut Session, burn_id: u64) -> *mut c_char {
    let run = || -> Result<serde_json::Value, i32> {
        unsafe { ensure_writable(handle) }?;
        let session = unsafe { &*handle };
        let engine = crate::p2p_service::p2p_engine(session).map_err(|e| map_err(&e))?;
        let v = engine.burn_erase(burn_id).map_err(|_| ERR_INTERNAL)?;
        session.audit(
            "burn",
            &format!("erase id={burn_id} erased={}", v["erased"]),
        );
        Ok(v)
    };
    match run() {
        Ok(v) => json_out(v),
        Err(_) => std::ptr::null_mut(),
    }
}

/// 票据状态：`{"burnId","side","phase","opened","erased",...}`。
///
/// # Safety
/// `handle` 有效。
#[no_mangle]
pub unsafe extern "C" fn vault_core_burn_status(handle: *mut Session, burn_id: u64) -> *mut c_char {
    let run = || -> Result<serde_json::Value, i32> {
        let session = unsafe { &*handle };
        let engine = crate::p2p_service::p2p_engine(session).map_err(|e| map_err(&e))?;
        Ok(engine.burn_status(burn_id))
    };
    match run() {
        Ok(v) => json_out(v),
        Err(_) => std::ptr::null_mut(),
    }
}

/// 解除与指定设备的配对（从对端登记表移除）。0 = OK（本来就不存在亦返回 0，幂等）。
/// # Safety
#[no_mangle]
pub unsafe extern "C" fn vault_core_p2p_unpair(
    handle: *mut Session,
    device_id: *const c_char,
) -> i32 {
    let run = || -> Result<(), i32> {
        unsafe { ensure_writable(handle) }?;
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
        unsafe { ensure_writable(handle) }?;
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
        unsafe { ensure_writable(handle) }?;
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

// ==== P7-10 远程锁定（docs/v2.0/05-01 §4.7）====

/// 远程锁定：向对端投递 kind=2 锁定指令（`peer_id=NULL` = 全部已配对设备）。
/// 离线对端照常入队（返回 0 且 JSON pending=true）；无配对记录 → 7。
///
/// # Safety
/// `handle` 必须为有效会话句柄；`peer_id` 可为 NULL。
#[no_mangle]
pub unsafe extern "C" fn vault_core_p2p_push_lock(
    handle: *mut Session,
    peer_id: *const c_char,
) -> i32 {
    if handle.is_null() {
        return ERR_INVALID_ARG;
    }
    let session = unsafe { &*handle };
    let pid = if peer_id.is_null() {
        None
    } else {
        match unsafe { cstr(peer_id) } {
            Ok(s) => Some(s.to_string()),
            Err(code) => return code,
        }
    };
    match crate::p2p_service::p2p_engine(session) {
        Ok(engine) => match engine.push_lock(pid.as_deref()) {
            Ok(v) => {
                audit_op(handle, "security", "lock.remote.send");
                let _ = v;
                OK
            }
            Err(msg) => {
                if msg.contains("not paired") {
                    ERR_INVALID_ARG
                } else {
                    ERR_IO
                }
            }
        },
        Err(_) => ERR_INTERNAL,
    }
}

// ==== P7-4 轮换传播（docs/v2.0/07 §四）====

/// 向对端入队轮换传播（kind=3，`peer_id=NULL` = 全部已配对设备）。
/// 载体 = state=2 时保留的 rotation.propagate（新 MK' 被旧 MK 包装）；
/// 对端下次解锁后自动完成等价轮换。无未投递项 → 7。
///
/// # Safety
/// `handle` 必须为有效会话句柄；`peer_id` 可为 NULL。
#[no_mangle]
pub unsafe extern "C" fn vault_core_p2p_push_rotation(
    handle: *mut Session,
    peer_id: *const c_char,
    out_task_id: *mut u32,
) -> i32 {
    if handle.is_null() {
        return ERR_INVALID_ARG;
    }
    let session = unsafe { &*handle };
    if !out_task_id.is_null() {
        unsafe { out_task_id.write(0) };
    }
    let pid = if peer_id.is_null() {
        None
    } else {
        match unsafe { cstr(peer_id) } {
            Ok(s) => Some(s.to_string()),
            Err(code) => return code,
        }
    };
    let data_dir = session.data_dir();
    let propagate = data_dir.join("rotation.propagate");
    let Ok(blob) = std::fs::read(&propagate) else {
        return 7; // 无待传播轮换
    };
    let ks = match crate::keystore::Keystore::load(&session.vault_path) {
        Ok(k) => k,
        Err(_) => return ERR_IO,
    };
    if ks.rotation_id == [0u8; 16] {
        return 7;
    }
    let rid = vault_crypto::hex_encode(&ks.rotation_id);
    match crate::p2p_service::p2p_engine(session) {
        Ok(engine) => {
            match engine.push_rotation(pid.as_deref(), &rid, &vault_crypto::hex_encode(&blob)) {
                Ok(v) => {
                    audit_op(handle, "security", "rotation.propagate push");
                    let _ = v;
                    OK
                }
                Err(msg) => {
                    if msg.contains("not paired") {
                        ERR_INVALID_ARG
                    } else {
                        ERR_IO
                    }
                }
            }
        }
        Err(_) => ERR_INTERNAL,
    }
}

// ==== P7-3 轮换会话三件套（docs/v2.0/05-01 §3.6）====

/// 轮换会话 · begin。0=完成（无对端）/ 1=密码错（进冷却）/ 9=只读 /
/// 10=已在轮换 / 3=VSRR 写失败。中断后由 [`vault_core_rotate_mk_resume`] 续做。
/// 偏差（记 LOG）：同步执行（数据面重加密在本调用内完成），`out_task_id`
/// 恒写 0——异步任务化归 P8-1 传输队列统一处理。
///
/// # Safety
/// `handle` 必须为有效会话句柄；`password`/`out_task_id` 语义同上。
#[no_mangle]
pub unsafe extern "C" fn vault_core_rotate_mk_begin(
    handle: *mut Session,
    password: *const c_char,
    out_task_id: *mut u32,
) -> i32 {
    if let Err(code) = unsafe { ensure_writable(handle) } {
        return code;
    }
    let session = unsafe { &*handle };
    let pwd = match unsafe { cstr(password) } {
        Ok(p) => p,
        Err(code) => return code,
    };
    if !out_task_id.is_null() {
        unsafe { out_task_id.write(0) };
    }
    let store = open_os_store();
    match service::rotate_mk_begin(
        session,
        pwd,
        store.as_ref().map(|st| st as &dyn SecureStore),
    ) {
        Ok(rewritten) => {
            audit_op(handle, "security", "rotation.begin done");
            let _ = rewritten;
            OK
        }
        Err(CoreError::WrongPassword(w)) => {
            write_wait(std::ptr::null_mut(), w);
            ERR_WRONG_PASSWORD
        }
        Err(e) => map_err(&e),
    }
}

/// 轮换会话 · resume（需要密码重包装 MK'；偏差见 service.rs）。0=完成 /
/// 11=恢复文件缺失或校验不过 / 10=已待传播。
///
/// # Safety
/// `handle` 必须为有效会话句柄。
#[no_mangle]
pub unsafe extern "C" fn vault_core_rotate_mk_resume(
    handle: *mut Session,
    password: *const c_char,
) -> i32 {
    if handle.is_null() {
        return ERR_INVALID_ARG;
    }
    let session = unsafe { &*handle };
    if session.is_readonly() {
        return ERR_LEASE_BUSY;
    }
    let pwd = match unsafe { cstr(password) } {
        Ok(p) => p,
        Err(code) => return code,
    };
    match service::rotate_mk_resume(session, pwd) {
        Ok(_) => OK,
        Err(e) => map_err(&e),
    }
}

/// 轮换会话 · status：JSON 载荷（03 §4.10）。失败返回 NULL。
///
/// # Safety
/// `handle` 必须为有效会话句柄。
#[no_mangle]
pub unsafe extern "C" fn vault_core_rotate_mk_status(handle: *mut Session) -> *mut c_char {
    if handle.is_null() {
        return std::ptr::null_mut();
    }
    let session = unsafe { &*handle };
    match service::rotate_mk_status(session) {
        Ok(v) => json_out(v),
        Err(_) => std::ptr::null_mut(),
    }
}

// ==== P6-6 FFI 契约 V2（docs/v2.0/02 §六）====

/// ABI v2 握手。握手必须先于一切调用；`abi_version` 主号不匹配由前端拒绝加载。
///
/// # Safety
/// `out` 非空且指向 VsAbiInfo 布局的内存。
#[no_mangle]
pub unsafe extern "C" fn vault_core_abi_info(out: *mut crate::contract::VsAbiInfo) -> i32 {
    if out.is_null() {
        return ERR_INVALID_ARG;
    }
    unsafe {
        (*out).abi_version = 2;
        (*out).reserved0 = 0;
        (*out).capability_bits = crate::contract::capability_bits();
        // max_write_ver_* 如实报告当前实现（原则 7）：VSVB/VSEF 仍是 v2（v3 是 P7），
        // VSIX v3 与 VSAU v1 已随 P6-3/P6-4 落地
        (*out).max_write_ver_vault = 2;
        (*out).max_write_ver_container = 2;
        (*out).max_write_ver_index = 3;
        (*out).max_write_ver_audit = 1;
        (*out).min_read_ver_vault = 2;
        (*out).feature_flags = 0;
        (*out).engine_version = crate::contract::engine_version_bytes();
        (*out).struct_size = std::mem::size_of::<crate::contract::VsAbiInfo>() as u32;
    }
    OK
}

/// 订阅推送事件流。返回订阅 id；未置位 CAP_EVENTS 返回 13。
///
/// # Safety
/// `callback` 必须为合法函数指针；`user` 原样回传。
#[no_mangle]
pub unsafe extern "C" fn vault_core_subscribe(
    callback: extern "C" fn(
        event: *mut crate::contract::VsEvent,
        user: *mut std::ffi::c_void,
    ) -> i32,
    user: *mut std::ffi::c_void,
    sub_out: *mut u32,
) -> i32 {
    if crate::contract::capability_bits() & crate::contract::CAP_EVENTS == 0 {
        return ERR_CAPABILITY;
    }
    if callback as usize == 0 || sub_out.is_null() {
        return ERR_INVALID_ARG;
    }
    unsafe { sub_out.write(crate::contract::subscribe(callback, user)) };
    OK
}

/// 取消订阅。
///
/// # Safety
/// 无。
#[no_mangle]
pub unsafe extern "C" fn vault_core_unsubscribe(sub_id: u32) -> i32 {
    if crate::contract::capability_bits() & crate::contract::CAP_EVENTS == 0 {
        return ERR_CAPABILITY;
    }
    if crate::contract::unsubscribe(sub_id) {
        OK
    } else {
        ERR_INVALID_ARG
    }
}

/// 拉取缓冲事件（推送模型兜底）。返回 JSON 数组字符串（调用方 free_string）；
/// 帧含 type/seq/tsMs/taskId/payload。
///
/// # Safety
/// `out` 非空。
#[no_mangle]
pub unsafe extern "C" fn vault_core_poll_events(out: *mut *mut c_char) -> i32 {
    if crate::contract::capability_bits() & crate::contract::CAP_EVENTS == 0 {
        return ERR_CAPABILITY;
    }
    if out.is_null() {
        return ERR_INVALID_ARG;
    }
    let mut events = Vec::new();
    crate::contract::poll_events(&mut |etype, seq, ts, task_id, payload| {
        let payload: serde_json::Value = serde_json::from_slice(payload).unwrap_or_default();
        events.push(serde_json::json!({
            "type": etype, "seq": seq, "tsMs": ts, "taskId": task_id, "payload": payload,
        }));
    });
    let v = json_out(serde_json::Value::Array(events));
    unsafe { out.write(v) };
    OK
}

/// 任务列表 JSON {"count","tasks":[{id,kind,state,priority,progress{…}}]}
/// （P8-2 起 progress 含 doneBytes/totalBytes/doneChunks/totalChunks/rateBps/etaMs）。
///
/// # Safety
/// `out` 非空。
#[no_mangle]
pub unsafe extern "C" fn vault_core_task_list(out: *mut *mut c_char) -> i32 {
    if crate::contract::capability_bits() & crate::contract::CAP_TASKS == 0 {
        return ERR_CAPABILITY;
    }
    if out.is_null() {
        return ERR_INVALID_ARG;
    }
    let tasks: Vec<_> = crate::contract::task_list()
        .into_iter()
        .map(task_json)
        .collect();
    let v = json_out(serde_json::json!({"count": tasks.len(), "tasks": tasks}));
    unsafe { out.write(v) };
    OK
}

fn task_json(t: crate::contract::TaskSnapshot) -> serde_json::Value {
    serde_json::json!({
        "id": t.id, "kind": t.kind, "state": t.state, "priority": t.priority,
        "progress": {
            "doneBytes": t.done_bytes, "totalBytes": t.total_bytes,
            "doneChunks": t.done_chunks, "totalChunks": t.total_chunks,
            "rateBps": t.rate_bps, "etaMs": t.eta_ms,
        },
        "resultJson": t.result_json,
    })
}

/// 单任务状态 JSON，未知 id 返回 7。
///
/// # Safety
/// `out` 非空。
#[no_mangle]
pub unsafe extern "C" fn vault_core_task_status(task_id: u32, out: *mut *mut c_char) -> i32 {
    if crate::contract::capability_bits() & crate::contract::CAP_TASKS == 0 {
        return ERR_CAPABILITY;
    }
    if out.is_null() {
        return ERR_INVALID_ARG;
    }
    match crate::contract::task_status(task_id) {
        None => ERR_INVALID_ARG,
        Some(t) => {
            let v = json_out(task_json(t));
            unsafe { out.write(v) };
            OK
        }
    }
}

/// 协作式取消：0=已请求 / 7=未知 / 12=已取消（幂等）。资源 5 s 内释放。
///
/// # Safety
/// 无。
#[no_mangle]
pub unsafe extern "C" fn vault_core_task_cancel(task_id: u32) -> i32 {
    if crate::contract::capability_bits() & crate::contract::CAP_TASKS == 0 {
        return ERR_CAPABILITY;
    }
    crate::contract::task_cancel(task_id)
}

/// 启动一个诊断自检任务（crypto/审计自检循环；进度经 TASK_PROGRESS 推送，
/// 可被 task_cancel 协作式取消 / task_pause 暂停续做）。任务框架的引擎内
/// 生产者，供冒烟与诊断使用。
///
/// # Safety
/// `out` 非空。
#[no_mangle]
pub unsafe extern "C" fn vault_core_task_spawn_selfcheck(out: *mut u32) -> i32 {
    if crate::contract::capability_bits() & crate::contract::CAP_TASKS == 0 {
        return ERR_CAPABILITY;
    }
    if out.is_null() {
        return ERR_INVALID_ARG;
    }
    let id = crate::contract::spawn_task("selfcheck", "background", move |ctl, progress| {
        for i in 0..20u64 {
            ctl.checkpoint()?;
            std::thread::sleep(std::time::Duration::from_millis(20));
            progress(i * 5, 100, i, 20);
        }
        if vault_crypto::self_check() && vault_audit::self_check() {
            Ok(serde_json::json!({"selfCheck": "ok"}))
        } else {
            Err("self check failed".into())
        }
    });
    unsafe { out.write(id) };
    OK
}

/// 任务暂停（P8-2，docs/v2.0/02 §6.5）：运行中任务在下一检查点转 paused，
/// 任务槽与已收块保留（区别于 cancel 的释放语义）。
/// 返回 0 / 7（未知任务）/ 12（状态不允许：排队中或终态）。
///
/// # Safety
/// `task_id` 由 task_spawn / 入队接口签发。
#[no_mangle]
pub unsafe extern "C" fn vault_core_task_pause(task_id: u32) -> i32 {
    if crate::contract::capability_bits() & crate::contract::CAP_TASKS == 0 {
        return ERR_CAPABILITY;
    }
    crate::contract::task_pause(task_id)
}

/// 任务恢复（P8-2）：paused → 原任务内续做（不重新入队、不清进度）。
/// 返回 0 / 7 / 12。
///
/// # Safety
/// `task_id` 由 task_spawn / 入队接口签发。
#[no_mangle]
pub unsafe extern "C" fn vault_core_task_resume(task_id: u32) -> i32 {
    if crate::contract::capability_bits() & crate::contract::CAP_TASKS == 0 {
        return ERR_CAPABILITY;
    }
    crate::contract::task_resume(task_id)
}

/// 会话只读状态（P6-2 只读降级）：1=只读（他进程持租约），0=可写。
///
/// # Safety
/// `handle` 必须为有效会话句柄；`out` 非空。
#[no_mangle]
pub unsafe extern "C" fn vault_core_session_readonly(handle: *mut Session, out: *mut i32) -> i32 {
    if handle.is_null() || out.is_null() {
        return ERR_INVALID_ARG;
    }
    let session = unsafe { &*handle };
    unsafe { out.write(i32::from(session.is_readonly())) };
    OK
}

/// 缩略图导出：P7-9 交付（CAP_THUMBNAIL 未置位 → 13）。能力探针。
///
/// # Safety
/// `handle` 必须为有效会话句柄。
#[no_mangle]
pub unsafe extern "C" fn vault_core_vault_thumbnail(
    handle: *mut Session,
    file_id: u64,
    _out_png: *mut *mut u8,
    _out_len: *mut u64,
) -> i32 {
    let _ = (handle, file_id);
    ERR_CAPABILITY
}

// ==== P7-7 V2→V3 格式迁移工具（docs/v2.0/07 §七）====

fn map_mig_err(e: crate::migrate::MigrateError) -> i32 {
    match e {
        crate::migrate::MigrateError::WrongPassword => ERR_WRONG_PASSWORD,
        crate::migrate::MigrateError::LeaseBusy => ERR_LEASE_BUSY,
        crate::migrate::MigrateError::Maintenance => ERR_MAINTENANCE,
        crate::migrate::MigrateError::Io(_) => ERR_IO,
    }
}

/// V2→V3 迁移 · begin。0=完成 / 1=密码错 / 9=只读 / 10=维护态 / 3=IO。
/// 同步执行（偏差记 LOG）：全部迁移在调用内完成，`out_task_id` 恒写 0；
/// 单文件失败不整库失败（日志标 failed），断点续做走 [`vault_core_migrate_resume`]。
///
/// # Safety
/// `handle` 必须为有效会话句柄；`password` 合法 UTF-8；`out_task_id` 可为 null。
#[no_mangle]
pub unsafe extern "C" fn vault_core_migrate_begin(
    handle: *mut Session,
    password: *const c_char,
    out_task_id: *mut u32,
) -> i32 {
    if handle.is_null() {
        return ERR_INVALID_ARG;
    }
    let session = unsafe { &*handle };
    if !out_task_id.is_null() {
        unsafe { out_task_id.write(0) };
    }
    let pwd = match unsafe { cstr(password) } {
        Ok(p) => p,
        Err(code) => return code,
    };
    match crate::migrate::begin(session, pwd) {
        Ok(summary) => {
            audit_op(handle, "security", &format!("migration.begin {}", summary));
            OK
        }
        Err(crate::migrate::MigrateError::WrongPassword) => ERR_WRONG_PASSWORD,
        Err(e) => map_mig_err(e),
    }
}

/// 迁移状态 JSON（{schema, files[], phases{}, backupPath}）。失败返回 NULL。
///
/// # Safety
/// `handle` 必须为有效会话句柄。
#[no_mangle]
pub unsafe extern "C" fn vault_core_migrate_status(handle: *mut Session) -> *mut c_char {
    if handle.is_null() {
        return std::ptr::null_mut();
    }
    let session = unsafe { &*handle };
    match crate::migrate::status(session) {
        Ok(v) => json_out(v),
        Err(_) => std::ptr::null_mut(),
    }
}

/// 断点续做：与 begin 同体（跳过日志 done 项）。0=完成 / 1=密码错 / 9 / 10 / 3。
///
/// # Safety
/// `handle` 必须为有效会话句柄；`password` 合法 UTF-8。
#[no_mangle]
pub unsafe extern "C" fn vault_core_migrate_resume(
    handle: *mut Session,
    password: *const c_char,
) -> i32 {
    if handle.is_null() {
        return ERR_INVALID_ARG;
    }
    let session = unsafe { &*handle };
    let pwd = match unsafe { cstr(password) } {
        Ok(p) => p,
        Err(code) => return code,
    };
    match crate::migrate::resume(session, pwd) {
        Ok(_) => {
            audit_op(handle, "security", "migration.resume done");
            OK
        }
        Err(e) => map_mig_err(e),
    }
}

/// 取消（偏差记 LOG）：同步执行下仅清理 `.mig.tmp` / `.mig.part` 残留并返回 0。
///
/// # Safety
/// `handle` 必须为有效会话句柄。
#[no_mangle]
pub unsafe extern "C" fn vault_core_migrate_cancel(handle: *mut Session) -> i32 {
    if handle.is_null() {
        return ERR_INVALID_ARG;
    }
    let session = unsafe { &*handle };
    match crate::migrate::cancel(session) {
        Ok(()) => {
            audit_op(handle, "security", "migration.cancel cleanup");
            OK
        }
        Err(e) => map_mig_err(e),
    }
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

    // ==== P8-10 容量协商 ====

    /// 构造真实 PNG（测试用 dev-dependency `png`；生产路径不依赖图像库）。
    fn tiny_png(dir: &std::path::Path, name: &str, w: u32, h: u32) -> String {
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, w, h);
            enc.set_color(png::ColorType::Rgb);
            enc.set_depth(png::BitDepth::Eight);
            let mut writer = enc.write_header().unwrap();
            let data = vec![0u8; (w as usize) * (h as usize) * 3];
            writer.write_image_data(&data).unwrap();
        }
        let p = dir.join(name);
        std::fs::write(&p, out).unwrap();
        p.to_string_lossy().to_string()
    }

    /// 证据（面板 P8-10 `stego::plan_matches_actual_shard_count` / `fits` 语义）：
    /// `_plan` 是容量的唯一真值来源——图不够时 `fits=false` + `imagesNeeded` + `shortfallBytes`，
    /// 不可读图如实计入 notes，`codec=auto` 标 png 回退。
    #[test]
    fn stego_plan_reports_needed_images_and_fits() {
        let dir = tempfile::tempdir().unwrap();
        let small = tiny_png(dir.path(), "s.png", 16, 16); // capacity = 92
        let big = tiny_png(dir.path(), "b.png", 256, 256); // capacity = 24572

        // 大文件 + 两图 → 不够：fits=false 且外推所需图数
        let plan = stego_plan_value(
            100_000,
            "f.bin",
            &[small.clone(), big.clone()],
            &serde_json::json!({"fillRatio": 0.8}),
        );
        assert_eq!(plan["fits"], serde_json::json!(false));
        assert_eq!(plan["imagesGiven"], serde_json::json!(2));
        assert!(
            plan["imagesNeeded"].as_u64().unwrap() > 2,
            "外推所需图数：{plan}"
        );
        assert!(plan["shortfallBytes"].as_u64().unwrap() > 0);
        assert_eq!(plan["perImage"].as_array().unwrap().len(), 2);
        // 净容量 = floor(cap × 0.8) − 分片头（首片含文件头），与切分口径一致
        let first = &plan["perImage"][0];
        assert_eq!(first["usable"], serde_json::json!(73)); // floor(92×0.8)
        assert_eq!(first["shardBytes"], serde_json::json!(20)); // 73 − (14+2+5+32)

        // 小文件 + 大图 → fits，且一张即够
        let ok = stego_plan_value(
            1000,
            "f.bin",
            std::slice::from_ref(&big),
            &serde_json::json!({}),
        );
        assert_eq!(ok["fits"], serde_json::json!(true));
        assert_eq!(ok["imagesNeeded"], serde_json::json!(1));
        assert_eq!(ok["shortfallBytes"], serde_json::json!(0));

        // codec=auto → notes 标 png 回退（JPEG 鲁棒路线不做）
        let auto = stego_plan_value(
            10,
            "f.bin",
            std::slice::from_ref(&small),
            &serde_json::json!({"codec": "auto"}),
        );
        assert!(auto["notes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n.as_str() == Some("codec_fallback=png")));

        // 不可读图 → fits=false + notes 如实标注（不谎报可嵌）
        let bad = stego_plan_value(
            10,
            "f.bin",
            &[dir.path().join("missing.png").to_string_lossy().to_string()],
            &serde_json::json!({}),
        );
        assert_eq!(bad["fits"], serde_json::json!(false));
        assert!(bad["perImage"][0]["error"].is_string());
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
