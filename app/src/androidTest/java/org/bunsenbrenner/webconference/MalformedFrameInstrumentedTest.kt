package org.bunsenbrenner.webconference

import android.os.ParcelFileDescriptor
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.async
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.fail
import org.junit.Test
import org.junit.runner.RunWith
import java.io.BufferedReader
import java.io.InputStreamReader
import uniffi.native_bridge.ChannelException
import uniffi.native_bridge.bindChannelListener
import uniffi.native_bridge.dialChannelDirect
import uniffi.native_bridge.generateChannelIdentity
import uniffi.native_bridge.newTextMessage

/**
 * Real, on-device proof of requirement #22 criterion 2's remaining half
 * (development-system-goal.md, #382): "no decode failure may cross the UniFFI boundary as
 * a Rust panic, proven by a real on-device emulator test inspecting logcat."
 *
 * `native-bridge/src/channel.rs`'s own hermetic tests already prove this at the Rust level,
 * twice over: once reaching into `ChannelSession`'s private fields directly, and again
 * (`send_raw_bytes_for_testing_reproduces_the_real_malformed_frame_scenario_through_the_public_api`)
 * through the exact same public UniFFI surface this test calls. What neither hermetic test can
 * prove is the one thing criterion 2 actually asks for: that a real JNI call through the actual
 * compiled `.so`, on real device hardware, behaves the same way -- a decode failure crossing the
 * FFI boundary as a raw pointer/buffer operation is exactly the kind of bug that can behave
 * differently on-device than in a `cargo test` process, which is why this criterion was never
 * satisfied by the hermetic tests alone.
 *
 * Runs entirely through the real native-bridge API (`generateChannelIdentity`,
 * `bindChannelListener`, `dialChannelDirect`, `sendRawBytesForTesting`, `recvText`) -- no
 * Activity/UI involved, since this is testing the native session layer itself, the same scope
 * `ConnectFlowInstrumentedTest` already established a UI-free instrumented test can cover when
 * a real device/emulator (not Robolectric) is what a criterion actually requires.
 */
@RunWith(AndroidJUnit4::class)
class MalformedFrameInstrumentedTest {

    // Same real, honestly-scoped markers `ConnectFlowInstrumentedTest` already established --
    // this project has no custom panic hook routing native stderr into logcat, so
    // "panicked at" is best-effort; "FATAL EXCEPTION" is what Android's own default
    // UncaughtExceptionHandler guarantees regardless of that.
    private val rustPanicMarkers = listOf("panicked at", "FATAL EXCEPTION", "Rust panic")

    private fun uiAutomation() = InstrumentationRegistry.getInstrumentation().uiAutomation

    private fun clearLogcat() {
        uiAutomation().executeShellCommand("logcat -c").close()
    }

    private fun assertNoRustPanicInLogcat() {
        val output = uiAutomation().executeShellCommand("logcat -d")
        val log = BufferedReader(InputStreamReader(ParcelFileDescriptor.AutoCloseInputStream(output))).use { it.readText() }
        val hit = rustPanicMarkers.firstOrNull { log.contains(it) }
        assertFalse("logcat contains a real Rust panic marker (\"$hit\") after a malformed frame -- the decode failure crossed the UniFFI boundary as a panic, not a real error: $log", hit != null)
    }

    @Test
    fun aMalformedFrameOverARealOnDeviceSessionSurfacesAsATypedErrorNeverAPanic() = runBlocking {
        clearLogcat()

        // The exact real, two-session scenario native-bridge's own hermetic tests already
        // proved off-device -- run here through a real JNI call on real device hardware.
        val responderIdentity = generateChannelIdentity()
        val initiatorIdentity = generateChannelIdentity()
        val responderPublicHex = responderIdentity.publicKeyHex()

        val listener = bindChannelListener("127.0.0.1:0")
        val boundAddr = listener.localAddr()

        val acceptDeferred = async { listener.accept(responderIdentity) }
        val initiatorSession = dialChannelDirect(initiatorIdentity, responderPublicHex, boundAddr)
        val responderSession = acceptDeferred.await()

        // The real hostile/corrupted-peer case, over the real JNI boundary: bytes that are
        // genuinely not a well-formed TextMessage, sent through the real, authenticated,
        // encrypted transport.
        responderSession.sendRawBytesForTesting("not a real TextMessage at all".toByteArray())

        try {
            initiatorSession.recvText()
            fail("a malformed frame must surface as a real ChannelException.Decode, not succeed")
        } catch (e: ChannelException.Decode) {
            // Expected: a real, typed error crossed the FFI boundary cleanly, not a panic.
        }

        // Criterion 3's own claim, proven again at this level: the session survives and a
        // real, well-formed follow-up message from the same peer still arrives normally.
        val followup = newTextMessage(responderPublicHex, "still here after the bad frame on-device")
        responderSession.sendText(followup)
        val received = initiatorSession.recvText()
        assertEquals("the real, well-formed follow-up message must arrive intact", followup.body, received.body)

        // Criterion 2's own distinct claim, checked for real: not just "the session
        // survived" (proven above) but "no Rust panic trace appears in logcat at all".
        assertNoRustPanicInLogcat()
    }
}
