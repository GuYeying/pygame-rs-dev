# camera_kit_demo —— 多文件工程最小模板（开发者对照样例）。
#
# 布局：本文件（主脚本）+ modules/rsdev_kit.py（相机封装）
#       + modules/rsdev_log.py（本地日志）——`import modules.xxx` 即达
#       （模块根 = 主脚本所在目录，引擎自动装载模块表）。
# 演示：自定义相机三件套——上半 2D 正交（blit_transform 旋转卡片）、
#       下半 3D 透视 + 深度会话（近大远小）、帧内回切 2D 画文字；
#       Android 窗口化 letterbox 与桌面同一份代码（引擎统一呈现映射）。
# 退出：ESC / 返回键 / 600 帧自动（≈10s，日志写 DONE 便于取证）。
import pygame

import modules.rsdev_kit as kit
import modules.rsdev_log as rlog


async def main():
    rlog.clear()                                    # 一轮运行一个日志文件
    screen = pygame.display.set_mode((800, 600))
    w, h = screen.get_size()
    rlog.log("set_mode =", (w, h), "| get_size 一致:", (w, h) == (800, 600))
    rlog.log("日志落点 =", rlog.path())

    font = pygame.font.Font(None, 26)               # 内嵌默认字体（拉丁）
    try:
        cjk = pygame.font.Font("cjk", 20)           # 宿主预载 CJK（缺则回退）
    except (ValueError, TypeError):
        cjk = font

    # 静态面创建一次、循环内只 blit（性能定约——勿每帧新建 GPU 表面）
    title = cjk.render(
        "camera_kit_demo：上半 2D 正交 / 下半 3D 透视", True, (220, 220, 230))
    card = pygame.Surface((120, 90))
    with card.render() as t:
        t.fill((30, 34, 48))
        t.rect((80, 200, 90), (6, 6, 108, 78), 2)
        t.circle((230, 70, 70), (34, 45), 16)

    cam2d = kit.Camera2D(w, h)                      # 相机只建一次
    cam3d = kit.Camera3D(w, h, fov=45.0)
    k = 0.5 / 120.0                                 # 卡片像素 → 3D 世界单位（宽≈0.5）

    frame = 0
    while frame < 600:
        for e in pygame.event.get():
            if e.type == pygame.event.QUIT:
                return
            if e.type == pygame.event.KEYDOWN and e.key == pygame.key.K_ESCAPE:
                return

        x = 100 + (frame * 3) % (w - 220)           # 匀速巡游（帧推进自证）

        # ── 上半：Manual 2D 正交 + blit_transform（平移×旋转矩阵链）──
        cam2d.use()
        with screen.render() as t:
            t.fill((16, 17, 24))
            t.blit(title, (14, 10))
            t.blit_transform(
                card, kit.chain(kit.pos(x, 120), kit.rot_z(frame % 360)),
                (255, 255, 255))
            t.rect(color=(70, 74, 92), rect=(0, h // 2, w, 3))

        # ── 下半：Manual 3D 透视 + 深度会话（y 向上右手系，看向 -Z）──
        cam3d.use()
        with screen.render_depth() as t:
            t.blit_transform(
                card, kit.chain(kit.pos(-0.5, -0.4, -2.2), kit.scale(k)),
                (255, 140, 140))
            t.blit_transform(
                card, kit.chain(kit.pos(0.5, -0.6, -3.4), kit.scale(k)),
                (140, 200, 255))

        # 回 2D 画文字/描边（Manual 状态跨帧保持——帧空档仍是正交）
        cam2d.use()
        with screen.render() as t:
            t.blit(cjk.render("下半：render_depth + 透视（近大远小）",
                              True, (150, 160, 180)), (14, h // 2 + 8))
            t.rect(color=(120, 60, 60),
                   rect=(20, h // 2 + 40, w - 40, h // 2 - 60), width=2)

        pygame.display.flip()
        frame += 1
        if frame == 30:
            rlog.log("[selftest] 帧空档相机 = manual:", kit.state() == "manual")
        await pygame.next_frame()

    rlog.log("DONE 600 帧渲染收口")
