package org.bunsenbrenner.webconference

import android.os.Bundle
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity

/**
 * Deliberately minimal: this is the pipeline's "basic setup" starting point
 * (The Development System, CADS-Tunnel#382), not a working client yet. The
 * pipeline's later stages replace this with a real Agent-Fabric channel-join
 * + Noise_IK handshake + WebRTC client -- ct-agent-wasm's browser
 * implementation is the reference behavior this must match.
 */
class MainActivity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val view = TextView(this)
        view.text = getString(R.string.status_scaffold)
        view.textSize = 18f
        view.setPadding(48, 96, 48, 48)
        setContentView(view)
    }
}
