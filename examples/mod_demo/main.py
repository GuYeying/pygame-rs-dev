# 多文件游戏示例（模块加载验证）：main.py + modules/ 自定义脚本包
# 运行：python.exe examples/mod_demo/main.py
import pygame

from modules.lib import a     # modules 是打包便利文件夹——按自然路径 import
import modules.utils          # 同上：import modules.utils（非裸 utils）

print("[mod-demo] PASS import lib.a =", a.HELLO)
print("[mod-demo] PASS import modules.utils =", modules.utils.TITLE)
print("[mod-demo] PASS 相对导入 =", a.BASE)


async def main():
    screen = pygame.display.set_mode((400, 300))
    obj = a.A("pygame")
    print("[mod-demo] PASS 实例化 =", obj.greet())
    font = pygame.font.Font(None, 26)
    frames = 0
    while frames < 90:
        with screen.render() as r:
            r.fill((24, 26, 38))
            r.blit(font.render(modules.utils.TITLE, True, (235, 235, 240)), (20, 20))
            r.blit(font.render(obj.greet(), True, (255, 210, 90)), (20, 56))
        pygame.display.flip()
        frames += 1
        await pygame.next_frame()
    print("[mod-demo] DONE")
