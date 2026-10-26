//! Web 宿主实现（[`Host`](crate::host::Host) 的 wasm 分支）。
//!
//! **设计定约（2026-10-26 用户定夺）**：web 端 = **python 基本指令的
//! 对接映射**——
//! - `python -c "<代码>"` ↔ 页面文件选择/控制台注入：JS 读到的代码
//!   串经导出函数 [`load_code`] 直传（不落盘、无文件名语义）；
//! - `python <路径>` ↔ `?script=<路径>`（缺省 main.py）→
//!   [`starfish_io::read`]（web 后端 = fetch，启动期一次）。
//!
//! 通道幂等守门（**进程一次契约**：本页只跑一个程序，换游戏刷新页面）。
//! 模块表：web 端 `module_root = None`（无文件系统扫描；wasm 的 bundle
//! 处理延后）——多模块游戏暂以桌面端开发调试为准。
//!
//! 入口 = `web_start`（`#[wasm_bindgen(start)]`，async 支持）；
//! `main` 为 wasm bin 的 rustc 存根（见 main.rs cfg 分支）。

use crate::host::RunRequest;
use starfish_std::debug::console_log;
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use wasm_bindgen::prelude::wasm_bindgen;

/// `-c` 形态的代码槽（页面文件选择/控制台注入）
thread_local! {
    static CODE: RefCell<Option<String>> = const { RefCell::new(None) };
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

/// 路径形态宿主（`python <路径>` 映射：io fetch 加载）
struct ScriptHost {
    script_url: String,
}

impl ScriptHost {
    fn from_url(url: String) -> Self {
        console_log(&format!(
            "[pygame-rs-dev] web 载荷 = {url}（fetch；?script= 可指定）"
        ));
        Self { script_url: url }
    }
}

impl crate::host::Host for ScriptHost {
    fn name(&self) -> &'static str {
        "pygame-rs-dev"
    }

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

/// `-c` 形态宿主（JS 直传代码串）
struct CodeHost {
    code: String,
}

impl crate::host::Host for CodeHost {
    fn name(&self) -> &'static str {
        "pygame-rs-dev"
    }

    fn args(&self) -> Vec<String> {
        Vec::new()
    }

    async fn acquire(&self) -> Result<RunRequest, String> {
        Ok(RunRequest {
            name: "<pygame-rs-dev -c>".to_string(),
            code: self.code.clone(),
            module_root: None,
        })
    }
}

/// 启动闸门（幂等）：代码槽有内容 → `-c` 形态；否则 `?script=` → 路径
/// 形态；两者皆无 → 返回 false（等待 load_code，不启动）
fn try_start() -> bool {
    if STARTED.swap(true, Ordering::SeqCst) {
        console_log("[pygame-rs-dev] 进程一次契约：本页已运行过程序——换游戏请刷新页面");
        return true;
    }
    let code = CODE.with(|c| c.borrow_mut().take());
    let script_url = query_script();
    match (code, script_url) {
        (Some(code), _) => {
            console_log("[pygame-rs-dev] 载荷 = -c 代码串（页面注入）");
            wasm_bindgen_futures::spawn_local(async move {
                let host = CodeHost { code };
                let code = crate::runtime::run(&host).await;
                if code != 0 {
                    console_log(&format!("[pygame-rs-dev] 运行结束（退出码 {code}）"));
                }
            });
            true
        }
        (None, Some(url)) => {
            wasm_bindgen_futures::spawn_local(async move {
                let host = ScriptHost::from_url(url);
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
        console_log("[pygame-rs-dev] 等待载荷：页面选择本地 .py 文件（-c 形态），或 ?script= 指定 URL");
    }
    Ok(())
}

/// 页面代码注入（`python -c` 映射）：JS 侧读入的代码串直传运行。
/// `exports.load_code(代码串)`——UTF-8/语法问题在运行期以 traceback 呈现。
#[wasm_bindgen]
pub fn load_code(code: String) {
    CODE.with(|c| *c.borrow_mut() = Some(code));
    console_log("[pygame-rs-dev] 载荷注入 = -c 代码串");
    try_start();
}
