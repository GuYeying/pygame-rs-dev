//! 平台宿主统一抽象（**trait 接口文件**，2026-10-26 用户定约：先把所有
//! 平台的共同接口 trait 化，各平台后续各自实现——桌面 CLI 即首个实现者）。
//!
//! 分工定约：
//! - [`Host`]（本文件）：只抽象"**拿什么跑 + 余参 + 平台名**"三件事；
//! - [`crate::runtime`]：通用驱动（genesis/argv/模块表/旗标预扫描/建窗/
//!   run_script_in/退出码），平台零感知；
//! - `crate::platform::*`：各平台 Host 实现（desktop = CLI/REPL；
//!   web = 查询参数 + fetch；android = dialog 选文件——后续批次）。
//!
//! 平台差异全在 `acquire`（如何拿到源码）与 `args`（余参语义）里；
//! 载荷统一为 [`RunRequest`]，运行语义全平台一致（基准 = main 所在目录）。

/// 一次运行的载荷
pub struct RunRequest {
    /// 虚拟名：traceback/argv[0]/日志定位用（桌面 = 命令行路径；
    /// web = URL；android = 选取的文件名）
    pub name: String,
    /// 源码全文（普通脚本或 bundle——bundle 自带注册 + 运行）
    pub code: String,
    /// 模块扫描根（main 所在目录，python sys.path[0] 同语义）。
    /// 桌面 = 脚本父目录；web = None（bundle 自含模块）；android =
    /// 选取文件的父目录。None = 不注入 finder。
    pub module_root: Option<String>,
}

/// 平台宿主抽象（每平台二进制恰有一个实现——静态分发即可）
pub trait Host {
    /// 平台名（日志前缀，如 "pygame-rs-dev"）
    fn name(&self) -> &'static str;

    /// 余参（python 语义 sys.argv[1..]：脚本/-c 之后的参数透传）
    fn args(&self) -> Vec<String>;

    /// 获取运行载荷（**唯一平台差异点**）：
    /// - desktop：CLI 解析 + 读盘（同步 fs）
    /// - web：查询参数 → fetch（异步网络，启动期一次）
    /// - android：dialog 选文件 → 读（app 私有/共享目录）
    ///
    /// Err = 载荷不可得（文件缺失/是目录/网络失败）→ 宿主以退出码 2 收场
    ///（python 同语义：无法打开脚本）。
    async fn acquire(&self) -> Result<RunRequest, String>;
}
