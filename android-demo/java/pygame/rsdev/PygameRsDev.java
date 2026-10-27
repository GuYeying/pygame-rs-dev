package pygame.rsdev;

import android.view.Surface;

/** pygame-rs-dev 嵌入式运行时 JNI 门面（符号对齐 examples/android_host.rs）。
 *
 *  进程一次契约：一轮 loadCode 只跑一个程序；切换 = stop() 后重进。
 */
public final class PygameRsDev {
    static { System.loadLibrary("android_host"); }

    private PygameRsDev() {}

    private static boolean contextBound = false;

    /** 绑定宿主 Context（ndk-context 初始化；cpal/robius 等 JNI 消费方
     *  依赖——嵌入形态必调一次；包装守卫防二次初始化断言） */
    public static void initContext(android.content.Context ctx) {
        if (contextBound) return;
        contextBound = true;
        nativeInitContext(ctx);
    }

    private static native void nativeInitContext(android.content.Context ctx);

    /** SurfaceView.surfaceCreated → 引擎抓取 ANativeWindow */
    public static native void attachSurface(Surface surface);

    /** SurfaceView.surfaceChanged → 位图尺寸同步 */
    public static native void surfaceResized(int width, int height);

    /** SurfaceView.surfaceDestroyed → 比对清除（传本次销毁的 Surface——
     *  与当前槽位不一致的陈旧 detach 被忽略，不连坐新场窗口） */
    public static native void detachSurface(Surface surface);

    /** io 基准目录（脚本相对路径 / fonts/cjk.ttf 就近解析） */
    public static native void setBaseDir(String path);

    /** 诊断落盘目录（loadCode 前调用一次）：嵌入形态无 glue，stdout 无
     *  logcat 重定向——每条引擎诊断同步落 <dir>/dev_log.txt，崩溃/黑屏
     *  以此定位。建议传 getExternalFilesDir(null)（USB 免 root 直拉） */
    public static native void setLogDir(String dir);

    /** 运行一段 Python 源码（python -c 形态）；引擎自起线程，不占 UI 主线程 */
    public static native void loadCode(String code);

    /** 触摸注入：action 0=down / 1=move / 2=up（引擎自定义编码，非 MotionEvent 原值）；
     *  坐标 = SurfaceView 位图像素 */
    public static native void injectTouch(int action, float x, float y);

    /** 请求停止当前运行（帧边界收口；线程无法强杀，取消标志 = 唯一安全方式） */
    public static native void stop();
}
