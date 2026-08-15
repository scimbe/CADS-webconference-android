# CADS-webconference-android

Native Android port of [CADS-webconference-demo](https://github.com/scimbe/CADS-webconference-demo) —
real Noise_IK/CADS-Tunnel Agent-Fabric video calling, native Rust core via UniFFI instead of a WebView.

See [ARCHITECTURE.md](./ARCHITECTURE.md) for the full design, the four known gaps this port fixes
(broken WebRTC fallback, no TURN, cleartext keys, narrow bridge trust model), and the
channel-to-channel feature.

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

**One known, precisely-diagnosed gap remains**: `rust-core-debug.aar` packages `classes.jar`
correctly but not the native `.so` libraries under `jni/<abi>/` — confirmed via `unzip -l` across
three separate build attempts, including a clean rebuild. Root-caused via
[mozilla/rust-android-gradle#85](https://github.com/mozilla/rust-android-gradle/issues/85):
`mergeDebugJniLibFolders` wasn't depending on `cargoBuild`, so it could snapshot an empty jniLibs
source before the `.so` files existed. Added the documented fix (`mergeDebugJniLibFolders
dependsOn cargoBuild`) — this corrected the task ordering, but a clean rebuild with the fix in
place now hits a *separate*, still-unresolved issue instead: a genuine duplicate registration of
the same jniLibs source directory (`Resource and asset merger: Duplicate resources` on every ABI,
reproduced consistently on a clean build, not just a stale-cache artifact as first suspected).
Matches [mozilla/rust-android-gradle#43](https://github.com/mozilla/rust-android-gradle/issues/43)
("JNI libraries missing in AAR file") — a plugin-side issue, not a config error here. An app
depending on this `.aar` as-is would compile against the Kotlin API but crash with
`UnsatisfiedLinkError` at runtime. Next step: a newer `rust-android-gradle` release, or manually
packaging the `.so` files into `src/main/jniLibs/<abi>/` and finding a way to suppress the
plugin's own automatic (and apparently duplicate) registration.

Not yet done:
- The native-library-packaging gap above.
- `KeyStoreIdentity.kt` (Android Keystore-backed key storage, gap 3) compiles but has no
  instrumented-test run yet (needs a connected device/emulator, not attempted).
- No real call UI beyond the identity-generation status screen.
- TURN (gap 2) has a tested config surface but no real TURN server/credentials behind it.
- Gap 4 (narrow bridge trust model) is server-side, out of this repo's scope.

See [ARCHITECTURE.md](./ARCHITECTURE.md) for the full per-gap detail.
