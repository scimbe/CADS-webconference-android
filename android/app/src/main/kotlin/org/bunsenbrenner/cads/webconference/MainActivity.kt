package org.bunsenbrenner.cads.webconference

import android.os.Bundle
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity

/**
 * Scaffold entry point. No call UI yet — see ARCHITECTURE.md for the planned
 * identity/transport/channel modules this will wire up once rust-core exposes
 * real UniFFI bindings instead of the current placeholder function.
 */
class MainActivity : AppCompatActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(TextView(this).apply {
            text = "CADS-webconference-android — scaffold, no call UI yet"
            textSize = 16f
        })
    }
}
