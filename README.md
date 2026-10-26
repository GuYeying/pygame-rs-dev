# pygame-rs-dev

**pygame 的 python** —— pygame 全内置的开发解释器（`python.exe`）。

零 Rust 工具链：像用 python 一样写 pygame，拿到 `python.exe` 配任意
编辑器就能开发 pygame 游戏——`import pygame` 即用，所有子模块原生
内置（非 ctypes shim）。

## 特性

- **python 标准命令行**：REPL / `-c` / 脚本直跑（`sys.argv` 透传、
  退出码对齐、目录实参报错）
- **协程游戏门**：脚本定义 `async def main():` 即自动驱动帧循环，
  帧体 `await pygame.next_frame()`（无 main = 纯脚本顶层执行完即收，
  python 语义：定义 ≠ 调用）
- **pygame 全内置**：draw/font/image/mixer/mask/transform/… 22 子模块，
  外加 `pygame.egui`（egui 界面）与 `pygame_physics`（rapier 2D/3D）
- **透明窗口**：`set_mode(..., TRANSPARENT | DIRECTX)` 逐像素透明
  （DirectComposition + 预乘 alpha），桌宠/悬浮件形态
- **经典 traceback**：语法错/运行期异常输出与 CPython 一致
- **启动仪表**：阶段耗时打点（解释器/字体/建窗/首帧）+ 慢调用哨兵

## 用法

```
python                            交互式 REPL（quit()/exit() 退）
python -c "<代码>"                单行直跑
python <脚本.py> [参数...]         脚本直跑（任意脚本；cwd 自动切脚本目录）
```

最小游戏（协程门）：

```python
import pygame

async def main():
    screen = pygame.display.set_mode((400, 300))
    while True:
        screen.fill((30, 30, 46))
        pygame.display.flip()
        await pygame.next_frame()
```

## 构建

```
cargo build --release        # 产物 target/release/python.exe
```

依赖：`../pygame-rs` 与 `../starfish`（path 依赖，同机同源）。
Windows MSVC 构建自动抬主线程栈（build.rs，RP 引导需要）。

## 架构（一句话）

[RustPython](https://github.com/RustPython/RustPython)（vendored，
零改动）+ [pygame-rs](https://github.com/GuYeying/pygame-rs) 原生绑定面
+ [starfish](https://github.com/GuYeying/starfish) 引擎（wgpu）。

## 定约与分工

- 入口严格 `async def main()`（跨平台统一——Web rAF 模型的硬约束）；
- 帧桥 `pygame.next_frame()`（待网络 awaitable 同机制扩展）；
- 与 pygame-runner（分发载体，玩家单元）分工：本仓 = 开发解释器
  （python.exe 形态），那仓 = 游戏门分发。

## 协议

MIT OR Apache-2.0 双协议（见 LICENSE-MIT / LICENSE-APACHE）。
