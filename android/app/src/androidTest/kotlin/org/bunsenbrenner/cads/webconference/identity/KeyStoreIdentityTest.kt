package org.bunsenbrenner.cads.webconference.identity

import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Instrumented (needs a real Android runtime for Keystore/EncryptedSharedPreferences --
 * cannot run as a plain JVM unit test). NOT YET RUN: written without a local Android
 * SDK/emulator available (see KeyStoreIdentity.kt's own doc comment). Uses a fake key
 * generator (no dependency on the not-yet-wired rust-core .aar) so this only exercises
 * the Keystore persistence logic itself, which is the actual gap-3 fix under test.
 */
@RunWith(AndroidJUnit4::class)
class KeyStoreIdentityTest {

    private fun freshIdentity() = KeyStoreIdentity(ApplicationProvider.getApplicationContext())

    @Test
    fun holderIdentityIsAbsentBeforeGeneration() {
        val identity = freshIdentity()
        identity.forgetHolderIdentity() // clean slate; a prior test run may have left one
        assertFalse(identity.hasHolderIdentity())
        assertNull(identity.holderPrivateHex())
    }

    @Test
    fun generatedHolderIdentityRoundTripsThroughEncryptedStorage() {
        val identity = freshIdentity()
        val fake = HexKeypair(publicHex = "aa".repeat(32), privateHex = "bb".repeat(32))

        val stored = identity.generateAndStoreHolderIdentity { fake }
        assertEquals(fake, stored)

        assertTrue(identity.hasHolderIdentity())
        assertEquals(fake.publicHex, identity.holderPublicHex())
        assertEquals(fake.privateHex, identity.holderPrivateHex())

        identity.forgetHolderIdentity()
        assertFalse(identity.hasHolderIdentity())
    }

    @Test
    fun holderAndNoiseIdentitiesAreStoredIndependently() {
        val identity = freshIdentity()
        val holder = HexKeypair(publicHex = "11".repeat(32), privateHex = "22".repeat(32))
        val noise = HexKeypair(publicHex = "33".repeat(32), privateHex = "44".repeat(32))

        identity.generateAndStoreHolderIdentity { holder }
        identity.generateAndStoreNoiseIdentity { noise }

        // Forgetting one must not touch the other -- same independence guarantee the web
        // demo's README documents for its own "Forget this identity" action.
        identity.forgetHolderIdentity()
        assertFalse(identity.hasHolderIdentity())
        assertTrue(identity.hasNoiseIdentity())
        assertEquals(noise.privateHex, identity.noisePrivateHex())
    }
}
