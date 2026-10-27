import pygame
import pygame.event

# 呈现映射自证探针：全屏拉伸 / 非全屏居中 4 秒轮换——深蓝底标出画布
# 可视边界，黄描边+四角白点 = 逻辑矩形四角，红圈 = 触摸映射落点
#（e.x/e.y）。一张截图同时诊断：锚点、拉伸、触摸映射三件事。

FULLSCREEN = 2  # 1 << 1（引擎 display 位定义）

async def main():
    screen = pygame.display.set_mode((1024, 768))
    fs = False
    frame = 0
    pos = None
    while True:
        for e in pygame.event.get():
            if e.type == pygame.event.QUIT:
                return
            if e.type == pygame.event.MOUSEMOTION:
                pos = (e.x, e.y)
        if frame > 0 and frame % 240 == 0:  # 4 秒一切（60fps 假定）
            fs = not fs
            screen = pygame.display.set_mode((1024, 768), FULLSCREEN if fs else 0)
            print(f"[probe] 切换 → 全屏拉伸={fs} get_size={screen.get_size()}")
        with screen.render() as r:
            r.fill((0, 0, 60))  # 深蓝底 = 画布可视边界（黑屏背景对照）
            r.rect((255, 255, 0), (0, 0, 1024, 768), 6)
            for cx, cy in ((20, 20), (1004, 20), (20, 748), (1004, 748)):
                r.circle((255, 255, 255), (cx, cy), 12)
            if pos:
                r.circle((255, 0, 0), pos, 20)
        pygame.display.flip()
        frame += 1
        if frame == 1:
            print(f"[probe] 起始 get_size={screen.get_size()}（非全屏段）")
        await pygame.next_frame()
