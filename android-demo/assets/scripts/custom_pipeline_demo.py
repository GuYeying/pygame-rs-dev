# custom_pipeline_demo —— 最小诊断版：纯绿满屏判定 letterbox 是否生效。
# 绿色铺满整屏 = 引擎 letterbox 未生效（batch 未被组合收进画布）；
# 绿色只占中部画布（左右留黑边）= letterbox 正常，问题在 raw/compute 附加层。
import pygame


async def main():
    screen = pygame.display.set_mode((800, 600))
    frame = 0
    while frame < 1200:
        for e in pygame.event.get():
            if e.type == pygame.event.QUIT:
                return
        with screen.render() as t:
            t.fill((0, 255, 0))
        pygame.display.flip()
        # flip 后回读已呈现帧（post-resolve = 屏幕真实内容）
        if frame in (10, 60):
            print('[state]', pygame.render.screen_frame_state())
            samples = pygame.render.screen_probe()
            print('[screen-probe] 样本数', len(samples))
        frame += 1
        await pygame.next_frame()
