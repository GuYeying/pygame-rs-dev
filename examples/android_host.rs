//! pygame-rs-dev Android 嵌入式调试 .so（cdylib）：**导出给原生 Android
//! 开发者的接口面**——宿主 app 用自己的原生 UI 选文件，把 Surface 和
//! 代码交给本运行时。
//!
//! ## Kotlin 接入（约 20 行）
//! ```kotlin
//! object PygameRsDev {
//!     init { System.loadLibrary("android_host") }
//!     external fun attachSurface(surface: Surface)
//!     external fun surfaceResized(w: Int, h: Int)
//!     external fun detachSurface()
//!     external fun setBaseDir(path: String)   // io 基准（游戏目录）
//!     external fun loadCode(code: String)     // 运行（-c 形态）
//!     external fun stop()                     // 帧边界收口
//! }
//! // SurfaceView 生命周期回调 → attachSurface/surfaceResized/detachSurface
//! // 选好文件读入文本 → loadCode(text) → 全屏 pygame 运行
//! ```
//!
//! ## 契约
//! - 游戏文件（.py + resources/）仅放**调用方 app 私有目录**（std::fs
//!   直读 + set_base_dir 基准——零 SAF/裸接口处理）；
//! - 进程一次契约：本 .so 一轮只跑一个程序，切换 = 调 stop + 重进；
//! - 引擎跑在 .so 自起线程（不占宿主 UI 主线程）。
//!
//! 构建：cargo ndk -t arm64-v8a -o jniLibs -- build --release --example android_host
//! （robius 前置：ANDROID_JAR 环境变量；APK 组装用
//!  pygame-rs/scripts/build_apk.py android_host --dir ..）

use pygame_rs_dev::platform::mobile;
use starfish_we::embedded;

// ── libandroid.so 原生符号（尺寸读取用）────────────────────────
extern "C" {
    fn ANativeWindow_getWidth(window: *mut std::ffi::c_void) -> i32;
    fn ANativeWindow_getHeight(window: *mut std::ffi::c_void) -> i32;
}

/// jstring → String（GetStringUTFChars/Release 直连；替换行为 lossy）
///
/// # Safety
/// `env`/`s` 必须为有效 JNI 句柄。
unsafe fn jstring_to_string(env: jni_sys::JNIEnv, s: jni_sys::jstring) -> String {
    // jni-sys 函数表形参 = *mut JNIEnv（双层指针）——env 转可变传入
    let env = env as *mut jni_sys::JNIEnv;
    let get = (*(*env)).GetStringUTFChars.expect("GetStringUTFChars missing");
    let ptr = get(env, s, std::ptr::null_mut());
    let text = std::ffi::CStr::from_ptr(ptr).to_string_lossy().into_owned();
    let release = (*(*env)).ReleaseStringUTFChars.expect("ReleaseStringUTFChars missing");
    release(env, s, ptr);
    text
}

// ── 独立运行形态（NativeActivity/glue）：android_main → 载荷裁决 ──
// 脚本 = internalDataPath/main.py（adb run-as 推送）；缺省 = 内置演示
//（触摸画线）。io 基准 = starfish 自动注入的 app 私有目录。
#[cfg(target_os = "android")]
const DEMO: &str = r#"import pygame
async def main():
    screen = pygame.display.set_mode((480, 320))
    f = 0
    pts = []
    while True:
        for e in pygame.event.get():
            if e.type == pygame.QUIT:
                return
            if e.type == pygame.MOUSEMOTION:
                pts.append((e.pos[0], e.pos[1]))
                if len(pts) > 300:
                    pts.pop(0)
        with screen.render() as r:
            r.fill((255, 140, 0))
            for i in range(1, len(pts)):
                pygame.draw.line(r, (30, 30, 30), pts[i - 1], pts[i], 5)
        pygame.display.flip()
        await pygame.next_frame()
"#;

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: android_activity::AndroidApp) {
    // ── 诊断日志：写到 app 外部存储目录（/sdcard/Android/data/<包名>/files/）
    // ——USB 文件传输**免 root 直拉**，比 internalDataPath 的 run-as 便捷──
    let base = app
        .external_data_path()
        .and_then(|p| p.into_os_string().into_string().ok())
        .unwrap_or_else(|| {
            app.internal_data_path()
                .and_then(|p| p.into_os_string().into_string().ok())
                .unwrap_or_else(|| ".".to_string())
        });
    let log_path = format!("{base}/dev_log.txt");
    let log_path_for_dlog = log_path.clone();
    let dlog = move |s: &str| {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().append(true).create(true).open(&log_path_for_dlog) {
            let _ = writeln!(f, "{s}");
        }
        starfish_std::debug::console_log(&format!("[pygame-rs-dev] {s}"));
    };
    // panic 钩子：崩溃信息也落入同一文件
    let log_path_clone = log_path.clone();
    std::panic::set_hook(Box::new(move |info| {
        let msg = format!("[PANIC] {info}");
        if let Ok(mut f) = std::fs::OpenOptions::new().append(true).create(true).open(&log_path_clone) {
            use std::io::Write;
            let _ = writeln!(f, "{msg}");
        }
        starfish_std::debug::console_log(&msg);
    }));
    let _ = std::fs::remove_file(&log_path);
    dlog("0 android_main entry");
    starfish_std::we::android_init(app.clone());
    dlog("1 android_init ok");
    starfish_std::run_control::reset();
    let script_path = format!("{base}/main.py");
    let (name, code) = match std::fs::read_to_string(&script_path) {
        Ok(code) => {
            dlog("2 main.py 读取 ok");
            ("main.py", code)
        }
        Err(_) => {
            dlog("2 main.py 不存在 → demo");
            ("demo.py", DEMO.to_string())
        }
    };
    starfish_std::debug::console_log(&format!(
        "[pygame-rs-dev] android 载荷 = {name}（{} B；脚本目录 {base}）",
        code.len()
    ));
    let host = pygame_rs_dev::platform::mobile::MobileHost {
        name: name.to_string(),
        code,
    };
    dlog("3 runtime::run start（16MB 大栈线程——RP genesis 需要）");
    // 大栈线程驱动 RP（glue 线程栈不够 RP genesis；桌面同样需要 16MB）
    let handle = std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            let exit = starfish_std::app::block_on(async move {
                pygame_rs_dev::runtime::run(&host).await
            });
            dlog(&format!("4 runtime::run 完成（退出码 {exit}）"));
            exit
        })
        .expect("spawn engine thread");
    let exit = handle.join().unwrap_or(1);
    // 与 app_entry! 宏同模式：exit 阻止 glue 清理代码执行——
    // android_main 返回后 InputQueue::detachLooper() 会撞已销毁互斥锁
    //（FORTIFY abort，tombstone 实证），直接退出进程绕过
    std::process::exit(exit);
}

#[no_mangle]
pub extern "system" fn Java_pygame_rsdev_PygameRsDev_attachSurface(
    env: jni_sys::JNIEnv,
    _clazz: jni_sys::jclass,
    surface: jni_sys::jobject,
) {
    unsafe { embedded::attach_surface(env as _, surface as _) };
}

#[no_mangle]
pub extern "system" fn Java_pygame_rsdev_PygameRsDev_surfaceResized(
    _env: jni_sys::JNIEnv,
    _clazz: jni_sys::jclass,
    width: i32,
    height: i32,
) {
    embedded::surface_resized(width.max(0) as u32, height.max(0) as u32);
}

#[no_mangle]
pub extern "system" fn Java_pygame_rsdev_PygameRsDev_detachSurface(
    _env: jni_sys::JNIEnv,
    _clazz: jni_sys::jclass,
) {
    embedded::detach_surface();
}

#[no_mangle]
pub extern "system" fn Java_pygame_rsdev_PygameRsDev_setBaseDir(
    env: jni_sys::JNIEnv,
    _clazz: jni_sys::jclass,
    path: jni_sys::jstring,
) {
    let dir = unsafe { jstring_to_string(env, path) };
    starfish_io::set_base_dir(&dir);
}

#[no_mangle]
pub extern "system" fn Java_pygame_rsdev_PygameRsDev_loadCode(
    env: jni_sys::JNIEnv,
    _clazz: jni_sys::jclass,
    code: jni_sys::jstring,
) {
    let code = unsafe { jstring_to_string(env, code) };
    // 引擎自起线程（不占宿主 UI 主线程）；block_on 驱动；帧边界响应 stop
    std::thread::spawn(move || {
        starfish_std::run_control::reset();
        let host = pygame_rs_dev::platform::mobile::MobileHost {
            name: "embedded.py".to_string(),
            code,
        };
        let exit = starfish_std::app::block_on(async move {
            pygame_rs_dev::runtime::run(&host).await
        });
        if exit != 0 {
            starfish_std::debug::console_log(&format!(
                "[pygame-rs-dev] 运行结束（退出码 {exit}）"
            ));
        }
    });
}

#[no_mangle]
pub extern "system" fn Java_pygame_rsdev_PygameRsDev_stop(
    _env: jni_sys::JNIEnv,
    _clazz: jni_sys::jclass,
) {
    starfish_std::run_control::request_stop();
}
