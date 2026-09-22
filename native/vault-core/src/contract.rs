//! FFI 契约 V2（P6-6，docs/v2.0/02 §六）。
//!
//! - ABI v2 握手（`VsAbiInfo`）：能力位 + 格式版本 + 引擎版本；
//! - 推送式事件流：环形缓冲 1024 帧 + 同类型合并节流 + `EVENT_OVERFLOW`；
//! - 任务注册表：list / status / cancel（pause / resume 是 P8-1）。
//!
//! 能力位语义只增不复用（§6.7 版本化规则）；调用未置位能力的导出返回 13。
#![allow(dead_code)]

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::Instant;

// ==== 能力位（docs/v2.0/02 §6.3 表格口径：bit 0–19；bit 20–63 保留）====

pub const CAP_EVENTS: u64 = 1 << 0;
pub const CAP_TASKS: u64 = 1 << 1;
pub const CAP_TRANSFER_QUEUE: u64 = 1 << 2;
pub const CAP_RESUME: u64 = 1 << 3;
pub const CAP_TX_PADDING: u64 = 1 << 4;
pub const CAP_DISCOVERY: u64 = 1 << 5;
pub const CAP_HOLE_PUNCH: u64 = 1 << 6;
pub const CAP_SELECTIVE_SYNC: u64 = 1 << 7;
pub const CAP_ERASE_CLASS: u64 = 1 << 8;
pub const CAP_ROTATION_SESSION: u64 = 1 << 9;
pub const CAP_MIGRATION: u64 = 1 << 10;
pub const CAP_PQ_HYBRID: u64 = 1 << 11;
pub const CAP_SEARCH_FRAGMENT: u64 = 1 << 12;
pub const CAP_BURN_SHARE: u64 = 1 << 13;
pub const CAP_BIO: u64 = 1 << 14;
pub const CAP_SECURE_STORE: u64 = 1 << 15;
pub const CAP_NOTIFICATIONS: u64 = 1 << 16;
pub const CAP_MOBILE_MANAGED: u64 = 1 << 17;
pub const CAP_THUMBNAIL: u64 = 1 << 18;
pub const CAP_LICENSE: u64 = 1 << 19;

/// 当前引擎能力位（运行期不变项；BIO / SECURE_STORE 在此一并置位，
/// 由调用方以 `vault_core_bio_bound` 探测运行期可用性）。
pub fn capability_bits() -> u64 {
    CAP_EVENTS
        | CAP_TASKS
        | CAP_BIO
        | CAP_SECURE_STORE
        // P7 交付（M7 收口置位，docs/v2.0/09）：全部有运行时接口与证据测试支撑；
        // CAP_THUMBNAIL（bit 18）不置位——编解码归 P9，导出如实返回 13。
        | crate::contract::CAP_ERASE_CLASS
        | crate::contract::CAP_ROTATION_SESSION
        | crate::contract::CAP_MIGRATION
        | crate::contract::CAP_PQ_HYBRID
        | crate::contract::CAP_SEARCH_FRAGMENT
}

// ==== 事件类型（docs/v2.0/02 §6.4，1–24；24–255 保留）====

pub const EV_ENGINE_READY: u16 = 1;
pub const EV_SESSION_STATE: u16 = 2;
pub const EV_LOCK_WARNING: u16 = 3;
pub const EV_LOCKED: u16 = 4;
pub const EV_VAULT_CHANGED: u16 = 5;
pub const EV_TASK_QUEUED: u16 = 6;
pub const EV_TASK_PROGRESS: u16 = 7;
pub const EV_TASK_DONE: u16 = 8;
pub const EV_TASK_FAILED: u16 = 9;
pub const EV_TASK_CANCELLED: u16 = 10;
pub const EV_TRANSFER_QUEUE: u16 = 11;
pub const EV_PEER_STATE: u16 = 12;
pub const EV_PATH_DEGRADED: u16 = 13;
pub const EV_ROTATION_STATE: u16 = 14;
pub const EV_MIGRATION_PROGRESS: u16 = 15;
pub const EV_ERASE_PROGRESS: u16 = 16;
pub const EV_MAINTENANCE_ENTER: u16 = 17;
pub const EV_MAINTENANCE_EXIT: u16 = 18;
pub const EV_CAPABILITY_CHANGED: u16 = 19;
pub const EV_AUDIT_APPENDED: u16 = 20;
pub const EV_UPDATE_AVAILABLE: u16 = 21;
pub const EV_ERROR_DIAG: u16 = 22;
pub const EV_EVENT_OVERFLOW: u16 = 23;
pub const EV_LICENSE_CHANGED: u16 = 24;

/// 事件帧（C 侧 `vs_event_t`）。payload 生命周期只到回调返回 / poll 拷贝完成。
#[repr(C)]
pub struct VsEvent {
    pub struct_size: u32,
    pub event_ver: u16,
    pub event_type: u16,
    pub seq: u64,
    pub ts_ms: u64,
    pub task_id: u32,
    pub payload_len: u32,
    pub payload: *const u8,
}

pub const VS_EVENT_SIZE: u32 = std::mem::size_of::<VsEvent>() as u32;
pub const VS_EVENT_VER: u16 = 1;

const RING_CAP: usize = 1024;
const PROGRESS_THROTTLE: std::time::Duration = std::time::Duration::from_millis(100);

struct Frame {
    event_type: u16,
    seq: u64,
    ts_ms: u64,
    task_id: u32,
    payload: Vec<u8>,
}

struct Subscriber {
    id: u32,
    callback: extern "C" fn(event: *mut VsEvent, user: *mut std::ffi::c_void) -> i32,
    user: *mut std::ffi::c_void,
}

unsafe impl Send for Subscriber {}

struct EventBusInner {
    ring: VecDeque<Frame>,
    next_seq: u64,
    dropped: u64,
    last_progress: Vec<(u32, Instant)>, // task_id → 上次投放时刻
}

struct EventBus {
    inner: Mutex<EventBusInner>,
    subscribers: Mutex<Vec<Subscriber>>,
    next_sub: AtomicU32,
    cond: Condvar,
}

static BUS: OnceLock<EventBus> = OnceLock::new();

fn bus() -> &'static EventBus {
    BUS.get_or_init(|| EventBus {
        inner: Mutex::new(EventBusInner {
            ring: VecDeque::with_capacity(RING_CAP),
            next_seq: 1,
            dropped: 0,
            last_progress: Vec::new(),
        }),
        subscribers: Mutex::new(Vec::new()),
        next_sub: AtomicU32::new(1),
        cond: Condvar::new(),
    })
}

/// 发布一条事件：入环形缓冲（满则淘汰最旧并合成 EVENT_OVERFLOW），
/// 并推送给全部订阅者（回调非 0 不重试；回调内再入引擎 API 由外层 FFI 拦截）。
pub fn publish(event_type: u16, task_id: u32, payload: serde_json::Value) {
    let payload_vec = serde_json::to_vec(&payload).unwrap_or_default();
    let b = bus();
    let mut inner = b.inner.lock().unwrap_or_else(|e| e.into_inner());

    // TASK_PROGRESS 节流：同 task 100 ms 内合并（保留最新）
    if event_type == EV_TASK_PROGRESS {
        let now = Instant::now();
        if let Some(pos) = inner
            .last_progress
            .iter()
            .position(|(id, _)| *id == task_id)
        {
            if now.duration_since(inner.last_progress[pos].1) < PROGRESS_THROTTLE {
                // 合并：替换 ring 中该 task 最后一条 PROGRESS
                for f in inner.ring.iter_mut().rev() {
                    if f.event_type == EV_TASK_PROGRESS && f.task_id == task_id {
                        f.payload = payload_vec;
                        break;
                    }
                }
                inner.last_progress[pos].1 = now;
                return;
            }
            inner.last_progress[pos].1 = now;
        } else {
            inner.last_progress.push((task_id, now));
        }
    }

    let seq = inner.next_seq;
    inner.next_seq += 1;
    let frame = Frame {
        event_type,
        seq,
        ts_ms: now_ms(),
        task_id,
        payload: payload_vec.clone(),
    };
    if inner.ring.len() >= RING_CAP {
        inner.ring.pop_front();
        inner.dropped += 1;
        // 合成 EVENT_OVERFLOW（droppedCount, fromSeq）
        let overflow = Frame {
            event_type: EV_EVENT_OVERFLOW,
            seq,
            ts_ms: frame.ts_ms,
            task_id: 0,
            payload: serde_json::to_vec(&serde_json::json!({
                "droppedCount": inner.dropped,
                "fromSeq": seq,
            }))
            .unwrap_or_default(),
        };
        inner.ring.push_back(overflow);
    }
    inner.ring.push_back(frame);
    drop(inner);

    // 推送订阅者（帧生命周期到回调返回）
    let subs = b.subscribers.lock().unwrap_or_else(|e| e.into_inner());
    for sub in subs.iter() {
        let mut f = VsEvent {
            struct_size: VS_EVENT_SIZE,
            event_ver: VS_EVENT_VER,
            event_type,
            seq,
            ts_ms: now_ms(),
            task_id,
            payload_len: payload_vec.len() as u32,
            payload: payload_vec.as_ptr(), // 订阅者回调期间 ring 不触达该内存
        };
        let _ = (sub.callback)(&mut f as *mut VsEvent, sub.user);
    }
}

/// 便捷发布（无任务、空载荷）。
pub fn publish_simple(event_type: u16) {
    publish(event_type, 0, serde_json::json!({}));
}

/// 订阅推送。返回订阅 id（0 = 失败）。
pub fn subscribe(
    callback: extern "C" fn(event: *mut VsEvent, user: *mut std::ffi::c_void) -> i32,
    user: *mut std::ffi::c_void,
) -> u32 {
    let id = bus().next_sub.fetch_add(1, Ordering::Relaxed);
    let mut subs = bus().subscribers.lock().unwrap_or_else(|e| e.into_inner());
    subs.push(Subscriber { id, callback, user });
    id
}

pub fn unsubscribe(sub_id: u32) -> bool {
    let mut subs = bus().subscribers.lock().unwrap_or_else(|e| e.into_inner());
    let before = subs.len();
    subs.retain(|s| s.id != sub_id);
    subs.len() != before
}

/// 拉取缓冲中的事件（推送模型兜底）；返回帧数。
type FrameSink<'a> = dyn FnMut(u16, u64, u64, u32, &[u8]) + 'a;

pub fn poll_events(out: &mut FrameSink) -> usize {
    let b = bus();
    let mut inner = b.inner.lock().unwrap_or_else(|e| e.into_inner());
    let n = inner.ring.len();
    while let Some(f) = inner.ring.pop_front() {
        out(f.event_type, f.seq, f.ts_ms, f.task_id, &f.payload);
    }
    n
}

/// 待处理帧数（diagnostics）。
pub fn pending_events() -> usize {
    bus()
        .inner
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .ring
        .len()
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ==== 任务注册表（docs/v2.0/02 §6.5）====

#[derive(Clone, Debug)]
pub struct TaskSnapshot {
    pub id: u32,
    pub kind: String,
    /// queued / running / done / failed / cancelled（paused/interrupted 是 P8）
    pub state: String,
    pub done_bytes: u64,
    pub total_bytes: u64,
}

struct TaskEntry {
    kind: String,
    state: String,
    done: u64,
    total: u64,
    cancel: std::sync::Arc<AtomicBool>,
}

static TASKS: OnceLock<Mutex<Vec<(u32, TaskEntry)>>> = OnceLock::new();
static NEXT_TASK: AtomicU32 = AtomicU32::new(1);

fn tasks() -> &'static Mutex<Vec<(u32, TaskEntry)>> {
    TASKS.get_or_init(|| Mutex::new(Vec::new()))
}

fn tasks_lock() -> std::sync::MutexGuard<'static, Vec<(u32, TaskEntry)>> {
    tasks().lock().unwrap_or_else(|e| e.into_inner())
}

/// 注册并启动一个后台任务。`body` 在独立线程执行，应周期检查 cancel 标志。
/// 返回 task_id。
pub fn spawn_task<F>(kind: &str, body: F) -> u32
where
    F: FnOnce(&AtomicBool, &dyn Fn(u64, u64)) -> Result<(), String> + Send + 'static,
{
    let id = NEXT_TASK.fetch_add(1, Ordering::Relaxed);
    let cancel = std::sync::Arc::new(AtomicBool::new(false));
    tasks_lock().push((
        id,
        TaskEntry {
            kind: kind.to_string(),
            state: "running".into(),
            done: 0,
            total: 0,
            cancel: cancel.clone(),
        },
    ));
    publish(EV_TASK_QUEUED, id, serde_json::json!({"kind": kind}));

    let progress = {
        move |done: u64, total: u64| {
            if let Some((_, t)) = tasks_lock().iter_mut().find(|(tid, _)| *tid == id) {
                t.done = done;
                t.total = total;
            }
            publish(
                EV_TASK_PROGRESS,
                id,
                serde_json::json!({"doneBytes": done, "totalBytes": total}),
            );
        }
    };
    let cancel_for_body = cancel.clone();
    let kind_owned = kind.to_string();
    std::thread::spawn(move || {
        let result = body(&cancel_for_body, &progress);
        let mut guard = tasks_lock();
        if let Some((_, t)) = guard.iter_mut().find(|(tid, _)| *tid == id) {
            if t.state == "running" {
                t.state = match &result {
                    Ok(()) => "done".into(),
                    Err(_) => "failed".into(),
                };
            }
            let state = t.state.clone();
            drop(guard);
            match state.as_str() {
                "done" => publish(EV_TASK_DONE, id, serde_json::json!({"kind": kind_owned})),
                "failed" => publish(
                    EV_TASK_FAILED,
                    id,
                    serde_json::json!({"kind": kind_owned, "reason": result.err().unwrap_or_default()}),
                ),
                "cancelled" => publish(
                    EV_TASK_CANCELLED,
                    id,
                    serde_json::json!({"kind": kind_owned}),
                ),
                _ => {}
            }
        }
    });
    id
}

pub fn task_list() -> Vec<TaskSnapshot> {
    tasks_lock()
        .iter()
        .map(|(id, t)| TaskSnapshot {
            id: *id,
            kind: t.kind.clone(),
            state: t.state.clone(),
            done_bytes: t.done,
            total_bytes: t.total,
        })
        .collect()
}

pub fn task_status(task_id: u32) -> Option<TaskSnapshot> {
    tasks_lock()
        .iter()
        .find(|(id, _)| *id == task_id)
        .map(|(id, t)| TaskSnapshot {
            id: *id,
            kind: t.kind.clone(),
            state: t.state.clone(),
            done_bytes: t.done,
            total_bytes: t.total,
        })
}

/// 协作式取消：置标志并等线程收敛（资源 5 s 内释放的语义由 body 保证）。
/// 返回 0 = 已请求；7 = 未知任务；12 = 任务已取消（幂等）。
pub fn task_cancel(task_id: u32) -> i32 {
    let mut guard = tasks_lock();
    let Some((_, t)) = guard.iter_mut().find(|(id, _)| *id == task_id) else {
        return crate::ffi::ERR_INVALID_ARG;
    };
    if t.state == "cancelled" {
        return crate::ffi::ERR_CANCELLED;
    }
    if t.state == "running" || t.state == "queued" {
        t.state = "cancelled".into();
        t.cancel.store(true, Ordering::Relaxed);
        drop(guard);
        publish(EV_TASK_CANCELLED, task_id, serde_json::json!({}));
        crate::ffi::OK
    } else {
        // 终态任务上再 cancel 幂等返回 0
        crate::ffi::OK
    }
}

// ==== VsAbiInfo（docs/v2.0/02 §6.3）====

/// ABI v2 握手结构（C 侧 `VsAbiInfo`）。
#[repr(C)]
pub struct VsAbiInfo {
    pub struct_size: u32,
    pub abi_version: u16,
    pub reserved0: u16,
    pub capability_bits: u64,
    pub max_write_ver_vault: u32,
    pub max_write_ver_container: u32,
    pub max_write_ver_index: u32,
    pub max_write_ver_audit: u32,
    pub min_read_ver_vault: u32,
    pub feature_flags: u32,
    pub engine_version: [std::ffi::c_char; 32],
}

/// 引擎版本字节（SemVer，NUL 填充）。
pub fn engine_version_bytes() -> [std::ffi::c_char; 32] {
    let ver = env!("CARGO_PKG_VERSION");
    let mut out = [0i8; 32];
    for (i, b) in ver.bytes().take(31).enumerate() {
        out[i] = b as std::ffi::c_char;
    }
    out
}
