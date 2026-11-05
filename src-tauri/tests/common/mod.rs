//! A tiny HTTP server for tests: one closure answers every request.
#![allow(dead_code)]

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

pub struct Req {
    pub method: String,
    pub path: String,
    pub query: String,
    pub body: Vec<u8>,
}

pub struct Server {
    pub base: String,
    pub hits: Arc<AtomicUsize>,
    pub log: Arc<Mutex<Vec<String>>>,
}

pub fn start<F>(handler: F) -> Server
where
    F: Fn(&Req, &str) -> (u16, Vec<u8>) + Send + Sync + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let hits = Arc::new(AtomicUsize::new(0));
    let log = Arc::new(Mutex::new(Vec::new()));
    let (h, l, handler, b2) = (hits.clone(), log.clone(), Arc::new(handler), base.clone());
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let (h, l, handler, b2) = (h.clone(), l.clone(), handler.clone(), b2.clone());
            std::thread::spawn(move || {
                let mut stream = stream;
                let mut data = Vec::new();
                let mut buf = [0u8; 8192];
                let header_end;
                loop {
                    let n = stream.read(&mut buf).unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    data.extend_from_slice(&buf[..n]);
                    if let Some(p) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                        header_end = p + 4;
                        break;
                    }
                }
                let head = String::from_utf8_lossy(&data[..header_end]).to_string();
                let len: usize = head.lines().find_map(|l| l.to_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse().unwrap_or(0))).unwrap_or(0);
                while data.len() < header_end + len {
                    let n = stream.read(&mut buf).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    data.extend_from_slice(&buf[..n]);
                }
                let mut first = head.lines().next().unwrap_or("").split_whitespace();
                let method = first.next().unwrap_or("").to_string();
                let target = first.next().unwrap_or("/").to_string();
                let (path, query) = target.split_once('?').map(|(a, b)| (a.to_string(), b.to_string())).unwrap_or((target.clone(), String::new()));
                h.fetch_add(1, Ordering::SeqCst);
                l.lock().unwrap().push(format!("{method} {target}"));
                let req = Req { method, path, query, body: data[header_end..].to_vec() };
                let (status, body) = handler(&req, &b2);
                let _ = write!(stream, "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                let _ = stream.write_all(&body);
            });
        }
    });
    Server { base, hits, log }
}
