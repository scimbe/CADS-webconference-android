package org.bunsenbrenner.webconference

import android.os.Bundle
import android.text.method.ScrollingMovementMethod
import android.view.Gravity
import android.widget.Button
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import androidx.annotation.VisibleForTesting
import androidx.appcompat.app.AppCompatActivity
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import uniffi.native_bridge.ChannelException
import uniffi.native_bridge.ChannelIdentity
import uniffi.native_bridge.ChannelSession
import uniffi.native_bridge.bindChannelListener
import uniffi.native_bridge.bridgeVersion
import uniffi.native_bridge.dialChannelDirect
import uniffi.native_bridge.generateChannelIdentity
import uniffi.native_bridge.generateNoisePublicKeyHex
import uniffi.native_bridge.newTextMessage

/**
 * The pipeline's (The Development System, CADS-Tunnel#382) minimal real chat client:
 * generate a Noise_IK channel identity, either listen for a peer or dial one directly,
 * then send/receive real text messages over the resulting [ChannelSession] -- the
 * actual thing a real emulator walkthrough (labor-setup.com, issue #13) needs to drive
 * to prove the run's own declared milestone ("1:1 Text-Messaging end-to-end").
 *
 * Still deliberately minimal: direct-address only (both sides exchange public
 * key/address out of band -- copy/paste between two devices), no broker-mediated
 * discovery yet (see the run's own backlog). Message history IS now really
 * persisted locally (see [MessageStore]), closing the last piece of the run's
 * declared M1 milestone alongside this connect/send/receive flow. What's here is
 * real, not a mock: every button drives an actual `uniffi.native_bridge` FFI call
 * into `libnative_bridge.so`.
 *
 * Every native call is guarded with the same [LinkageError] fallback the earlier
 * scaffold established (`nativeBridgeStatusLine`'s doc comment): `libnative_bridge.so`
 * can only load on a real Android device/emulator, never under Robolectric's host JVM
 * (desktop glibc, not Bionic). Unit tests here can only exercise these defensive
 * fallback paths -- proving the real connect/send/receive success path needs an actual
 * emulator, which is exactly what issue #13 is for.
 */
class MainActivity : AppCompatActivity() {

    private var identity: ChannelIdentity? = null
    private var myPublicKeyHex: String = ""
    private var session: ChannelSession? = null

    // internal, not private -- the real test proving onDestroy() actually closes this needs to
    // reach the live SQLiteDatabase handle directly, the same real visibility precedent
    // resetForNewConnection below already established for this test class.
    internal lateinit var messageStore: MessageStore
    private lateinit var identityText: TextView
    private lateinit var startListeningButton: Button
    private lateinit var peerPublicKeyInput: EditText
    private lateinit var peerAddressInput: EditText
    private lateinit var connectButton: Button
    private lateinit var connectionStatusText: TextView
    private lateinit var messagesText: TextView
    private lateinit var messageInput: EditText
    private lateinit var sendButton: Button

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        messageStore = MessageStore(this)
        setContentView(buildLayout())
        initializeChannelIdentity()
        loadPersistedHistory()
    }

    /**
     * Real gap found live (#382, DAU lens -- surfaced by `devsystem.assistant`, independently
     * corroborating a finding this run's own history had already flagged): [MessageStore] is a
     * real [android.database.sqlite.SQLiteOpenHelper], which keeps a real, cacheable
     * `SQLiteDatabase` handle open once `writableDatabase`/`readableDatabase` is first touched --
     * nothing ever called `close()` on it. Every real `onCreate` (a config change, e.g. rotation,
     * genuinely destroys and recreates the Activity) opened a fresh [MessageStore] backed by a
     * fresh handle to the same on-disk file without ever releasing the previous one, a real,
     * unbounded per-instance leak over the process's lifetime.
     */
    override fun onDestroy() {
        messageStore.close()
        super.onDestroy()
    }

    /** Real persisted history from a previous session, rendered before anything new arrives. */
    private fun loadPersistedHistory() {
        lifecycleScope.launch {
            val history = withContext(Dispatchers.IO) { messageStore.loadAll() }
            history.forEach { renderMessage(it.body, it.direction, it.status) }
        }
    }

    /**
     * Programmatic layout, matching the earlier scaffold's convention (no XML layout
     * files in this project yet) -- a vertically scrolling column of the status
     * section, the identity/listen section, the connect section, and the message
     * thread, in that order.
     */
    private fun buildLayout(): ScrollView {
        val horizontal = resources.getDimensionPixelSize(R.dimen.scaffold_padding_horizontal)
        val top = resources.getDimensionPixelSize(R.dimen.scaffold_padding_top)
        val bottom = resources.getDimensionPixelSize(R.dimen.scaffold_padding_bottom)

        val root = LinearLayout(this)
        root.orientation = LinearLayout.VERTICAL
        root.setPadding(horizontal, top, horizontal, bottom)

        val status = TextView(this)
        status.id = R.id.scaffold_status_text
        status.text = getString(R.string.status_scaffold) + "\n\n" + nativeBridgeStatusLine()
        root.addView(status)

        identityText = TextView(this)
        identityText.id = R.id.my_identity_text
        identityText.setPadding(0, top, 0, 0)
        identityText.setTextIsSelectable(true)
        root.addView(identityText)

        startListeningButton = Button(this)
        startListeningButton.id = R.id.start_listening_button
        startListeningButton.text = getString(R.string.start_listening)
        startListeningButton.setOnClickListener { onStartListeningClicked() }
        root.addView(startListeningButton)

        peerPublicKeyInput = EditText(this)
        peerPublicKeyInput.id = R.id.peer_public_key_input
        peerPublicKeyInput.hint = getString(R.string.peer_public_key_hint)
        root.addView(peerPublicKeyInput)

        peerAddressInput = EditText(this)
        peerAddressInput.id = R.id.peer_address_input
        peerAddressInput.hint = getString(R.string.peer_address_hint)
        root.addView(peerAddressInput)

        connectButton = Button(this)
        connectButton.id = R.id.connect_button
        connectButton.text = getString(R.string.connect)
        connectButton.setOnClickListener { onConnectClicked() }
        root.addView(connectButton)

        connectionStatusText = TextView(this)
        connectionStatusText.id = R.id.connection_status_text
        root.addView(connectionStatusText)

        messagesText = TextView(this)
        messagesText.id = R.id.messages_text
        messagesText.setPadding(0, top, 0, 0)
        messagesText.setTextIsSelectable(true)
        messagesText.movementMethod = ScrollingMovementMethod()
        root.addView(messagesText)

        val sendRow = LinearLayout(this)
        sendRow.orientation = LinearLayout.HORIZONTAL
        sendRow.gravity = Gravity.CENTER_VERTICAL
        messageInput = EditText(this)
        messageInput.id = R.id.message_input
        messageInput.hint = getString(R.string.message_hint)
        messageInput.layoutParams = LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f)
        sendRow.addView(messageInput)
        sendButton = Button(this)
        sendButton.id = R.id.send_button
        sendButton.text = getString(R.string.send)
        sendButton.setOnClickListener { onSendClicked() }
        sendRow.addView(sendButton)
        root.addView(sendRow)

        val scroll = ScrollView(this)
        scroll.addView(root)
        return scroll
    }

    /**
     * Generates this device's real Noise_IK channel identity once, at launch -- the
     * public half is what gets shared with the peer (out of band) as their
     * `peerPublicKeyHex`. Same [LinkageError] guard as [nativeBridgeStatusLine].
     */
    private fun initializeChannelIdentity() {
        try {
            val id = generateChannelIdentity()
            identity = id
            myPublicKeyHex = id.publicKeyHex()
            identityText.text = getString(R.string.my_identity_label, myPublicKeyHex)
        } catch (e: LinkageError) {
            identityText.text = getString(R.string.identity_unavailable, e.message)
        }
    }

    private fun onStartListeningClicked() {
        val id = identity ?: return
        startListeningButton.isEnabled = false
        lifecycleScope.launch {
            try {
                val listener = bindChannelListener("0.0.0.0:0")
                connectionStatusText.text = getString(R.string.listening_on, listener.localAddr())
                val newSession = listener.accept(id)
                session = newSession
                onConnected()
                receiveLoop(newSession)
            } catch (e: ChannelException) {
                connectionStatusText.text = getString(R.string.connection_failed, e.message)
                startListeningButton.isEnabled = true
            } catch (e: LinkageError) {
                connectionStatusText.text = getString(R.string.connection_failed, e.message)
                startListeningButton.isEnabled = true
            }
        }
    }

    private fun onConnectClicked() {
        val peerKey = peerPublicKeyInput.text.toString().trim()
        val peerAddr = peerAddressInput.text.toString().trim()
        // Real gap found and fixed (#382, DAU lens): tapping Connect with either
        // field blank used to go straight to a real native dialChannelDirect() call
        // and surface whatever error message the Rust side happened to produce for
        // malformed input -- confusing for a careless tap, and a real native FFI
        // round-trip for something checkable locally in a single line. Now caught
        // immediately, before any native call (and before the identity-readiness
        // check below), with a clear, actionable message -- what the user actually
        // typed wrong is the more useful thing to tell them first.
        if (peerKey.isEmpty() || peerAddr.isEmpty()) {
            connectionStatusText.text = getString(R.string.connect_fields_required)
            return
        }
        val id = identity ?: return
        connectButton.isEnabled = false
        connectionStatusText.text = getString(R.string.connecting)
        lifecycleScope.launch {
            try {
                val newSession = dialChannelDirect(id, peerKey, peerAddr)
                session = newSession
                onConnected()
                receiveLoop(newSession)
            } catch (e: ChannelException.InvalidPeerKey) {
                // A mistyped/truncated peer key is ordinary user error, so it gets its own
                // actionable, localized message instead of being folded into the generic
                // "Connection failed: <native reason string>" line.
                connectionStatusText.text = getString(R.string.invalid_peer_key)
                connectButton.isEnabled = true
            } catch (e: ChannelException) {
                connectionStatusText.text = getString(R.string.connection_failed, e.message)
                connectButton.isEnabled = true
            } catch (e: LinkageError) {
                connectionStatusText.text = getString(R.string.connection_failed, e.message)
                connectButton.isEnabled = true
            } catch (e: CancellationException) {
                // Catching Exception below would otherwise swallow ordinary coroutine
                // cancellation (the Activity being destroyed mid-dial) and treat it as a
                // connection failure.
                throw e
            } catch (e: Exception) {
                // Requirement #17: a Rust-side panic does NOT arrive as a ChannelException --
                // UniFFI's generated call wrapper turns it into `InternalException("Rust
                // panic")`, which is a plain kotlin.Exception. Before this catch existed such
                // a panic escaped the lifecycleScope coroutine uncaught and took the process
                // down, so a native bug anywhere under dialChannelDirect crashed the app
                // instead of surfacing here. The native defect that proved this (hex_decode_32
                // slicing a &str at non-char-boundary byte offsets) is fixed in
                // native-bridge/src/channel.rs, but the catch is what keeps the next one from
                // being fatal too. The user-visible text is deliberately generic: an internal
                // panic message is not something a user can act on.
                connectionStatusText.text = getString(R.string.connection_failed_unexpected)
                connectButton.isEnabled = true
            }
        }
    }

    private fun onConnected() {
        connectionStatusText.text = getString(R.string.connected)
        startListeningButton.isEnabled = false
        connectButton.isEnabled = false
    }

    /** Loops on real `recvText()` until the session errors (peer disconnects, I/O fails). */
    private suspend fun receiveLoop(activeSession: ChannelSession) {
        try {
            while (true) {
                val msg = activeSession.recvText()
                withContext(Dispatchers.IO) { messageStore.insert(msg, MessageDirection.RECEIVED) }
                renderMessage(msg.body, MessageDirection.RECEIVED, null)
            }
        } catch (e: ChannelException) {
            resetForNewConnection(getString(R.string.disconnected, e.message))
        }
    }

    private fun onSendClicked() {
        val body = messageInput.text.toString()
        // Real gap found and fixed (#382, DAU lens): tapping Send with an empty or
        // whitespace-only message used to silently do nothing -- no feedback at all,
        // and `.isEmpty()` alone never caught whitespace-only input (" " is not
        // empty by length), so a message consisting of just a space would actually
        // have been sent as real content. Checked before the session-readiness guard
        // below (same lesson as onConnectClicked's own fix: what the user typed
        // wrong is the more useful thing to tell them first, and it keeps this path
        // testable under Robolectric, where `session` is always null) -- a real
        // status message and focus kept on the input for retry.
        if (body.isBlank()) {
            connectionStatusText.text = getString(R.string.empty_message_not_sent)
            messageInput.requestFocus()
            return
        }
        // Real gap found live (#382, DAU lens): a real, non-blank message typed
        // before ever connecting to a peer used to hit `session ?: return` here
        // and vanish completely -- no status update, nothing in the message
        // thread, no indication anything happened at all. A user who hasn't
        // read the connection status text has no way to tell "not connected"
        // apart from "the app is broken." The message itself is preserved (not
        // cleared), so retrying after connecting doesn't require retyping it.
        val activeSession = session ?: run {
            connectionStatusText.text = getString(R.string.not_connected_message_not_sent)
            return
        }
        lifecycleScope.launch {
            val msg = newTextMessage(myPublicKeyHex, body)
            try {
                activeSession.sendText(msg)
                withContext(Dispatchers.IO) { messageStore.insert(msg, MessageDirection.SENT, MessageStatus.SENT) }
                renderMessage(msg.body, MessageDirection.SENT, MessageStatus.SENT)
                messageInput.text.clear()
            } catch (e: ChannelException) {
                // Real gap found live, requirement #4's first bounded slice: a failed
                // send used to vanish completely -- no thread entry, nothing persisted,
                // only the separate connection-status text changed. The user had no way
                // to tell "this specific message never went out" from "it's still
                // somewhere in flight." Now recorded and rendered as a real, honest
                // FAILED entry in the thread itself, not silently dropped -- retyping
                // and resending is still the real recovery path (no queue/retry
                // mechanism exists), but at least the failure is visible where the
                // message would have been.
                withContext(Dispatchers.IO) { messageStore.insert(msg, MessageDirection.SENT, MessageStatus.FAILED) }
                renderMessage(msg.body, MessageDirection.SENT, MessageStatus.FAILED)
                resetForNewConnection(getString(R.string.disconnected, e.message))
            }
        }
    }

    /**
     * Real gap found and closed (#382): [receiveLoop] and [onSendClicked]'s
     * `ChannelException` catch blocks used to only update the status text, leaving
     * `session` set and both connect controls disabled forever after -- so a peer
     * disconnecting (or a send failing after the fact) made the app permanently
     * unusable until force-restarted. This is the one real recovery path for both:
     * drop the dead session and let the user actually start listening or dial again.
     */
    @VisibleForTesting
    internal fun resetForNewConnection(statusMessage: String) {
        connectionStatusText.text = statusMessage
        session = null
        startListeningButton.isEnabled = true
        connectButton.isEnabled = true
    }

    /** Pure UI append -- persistence is a separate, explicit call at each real call site
     * ([onSendClicked], [receiveLoop], [loadPersistedHistory]), never implied by this.
     * [status] is real, honest per-message state (requirement #4's first bounded slice --
     * see [MessageStatus]'s own doc comment for what it does and does not yet claim) --
     * always `null` for [MessageDirection.RECEIVED]. */
    private fun renderMessage(body: String, direction: MessageDirection, status: MessageStatus?) {
        val line = when (direction) {
            MessageDirection.SENT -> when (status) {
                MessageStatus.FAILED -> getString(R.string.message_line_sent_failed, body)
                // SENT (the real, expected case for anything already persisted) and the
                // impossible `null` case (a SENT row must always carry a real status --
                // see insert()'s own contract) both render as the plain sent line;
                // `null` is not reachable from real code but a `when` here must still
                // be exhaustive, and defaulting to the honest, unremarkable case is
                // safer than a fabricated guess at which status was meant.
                else -> getString(R.string.message_line_sent, body)
            }
            MessageDirection.RECEIVED -> getString(R.string.message_line_received, body)
        }
        messagesText.text = if (messagesText.text.isEmpty()) line else "${messagesText.text}\n$line"
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
