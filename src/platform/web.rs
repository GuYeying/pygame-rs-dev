//! Web 宿主实现（[`Host`](crate::host::Host) 的 wasm 分支）。
//!
//! **目标定位**（2026-10-26 定约）：host 编译一次，之后任何游戏免重编译
//! ——运行载荷 = 单文件（bundle.py 自含模块表 + main，或单文件脚本），
//! 宿主差异只剩"怎么拿到它"。两条载荷通道：
//!
//! 1. **URL 拉取**：页面查询参数 `?script=<路径>`（相对页面地址，缺省
//!    `main.py`）→ [`starfish_io::read`]（fetch，启动期一次）；
//! 2. **本地文件**：页面 `<input type=file>` 读入 → 调导出函数
//!    [`load_payload`]（字节注入槽位）——单文件运行/调试入口。
//!
//! 两条通道幂等守门（**进程一次契约**：本页只跑一个程序，换游戏刷新
//! 页面）。web 端 `module_root = None`（无文件系统扫描）——多模块游戏
//! 以 bundle 交付（import 走 bundle 内置模块表，全内存）；散文件开发
//! 保持桌面端（modfind）。
//!
//! 入口 = `web_start`（`#[wasm_bindgen(start)]`，async 支持）；
//! `main` 为 wasm bin 的 rustc 存根（见 main.rs cfg 分支）。

use crate::host::RunRequest;
use starfish_std::debug::console_log;
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use wasm_bindgen::prelude::wasm_bindgen;

/// 页面选入的载荷槽（单线程 wasm，RefCell 足矣）
thread_local! {
    static PAYLOAD: RefCell<Option<(String, Vec<u8>)>> = const { RefCell::new(None) };
}

/// 进程一次契约守门（本页只运行一个程序；换游戏 = 刷新页面）
static STARTED: AtomicBool = AtomicBool::new(false);

fn query_script() -> Option<String> {
    let search = web_sys::window()
        .and_then(|w| w.location().search().ok())
        .unwrap_or_default();
    search
        .strip_prefix('?')
        .map(|q| {
            q.split('&').find_map(|kv| {
                let (k, v) = kv.split_once('=')?;
                (k == "script").then(|| v.to_string())
            })
        })
        .flatten()
        .filter(|s| !s.is_empty())
}

/// URL 拉取宿主（?script= 指定；缺省 main.py）
struct WebHost {
    script_url: String,
}

impl WebHost {
    fn from_url(url: String) -> Self {
        console_log(&format!(
            "[pygame-rs-dev] web 载荷 = {url}（fetch；?script= 可指定）"
        ));
        Self { script_url: url }
    }
}

impl crate::host::Host for WebHost {
    fn name(&self) -> &'static str {
        "pygame-rs-dev"
    }

    /// web 无命令行余参（v1）
    fn args(&self) -> Vec<String> {
        Vec::new()
    }

    async fn acquire(&self) -> Result<RunRequest, String> {
        let data = starfish_io::read(&self.script_url)
            .await
            .map_err(|e| format!("载荷下载失败 {}: {e}", self.script_url))?;
        let code = String::from_utf8(data)
            .map_err(|e| format!("载荷非 UTF-8（{}）: {e}", self.script_url))?;
        Ok(RunRequest {
            name: self.script_url.clone(),
            code,
            module_root: None,
        })
    }
}

/// 本地文件宿主（页面 file input → load_payload 注入）
struct PickedHost {
    name: String,
    code: String,
}

impl crate::host::Host for PickedHost {
    fn name(&self) -> &'static str {
        "pygame-rs-dev"
    }

    fn args(&self) -> Vec<String> {
        Vec::new()
    }

    async fn acquire(&self) -> Result<RunRequest, String> {
        Ok(RunRequest {
            name: self.name.clone(),
            code: self.code.clone(),
            module_root: None,
        })
    }
}

/// 启动闸门（幂等）：载荷槽有内容 → 文件宿主；否则 ?script= → 拉取宿主；
/// 两者皆无 → 返回 false（等待 load_payload，不启动）
fn try_start() -> bool {
    if STARTED.swap(true, Ordering::SeqCst) {
        console_log("[pygame-rs-dev] 进程一次契约：本页已运行过程序——换游戏请刷新页面");
        return true;
    }
    let payload = PAYLOAD.with(|p| p.borrow_mut().take());
    let script_url = query_script();
    match (payload, script_url) {
        (Some((name, bytes)), _) => {
            let code = match String::from_utf8(bytes) {
                Ok(c) => c,
                Err(e) => {
                    console_log(&format!("[pygame-rs-dev] 载荷非 UTF-8（{name}）: {e}"));
                    return true;
                }
            };
            console_log(&format!("[pygame-rs-dev] 本地文件载荷 = {name}"));
            wasm_bindgen_futures::spawn_local(async move {
                let host = PickedHost { name, code };
                let code = crate::runtime::run(&host).await;
                if code != 0 {
                    console_log(&format!("[pygame-rs-dev] 运行结束（退出码 {code}）"));
                }
            });
            true
        }
        (None, Some(url)) => {
            wasm_bindgen_futures::spawn_local(async move {
                let host = WebHost::from_url(url);
                let code = crate::runtime::run(&host).await;
                if code != 0 {
                    console_log(&format!("[pygame-rs-dev] 运行结束（退出码 {code}）"));
                }
            });
            true
        }
        (None, None) => {
            STARTED.store(false, Ordering::SeqCst);
            false
        }
    }
}

/// wasm 真入口（async 支持；wasm-bindgen start 属性）
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub async fn web_start() -> Result<(), wasm_bindgen::JsValue> {
    // panic 消息转发控制台（wasm32 panic=abort——默认只留 unreachable 陷阱）
    std::panic::set_hook(Box::new(|info| {
        console_log(&format!("[panic] {info}"));
    }));
    if !try_start() {
        console_log("[pygame-rs-dev] 等待载荷：选择本地 .py/bundle 文件，或 ?script= 指定 URL");
    }
    Ok(())
}

/// 页面文件选择注入（单文件运行入口）：JS 侧 `exports.load_payload(名字, 字节)`
/// → 注册载荷并启动运行。UTF-8 校验失败在运行期以 traceback 呈现。
#[wasm_bindgen]
pub fn load_payload(name: String, bytes: Vec<u8>) {
    PAYLOAD.with(|p| *p.borrow_mut() = Some((name.clone(), bytes)));
    console_log(&format!("[pygame-rs-dev] 载荷注入 = {name}"));
    try_start();
}
