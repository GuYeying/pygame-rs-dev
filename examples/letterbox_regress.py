import pygame
import pygame.event

# letterbox 桌面回归：逻辑==物理恒等映射锚点——get_size 回 set_mode 值，
# 30 帧后自退（窗口一闪即退，CI 可跑）
async def main():
    screen = pygame.display.set_mode((800, 600))
    assert screen.get_size() == (800, 600), f"get_size={screen.get_size()}"
    print("[regress] get_size 恒等 ✓", screen.get_size())
    for frame in range(30):
        for e in pygame.event.get():
            if e.type == pygame.event.QUIT:
                return
        with screen.render() as r:
            r.fill((20, 120, 20))
            r.rect((255, 255, 0), (100, 80, 200, 150), 3)
        pygame.display.flip()
        await pygame.next_frame()
    assert screen.get_size() == (800, 600)
    print("[regress] 30 帧渲染 + 尺寸稳定 ✓")
