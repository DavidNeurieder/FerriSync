package com.example.ferrisync

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat

/**
 * Native side of the `ferrisync/notifications` MethodChannel.
 *
 * Extracted from MainActivity so instrumented tests can exercise channel,
 * preference and posting logic without driving the Flutter engine. All
 * methods are safe to call from any Context holder; only the runtime
 * permission dialog itself stays in MainActivity (needs an Activity).
 */
class NotificationsController(private val context: Context) {

    fun areNotificationsEnabled(): Boolean =
        NotificationManagerCompat.from(context).areNotificationsEnabled()

    /** True when the POST_NOTIFICATIONS runtime dialog must be shown. */
    fun needsRuntimeRequest(): Boolean =
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            ContextCompat.checkSelfPermission(
                context,
                Manifest.permission.POST_NOTIFICATIONS,
            ) != PackageManager.PERMISSION_GRANTED

    /**
     * Whether we already showed the permission dialog at least once. After
     * repeated silent denials many OEM skins stop showing the dialog at all;
     * callers use this to short-circuit into "permanently denied" handling
     * instead of firing a request that can never resolve visibly.
     */
    fun hasAskedForPermissionBefore(): Boolean =
        prefs().getInt(KEY_PERMISSION_ASKS, 0) > 0

    fun recordPermissionAsk() {
        prefs().edit()
            .putInt(KEY_PERMISSION_ASKS, prefs().getInt(KEY_PERMISSION_ASKS, 0) + 1)
            .apply()
    }

    fun resetPermissionAskHistory() {
        prefs().edit().remove(KEY_PERMISSION_ASKS).apply()
    }

    fun ensureResultsChannel() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val manager =
                context.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
            manager.createNotificationChannel(
                NotificationChannel(
                    SYNC_RESULTS_CHANNEL_ID,
                    "Sync results",
                    NotificationManager.IMPORTANCE_DEFAULT,
                ),
            )
        }
    }

    /** Posts a sync-completion notice; returns false when suppressed. */
    fun postSyncResult(title: String, body: String): Boolean {
        if (!areNotificationsEnabled()) return false
        val notification = NotificationCompat.Builder(context, SYNC_RESULTS_CHANNEL_ID)
            .setContentTitle(title)
            .setContentText(body)
            .setSmallIcon(android.R.drawable.stat_notify_sync_noanim)
            .setAutoCancel(true)
            .build()
        return try {
            NotificationManagerCompat.from(context)
                .notify(nextNotificationId(), notification)
            true
        } catch (_: SecurityException) {
            // Permission was revoked between the check and notify().
            false
        }
    }

    fun getSyncNotificationsPref(): Boolean =
        prefs().getBoolean(KEY_SYNC_NOTIFICATIONS, false)

    fun setSyncNotificationsPref(enabled: Boolean) {
        prefs().edit().putBoolean(KEY_SYNC_NOTIFICATIONS, enabled).apply()
    }

    private fun nextNotificationId(): Int =
        prefs().getInt(KEY_NEXT_NOTIFICATION_ID, SYNC_NOTIFICATION_ID_BASE).also { id ->
            prefs().edit().putInt(KEY_NEXT_NOTIFICATION_ID, id + 1).apply()
        }

    private fun prefs() =
        context.getSharedPreferences(PREFS_FILE, Context.MODE_PRIVATE)

    companion object {
        const val PREFS_FILE = "ferrisync_prefs"
        const val KEY_SYNC_NOTIFICATIONS = "sync_notifications"
        const val KEY_PERMISSION_ASKS = "notification_permission_asks"
        const val KEY_NEXT_NOTIFICATION_ID = "next_notification_id"
        const val SYNC_RESULTS_CHANNEL_ID = "sync_results"
        const val SYNC_NOTIFICATION_ID_BASE = 1000
    }
}
