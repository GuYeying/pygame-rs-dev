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
//!     external fun injectTouch(action: Int, x: Float, y: Float)
//!     external fun stop()                     // 帧边界收口
//! }
//! // SurfaceView 生命周期回调 → attachSurface/surfaceResized/detachSurface
//! // View.onTouchEvent → injectTouch（DOWN→0 / MOVE→1 / UP,CANCEL→2）
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

/// 直退系统调用（绕过 atexit/C++ 静态析构窗口）。`std::process::exit`
/// 的 exit() 会先跑 atexit 与静态析构——期间 Java 线程（HeapTaskDaemon
/// 等）仍活着，撞上已销毁互斥锁 → FORTIFY abort → SIGABRT 闪退
///（真机 crash buffer 实证：destroyed mutex @ HeapTaskDaemon）
#[cfg(target_os = "android")]
extern "C" {
    fn _exit(code: i32) -> !;
}

// ── 原生崩溃遗嘱（信号遗嘱）：SIGSEGV/SIGABRT 等致命信号死前往
// dev_log 写标记——区分"Rust panic（panic 钩子已有 [PANIC]）"与
// "原生崩溃（本处理器）"，ANCO 无 tombstone 可取时的定位手段 ──
#[cfg(target_os = "android")]
static DEVLOG_FD: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(-1);

#[cfg(target_os = "android")]
const FATAL_SIGNS: [(i32, &str); 6] = [
    (4, "SIGILL"),
    (6, "SIGABRT"),
    (7, "SIGBUS"),
    (8, "SIGFPE"),
    (11, "SIGSEGV"),
    (31, "SIGSYS"),
];

#[cfg(target_os = "android")]
extern "C" fn crash_handler(sig: i32) {
    // 异步信号安全约束：只允许 write(2) 直写已打开的 fd
    let fd = DEVLOG_FD.load(std::sync::atomic::Ordering::SeqCst);
    if fd >= 0 {
        let name = FATAL_SIGNS
            .iter()
            .find(|(s, _)| *s == sig)
            .map(|(_, n)| *n)
            .unwrap_or("SIGNAL");
        let line = format!(
            "\n=== {name}（原生致命崩溃，非 Rust panic——引擎最后面包屑见上）===\n"
        );
        unsafe { write(fd, line.as_ptr().cast(), line.len()) };
    }
    // 恢复默认处理并重发——系统 tombstone 照常生成
    unsafe {
        signal(sig, 0);
        raise(sig);
    }
}

#[cfg(target_os = "android")]
extern "C" {
    fn signal(sig: i32, handler: usize) -> usize;
    fn write(fd: i32, buf: *const u8, count: usize) -> isize;
    fn raise(sig: i32) -> i32;
}

/// 注册信号遗嘱 + 常驻 dev_log fd（setLogDir 末尾调用；fd 泄漏常驻
/// ——进程级遗嘱通道，一次运行一枚有界；处理器内只写不开——open 非
/// async-signal-safe）
#[cfg(target_os = "android")]
fn install_crash_handlers(log_path: &str) {
    use std::os::unix::io::AsRawFd;
    if let Ok(f) = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(log_path)
    {
        let fd = f.as_raw_fd();
        std::mem::forget(f);
        DEVLOG_FD.store(fd, std::sync::atomic::Ordering::SeqCst);
        for (sig, _) in FATAL_SIGNS {
            unsafe { signal(sig, crash_handler as usize) };
        }
    }
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
    pts = []
    while True:
        for e in pygame.event.get():
            if e.type == pygame.event.QUIT:
                return
            if e.type == pygame.event.MOUSEMOTION:
                pts.append((e.x, e.y))
                if len(pts) > 300:
                    pts.pop(0)
        with screen.render() as r:
            r.fill((255, 140, 0))
            for i in range(1, len(pts)):
                r.line((30, 30, 30), pts[i - 1], pts[i], 5)
        pygame.display.flip()
        await pygame.next_frame()
"#;

// ── 诊断基建：起点时钟 + 盖戳 + 落盘（闪退定位 = 最后一条面包屑）──
#[cfg(target_os = "android")]
static T0: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

/// 盖戳：`[  312ms] 正文`——所有面包屑带相对 android_main 入口的耗时
#[cfg(target_os = "android")]
fn stamp(s: &str) -> String {
    let ms = T0
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_millis();
    format!("[{ms:>5}ms] {s}")
}

/// dev_log.txt 追加一行（外部存储目录——USB 文件传输免 root 直拉）
#[cfg(target_os = "android")]
fn write_diag(log_path: &str, line: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(log_path)
    {
        let _ = writeln!(f, "{line}");
    }
}

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: android_activity::AndroidApp) {
    let _ = T0.set(std::time::Instant::now());
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
    // 诊断 sink 先注入：此后所有 console_log（含 runtime ①-④ / testhost
    // 首帧分段 / panic 短讯）每条同步落盘并统一盖戳——崩溃后 USB 直拉
    // 定位最后一条
    let sink_path = log_path.clone();
    starfish_std::debug::set_diag_sink(Box::new(move |s: &str| {
        write_diag(&sink_path, &stamp(s));
    }));
    let dlog = |s: &str| {
        starfish_std::debug::console_log(&format!("[pygame-rs-dev] {s}"));
    };
    // panic 钩子：崩溃信息 + 强制回溯整块直写文件（release 无符号也留
    // 地址帧——配未 strip 的 .so 可 llvm-symbolizer 还原）
    let panic_path = log_path.clone();
    std::panic::set_hook(Box::new(move |info| {
        let bt = std::backtrace::Backtrace::force_capture();
        let msg = stamp(&format!("[PANIC] {info}\nbacktrace:\n{bt}"));
        write_diag(&panic_path, &msg);
        starfish_std::debug::console_log(&format!("[pygame-rs-dev] [PANIC] {info}"));
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
    let host = pygame_rs_dev::platform::mobile::MobileHost {
        name: name.to_string(),
        code,
    };
    dlog("3 runtime::run start（android_main 线程——事件轮询必须在此线程）");
    // 直接在 android_main 线程上 block_on 驱动（android-activity 的事件
    // 轮询必须从 android_init 所在线程发起——跨线程 poll 会 panic：
    // "Application tried to poll events from non-main thread"）
    let exit = starfish_std::app::block_on(async move {
        pygame_rs_dev::runtime::run(&host).await
    });
    dlog(&format!("4 runtime::run 完成（退出码 {exit}）"));
    // _exit 直退（见顶部 extern 注释）：绕过 exit() 的析构窗口——
    // 该窗口内 Java 线程撞已销毁互斥锁会 FORTIFY SIGABRT（真机实证）
    unsafe { _exit(exit) };
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
    env: jni_sys::JNIEnv,
    _clazz: jni_sys::jclass,
    surface: jni_sys::jobject,
) {
    unsafe { embedded::detach_surface(env as _, surface as _) };
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

/// 触摸注入（宿主 View 的 MotionEvent 转发）。
/// Java 侧换算：ACTION_DOWN(0)→0，ACTION_MOVE(2)→1，ACTION_UP/CANCEL(1,3)→2
///（引擎门面自定义编码：0 = down，1 = move，2 = up——非 MotionEvent 原值）。
/// **JNI 原生类型必须逐一对齐 Java**：int→i32、float→f32（曾把坐标声明为
/// f64——AArch64 上 Java float 走 32 位浮点寄存器，按 f64 读出恒 0/垃圾，
/// "触摸坐标恒 (0,0)"真机实证），f64 在边界扩宽
#[no_mangle]
pub extern "system" fn Java_pygame_rsdev_PygameRsDev_injectTouch(
    _env: jni_sys::JNIEnv,
    _clazz: jni_sys::jclass,
    action: i32,
    x: f32,
    y: f32,
) {
    embedded::inject_touch(action as u8, x as f64, y as f64);
}

#[no_mangle]
pub extern "system" fn Java_pygame_rsdev_PygameRsDev_loadCode(
    env: jni_sys::JNIEnv,
    _clazz: jni_sys::jclass,
    code: jni_sys::jstring,
) {
    let code = unsafe { jstring_to_string(env, code) };
    // 引擎自起线程（不占宿主 UI 主线程）；16MB 大栈（RP genesis 需要——
    // 与独立形态同参数）；block_on 驱动；帧边界响应 stop
    std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            starfish_std::debug::console_log("[pygame-rs-dev] 引擎线程启动");
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
            // 防御性收尾（正常路径 testhost 已 quit；早退分支兜底）：
            // display/混音器槽位是本线程的 thread_local——必须在本线程
            // 清理（音频设备流 Drop 即停，漏了会"退脚本后声音还在"）
            starfish_pygame::display::quit();
            starfish_pygame::mixer::quit();
            starfish_std::debug::console_log("[pygame-rs-dev] 引擎线程收尾（display/mixer 已清）");
            // 引擎已完全停止 → 窗口引用物理释放此刻安全（防
            // present-after-release；槽位已空 = no-op）
            embedded::release_surface();
        })
        .expect("spawn engine thread");
}

/// 绑定宿主 Context（宿主在最早入口调一次；包装层守卫防二次初始化——
/// ndk-context 的 initialize 断言仅容一次）。
///
/// 嵌入形态无 glue：无人调 ndk-glue 的初始化，而 cpal（音频）/ robius
/// （dialog）等 JNI 消费方启动即读 `ndk_context::android_context()`——
/// 未初始化直接 panic（真机实证：mixer 脚本开跑 ~37ms 即崩）。
#[no_mangle]
pub extern "system" fn Java_pygame_rsdev_PygameRsDev_nativeInitContext(
    env: jni_sys::JNIEnv,
    _clazz: jni_sys::jclass,
    context: jni_sys::jobject,
) {
    unsafe {
        let env = env as *mut jni_sys::JNIEnv;
        let get_vm = (*(*env)).GetJavaVM.expect("GetJavaVM missing");
        let mut vm: *mut jni_sys::JavaVM = std::ptr::null_mut();
        if get_vm(env, &mut vm) != 0 {
            starfish_std::debug::console_log("[pygame-rs-dev] GetJavaVM 失败——ndk-context 未绑定");
            return;
        }
        let new_ref = (*(*env)).NewGlobalRef.expect("NewGlobalRef missing");
        let gref = new_ref(env, context);
        ndk_context::initialize_android_context(vm.cast(), gref.cast());
        starfish_std::debug::console_log("[pygame-rs-dev] ndk-context 已绑定（VM + 全局引用）");
    }
}

/// 注入诊断落盘目录（宿主在 loadCode 前调用一次）。
///
/// 嵌入形态无 glue：stdout 无 logcat 重定向（println 不可见），诊断
/// sink 是唯一出口——每条 console_log（含 runtime ①-④ 阶段打点/
/// 首帧分段/panic 短讯）同步落 `<dir>/dev_log.txt`。建议传外部存储
/// 目录（getExternalFilesDir）——USB 免 root 直拉。
#[no_mangle]
pub extern "system" fn Java_pygame_rsdev_PygameRsDev_setLogDir(
    env: jni_sys::JNIEnv,
    _clazz: jni_sys::jclass,
    dir: jni_sys::jstring,
) {
    let dir = unsafe { jstring_to_string(env, dir) };
    let log_path = format!("{dir}/dev_log.txt");
    let sink_path = log_path.clone();
    starfish_std::debug::set_diag_sink(Box::new(move |s: &str| {
        let ms = T0.get_or_init(std::time::Instant::now).elapsed().as_millis();
        write_diag(&sink_path, &format!("[{ms:>6}ms] {s}"));
    }));
    // 常驻 fd + 原生致命信号遗嘱：先删旧文件再打开常驻 fd（遗嘱与
    // 后续面包屑同路径同 inode，不落已删除文件）
    let _ = std::fs::remove_file(&log_path);
    install_crash_handlers(&log_path);
    let panic_path = log_path;
    std::panic::set_hook(Box::new(move |info| {
        let bt = std::backtrace::Backtrace::force_capture();
        let ms = T0.get_or_init(std::time::Instant::now).elapsed().as_millis();
        let msg = format!("[{ms:>6}ms] [PANIC] {info}\nbacktrace:\n{bt}");
        write_diag(&panic_path, &msg);
        starfish_std::debug::console_log(&format!("[pygame-rs-dev] [PANIC] {info}"));
    }));
}

#[no_mangle]
pub extern "system" fn Java_pygame_rsdev_PygameRsDev_stop(
    _env: jni_sys::JNIEnv,
    _clazz: jni_sys::jclass,
) {
    starfish_std::run_control::request_stop();
}
