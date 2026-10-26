//! pygame-rs-dev —— **pygame 的 python**（开发解释器）：pygame 全内置、
//! 零 Rust 工具链，拿到 `python.exe` 配任意编辑器就能开发 pygame 游戏。
//!
//! 架构（2026-10-26 trait 化定约）：
//! - [`host`]：平台宿主 trait 接口（RunRequest + name/args/acquire）；
//! - [`runtime`]：平台零感知通用驱动（genesis/argv/模块表/旗标预扫描/
//!   建窗/run_script_in/退出码）；
//! - [`platform`]：各平台 Host 实现——desktop（本文件即桌面薄壳）为
//!   首个实现者，web/android 后续实现同一 trait。
//!
//! **目标定位**：全平台免重编译的运行调试运行时——同一份游戏（多文件
//! 或 bundle）桌面 `python.exe game/main.py`、wasm 单 fetch、Android
//! 调试器 app dialog 载入，运行语义三端一致。

mod host;
mod platform;
mod runtime;

/// 桌面入口（block_on + 退出码语义；REPL 直入）
#[cfg(not(target_arch = "wasm32"))]
fn main() {
    starfish_std::app::block_on(async move {
        let host = platform::desktop::DesktopHost::from_env();
        if host.wants_repl() {
            platform::desktop::repl_mode(); // 桌面专属交互形态（不返回）
        }
        let code = runtime::run(&host).await;
        if code != 0 {
            std::process::exit(code);
        }
    });
}

/// wasm bin 的 rustc 存根——真入口 = platform::web::web_start
///（`#[wasm_bindgen(start)]`，platform/web.rs）
#[cfg(target_arch = "wasm32")]
fn main() {}
