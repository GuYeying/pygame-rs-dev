//! pygame-rs-dev —— **开发解释器**（"pygame 的 python"）：开发者日常工具，
//! pygame 全内置、零 Rust 工具链。与 pygame-runner（分发载体，只认
//! 游戏门）分工——见 reference/开发与游戏打包体系设计稿。
//!
//! 命令行 = **python 标准三形态**（批次 60 定约：--game/目录形态退役）：
//! - `pygame-rs-dev`                     → REPL（交互式；quit()/exit() 退）
//! - `pygame-rs-dev -c "<代码>"`         → 单行直跑（不建窗；纯脚本）
//! - `pygame-rs-dev <脚本.py> [参数...]` → 脚本直跑（sys.argv 透传；定义
//!   `async def main():` 协程则自动驱动帧循环——入口自动探测；
//!   无 main / 同步 main = 顶层执行完即收，python 语义：定义 ≠ 调用）
//!
//! **启动序（性能定约）**：RP genesis、sys.argv、字体预载、cwd 切换
//! 全部在**建窗前**的控制台阶段完成 → 建窗（可见）→ 立即进入脚本执行
//! ——确定性开销不占窗口期，窗口期只剩 GPU 装配 + 首帧（百 ms 级）。
//! 各阶段耗时由打点行量化：`①解释器 ②字体 ③建窗 ④执行 → 首帧分段`。
//! 可见性不做刻意安排（HIDDEN+首帧后显示的机制 2026-10-26 退役：不
//! 跨平台，且根因修复后白窗已短暂）。
//!
//! **创建期旗标**：透明/后端声明等只在建窗时生效——pygame-rs-dev 以脚本文本
//! 预扫描（含 TRANSPARENT/DIRECTX/VULKAN/OPENGL(ES)/HIDDEN 字样即
//! 置位）替脚本先行声明；运行期旗标（RESIZABLE/FULLSCREEN/…）由脚本
//! set_mode 经 apply_window_request 照常生效。
//!
//! 执行统一走 testhost 面（`run_script_in`——帧桥注入/协程驱动/
//! traceback/退出码全在内）。

use pygame_rs::rpy;
use starfish_pygame::display;
use std::time::Instant;

fn main() {
    // 直接调 block_on（不用 app_entry! 宏——pygame-rs-dev 纯桌面工具，宏的
    // Android/Web 入口分支与尾部 exit(0) 是死代码/退出码干扰）
    starfish_std::app::block_on(async move {
        let t0 = Instant::now();
        let mut argv = std::env::args().skip(1);
        let first = match argv.next() {
            Some(a) => a,
            None => repl_mode(),
        };
        match first.as_str() {
            "-c" => {
                let code = match argv.next() {
                    Some(c) => c,
                    None => usage(),
                };
                let rest: Vec<String> = argv.collect();
                let interp = rpy::new_interpreter();
                interp.enter(|vm| set_argv(vm, "-c", &rest));
                if let Err(e) = rpy::testhost::run_script_in(interp, &code, "<pygame-rs-dev -c>").await {
                    eprintln!("[pygame-rs-dev] {e}");
                    std::process::exit(1);
                }
            }
            // 脚本直跑（python 标准：首个参数 = 脚本路径，余者透传 argv）
            script_path => run_file(&script_path, argv.collect(), t0).await,
        }
    });
}

/// 脚本直跑（`pygame-rs-dev <脚本.py> [参数...]`）
async fn run_file(path: &str, args: Vec<String>, t0: Instant) {
    let sp = std::path::PathBuf::from(path);
    if sp.is_dir() {
        eprintln!("[pygame-rs-dev] {path}: 是目录——请传脚本文件（目录工程传其入口，如 main.py）");
        std::process::exit(2);
    }
    let script = match std::fs::read_to_string(&sp) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[pygame-rs-dev] 无法读取 {path}: {e}");
            std::process::exit(2);
        }
    };
    // cwd 切脚本目录（资源相对路径与脚本预期一致；游戏资产就近解析）
    let sp_abs = sp.canonicalize().unwrap_or(sp);
    if let Some(parent) = sp_abs.parent() {
        let _ = std::env::set_current_dir(parent);
    }
    // ── 控制台阶段：建窗前完成全部确定性启动开销（各阶段打点——
    // 顺序与耗时可审计，"白窗等待"归因用）──
    let t_rp = Instant::now();
    let interp = rpy::new_interpreter();
    println!("[pygame-rs-dev] ①解释器就绪 {}ms", t_rp.elapsed().as_millis());
    interp.enter(|vm| set_argv(vm, path, &args)); // python 标准 sys.argv
    // 游戏脚本模块加载（python 行为：解析基准 = main 所在目录——本壳
    // 已切 cwd，故首根 "." 即脚本目录（sys.path[0] 同语义），约定目录
    // modules/ 次之；两根皆缺失则零注入、纯脚本零开销）
    interp.enter(|vm| match rpy::install_module_finder(vm, &[".", "modules"]) {
        Ok(0) => {}
        Ok(n) => println!("[pygame-rs-dev] 模块目录装载 {n} 个（脚本目录 + modules/）"),
        Err(e) => eprintln!("[pygame-rs-dev] 模块装载失败: {e}"),
    });
    let t_font = Instant::now();
    rpy::testhost::preload_cjk("pygame-rs-dev").await;
    println!("[pygame-rs-dev] ②字体预载完 {}ms", t_font.elapsed().as_millis());
    // ── 创建期旗标预扫描（dev 解释器启发式）：TRANSPARENT 等创建期
    // 旗标只在建窗时生效（apply_window_request 的运行时面不覆盖；
    // 后端声明位同样"首次 set_mode 定终身"）——宿主壳（pygame-rs-dev 本体）
    // 预建窗须替脚本带上。脚本文本含词即置位：注释提及也会命中
    //（保守误报可接受）；运行期旗标（RESIZABLE 等）不在此列，脚本
    // set_mode 经 apply_window_request 照常生效 ──
    let mut host_flags = 0u32;
    for (token, bit) in [
        ("TRANSPARENT", display::TRANSPARENT),
        ("DIRECTX", display::DIRECTX),
        ("VULKAN", display::VULKAN),
        ("OPENGLES", display::OPENGLES),
        ("OPENGL", display::OPENGL),
        ("HIDDEN", display::HIDDEN),
    ] {
        if script.contains(token) {
            host_flags |= bit;
        }
    }
    // ── 建窗（可见；GPU/交换链装配承载于窗口期——百 ms 级）──
    let t_gpu = Instant::now();
    if let Err(e) = display::set_mode_ex((800, 600), host_flags).await {
        eprintln!("[pygame-rs-dev] set_mode 失败: {e}");
        std::process::exit(1);
    }
    println!(
        "[pygame-rs-dev] ③建窗+GPU装配完 {}ms（创建期旗标 {host_flags:#b}）",
        t_gpu.elapsed().as_millis()
    );
    println!("[pygame-rs-dev] ④执行 {path}（累计 {}ms）", t0.elapsed().as_millis());
    // 装配 + 执行统一走 testhost 面（帧桥注入/协程驱动/traceback 全在内）
    if let Err(e) = rpy::testhost::run_script_in(interp, &script, path).await {
        eprintln!("[pygame-rs-dev] {e}");
        std::process::exit(1);
    }
}

/// python 标准 sys.argv 接线（argv[0] = "-c" / 脚本路径，余者透传）
fn set_argv(vm: &rustpython_vm::VirtualMachine, argv0: &str, rest: &[String]) {
    if let Ok(sys) = vm.import("sys", 0) {
        let mut items: Vec<rustpython_vm::PyObjectRef> = vec![vm.ctx.new_str(argv0.to_string()).into()];
        items.extend(rest.iter().map(|a| vm.ctx.new_str(a.clone()).into()));
        let _ = sys.set_attr("argv", vm.ctx.new_list(items), vm);
    }
}

/// REPL（逐行；表达式值回显）
fn repl_mode() -> ! {
    println!("pygame-rs-dev REPL（pygame 全内置；quit() 退出）");
    let interp = rpy::new_interpreter();
    let scope = enter_setup(&interp);
    let stdin = std::io::stdin();
    loop {
        print!(">>> ");
        use std::io::Write;
        let _ = std::io::stdout().flush();
        let mut line = String::new();
        if stdin.read_line(&mut line).unwrap_or(0) == 0 {
            break; // EOF
        }
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }
        if line == "quit()" || line == "exit()" {
            break;
        }
        // 表达式优先（Single 语义：值回显）；非表达式降级 Exec
        match vm_compile_eval(&interp, line) {
            Some(c) => {
                interp.enter(|vm| {
                    match vm.run_code_obj(c, scope.clone()) {
                        Ok(v) => {
                            if !vm.is_none(&v) {
                                if let Ok(r) = v.repr(vm) {
                                    println!("{}", r);
                                }
                            }
                        }
                        Err(e) => eprintln!("{}", format_tb(vm, &e)),
                    }
                });
            }
            None => match vm_compile_exec(&interp, line) {
                Some(c) => {
                    interp.enter(|vm| {
                        if let Err(e) = vm.run_code_obj(c, scope.clone()) {
                            eprintln!("{}", format_tb(vm, &e));
                        }
                    });
                }
                None => eprintln!("SyntaxError: 语句编译失败"),
            },
        }
    }
    std::process::exit(0);
}

fn format_tb(
    vm: &rustpython_vm::VirtualMachine,
    exc: &rustpython_vm::builtins::PyBaseExceptionRef,
) -> String {
    let mut buf = std::io::Cursor::new(Vec::<u8>::new());
    let _ = vm.write_exception(&mut rustpython_vm::py_io::IoWriter(&mut buf), exc);
    String::from_utf8_lossy(buf.get_ref()).trim_end().to_string()
}

/// 装配（解释器 enter 内模块注入）——返回 scope 供 REPL 用。
/// （stdio 接线退役：RP "stdio" feature 桌面 = std::io 直通，print
/// 天然行缓冲出控制台；曾试 TextIOWrapper(io.FileIO) 接线，frozen
/// `_io` 无 FileIO——多此一举且必败，2026-10-26 实测定约。）
fn enter_setup(interp: &rustpython_vm::Interpreter) -> rustpython_vm::scope::Scope {
    let scope = std::cell::RefCell::new(None);
    interp.enter(|vm| {
        let sc = vm.new_scope_with_builtins();
        rpy::install_modules(vm);
        *scope.borrow_mut() = Some(sc);
    });
    scope.into_inner().expect("enter 必然装配 scope")
}

/// Single 模式编译（REPL 值回显用）
fn vm_compile_eval(
    interp: &rustpython_vm::Interpreter,
    source: &str,
) -> Option<rustpython_vm::PyRef<rustpython_vm::builtins::PyCode>> {
    interp.enter(|vm| {
        vm.compile(
            source,
            rustpython_vm::compiler::Mode::Eval,
            "<pygame-rs-dev repl>".to_string(),
        )
        .ok()
    })
}

/// Exec 模式编译（REPL 语句降级用）
fn vm_compile_exec(
    interp: &rustpython_vm::Interpreter,
    source: &str,
) -> Option<rustpython_vm::PyRef<rustpython_vm::builtins::PyCode>> {
    interp.enter(|vm| {
        vm.compile(
            source,
            rustpython_vm::compiler::Mode::Exec,
            "<pygame-rs-dev repl>".to_string(),
        )
        .ok()
    })
}

fn usage() -> ! {
    eprintln!(
        "pygame-rs-dev —— pygame 的 python（开发解释器）\n\
\n\
用法（python 标准命令行）：\n\
  pygame-rs-dev                            交互式 REPL\n\
  pygame-rs-dev -c \"<代码>\"                单行直跑\n\
  pygame-rs-dev <脚本.py> [参数...]         脚本直跑（async def main 协程自动驱动）\n\
\n\
pygame 全内置（import pygame 即用）；-c 不建窗，脚本形态预建渲染窗。"
    );
    std::process::exit(2);
}
