//! pygame-rs-dev 宿主运行时库：[`host`]（平台宿主 trait 接口）+
//! [`runtime`]（平台零感知通用驱动）+ [`platform`]（各平台 Host 实现）。
//!
//! 目标形态（2026-10-26 定约）：**全平台免重编译的运行调试运行时**——
//! 同一份游戏（单文件/bundle）桌面 `python.exe` 直跑、web 单 fetch、
//! 移动端 dialog 选文件（-c 形态），运行语义三端一致。
//!
//! 目标划分：
//! - bin `python`（桌面 CLI 壳）→ [`platform::desktop::desktop_main`]；
//! - example `android_host`（Android 调试壳 cdylib）→
//!   [`platform::mobile`]（dialog 载荷）；
//! - wasm：[`platform::web`]（?script= / ?code= / load_code）。
//!
//! 桌面专属工具 [`serve`]（内置静态服务器）亦在库内（desktop_main 分派）。

pub mod host;
pub mod platform;
pub mod runtime;
pub mod serve;
