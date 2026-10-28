package pygame.rsdev;

import android.app.Activity;
import android.graphics.SurfaceTexture;
import android.os.Bundle;
import android.view.Gravity;
import android.view.MotionEvent;
import android.view.Surface;
import android.view.SurfaceHolder;
import android.view.SurfaceView;
import android.view.TextureView;
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

/** 脚本运行页：surface 挂引擎（**宿主布局 letterbox / 拉伸全屏**）。
 *
 *  尺寸定约（demo 层自决，引擎零改动）：
 *  - 非全屏：SurfaceView，buffer 钉逻辑尺寸，整数倍放大居中——
 *    view/buffer/引擎表面三方 1:1（get_size/egui/触摸天然一致）；
 *  - FULLSCREEN 旗标：**TextureView 铺满整屏**——buffer 跟随 view
 *    （满屏），引擎表面初始 = 外部窗口实测尺寸（external_size），
 *    脚本 set_mode 后 GLES 缩放铺满（既定满屏语义）。选 TextureView
 *    而非 SurfaceView：ANCO 兼容层对 SurfaceView 的合成定位不受应用
 *    控制（画布偏移/溢出，真机截图实证）；TextureView = 普通 view
 *    纹理，位置缩放全由应用掌控。
 *
 *  接线次序（契约：先 attach 后 load——引擎建窗时窗口必须已就位）：
 *  onCreate → 预扫描定布局 + setBaseDir/setLogDir
 *  surface 就绪 → attachSurface（一次）→ loadCode
 *  尺寸变化 → surfaceResized；销毁 → stop() + detachSurface
 *  onTouchEvent → injectTouch（MotionEvent 编码 → 引擎编码换算）
 *  返回键 → stop()（帧边界收口）→ 返回选择页
 */
public class ScriptActivity extends Activity
        implements SurfaceHolder.Callback, TextureView.SurfaceTextureListener {
    private static final Pattern SET_MODE =
            Pattern.compile("set_mode\\s*\\(\\s*\\(\\s*(\\d+)\\s*,\\s*(\\d+)");
    /** 全屏判定 = FULLSCREEN 作为 set_mode 的实参（同行匹配；注释行
     *  跳过——probe 头注释含完整 set_mode 字面，曾把注释当调用） */
    private static final Pattern SET_MODE_FULLSCREEN =
            Pattern.compile("set_mode\\s*\\(.*FULLSCREEN");
    private boolean loadSent = false;
    private String scriptName = "main.py";
    private int logicalW = 800, logicalH = 600;
    private boolean fullscreen = false;
    private int viewScale = 1;
    private FrameLayout root;
    private SurfaceView surfaceView;
    private volatile Surface textureSurface;
    /** attach 代数（进程级）：销毁回调只在本场仍是当前场时生效——
     *  换场重叠期旧场销毁晚到，会连坐新场窗口 */
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
        if (fullscreen) {
            // 全屏：TextureView 铺满整屏（buffer 跟随 view），引擎表面
            // 初始 = 外部窗口实测尺寸（external_size），脚本 set_mode 后
            // GLES 缩放铺满——位置缩放全由应用掌控，绕开 ANCO 对
            // SurfaceView 的合成定位怪癖（画布偏移/溢出，真机截图实证）
            TextureView tv = new TextureView(this);
            tv.setOpaque(true);
            tv.setSurfaceTextureListener(this);
            root.addView(tv, new FrameLayout.LayoutParams(
                    FrameLayout.LayoutParams.MATCH_PARENT,
                    FrameLayout.LayoutParams.MATCH_PARENT));
        } else {
            SurfaceView sv = new SurfaceView(this);
            surfaceView = sv;
            sv.getHolder().addCallback(this);
            // buffer 钉死脚本逻辑尺寸 + 整数倍放大居中：view/buffer/
            // 引擎表面三方 1:1（get_size/egui/触摸天然一致）
            sv.getHolder().setFixedSize(logicalW, logicalH);
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

    // ── SurfaceView 路径（非全屏）────────────────────────────────
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
        // onBackPressed（真机实证：无 stop 标志时引擎不收口 → 音频泄漏）
        SdLog.log("surfaceDestroyed → stop + detach");
        PygameRsDev.stop();
        PygameRsDev.detachSurface(holder.getSurface());
    }

    // ── TextureView 路径（全屏）──────────────────────────────────
    @Override
    public void onSurfaceTextureAvailable(SurfaceTexture st, int width, int height) {
        SdLog.log("textureAvailable " + width + "x" + height
                + "（gen " + (activeGen + 1) + "）");
        myGen = ++activeGen;
        textureSurface = new Surface(st);
        PygameRsDev.attachSurface(textureSurface);
        loadOnce();
    }

    @Override
    public void onSurfaceTextureSizeChanged(SurfaceTexture st, int width, int height) {
        SdLog.log("textureSizeChanged " + width + "x" + height);
        PygameRsDev.surfaceResized(width, height);
    }

    @Override
    public boolean onSurfaceTextureDestroyed(SurfaceTexture st) {
        if (myGen == activeGen) {
            SdLog.log("textureDestroyed → stop + detach");
            PygameRsDev.stop();
            PygameRsDev.detachSurface(textureSurface);
        } else {
            SdLog.log("textureDestroyed 陈旧——忽略");
        }
        if (textureSurface != null) {
            textureSurface.release();
            textureSurface = null;
        }
        return true; // 允许释放 SurfaceTexture（detach 已对消引用）
    }

    @Override
    public void onSurfaceTextureUpdated(SurfaceTexture st) {
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

    @Override
    public boolean onTouchEvent(MotionEvent e) {
        final int a = e.getActionMasked();
        int action;
        if (a == MotionEvent.ACTION_DOWN) action = 0;
        else if (a == MotionEvent.ACTION_MOVE) action = 1;
        else if (a == MotionEvent.ACTION_UP || a == MotionEvent.ACTION_CANCEL) action = 2;
        else return false;
        // Activity 级坐标 = 全屏系——换算到 view 局部（脚本逻辑系）：
        // 全屏 = 按铺满比例反算；非全屏 = 减 view 原点 + 除放大倍数
        float x = e.getX(), y = e.getY();
        if (fullscreen && root != null && root.getWidth() > 0) {
            x = x * logicalW / root.getWidth();
            y = y * logicalH / root.getHeight();
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
