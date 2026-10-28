import pygame
import pygame.event

# 窗口化 letterbox 校准探针（无 FULLSCREEN 旗标）：画布 1024×768，
# 引擎等比缩放居中映射到原生窗口——蓝底 = 逻辑画布实际落位（黑边宽
# 度一目了然），黄描边 + 四角白点验证锚点，红圈 = 触摸映射落点。
# 若蓝区铺满无黑边 = 物理窗口恰好同比例；若对称黑边 = letterbox 正确；
# 若偏移/溢出 = 映射残差（对照 dev_log [present] 行定位）。

async def main():
    w, h = 1024, 768
    screen = pygame.display.set_mode((w, h))
    print(f"[probe] set_mode = ({w}, {h}) 窗口化；get_size = {screen.get_size()}")
    frame = 0
    pos = None
    while True:
        for e in pygame.event.get():
            if e.type == pygame.event.QUIT:
                return
            if e.type == pygame.event.MOUSEMOTION:
                pos = (e.x, e.y)
        with screen.render() as r:
            r.fill((0, 0, 255))
            r.rect((255, 255, 0), (0, 0, w, h), 6)
            for cx, cy in ((8, 8), (w - 8, 8), (8, h - 8), (w - 8, h - 8)):
                r.circle((255, 255, 255), (cx, cy), 8)
            if pos:
                r.circle((255, 0, 0), pos, 20)
        pygame.display.flip()
        frame += 1
        if frame == 1 or frame % 120 == 0:
            print(f"[probe] frame {frame}: surface = {screen.get_size()}")
        await pygame.next_frame()
