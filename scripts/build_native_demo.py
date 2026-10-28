#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""pygame-rs-dev 原生 UI demo app 组装（嵌入式 .so 交付形态的参考宿主）。

链路（build_apk.py 的 Java app 变体——无 NativeActivity，自有 Java UI）：
  1. 收集全量示例脚本 → android-demo/assets/scripts/（.py 全量打进 APK）
  2. cargo ndk 交叉编译 android_host .so（gui/physics2d/physics3d 已在
     pygame-rs-dev 依赖里硬编码，无额外 feature）
  3. javac（-release 11，classpath android.jar）
  4. d8 → classes.dex
  5. aapt2 link（-A assets 打进资源表）
  6. 组装 base + classes.dex + .so（.so 恒 DEFLATED、条目恒正斜杠）
  7. zipalign 4 + apksigner（debug.keystore）

用法：
  python scripts/build_native_demo.py [--skip-build] [--install]
产物：target/apk/pygame_rsdev_demo.apk（包名 pygame.rsdev，与
      com.starfish.test 独立运行形态并存）
"""

import argparse
import shutil
import subprocess
import sys
import zipfile
from pathlib import Path

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

CRATE = Path(__file__).resolve().parent.parent
DEMO = CRATE / "android-demo"
ASSETS = DEMO / "assets"
ABI = "arm64-v8a"
OUT_DIR = CRATE / "target" / "apk"
WORK = CRATE / "target" / "apk_demo_build"
FINAL_NAME = "pygame_rsdev_demo.apk"
PKG = "pygame.rsdev"
ACTIVITY = f"{PKG}/.MainActivity"
# 全量脚本源（pygame-rs 家族两个 examples 目录；递归 games/ 形态暂不收）
SCRIPT_SOURCES = [
    Path("D:/Projects/Rust/pygame-rs/examples"),
    Path("D:/Projects/Rust/pygame-rs-dev/examples"),
]


def sh(cmd, cwd=None, tail=0):
    print("+", " ".join(str(c) for c in cmd))
    r = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, errors="replace")
    if r.returncode != 0:
        print(r.stdout[-3000:])
        print(r.stderr[-3000:])
        sys.exit(f"[FAIL] 命令失败: {cmd[0]} …")
    out = (r.stdout + r.stderr).strip()
    if tail:
        print("\n".join(out.splitlines()[-tail:]))
    return out


def best_subdir(base: Path, prefix: str) -> Path:
    cands = [p for p in base.glob(f"{prefix}*") if p.is_dir()] if base.is_dir() else []
    if not cands:
        sys.exit(f"[FAIL] 未找到 {base}/{prefix}*（检查 SDK 安装）")
    return max(cands, key=lambda p: [int(x) if x.isdigit() else 0 for x in p.name.split(".")])


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--skip-build", action="store_true", help="复用现有 jniLibs .so（调试迭代用）")
    ap.add_argument("--install", action="store_true", help="构建后 adb install -r 并启动")
    args = ap.parse_args()

    sdk = Path("D:/SDK/Android")
    ndk = best_subdir(sdk / "ndk", "")
    bt = best_subdir(sdk / "build-tools", "")
    jar = best_subdir(sdk / "platforms", "android-") / "android.jar"
    ndk_bin = ndk / "toolchains" / "llvm" / "prebuilt" / "windows-x86_64" / "bin"

    # 1) 全量脚本收集（覆盖式——与仓库 examples 保持一致）
    print("=== 1/7 收集全量脚本 → assets/scripts ===")
    scripts_dir = ASSETS / "scripts"
    if scripts_dir.is_dir():
        for p in scripts_dir.glob("*.py"):
            p.unlink()
    scripts_dir.mkdir(parents=True, exist_ok=True)
    count = 0
    for src in SCRIPT_SOURCES:
        for py in sorted(src.glob("*.py")):
            (scripts_dir / py.name).write_bytes(py.read_bytes())
            count += 1
    print(f"[OK] 打包脚本 {count} 个")
    # 1b) 模块文件夹收集（examples/modules/ → assets/modules/，递归保留
    # 子包结构）：多文件工程约定（与桌面 mod_demo 同构）——脚本侧
    # `import modules.xxx`；APK 侧由 ScriptActivity.prepareFiles 落盘
    # io 基准目录，模块表装载后自然路径可达
    mods_dir = ASSETS / "modules"
    if mods_dir.is_dir():
        shutil.rmtree(mods_dir)
    mods_dir.mkdir(parents=True, exist_ok=True)
    mcount = 0
    for src in SCRIPT_SOURCES:
        msrc = src / "modules"
        if msrc.is_dir():
            for py in sorted(msrc.rglob("*.py")):
                rel = py.relative_to(msrc)
                dst = mods_dir / rel
                dst.parent.mkdir(parents=True, exist_ok=True)
                dst.write_bytes(py.read_bytes())
                mcount += 1
    print(f"[OK] 打包模块 {mcount} 个")

    # 2) 交叉编译 .so（release 铁律）
    so_name = "libandroid_host.so"
    jni_libs = CRATE / "jniLibs"
    so = jni_libs / ABI / so_name
    if not args.skip_build:
        print(f"=== 2/7 cargo ndk build ({ABI}) ===")
        sh(["cargo", "ndk", "-t", ABI, "-P", "26", "-o", str(jni_libs),
            "--", "build", "--release", "--example", "android_host"],
           cwd=CRATE, tail=1)
    if not so.is_file():
        sys.exit(f"[FAIL] .so 缺失: {so}")
    sh([str(ndk_bin / "llvm-strip"), str(so)])
    nm = sh([str(ndk_bin / "llvm-nm"), "-D", str(so)])
    for sym in ("Java_pygame_rsdev_PygameRsDev_loadCode",
                "Java_pygame_rsdev_PygameRsDev_attachSurface",
                "Java_pygame_rsdev_PygameRsDev_injectTouch"):
        if f"T {sym}" not in nm:
            sys.exit(f"[FAIL] JNI 符号未导出: {sym}")
    print("[nm] JNI 符号面完整 ✓")

    # 3) javac
    print("=== 3/7 javac ===")
    obj = WORK / "obj"
    if obj.is_dir():
        shutil.rmtree(obj)
    obj.mkdir(parents=True)
    java_srcs = list((DEMO / "java").rglob("*.java"))
    sh(["javac", "--release", "11", "-classpath", str(jar), "-d", str(obj),
        *[str(s) for s in java_srcs]])

    # 4) d8 → classes.dex
    print("=== 4/7 d8 ===")
    dex_out = WORK / "dex"
    if dex_out.is_dir():
        shutil.rmtree(dex_out)
    dex_out.mkdir(parents=True)
    class_files = [str(p) for p in obj.rglob("*.class")]
    sh([str(bt / "d8.bat"), "--release", "--lib", str(jar), "--min-api", "26",
        "--output", str(dex_out), *class_files])
    dex = dex_out / "classes.dex"
    if not dex.is_file():
        sys.exit("[FAIL] classes.dex 未产出")

    # 5) aapt2 link（-A assets 打进资源表）
    print("=== 5/7 aapt2 link ===")
    WORK.mkdir(parents=True, exist_ok=True)
    base_apk = WORK / "base.apk"
    sh([str(bt / "aapt2.exe"), "link", "--manifest", str(DEMO / "AndroidManifest.xml"),
        "-I", str(jar), "-A", str(ASSETS), "-o", str(base_apk)])

    # 6) 组装（dex + so；DEFLATED + 正斜杠条目）
    print("=== 6/7 组装 ===")
    final = OUT_DIR / FINAL_NAME
    tmp_full = WORK / "full.apk"
    with zipfile.ZipFile(base_apk) as zb, zipfile.ZipFile(tmp_full, "w") as zout:
        for item in zb.infolist():
            zout.writestr(item, zb.read(item.filename))
        zout.writestr("classes.dex", dex.read_bytes(), compress_type=zipfile.ZIP_DEFLATED)
        zout.writestr(f"lib/{ABI}/{so_name}", so.read_bytes(),
                      compress_type=zipfile.ZIP_DEFLATED)

    # 7) zipalign + 签名 + 验证
    print("=== 7/7 zipalign + apksigner ===")
    aligned = WORK / "aligned.apk"
    sh([str(bt / "zipalign.exe"), "-f", "4", str(tmp_full), str(aligned)])
    ks = Path.home() / ".android" / "debug.keystore"
    if not ks.is_file():
        sys.exit(f"[FAIL] 未找到签名 keystore: {ks}")
    sh([str(bt / "apksigner.bat"), "sign", "--ks", str(ks), "--ks-pass", "pass:android",
        "--out", str(final), str(aligned)])
    sh([str(bt / "apksigner.bat"), "verify", str(final)])
    tmp_full.unlink(missing_ok=True)
    aligned.unlink(missing_ok=True)
    print(f"[OK] {final}  ({final.stat().st_size // 1024 // 1024} MB；脚本 {count} 个)")

    if args.install:
        adb = "D:/android-tools/platform-tools/adb.exe"
        sh([adb, "install", "-r", str(final)], tail=1)
        sh([adb, "shell", "am", "start", "-n", f"{PKG}/.MainActivity"], tail=1)


if __name__ == "__main__":
    main()
