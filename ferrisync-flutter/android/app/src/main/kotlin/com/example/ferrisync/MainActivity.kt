package com.example.ferrisync

import android.Manifest
import android.content.Intent
import android.os.Build
import androidx.annotation.NonNull
import androidx.core.app.ActivityCompat
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel

class MainActivity : FlutterActivity() {

    private val channelName = "ferrisync/service"
    private val notificationChannelName = "ferrisync/notifications"

    private lateinit var notifications: NotificationsController

    /** Dart callback parked while the runtime permission dialog is up. */
    private var pendingPermissionResult: MethodChannel.Result? = null

    override fun configureFlutterEngine(@NonNull flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        notifications = NotificationsController(this)
        notifications.ensureResultsChannel()

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
                "areNotificationsEnabled" ->
                    result.success(notifications.areNotificationsEnabled())
                "requestPermission" -> requestNotificationPermission(result)
                "show" -> {
                    notifications.postSyncResult(
                        title = call.argument<String>("title") ?: "",
                        body = call.argument<String>("body") ?: "",
                    )
                    result.success(null)
                }
                "getPref" -> result.success(notifications.getSyncNotificationsPref())
                "setPref" -> {
                    notifications.setSyncNotificationsPref(
                        call.arguments as? Boolean == true,
                    )
                    result.success(null)
                }
                else -> result.notImplemented()
            }
        }
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
                    grantResults[0] == android.content.pm.PackageManager.PERMISSION_GRANTED,
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

    private fun requestNotificationPermission(result: MethodChannel.Result) {
        if (!notifications.needsRuntimeRequest()) {
            // Below Android 13, or already granted.
            result.success(true)
            return
        }
        // The dialog was shown before and denied: many OEM skins silently
        // suppress further requests. Resolve false immediately so Dart can
        // surface the "Open settings" path instead of hanging forever.
        if (notifications.hasAskedForPermissionBefore()) {
            result.success(false)
            return
        }
        // Park the callback; the answer arrives in
        // onRequestPermissionsResult and completes the Dart Future.
        pendingPermissionResult = result
        notifications.recordPermissionAsk()
        ActivityCompat.requestPermissions(
            this,
            arrayOf(Manifest.permission.POST_NOTIFICATIONS),
            REQUEST_POST_NOTIFICATIONS_CHANNEL,
        )
    }

    companion object {
        private const val REQUEST_POST_NOTIFICATIONS_CHANNEL = 4712
        private const val REQUEST_POST_NOTIFICATIONS = 4711
    }
}
