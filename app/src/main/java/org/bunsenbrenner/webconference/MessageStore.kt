package org.bunsenbrenner.webconference

import android.content.ContentValues
import android.content.Context
import android.database.sqlite.SQLiteDatabase
import android.database.sqlite.SQLiteOpenHelper
import uniffi.native_bridge.TextMessage

/** Which side of a real [TextMessage] this device was on. */
enum class MessageDirection { SENT, RECEIVED }

/** One real, persisted message -- just what the thread view needs to render. */
data class StoredMessage(val body: String, val direction: MessageDirection)

/**
 * Real local persistence for the message thread -- "Verlauf lokal persistiert",
 * the run's own declared M1 milestone (`runs/webconference-android/state.json`).
 *
 * A real, stated substitution: the backlog item that scoped this named "Room" as
 * the storage choice; this uses plain `SQLiteOpenHelper` instead. Room needs a
 * KSP (or kapt) annotation-processor plugin pinned to this exact Kotlin/AGP
 * toolchain version (this project is on AGP 9.3.1's built-in Kotlin 2.2.10,
 * verified via a real build, not a stable/long-published KSP pairing at time of
 * writing) -- a new real failure surface this project's established
 * minimal-dependency, no-XML-layout convention doesn't need for one table and
 * two queries. `SQLiteOpenHelper` is already part of the Android platform: zero
 * new dependency, zero new Gradle plugin, zero version-pairing risk. If a real
 * later need (more tables, real query complexity) makes Room's compile-time
 * verification worth that toolchain risk, that's its own explicit decision to
 * make then, not a default now.
 */
class MessageStore(context: Context) : SQLiteOpenHelper(context, DB_NAME, null, DB_VERSION) {

    override fun onCreate(db: SQLiteDatabase) {
        db.execSQL(
            "CREATE TABLE messages (" +
                "msg_id TEXT PRIMARY KEY, " +
                "sender_pubkey TEXT NOT NULL, " +
                "timestamp INTEGER NOT NULL, " +
                "body TEXT NOT NULL, " +
                "direction TEXT NOT NULL)"
        )
    }

    override fun onUpgrade(db: SQLiteDatabase, oldVersion: Int, newVersion: Int) {
        db.execSQL("DROP TABLE IF EXISTS messages")
        onCreate(db)
    }

    /**
     * Persists one real message. `CONFLICT_REPLACE` on `msg_id` -- a duplicate
     * real-time receive of the same message (the channel's own at-least-once
     * framing, per `TextMessage.msgId`'s own doc comment on why it exists at
     * all: dedup) overwrites the existing row instead of throwing, so a retried
     * receive never crashes the app.
     */
    fun insert(message: TextMessage, direction: MessageDirection) {
        val values = ContentValues().apply {
            put("msg_id", message.msgId)
            put("sender_pubkey", message.senderPubkey)
            put("timestamp", message.timestamp.toLong())
            put("body", message.body)
            put("direction", direction.name)
        }
        writableDatabase.insertWithOnConflict("messages", null, values, SQLiteDatabase.CONFLICT_REPLACE)
    }

    /** Real persisted history, oldest first -- matches the live thread's own append-at-the-bottom order. */
    fun loadAll(): List<StoredMessage> {
        val results = mutableListOf<StoredMessage>()
        readableDatabase.query("messages", arrayOf("body", "direction"), null, null, null, null, "timestamp ASC").use { cursor ->
            val bodyIndex = cursor.getColumnIndexOrThrow("body")
            val directionIndex = cursor.getColumnIndexOrThrow("direction")
            while (cursor.moveToNext()) {
                results.add(StoredMessage(body = cursor.getString(bodyIndex), direction = MessageDirection.valueOf(cursor.getString(directionIndex))))
            }
        }
        return results
    }

    companion object {
        private const val DB_NAME = "messages.db"
        private const val DB_VERSION = 1
    }
}
