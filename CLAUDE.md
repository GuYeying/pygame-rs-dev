- **批次 60 补（2026-10-26 实测）**：脚本直跑形态补**预置窗**
  （set_mode 的 Python 绑定只记录需求，真窗由宿主壳装配——脚本
  直跑形态本壳就是宿主；缺失 = 绘制调用 panic 渲染未初始化）。
  **坑位**：pypg 仓 main.rs 的插入曾被 IDE 缓冲覆盖丢失（Edit 成功
  后文件又回旧版）——多轮实测才定位；**script_test 里的 pypg.exe
  是用户旧拷贝**（拆仓验证时的版本，无 bins 栈修/帧桥/预置窗）——
  分发前须以 pygame-rs target/release 的最新构建替换。
- **批次 60 补 2（2026-10-26 实测）**：脚本分支补**入口自动探测+帧驱动**
  （run_code_obj 后查 globals main → coroutine → send(None) 帧驱动循环
  ——游戏门脚本 `pypg <脚本.py>` 直接跑，免 --game）；setup_stdio 收
  globals（PyDictRef）+ run_code_obj 收 Scope::new(None, globals)
  ——RP 嵌入的类型形态定约（run_code_obj 要 Scope 非 PyDict）；
  build.rs 补 **bins 栈参数**（原 examples 专属——pypg/runner 为
  bin 目标，debug 栈 1MB 撞 RP 引导帧深 → essential init failed，
  实测根因）；games/ 测试目录（async_demo/draw_test）。
