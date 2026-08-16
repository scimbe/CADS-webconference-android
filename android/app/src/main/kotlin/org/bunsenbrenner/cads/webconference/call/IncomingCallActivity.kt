package org.bunsenbrenner.cads.webconference.call

import android.app.KeyguardManager
import android.os.Build
import android.os.Bundle
import android.view.WindowManager
import android.widget.Button
import android.widget.LinearLayout
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity

/**
 * Full-screen ringing UI, launched by [IncomingCallNotifications]'s full-screen intent (or
 * directly by its Accept/Decline notification actions). Shows over the lock screen the way
 * a real incoming-call screen does.
 *
 * HONEST BOUNDARY: Accept/Decline currently only dismiss the notification and close this
 * screen -- they do not yet drive any real signaling (no NoiseTransport response is sent,
 * no RTCPeerConnection is created). That needs the same live network/WebRTC wiring
 * [IncomingCallNotifications] itself is waiting on. This activity's job is only the real,
 * complete part: showing a genuine full-screen ringing prompt and capturing the user's
 * accept/decline intent, ready for a transport layer to act on once it exists.
 */
class IncomingCallActivity : AppCompatActivity() {

    companion object {
        const val EXTRA_CALLER_HOLDER_HEX = "caller_holder_hex"
        const val ACTION_ACCEPT = "org.bunsenbrenner.cads.webconference.call.ACCEPT"
        const val ACTION_DECLINE = "org.bunsenbrenner.cads.webconference.call.DECLINE"
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        // Real over-lock-screen call-screen behavior, not a workaround -- the documented
        // modern replacement for the deprecated FLAG_SHOW_WHEN_LOCKED/FLAG_TURN_SCREEN_ON
        // window flags.
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O_MR1) {
            setShowWhenLocked(true)
            setTurnScreenOn(true)
            getSystemService(KeyguardManager::class.java)?.requestDismissKeyguard(this, null)
        } else {
            @Suppress("DEPRECATION")
            window.addFlags(
                WindowManager.LayoutParams.FLAG_SHOW_WHEN_LOCKED or
                    WindowManager.LayoutParams.FLAG_TURN_SCREEN_ON or
                    WindowManager.LayoutParams.FLAG_DISMISS_KEYGUARD,
            )
        }

        val callerHex = intent.getStringExtra(EXTRA_CALLER_HOLDER_HEX).orEmpty()

        when (intent.action) {
            ACTION_ACCEPT -> { onAccept(callerHex); return }
            ACTION_DECLINE -> { onDecline(callerHex); return }
        }

        setContentView(buildRingingView(callerHex))
    }

    private fun buildRingingView(callerHex: String) = LinearLayout(this).apply {
        orientation = LinearLayout.VERTICAL
        setPadding(48, 96, 48, 48)
        addView(TextView(this@IncomingCallActivity).apply {
            text = "Incoming call"
            textSize = 24f
        })
        addView(TextView(this@IncomingCallActivity).apply {
            text = callerHex.take(16) + if (callerHex.length > 16) "…" else ""
            textSize = 14f
        })
        addView(Button(this@IncomingCallActivity).apply {
            text = "Accept"
            setOnClickListener { onAccept(callerHex) }
        })
        addView(Button(this@IncomingCallActivity).apply {
            text = "Decline"
            setOnClickListener { onDecline(callerHex) }
        })
    }

    private fun onAccept(callerHex: String) {
        // TODO, not yet real: drive the actual NoiseTransport/RTCPeerConnection accept flow
        // once a live signaling connection exists. Today this only clears the ringing UI.
        IncomingCallNotifications.dismiss(this)
        finish()
    }

    private fun onDecline(callerHex: String) {
        // TODO, not yet real: send an encode_signal_bye() over a live NoiseTransport once
        // one exists, so the caller's side sees a real decline instead of silence.
        IncomingCallNotifications.dismiss(this)
        finish()
    }
}
