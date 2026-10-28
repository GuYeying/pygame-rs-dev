# rsdev_log —— 本地日志模块（开发者模板随包）。
#
# 为什么需要它：Android 嵌入形态下脚本 print() 不可见（stdout 无
# logcat 重定向），且嵌入解释器未编入 FileIO（内建 open() 不可用）——
# 脚本侧日志唯一通道 = pygame.io（引擎 std::fs 直写）。
#
# 落点探测（首可写者胜，进程内固定）：
#   1. /sdcard/Download/pygame_rsdev_script_log.txt
#      （与宿主 dev_log 同目录——USB 传输免 root 直拉）；
#   2. rsdev_script_log.txt（io 基准目录相对名——app 私有 files/，
#      adb run-as 可取；桌面 = 主脚本所在目录）。
#
# 用法（三行接入）：
#   import modules.rsdev_log as rlog
#   rlog.clear()                  # main() 开头调一次——一轮一个文件
#   rlog.log("set_mode", size)    # 任意位置追加（[ms] 引擎时钟前缀）
import pygame

_PATHS = ["/sdcard/Download/pygame_rsdev_script_log.txt", "rsdev_script_log.txt"]

_path = None  # 命中的落点（None = 尚未探测/全部不可写）


def _probe():
    """惰性探测首个可写目标（写回原内容 = 无损试写）；失败静默"""
    global _path
    if _path is None:
        for p in _PATHS:
            try:
                old = pygame.io.read_text(p) if pygame.io.exists(p) else ""
                pygame.io.write_text(p, old)
                _path = p
                break
            except Exception:
                continue
    return _path


def clear():
    """清空日志文件——main() 开头调一次（一轮运行一个文件）"""
    p = _probe()
    if p is not None:
        try:
            pygame.io.write_text(p, "")
        except Exception:
            pass


def log(*parts):
    """追加一行：[引擎毫秒] part1 part2 ...（读旧+拼接+写回；失败静默
    ——日志永不干扰游戏）"""
    p = _probe()
    if p is not None:
        try:
            old = pygame.io.read_text(p) if pygame.io.exists(p) else ""
            old += "[%d] %s\n" % (
                pygame.time.get_ticks(), " ".join(str(x) for x in parts))
            pygame.io.write_text(p, old)
        except Exception:
            pass


def path():
    """当前日志落点（None = 尚未探测成功/全部不可写）"""
    return _probe()
