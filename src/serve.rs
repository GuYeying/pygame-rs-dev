//! 内置静态文件服务器（`python.exe serve [目录] [端口]`）：web 宿主
//! 页面的零依赖打开方式——ES module import 在 file:// 下被 CORS 拦截
//! （origin null），必须走 HTTP；开发者机器上没有 CPython/Node 也无需
//! 装任何工具，本二进制自身即服务器（std TcpListener，每连接一线程）。

use std::{
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::Path,
};

fn mime(p: &str) -> &'static str {
    if p.ends_with(".html") {
        "text/html; charset=utf-8"
    } else if p.ends_with(".js") {
        "application/javascript"
    } else if p.ends_with(".wasm") {
        "application/wasm"
    } else if p.ends_with(".py") {
        "text/plain; charset=utf-8"
    } else if p.ends_with(".ttf") {
        "font/ttf"
    } else {
        "application/octet-stream"
    }
}

/// 阻塞服务（永不返回；Ctrl-C 收场）。`dir` = 站点根（缺省 web/），
/// `port` 缺省 8123。
pub fn serve(dir: &str, port: u16) -> ! {
    let addr = format!("127.0.0.1:{port}");
    let listener = TcpListener::bind(&addr).unwrap_or_else(|e| {
        eprintln!("[pygame-rs-dev] serve 绑定 {addr} 失败: {e}");
        std::process::exit(1);
    });
    println!("[pygame-rs-dev] 静态服务 {dir} → http://{addr}/（Ctrl-C 停止）");
    for stream in listener.incoming() {
        let mut s = match stream {
            Ok(s) => s,
            Err(_) => continue,
        };
        let root = dir.to_string();
        std::thread::spawn(move || {
            let mut buf = [0u8; 2048];
            let _ = s.read(&mut buf);
            let req = String::from_utf8_lossy(&buf);
            let path = req.split_whitespace().nth(1).unwrap_or("/");
            let path = path.split('?').next().unwrap_or("/");
            let rel = if path == "/" { "/index.html" } else { path };
            let file = Path::new(&root).join(rel.trim_start_matches('/'));
            match fs::read(&file) {
                Ok(body) => {
                    // no-cache：重建后 glue/wasm 必须重新拉取（无缓存头时
                    // Chromium 启发式缓存会让浏览器拿旧产物——2026-10-26 实测坑）
                    let head = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n",
                        mime(rel),
                        body.len()
                    );
                    let _ = s.write_all(head.as_bytes());
                    let _ = s.write_all(&body);
                }
                Err(_) => {
                    let _ = s.write_all(
                        b"HTTP/1.1 404 NOT FOUND\r\nContent-Length: 9\r\n\r\nnot found",
                    );
                }
            }
        });
    }
    unreachable!("listener.incoming() 永不结束")
}
