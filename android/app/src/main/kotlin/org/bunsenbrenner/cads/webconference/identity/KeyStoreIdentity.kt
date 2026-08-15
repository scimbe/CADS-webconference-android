package org.bunsenbrenner.cads.webconference.identity

import android.content.Context
import android.content.SharedPreferences
import androidx.security.crypto.EncryptedSharedPreferences
import androidx.security.crypto.MasterKey

/**
 * ARCHITECTURE.md gap 3: the reference web demo keeps identity private keys in
 * `localStorage` in the clear. This class is the Android fix: private key hex from
 * `rust-core`'s `generateHolderIdentity()`/`generateNoiseIdentity()` (UniFFI bindings,
 * package `uniffi.ct_agent_android` once the cargo-ndk .aar build is wired into this
 * module -- see android/app/build.gradle.kts's TODO) is written straight into
 * [EncryptedSharedPreferences], backed by a hardware-backed [MasterKey] where the device
 * supports it (minSdk 26, set in build.gradle.kts specifically for this), and is never
 * held in a plain Kotlin `val`/field beyond the single call that generates and persists it.
 *
 * NOT YET COMPILED OR TESTED: no Android SDK/Gradle/Kotlin toolchain is available in the
 * environment this was written in (verified: only `javac` present, no `gradle`/`kotlinc`,
 * `$ANDROID_HOME`/`$ANDROID_SDK_ROOT` unset). Written carefully against the documented
 * `androidx.security.crypto` API, but needs a real instrumented test run (see
 * [KeyStoreIdentityTest], also new, also unverified) before this is trusted in a build.
 */
class KeyStoreIdentity(context: Context) {

    private val prefs: SharedPreferences = run {
        val masterKey = MasterKey.Builder(context)
            .setKeyScheme(MasterKey.KeyScheme.AES256_GCM)
            .build()
        EncryptedSharedPreferences.create(
            context,
            PREFS_FILE_NAME,
            masterKey,
            EncryptedSharedPreferences.PrefKeyEncryptionScheme.AES256_SIV,
            EncryptedSharedPreferences.PrefValueEncryptionScheme.AES256_GCM,
        )
    }

    /**
     * True once a holder identity has been generated and persisted. The caller should
     * check this before [holderPublicHex]/[holderPrivateHex] to distinguish "no identity
     * yet" from a genuinely missing/corrupted store.
     */
    fun hasHolderIdentity(): Boolean = prefs.contains(KEY_HOLDER_PRIVATE)

    fun hasNoiseIdentity(): Boolean = prefs.contains(KEY_NOISE_PRIVATE)

    /**
     * Generate a fresh holder identity via the Rust core and persist it immediately --
     * the private key hex returned by the FFI call exists as a plain Kotlin `String` only
     * for the duration of this function, never stored in a field, never logged.
     *
     * TODO: replace the placeholder call once `uniffi.ct_agent_android.generateHolderIdentity()`
     * is available from the wired-in .aar (rust-core is UniFFI-scaffolded and tested, see
     * rust-core/src/lib.rs, but not yet cross-compiled for Android -- that's the remaining
     * build-wiring step, not a code gap in rust-core itself).
     */
    fun generateAndStoreHolderIdentity(generate: () -> HexKeypair): HexKeypair {
        val identity = generate()
        prefs.edit()
            .putString(KEY_HOLDER_PUBLIC, identity.publicHex)
            .putString(KEY_HOLDER_PRIVATE, identity.privateHex)
            .apply()
        return identity
    }

    fun generateAndStoreNoiseIdentity(generate: () -> HexKeypair): HexKeypair {
        val identity = generate()
        prefs.edit()
            .putString(KEY_NOISE_PUBLIC, identity.publicHex)
            .putString(KEY_NOISE_PRIVATE, identity.privateHex)
            .apply()
        return identity
    }

    fun holderPublicHex(): String? = prefs.getString(KEY_HOLDER_PUBLIC, null)
    fun holderPrivateHex(): String? = prefs.getString(KEY_HOLDER_PRIVATE, null)
    fun noisePublicHex(): String? = prefs.getString(KEY_NOISE_PUBLIC, null)
    fun noisePrivateHex(): String? = prefs.getString(KEY_NOISE_PRIVATE, null)

    /** "Forget this identity", mirroring the web demo's own control (see its README). */
    fun forgetHolderIdentity() {
        prefs.edit().remove(KEY_HOLDER_PUBLIC).remove(KEY_HOLDER_PRIVATE).apply()
    }

    fun forgetNoiseIdentity() {
        prefs.edit().remove(KEY_NOISE_PUBLIC).remove(KEY_NOISE_PRIVATE).apply()
    }

    companion object {
        private const val PREFS_FILE_NAME = "ct_identity_encrypted_prefs"
        private const val KEY_HOLDER_PUBLIC = "holder_public_hex"
        private const val KEY_HOLDER_PRIVATE = "holder_private_hex"
        private const val KEY_NOISE_PUBLIC = "noise_public_hex"
        private const val KEY_NOISE_PRIVATE = "noise_private_hex"
    }
}

/** Plain hex keypair shape -- mirrors rust-core's `HolderIdentity`/`NoiseIdentity` UniFFI records. */
data class HexKeypair(val publicHex: String, val privateHex: String)
