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

**One known, precisely-diagnosed gap remains**: `rust-core-debug.aar` packages `classes.jar`
correctly but not the native `.so` libraries under `jni/<abi>/` — confirmed via `unzip -l` across
three separate build attempts, including a clean rebuild. Root-caused via
[mozilla/rust-android-gradle#85](https://github.com/mozilla/rust-android-gradle/issues/85):
`mergeDebugJniLibFolders` wasn't depending on `cargoBuild`, so it could snapshot an empty jniLibs
source before the `.so` files existed. Added the documented fix (`mergeDebugJniLibFolders
dependsOn cargoBuild`) — this corrected the task ordering, but a clean rebuild with the fix in
place still hits a *separate* issue: a genuine duplicate registration of the same jniLibs source
directory (`Resource and asset merger: Duplicate resources` on every ABI, reproduced consistently
on multiple clean builds, not a stale-cache artifact). Tried a second, community-confirmed fix
from the same upstream thread (`preDebugBuild`/`preReleaseBuild dependsOn cargoBuild` instead of
the merge task directly) — same result. Checked the plugin's actual source
(`RustAndroidPlugin.kt` v0.9.6): it registers `sourceSets.getByName("main").jniLibs.srcDir(...)`
exactly **once** — so the duplicate isn't a double-registration in the plugin itself, and the
real cause is still unidentified (likely an AGP-version/plugin-version interaction rather than
anything this project's config controls). Matches
[mozilla/rust-android-gradle#43](https://github.com/mozilla/rust-android-gradle/issues/43). Spent
three focused rounds on this; stopping here rather than continuing to chase it — an app depending
on this `.aar` as-is would compile against the Kotlin API but crash with `UnsatisfiedLinkError` at
runtime. Real next steps, not yet tried: pin an older/newer `rust-android-gradle` version, switch
to the actively-maintained `willir/cargo-ndk-android-gradle` fork (mentioned favorably in the same
upstream thread), or hand-roll `.aar` assembly without this plugin's jniLibs wiring at all.

Not yet done:
- The native-library-packaging gap above.
- `KeyStoreIdentity.kt` (Android Keystore-backed key storage, gap 3) compiles but has no
  instrumented-test run yet (needs a connected device/emulator, not attempted).
- No real call UI beyond the identity-generation status screen.
- TURN (gap 2) has a tested config surface but no real TURN server/credentials behind it.
- Gap 4 (narrow bridge trust model) is server-side, out of this repo's scope.

See [ARCHITECTURE.md](./ARCHITECTURE.md) for the full per-gap detail.
