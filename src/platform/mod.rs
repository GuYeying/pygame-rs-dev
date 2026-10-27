//! 平台 Host 实现（每平台一文件，实现 [`crate::host::Host`]）。
//! 平台差异只允许在 acquire（拿载荷）与平台 main 的收尾（退出码语义）。
//!
//! - desktop：CLI/REPL/serve（桌面 bin 入口）；
//! - web：?script= fetch / load_code 注入（wasm）；
//! - mobile：dialog 选文件（Android / iOS 共用同一契约——robius 实现；
//!   未来鸿蒙 = starfish-we harmony 后端立项后 cfg 门直接扩）。
#[cfg(not(target_arch = "wasm32"))]
pub mod desktop;
#[cfg(target_arch = "wasm32")]
pub mod web;
#[cfg(any(target_os = "android", target_os = "ios"))]
pub mod mobile;
