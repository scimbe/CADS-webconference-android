package org.bunsenbrenner.webconference

import android.widget.TextView
import androidx.test.core.app.ActivityScenario
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/**
 * The pipeline's first real "test" stage output for this project: proves the
 * scaffold actually displays its status string, not just that it compiles
 * (assembleDebug's job). Runs on the JVM via Robolectric -- no emulator needed.
 *
 * The second and third lines come from real calls into
 * `uniffi.native_bridge.bridgeVersion()` and `generateNoisePublicKeyHex()` (see
 * MainActivity.nativeBridgeStatusLine()/noisePublicKeyStatusLine()). Under
 * Robolectric -- a host JVM on desktop glibc Linux -- `libnative_bridge.so`
 * (cross-compiled by cargo-ndk against Android's Bionic libc) cannot load, so
 * this test exercises the real calls' defensive failure paths, not a mocked
 * success. Only an actual Android device/emulator (unavailable on this host,
 * see the #382 run notes) can prove the success paths render the real
 * Rust-computed values.
 */
@RunWith(RobolectricTestRunner::class)
class MainActivityTest {

    @Test
    fun displaysTheScaffoldStatusStringFollowedByTheNativeBridgeAndNoiseKeyLines() {
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            scenario.onActivity { activity ->
                val view = activity.findViewById<TextView>(R.id.scaffold_status_text)
                val text = view.text.toString()
                assertTrue(text.startsWith(activity.getString(R.string.status_scaffold)))
                // Either the real values came back (only possible on a real
                // device/emulator ABI-matching jniLibs/), or the LinkageError
                // fallback fired (expected here, under Robolectric) -- both are
                // prefixed strings from MainActivity's real call paths.
                assertTrue(text.contains("Native bridge"))
                assertTrue(text.contains("Noise_IK"))
            }
        }
    }
}
