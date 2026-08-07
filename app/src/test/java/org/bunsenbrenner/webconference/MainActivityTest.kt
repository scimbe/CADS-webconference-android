package org.bunsenbrenner.webconference

import android.widget.Button
import android.widget.EditText
import android.widget.TextView
import androidx.lifecycle.Lifecycle
import androidx.test.core.app.ActivityScenario
import org.junit.Assert.assertFalse
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

    /**
     * Real bug found and fixed (#382): a peer disconnecting (or a send failing on a
     * dead session) used to leave both connect controls disabled forever, with no way
     * to reconnect short of restarting the app. Drives the actual production recovery
     * path ([MainActivity.resetForNewConnection]) that both [receiveLoop]'s and
     * `onSendClicked`'s `ChannelException` catch blocks now call -- not a
     * reimplementation of the fix's logic.
     */
    @Test
    fun aSessionLossReEnablesBothConnectControlsSoTheUserCanReconnectWithoutRestarting() {
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            scenario.onActivity { activity ->
                val startListening = activity.findViewById<Button>(R.id.start_listening_button)
                val connect = activity.findViewById<Button>(R.id.connect_button)
                val status = activity.findViewById<TextView>(R.id.connection_status_text)

                // Mirrors onConnected()'s real effect of disabling both controls once
                // a session is live.
                startListening.isEnabled = false
                connect.isEnabled = false

                activity.resetForNewConnection("peer disconnected")

                assertTrue(startListening.isEnabled)
                assertTrue(connect.isEnabled)
                assertTrue(status.text.toString() == "peer disconnected")
            }
        }
    }

    /**
     * Real gap found and fixed (#382, DAU lens): tapping Connect with either the peer
     * public key or address field blank used to go straight to a real native
     * dialChannelDirect() call. Now caught immediately with a clear message -- proven
     * here by the connect button staying enabled (it's only ever disabled *after* the
     * validation check passes, so it staying enabled is real evidence the native call
     * path was never reached, not just that an error string happens to match).
     */
    @Test
    fun connectingWithEitherFieldBlankIsCaughtBeforeAnyNativeCall() {
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            scenario.onActivity { activity ->
                val peerKey = activity.findViewById<EditText>(R.id.peer_public_key_input)
                val peerAddress = activity.findViewById<EditText>(R.id.peer_address_input)
                val connect = activity.findViewById<Button>(R.id.connect_button)
                val status = activity.findViewById<TextView>(R.id.connection_status_text)
                val required = activity.getString(R.string.connect_fields_required)

                // Both blank.
                peerKey.setText("")
                peerAddress.setText("")
                connect.performClick()
                assertTrue(status.text.toString() == required)
                assertTrue("must never have been disabled -- the native call path was never reached", connect.isEnabled)

                // Only the address is blank -- still caught.
                peerKey.setText("deadbeef")
                peerAddress.setText("  ")
                connect.performClick()
                assertTrue(status.text.toString() == required)
                assertTrue(connect.isEnabled)
            }
        }
    }

    /**
     * Real gap found and fixed (#382, DAU lens): tapping Send with an empty or
     * whitespace-only message used to silently do nothing -- no feedback at all, and
     * a whitespace-only message wasn't even caught (would have actually been sent as
     * real content). Checked before the session-readiness guard, the same real
     * production ordering that also makes this path reachable here (`session` is
     * always null under Robolectric -- a `session ?: return` first would make this
     * whole test a no-op against real production code, same lesson as the connect
     * fields test above).
     */
    @Test
    fun sendingABlankMessageIsCaughtWithRealFeedbackAndKeepsFocusForRetry() {
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            scenario.onActivity { activity ->
                val messageInput = activity.findViewById<EditText>(R.id.message_input)
                val send = activity.findViewById<Button>(R.id.send_button)
                val status = activity.findViewById<TextView>(R.id.connection_status_text)
                val messages = activity.findViewById<TextView>(R.id.messages_text)
                val required = activity.getString(R.string.empty_message_not_sent)

                messageInput.setText("")
                send.performClick()
                assertTrue(status.text.toString() == required)
                assertTrue("a no-op tap must never render a message", messages.text.toString().isEmpty())
                assertTrue("focus must stay on the input for retry", messageInput.isFocused)

                // Whitespace-only -- not merely empty -- must be caught the same way,
                // not silently sent as real content.
                messageInput.setText("   ")
                send.performClick()
                assertTrue(status.text.toString() == required)
                assertTrue(messages.text.toString().isEmpty())
            }
        }
    }

    /**
     * Real gap found live (#382, DAU lens): a real, non-blank message typed
     * before ever connecting to a peer used to hit `session ?: return` and
     * vanish completely -- no status text update, nothing rendered, no
     * indication anything happened. `session` is always null under Robolectric
     * (real `libnative_bridge.so` can't load here), which is exactly what makes
     * this path reachable in this test environment, the same real production
     * ordering the blank-message test above relies on.
     */
    @Test
    fun sendingARealMessageBeforeConnectingGetsRealFeedbackNotSilentFailure() {
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            scenario.onActivity { activity ->
                val messageInput = activity.findViewById<EditText>(R.id.message_input)
                val send = activity.findViewById<Button>(R.id.send_button)
                val status = activity.findViewById<TextView>(R.id.connection_status_text)
                val messages = activity.findViewById<TextView>(R.id.messages_text)
                val required = activity.getString(R.string.not_connected_message_not_sent)

                messageInput.setText("hello, anyone there?")
                send.performClick()
                assertTrue("a real status message must explain why nothing was sent", status.text.toString() == required)
                assertTrue("a no-op tap must never render a message", messages.text.toString().isEmpty())
                assertTrue("the typed message must be preserved, not silently cleared", messageInput.text.toString() == "hello, anyone there?")
            }
        }
    }

    /**
     * Real gap found live (#382, DAU lens -- surfaced by `devsystem.assistant`, independently
     * corroborating a finding this run's own history had already flagged): [MessageStore] never
     * had its real [android.database.sqlite.SQLiteDatabase] handle closed on Activity teardown.
     * Drives the actual production path ([MainActivity.onDestroy] calling the real
     * [MessageStore.close]) against the real handle -- not a reimplementation of the fix's own
     * logic, and not merely "onDestroy runs without crashing" (which would pass even if the
     * fix did nothing at all).
     */
    @Test
    fun onDestroyClosesTheMessageStoreAndTheRealDatabaseHandleReportsClosed() {
        val scenario = ActivityScenario.launch(MainActivity::class.java)
        var handle: android.database.sqlite.SQLiteDatabase? = null
        scenario.onActivity { activity -> handle = activity.messageStore.writableDatabase }
        val db = handle!!
        assertTrue("sanity check: the real handle must actually be open before teardown", db.isOpen)

        scenario.moveToState(Lifecycle.State.DESTROYED)

        assertFalse("onDestroy must have closed the real database handle, not left it open", db.isOpen)
    }
}
