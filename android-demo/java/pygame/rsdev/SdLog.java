package pygame.rsdev;

import android.content.Context;
import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.text.SimpleDateFormat;
import java.util.Date;

/** Kotlin/Java 层诊断落盘（鸿蒙机无 adb 的等效通道）：追加写
 *  externalFilesDir/ksys_log.txt——USB 文件传输/文件管理器直拉，
 *  与 Rust 侧 dev_log.txt 并列（前者记 Java 生命周期/崩溃，后者记
 *  引擎阶段面包屑）。 */
public final class SdLog {
    /** 构建标签：ksys_log 首行即可验证真机在跑哪个包。占位符
     *  __BUILD_ID__ 由 build_native_demo.py 在 javac 前注入构建时刻
     *  （每包唯一——手动常量从不更新，真机新旧包无法判定的教训） */
    public static final String BUILD = "__BUILD_ID__";
    private static File file;
    private static final SimpleDateFormat TS =
            new SimpleDateFormat("MM-dd HH:mm:ss.SSS");

    private SdLog() {}

    /** 进程早期调用一次（MainActivity.onCreate 首行）。落点优先
     *  /sdcard/Download（文件管理器/USB 直取，manifest 已声明 legacy
     *  外部存储）；不可写则回退应用私有 external 目录。 */
    public static void init(Context ctx) {
        File download = new File("/sdcard/Download");
        if (download.isDirectory() || download.mkdirs()) {
            File probe = new File(download, "pygame_rsdev_ksys_log.txt");
            try {
                FileOutputStream t = new FileOutputStream(probe, true);
                t.close();
                file = probe;
            } catch (IOException ignored) {
            }
        }
        if (file == null) {
            File dir = ctx.getExternalFilesDir(null);
            if (dir == null) dir = ctx.getFilesDir();
            file = new File(dir, "ksys_log.txt");
        }
        log("=== 进程启动 pid=" + android.os.Process.myPid()
                + " build=" + BUILD
                + " log=" + file.getAbsolutePath() + " ===");
    }

    public static synchronized void log(String msg) {
        if (file == null) return;
        FileOutputStream out = null;
        try {
            out = new FileOutputStream(file, true);
            out.write((TS.format(new Date()) + " " + msg + "\n").getBytes("UTF-8"));
        } catch (IOException ignored) {
        } finally {
            if (out != null) try { out.close(); } catch (IOException ignored) {}
        }
    }
}
