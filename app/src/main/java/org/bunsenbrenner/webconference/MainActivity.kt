package org.bunsenbrenner.webconference

import android.os.Bundle
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import uniffi.native_bridge.bridgeVersion

/**
 * Deliberately minimal: this is the pipeline's "basic setup" starting point
 * (The Development System, CADS-Tunnel#382), not a working client yet. The
 * pipeline's later stages replace this with a real Agent-Fabric channel-join
 * + Noise_IK handshake + WebRTC client -- ct-agent-wasm's browser
 * implementation is the reference behavior this must match.
 *
 * `bridgeVersion()` below is a real UniFFI/JNA call across the FFI boundary
 * into the native-bridge Rust spike (libnative_bridge.so, bundled per-ABI in
 * jniLibs/) -- not a hardcoded string. It's the toolchain's proof-of-life,
 * not the real Noise_IK/Agent-Fabric client yet.
 */
class MainActivity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val view = TextView(this)
        view.id = R.id.scaffold_status_text
        view.text = getString(R.string.status_scaffold) + "\n\n" + nativeBridgeStatusLine()
        view.textSize = 18f
        val horizontal = resources.getDimensionPixelSize(R.dimen.scaffold_padding_horizontal)
        val top = resources.getDimensionPixelSize(R.dimen.scaffold_padding_top)
        val bottom = resources.getDimensionPixelSize(R.dimen.scaffold_padding_bottom)
        view.setPadding(horizontal, top, horizontal, bottom)
        setContentView(view)
    }

    /**
     * Calls the real `bridgeVersion()` FFI function. `libnative_bridge.so` is
     * cross-compiled by cargo-ndk against Android's Bionic libc for
     * arm64-v8a/x86_64 -- it can only load on an actual Android device or
     * emulator, never under a host JVM (e.g. Robolectric unit tests, which run
     * on desktop glibc Linux). [LinkageError] (covering both
     * `UnsatisfiedLinkError` and the `ExceptionInInitializerError` JNA's lazy
     * `UniffiLib` object triggers on first failed load) is caught here so the
     * scaffold still renders in those environments; on a real device with a
     * matching jniLibs/ ABI this returns the genuine Rust-computed value.
     */
    private fun nativeBridgeStatusLine(): String {
        return try {
            getString(R.string.status_native_bridge, bridgeVersion())
        } catch (e: LinkageError) {
            getString(R.string.status_native_bridge_unavailable, e.message)
        }
    }
}
