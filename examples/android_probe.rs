//! pygame-rs-dev Android 启动探针（native 可执行——adb push 到
//! /data/local/tmp 直接跑，无需装 APK）：逐阶段打点，定位真机上
//! 启动崩溃的具体阶段。
//!
//! 运行：
//!   adb push target/aarch64-linux-android/release/examples/android_probe /data/local/tmp/
//!   adb shell chmod +x /data/local/tmp/android_probe
//!   adb shell /data/local/tmp/android_probe   ← 输出直接打到 adb shell

use pygame_rs::rpy;

fn main() {
    println!("[probe] 0 entry");

    // 1. RP genesis（Android glue 线程上执行）
    println!("[probe] 1 genesis start");
    let interp = rpy::new_interpreter();
    println!("[probe] 1 genesis ok");

    // 2. 模块注入
    interp.enter(|vm| {
        pygame_rs::rpy::install_modules(vm);
    });
    println!("[probe] 2 install_modules ok");

    // 3. 纯 CPU 脚本（无窗口/GPU——隔离 GPU 阶段）
    interp.enter(|vm| {
        let scope = vm.new_scope_with_builtins();
        let code = vm
            .compile("print('script ok')", rustpython_vm::compiler::Mode::Exec, "<probe>".to_string())
            .expect("compile");
        vm.run_code_obj(code, scope).expect("run_code_obj");
    });
    println!("[probe] 3 script ok");

    // 4. 字体路径存在性（fs 检查——无网络）
    let font_ok = std::path::Path::new("/system/fonts").exists();
    println!("[probe] 4 font path exists = {font_ok}");

    // 5. GPU 建窗（崩溃高危区——block_on 驱动 async set_mode）
    println!("[probe] 5 set_mode start");
    match starfish_std::app::block_on(async {
        starfish_pygame::display::set_mode((480, 320)).await
    }) {
        Ok(_) => println!("[probe] 5 set_mode ok"),
        Err(e) => {
            println!("[probe] 5 set_mode ERR: {e:?}");
            return;
        }
    }

    // 6. 帧拍一次（present 链路）
    println!("[probe] 6 flip start");
    starfish_pygame::display::flip();
    println!("[probe] 6 flip ok");

    println!("=== PROBE DONE ===");
}
