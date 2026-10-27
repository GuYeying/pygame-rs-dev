//! pygame-rs-dev Android 调试壳（cdylib）：dialog 选文件 → 运行（-c 形态）。
//!
//! 单文件运行调试的移动形态（与桌面 `python.exe game.py`、web 文件选择
//! 通道同构）；多模块游戏随 pack 仓（bundle 嵌入模板，后续批次）。
//!
//! 构建（家族链）：
//!   cargo ndk -t arm64-v8a -o jniLibs --release --example android_host
//!   （robius 前置：ANDROID_JAR 环境变量；APK 组装用
//!    pygame-rs/scripts/build_apk.py android_host --dir ..）
//!
//! 真机：安装 APK → 启动即弹文件选择 → 选 .py 运行（io 基准 = app
//! 私有目录——资源随文件同目录部署）。

use pygame_rs_dev::platform::mobile;

async fn run() -> Result<(), String> {
    let code = mobile::pick_and_run().await;
    if code != 0 {
        return Err(format!("退出码 {code}"));
    }
    Ok(())
}

starfish_std::app_entry!(run());
