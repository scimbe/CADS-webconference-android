package org.bunsenbrenner.cads.webconference

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import androidx.core.content.ContextCompat
import org.bunsenbrenner.cads.webconference.call.IncomingCallService
import org.bunsenbrenner.cads.webconference.identity.HexKeypair
import org.bunsenbrenner.cads.webconference.identity.KeyStoreIdentity
import uniffi.ct_agent_android.generateHolderIdentity
import uniffi.ct_agent_android.generateNoiseIdentity

/**
 * Scaffold entry point. On create: load the persisted holder/noise identity from
 * [KeyStoreIdentity] (generating one via rust-core's real UniFFI bindings if none exists
 * yet -- the native-lib-packaging gap this depended on is fixed, see README.md), request
 * the runtime POST_NOTIFICATIONS permission (API 33+) needed for incoming-call alerts, and
 * start [IncomingCallService]. See that service's own doc comment for the honest boundary:
 * it runs and posts its own foreground notification, but has no live network listener yet.
 */
class MainActivity : AppCompatActivity() {

    private val notificationPermissionRequest = registerForActivityResult(
        androidx.activity.result.contract.ActivityResultContracts.RequestPermission(),
    ) { /* no extra action either way -- IncomingCallService still starts; only the
           user-visible notification itself is suppressed by the OS without this grant. */ }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            ContextCompat.checkSelfPermission(this, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            notificationPermissionRequest.launch(Manifest.permission.POST_NOTIFICATIONS)
        }
        ContextCompat.startForegroundService(this, Intent(this, IncomingCallService::class.java))

        val identity = KeyStoreIdentity(applicationContext)
        val status = try {
            if (!identity.hasHolderIdentity()) {
                identity.generateAndStoreHolderIdentity {
                    val real = generateHolderIdentity()
                    HexKeypair(publicHex = real.publicHex, privateHex = real.privateHex)
                }
            }
            if (!identity.hasNoiseIdentity()) {
                identity.generateAndStoreNoiseIdentity {
                    val real = generateNoiseIdentity()
                    HexKeypair(publicHex = real.publicHex, privateHex = real.privateHex)
                }
            }
            "Holder identity: ${identity.holderPublicHex()?.take(16)}…\n" +
                "Noise identity: ${identity.noisePublicHex()?.take(16)}…\n" +
                "(persisted via Android Keystore / EncryptedSharedPreferences)"
        } catch (e: UnsatisfiedLinkError) {
            "Native library not loaded (expected for now): ${e.message}\n\n" +
                "rust-core-debug.aar does not yet package the compiled .so libraries -- " +
                "see README.md's known-gaps section (mozilla/rust-android-gradle#43)."
        }

        setContentView(TextView(this).apply {
            text = "CADS-webconference-android — scaffold, no call UI yet\n\n$status"
            textSize = 16f
        })
    }
}
