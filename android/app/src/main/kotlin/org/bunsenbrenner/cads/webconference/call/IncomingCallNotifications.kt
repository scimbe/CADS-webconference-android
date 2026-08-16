package org.bunsenbrenner.cads.webconference.call

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.os.Build

/**
 * Real, standalone notification-channel setup and incoming-call notification builder --
 * the part of "push notifications and prompting on incoming calls" (the user's own ask)
 * that can be built and reasoned about without the network transport layer existing yet.
 *
 * HONEST BOUNDARY: nothing in this file, or [IncomingCallService], ever calls
 * [IncomingCallNotifications.show] on its own. That call has to come from wherever a real
 * incoming [uniffi.ct_agent_android.SignalMessage] arrives over a live signaling connection
 * -- which needs a real WebSocket client talking to a CADS-Tunnel edge's `ws_channel.rs`
 * listener, not yet implemented anywhere in this app. The Rust-side wire format
 * (SignalMessage, holder_sign, build_channel_join_request, channel_id_for_link) is already
 * ported and tested in rust-core; only the actual live network listener that would drive
 * this notification is missing. Wiring that listener is the next real step, not done here.
 */
object IncomingCallNotifications {
    const val CHANNEL_ID = "incoming_call"
    private const val NOTIFICATION_ID = 1001

    /** Registers the notification channel. Safe to call repeatedly (idempotent on Android). */
    fun ensureChannel(context: Context) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val manager = context.getSystemService(NotificationManager::class.java)
        val channel = NotificationChannel(
            CHANNEL_ID,
            "Incoming calls",
            NotificationManager.IMPORTANCE_HIGH,
        ).apply {
            description = "Real-time alerts for an incoming CADS-Tunnel channel call"
            setBypassDnd(true)
            enableVibration(true)
            lockscreenVisibility = Notification.VISIBILITY_PUBLIC
        }
        manager.createNotificationChannel(channel)
    }

    /**
     * Builds and posts a full-screen incoming-call notification for [callerHolderPublicHex]
     * (the caller's holder identity public key hex -- the only stable identifier a channel
     * member has for a peer, see rust-core's HolderIdentity). Uses a full-screen intent so
     * this rings/wakes the device the way a real call does, not a passive notification.
     */
    fun show(context: Context, callerHolderPublicHex: String) {
        ensureChannel(context)

        val fullScreenIntent = Intent(context, IncomingCallActivity::class.java).apply {
            putExtra(IncomingCallActivity.EXTRA_CALLER_HOLDER_HEX, callerHolderPublicHex)
            addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
        }
        val fullScreenPendingIntent = PendingIntent.getActivity(
            context, 0, fullScreenIntent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )

        val acceptIntent = Intent(context, IncomingCallActivity::class.java).apply {
            action = IncomingCallActivity.ACTION_ACCEPT
            putExtra(IncomingCallActivity.EXTRA_CALLER_HOLDER_HEX, callerHolderPublicHex)
            addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
        }
        val declineIntent = Intent(context, IncomingCallActivity::class.java).apply {
            action = IncomingCallActivity.ACTION_DECLINE
            putExtra(IncomingCallActivity.EXTRA_CALLER_HOLDER_HEX, callerHolderPublicHex)
            addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
        }

        val shortCallerId = callerHolderPublicHex.take(12) + "…"
        val notification = Notification.Builder(context, CHANNEL_ID)
            .setContentTitle("Incoming call")
            .setContentText(shortCallerId)
            .setSmallIcon(android.R.drawable.sym_call_incoming)
            .setCategory(Notification.CATEGORY_CALL)
            .setPriority(Notification.PRIORITY_MAX)
            .setFullScreenIntent(fullScreenPendingIntent, true)
            .setOngoing(true)
            .addAction(
                android.R.drawable.ic_menu_call,
                "Accept",
                PendingIntent.getActivity(context, 1, acceptIntent, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE),
            )
            .addAction(
                android.R.drawable.ic_menu_close_clear_cancel,
                "Decline",
                PendingIntent.getActivity(context, 2, declineIntent, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE),
            )
            .build()

        context.getSystemService(NotificationManager::class.java).notify(NOTIFICATION_ID, notification)
    }

    fun dismiss(context: Context) {
        context.getSystemService(NotificationManager::class.java).cancel(NOTIFICATION_ID)
    }
}
