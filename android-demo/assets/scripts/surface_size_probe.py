import pygame
import pygame.event
import sys

# Surface 真实处理尺寸探针（自适应满屏）：Android 壳把实测屏幕尺寸
# 附在 sys.argv 末尾（argv[-2:]），脚本据此 set_mode——画布 = 屏幕，
# 1:1 满屏零变形（"std 屏幕尺寸接口"的壳层先行形态）。桌面/web 无
# 附加参数 = 回退 1024×768。蓝底标画布边界，黄描边 + 四角白点验证
# 锚点，红圈 = 触摸映射落点。源码含 OPENGLES 词 → 创建期预扫描置位。

FULLSCREEN = 2  # 1 << 1（引擎 display 位定义；Python 常量面待开放）

async def main():
    req_w, req_h = 1024, 768
    if len(sys.argv) >= 3:
        req_w, req_h = int(sys.argv[-2]), int(sys.argv[-1])
    screen = pygame.display.set_mode((req_w, req_h), FULLSCREEN)
    sw, sh = screen.get_size()  # FULLSCREEN 下引擎以原生窗口实测尺寸为准
    print(f"[probe] 请求 = ({req_w}, {req_h})；FULLSCREEN 生效 = ({sw}, {sh})")
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
            r.rect((255, 255, 0), (0, 0, sw, sh), 6)
            for cx, cy in ((8, 8), (sw - 8, 8), (8, sh - 8), (sw - 8, sh - 8)):
                r.circle((255, 255, 255), (cx, cy), 8)
            if pos:
                r.circle((255, 0, 0), pos, 20)
        pygame.display.flip()
        frame += 1
        if frame == 1 or frame % 120 == 0:
            print(f"[probe] frame {frame}: surface = {screen.get_size()}")
        await pygame.next_frame()
