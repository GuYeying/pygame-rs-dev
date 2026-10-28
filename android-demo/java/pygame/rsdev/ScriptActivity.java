package pygame.rsdev;

import android.app.Activity;
import android.os.Bundle;
import android.view.MotionEvent;
import android.view.Surface;
import android.view.SurfaceHolder;
import android.view.SurfaceView;
import android.view.View;
import android.view.Window;
import android.view.WindowManager;
import android.widget.FrameLayout;
import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;

/** 脚本运行页：SurfaceView 铺满整屏挂引擎（**零 letterbox 壳**）。
 *
 *  呈现定约（2026-10-28 定案）：壳只提供"满屏 surface + 原始触摸"，
 *  呈现策略全在引擎（starfish-pygame letterbox）——
 *  - 引擎表面初始 = 外部窗口实测尺寸（壳 attach 时的窗口）；
 *  - 脚本 set_mode(W, H)：
 *    · FULLSCREEN 旗标 → 画布 = 窗口实测尺寸（1:1 满屏零变形）；
 *    · 非全屏 → 画布 = (W, H) 语义尺寸，引擎等比缩放居中（letterbox）；
 *  - 触摸原始窗口像素直通，引擎侧逆映射回脚本坐标系（map_point）。
 *
 *  Kotlin 层职责（与未来导出壳同构，零呈现逻辑）：
 *  surface 生命周期三态转发（attach/resize/detach + 代数守卫 + stop
 *  补发）、原始触摸转发（MotionEvent 编码换算）、诊断/取证（SdLog/
 *  崩溃遗嘱/dev_log 落点）、接线（setBaseDir/setLogDir/initContext/
 *  loadCode/stop）。
 */
public class ScriptActivity extends Activity implements SurfaceHolder.Callback {
    private boolean loadSent = false;
    private String scriptName = "main.py";

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        requestWindowFeature(Window.FEATURE_NO_TITLE);
        getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
        // 全屏铺满：cutout 区域也纳入（默认 view 避开挖孔），导航/状态
        // 栏沉浸隐藏
        if (android.os.Build.VERSION.SDK_INT >= 28) {
            getWindow().getAttributes().layoutInDisplayCutoutMode =
                android.view.WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES;
        }
        View decor = getWindow().getDecorView();
        decor.setSystemUiVisibility(View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
                | View.SYSTEM_UI_FLAG_LAYOUT_STABLE
                | View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
                | View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
                | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN
                | View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
                | View.SYSTEM_UI_FLAG_FULLSCREEN);

        scriptName = getIntent().getStringExtra("script");
        if (scriptName == null) scriptName = "main.py";
        SdLog.log("prescan 已废弃（呈现归引擎 letterbox）script=" + scriptName);

        FrameLayout root = new FrameLayout(this);
        root.setBackgroundColor(0xFF000000);
        SurfaceView sv = new SurfaceView(this);
        sv.getHolder().addCallback(this);
        // 不钉 buffer：跟随 view 满屏；脚本 set_mode 的尺寸语义由引擎
        // 呈现映射承载（FULLSCREEN = 画布即屏幕；非全屏 = 等比居中）
        root.addView(sv, new FrameLayout.LayoutParams(
                FrameLayout.LayoutParams.MATCH_PARENT,
                FrameLayout.LayoutParams.MATCH_PARENT));
        setContentView(root);

        prepareFiles();
        PygameRsDev.setBaseDir(getFilesDir().getAbsolutePath());
        // 引擎面包屑落点：优先 /sdcard/Download（用户可见），探测可写
        // 失败回退应用私有目录——与 SdLog 同款判定
        File logDir;
        File dl = new File("/sdcard/Download");
        try {
            dl.mkdirs();
            File probe = new File(dl, ".probe");
            FileOutputStream t = new FileOutputStream(probe);
            t.close();
            probe.delete();
            logDir = dl;
        } catch (IOException e) {
            logDir = getExternalFilesDir(null);
            if (logDir == null) logDir = getFilesDir();
        }
        PygameRsDev.setLogDir(logDir.getAbsolutePath());
        SdLog.log("setLogDir=" + logDir.getAbsolutePath()
                + " ScriptActivity.onCreate " + scriptName);
    }

    private FrameLayout root;

    @Override
    public void surfaceCreated(SurfaceHolder holder) {
        SdLog.log("surfaceCreated → attach（gen " + (activeGen + 1) + "）");
        myGen = ++activeGen;
        PygameRsDev.attachSurface(holder.getSurface());
        loadOnce();
    }

    @Override
    public void surfaceChanged(SurfaceHolder holder, int format, int width, int height) {
        SdLog.log("surfaceChanged " + width + "x" + height);
        PygameRsDev.surfaceResized(width, height);
    }

    @Override
    public void surfaceDestroyed(SurfaceHolder holder) {
        if (myGen != activeGen) {
            SdLog.log("surfaceDestroyed 陈旧（gen " + myGen + " < "
                    + activeGen + "）——忽略，不碰新场窗口");
            return;
        }
        // 可靠收口：surface 销毁 = 本场运行必终。ANCO 的返回手势不触发
        // onBackPressed（真机实证：无 stop 标志时引擎不收口 → 音频泄漏
        // /换场异常）——这里补发 stop，detach 的 Close 双保险
        SdLog.log("surfaceDestroyed → stop + detach");
        PygameRsDev.stop();
        PygameRsDev.detachSurface(holder.getSurface());
    }

    // ── 共用：attach 后的一次性加载 ─────────────────────────────
    private void loadOnce() {
        if (loadSent) return;
        loadSent = true;
        String code = readAsset("scripts/" + scriptName);
        if (code != null) {
            SdLog.log("loadCode " + scriptName + "（" + code.length() + " B）");
            PygameRsDev.loadCode(code);
        } else {
            SdLog.log("loadCode 失败：asset 缺失 " + scriptName);
        }
    }

    // ── 输入（原始窗口像素直通，引擎 map_point 逆映射）────────────

    @Override
    public boolean onTouchEvent(MotionEvent e) {
        final int a = e.getActionMasked();
        int action;
        if (a == MotionEvent.ACTION_DOWN) action = 0;
        else if (a == MotionEvent.ACTION_MOVE) action = 1;
        else if (a == MotionEvent.ACTION_UP || a == MotionEvent.ACTION_CANCEL) action = 2;
        else return false;
        // 原始窗口像素直通：引擎 map_point 逆映射回脚本坐标系（呈现
        // 映射的逆，FULLSCREEN/非全屏同一通道）
        float x = e.getX(), y = e.getY();
        if (action == 0) SdLog.log("touch down " + x + "," + y);
        PygameRsDev.injectTouch(action, x, y);
        return true;
    }

    @Override
    public void onBackPressed() {
        SdLog.log("onBackPressed → stop");
        PygameRsDev.stop();
        finish();
    }

    /** attach 代数（进程级）：销毁回调只在本场仍是当前场时生效——
     *  换场重叠期旧场销毁晚到，会连坐新场窗口 */
    private static int activeGen = 0;
    private int myGen = 0;

    /** assets/scripts/*.py → filesDir/scripts/（io 基准内的就近副本；
     *  assets/modules/* → filesDir/modules/（多文件工程的模块——脚本
     *  import modules.xxx；嵌套子目录随包保留）；字体 assets/fonts/cjk.ttf
     *  存在才拷——缺省走系统 Noto 兜底） */
    private void prepareFiles() {
        try {
            File dir = new File(getFilesDir(), "scripts");
            dir.mkdirs();
            for (String n : getAssets().list("scripts")) {
                copyAsset("scripts/" + n, new File(dir, n));
            }
            File mdir = new File(getFilesDir(), "modules");
            mdir.mkdirs();
            String[] mods = getAssets().list("modules");
            if (mods != null) {
                for (String n : mods) {
                    // 子包目录跳过（v1 平铺约定——嵌套目录非资产文件，
                    // copyAsset 会抛 IO 异常连带跳过后续拷贝）
                    String[] nested = getAssets().list("modules/" + n);
                    if (nested != null && nested.length > 0) continue;
                    copyAsset("modules/" + n, new File(mdir, n));
                }
            }
            String[] fonts = getAssets().list("fonts");
            if (fonts != null && fonts.length > 0) {
                File fdir = new File(getFilesDir(), "fonts");
                fdir.mkdirs();
                copyAsset("fonts/cjk.ttf", new File(fdir, "cjk.ttf"));
            }
        } catch (IOException ignored) {
        }
    }

    private void copyAsset(String assetPath, File dst) throws IOException {
        InputStream in = null;
        FileOutputStream out = null;
        try {
            in = getAssets().open(assetPath);
            out = new FileOutputStream(dst);
            byte[] buf = new byte[16384];
            int n;
            while ((n = in.read(buf)) > 0) out.write(buf, 0, n);
        } finally {
            if (in != null) try { in.close(); } catch (IOException ignored) {}
            if (out != null) try { out.close(); } catch (IOException ignored) {}
        }
    }

    private String readAsset(String assetPath) {
        try {
            InputStream in = getAssets().open(assetPath);
            ByteArrayOutputStream bos = new ByteArrayOutputStream();
            byte[] buf = new byte[16384];
            int n;
            while ((n = in.read(buf)) > 0) bos.write(buf, 0, n);
            in.close();
            return new String(bos.toByteArray(), "UTF-8");
        } catch (IOException e) {
            return null;
        }
    }
}
