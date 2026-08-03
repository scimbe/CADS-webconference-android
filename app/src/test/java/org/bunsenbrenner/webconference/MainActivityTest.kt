package org.bunsenbrenner.webconference

import android.widget.TextView
import androidx.test.core.app.ActivityScenario
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/**
 * The pipeline's first real "test" stage output for this project: proves the
 * scaffold actually displays its status string, not just that it compiles
 * (assembleDebug's job). Runs on the JVM via Robolectric -- no emulator needed.
 */
@RunWith(RobolectricTestRunner::class)
class MainActivityTest {

    @Test
    fun displaysTheScaffoldStatusString() {
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            scenario.onActivity { activity ->
                val view = activity.findViewById<TextView>(R.id.scaffold_status_text)
                assertEquals(activity.getString(R.string.status_scaffold), view.text.toString())
            }
        }
    }
}
