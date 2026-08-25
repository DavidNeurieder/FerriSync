package com.example.ferrisync

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import androidx.annotation.NonNull
import androidx.core.app.ActivityCompat
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel

class MainActivity : FlutterActivity() {

    private val channelName = "ferrisync/service"
    private val notificationChannelName = "ferrisync/notifications"

    /** Dart callback parked while the runtime permission dialog is up. */
    private var pendingPermissionResult: MethodChannel.Result? = null

    private var nextNotificationId = SYNC_NOTIFICATION_ID_BASE

    override fun configureFlutterEngine(@NonNull flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        MethodChannel(
            flutterEngine.dartExecutor.binaryMessenger,
            channelName,
        ).setMethodCallHandler { call, result ->
            when (call.method) {
                "start" -> {
                    startSyncService()
                    result.success(null)
                }
                "stop" -> {
                    stopService(Intent(this, SyncForegroundService::class.java))
                    result.success(null)
                }
                else -> result.notImplemented()
            }
        }
        MethodChannel(
            flutterEngine.dartExecutor.binaryMessenger,
            notificationChannelName,
        ).setMethodCallHandler { call, result ->
            when (call.method) {
                "areNotificationsEnabled" -> result.success(areNotificationsEnabled())
                "requestPermission" -> requestNotificationPermission(result)
                "show" -> {
                    showSyncResult(
                        title = call.argument<String>("title") ?: "",
                        body = call.argument<String>("body") ?: "",
                    )
                    result.success(null)
                }
                "getPref" -> result.success(syncNotificationsPref())
                "setPref" -> {
                    setSyncNotificationsPref(call.arguments as? Boolean == true)
                    result.success(null)
                }
                else -> result.notImplemented()
            }
        }
        ensureSyncResultsChannel()
    }

    override fun onRequestPermissionsResult(
        requestCode: Int,
        permissions: Array<out String>,
        grantResults: IntArray,
    ) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (requestCode == REQUEST_POST_NOTIFICATIONS_CHANNEL) {
            pendingPermissionResult?.success(
                grantResults.isNotEmpty() &&
                    grantResults[0] == PackageManager.PERMISSION_GRANTED,
            )
            pendingPermissionResult = null
        }
    }

    private fun startSyncService() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            requestPermissions(
                arrayOf(Manifest.permission.POST_NOTIFICATIONS),
                REQUEST_POST_NOTIFICATIONS,
            )
        }
        val intent = SyncForegroundService.startIntent(this)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            startForegroundService(intent)
        } else {
            startService(intent)
        }
    }

    private fun areNotificationsEnabled(): Boolean =
        NotificationManagerCompat.from(this).areNotificationsEnabled()

    private fun requestNotificationPermission(result: MethodChannel.Result) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU ||
            ContextCompat.checkSelfPermission(
                this,
                Manifest.permission.POST_NOTIFICATIONS,
            ) == PackageManager.PERMISSION_GRANTED
        ) {
            result.success(true)
            return
        }
        // The dialog answer arrives in onRequestPermissionsResult; park the
        // callback so the Dart Future resolves with the real outcome.
        pendingPermissionResult = result
        ActivityCompat.requestPermissions(
            this,
            arrayOf(Manifest.permission.POST_NOTIFICATIONS),
            REQUEST_POST_NOTIFICATIONS_CHANNEL,
        )
    }

    private fun showSyncResult(title: String, body: String) {
        if (!areNotificationsEnabled()) return
        val notification = NotificationCompat.Builder(this, SYNC_RESULTS_CHANNEL_ID)
            .setContentTitle(title)
            .setContentText(body)
            .setSmallIcon(android.R.drawable.stat_notify_sync_noanim)
            .setAutoCancel(true)
            .build()
        try {
            NotificationManagerCompat.from(this)
                .notify(nextNotificationId++, notification)
        } catch (_: SecurityException) {
            // Permission was revoked between the check and notify().
        }
    }

    private fun ensureSyncResultsChannel() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val manager =
                getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
            manager.createNotificationChannel(
                NotificationChannel(
                    SYNC_RESULTS_CHANNEL_ID,
                    "Sync results",
                    NotificationManager.IMPORTANCE_DEFAULT,
                ),
            )
        }
    }

    private fun syncNotificationsPref(): Boolean =
        getSharedPreferences(PREFS_FILE, Context.MODE_PRIVATE)
            .getBoolean(KEY_SYNC_NOTIFICATIONS, false)

    private fun setSyncNotificationsPref(enabled: Boolean) {
        getSharedPreferences(PREFS_FILE, Context.MODE_PRIVATE)
            .edit()
            .putBoolean(KEY_SYNC_NOTIFICATIONS, enabled)
            .apply()
    }

    companion object {
        private const val PREFS_FILE = "ferrisync_prefs"
        private const val KEY_SYNC_NOTIFICATIONS = "sync_notifications"
        private const val SYNC_RESULTS_CHANNEL_ID = "sync_results"
        private const val SYNC_NOTIFICATION_ID_BASE = 1000

        // Distinct from REQUEST_POST_NOTIFICATIONS so the fire-and-forget
        // request in startSyncService can never resolve a parked channel
        // callback (or vice versa).
        private const val REQUEST_POST_NOTIFICATIONS_CHANNEL = 4712
        private const val REQUEST_POST_NOTIFICATIONS = 4711
    }
}
