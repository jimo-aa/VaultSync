//! 优先级传输队列（P8-1/P8-2，docs/v2.0/02 §6.5、05-03 §五）。
//!
//! - 并发默认 **3**；优先级 **destroy > interactive > background**（取件按
//!   `(prio, 入队序)`），destroy 入队插队首即近似抢占（worker 空闲立即取走）；
//! - `cancel`：协作式——置令牌，作业在下一个检查点停止，**资源 5 s 内释放**
//!   （作业体保证检查点粒度内响应）；终态再取消 → 幂等；
//! - `pause` / `resume`：**保留**任务槽与已收块（`.part` 偏移），状态转 `paused`，
//!   `resume` 在原任务内续做——不是「取消 + 重新入队」（02 §6.5 明文区别）。
//!
//! 本 crate 只调度**不透明作业**：作业体经 [`JobHandle`] 感知取消 / 暂停 / 汇报进度；
//! 队列不知道清单、块哈希、向量时钟、文件 id（边界禁止项，架构测试拦截）。
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Duration;

/// 作业体类型（不透明同步无关单元）。
pub type JobBody = Box<dyn FnOnce(&JobHandle) -> Result<(), String> + Send>;
/// 状态迁移回调。
pub type StateCallback = Box<dyn Fn(u64, &str) + Send + Sync>;
/// 进度回调 (taskId, doneBytes, totalBytes, doneChunks, totalChunks)。
pub type ProgressCallback = Box<dyn Fn(u64, u64, u64, u64, u64) + Send + Sync>;

/// 优先级（02 §6.5）：destroy 最高、interactive（配对/分享/阅后即焚）次之、background 最低。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Prio {
    Destroy,
    Interactive,
    Background,
}

impl Prio {
    pub fn as_str(self) -> &'static str {
        match self {
            Prio::Destroy => "destroy",
            Prio::Interactive => "interactive",
            Prio::Background => "background",
        }
    }
}

/// 任务状态（02 §6.5 任务 JSON 的 state 字段；interrupted 由宿主按断连语义标注）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TaskState {
    Queued,
    Running,
    Paused,
    Done,
    Failed,
    Cancelled,
}

impl TaskState {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskState::Queued => "queued",
            TaskState::Running => "running",
            TaskState::Paused => "paused",
            TaskState::Done => "done",
            TaskState::Failed => "failed",
            TaskState::Cancelled => "cancelled",
        }
    }
}

/// 队列可见的任务快照。
#[derive(Clone, Debug)]
pub struct QueueTaskInfo {
    pub id: u64,
    pub kind: String,
    pub prio: Prio,
    pub state: TaskState,
}

/// 排队 / 运行中作业的共享控制面（cancel / pause / resume 的操作目标）。
struct Slot {
    id: u64,
    kind: String,
    prio: Prio,
    cancel: Arc<AtomicBool>,
    /// true = 请求暂停；作业体在 checkpoint 阻塞于 gate 直到 resume / cancel。
    pause: Arc<AtomicBool>,
    /// true = 允许推进。
    gate: Arc<(Mutex<bool>, Condvar)>,
    state: Mutex<TaskState>,
}

impl Slot {
    fn info(&self) -> QueueTaskInfo {
        QueueTaskInfo {
            id: self.id,
            kind: self.kind.clone(),
            prio: self.prio,
            state: self.state.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        }
    }

    fn open_gate(&self) {
        let (lock, cv) = &*self.gate;
        *lock.lock().unwrap_or_else(|e| e.into_inner()) = true;
        cv.notify_all();
    }
}

struct Job {
    slot: Arc<Slot>,
    body: JobBody,
}

struct QueueInner {
    pending: Mutex<VecDeque<Job>>,
    live: Mutex<Vec<Arc<Slot>>>,
    alive: AtomicBool,
    next_id: AtomicU64,
    on_state: Mutex<Option<StateCallback>>,
    on_progress: Mutex<Option<ProgressCallback>>,
    cv: Condvar,
}

impl QueueInner {
    fn emit_state(&self, id: u64, state: &str) {
        if let Some(f) = self
            .on_state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            f(id, state);
        }
    }
}

/// 优先级传输队列。`concurrency` = 同时运行的作业数上限（生产默认 3，02 §六.5）。
/// 构造期启动等量常驻 worker；`shutdown` / Drop 后 worker 退出。
pub struct TransferQueue {
    inner: Arc<QueueInner>,
}

impl TransferQueue {
    pub fn new(concurrency: usize) -> Self {
        let concurrency = concurrency.max(1);
        let inner = Arc::new(QueueInner {
            pending: Mutex::new(VecDeque::new()),
            live: Mutex::new(Vec::new()),
            alive: AtomicBool::new(true),
            next_id: AtomicU64::new(1),
            on_state: Mutex::new(None),
            on_progress: Mutex::new(None),
            cv: Condvar::new(),
        });
        for i in 0..concurrency {
            let ic = Arc::clone(&inner);
            let _ = std::thread::Builder::new()
                .name(format!("vsnet-worker-{i}"))
                .spawn(move || worker_loop(ic));
        }
        Self { inner }
    }

    /// 注入状态迁移回调（宿主映射为 TASK_QUEUED / TASK_DONE / … 事件）。
    pub fn set_on_state(&self, f: StateCallback) {
        *self
            .inner
            .on_state
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(f);
    }

    /// 注入进度回调 (taskId, doneBytes, totalBytes, doneChunks, totalChunks)。
    pub fn set_on_progress(&self, f: ProgressCallback) {
        *self
            .inner
            .on_progress
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(f);
    }

    /// 入队。destroy 插队首；worker 取件按 `(prio, 入队序)`。
    pub fn submit(&self, kind: &str, prio: Prio, body: JobBody) -> u64 {
        let id = self.inner.next_id.fetch_add(1, Ordering::SeqCst);
        let slot = Arc::new(Slot {
            id,
            kind: kind.to_string(),
            prio,
            cancel: Arc::new(AtomicBool::new(false)),
            pause: Arc::new(AtomicBool::new(false)),
            gate: Arc::new((Mutex::new(true), Condvar::new())),
            state: Mutex::new(TaskState::Queued),
        });
        let job = Job { slot, body };
        {
            let mut q = self.inner.pending.lock().unwrap_or_else(|e| e.into_inner());
            if prio == Prio::Destroy {
                q.push_front(job);
            } else {
                q.push_back(job);
            }
        }
        self.inner.emit_state(id, "queued");
        self.inner.cv.notify_all();
        id
    }

    /// 取消：排队中直接摘除标记；运行 / 暂停中置令牌并唤醒暂停门（作业在
    /// 下一检查点收敛）。返回 0 = 已请求；2 = 已取消（幂等）；7 = 未知任务。
    pub fn cancel(&self, id: u64) -> i32 {
        {
            let mut q = self.inner.pending.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(pos) = q.iter().position(|j| j.slot.id == id) {
                let j = q.remove(pos).expect("pos valid");
                *j.slot.state.lock().unwrap_or_else(|e| e.into_inner()) = TaskState::Cancelled;
                j.slot.cancel.store(true, Ordering::SeqCst);
                drop(q);
                self.inner.emit_state(id, "cancelled");
                return 0;
            }
        }
        let live = self.inner.live.lock().unwrap_or_else(|e| e.into_inner());
        let Some(s) = live.iter().find(|s| s.id == id) else {
            return 7;
        };
        if *s.state.lock().unwrap_or_else(|e| e.into_inner()) == TaskState::Cancelled {
            return 2;
        }
        s.cancel.store(true, Ordering::SeqCst);
        s.open_gate();
        drop(live);
        self.inner.emit_state(id, "cancelled");
        0
    }

    /// 暂停：运行中作业在下一个 checkpoint 转入 paused（槽位与已收块保留）。
    /// 返回 0；7 = 未知 / 已结束；12 = 状态不允许（排队中 / 非 running）。
    pub fn pause(&self, id: u64) -> i32 {
        let live = self.inner.live.lock().unwrap_or_else(|e| e.into_inner());
        let Some(s) = live.iter().find(|s| s.id == id) else {
            return 7;
        };
        if *s.state.lock().unwrap_or_else(|e| e.into_inner()) != TaskState::Running {
            return 12;
        }
        s.pause.store(true, Ordering::SeqCst);
        let (lock, _) = &*s.gate;
        *lock.lock().unwrap_or_else(|e| e.into_inner()) = false;
        *s.state.lock().unwrap_or_else(|e| e.into_inner()) = TaskState::Paused;
        drop(live);
        self.inner.emit_state(id, "paused");
        0
    }

    /// 恢复：paused → 原任务内续做（唤醒暂停门；槽位与进度不动）。
    pub fn resume(&self, id: u64) -> i32 {
        let live = self.inner.live.lock().unwrap_or_else(|e| e.into_inner());
        let Some(s) = live.iter().find(|s| s.id == id) else {
            return 7;
        };
        if *s.state.lock().unwrap_or_else(|e| e.into_inner()) != TaskState::Paused {
            return 12;
        }
        s.pause.store(false, Ordering::SeqCst);
        s.open_gate();
        *s.state.lock().unwrap_or_else(|e| e.into_inner()) = TaskState::Running;
        drop(live);
        self.inner.emit_state(id, "running");
        0
    }

    pub fn list(&self) -> Vec<QueueTaskInfo> {
        let mut out = Vec::new();
        {
            let q = self.inner.pending.lock().unwrap_or_else(|e| e.into_inner());
            for j in q.iter() {
                out.push(j.slot.info());
            }
        }
        let live = self.inner.live.lock().unwrap_or_else(|e| e.into_inner());
        for s in live.iter() {
            out.push(s.info());
        }
        out
    }

    pub fn state_of(&self, id: u64) -> Option<TaskState> {
        self.list()
            .into_iter()
            .find(|t| t.id == id)
            .map(|t| t.state)
    }

    /// 停止全部 worker（测试收尾用；生产进程生命周期 = 队列生命周期）。
    pub fn shutdown(&self) {
        self.inner.alive.store(false, Ordering::SeqCst);
        self.inner.cv.notify_all();
    }
}

impl Drop for TransferQueue {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn worker_loop(inner: Arc<QueueInner>) {
    loop {
        let job = {
            let mut q: MutexGuard<VecDeque<Job>> =
                inner.pending.lock().unwrap_or_else(|e| e.into_inner());
            loop {
                if !inner.alive.load(Ordering::SeqCst) {
                    return;
                }
                if !q.is_empty() {
                    break;
                }
                let (ng, _) = inner
                    .cv
                    .wait_timeout(q, Duration::from_millis(200))
                    .unwrap_or_else(|e| e.into_inner());
                q = ng;
            }
            // 取件：min (prio, 入队序) —— id 单调递增即各优先级内的 FIFO
            let pos = q
                .iter()
                .enumerate()
                .min_by_key(|(_, j)| (j.slot.prio, j.slot.id))
                .map(|(i, _)| i)
                .expect("non-empty checked");
            q.remove(pos).expect("pos valid")
        };
        let slot = Arc::clone(&job.slot);
        {
            let mut l = inner.live.lock().unwrap_or_else(|e| e.into_inner());
            l.push(Arc::clone(&slot));
        }
        *slot.state.lock().unwrap_or_else(|e| e.into_inner()) = TaskState::Running;
        inner.emit_state(slot.id, "running");

        let handle = JobHandle {
            slot: Arc::clone(&slot),
            inner: Arc::clone(&inner),
        };
        let result = (job.body)(&handle);

        let final_state = match &result {
            Ok(()) => TaskState::Done,
            Err(e) if e == "cancelled" => TaskState::Cancelled,
            Err(_) => TaskState::Failed,
        };
        // 终态保留在 live 表（宿主注册表同样保留任务记录，list 语义一致）
        *slot.state.lock().unwrap_or_else(|e| e.into_inner()) = final_state.clone();
        inner.emit_state(slot.id, final_state.as_str());
    }
}

/// 作业体控制句柄。
pub struct JobHandle {
    slot: Arc<Slot>,
    inner: Arc<QueueInner>,
}

impl JobHandle {
    /// 是否已被请求取消。
    pub fn cancelled(&self) -> bool {
        self.slot.cancel.load(Ordering::SeqCst)
    }

    /// 检查点：取消 → Err("cancelled")；暂停中阻塞等待（槽位与已收块保留）。
    /// 作业体应在块边界 / 步骤边界调用（02 §6.5：取消在块边界生效，5s 内释放）。
    pub fn checkpoint(&self) -> Result<(), &'static str> {
        if self.cancelled() {
            return Err("cancelled");
        }
        let (lock, cv) = &*self.slot.gate;
        let mut go = lock.lock().unwrap_or_else(|e| e.into_inner());
        while !*go && !self.cancelled() {
            let (ng, _) = cv
                .wait_timeout(go, Duration::from_millis(200))
                .unwrap_or_else(|e| e.into_inner());
            go = ng;
        }
        if self.cancelled() {
            return Err("cancelled");
        }
        Ok(())
    }

    /// 进度汇报 (doneBytes, totalBytes, doneChunks, totalChunks)；事件节流由宿主负责。
    pub fn progress(&self, done_bytes: u64, total_bytes: u64, done_chunks: u64, total_chunks: u64) {
        let f = self
            .inner
            .on_progress
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(f) = f.as_ref() {
            f(
                self.slot.id,
                done_bytes,
                total_bytes,
                done_chunks,
                total_chunks,
            );
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn settled(queue: &TransferQueue, id: u64, timeout_ms: u64) -> Option<TaskState> {
        let deadline = std::time::Instant::now() + Duration::from_millis(timeout_ms);
        loop {
            if let Some(s) = queue.state_of(id) {
                if matches!(
                    s,
                    TaskState::Done | TaskState::Failed | TaskState::Cancelled
                ) {
                    return Some(s);
                }
            }
            if std::time::Instant::now() > deadline {
                return queue.state_of(id);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// 证据（02 §6.5 / 面板 P8-1）：并发 1 时严格按优先级取件，
    /// destroy 插队首（近似抢占）。
    #[test]
    fn priority_order_respected() {
        let q = TransferQueue::new(1);
        let (tx, rx) = mpsc::channel();
        let order: Arc<Mutex<Vec<&'static str>>> = Arc::new(Mutex::new(Vec::new()));
        for (name, prio) in [
            ("bg1", Prio::Background),
            ("bg2", Prio::Background),
            ("inter", Prio::Interactive),
            ("bg3", Prio::Background),
        ] {
            let ord = Arc::clone(&order);
            let tx2 = tx.clone();
            q.submit(
                name,
                prio,
                Box::new(move |_h| {
                    ord.lock().unwrap().push(name);
                    tx2.send(()).unwrap();
                    Ok(())
                }),
            );
        }
        // destroy 最后入队但必须最先跑（队首插入）
        let ord = Arc::clone(&order);
        let tx2 = tx.clone();
        q.submit(
            "destroy",
            Prio::Destroy,
            Box::new(move |_h| {
                ord.lock().unwrap().push("destroy");
                tx2.send(()).unwrap();
                Ok(())
            }),
        );
        for _ in 0..5 {
            rx.recv_timeout(Duration::from_secs(3)).unwrap();
        }
        q.shutdown();
        assert_eq!(
            *order.lock().unwrap(),
            vec!["destroy", "inter", "bg1", "bg2", "bg3"],
            "并发 1 下取件序 = (prio, FIFO)：destroy 最先，interactive 先于 background"
        );
    }

    /// 证据（02 §6.5）：取消在检查点生效且幂等；5s 内收敛（资源释放）。
    #[test]
    fn cancel_releases_within_5s_and_is_idempotent() {
        let q = TransferQueue::new(1);
        // 作业体每块后等测试门（保证取消时它确定处于运行中）
        let gate = Arc::new((Mutex::new(true), Condvar::new()));
        let g2 = Arc::clone(&gate);
        let id = q.submit(
            "long",
            Prio::Background,
            Box::new(move |h| {
                for _ in 0..3 {
                    h.checkpoint()?;
                    let (l, cv) = &*g2;
                    let mut go = l.lock().unwrap();
                    go = cv.wait_timeout(go, Duration::from_millis(20)).unwrap().0;
                }
                Ok(())
            }),
        );
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while q.state_of(id) != Some(TaskState::Running) {
            assert!(std::time::Instant::now() < deadline, "任务未进入 running");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(q.cancel(id), 0);
        assert_eq!(
            settled(&q, id, 5000),
            Some(TaskState::Cancelled),
            "取消必须 5s 内收敛（作业体在检查点退出，worker 空闲）"
        );
        // 幂等：再取消 → 2（已取消）
        assert_eq!(q.cancel(id), 2);
        q.shutdown();
    }

    /// 证据（02 §6.5）：pause 保留任务槽与「已收块」计数，resume 原任务内续做。
    #[test]
    fn pause_keeps_partial() {
        let q = TransferQueue::new(1);
        let received = Arc::new(AtomicU64::new(0));
        let r2 = Arc::clone(&received);
        // 第 2 块后作业体等测试门——确保暂停时它停在确定位置
        let hold = Arc::new((Mutex::new(false), Condvar::new()));
        let h2 = Arc::clone(&hold);
        let id = q.submit(
            "xfer",
            Prio::Background,
            Box::new(move |h| {
                for i in 0..6u64 {
                    h.checkpoint()?;
                    h.progress(i * 100, 600, i, 6);
                    r2.store(i + 1, Ordering::SeqCst);
                    if i == 1 {
                        let (l, cv) = &*h2;
                        let mut go = l.lock().unwrap();
                        while !*go && !h.cancelled() {
                            go = cv.wait_timeout(go, Duration::from_millis(20)).unwrap().0;
                        }
                    }
                }
                Ok(())
            }),
        );
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            if received.load(Ordering::SeqCst) >= 2 || std::time::Instant::now() > deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(q.pause(id), 0);
        assert_eq!(q.state_of(id), Some(TaskState::Paused));
        let kept = received.load(Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(120));
        assert_eq!(
            received.load(Ordering::SeqCst),
            kept,
            "暂停后作业体冻结在检查点（已收块计数不变）"
        );
        // 放行 + 恢复 → 原任务内续做至完成
        *hold.0.lock().unwrap() = true;
        hold.1.notify_all();
        assert_eq!(q.resume(id), 0);
        assert_eq!(
            settled(&q, id, 3000),
            Some(TaskState::Done),
            "resume 后原任务内续做至完成"
        );
        assert_eq!(received.load(Ordering::SeqCst), 6, "续做不清空已有进度");
        q.shutdown();
    }

    /// destroy 相对 background 的抢占语义：并发 1 时 destroy 后提交者先跑。
    #[test]
    fn destroy_preempts_background_transfer() {
        let q = TransferQueue::new(1);
        let (tx, rx) = mpsc::channel();
        // 占住唯一 worker
        let g = Arc::new((Mutex::new(true), Condvar::new()));
        let g2 = Arc::clone(&g);
        q.submit(
            "bg-long",
            Prio::Background,
            Box::new(move |h| {
                let (l, cv) = &*g2;
                let mut go = l.lock().unwrap();
                while *go {
                    go = cv.wait_timeout(go, Duration::from_millis(50)).unwrap().0;
                    h.checkpoint()?;
                }
                Ok(())
            }),
        );
        std::thread::sleep(Duration::from_millis(50));
        // 先排一个 background，再排 destroy → destroy 先于 background 执行
        let tx_bg = tx.clone();
        q.submit(
            "bg-after",
            Prio::Background,
            Box::new(move |_h| {
                tx_bg.send("bg").unwrap();
                Ok(())
            }),
        );
        q.submit(
            "destroy-cmd",
            Prio::Destroy,
            Box::new(move |_h| {
                tx.send("destroy").unwrap();
                Ok(())
            }),
        );
        // 放行占位任务
        *g.0.lock().unwrap() = false;
        g.1.notify_all();
        let first = rx.recv_timeout(Duration::from_secs(3)).unwrap();
        assert_eq!(first, "destroy", "destroy 必须先于排队的 background 执行");
        let _ = rx.recv_timeout(Duration::from_secs(3)).unwrap();
        q.shutdown();
    }
}
