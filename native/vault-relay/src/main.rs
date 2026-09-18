//! VaultSync 自建中继服务端（协议见 docs/08）。
//!
//! 零信任语义：
//! 1. 不落盘——进程无持久化存储，转发缓冲即用即弃；
//! 2. 不解密——仅按房间令牌配对两条连接后原样互拷字节，两端业务在本端 Noise 信道内；
//! 3. 不溯源——只输出匿名计量（房间号哈希前缀、转发字节数），不记录地址/公钥。
//!
//! 协议：客户端连接后首行发送 `VSR1 <room>`；同房间第二个客户端到达后，两端字节双向
//! 透明转发直至任一端断开。房间空闲 10 分钟自动回收。
//!
//! 用法：vault-relay [bind_addr:port]（默认 0.0.0.0:47471）
#![deny(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const MAGIC: &str = "VSR1";
const ROOM_TTL: Duration = Duration::from_secs(10 * 60);
const MAX_ROOMS: usize = 1024;

struct Waiting {
    stream: TcpStream,
    created: std::time::Instant,
}

fn main() {
    let addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "0.0.0.0:47471".to_string());
    let listener = match TcpListener::bind(&addr) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("vault-relay: cannot bind {addr}: {e}");
            std::process::exit(1);
        }
    };
    println!("vault-relay listening on {addr} (no-disk, no-plaintext, metering only)");
    let rooms: Arc<Mutex<HashMap<String, Waiting>>> = Arc::new(Mutex::new(HashMap::new()));
    for stream in listener.incoming().flatten() {
        let rooms = Arc::clone(&rooms);
        let _ = std::thread::Builder::new()
            .name("relay-conn".into())
            .spawn(move || handle(stream, rooms));
    }
}

fn handle(mut stream: TcpStream, rooms: Arc<Mutex<HashMap<String, Waiting>>>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let mut line = String::new();
    if std::io::BufReader::new(&stream)
        .read_line(&mut line)
        .is_err()
    {
        return;
    }
    let mut parts = line.trim().split_whitespace();
    if parts.next() != Some(MAGIC) {
        let _ = stream.write_all(b"ERR bad-magic\n");
        return;
    }
    let room = parts.next().unwrap_or("").to_string();
    if room.is_empty()
        || room.len() > 128
        || !room
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        let _ = stream.write_all(b"ERR bad-room\n");
        return;
    }
    let _ = stream.set_read_timeout(None);
    let first = {
        let mut map = rooms.lock().unwrap_or_else(|e| e.into_inner());
        // 回收过期房间
        let expired: Vec<String> = map
            .iter()
            .filter(|(_, w)| w.created.elapsed() > ROOM_TTL)
            .map(|(k, _)| k.clone())
            .collect();
        for k in expired {
            map.remove(&k);
        }
        if map.len() >= MAX_ROOMS {
            let _ = stream.write_all(b"ERR busy\n");
            return;
        }
        match map.remove(&room) {
            Some(w) => {
                let _ = stream.write_all(b"OK\n");
                w.stream
            }
            None => {
                let _ = stream.write_all(b"OK\n");
                map.insert(
                    room.clone(),
                    Waiting {
                        stream,
                        created: std::time::Instant::now(),
                    },
                );
                return;
            }
        }
    };
    // 房间配对完成：双向透明转发（密文比特搬运，不解释内容）
    let meter = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let meter2 = Arc::clone(&meter);
    let mut a = first;
    let mut b = stream;
    let (mut a_rd, mut b_rd) = match (a.try_clone(), b.try_clone()) {
        (Ok(x), Ok(y)) => (x, y),
        _ => return,
    };
    let up = std::thread::spawn(move || {
        let n = std::io::copy(&mut a_rd, &mut b).unwrap_or(0);
        meter2.fetch_add(n, std::sync::atomic::Ordering::Relaxed);
    });
    let down = std::io::copy(&mut b_rd, &mut a).unwrap_or(0);
    let _ = up.join();
    // 匿名计量：仅房间名哈希前缀与字节数（docs/08 §二.3）
    let total = meter.load(std::sync::atomic::Ordering::Relaxed) + down;
    println!("room {:08x} relayed {total} bytes", hash_room(&room));
}

fn hash_room(room: &str) -> u64 {
    // FNV-1a，仅用于日志匿名化
    let mut h: u64 = 0xcbf29ce484222325;
    for b in room.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}
