//! 桌面宿主实现（首个 [`Host`](crate::host::Host) 实现者）：CLI 解析、
//! 载荷读盘、REPL 交互形态。行为与 trait 化前逐字节一致。
//!
//! 载荷获取 = std::fs 同步读；模块根 = 脚本父目录（运行时切 cwd——
//! python sys.path[0] 同语义）。

use crate::host::RunRequest;
use pygame_rs::rpy;
use rustpython_vm::VirtualMachine;

/// CLI 解析结果（python 标准命令行三形态 + serve 子命令）
enum Mode {
    /// 无参 = 交互式 REPL（桌面专属交互，main 直入、不经 acquire）
    Repl,
    /// `-c "<代码>"`（不建窗；纯脚本）
    Eval(String),
    /// `<脚本.py>`（余参透传 sys.argv）
    File(String),
    /// `serve [目录] [端口]`：内置静态服务器（web 宿主页面的零依赖
    /// 打开方式——file:// 下 ES module 被 CORS 拦截）
    Serve { dir: String, port: u16 },
}

pub struct DesktopHost {
    mode: Mode,
    rest: Vec<String>,
}

impl DesktopHost {
    /// 从进程参数装配（python 标准：首个参数 = -c 或脚本路径，余者透传）
    pub fn from_env() -> Self {
        let mut argv = std::env::args().skip(1);
        match argv.next() {
            None => Self { mode: Mode::Repl, rest: Vec::new() },
            Some(first) => match first.as_str() {
                "-c" => {
                    let code = match argv.next() {
                        Some(c) => c,
                        None => usage(),
                    };
                    Self { mode: Mode::Eval(code), rest: argv.collect() }
                }
                "serve" => {
                    let dir = argv.next().unwrap_or_else(|| "web".to_string());
                    let port = argv
                        .next()
                        .and_then(|p| p.parse::<u16>().ok())
                        .unwrap_or(8123);
                    Self { mode: Mode::Serve { dir, port }, rest: Vec::new() }
                }
                path => Self { mode: Mode::File(path.to_string()), rest: argv.collect() },
            },
        }
    }

    /// REPL 专属：无参调用 = 交互模式（main 据此直入 repl_mode）
    pub fn wants_repl(&self) -> bool {
        matches!(self.mode, Mode::Repl)
    }

    /// serve 专属：静态服务请求（main 据此进入内置服务器，不返回）
    pub fn serve_request(&self) -> Option<(String, u16)> {
        match &self.mode {
            Mode::Serve { dir, port } => Some((dir.clone(), *port)),
            _ => None,
        }
    }
}

impl crate::host::Host for DesktopHost {
    fn name(&self) -> &'static str {
        "pygame-rs-dev"
    }

    fn args(&self) -> Vec<String> {
        self.rest.clone()
    }

    async fn acquire(&self) -> Result<RunRequest, String> {
        match &self.mode {
            Mode::Repl => unreachable!("REPL 由 main 直入 repl_mode，不经 acquire"),
            Mode::Serve { .. } => unreachable!("serve 由 main 直入 serve::serve，不经 acquire"),
            Mode::Eval(code) => Ok(RunRequest {
                name: "<pygame-rs-dev -c>".to_string(),
                code: code.clone(),
                module_root: None, // -c 无基准目录、不注入模块表
            }),
            Mode::File(path) => {
                let sp = std::path::PathBuf::from(path);
                if sp.is_dir() {
                    return Err(format!(
                        "{path}: 是目录——请传脚本文件（目录工程传其入口，如 main.py）"
                    ));
                }
                let script = std::fs::read_to_string(&sp)
                    .map_err(|e| format!("无法读取 {path}: {e}"))?;
                let sp_abs = sp.canonicalize().unwrap_or(sp);
                let module_root = Some(
                    sp_abs
                        .parent()
                        .map(|p| p.to_string_lossy().to_string())
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| ".".to_string()),
                );
                Ok(RunRequest { name: path.clone(), code: script, module_root })
            }
        }
    }
}

/// REPL（逐行；表达式值回显）——桌面专属交互形态
pub fn repl_mode() -> ! {
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
                interp.enter(|vm| match vm.run_code_obj(c, scope.clone()) {
                    Ok(v) => {
                        if !vm.is_none(&v) {
                            if let Ok(r) = v.repr(vm) {
                                println!("{}", r);
                            }
                        }
                    }
                    Err(e) => eprintln!("{}", format_tb(vm, &e)),
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
    vm: &VirtualMachine,
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
  pygame-rs-dev <脚本.py> [参数...]         脚本直跑（async def main 协程自动驱动）
  pygame-rs-dev serve [目录] [端口]          内置静态服务器（web 宿主页面，缺省 web/:8123）\n\
\n\
pygame 全内置（import pygame 即用）；-c 不建窗，脚本形态预建渲染窗。"
    );
    std::process::exit(2);
}
