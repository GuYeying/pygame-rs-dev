//! 通用运行驱动（**平台零感知**）：拿到 [`RunRequest`] 后的全部流程——
//! RP genesis → sys.argv → 模块表 → 字体预载 → 创建期旗标预扫描 → 建窗 →
//! run_script_in（帧桥/协程驱动/traceback）→ 退出码。
//!
//! 各阶段打点沿用（①解释器 ②字体 ③建窗 ④执行 → 首帧分段）——
//! 启动耗时归因仪表，跨平台口径一致。
//!
//! 平台差异只允许存在于 [`crate::host::Host::acquire`]（拿载荷）与
//! 平台 main 的收尾（退出码语义）里，不得下沉到本模块。

use crate::host::{Host, RunRequest};
use pygame_rs::rpy;
use starfish_pygame::display;
use starfish_std::debug::{console_log, Stopwatch};

/// 日志出口 = console_log 单路（桌面 stdout / wasm 控制台——runtime
/// 对平台不可知，web 端 println 不可见故不走 println）
fn log(msg: &str) {
    console_log(msg);
}

/// 驱动一次运行，返回进程退出码（0 = 成功；2 = 载荷不可得；1 = 运行错误
/// ——python 同语义）
pub async fn run<H: Host>(host: &H) -> i32 {
    let name = host.name();
    let t0 = Stopwatch::now();
    // ── 载荷获取（唯一平台差异点）──
    let req: RunRequest = match host.acquire().await {
        Ok(r) => r,
        Err(msg) => {
            log(&format!("[{name}] {msg}"));
            return 2;
        }
    };
    // cwd 切模块根（资源相对路径与脚本预期一致；starfish-io 桌面基准 =
    // cwd——io 正确性的锚点；web/android 端各由平台承载，此调用无害）
    if let Some(root) = &req.module_root {
        let _ = std::env::set_current_dir(root);
    }
    // ── 控制台阶段：建窗前完成全部确定性启动开销（各阶段打点——
    // 顺序与耗时可审计，"白窗等待"归因用）──
    let t_rp = Stopwatch::now();
    let interp = rpy::new_interpreter();
    log(&format!("[{name}] ①解释器就绪 {}ms", t_rp.ms()));
    interp.enter(|vm| set_argv(vm, &req.name, &host.args())); // python 标准 sys.argv
    // 游戏脚本模块加载（python 行为：唯一解析根 = main 所在目录——
    // sys.path[0] 同语义）。modules/、resources/ 仅为打包便利文件夹，
    // 无导入特权：加载 modules 下模块 = import modules.xxx（自然相对
    // 路径）；根缺失则零注入、纯脚本零开销
    if req.module_root.is_some() {
        interp.enter(|vm| match rpy::install_module_finder(vm, &["."]) {
            Ok(0) => {}
            Ok(n) => log(&format!("[{name}] 模块表装载 {n} 个（基准 = main 目录）")),
            Err(e) => log(&format!("[{name}] 模块装载失败: {e}")),
        });
    }
    let t_font = Stopwatch::now();
    rpy::testhost::preload_cjk(name).await;
    log(&format!("[{name}] ②字体预载完 {}ms", t_font.ms()));
    // ── 创建期旗标预扫描（dev 解释器启发式）：TRANSPARENT 等创建期
    // 旗标只在建窗时生效（apply_window_request 的运行时面不覆盖；
    // 后端声明位同样"首次 set_mode 定终身"）——宿主壳预建窗须替脚本
    // 带上。脚本文本含词即置位：注释提及也会命中（保守误报可接受）；
    // 运行期旗标（RESIZABLE 等）不在此列，脚本 set_mode 经
    // apply_window_request 照常生效 ──
    let mut host_flags = 0u32;
    for (token, bit) in [
        ("TRANSPARENT", display::TRANSPARENT),
        ("DIRECTX", display::DIRECTX),
        ("VULKAN", display::VULKAN),
        ("OPENGLES", display::OPENGLES),
        ("OPENGL", display::OPENGL),
        ("HIDDEN", display::HIDDEN),
    ] {
        if req.code.contains(token) {
            host_flags |= bit;
        }
    }
    // ── 建窗（可见；GPU/交换链装配承载于窗口期——百 ms 级）──
    let t_gpu = Stopwatch::now();
    if let Err(e) = display::set_mode_ex((800, 600), host_flags).await {
        log(&format!("[{name}] set_mode 失败: {e}"));
        return 1;
    }
    log(&format!(
        "[{name}] ③建窗+GPU装配完 {}ms（创建期旗标 {host_flags:#b}）",
        t_gpu.ms()
    ));
    log(&format!(
        "[{name}] ④执行 {}（累计 {}ms）",
        req.name,
        t0.ms()
    ));
    // 装配 + 执行统一走 testhost 面（帧桥注入/协程驱动/traceback 全在内）
    if let Err(e) = rpy::testhost::run_script_in(interp, &req.code, &req.name).await {
        log(&format!("[{name}] {e}"));
        return 1;
    }
    0
}

/// python 标准 sys.argv 接线（argv[0] = "-c" / 脚本路径，余者透传）
fn set_argv(vm: &rustpython_vm::VirtualMachine, argv0: &str, rest: &[String]) {
    if let Ok(sys) = vm.import("sys", 0) {
        let mut items: Vec<rustpython_vm::PyObjectRef> =
            vec![vm.ctx.new_str(argv0.to_string()).into()];
        items.extend(rest.iter().map(|a| vm.ctx.new_str(a.clone()).into()));
        let _ = sys.set_attr("argv", vm.ctx.new_list(items), vm);
    }
}
