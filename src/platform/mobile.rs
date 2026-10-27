//! 移动宿主（**Android / iOS 共用**——dialog 载荷契约，2026-10-26 定约）。
//!
//! 三平台一个交互设计：与 web 宿主映射同构——
//! - 调试形态：系统文件选择器（starfish-dialog，robius 实现）选 .py →
//!   读取 → `-c` 形态运行（与 web `load_code` 同构）；
//! - 出货形态：模板工程嵌 bundle 启动即跑（随 pack 仓）。
//!
//! 平台差异全部由家族设施承载：窗口/事件 = starfish-we android/ios
//! 后端；文件选择 = starfish-dialog（robius-file-picker）；io 基准 =
//! starfish 自动注入 app 私有目录（资源就近解析）。宿主层零平台分支。
//!
//! 参数：移动端无命令行 → `sys.argv = [选取文件名]`（-c 语义同型）。
//! 鸿蒙：前提 = starfish-we 新增 harmony 窗口/事件后端（基础设施工程）；
//! 本模块的载荷契约对其零障碍（未来 cfg 门直接扩）。

use crate::host::{Host, RunRequest};
use starfish_std::debug::console_log;

/// dialog 选定后的载荷宿主（-c 形态）
pub struct MobileHost {
    /// 选取文件名（sys.argv[0] / 日志定位）
    pub name: String,
    /// 源码全文
    pub code: String,
}

impl Host for MobileHost {
    fn name(&self) -> &'static str {
        "pygame-rs-dev"
    }

    /// 移动端无命令行 → sys.argv = [选取文件名]
    fn args(&self) -> Vec<String> {
        vec![self.name.clone()]
    }

    async fn acquire(&self) -> Result<RunRequest, String> {
        Ok(RunRequest {
            name: self.name.clone(),
            code: self.code.clone(),
            module_root: None, // 模块表：移动 v1 = 单文件/bundle；目录形态随 pack 仓
        })
    }
}

/// dialog 选文件 → 读取 → 运行（`python <文件>` 的移动映射）。
///
/// 返回进程退出码：0 = 正常结束（含用户取消）；2 = dialog/读取失败；
/// 1 = 脚本运行错误（traceback 已由 runtime 打印）。
pub async fn pick_and_run() -> i32 {
    let picked = match starfish_dialog::pick_file(
        Some("选择 pygame 脚本"),
        &[("Python 脚本", &["py"])],
    )
    .await
    {
        Ok(p) => p,
        Err(e) => {
            console_log(&format!("[pygame-rs-dev] dialog 失败: {e}"));
            return 2;
        }
    };
    let Some(pf) = picked else {
        console_log("[pygame-rs-dev] 用户取消选择");
        return 0;
    };
    let name = pf.name().to_string();
    let bytes = match pf.read() {
        Ok(b) => b,
        Err(e) => {
            console_log(&format!("[pygame-rs-dev] 读取失败 {name}: {e}"));
            return 2;
        }
    };
    if std::str::from_utf8(&bytes).is_err() {
        console_log(&format!("[pygame-rs-dev] {name} 非 UTF-8"));
        return 2;
    }
    let code = String::from_utf8_lossy(&bytes).into_owned();
    console_log(&format!("[pygame-rs-dev] 载荷 = {name}（dialog，{} B）", bytes.len()));
    let host = MobileHost { name, code };
    crate::runtime::run(&host).await
}
