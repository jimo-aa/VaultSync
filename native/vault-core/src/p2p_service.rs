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
    let k = &session.keys;
    let keys = vault_p2p::engine::EngineKeys {
        mk,
        orders: k.orders,
        ident: k.discovery,
        index: k.index,
        search: k.search,
    };
    let vault_path = session.vault_path.clone();
    let wipe_path = vault_path.clone();
    let wipe: vault_p2p::engine::WipeFn =
        Box::new(move || crate::service::wipe_local(&wipe_path, false));

    let engine = match P2pEngine::new(
        &vault_path,
        &keys,
        "desktop",
        Arc::clone(&session.vault),
        wipe,
    ) {
        Ok(e) => {
            // P7-10：远程锁定执行体——清保险箱槽位（密钥材料随 Vault 值零化），
            // 审计与 P2P 引擎随会话销毁；会话级全量锁归外壳生命周期管理（P9-4）。
            let slot = Arc::clone(&session.vault);
            e.set_lock_fn(Box::new(move || {
                if let Ok(mut g) = slot.lock() {
                    *g = None;
                }
                eprintln!("vsync remote lock executed");
            }));
            Arc::new(e)
        }
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
