# CADS-webconference-android

Native Android port of [CADS-webconference-demo](https://github.com/scimbe/CADS-webconference-demo) —
real Noise_IK/CADS-Tunnel Agent-Fabric video calling, native Rust core via UniFFI instead of a WebView.

See [ARCHITECTURE.md](./ARCHITECTURE.md) for the full design, the four known gaps this port fixes
(broken WebRTC fallback, no TURN, cleartext keys, narrow bridge trust model), and the
channel-to-channel feature.

**License:** [PolyForm Noncommercial 1.0.0](./LICENSE), matching `ct-common`
(`scimbe/CADS-Tunnel`), which `rust-core` depends on directly — a more permissive license
(e.g. MIT) for this repo wouldn't accurately cover the `ct-common` code compiled into any
resulting APK regardless of what license wrapped it.

## Status

`rust-core` is functionally complete and CI-verified: identity generation, the full
Noise_IK handshake/transport, channel-join + signing, WebRTC signal encoding, ICE/TURN
config, and the transport-fallback state machine are all ported/implemented and covered
by 17 passing tests, run both locally and in the actual GitHub Actions CI (`docker
compose run test`, not just `cargo test` on the host) — see [`.github/workflows/ci.yml`](.github/workflows/ci.yml).

**`gradle :rust-core:assembleDebug` runs end-to-end — verified, not assumed**, through a real
debugging session (each fix live-caught, not guessed): cross-compiles for all 4 Android ABIs,
runs UniFFI's Kotlin codegen against the compiled library, and compiles the generated Kotlin
against it. Along the way this surfaced and fixed four real, distinct build bugs: AGP's NDK
auto-detection needing an explicit `ndkVersion` in the DSL (env vars alone aren't enough), the
`rust-android-gradle` linker wrapper needing `python` on `PATH`, a missing `[[bin]] uniffi-bindgen`
target (the `uniffi` crate's `"cli"` feature does not auto-provide one), a JVM-target mismatch
between `compileDebugJavaWithJavac` (1.8) and Kotlin (17), and a missing `net.java.dev.jna:jna`
dependency (UniFFI's generated Kotlin needs JNA's `Pointer` type).

**`MainActivity` now calls the real generated bindings** (`generateHolderIdentity()`/
`generateNoiseIdentity()`, persisted via `KeyStoreIdentity`) — `gradle :app:compileDebugKotlin`
is verified `BUILD SUCCESSFUL` against them. Found and fixed two more real bugs doing so: a
missing `android/gradle.properties` (`android.useAndroidX=true` was never set), and
`org.webrtc:google-webrtc:1.0.32006` not existing on any real repo (only ever distributed via
the long-shut-down JCenter) — replaced with `com.infobip:google-webrtc:1.0.45036`.

**RESOLVED — this repo now builds a real, complete, installable APK.** The native-lib-packaging
gap (`rust-core-debug.aar` missing `.so` files) was chased through `mozilla/rust-android-gradle`
across three rounds (task-dependency fixes, source-code inspection) without success — instead of
continuing to fight that plugin's unresolved bug
([mozilla/rust-android-gradle#43](https://github.com/mozilla/rust-android-gradle/issues/43)), it
was removed entirely. `rust-core/build.gradle.kts` now cross-compiles directly via
`cargo ndk -o src/main/jniLibs build` (no plugin needed — that's cargo-ndk's own `-o` flag writing
straight into the directory layout AGP's `com.android.library` plugin already understands
natively). Verified end-to-end: `gradle :rust-core:assembleDebug` produces an `.aar` with real
`jni/<abi>/libct_agent_android.so` for all 4 ABIs (confirmed via `unzip -l`), and
`gradle :app:assembleDebug` produces a real 286MB `app-debug.apk` containing all dex classes, our
native lib, `google-webrtc`'s native lib, and JNA's — also confirmed via `unzip -l` on the actual
APK, not assumed from a successful exit code.

Not yet done:
- `KeyStoreIdentity.kt` (Android Keystore-backed key storage, gap 3) compiles and is packaged but
  has no instrumented-test run yet (needs a connected device/emulator, not attempted).
- No real call UI beyond the identity-generation status screen.
- TURN (gap 2) has a tested config surface but no real TURN server/credentials behind it.
- Gap 4 (narrow bridge trust model) is server-side, out of this repo's scope.
- The `.so` libraries are currently unstripped debug builds (~55-61MB each) — `strip` wasn't
  available in the NDK toolchain path used, so release builds would need that fixed for a
  reasonably-sized production APK.

See [ARCHITECTURE.md](./ARCHITECTURE.md) for the full per-gap detail.
