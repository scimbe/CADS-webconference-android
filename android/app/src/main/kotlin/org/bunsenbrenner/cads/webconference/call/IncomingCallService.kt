package org.bunsenbrenner.cads.webconference.call

import android.app.Notification
import android.app.Service
import android.content.Intent
import android.os.IBinder

/**
 * Foreground service meant to hold a real signaling connection alive in the background so
 * an incoming call can be detected and notified about even while the app isn't in the
 * foreground -- the standard Android pattern real calling apps use (matches
 * FOREGROUND_SERVICE_TYPE_PHONE_CALL, declared in AndroidManifest.xml).
 *
 * HONEST BOUNDARY, same one [IncomingCallNotifications] documents: this service currently
 * starts, posts its own required foreground notification, and does nothing else. It does
 * NOT open a real WebSocket to a CADS-Tunnel edge, does NOT run a NoiseHandshake, and never
 * calls [IncomingCallNotifications.show] on its own -- there is no live listener loop here
 * yet. What exists is the real Android service scaffolding (foreground-service lifecycle,
 * manifest declaration, notification channel) that a real listener loop would run inside
 * once the transport layer exists; wiring that loop is explicitly not done in this change.
 */
class IncomingCallService : Service() {

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        IncomingCallNotifications.ensureChannel(this)
        val notification = Notification.Builder(this, IncomingCallNotifications.CHANNEL_ID)
            .setContentTitle("CADS Webconference")
            .setContentText("Listening for incoming calls")
            .setSmallIcon(android.R.drawable.sym_call_incoming)
            .setOngoing(true)
            .build()
        startForeground(2001, notification)

        // TODO, not yet real: open a real WebSocket to the CADS-Tunnel edge's
        // ws_channel.rs listener, run build_channel_join_request()/holder_sign() (both
        // already ported and tested in rust-core) to join, then loop on
        // NoiseTransport::decrypt()+decode_signal_message() for real incoming Offer
        // messages, calling IncomingCallNotifications.show(this, callerHex) when one
        // arrives. None of that networking exists in this app yet.

        return START_STICKY
    }
}
