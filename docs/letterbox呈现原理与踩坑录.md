# letterbox 呈现：实现原理与踩坑录

> 2026-10-28 定稿。适用 = starfish-pygame 呈现层的 Android 窗口化 letterbox
> 体系（桌面/web/全屏恒等，免费继承）。本文记录现行实现原理与本次真机
> 联调踩过的全部坑——坑按"现象 → 根因 → 修复 → 判定"组织，后来者按图索骥。

---

## 一、模型与责任划分

**三个坐标系**：

| 坐标系 | 取值 | 归属 |
|---|---|---|
| 物理表面 | 原生窗口实测尺寸（如 2412×1084） | 宿主壳 / ANativeWindow |
| 逻辑画布 | `set_mode(W, H)` 语义尺寸（如 800×600） | 脚本 |
| NDC | [-1,1]² clip 空间 | 引擎内部 |

**一句话模型**：物理表面恒为原生窗口实测尺寸（不随请求缩小）；逻辑画布归
脚本 `set_mode` 所有；呈现 = 逻辑画布等比缩放（`s = min(Pw/Lw, Ph/Lh)`）后
居中——短轴铺满、长轴留黑边、零变形零裁剪。

**责任划分定约**：壳（ScriptActivity）只提供"满屏 SurfaceView + 原始触摸"，
**零呈现逻辑**；letterbox 的全部语义在引擎（starfish-pygame 呈现层）。
换算公式唯一收敛在 `starfish-std/src/letterbox.rs`（纯计算零依赖），
谁用谁调，公式不漂移。

---

## 二、当前实现

### 2.1 Fit 数学（`starfish-std/src/letterbox.rs`）

```
scale   = min(Pw/Lw, Ph/Lh)          // 短轴比；可 >1（放大）可 <1（缩小）
offset  = (P - L×scale) / 2          // 长轴两侧黑边 = |P - L×s|/2
正变换  logical_to_physical(x,y) = (x·s + ox, y·s + oy)
逆变换  physical_to_logical(x,y) = ((x-ox)/s, (y-oy)/s)   // 不钳制
```

- 逻辑/物理零尺寸 = 恒等回退（除零保护）；
- 同比例 → 铺满零黑边；scale 浮点连续，无整数取整；
- **过大不裁剪只整体缩小**（min 的意义）——画面永远完整可见。

### 2.2 呈现映射 `fit_ndc`：NDC 域统一组合

`Screen::fit_ndc()`（target.rs）产出 **clip 空间仿射矩阵**（底行
`(0,0,0,1)`——透视除法后恰为"NDC 缩放+平移"），把整幅 NDC 收进画布
子矩形。pack 组合：

```
mvp = fit_ndc × resolve_mvp(相机状态, 逻辑尺寸)
```

**相机状态机三态统一**（`render/camera.rs`）：

| 状态 | resolve_mvp（一律**逻辑画布域**） | 说明 |
|---|---|---|
| Auto2d | `ortho(Lw, Lh)` | 像素直入 |
| Auto3d | `perspective(fov, Lw/Lh)` | 纵横比归画布 |
| Manual | 用户相机 `mvp()`（本就按逻辑画布构造） | 全权接管投影 |

组合后 Auto2d 与旧实现（`正交(物理)×平移×缩放`）**逐位等价**（恒等桌面）；
Manual 修正双映射；Auto3d 修正纵横比。同一 `fit_ndc` 也供
`present_mvp()`（= fit_ndc × 全局相机）与 `present_fit()`（自管相机对齐口，
见坑 4）。

### 2.3 画布 scissor（"画布外不可见"）

桌面窗口即画布，出界绘制被窗口边缘天然硬裁；Android letterbox 画布是
物理表面的**子矩形**，黑边只是清屏色——pack 时显式 `set_scissor_rect`
（脚本 `set_clip` 映射 ∩ 画布子矩形，**非恒等才启用**，恒等零动作）。
覆盖 batch 通道与 raw_render 回放（`OverlayCtx::with_pass` 起的叠加通道
**无 scissor**——见坑 5 的教训：矩阵组合管"画在哪"，scissor 管"越界封死"，
两者都要）。

### 2.4 输入逆映射（注入边界一次转换）

Java `injectTouch`（原始窗口像素）→ 引擎 `event.rs` → `display::map_point`
→ `Fit::physical_to_logical`——**触摸进引擎即翻译为逻辑坐标**，此后脚本、
egui、任何消费者看到的是同一套逻辑坐标（hit 与视觉天然一致）。恒等平台
自动退化为透传。

### 2.5 消费者对齐矩阵（"游戏内容与 X 永远对齐"）

| 消费者 | 对齐方式 | 代码位置 |
|---|---|---|
| batch 主绘制 | pack 内 fit_ndc 组合 | target.rs `pack` |
| egui（draw_ui） | `screen.present_mvp()` 合成 + screen_rect=逻辑画布 | pygame_gui/runtime.rs |
| 物理调试线（follow_global） | 调用方传入 present_mvp（Screen）/表面尺寸解算（Surface） | pygame_physics/runtime_2d·3d |
| 自定义管线（自管相机） | `write_camera_mvp(buf, cam.mvp())` 引擎左乘 / `present_fit()` 手动左乘 | pygame/render.rs |
| 自绘全局相机 | `write_camera_mvp(buf)` 单参 = present_mvp | 同上 |

**自管相机定约**：write_buffer 直写的相机矩阵对引擎是不透明字节，无法
拦截组合——脚本在设置相机处调 `write_camera_mvp` 即完成引擎侧插入。

### 2.6 平台语义速查

| 场景 | 物理 | 逻辑 | 呈现 |
|---|---|---|---|
| 桌面/web | 窗口/画布即请求尺寸 | ==物理 | 恒等（零黑边） |
| Android FULLSCREEN | 原生实测 | ==物理（引擎取实测） | 恒等 1:1 |
| Android 窗口化 | 原生实测 | 脚本请求值 | letterbox |
| 脚本读 argv 自适应（sys.argv[-2:]） | 原生实测 | ==物理 | 恒等（脚本主动全屏） |

旋转/窗口尺寸变化：fit 每帧现算，黑边即时重算；Android `handle_resized`
不回填逻辑（逻辑归 set_mode 所有）。

---

## 三、踩坑录

### 坑 1：fill 用物理尺寸——黑边被涂成画布色

- **现象**：真机 letterbox 黑边只剩左侧半条，画布色一路铺到屏幕右缘。
- **根因**：`Screen::fill` 的满屏 quad 取 `self.size()`（物理尺寸），而
  batch 顶点在 pack 时统一过呈现映射——物理尺寸的 quad 再被平移+缩放，
  覆盖 `[ox, ∞)`，右/下黑边全被涂掉（桌面逻辑==物理恒等，从不暴露）。
- **修复**：fill 改用 `logical_size()`——fill 的语义对象是脚本画布，
  与 batch 其余顶点同一坐标系。
- **判定**：canvas fill 色与纯黑差异极小时肉眼不可判——锚点用**内容边界
  位置**（见 §五）。

### 坑 2：Manual 相机双映射——内容越画布溢出

- **现象**：blit_transform_demo 在 Android 窗口化下整幅内容飞出画布右/下，
  标题文字被裁。
- **根因**：pack 旧实现对**任何**相机输出叠 `平移(ox)×缩放(s)`。该组合
  只对"按物理尺寸解算"的 Auto 相机成立；Manual 相机输出已是 NDC（逻辑域
  构造），物理域 T·S 叠上去等于把坐标**再当逻辑值解读一次**——逻辑 x=0 →
  物理 483 → 被当逻辑 483 → 屏幕 1454px。
- **修复**：呈现映射改 NDC 域统一组合（§2.2）——相机（Auto/Manual）一律
  解算在逻辑画布域，输出统一收进画布子矩形；Auto2d 逐位等价。
- **判定**：`[wcmvp]`/`[present]` 打点数值与手算对照（§五.1）。
- **教训**：**定约必须落在单一执行点**。当时"Manual 相机尊重覆盖"只在
  present_mvp（egui 侧）实现，pack 侧漏了——同一语义两处实现必漏一处。

### 坑 3：自管相机引擎无法拦截——接口缺口（非回归）

- **现象**：custom_pipeline_demo 无全屏旗标却全屏渲染（自绘波纹铺满）。
- **根因**：自绘管线经 `write_buffer` 直写的相机矩阵对引擎是**不透明
  字节**，无法拦截组合——正交(800,600) 直接映射全屏 NDC。
- **修复**：接口补齐——`write_camera_mvp(buf, cam_mvp)` 双参形态（引擎
  侧左乘 present_fit，脚本零矩阵知识）+ `present_fit()` 暴露（显式拿矩阵）。
- **教训**：自管相机的 letterbox 对齐是**脚本的调用义务**，引擎给工具
  不做魔法；demo 必须示范正确调用（此前 demo 自己都没调）。

### 坑 4：画布外可见——黑边可被覆盖（语义缺失）

- **现象**：越界绘制（出界坐标/未对齐自绘）溢进黑边区可见；桌面则被窗口
  边缘天然硬裁。
- **根因**：黑边只是清屏色，全链路无裁剪。
- **修复**：pack 画布 scissor（§2.3）——与脚本 set_clip 相交，非恒等启用。
- **判定**：`[present]` 行 `scissor=Some([ox, 0, Lw×s, Ph])` 即激活。
- **注意**：叠加通道（OverlayCtx）自成 pass **无 scissor**——矩阵组合管
  位置，scissor 管边界，两条防线分别在 pack 与叠加，缺一即漏。

### 坑 5：egui "windowed" 全屏——脚本自适应通道（非 bug，易误判）

- **现象**：egui_test_windowed 无全屏旗标却全屏，事件"无偏移"。
- **根因**：脚本自己从 `sys.argv[-2:]` 读壳附加的原生实测尺寸 set_mode——
  逻辑==物理恒等，全屏与事件零偏移自洽。
- **处置**：demo 改固定 800×600（真·窗口化，真机验证 egui letterbox 链路）；
  argv 铺满形态保留为文档。
- **教训**：**"没有全屏旗标"≠"会 letterbox"**——set_mode 的实际请求值才
  是判据（[present] 行的 `逻辑=` 字段）。

### 坑 6："事件需要额外坐标转换吧？"——已经存在，位置在注入边界

- **疑问**：egui 按窗口尺寸做命中测试，letterbox 下事件难道不需要矩阵
  翻译？
- **答案**：需要且**已存在**——`event.rs` 在注入边界调 `map_point`
  （fit 的逆矩阵），触摸进引擎即翻译为逻辑坐标；此后 egui 布局/命中与
  渲染（present_mvp）在同一坐标系。**三个坐标系在引擎两个边界各转换一次**，
  脚本与 egui 零感知。

### 坑 7：Auto3d 纵横比取物理——透视变形

- **现象/根因**：Auto3d 按目标（物理）尺寸取纵横比，窗口化下透视被拉伸。
- **修复**：随 §2.2 统一改为逻辑画布纵横比 + fit_ndc 组合（§2.2）。

### 附：真机排查连环坑（非 letterbox 本体，但全程干扰判定）

| 坑 | 现象 | 根因/修复 |
|---|---|---|
| versionCode 恒定 | "安装成功"实为 ROM 静默保留旧应用——文件新/装了/日期对三样都真，跑的还是旧代码 | versionCode/Name 随构建注入（aapt2 flag 不覆盖 manifest 显式属性，须模板替换） |
| 构建 ID 不变 | ksys 首行无法判包新旧 | SdLog.BUILD 占位符，javac 前注入构建时刻 |
| `loadCode（6322 B）` 误读 | 误判"设备跑旧脚本"（实为新脚本） | Java `String.length()` 是**字符数**非字节数——中文 UTF-8 7627B = 6322 字符 |
| wasm 页面挂死 | SyntaxError / load_code 链接错 / OOB / TypeError | 独立问题族：模板 `\n` 转义、`--gc-sections` 裁掉 wasm_bindgen 导出（空 main 须触碰模块）、`?script=` 无人分派、ensureInit 双实例竞态——见 git log 164008 前后提交 |

**方法论教训**：真机判定先立**指纹**（构建 ID / 脚本字节数 / 字符数），
再谈现象；指纹不清时一切现象归因皆不可信。

---

## 四、调试工具与取证方法

### 4.1 引擎打点（dev_log.txt，`setLogDir` 落点）

```
[present] 物理=2412x1084 逻辑=800x600 scale=1.807 ox=483.3 oy=0.0 scissor=Some([483, 0, 1445, 1084])
```
首 pack 一次。判读：`scale`/`ox` 非恒等 = letterbox 激活；`scissor` = 裁剪
矩形；`逻辑=` 即脚本 set_mode 实际生效值。

```
[wcmvp] fit sx=0.5992 sy=1.0000 tx=0 ty=0 | cam平移=(-1,1) | 合成平移=(-0.6,1) 合成sx=0.0015
```
write_camera_mvp 首调一次。判读：`fit sx` = 画布宽占屏比（恒等=1）；
`合成sx` = fit_sx×2/逻辑宽；组合正确时 `合成平移 = fit(相机平移)`。

### 4.2 桌面复现钩子

`STARFISH_DESKTOP_LETTERBOX=宽x高`——窗口实测钉为该值、逻辑恒 = set_mode
请求：Android 窗口化行为在本机复现（自绘/叠加通道 letterbox 对齐本地可调，
RenderDoc 可用）。未设置零行为变化。

### 4.3 像素测量法

截图用 PowerShell `GetPixel` 逐行扫描：黑边宽度、内容边界、特定色块
（本 demo 画布色 (30,30,46)/wave 青) 的精确位置——**肉眼不可分（近黑画布
vs 纯黑）时唯一的客观手段**。

### 4.4 判定锚点

- letterbox 是否生效：**内容边界位置**（如标题左缘应在 `ox` 处），不是
  黑边可见性；
- 内容是否完整：min 规则保证永不裁剪；
- 命中是否一致：touch 日志坐标 → 逻辑坐标换算对照。

---

## 五、遗留注意

- **出界触摸坐标为负/超界**（不钳制）——脚本如在意自行判断；
- **叠加通道无 scissor**——raw_render 内容靠矩阵组合约束，配合 pack 裁剪
  双保险（若未来开放叠加 scissor 需求，在 `OverlayCtx::with_pass` 补）；
- **`set_clip` 与画布相交**：脚本 clip 过映射后与画布矩形取交，零面积 =
  不裁；
- 转屏：物理尺寸变化即时重算（fit 每帧现算），Android 经
  `handle_resized` 自愈，逻辑不回填；
- iOS/鸿蒙：呈现层平台无关，原生免费继承；宿主壳按同一 Fit 公式各自实现
  （letterbox.rs 头注定约）。
