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
        view.id = R.id.scaffold_status_text
        view.text = getString(R.string.status_scaffold)
        view.textSize = 18f
        val horizontal = resources.getDimensionPixelSize(R.dimen.scaffold_padding_horizontal)
        val top = resources.getDimensionPixelSize(R.dimen.scaffold_padding_top)
        val bottom = resources.getDimensionPixelSize(R.dimen.scaffold_padding_bottom)
        view.setPadding(horizontal, top, horizontal, bottom)
        setContentView(view)
    }
}
