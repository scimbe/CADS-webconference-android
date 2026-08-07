package org.bunsenbrenner.webconference

import androidx.test.core.app.ApplicationProvider
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import uniffi.native_bridge.TextMessage

/**
 * Proves [MessageStore]'s own SQL logic against Robolectric's real SQLite
 * implementation (not a mock) -- a real `Context` from [ApplicationProvider], a
 * real on-device-shaped database file, real inserts/queries. No FFI/native
 * bridge involved at all, so unlike [MainActivityTest]/[TextMessageBridgeTest]
 * there's no [LinkageError] branch here -- this is fully exercisable under
 * Robolectric.
 */
@RunWith(RobolectricTestRunner::class)
class MessageStoreTest {

    private fun message(msgId: String, sender: String, timestamp: ULong, body: String) =
        TextMessage(msgId = msgId, senderPubkey = sender, timestamp = timestamp, body = body)

    @Test
    fun loadAllReturnsPersistedMessagesOldestFirstWithTheirRealDirection() {
        val store = MessageStore(ApplicationProvider.getApplicationContext())
        store.insert(message("1", "aa", 100u, "first"), MessageDirection.SENT)
        store.insert(message("2", "bb", 200u, "second"), MessageDirection.RECEIVED)
        store.insert(message("3", "aa", 50u, "earliest"), MessageDirection.SENT)

        val history = store.loadAll()
        assertEquals(listOf("earliest", "first", "second"), history.map { it.body })
        assertEquals(
            listOf(MessageDirection.SENT, MessageDirection.SENT, MessageDirection.RECEIVED),
            history.map { it.direction },
        )
    }

    @Test
    fun insertingTheSameMsgIdTwiceReplacesRatherThanDuplicatesOrCrashes() {
        val store = MessageStore(ApplicationProvider.getApplicationContext())
        store.insert(message("dup", "aa", 1u, "original"), MessageDirection.SENT)
        // Same msg_id, real at-least-once-delivery retry shape -- must overwrite,
        // not throw a primary-key-conflict exception, not silently duplicate.
        store.insert(message("dup", "aa", 1u, "retried body"), MessageDirection.SENT)

        val history = store.loadAll()
        assertEquals(1, history.size)
        assertEquals("retried body", history[0].body)
    }

    @Test
    fun aFreshDatabaseStartsWithNoHistory() {
        val store = MessageStore(ApplicationProvider.getApplicationContext())
        assertEquals(emptyList<StoredMessage>(), store.loadAll())
    }

    @Test
    /** Requirement #4's first bounded slice: a real, persisted per-message status
     * round-trips through the real database, and a [MessageDirection.RECEIVED] row
     * (which never carries a status) reloads honestly as `null`, not a fabricated value. */
    fun sentMessageStatusRoundTripsRealAndReceivedMessagesHaveNoStatus() {
        val store = MessageStore(ApplicationProvider.getApplicationContext())
        store.insert(message("ok", "aa", 1u, "went through"), MessageDirection.SENT, MessageStatus.SENT)
        store.insert(message("bad", "aa", 2u, "never made it"), MessageDirection.SENT, MessageStatus.FAILED)
        store.insert(message("rx", "bb", 3u, "from the peer"), MessageDirection.RECEIVED)

        val history = store.loadAll()
        assertEquals(listOf(MessageStatus.SENT, MessageStatus.FAILED, null), history.map { it.status })
    }

    @Test
    /** A retried send (the same real msg_id, now succeeding) must overwrite the earlier
     * FAILED row's status too, not just its body -- the same real CONFLICT_REPLACE
     * contract [insertingTheSameMsgIdTwiceReplacesRatherThanDuplicatesOrCrashes] already
     * proves for body, extended to status now that a row can carry one. */
    fun retryingAFailedSendReplacesItsStatusNotJustItsBody() {
        val store = MessageStore(ApplicationProvider.getApplicationContext())
        store.insert(message("retry", "aa", 1u, "first attempt"), MessageDirection.SENT, MessageStatus.FAILED)
        store.insert(message("retry", "aa", 1u, "first attempt"), MessageDirection.SENT, MessageStatus.SENT)

        val history = store.loadAll()
        assertEquals(1, history.size)
        assertEquals(MessageStatus.SENT, history[0].status)
    }

    @Test
    fun historyReallyPersistsAcrossReopeningTheStore() {
        val context = ApplicationProvider.getApplicationContext<android.content.Context>()
        MessageStore(context).use { it.insert(message("p", "aa", 1u, "persisted across reopen"), MessageDirection.RECEIVED) }

        // A second, independent MessageStore instance against the same real
        // on-disk database file -- proves this is real persistence, not an
        // in-memory value that only survives within one object's lifetime.
        val reopened = MessageStore(context)
        assertEquals(listOf("persisted across reopen"), reopened.loadAll().map { it.body })
    }
}
