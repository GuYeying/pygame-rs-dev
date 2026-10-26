//! 构建脚本：Windows(MSVC) 主线程栈 Reserve 抬高（RP 的 with_recursion
//! 守卫量真实 C 栈指针——debug 构建引导冻结 importlib 的原生帧深
//! >768KB 即撞默认 1MB 栈 → essential initialization failed）。
//! Reserve ≠ Commit，不用不占内存；CPython 的 python.exe 同款做法。

fn main() {
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if os == "windows" && env == "msvc" {
        println!("cargo:rustc-link-arg-bins=/STACK:16777216");
    }
}
