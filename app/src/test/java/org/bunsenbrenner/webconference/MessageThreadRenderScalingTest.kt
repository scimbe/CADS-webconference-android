package org.bunsenbrenner.webconference

import android.text.Editable
import android.widget.TextView
import androidx.test.core.app.ActivityScenario
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/**
 * Requirement #19: appending one message to the conversation must cost the same
 * whether the transcript already holds ten messages or ten thousand.
 *
 * The defect this pins is real and was measured, not hypothetical:
 * `MainActivity.renderMessage` used to do
 * `messagesText.text = "${messagesText.text}\n$line"`, reading the entire displayed
 * transcript back out and building a fresh String from it on every message -- 2/9/54/218 ms
 * to replay 1k/2k/5k/10k messages, i.e. time quadrupling per doubling of n, on the UI thread.
 *
 * Two complementary tests, because a timing assertion alone is a weak regression fence:
 * [appendingReusesTheSameBufferInsteadOfRebuildingTheWholeTranscript] fails
 * deterministically the moment anyone reintroduces a whole-transcript copy, and
 * [renderingTenTimesTheMessagesCostsFarLessThanAHundredTimesTheWork] is the scaling
 * assertion the requirement's own acceptance criterion asks for.
 */
@RunWith(RobolectricTestRunner::class)
class MessageThreadRenderScalingTest {

    /**
     * The structural half. The old code assigned a brand-new String to the TextView on
     * every message, so the CharSequence identity changed each time and the copy was
     * O(transcript). Appending in place keeps one buffer for the life of the view.
     */
    @Test
    fun appendingReusesTheSameBufferInsteadOfRebuildingTheWholeTranscript() {
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            scenario.onActivity { activity ->
                val view = activity.findViewById<TextView>(R.id.messages_text)
                assertTrue(
                    "the transcript must be held in an Editable so it can be appended to in place",
                    view.text is Editable,
                )

                activity.renderMessage("first", MessageDirection.SENT, MessageStatus.SENT)
                val bufferAfterFirst = view.text
                activity.renderMessage("second", MessageDirection.RECEIVED, null)

                assertSame(
                    "appending a message must not replace the transcript with a fresh copy",
                    bufferAfterFirst,
                    view.text,
                )
                // ...and the visible result is still the same two-line thread as before.
                assertEquals(2, view.text.toString().lines().size)
                assertTrue(view.text.toString().contains("first"))
                assertTrue(view.text.toString().contains("second"))
            }
        }
    }

    /**
     * The scaling half, phrased as the requirement asks: a ratio, not an absolute
     * timing, so it stays meaningful on any CI machine. Linear-plus-overhead lands
     * near 10x and passes; the quadratic path this replaced lands near 100x and fails.
     * Best-of-three on each size, after a warm-up, so JIT and a stray GC pause cannot
     * flip the verdict on their own.
     */
    @Test
    fun renderingTenTimesTheMessagesCostsFarLessThanAHundredTimesTheWork() {
        renderDuration(SMALL) // warm-up, result discarded

        val small = bestOfThree(SMALL)
        val large = bestOfThree(SMALL * 10)
        val ratio = large.toDouble() / small.toDouble()

        assertTrue(
            "rendering ${SMALL * 10} messages took ${ratio.format()}x the time of $SMALL " +
                "(${large / 1_000_000.0} ms vs ${small / 1_000_000.0} ms); a linear append path " +
                "should be near 10x, the quadratic one this replaced was near 100x",
            ratio <= MAX_RATIO,
        )
    }

    private fun bestOfThree(count: Int): Long =
        (1..3).minOf { renderDuration(count) }.coerceAtLeast(1L)

    /** Nanoseconds spent purely in the append path -- Activity launch is deliberately outside. */
    private fun renderDuration(count: Int): Long {
        var elapsed = 0L
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            scenario.onActivity { activity ->
                val started = System.nanoTime()
                for (i in 0 until count) {
                    activity.renderMessage(BODY, MessageDirection.SENT, MessageStatus.SENT)
                }
                elapsed = System.nanoTime() - started
            }
        }
        return elapsed
    }

    private fun Double.format() = String.format("%.1f", this)

    private companion object {
        const val SMALL = 1_000
        const val MAX_RATIO = 15.0

        /** A realistic message body length; the defect's cost is driven by total transcript chars. */
        const val BODY = "a reasonably typical chat message body, about so long"
    }
}
