#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""pygame-rs-dev 独立运行形态 APK 组装（android_host 专用五步链）。

与 pygame-rs/scripts/build_apk.py 同链（批次十八/十九真机验证过的
五步），差异仅两点：
  - 示例名无 _android 后缀（本仓单一 example：android_host）；
  - classes.dex 无本地产物——兜底取 pygame-rs 历史 APK（dex 是
    robius 胶水，与示例无关可复用）。

用法：
  python scripts/build_apk.py [--skip-build]   # --skip-build 复用现有 .so

产物：target/apk/android_host.apk（debug 签名）
"""

import argparse
import subprocess
import sys
import zipfile
from pathlib import Path

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

CRATE = Path(__file__).resolve().parent.parent
EXAMPLE = "android_host"
ABI = "arm64-v8a"
TRIPLE = "aarch64-linux-android"
LIB_NAME = EXAMPLE                     # 本仓 lib 名无 _android 后缀
OUT_DIR = CRATE / "target" / "apk"
WORK = CRATE / "target" / "apk_build"
# dex 兜底：dex 是与示例无关的 robius 胶水，历史 APK 里捞
DEX_FALLBACK_APKS = [Path("D:/Projects/Rust/pygame-rs/target/android-apk")]


def sh(cmd, tail=0):
    print("+", " ".join(str(c) for c in cmd))
    r = subprocess.run(cmd, capture_output=True, text=True, errors="replace")
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
    args = ap.parse_args()

    sdk = Path("D:/SDK/Android")
    ndk = best_subdir(sdk / "ndk", "")
    bt = best_subdir(sdk / "build-tools", "")
    jar = best_subdir(sdk / "platforms", "android-") / "android.jar"
    ndk_bin = ndk / "toolchains" / "llvm" / "prebuilt" / "windows-x86_64" / "bin"

    so_name = f"lib{LIB_NAME}.so"
    jni_libs = CRATE / "jniLibs"
    so = jni_libs / ABI / so_name

    # 1) 交叉编译（release 铁律：RP debug genesis 必败）
    if not args.skip_build:
        print(f"=== 1/5 cargo ndk build ({ABI}) ===")
        sh(["cargo", "ndk", "-t", ABI, "-P", "26", "-o", str(jni_libs),
            "--", "build", "--release", "--example", EXAMPLE], cwd=CRATE, tail=1)
    if not so.is_file():
        sys.exit(f"[FAIL] .so 缺失: {so}")

    # 2) strip + android_main 导出自检
    print("=== 2/5 llvm-strip ===")
    sh([str(ndk_bin / "llvm-strip"), str(so)])
    nm = sh([str(ndk_bin / "llvm-nm"), "-D", str(so)])
    if "T android_main" not in nm:
        sys.exit("[FAIL] android_main 未导出（NativeActivity 将无法启动）")
    print("[nm] android_main 已导出 ✓")

    # 3) manifest 模板替换 + aapt2 link（lib_name 对齐）
    print("=== 3/5 aapt2 link ===")
    WORK.mkdir(parents=True, exist_ok=True)
    mtext = (CRATE / "android" / "AndroidManifest.xml").read_text(encoding="utf-8")
    m = WORK / "AndroidManifest.xml"
    m.write_text(mtext.replace("__LIB_NAME__", LIB_NAME), encoding="utf-8")
    base_apk = WORK / "base.apk"
    res_dir = CRATE / "android" / "res"
    link_args = [str(bt / "aapt2.exe"), "link", "--manifest", str(m), "-I", str(jar),
                 "-o", str(base_apk)]
    if res_dir.is_dir():
        res_zip = WORK / "res.zip"
        sh([str(bt / "aapt2.exe"), "compile", "--dir", str(res_dir), "-o", str(res_zip)])
        link_args.append(str(res_zip))
    sh(link_args)

    # 4) 组装（dex + so；.so 恒 DEFLATED；zip 条目恒正斜杠——PowerShell
    #    反斜杠坑的根治，安卓解析器只认 /）
    print("=== 4/5 组装 ===")
    dex = None
    for root in DEX_FALLBACK_APKS:
        for apk in sorted(root.glob("*.apk")):
            try:
                with zipfile.ZipFile(apk) as z:
                    if "classes.dex" in z.namelist():
                        dex = z.read("classes.dex")
                        print(f"[dex] 取自历史 APK: {apk.name}")
                        break
            except zipfile.BadZipFile:
                continue
        if dex:
            break
    if not dex:
        sys.exit("[FAIL] 未找到 classes.dex（先在 pygame-rs 打一个含 dialog 的 APK）")
    final = OUT_DIR / f"{EXAMPLE}.apk"
    tmp_full = WORK / "full.apk"
    with zipfile.ZipFile(base_apk) as zb, zipfile.ZipFile(tmp_full, "w") as zout:
        for item in zb.infolist():
            zout.writestr(item, zb.read(item.filename))
        zout.writestr("classes.dex", dex, compress_type=zipfile.ZIP_DEFLATED)
        zout.writestr(f"lib/{ABI}/{so_name}", so.read_bytes(),
                      compress_type=zipfile.ZIP_DEFLATED)

    # 5) zipalign + 签名 + 验证 + lib_name 自检
    print("=== 5/5 zipalign + apksigner ===")
    aligned = WORK / "aligned.apk"
    sh([str(bt / "zipalign.exe"), "-f", "4", str(tmp_full), str(aligned)])
    ks = Path.home() / ".android" / "debug.keystore"
    if not ks.is_file():
        sys.exit(f"[FAIL] 未找到签名 keystore: {ks}")
    sh([str(bt / "apksigner.bat"), "sign", "--ks", str(ks), "--ks-pass", "pass:android",
        "--out", str(final), str(aligned)])
    sh([str(bt / "apksigner.bat"), "verify", str(final)])
    badging = subprocess.run([str(bt / "aapt2.exe"), "dump", "xmltree", "--file",
                              "AndroidManifest.xml", str(final)],
                             capture_output=True, text=True, errors="replace")
    ok = f'"{LIB_NAME}"' in badging.stdout
    print(f"[自检] lib_name={LIB_NAME} {'✓' if ok else '✗ 不匹配！'}")
    if not ok:
        sys.exit("[FAIL] lib_name 自检失败——禁止分发")
    tmp_full.unlink(missing_ok=True)
    aligned.unlink(missing_ok=True)
    print(f"[OK] {final}  ({final.stat().st_size // 1024 // 1024} MB)")


if __name__ == "__main__":
    main()
