package org.bunsenbrenner.webconference

import android.widget.Button
import android.widget.EditText
import android.widget.TextView
import androidx.test.core.app.ActivityScenario
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/**
 * Proves the scaffold's status line still renders, and that the real connect/send/
 * receive UI (#382, minimal chat client) is actually present and wired up -- not just
 * that MainActivity compiles.
 *
 * Runs on the JVM via Robolectric -- no emulator needed. `libnative_bridge.so`
 * (cross-compiled by cargo-ndk against Android's Bionic libc) cannot load under
 * Robolectric's host JVM (desktop glibc), so every real FFI call here exercises its
 * [LinkageError] fallback path, not the success path -- proving the actual
 * connect/send/receive behavior needs a real device/emulator (issue #13,
 * labor-setup.com), which these tests deliberately do not attempt to fake.
 */
@RunWith(RobolectricTestRunner::class)
class MainActivityTest {

    @Test
    fun displaysTheScaffoldStatusStringAndTheNativeBridgeLine() {
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            scenario.onActivity { activity ->
                val view = activity.findViewById<TextView>(R.id.scaffold_status_text)
                val text = view.text.toString()
                assertTrue(text.startsWith(activity.getString(R.string.status_scaffold)))
                // Either the real bridge version came back (only possible on a real
                // device/emulator ABI-matching jniLibs/), or the LinkageError
                // fallback fired (expected here, under Robolectric) -- both are
                // prefixed strings from MainActivity's own real call path.
                assertTrue(text.contains("Native bridge"))
            }
        }
    }

    @Test
    fun showsAnIdentityStatusEitherRealOrTheHonestUnavailableFallback() {
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            scenario.onActivity { activity ->
                val view = activity.findViewById<TextView>(R.id.my_identity_text)
                val text = view.text.toString()
                assertTrue(
                    "expected either the real identity label or the unavailable fallback, got: $text",
                    text.contains("My identity") || text.contains("unavailable"),
                )
            }
        }
    }

    @Test
    fun theRealConnectSendReceiveControlsExistAndAreWiredUp() {
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            scenario.onActivity { activity ->
                val startListening = activity.findViewById<Button>(R.id.start_listening_button)
                val peerKey = activity.findViewById<EditText>(R.id.peer_public_key_input)
                val peerAddress = activity.findViewById<EditText>(R.id.peer_address_input)
                val connect = activity.findViewById<Button>(R.id.connect_button)
                val messages = activity.findViewById<TextView>(R.id.messages_text)
                val messageInput = activity.findViewById<EditText>(R.id.message_input)
                val send = activity.findViewById<Button>(R.id.send_button)

                assertTrue(startListening.hasOnClickListeners())
                assertTrue(connect.hasOnClickListeners())
                assertTrue(send.hasOnClickListeners())
                assertTrue(peerKey.hint.toString().contains("public key"))
                assertTrue(peerAddress.hint.toString().contains("address"))
                assertTrue(messageInput.hint.toString() == activity.getString(R.string.message_hint))
                // No message thread yet -- nothing sent or received before any real
                // channel connects.
                assertTrue(messages.text.toString().isEmpty())
            }
        }
    }
}
