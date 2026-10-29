package com.leishuo.galaxyrecorder

import android.os.Bundle
import android.graphics.Color
import androidx.activity.SystemBarStyle
import androidx.activity.enableEdgeToEdge
import java.io.File

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    RecorderCrashLog.install(this)
    enableEdgeToEdge(
      statusBarStyle = SystemBarStyle.light(Color.TRANSPARENT, Color.TRANSPARENT),
      navigationBarStyle = SystemBarStyle.light(Color.TRANSPARENT, Color.TRANSPARENT)
    )
    super.onCreate(savedInstanceState)
  }
}

private object RecorderCrashLog {
  private var installed = false
  fun install(context: android.content.Context) {
    if (installed) return
    installed = true
    val previous = Thread.getDefaultUncaughtExceptionHandler()
    val file = File(context.applicationContext.filesDir, "diagnostics/android-crash.log")
    Thread.setDefaultUncaughtExceptionHandler { thread, error ->
      try {
        file.parentFile?.mkdirs()
        // Exception messages can contain URLs or credentials. Keep only types and code locations.
        file.writeText(java.time.Instant.now().toString() + "\n" + error.javaClass.name + "\n" + error.stackTrace.take(80).joinToString("\n"))
      } catch (_: Exception) { }
      previous?.uncaughtException(thread, error)
    }
  }
}
