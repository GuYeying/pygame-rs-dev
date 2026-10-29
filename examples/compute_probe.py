# compute_probe —— GPU 粒子系统：compute 积分 2048 粒子（重力+反弹）
# → copy 到 uniform → 渲染端每粒子一个小三角形（顶点索引展开角点）。
# 全程 GPU 闭环：CPU 每帧仅提交，零回读。
# 120 帧自退；dev_log 含 [fb-dump] 帧缓冲像素取证。
import pygame

COMPUTE_WGSL = """
struct Particle { pos: vec4<f32>, vel: vec4<f32> };   // vec4 对齐：copy 平面连续
struct State { particles: array<Particle, 2048> };
@group(0) @binding(0) var<storage, read_write> state: State;

@compute @workgroup_size(64)
fn cs_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= 2048u) { return; }
    var p = state.particles[gid.x];
    p.vel = p.vel + vec4<f32>(0.0, 240.0, 0.0, 0.0) * 0.016;  // 重力
    p.pos = p.pos + p.vel * 0.016;                              // 积分
    if (p.pos.x < 10.0 || p.pos.x > 790.0) {                  // 反弹
        p.vel.x = -p.vel.x;
        p.pos.x = clamp(p.pos.x, 10.0, 790.0);
    }
    if (p.pos.y < 10.0 || p.pos.y > 590.0) {
        p.vel.y = -p.vel.y;
        p.pos.y = clamp(p.pos.y, 10.0, 590.0);
    }
    state.particles[gid.x] = p;
}
"""

RENDER_WGSL = """
struct Pos { data: array<vec4<f32>, 2048> };
struct Camera { mvp: mat4x4<f32> };
@group(0) @binding(0) var<uniform> pos: Pos;
@group(0) @binding(1) var<uniform> camera: Camera;

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec3<f32>,
};

const CORNERS = array<vec2<f32>, 3>(
    vec2<f32>(-3.0, -3.0), vec2<f32>(3.0, -3.0), vec2<f32>(0.0, 3.0));

@vertex
fn vs_main(@location(0) corner: vec2<f32>,
           @builtin(vertex_index) vi: u32) -> VOut {
    let p = pos.data[vi / 3u].xy + corner;
    let hue = f32(vi / 3u % 64u) / 64.0;
    var o: VOut;
    o.pos = camera.mvp * vec4<f32>(p, 0.0, 1.0);
    o.color = vec3<f32>(hue, 0.9 - hue * 0.6, 0.3 + hue * 0.4);
    return o;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
"""


async def main():
    screen = pygame.display.set_mode((800, 600))
    ac = pygame.render.access()
    N = 2048

    compute_shader = ac.shader(COMPUTE_WGSL, label="p_cs")
    render_shader = ac.shader(RENDER_WGSL, label="p_rs")

    # 角点模板顶点缓冲：每粒子 3 顶点 × corner vec2（顶点着色器展开）
    corners = [-2.0, -2.0, 2.0, -2.0, 0.0, 2.0] * N
    mesh = ac.mesh("corner2", corners, label="p_mesh")

    state = ac.buffer(N * 32, usage="storage", label="p_state")   # pos+vel
    pos_buf = ac.buffer(N * 16, label="p_pos")                     # 渲染读
    cam_buf = ac.buffer(64, label="p_cam")

    state_bind = ac.bind_group([(0, "storage_rw", state)], label="p_state_bind")
    # 渲染 bind group：pos 数组 + 相机 MVP 同组（WGSL b0/b1 都在 group 0）
    rbind = ac.bind_group([(0, "uniform", pos_buf), (1, "uniform", cam_buf)],
                          label="p_pos_bind")
    cpipe = ac.compute_pipeline(compute_shader, [state_bind], label="p_cpipe")
    rpipe = ac.pipeline(render_shader, mesh, [rbind],
                        flavor="2d", label="p_rpipe")
    cam = pygame.render.camera_ortho(800.0, 600.0)

    pygame.render.set_fb_dump(True)  # 帧缓冲取证：30-35 帧 dump 落 dev_log

    # 零导入 LCG（嵌入构建无 random/math——纯算术伪随机）
    seed = 20261029
    init = []
    for i in range(N):
        seed = (seed * 1103515245 + 12345) % (1 << 31)
        x = 100.0 + (seed >> 8) % 600
        seed = (seed * 1103515245 + 12345) % (1 << 31)
        y = 80.0 + (seed >> 8) % 220
        seed = (seed * 1103515245 + 12345) % (1 << 31)
        vx = ((seed >> 7) % 120) - 60
        seed = (seed * 1103515245 + 12345) % (1 << 31)
        vy = ((seed >> 7) % 60) - 30
        # vec4 pos + vec4 vel（zw 占 0——storage 布局对齐）
        init += [x, y, 0.0, 0.0, float(vx), float(vy), 0.0, 0.0]
    ac.write_buffer(state, init)

    frame = 0
    while frame < 1200:
        for e in pygame.event.get():
            if e.type == pygame.event.QUIT:
                return
        pygame.render.write_camera_mvp(cam_buf, cam.mvp())
        with screen.render() as t:
            t.fill((6, 8, 14))
            t.rect((60, 70, 110), (1, 1, 798, 598), 2)
            with t.raw_compute() as p:
                p.set_pipeline(cpipe)
                p.set_bind_group(0, state_bind)
                p.dispatch(32, 1, 1)                      # 2048/64 工作组
                p.copy_buffer(state, pos_buf, N * 16)  # pos 平面（每粒子前 16B）
            with t.raw_render() as p:
                p.set_pipeline(rpipe)
                p.set_bind_group(0, rbind)
                p.draw_mesh(mesh)
        pygame.display.flip()
        # flip 后回读已呈现帧（post-resolve = 屏幕真实内容）
        if frame in (30, 60):
            print('[state]', pygame.render.screen_frame_state())
            samples = pygame.render.screen_probe()
            if samples:
                print('[compute-probe] 帧缓冲样本', len(samples))
        frame += 1
        await pygame.next_frame()
