# rsdev_kit —— 相机/矩阵封装（开发者模板随包）。
#
# 收拢引擎高级渲染接口（set_default_camera / camera_perspective /
# blit_transform）的必备调用，收敛成**画布域友好形态**。
#
# 核心定约（引擎已保证，开发者零心智负担）：相机一律按 set_mode 的
# 逻辑画布尺寸构造——letterbox 呈现由引擎统一施加（Auto/Manual 同一
# 映射）。窗口化 / 全屏 / 桌面 / web 同一份代码，无需感知物理屏幕。
import pygame


class Camera2D:
    """2D 正交相机（画布像素域：(0,0) 左上 → (w,h) 右下，y 向下）"""

    def __init__(self, w, h):
        self.w, self.h = w, h
        self.cam = pygame.render.camera_ortho(float(w), float(h))

    def use(self):
        """接管全局渲染（后续所有绘制经本相机，直到换下一个）"""
        pygame.render.set_default_camera(self.cam)


class Camera3D:
    """3D 透视相机（y 向上右手系，看向 -Z；纵横比恒取画布——三端同源）"""

    def __init__(self, w, h, fov=45.0, near=0.1, far=100.0):
        self.cam = pygame.render.camera_perspective(
            fov, float(w) / float(h), near, far)

    def use(self):
        pygame.render.set_default_camera(self.cam)


def use_2d(w, h):
    """回 2D 正交（3D 会话后的标准回切——等价桌面默认相机）"""
    Camera2D(w, h).use()


def state():
    """当前全局相机状态（'auto2d' / 'auto3d' / 'manual'）"""
    return pygame.render.camera_state()


def fit():
    """呈现拟合矩阵（NDC 域）——**自管相机（write_buffer 直写矩阵的
    自绘管线）专用**：引擎无法拦截组合 letterbox，自绘内容左乘本矩阵
    即与主绘制同一映射。桌面/全屏恒等，Android 窗口化收进居中画布。
    帧内快照语义（每帧重取）"""
    return pygame.render.present_fit()


# ── 矩阵链（blit_transform 的变换参数；可读名替代手写 .mul_mat4 链）──

def pos(x, y, z=0.0):
    """平移"""
    return pygame.math.mat4_translation(x, y, z)


def rot_z(deg):
    """绕 Z 轴旋转（度数制；2D 卡片旋转用）"""
    return pygame.math.mat4_from_quat(
        pygame.math.quat_from_axis_angle(0, 0, 1, deg))


def scale(s, sy=None, sz=1.0):
    """缩放（单参 = 等比；sy=None 时取 s）"""
    if sy is None:
        sy = s
    return pygame.math.mat4_scale(s, sy, sz)


def chain(*mats):
    """依序复合（与 .mul_mat4 链同序：chain(T, R) = demo 的 T.mul_mat4(R)
    ——先传者在外层，顶点先经后传者变换）"""
    m = mats[0]
    for n in mats[1:]:
        m = m.mul_mat4(n)
    return m
