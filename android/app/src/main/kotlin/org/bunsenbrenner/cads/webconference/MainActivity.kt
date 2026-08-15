package org.bunsenbrenner.cads.webconference

import android.os.Bundle
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import org.bunsenbrenner.cads.webconference.identity.HexKeypair
import org.bunsenbrenner.cads.webconference.identity.KeyStoreIdentity
import uniffi.ct_agent_android.generateHolderIdentity
import uniffi.ct_agent_android.generateNoiseIdentity

/**
 * Scaffold entry point, first real wiring of rust-core's UniFFI bindings into the app --
 * still no call UI. On create: load the persisted holder/noise identity from
 * [KeyStoreIdentity], generating one via rust-core if none exists yet.
 *
 * KNOWN GAP, not yet fixed (see README.md "Not yet done" / mozilla/rust-android-gradle#43):
 * the rust-core-debug.aar this depends on does not yet package the native .so libraries,
 * only the compiled Kotlin bindings. Calling generateHolderIdentity()/generateNoiseIdentity()
 * as written below is real, correct integration code against the real generated API
 * (`uniffi.ct_agent_android.generateHolderIdentity(): HolderIdentity`, confirmed present in
 * the generated bindings) -- but until that packaging gap is fixed, it will throw
 * UnsatisfiedLinkError at runtime, not silently no-op. The catch below surfaces that
 * honestly in the UI rather than swallowing it.
 */
class MainActivity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

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
