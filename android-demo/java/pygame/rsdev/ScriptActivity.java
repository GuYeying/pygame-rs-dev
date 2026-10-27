package pygame.rsdev;

import android.app.Activity;
import android.os.Bundle;
import android.view.Gravity;
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
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/** 脚本运行页：SurfaceView 挂引擎（**宿主布局 letterbox / 拉伸全屏**）。
 *
 *  尺寸定约（demo 层自决，引擎零改动）：onCreate 读脚本文本正则预扫描
 *  set_mode 尺寸 → `holder.setFixedSize(w, h)` 钉死 buffer = 脚本逻辑
 *  尺寸（get_size/egui/触摸三方天然 1:1，无需任何映射）：
 *  - 非全屏：view = w×h 黑底居中（布局 letterbox）；
 *  - FULLSCREEN 旗标：view 铺满整屏 + pivot(0,0) 矩阵缩放把 w×h 的
 *    view 拉伸到满屏（非等比，buffer 整体即画布，无锚点歧义），
 *    触摸按缩放比反算回逻辑坐标。
 *  动态尺寸扫描失败回退 800×600。
 *
 *  接线次序（契约：先 attach 后 load——引擎建窗时窗口必须已就位）：
 *  onCreate → 预扫描定布局 + setFixedSize + setBaseDir/setLogDir
 *  surfaceCreated → attachSurface（一次）→ loadCode
 *  surfaceChanged → surfaceResized（恒报固定逻辑尺寸）
 *  surfaceDestroyed → detachSurface（比对清除）
 *  onTouchEvent → injectTouch（MotionEvent 编码 → 引擎编码换算）
 *  返回键 → stop()（帧边界收口）→ 返回选择页
 */
public class ScriptActivity extends Activity implements SurfaceHolder.Callback {
    private static final Pattern SET_MODE =
            Pattern.compile("set_mode\\s*\\(\\s*\\(\\s*(\\d+)\\s*,\\s*(\\d+)");
    /** 全屏判定 = FULLSCREEN 作为 set_mode 的实参（同行匹配——元组
     *  右括号会截断 [^)]*，[^)] 版曾把元组形态的调用漏判）。注释提及
     *  全屏字样但未在 set_mode 行内使用 = 不命中 */
    private static final Pattern SET_MODE_FULLSCREEN =
            Pattern.compile("set_mode\\s*\\(.*FULLSCREEN");
    private boolean loadSent = false;
    private String scriptName = "main.py";
    private int logicalW = 800, logicalH = 600;
    private boolean fullscreen = false;
    private int viewScale = 1;
    private FrameLayout root;
    private SurfaceView surfaceView;
    /** attach 代数（进程级）：surfaceDestroyed 只在本场仍是当前场时
     *  detach——换场重叠期旧场销毁晚到，陈旧 detach 会把新场窗口清掉
     *  （引擎永不收口/黑屏；Rust 侧指针比对在 ANCO 不可靠，守卫前置） */
    private static int activeGen = 0;
    private int myGen = 0;

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
                | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN
                | View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
                | View.SYSTEM_UI_FLAG_FULLSCREEN);

        scriptName = getIntent().getStringExtra("script");
        if (scriptName == null) scriptName = "main.py";
        String code = readAsset("scripts/" + scriptName);
        int[] geo = prescanGeometry(code);
        logicalW = geo[0];
        logicalH = geo[1];
        fullscreen = code != null && SET_MODE_FULLSCREEN.matcher(code).find();
        SdLog.log("prescan " + scriptName + " → " + logicalW + "x" + logicalH
                + " fullscreen=" + fullscreen);

        root = new FrameLayout(this);
        root.setBackgroundColor(0xFF000000);
        SurfaceView sv = new SurfaceView(this);
        surfaceView = sv;
        sv.getHolder().addCallback(this);
        // buffer 钉死脚本逻辑尺寸（与 view 缩放/布局解耦——引擎表面、
        // buffer、脚本坐标系三方恒 1:1）
        sv.getHolder().setFixedSize(logicalW, logicalH);
        // view 自适应放大：逻辑尺寸的整数倍铺到屏幕上限（800×600 在
        // 高分屏上太小——用户可感知的"egui/画面放大"即来自此；整数倍
        // = 像素干净，触摸/view/buffer 仍 1:1）。倍率在布局完成后按
        // root 实测尺寸计算（dm 在部分兼容层横屏下谎报竖屏值）
        if (fullscreen) {
            // 全屏：view 铺满整屏，buffer 在 surfaceCreated 时钉为屏幕
            // 实测尺寸——view/buffer/引擎表面三方同尺寸，纯 1:1 满屏
            root.addView(sv, new FrameLayout.LayoutParams(
                    FrameLayout.LayoutParams.MATCH_PARENT,
                    FrameLayout.LayoutParams.MATCH_PARENT));
        } else {
            root.addView(sv, new FrameLayout.LayoutParams(
                    logicalW, logicalH, Gravity.CENTER));
            final SurfaceView fsv = sv;
            sv.post(new Runnable() {
                @Override
                public void run() {
                    int scale = Math.max(1, Math.min(
                            root.getWidth() / logicalW,
                            root.getHeight() / logicalH));
                    if (scale != viewScale) {
                        viewScale = scale;
                        FrameLayout.LayoutParams lp =
                                (FrameLayout.LayoutParams) fsv.getLayoutParams();
                        lp.width = logicalW * scale;
                        lp.height = logicalH * scale;
                        fsv.setLayoutParams(lp);
                    }
                    SdLog.log("view scale " + scale + "x → "
                            + logicalW * scale + "x" + logicalH * scale
                            + "（root " + root.getWidth() + "x" + root.getHeight() + "）");
                }
            });
        }
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
                + " ScriptActivity.onCreate " + logicalW + "x" + logicalH);
    }

    /** 脚本文本预扫描：第一处 set_mode((w, h)) 的字面尺寸；无匹配 =
     *  800×600 兜底（动态尺寸场景，demo 层可接受）。 */
    /** 第一处有效（非注释行）set_mode 匹配：'#' 出现在匹配位置之前
     *  （同行）= 注释行跳过——probe 头注释含完整 set_mode 字面，曾把
     *  注释里的数字当调用改写，真实调用纹丝不动 → 画布左上角对齐 */
    private Matcher setModeMatch(String code) {
        Matcher m = SET_MODE.matcher(code);
        while (m.find()) {
            int lineStart = code.lastIndexOf('\n', m.start()) + 1;
            if (!code.substring(lineStart, m.start()).contains("#")) {
                return m;
            }
        }
        return null;
    }

    private int[] prescanGeometry(String code) {
        int w = 800, h = 600;
        if (code != null) {
            Matcher m = setModeMatch(code);
            if (m != null) {
                try {
                    w = Integer.parseInt(m.group(1));
                    h = Integer.parseInt(m.group(2));
                } catch (NumberFormatException ignored) {
                }
            }
        }
        return new int[]{w, h};
    }

    @Override
    public void surfaceCreated(SurfaceHolder holder) {
        // 全屏 = 画布即屏幕：按 root 实测尺寸改写脚本 set_mode 的字面
        // 尺寸（正道前奏——将来 std 屏幕尺寸接口落地后脚本自查自适应，
        // 本壳层先行代劳），buffer 同步钉为屏幕尺寸——全程 1:1 满屏、
        // 零变形、零映射（egui 布局/命中/触摸三方同系）
        int sw = 800, sh = 600;
        if (fullscreen) {
            sw = Math.max(1, root.getWidth());
            sh = Math.max(1, root.getHeight());
            logicalW = sw;
            logicalH = sh;
            holder.setFixedSize(sw, sh);
        }
        SdLog.log("surfaceCreated → attach（gen " + (activeGen + 1) + "） surface="
                + logicalW + "x" + logicalH);
        myGen = ++activeGen;
        PygameRsDev.attachSurface(holder.getSurface());
        if (!loadSent) {
            loadSent = true;
            String code = readAsset("scripts/" + scriptName);
            if (code != null) {
                if (fullscreen) {
                    code = patchSetModeSize(code, sw, sh);
                }
                SdLog.log("loadCode " + scriptName + "（" + code.length() + " B）");
                PygameRsDev.loadCode(code);
            } else {
                SdLog.log("loadCode 失败：asset 缺失 " + scriptName);
            }
        }
    }

    /** 全屏改写：第一处 set_mode 的字面宽高 → 屏幕实测尺寸（画布即
     *  屏幕；未找到字面尺寸则原样返回，脚本侧自适应属后续接口职责） */
    private String patchSetModeSize(String code, int w, int h) {
        if (code == null) return code;
        Matcher m = setModeMatch(code);
        if (m != null) {
            String patched = code.substring(0, m.start(1)) + w + ", " + h
                    + code.substring(m.end(2));
            SdLog.log("set_mode 尺寸改写 → " + w + "x" + h + "（画布即屏幕）");
            return patched;
        }
        SdLog.log("set_mode 无字面尺寸——不改写（画布 " + w + "x" + h + "）");
        return code;
    }

    @Override
    public void surfaceChanged(SurfaceHolder holder, int format, int width, int height) {
        // buffer 已 setFixedSize 钉死——恒报逻辑尺寸（全屏拉伸时 view
        // 尺寸 ≠ buffer，报 view 尺寸会让引擎表面错配）
        SdLog.log("surfaceChanged view=" + width + "x" + height
                + " → 引擎表面 " + logicalW + "x" + logicalH);
        PygameRsDev.surfaceResized(logicalW, logicalH);
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

    @Override
    public boolean onTouchEvent(MotionEvent e) {
        final int a = e.getActionMasked();
        int action;
        if (a == MotionEvent.ACTION_DOWN) action = 0;
        else if (a == MotionEvent.ACTION_MOVE) action = 1;
        else if (a == MotionEvent.ACTION_UP || a == MotionEvent.ACTION_CANCEL) action = 2;
        else return false;
        // Activity 级坐标 = 全屏系——换算到 view 局部（脚本逻辑系）：
        // 全屏等比 = 减居中原点 + 除缩放；非全屏 = 减 view 原点 + 除
        // view 放大倍数（漏算 = egui/触摸整体错位，只有左上角可交互）
        float x = e.getX(), y = e.getY();
        if (fullscreen && root != null && root.getWidth() > 0) {
            float s = Math.min(root.getWidth() / (float) logicalW,
                               root.getHeight() / (float) logicalH);
            float ox = (root.getWidth() - logicalW * s) / 2f;
            float oy = (root.getHeight() - logicalH * s) / 2f;
            x = (x - ox) / s;
            y = (y - oy) / s;
        } else if (surfaceView != null) {
            x = (x - surfaceView.getX()) / viewScale;
            y = (y - surfaceView.getY()) / viewScale;
        }
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

    /** assets/scripts/*.py → filesDir/scripts/（io 基准内的就近副本；
     *  字体 assets/fonts/cjk.ttf 存在才拷——缺省走系统 Noto 兜底） */
    private void prepareFiles() {
        try {
            File dir = new File(getFilesDir(), "scripts");
            dir.mkdirs();
            for (String n : getAssets().list("scripts")) {
                copyAsset("scripts/" + n, new File(dir, n));
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
