//! 平台 Host 实现（每平台一文件，实现 [`crate::host::Host`]）。
//! 平台差异只允许在 acquire（拿载荷）与平台 main 的收尾（退出码语义）。
#[cfg(not(target_arch = "wasm32"))]
pub mod desktop;
#[cfg(target_arch = "wasm32")]
pub mod web;
// pub mod android;  // 后续批次：dialog 选文件载荷（调试器 app 形态）
