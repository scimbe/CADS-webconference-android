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
 * The second line comes from a real call into `uniffi.native_bridge.bridgeVersion()`
 * (see MainActivity.nativeBridgeStatusLine()). Under Robolectric -- a host JVM on
 * desktop glibc Linux -- `libnative_bridge.so` (cross-compiled by cargo-ndk against
 * Android's Bionic libc) cannot load, so this test exercises the real call's
 * defensive failure path, not a mocked success. Only an actual Android
 * device/emulator (unavailable on this host, see the #382 run notes) can prove the
 * success path renders the real Rust-computed version string.
 */
@RunWith(RobolectricTestRunner::class)
class MainActivityTest {

    @Test
    fun displaysTheScaffoldStatusStringFollowedByTheNativeBridgeLine() {
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            scenario.onActivity { activity ->
                val view = activity.findViewById<TextView>(R.id.scaffold_status_text)
                val text = view.text.toString()
                assertTrue(text.startsWith(activity.getString(R.string.status_scaffold)))
                // Either the real bridge_version() value came back (only possible on
                // a real device/emulator ABI-matching jniLibs/), or the LinkageError
                // fallback fired (expected here, under Robolectric) -- both are
                // "Native bridge" prefixed strings from MainActivity's real call path.
                assertTrue(text.contains("Native bridge"))
            }
        }
    }
}
