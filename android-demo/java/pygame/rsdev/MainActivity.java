package pygame.rsdev;

import android.app.ListActivity;
import android.content.Intent;
import android.os.Bundle;
import android.view.View;
import android.widget.ArrayAdapter;
import android.widget.ListView;
import java.util.Arrays;

/** 脚本选择页：列 APK assets/scripts/ 下打包的全量 .py，点选进运行页。 */
public class MainActivity extends ListActivity {
    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        SdLog.init(getApplicationContext());
        // 崩溃钩子：Java 层异常全栈落盘后交回默认处理（鸿蒙机无 adb
        // logcat——这里是 Java 侧崩溃的唯一取证通道）
        final Thread.UncaughtExceptionHandler prev =
                Thread.getDefaultUncaughtExceptionHandler();
        Thread.setDefaultUncaughtExceptionHandler((t, e) -> {
            SdLog.log("[CRASH] thread=" + t.getName() + ": " + e);
            for (StackTraceElement s : e.getStackTrace()) SdLog.log("  at " + s);
            if (prev != null) prev.uncaughtException(t, e);
        });
        PygameRsDev.initContext(getApplicationContext());
        SdLog.log("MainActivity.onCreate ndk-context 已绑定");
        try {
            String[] names = getAssets().list("scripts");
            Arrays.sort(names);
            setListAdapter(new ArrayAdapter<>(this,
                    android.R.layout.simple_list_item_1, names));
        } catch (Exception e) {
            setListAdapter(new ArrayAdapter<>(this,
                    android.R.layout.simple_list_item_1,
                    new String[]{"assets/scripts 缺失（先跑 build_native_demo.py）"}));
        }
    }

    @Override
    protected void onListItemClick(ListView l, View v, int position, long id) {
        String name = (String) getListAdapter().getItem(position);
        startActivity(new Intent(this, ScriptActivity.class)
                .putExtra("script", name));
    }
}
