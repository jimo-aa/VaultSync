//! P2P 同步服务编排（P3）：按会话惰性创建引擎，注入擦除回调与保险箱槽位。
//! 引擎随会话销毁；伪装空间会话禁止一切 P2P 操作。
use std::sync::Arc;

use vault_p2p::P2pEngine;

use crate::service::CoreError;
use crate::session::Session;

/// 取会话的 P2P 引擎（首次调用时创建：设备身份 / 对端表 / 监听线程）。
pub(crate) fn p2p_engine(session: &Session) -> Result<Arc<P2pEngine>, CoreError> {
    if session.disguise {
        return Err(CoreError::Internal("disguise session has no p2p"));
    }
    let mut guard = session.p2p.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(e) = guard.as_ref() {
        return Ok(Arc::clone(e));
    }
    let mk = *session.mk;
    let vault_path = session.vault_path.clone();
    let wipe_path = vault_path.clone();
    let wipe: vault_p2p::engine::WipeFn = Box::new(move || {
        // 远程销毁执行体：删保险箱头部 + 整个数据目录（docs/05-06 销毁语义）
        let stem = wipe_path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or("bad vault path")?;
        let data_dir = wipe_path
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .join(format!("{stem}.data"));
        let mut errs = Vec::new();
        if std::fs::remove_file(&wipe_path).is_err() && wipe_path.exists() {
            errs.push("vault file");
        }
        if std::fs::remove_dir_all(&data_dir).is_err() && data_dir.exists() {
            errs.push("data dir");
        }
        if errs.is_empty() {
            Ok(())
        } else {
            Err(format!("wipe failed: {}", errs.join(", ")))
        }
    });
    let engine = match P2pEngine::new(
        &vault_path,
        &mk,
        "desktop",
        Arc::clone(&session.vault),
        wipe,
    ) {
        Ok(e) => Arc::new(e),
        Err(e) => {
            eprintln!(
                "vsync p2p engine init failed: {e} (vault={:?})",
                vault_path.display()
            );
            return Err(CoreError::Internal("p2p engine init"));
        }
    };
    *guard = Some(Arc::clone(&engine));
    Ok(engine)
}
