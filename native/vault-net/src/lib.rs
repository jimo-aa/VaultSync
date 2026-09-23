//! 传输细节层（P8-1/P8-2/P8-3/P8-6，docs/v2.0/02 §三、05-03 §五/§六、05-04 §三）。
//!
//! **边界即禁止项**（`02` §三，架构测试 `tests/arch.rs` 拦截）：
//! 本 crate **不得知道同步语义**——不得出现清单（manifest）、块哈希（chunk hash /
//! block hash）、向量时钟（vector clock / vc）、文件 id（file_id）等概念；
//! `vault-p2p` 才持有「传什么」，这里只有「怎么传」。
//!
//! - [`queue`]：优先级传输队列（并发 3，destroy > interactive > background）；
//! - [`frame`]：填充档位帧层（加密**前**施加填充，05-04 §3.5）；
//! - [`part`]：跨会话断点续传 `.part` 载体（72B 头 + 块索引表）；
//! - [`path`]：路径状态机（Direct → Punched → Relayed，自动降级）。
#![forbid(unsafe_code)]
#![deny(warnings)]

pub mod discovery;
// 引擎层广播/浏览需要同一守护进程类型（vault-p2p 不直接依赖 mdns-sd）
pub use mdns_sd;
pub mod frame;
pub mod part;
pub mod path;
pub mod punch;
pub mod queue;
