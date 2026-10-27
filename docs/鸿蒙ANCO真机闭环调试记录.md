# 鸿蒙 ANCO 真机闭环调试记录（原生 UI demo 阶段）

> 2026-10-27 会话。环境：鸿蒙 NEXT + ANCO 兼容层（无 adb，hdc 仅文件通道），
> 调试机 6CS9K26107041278；后段 ColorOS 真机损坏，全部真机验证在 ANCO 完成。
> 交付形态：pygame.rsdev 原生 UI demo app——列表选脚本 → SurfaceView 挂引擎
> → 布局 letterbox / 画布即屏幕双呈现 → 换场闭环。

## 一、崩溃类

### 1. 进程闪退（SIGABRT / FORTIFY destroyed mutex）
- **现象**：脚本退出后进程 abort，crash buffer 报
  `pthread_mutex_lock called on a destroyed mutex @ HeapTaskDaemon`。
- **根因**：`std::process::exit()` 的 exit() 先跑 atexit 与 C++ 静态析构，
  窗口期内 Java 线程（HeapTaskDaemon 等）仍活着，撞上已销毁互斥锁。
- **修复**：`unsafe { _exit(code) }` 直退系统调用，绕过析构窗口
  （android_host.rs 顶部 extern + 注释）。
- **验证**：crash buffer 完全干净，am_proc_died 无伴随崩溃。

### 2. 带音频脚本开跑 ~37ms 即崩（ndk-context）
- **现象**：脚本一碰音频立刻死，dev_log 抓到
  `ndk_context::android_context(): android context was not initialized`。
- **根因**：嵌入形态无 glue，无人调 ndk-glue 的上下文初始化；cpal/robius
  等 JNI 消费方启动即读该静态。
- **修复**：新增 `nativeInitContext` JNI 导出（GetJavaVM + NewGlobalRef +
  `ndk_context::initialize_android_context`），Java 层 `initContext`
  包装守卫防二次初始化断言；`ndk-context = "0.1.1"` 入 pygame-rs-dev
  android 依赖（与依赖树同版本单实例）。

### 3. 换场崩溃（卡死后崩退，两轮根因）
- **第一轮（ColorOS）**：旧场 detach 落下的 `CloseRequested` 残留事件被
  新场首帧吃到 → 新场 18ms 即收口黑屏。修复：attach 时清残留事件队列 +
  重置单窗槽位（窗口生命周期按"运行"而非"进程"计）。
- **第二轮（ANCO）**：收尾 `release_surface()` 无参取"当前槽位"——换场
  重叠期跑在新场 attach 之后，把新场窗口连坐释放 → present-after-release。
  修复：detach 暂存**本尊指针**入 pending-release 槽，收尾精确释放该指针，
  永不碰当前窗口。
- **第三轮（ANCO 实证推翻）**：`ANativeWindow_fromSurface` 在 ANCO 对同一
  Surface 未必返回同一指针 → 指针比对把合法 detach 误判为陈旧 → 引擎永不
  收口（音频不停）+ 新场黑屏。终案：**比对清除废除**，陈旧 detach 判定前置
  到 **Java 层代数守卫**（`activeGen` 计数，身份零歧义），Rust 侧无条件
  清槽 + CloseRequested。

### 4. egui "点击无响应"真相：进程原生崩死
- **现象**：点击后 ~2 秒进程死（ksys_log 无销毁记录直接重启新 pid）。
- **手段**：信号遗嘱——SIGSEGV/SIGABRT/SIGBUS/SIGFPE/SIGILL/SIGSYS 处理器
  死前往 dev_log 写 `=== SIGxxx（原生致命崩溃）===` 标记，与 Rust panic 的
  `[PANIC]` 钩子区分；fd 常驻打开（open 非 async-signal-safe），写后
  signal(SIG_DFL)+raise 保留系统 tombstone。

## 二、事件与坐标类

### 5. JNI 原生类型错配 → 触摸坐标恒 (0,0)
- **现象**：事件流到达脚本、wants_pointer 能翻转，但坐标恒 (0,0)，
  红圈钉死 view 左上角。
- **根因**：Java `injectTouch(int, float, float)` vs Rust `(u8, f64, f64)`
  ——AArch64 上 Java float 走 32 位浮点寄存器（s0/s1），Rust 按 64 位
  d 寄存器读出零/垃圾。
- **修复**：JNI 边界逐一对齐 `action: i32, x: f32, y: f32`，边界处扩宽
  f64。教训入注释：**JNI 原生类型必须逐一对齐 Java**。

### 6. 窗口化触摸/egui 整体偏移一个边距
- **现象**：只有 view 左上角重叠区可交互。
- **根因**：`Activity.onTouchEvent` 的 `getX()` 是全屏系，view 居中有
  偏移——未减原点直接 1:1 喂引擎。
- **修复**：非全屏 `(x - sv.getX())/viewScale`；全屏等比 `(x-ox)/s`。

### 7. ANCO 返回手势不触发 onBackPressed → 音频泄漏 + 引擎不收口
- **现象**：退出脚本后引擎仍在跑（音频继续），换场新引擎被旧引擎搅局。
- **根因**：stop 标志从未设置（ksys_log 全程无 onBackPressed 行），
  引擎收口只押 detach 的 CloseRequested，循环未轮转到泵时即漏。
- **修复**：`surfaceDestroyed`（代数守卫内）**先 stop() 再 detach**——
  surface 销毁 = 本场必终，双保险。

## 三、呈现类

### 8. setBuffersGeometry 实验：撤销并存档
- **尝试**：脚本 set_mode 时 `ANativeWindow_setBuffersGeometry` 改 buffer
  几何让合成器缩放铺满。
- **结果**：几何本身生效，但先于几何变更创建的 EGLSurface 在驱动上不跟随
  重建 → eglSwapBuffers 恒失败 → 全黑（帧循环健康、无 panic、present
  静默丢帧）。
- **结论**：正解在 wgpu/EGL 层（surface 重建）或引擎 letterbox MVP；
  `set_size` 维持 no-op，实验全文存档于注释。

### 9. 窗口化"egui 太小" → view 自适应整数倍放大
- 800×600 逻辑画布在高分屏上 1:1 太小；view 按逻辑尺寸 × 整数倍
  （`max(1, min(rootW/w, rootH/h))`，布局完成后按 root 实测尺寸计算——
  ANCO 的 `getDisplayMetrics` 横屏下谎报竖屏值）居中；buffer 仍钉逻辑
  尺寸，触摸除以倍数。egui/画面视觉 ×2，坐标链恒 1:1。

### 10. 全屏两版语义（最终案：画布即屏幕）
- 非等比拉伸版：铺满但 UI 变形 + 点击直觉错位 → 废。
- 等比 letterbox 版：不变形但长轴留黑边。
- **终案（画布即屏幕）**：全屏旗标脚本由壳层把 set_mode 字面尺寸改写为
  屏幕实测尺寸（正则换数字），buffer 钉屏幕尺寸，view 铺满——四方同尺寸
  纯 1:1，满屏且零变形，egui CentralPanel 自动铺满，`get_size()` 返回
  真实屏幕尺寸（"std 屏幕尺寸接口"的壳层先行版）。边界：动态尺寸
  set_mode 无法改写；固定坐标内容集中在左上区域属"画布即屏幕"语义。

### 11. 全屏判定正则两连
- 文本 `contains("FULLSCREEN")`：注释残词误判（窗口化变体被拉成全屏）。
- `set_mode\([^)]*FULLSCREEN`：元组右括号截断 `[^)]*`，永远漏判。
- **终案**：`set_mode\s*\(.*FULLSCREEN`（同行匹配，调用形每行一条）。

## 四、诊断基建（本轮沉淀，长期资产）

1. **starfish-std `set_diag_sink`**：console_log 每条同步转发落盘——
   Android stdout 是 logcat 管道块缓冲，进程死时未冲刷内容全丢。
2. **Java 层 SdLog**：优先 `/sdcard/Download`（兼容应用数据/Download，
   文件管理器直取），崩溃钩子全栈落盘，生命周期埋点。
3. **信号遗嘱**（见一.4）。
4. **呈现自证探针**：surface_size_probe——画布描边 + 四角点 + 指针红圈，
   锚点/拉伸/触摸映射一张截图全诊断；`[present]` 一次性映射参数日志。
5. **双日志通道**：ksys_log.txt（Java 层）/ dev_log.txt（引擎层），
   均落 /sdcard/Download。

## 五、环境与工具

- ANCO 无 adb：hdc 仅文件通道（`MSYS_NO_PATHCONV=1` 防 Git Bash 路径
  转换），装 APK/取证靠 USB 文件传输 + 文件管理器。
- 本机 `python` 是商店占位符：用 `/d/Toolchains/emsdk/python/3.13.3_64bit/python.exe`。
- 引擎 API 与真 pygame 差异（demo 脚本三连坑）：`pygame.QUIT` →
  `pygame.event.QUIT`；`e.pos` → `e.x/e.y`；`pygame.draw.*` → 会话方法
  `r.line(...)` 系。

## 六、桌面端顺手修

- 正常返回（退出码 0）的脚本落进 `unreachable!("block_on 完成后必经
  exit")`：desktop_main 改为无条件 `std::process::exit(code)`。
