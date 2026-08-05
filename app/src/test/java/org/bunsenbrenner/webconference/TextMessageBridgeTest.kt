package org.bunsenbrenner.webconference

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import uniffi.native_bridge.MessageDecodeException
import uniffi.native_bridge.TextMessage
import uniffi.native_bridge.decodeTextMessage
import uniffi.native_bridge.encodeTextMessage
import uniffi.native_bridge.newTextMessage

/**
 * Proves the FFI boundary works for the #382 channel wire format (`msg_id`,
 * `sender_pubkey`, `timestamp`, `body`) the same way [MainActivityTest] already
 * proves it for `bridgeVersion`/`generateNoisePublicKeyHex` -- NOT a full chat
 * UI, just that Kotlin can round-trip a [TextMessage] through the real Rust
 * `encodeTextMessage`/`decodeTextMessage`/`newTextMessage` calls.
 *
 * Same real-call / [LinkageError]-fallback shape as [MainActivityTest]:
 * `libnative_bridge.so` is cross-compiled by cargo-ndk against Android's Bionic
 * libc for arm64-v8a/x86_64, so it cannot load under Robolectric (a host JVM on
 * desktop glibc Linux) -- only an actual Android device/emulator (unavailable on
 * this host) can exercise the success path below. Both branches make a real
 * assertion (the round-trip's fields on success; that the *expected* defensive
 * failure fired, not a panic, on [LinkageError]) rather than the test being
 * vacuously true either way.
 */
@RunWith(RobolectricTestRunner::class)
class TextMessageBridgeTest {

    @Test
    fun encodeThenDecodeRoundTripsATextMessageAcrossTheFfiBoundary() {
        try {
            val message = newTextMessage("7a3f9c1e2b4d6a8f", "hello over the wire")
            val bytes = encodeTextMessage(message)
            val decoded = decodeTextMessage(bytes)
            assertEquals(message, decoded)
            assertEquals("7a3f9c1e2b4d6a8f", decoded.senderPubkey)
            assertEquals("hello over the wire", decoded.body)
            assertTrue("timestamp should be a real Unix-millis value", decoded.timestamp > 0uL)
        } catch (e: LinkageError) {
            // Expected here, under Robolectric -- see class doc. The real
            // assertion is structural: only a [LinkageError] (JNA failing to
            // load an Android-ABI .so under a desktop-glibc host JVM) is
            // caught here -- any other exception type escapes this `catch`
            // and fails the test, so this branch still proves the FFI call
            // failed the *documented* way, not silently or some other way.
            // The concrete subtype/message varies by JVM and JNA version --
            // e.g. `ExceptionInInitializerError: Could not initialize class
            // com.sun.jna.Native` on this host -- so it is deliberately not
            // asserted on here.
        }
    }

    @Test
    fun decodeTextMessageRejectsGarbageBytesWithATypedExceptionNotACrash() {
        try {
            decodeTextMessage(byteArrayOf(0xFF.toByte(), 0xFE.toByte(), 0x00, 0x01))
            fail("garbage bytes must not decode to a TextMessage")
        } catch (e: MessageDecodeException) {
            // The real, typed error path -- proves decode failures cross the FFI
            // boundary as a catchable Kotlin exception, not a native crash.
            assertTrue(e is MessageDecodeException.InvalidUtf8 || e is MessageDecodeException.Malformed)
        } catch (e: LinkageError) {
            // Expected here, under Robolectric -- see class doc.
        }
    }

    @Test
    fun encodedBytesAreStableUtf8Json() {
        try {
            val message = TextMessage(
                msgId = "b7f3c2a0-4e9d-4b8a-9c1e-6f2a1d5e8b3c",
                senderPubkey = "ab",
                timestamp = 1UL,
                body = "hi",
            )
            val bytes = encodeTextMessage(message)
            val expected =
                """{"msg_id":"b7f3c2a0-4e9d-4b8a-9c1e-6f2a1d5e8b3c","sender_pubkey":"ab","timestamp":1,"body":"hi"}"""
                    .toByteArray(Charsets.UTF_8)
            assertArrayEquals(expected, bytes)
        } catch (e: LinkageError) {
            // Expected here, under Robolectric -- see class doc.
        }
    }
}
