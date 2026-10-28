#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""pygame-rs-dev Web 构建（host 一次编译，游戏免重编译）。

用法：
    python scripts/build_web.py            # 构建 wasm host + web/ 产物目录

产物：pygame-rs-dev/web/
    index.html / python.js / python_bg.wasm   ← host（编译一次）
游戏部署：把脚本放进 web/ 并命名为 main.py（或任意名后
    ?script=<名> 指定），打开 index.html 点启动；页面亦支持选择本地
    .py 单文件直接运行（-c 形态，load_code 通道）。

测试：cd web 后起静态服务（file:// 无效，fetch 受 CORS 限制）——
    也可用本解释器内置服务器：python.exe serve
前置：rustup target add wasm32-unknown-unknown；wasm-bindgen-cli 版本
      与 Cargo.lock 严格一致（脚本自检）。
"""

import argparse
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
WEB = ROOT / "web"
TARGET_DIR = ROOT / "target" / "wasm32-unknown-unknown" / "release"
LOCK = ROOT / "Cargo.lock"

# 模板占位符 __BUILD_TS__ 由脚本替换（不用 str.format——JS/CSS 花括号免转义）。
# r""" 原始串：模板内 JS 的 '\n'/正则 /\s+/ 须原样到达输出——普通串会被
# Python 转义成真换行注入 JS，整段 module script SyntaxError（真机踩坑）
INDEX_TEMPLATE = r"""<!DOCTYPE html>
<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<title>pygame-rs-dev · web 宿主</title>
<style>
  html, body { margin: 0; height: 100%; background: #000; color: #ccc;
    display: flex; }
  /* ── 左侧控制面板 ── */
  #side { width: 300px; flex: none; box-sizing: border-box; padding: 14px 12px;
    display: flex; flex-direction: column; gap: 10px;
    border-right: 1px solid #222; background: #0a0a0a; }
  #side h1 { font: 15px/1.4 sans-serif; color: #ddd; margin: 0; }
  #side label { font: 12px/1.4 sans-serif; color: #888; }
  #script-path { box-sizing: border-box; width: 100%; padding: 8px 10px;
    font: 13px/1 Consolas, monospace; color: #eee; background: #111;
    border: 1px solid #333; border-radius: 6px; }
  #script-path:focus { outline: none; border-color: #4a4; }
  #run-btn, #pick-btn { padding: 10px 12px; font: 14px/1 sans-serif;
    color: #eee; background: #222; border: 1px solid #555;
    border-radius: 6px; cursor: pointer; text-align: left; }
  #run-btn:hover { background: #333; border-color: #888; }
  #pick-btn { color: #9c9; background: #141414; border: 1px dashed #4a4; }
  #pick-btn:hover { background: #1e1e1e; }
  #status { font: 11px/1.6 Consolas, monospace; color: #6c6;
    min-height: 48px; white-space: pre-wrap; }
  #hint { font: 11px/1.6 sans-serif; color: #555; margin-top: auto; }
  /* ── 右侧主区：画布（尺寸由游戏 set_mode 决定，页面只防溢出）── */
  #main { flex: 1; display: flex; flex-direction: column;
    align-items: center; justify-content: center; min-width: 0; }
  canvas { outline: none; max-width: 96vw; max-height: 82vh; }
  #raf { color: #6c6; font: 11px/1 Consolas, monospace; opacity: .8;
    align-self: flex-end; padding: 4px 10px; }
  #wlog { width: calc(100% - 24px); box-sizing: border-box; height: 150px;
    overflow-y: auto; padding: 4px 8px; white-space: pre-wrap;
    color: #9c9; background: #111; border: 1px solid #222; border-radius: 4px;
    font: 11px/1.5 Consolas, monospace; text-align: left; }
</style>
</head>
<body>
<div id="side">
  <h1>pygame-rs-dev · web 宿主</h1>
  <label for="script-path">脚本路径（web/ 目录内）</label>
  <input id="script-path" placeholder="main.py">
  <label for="script-args">命令参数（空格分隔，透传 sys.argv）</label>
  <input id="script-args" placeholder="--level fast">
  <button id="run-btn">▶ 运行</button>
  <label>本地单文件（-c 形态，参数同样生效）</label>
  <button id="pick-btn">选择本地 .py 运行</button>
  <div id="status"></div>
  <div id="hint">▶ 运行 = 重载并运行输入的脚本；本地文件 = 选完即自动运行。
    均为独立进程（干净切换，无状态残留）。多模块工程暂以桌面端为准。</div>
</div>
<div id="main">
  <canvas id="starfish-we"></canvas>
  <div id="raf">raf 0</div>
  <pre id="wlog" aria-label="console log"></pre>
</div>
<script type="module">
(() => {
  let n = 0;
  const el = document.getElementById('raf');
  const tick = () => { n++; if ((n % 10) === 0) el.textContent = 'raf ' + n;
                       requestAnimationFrame(tick); };
  requestAnimationFrame(tick);
})();

const _wl = document.getElementById('wlog');
const _fmt = a => a.map(x => { try {
  return (x && x.message !== undefined) ? x.message : String(x);
} catch (e) { return '<unprintable>'; } }).join(' ');
for (const m of ['log', 'info', 'warn', 'error']) {
  const orig = console[m].bind(console);
  console[m] = (...a) => {
    _wl.textContent += '[' + m + '] ' + _fmt(a) + '\n';
    const lines = _wl.textContent.split('\n');
    if (lines.length > 200) _wl.textContent = lines.slice(-200).join('\n');
    _wl.scrollTop = _wl.scrollHeight;
    orig(...a);
  };
}
window.addEventListener('error', e => {
  _wl.textContent += '[onerror] ' + (e.message || e.error) +
    (e.filename ? ' @ ' + e.filename + ':' + e.lineno : '') + '\n';
});
window.addEventListener('unhandledrejection', e => {
  _wl.textContent += '[rejection] ' + e.reason + '\n';
});

// ── 控制面板 ─────────────────────────────────────────────────
const params = new URLSearchParams(location.search);
const bootScript = params.get('script');
const bootArgv = params.getAll('argv');
const pathInput = document.getElementById('script-path');
const argsInput = document.getElementById('script-args');
const runBtn = document.getElementById('run-btn');
const pickBtn = document.getElementById('pick-btn');
const fileInput = document.createElement('input');
fileInput.type = 'file';
fileInput.accept = '.py';
document.body.appendChild(fileInput);
fileInput.style.display = 'none';
const statusEl = document.getElementById('status');
const status = s => { statusEl.textContent = s; };

pathInput.value = bootScript ?? 'main.py';
argsInput.value = bootArgv.join(' ');

let exportsPromise = null; // init 单飞（缓存 Promise 本身！）
let pageStarted = false; // 本页是否已运行过程序（进程一次契约）
// 缓存必须是 Promise：顶层点火与 boot 分支在同一 tick 内同步连调
// ensureInit，缓存"结果"会让两次都看到空值 → 双 init() → 双 wasm
// 实例共享同一份 glue 对象堆——槽位互踩，回调里 getObject 拿到
// undefined（getBoundingClientRect TypeError / 任意 OOB，真机实测：
// "web 宿主就绪"×2 即此症状）
const ensureInit = () => {
  if (!exportsPromise) {
    // wasm 本体带版本参数：index.html/python.js 都随构建换 URL，唯独
    // python_bg.wasm 恒名——浏览器启发式缓存会吃旧 wasm（宿主逻辑更新
    // 不生效，真机实测"守卫无效"假象）
    exportsPromise = init({ module_or_path: './python_bg.wasm?v=__BUILD_TS__' }).then((ex) => {
      // shim 版导出（init() 返回的裸 wasm 导出无字符串编组）
      window.__pygame_rs_dev = {
        load_code: (c, a) => load_code(c, a ?? []),
        start_script: (u) => start_script(u),
      };
      return ex;
    });
  }
  return exportsPromise;
};
import init, { load_code, start_script } from './python.js?v=__BUILD_TS__';

const currentArgs = () => argsInput.value.trim().split(/\s+/).filter(Boolean);

const runCode = async (code, label) => {
  await ensureInit();
  load_code(code, currentArgs());
  pageStarted = true;
  status('运行 ' + label + '（-c 形态，argv = ' + JSON.stringify(currentArgs()) + '）');
};

// ── 路径通道（独立按钮）：▶ 运行 = 导航 ?script=<输入> + ?argv=<参数…>
//（干净重载，参数透传 sys.argv）────────────────────────────────
runBtn.addEventListener('click', async () => {
  const p = pathInput.value.trim() || 'main.py';
  if (bootScript === p && !pageStarted) {
    // 落在 ?script= 页面上的首次点击 = 本页还没真正分派过——补发
    // start_script（web_start 只装配不分派，2026-10-26 定约——分派权
    // 在页面 boot；此前此分支只改文案，载荷永不运行）
    await ensureInit();
    pageStarted = true;
    __pygame_rs_dev.start_script(p);
    status('运行 ' + p);
    return;
  }
  const sp = new URLSearchParams(location.search); // 保留 script 以外的参数（如 argv=...）
  sp.set('script', p);
  const args = currentArgs();
  sp.delete('argv');
  for (const a of args) sp.append('argv', a);
  location.search = '?' + sp.toString();
});
if (bootScript) {
  runBtn.textContent = '▶ 启动 ' + bootScript;
}

// ── 文件通道（独立按钮）：选择即自动运行；运行中再选 = 暂存 + 自动
// 重载重跑（一次动作完成切换）────────────────────────────────
pickBtn.addEventListener('click', () => fileInput.click());
fileInput.addEventListener('change', async () => {
  const f = fileInput.files[0];
  if (!f) return;
  const text = await f.text();
  if (!pageStarted) {
    await runCode(text, f.name);
  } else {
    sessionStorage.setItem('pygame_rs_dev_stash', f.name + '\n' + text);
    status('切换到 ' + f.name + '…（自动重载运行）');
    location.reload();
  }
});

// ── 暂存自动消费：重载后零点击自动运行切换脚本 ──────────────────
const stashRaw = sessionStorage.getItem('pygame_rs_dev_stash');
if (stashRaw !== null) {
  sessionStorage.removeItem('pygame_rs_dev_stash');
  const cut = stashRaw.indexOf('\n');
  runCode(stashRaw.slice(cut + 1), stashRaw.slice(0, cut));
}

// ── 自动点火（零点击）：wasm 加载即初始化；?script= 载荷在本页 init
// 就绪后经 start_script 分派（web_start 只装配不分派——分派权在页面
// boot，2026-10-26 定约）；暂存载荷自动消费 ─────────────────────────
ensureInit();
if (bootScript) {
  status('运行 ' + bootScript + '…');
  ensureInit().then(() => {
    pageStarted = true;
    __pygame_rs_dev.start_script(bootScript);
    status('运行 ' + bootScript);
  });
} else {
  status('就绪：输入路径运行，或选择本地 .py 文件');
}
</script>
</body>
</html>
"""


def sh(cmd, tail=0):
    print("+", " ".join(str(c) for c in cmd))
    r = subprocess.run(cmd, capture_output=True, text=True, errors="replace")
    if r.returncode != 0:
        print(r.stdout[-3000:])
        print(r.stderr[-3000:])
        sys.exit(f"[FAIL] 命令失败: {cmd[0]} …")
    if tail:
        print("\n".join(r.stdout.splitlines()[-tail:]))
    return r.stdout


def check_bindgen_version():
    """wasm-bindgen-cli 必须与 Cargo.lock 的 wasm-bindgen 严格一致。"""
    r = subprocess.run(["wasm-bindgen", "--version"], capture_output=True, text=True)
    m_cli = re.search(r"(\d+\.\d+\.\d+)", r.stdout)
    cli = m_cli.group(1) if m_cli else None
    lock_ver = None
    if LOCK.is_file():
        m_lock = re.search(r'name = "wasm-bindgen"\nversion = "([^"]+)"',
                           LOCK.read_text(encoding="utf-8"))
        lock_ver = m_lock.group(1) if m_lock else None
    print(f"[版本] wasm-bindgen-cli = {cli} / Cargo.lock = {lock_ver}")
    if cli != lock_ver:
        sys.exit(f"[FAIL] 版本不一致（cli {cli} ≠ lock {lock_ver}）——重装："
                 f"cargo install wasm-bindgen-cli --version {lock_ver}")


def main():
    ap = argparse.ArgumentParser(description="pygame-rs-dev Web 宿主构建")
    ap.parse_args()

    check_bindgen_version()
    print("=== 构建 wasm host（bin python）===")
    sh(["cargo", "build", "--release", "--target", "wasm32-unknown-unknown",
        "--bin", "python"])
    wasm = TARGET_DIR / "python.wasm"
    if not wasm.is_file():
        sys.exit(f"[FAIL] wasm 产物缺失: {wasm}")
    WEB.mkdir(parents=True, exist_ok=True)
    sh(["wasm-bindgen", "--out-dir", str(WEB), "--target", "web", str(wasm)])
    from datetime import datetime
    build_ts = datetime.now().strftime("%Y%m%d%H%M%S")
    (WEB / "index.html").write_text(
        INDEX_TEMPLATE.replace("__BUILD_TS__", build_ts), encoding="utf-8")

    # CJK 字体部署（宿主 preload_cjk 的取用路径；缺省静默）
    for src in (ROOT / "script_test" / "fonts" / "cjk.ttf",):
        dst = WEB / "fonts" / "cjk.ttf"
        if src.is_file() and not dst.is_file():
            dst.parent.mkdir(parents=True, exist_ok=True)
            dst.write_bytes(src.read_bytes())
            print(f"[OK] CJK 字体部署: {dst}")

    print("[OK] web/index.html + python.js + python_bg.wasm")
    print("=== 游戏部署 ===")
    print(f"  把脚本放入 {WEB} 并命名 main.py（或 ?script=<名> 指定）；")
    print("  页面亦可选本地 .py 单文件直接运行（-c 形态，load_code）")
    print("=== 测试 ===")
    print(f"  cd {WEB} 后起静态服务（或 python.exe serve），浏览器打开 index.html")


if __name__ == "__main__":
    main()
