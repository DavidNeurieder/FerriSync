package com.example.ferrisync

import android.app.NotificationManager
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Instrumented tests for the notification layer (NotificationsController).
 * Run on a device/emulator via:
 *   ./gradlew :app:connectedDebugAndroidTest
 *
 * IMPORTANT: these tests NEVER mutate runtime permission state. Granting or
 * revoking a permission force-stops the app process — which is where the
 * instrumentation itself lives — killing the whole run. The granted vs
 * revoked variants are exercised by driving the device externally:
 *   adb shell pm grant com.example.ferrisync android.permission.POST_NOTIFICATIONS
 *   adb shell pm revoke com.example.ferrisync android.permission.POST_NOTIFICATIONS
 *
 * Also avoids androidx.test:core AND ext:junit (ActivityScenario,
 * ApplicationProvider, AndroidJUnit4): versions compatible with Flutter's
 * pinned androidx.test:runner fail the Android-12 manifest export check,
 * so we use plain JUnit4 + raw instrumentation APIs.
 */
class NotificationsControllerTest {

    private val context: Context
        get() = InstrumentationRegistry.getInstrumentation().targetContext

    private fun notificationsGranted(): Boolean =
        Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU ||
            context.checkSelfPermission(android.Manifest.permission.POST_NOTIFICATIONS) ==
            PackageManager.PERMISSION_GRANTED

    // ─── Channel wiring ──────────────────────────────────────────────────────

    @Test
    fun ensureResultsChannelIsIdempotent() {
        val controller = NotificationsController(context)
        controller.ensureResultsChannel()
        controller.ensureResultsChannel()
        val manager =
            context.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        assertNotNull(manager.getNotificationChannel(NotificationsController.SYNC_RESULTS_CHANNEL_ID))
    }

    // ─── Preference persistence ──────────────────────────────────────────────

    @Test
    fun syncNotificationsPrefDefaultsToFalse() {
        val controller = NotificationsController(context)
        controller.setSyncNotificationsPref(false) // reset
        assertFalse(controller.getSyncNotificationsPref())
    }

    @Test
    fun syncNotificationsPrefRoundTrips() {
        val controller = NotificationsController(context)
        controller.setSyncNotificationsPref(true)
        assertTrue(controller.getSyncNotificationsPref())
        controller.setSyncNotificationsPref(false)
        assertFalse(controller.getSyncNotificationsPref())
    }

    // ─── Ask history (permanent-denial heuristic) ────────────────────────────

    @Test
    fun askHistoryRoundTrips() {
        val controller = NotificationsController(context)
        controller.resetPermissionAskHistory()
        assertFalse(controller.hasAskedForPermissionBefore())
        controller.recordPermissionAsk()
        assertTrue(controller.hasAskedForPermissionBefore())
        controller.resetPermissionAskHistory()
        assertFalse(controller.hasAskedForPermissionBefore())
    }

    // ─── Permission-state wiring (read-only) ─────────────────────────────────

    @Test
    fun needsRuntimeRequestMirrorsOsGrantState() {
        val controller = NotificationsController(context)
        val sdk33Plus = Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU
        val granted = notificationsGranted()

        assertEquals(
            "runtime request needed iff API 33+ and not granted",
            sdk33Plus && !granted,
            controller.needsRuntimeRequest(),
        )
    }

    @Test
    fun areEnabledMirrorsOsGrantState() {
        val controller = NotificationsController(context)

        assertEquals(
            "areNotificationsEnabled must track the OS grant",
            notificationsGranted(),
            controller.areNotificationsEnabled(),
        )
    }

    // ─── Posting (read-only wrt permissions) ─────────────────────────────────

    @Test
    fun postSyncResultBehaviorMatchesPermissionState() {
        val controller = NotificationsController(context)
        controller.ensureResultsChannel()
        val manager =
            context.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        val before = manager.activeNotifications.size

        val posted = controller.postSyncResult("FerriSync test", "instrumented body")

        assertEquals(
            "post succeeds iff notifications are enabled",
            controller.areNotificationsEnabled(),
            posted,
        )
        if (posted) {
            assertEquals(before + 1, manager.activeNotifications.size)
        }
    }
}
