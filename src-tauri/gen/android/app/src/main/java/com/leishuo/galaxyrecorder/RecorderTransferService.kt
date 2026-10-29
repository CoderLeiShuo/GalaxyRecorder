package com.leishuo.galaxyrecorder

import android.app.*
import android.content.Intent
import android.os.*
import androidx.core.app.NotificationCompat

class RecorderTransferService : Service() {
    companion object { var cancelDownloads: (() -> Unit)? = null }
    private var wake: PowerManager.WakeLock? = null
    override fun onBind(intent: Intent?) = null
    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == "CANCEL_DOWNLOADS") { cancelDownloads?.invoke(); return START_NOT_STICKY }
        val manager = getSystemService(NOTIFICATION_SERVICE) as NotificationManager
        manager.createNotificationChannel(NotificationChannel("recorder-transfers", "录像下载与导出", NotificationManager.IMPORTANCE_LOW))
        val open = PendingIntent.getActivity(this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
        val builder = NotificationCompat.Builder(this, "recorder-transfers")
            .setSmallIcon(android.R.drawable.stat_sys_download).setContentTitle("吉行影记")
            .setContentText(intent?.getStringExtra("message") ?: "正在传输录像，保持车机热点连接")
            .setContentIntent(open).setOngoing(true).setOnlyAlertOnce(true)
        if (intent?.getBooleanExtra("downloads", false) == true) {
            val cancel = PendingIntent.getService(this, 1, Intent(this, RecorderTransferService::class.java).setAction("CANCEL_DOWNLOADS"), PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
            builder.addAction(android.R.drawable.ic_menu_close_clear_cancel, "取消全部下载", cancel)
        }
        val notification = builder.build()
        startForeground(1004, notification)
        if (wake == null) wake = (getSystemService(POWER_SERVICE) as PowerManager).newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "GalaxyRecorder:transfer").apply { acquire(2 * 60 * 60 * 1000L) }
        return START_NOT_STICKY
    }
    override fun onDestroy() { wake?.let { if (it.isHeld) it.release() }; wake = null; super.onDestroy() }
    override fun onTimeout(startId: Int, fgsType: Int) { ForegroundWork.stopAll(this); cancelDownloads?.invoke(); stopSelf() }
}
