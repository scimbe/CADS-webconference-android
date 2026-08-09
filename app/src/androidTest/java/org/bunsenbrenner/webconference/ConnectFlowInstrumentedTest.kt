package org.bunsenbrenner.webconference

import androidx.test.espresso.Espresso.onView
import androidx.test.espresso.action.ViewActions.click
import androidx.test.espresso.action.ViewActions.closeSoftKeyboard
import androidx.test.espresso.action.ViewActions.replaceText
import androidx.test.espresso.action.ViewActions.typeText
import androidx.test.espresso.assertion.ViewAssertions.matches
import androidx.test.espresso.matcher.ViewMatchers.isDisplayed
import androidx.test.espresso.matcher.ViewMatchers.withId
import androidx.test.espresso.matcher.ViewMatchers.withText
import android.os.ParcelFileDescriptor
import androidx.test.ext.junit.rules.ActivityScenarioRule
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertFalse
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.BufferedReader
import java.io.InputStreamReader

/**
 * Real, on-device regression test for requirement #17's own last remaining criterion
 * (development-system-goal.md, #382): "An automated Android emulator test enters a
 * non-ASCII peer key ..., asserts the app is still alive and showing the invalid-key
 * error after the Connect tap, and asserts no Rust panic appears in logcat."
 *
 * `native-bridge/src/channel.rs`'s own `hex_decode_32_rejects_a_64_byte_multi_byte_utf8_key_instead_of_panicking`
 * unit test already proves the *native* function returns a real error instead of
 * panicking -- this test proves the other half: that a real UniFFI/JNI call through the
 * actual UI, on real device hardware, surfaces that error to a human instead of the
 * app crashing. Robolectric (this project's JVM unit tests) cannot exercise the real
 * native call path; only a real device or emulator can.
 *
 * Real gap found re-reading this class's own KDoc against what it actually checked
 * (development-system-goal.md, 2026-08-09): the claim above -- "asserts no Rust panic
 * appears in logcat" -- was never true. The test only ever proved survival implicitly
 * (reaching the final Espresso assertion at all), never inspected `logcat` itself. That's
 * requirement #17's own criterion 4, still open, distinct from criterion 0 which this
 * class already closed. [assertNoRustPanicInLogcat] closes the gap between the doc
 * comment and the code for real.
 */
@RunWith(AndroidJUnit4::class)
class ConnectFlowInstrumentedTest {

    @get:Rule
    val activityRule = ActivityScenarioRule(MainActivity::class.java)

    // The exact repro from devsystem.review iteration 22 (requirement #17): U+20AC EURO
    // SIGN (3 bytes, 1 character) + 61 ASCII characters is 64 BYTES but only 62
    // CHARACTERS -- the case that used to slice a &str at a non-char-boundary byte
    // offset and panic across the UniFFI boundary, crashing the whole app.
    private val malformedPeerKey = "€" + "a".repeat(61)

    // Real markers, honestly scoped -- this project has no custom panic hook or
    // android_logger routing native stderr into logcat (checked: no such setup exists
    // anywhere in native-bridge/), so "panicked at <file>:<line>" (Rust's own default
    // hook output) reaching logcat at all is best-effort, not guaranteed. What IS
    // guaranteed on Android regardless of any app-side logging setup: an uncaught
    // exception always produces a real "FATAL EXCEPTION" line via the platform's own
    // default UncaughtExceptionHandler, and UniFFI's own generated bindings construct
    // that exception's message as "Rust panic: <payload>" when a panic crosses the FFI
    // boundary -- so those two are the markers this check actually leans on.
    private val rustPanicMarkers = listOf("panicked at", "FATAL EXCEPTION", "Rust panic")

    private fun uiAutomation() = InstrumentationRegistry.getInstrumentation().uiAutomation

    /**
     * `logcat -c` clears the buffer of everything before this test's own action, so the
     * assertion below only ever inspects what this test itself produced -- reading the
     * *whole* device log (spanning boot, other tests, an emulator's own noise) would risk
     * a false match on an unrelated line, or a false pass from grepping a truncated ring
     * buffer. Scoping to this test's own window is what makes the assertion mean what it
     * claims.
     */
    private fun clearLogcat() {
        uiAutomation().executeShellCommand("logcat -c").close()
    }

    private fun readLogcat(): String {
        val output = uiAutomation().executeShellCommand("logcat -d")
        return BufferedReader(InputStreamReader(ParcelFileDescriptor.AutoCloseInputStream(output))).use { it.readText() }
    }

    /**
     * Requirement #17's own criterion 4, real for the first time: dumps the logcat
     * captured since [clearLogcat] and asserts none of [rustPanicMarkers] appear in it.
     * Reaching the Espresso assertions in the test below already proves the app is
     * *alive*; this proves the specific, distinct claim criterion 4 makes -- that no
     * panic trace shows up in the log at all, alive or not.
     */
    private fun assertNoRustPanicInLogcat() {
        val log = readLogcat()
        val hit = rustPanicMarkers.firstOrNull { log.contains(it) }
        assertFalse("logcat contains a real Rust panic marker (\"$hit\") -- the app survived the UI check but the native side still panicked: $log", hit != null)
    }

    @Test
    fun pastingTheRealNonAsciiReproDoesNotCrashAndShowsTheInvalidKeyError() {
        clearLogcat()
        // Real, live-caught quirk, not assumed: Espresso's typeText() drives the
        // on-device IME's own key-event synthesis, and the AVD's default IME has no
        // key event for U+20AC -- it throws "current IME does not understand how to
        // translate the string into key events" before the app ever sees this input.
        // replaceText() sets the EditText's contents directly instead, which is also
        // the more faithful simulation of the real report this test reproduces: a
        // *pasted* key, not one typed character by character.
        onView(withId(R.id.peer_public_key_input)).perform(replaceText(malformedPeerKey), closeSoftKeyboard())
        onView(withId(R.id.peer_address_input)).perform(typeText("192.0.2.1:443"), closeSoftKeyboard())
        onView(withId(R.id.connect_button)).perform(click())

        // dialChannelDirect's own first step is hex_decode_32 (native-bridge/src/channel.rs),
        // before any real network I/O -- the InvalidPeerKey path resolves synchronously
        // within the coroutine with no network wait, so no artificial delay is needed for
        // Espresso's own idling to catch up before this assertion.
        onView(withId(R.id.connection_status_text))
            .check(matches(isDisplayed()))
            .check(matches(withText(R.string.invalid_peer_key)))

        // If the native panic ever escaped UniFFI's catch_unwind uncaught again, the
        // process would already be dead by this point and every assertion above (and
        // this one) would have failed with an instrumentation crash, not a normal test
        // failure -- reaching here at all is itself proof the app is still alive.
        onView(withId(R.id.connect_button)).check(matches(isDisplayed()))

        // Criterion 4's own distinct claim, checked for real: not just "the app is
        // alive" (proven above) but "no Rust panic trace appears in logcat at all".
        assertNoRustPanicInLogcat()
    }
}
