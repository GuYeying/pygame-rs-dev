//! Web 宿主实现（[`Host`](crate::host::Host) 的 wasm 分支）。
//!
//! **目标定位**（2026-10-26 定约）：host 编译一次，之后任何游戏免重编译
//! ——运行载荷 = **单文件 bundle.py**（自含模块表 + main），宿主差异
//! 只剩"怎么拿到它"：URL = 页面查询参数 `?script=<路径>`（相对页面
//! 地址；缺省 `main.py`——打包产物按此名部署即可）。
//!
//! 载荷获取 = [`starfish_io::read`]（web 后端 = fetch，启动期一次；
//! 相对路径基准 = 页面地址——io 基准目录语义与桌面"main 所在目录"
//! 对齐：产物部署保持目录结构，资源调用写法零改写）。
//!
//! 模块表：web 端 `module_root = None`（无文件系统扫描）——多模块
//! 游戏以 bundle 交付（bundle 自含模块表，import 走内存）；散文件
//! 开发保持桌面端（modfind）。
//!
//! 入口 = `web_start`（`#[wasm_bindgen(start)]`，async 支持）；
//! `main` 为 wasm bin 的 rustc 存根（见 main.rs cfg 分支）。

use crate::host::RunRequest;
use starfish_std::debug::console_log;
use wasm_bindgen::prelude::wasm_bindgen;

pub struct WebHost {
    script_url: String,
}

impl WebHost {
    /// 从页面查询参数装配：`?script=<路径>`，缺省 `main.py`
    pub fn from_query() -> Self {
        let search = web_sys::window()
            .and_then(|w| w.location().search().ok())
            .unwrap_or_default();
        let script = search
            .strip_prefix('?')
            .map(|q| {
                q.split('&').find_map(|kv| {
                    let (k, v) = kv.split_once('=')?;
                    (k == "script").then(|| v.to_string())
                })
            })
            .flatten()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "main.py".to_string());
        console_log(&format!("[pygame-rs-dev] web 载荷 = {script}（?script= 可指定）"));
        Self { script_url: script }
    }
}

impl crate::host::Host for WebHost {
    fn name(&self) -> &'static str {
        "pygame-rs-dev"
    }

    /// web 无命令行余参（v1；将来可透传查询参数）
    fn args(&self) -> Vec<String> {
        Vec::new()
    }

    async fn acquire(&self) -> Result<RunRequest, String> {
        let data = starfish_io::read(&self.script_url)
            .await
            .map_err(|e| format!("载荷下载失败 {}: {e}", self.script_url))?;
        let code = String::from_utf8(data)
            .map_err(|e| format!("载荷非 UTF-8（{}）: {e}", self.script_url))?;
        // module_root = None：web 无文件系统扫描——多模块游戏以 bundle
        // 交付（bundle 内置模块表，import 全内存）
        Ok(RunRequest {
            name: self.script_url.clone(),
            code,
            module_root: None,
        })
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
    let host = WebHost::from_query();
    let code = crate::runtime::run(&host).await;
    if code != 0 {
        console_log(&format!("[pygame-rs-dev] 运行结束（退出码 {code}）"));
    }
    Ok(())
}
