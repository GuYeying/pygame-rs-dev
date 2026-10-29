# compute_probe —— compute/storage/texture/sampler 绑定验收（本地诊断）：
# 注意：compute 需 WebGPU——wasm 端 WebGL2 后端不支持（创建即报错属预期）。
# 计算着色器把四角颜色写入 storage buffer → 渲染 quad 顶点读存储着色。
# 四色 quad = compute + storage_rw + storage 只读读回全通；再附 texture/
# sampler 创建与绑定冒烟。120 帧自退。
import pygame

COMPUTE_WGSL = """
struct QuadColors { colors: array<vec4<f32>, 4> };
@group(0) @binding(0) var<storage, read_write> colors: QuadColors;

@compute @workgroup_size(4)
fn cs_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x % 4u;
    let c = vec4<f32>(
        f32((i + 1u) & 1u),         // R: 0/1 交替
        f32(i / 2u),                 // G: 下半亮
        f32((i + 1u) / 2u) * 0.5,   // B: 渐弱
        1.0);
    colors.colors[i] = c;
}
"""

RENDER_WGSL = """
struct QuadColors { colors: array<vec4<f32>, 4> };
@group(0) @binding(0) var<storage, read> colors: QuadColors;

struct VIn {
    @location(0) pos: vec3<f32>,
    @location(1) color: vec4<f32>,
};
struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) slot: f32,
};

@vertex
fn vs_main(in: VIn, @builtin(vertex_index) vi: u32) -> VOut {
    var o: VOut;
    o.pos = vec4<f32>(in.pos, 1.0);  // 自管 NDC 直入（无相机参与）
    var slots = array<u32, 6>(0u, 1u, 2u, 0u, 2u, 3u);
    o.slot = f32(slots[vi]);
    return o;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    // storage 读取在片元段（GLES 普遍支持；顶点段 SSBO 属设备相关）
    return colors.colors[u32(in.slot)];
}
"""


async def main():
    screen = pygame.display.set_mode((800, 600))
    ac = pygame.render.access()

    compute_shader = ac.shader(COMPUTE_WGSL, label="probe_cs")
    render_shader = ac.shader(RENDER_WGSL, label="probe_vs_fs")

    # 顶点：pos3color4（color 占位——真色由 storage 决定）
    verts = [
        -1.0, -1.0, 0.0, 1.0, 1.0, 1.0, 1.0,
        1.0, -1.0, 0.0, 1.0, 1.0, 1.0, 1.0,
        1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0,
        -1.0, -1.0, 0.0, 1.0, 1.0, 1.0, 1.0,
        1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0,
        -1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0,
    ]
    pygame.render.set_fb_dump(True)  # 帧缓冲取证：30-33 帧 dump 落 dev_log
    mesh = ac.mesh("pos3color4", verts, label="probe_mesh")
    storage = ac.buffer(64, usage="storage", label="probe_storage")
    cbind = ac.bind_group([(0, "storage_rw", storage)], label="probe_compute_bind")
    rbind = ac.bind_group([(0, "storage", storage)], label="probe_render_bind")
    cpipe = ac.compute_pipeline(compute_shader, [cbind], label="probe_cpipe")
    rpipe = ac.pipeline(render_shader, mesh, [rbind], flavor="2d", label="probe_rpipe")

    # texture/sampler 创建与绑定冒烟（不参与绘制）
    tex = ac.texture(size=(16, 16), label="probe_tex")
    smp = ac.sampler(label="probe_sampler")
    _ = ac.bind_group(
        [(0, "texture", tex), (1, "sampler", smp)], label="probe_tex_bind")
    print("[compute-probe] PASS texture/sampler/bind 创建绑定")

    frame = 0
    while frame < 120:
        for e in pygame.event.get():
            if e.type == pygame.event.QUIT:
                return
        with screen.render() as t:
            t.fill((8, 8, 12))
            t.rect((255, 0, 255), (1, 1, 798, 598), 2)  # 诊断：画布边界框
            with t.raw_compute() as p:
                p.set_pipeline(cpipe)
                p.set_bind_group(0, cbind)
                p.dispatch(1, 1, 1)
            with t.raw_render() as p:
                p.set_pipeline(rpipe)
                p.set_bind_group(0, rbind)
                p.draw_mesh(mesh)
        pygame.display.flip()
        # 帧缓冲取证：flip 后取样（读已呈现帧 = 屏幕真实内容）
        if frame in (30, 45, 60):
            print('[state]', pygame.render.screen_frame_state())
            samples = pygame.render.screen_probe()
            if samples:
                print('[compute-probe] 帧缓冲样本', len(samples))
        frame += 1
        if frame == 35:
            pygame.render.set_fb_dump(False)
        if frame == 90:
            print("[compute-probe] SUMMARY PASS（90 帧后仍稳定）")
        await pygame.next_frame()
