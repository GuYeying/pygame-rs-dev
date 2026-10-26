#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""pygame-rs-dev Web 构建（host 一次编译，游戏免重编译）。

用法：
    python scripts/build_web.py            # 构建 wasm host + web/ 产物目录

产物：pygame-rs-dev/web/
    index.html / python.js / python_bg.wasm   ← host（编译一次）
游戏部署：把 bundle.py 放进 web/ 并命名为 main.py（或任意名后
    ?script=<名> 指定），打开 index.html 点启动；页面亦支持选择本地
    .py / bundle 单文件直接运行（load_payload 通道）。

测试：cd web 后起静态服务（file:// 无效，fetch 受 CORS 限制）
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

# 模板占位符 __BUILD_TS__ 由脚本替换（不用 str.format——JS/CSS 花括号免转义）
INDEX_TEMPLATE = """<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<title>pygame-rs-dev · web 宿主</title>
<style>
  html, body { margin: 0; height: 100%; background: #000;
    display: flex; flex-direction: column;
    align-items: center; justify-content: center; }
  canvas { outline: none; width: 800px; height: 600px; }
  p { color: #888; font: 13px/1.6 sans-serif; margin: 8px 0 0; }
  #raf { position: fixed; right: 8px; top: 6px; z-index: 20;
    color: #6c6; font: 11px/1 Consolas, monospace; opacity: .8; }
  #wlog { width: 800px; max-height: 140px; overflow-y: auto; margin: 6px 0 0;
    padding: 4px 8px; box-sizing: border-box; white-space: pre-wrap;
    color: #9c9; background: #111; border: 1px solid #222; border-radius: 4px;
    font: 11px/1.5 Consolas, monospace; text-align: left; }
  #start-overlay { position: fixed; inset: 0; background: rgba(0,0,0,.88);
    display: flex; flex-direction: column; gap: 14px;
    align-items: center; justify-content: center; z-index: 10; }
  #start-btn { padding: 14px 42px; font: 16px/1 sans-serif;
    color: #eee; background: #222; border: 1px solid #555;
    border-radius: 8px; cursor: pointer; }
  #start-btn:hover { background: #333; border-color: #888; }
  #pick-btn { padding: 12px 34px; font: 14px/1 sans-serif;
    color: #9c9; background: #1a1a1a; border: 1px dashed #4a4;
    border-radius: 8px; cursor: pointer; }
  #pick-btn:hover { background: #222; }
  #hint { color: #666; font: 12px/1.6 sans-serif; max-width: 640px;
    text-align: center; }
</style>
</head>
<body>
<canvas id="starfish-we"></canvas>
<div id="raf">raf 0</div>
<p>【pygame-rs-dev】web 宿主（?script=&lt;路径&gt; 指定载荷，缺省 main.py）</p>
<pre id="wlog" aria-label="console log"></pre>
<div id="start-overlay">
  <button id="start-btn">▶ 运行 main.py（?script= 可指定）</button>
  <button id="pick-btn">或选择本地 .py / bundle 单文件运行</button>
  <div id="hint">单文件 = 自包含脚本或 bundle（多模块游戏请先打包为 bundle）；
    进程一次契约：换游戏请刷新页面</div>
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
    _wl.textContent += '[' + m + '] ' + _fmt(a) + '\\n';
    const lines = _wl.textContent.split('\\n');
    if (lines.length > 200) _wl.textContent = lines.slice(-200).join('\\n');
    _wl.scrollTop = _wl.scrollHeight;
    orig(...a);
  };
}
window.addEventListener('error', e => {
  _wl.textContent += '[onerror] ' + (e.message || e.error) +
    (e.filename ? ' @ ' + e.filename + ':' + e.lineno : '') + '\\n';
});
window.addEventListener('unhandledrejection', e => {
  _wl.textContent += '[rejection] ' + e.reason + '\\n';
});

let exports = null;
const ensureInit = async () => {
  if (!exports) exports = await init();
  window.__pygame_rs_dev = exports;   // 自动化/控制台注入载荷口
  return exports;
};
import init from './python.js?v=__BUILD_TS__';

// 路径通道：?script= 存在 → init 即自动拉取运行（web_start 内分派）
const hasScript = new URLSearchParams(location.search).has('script');

document.getElementById('start-btn').addEventListener('click', async () => {
  await ensureInit();
  if (hasScript) {
    document.getElementById('start-overlay').remove();
  } else {
    console.log('未指定 ?script=——请用下方按钮选择本地 .py / bundle 文件');
  }
});

// 文件通道：选择本地单文件 → load_payload 注入运行
document.getElementById('pick-btn').addEventListener('click', () => {
  const input = document.createElement('input');
  input.type = 'file';
  input.accept = '.py';
  input.onchange = async () => {
    const f = input.files[0];
    if (!f) return;
    const ex = await ensureInit();
    const bytes = new Uint8Array(await f.arrayBuffer());
    document.getElementById('start-overlay').remove();
    ex.load_payload(f.name, bytes);
  };
  input.click();
});
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
    print(f"  把 bundle.py 放入 {WEB} 并命名 main.py（或 ?script=<名> 指定）；")
    print("  页面亦可选本地 .py / bundle 单文件直接运行（load_payload）")
    print("=== 测试 ===")
    print(f"  cd {WEB} 后起静态服务，浏览器打开 index.html（file:// 无效）")


if __name__ == "__main__":
    main()
