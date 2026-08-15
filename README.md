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

**One known, precisely-diagnosed gap remains**: the final `rust-core-debug.aar` packages
`classes.jar` (the real, compiled Kotlin bindings) correctly, but does **not** include the native
`.so` libraries under `jni/<abi>/` — confirmed by inspecting the `.aar` with `unzip -l`, twice,
across two separate build attempts. This matches a known, documented upstream bug in
`rust-android-gradle` itself ([mozilla/rust-android-gradle#43](https://github.com/mozilla/rust-android-gradle/issues/43),
"JNI libraries missing in AAR file on first build") — not a config error on this project's side.
The documented single-rebuild workaround did not resolve it here (`gradle` marked the bundling
task `UP-TO-DATE` and skipped it; forcing a full re-run with `--rerun-tasks` reproduced the
*duplicate resources* error from earlier in this same debugging session instead, suggesting this
plugin version double-registers the jniLibs source directory under some invocation paths). An app
depending on this `.aar` as-is would compile against the Kotlin API but crash with
`UnsatisfiedLinkError` at runtime. Next step: either a newer `rust-android-gradle` release, or
packaging the `.so` files into `src/main/jniLibs/<abi>/` manually instead of relying on the
plugin's automatic wiring.

Not yet done:
- The native-library-packaging gap above.
- Nothing in the app UI calls the generated `uniffi.ct_agent_android.*` bindings yet, or
  `KeyStoreIdentity.kt` (Android Keystore-backed key storage, gap 3) — both compile now, but
  nothing exercises them at runtime.
- No call UI. `MainActivity` is a placeholder screen.
- TURN (gap 2) has a tested config surface but no real TURN server/credentials behind it.
- Gap 4 (narrow bridge trust model) is server-side, out of this repo's scope.

See [ARCHITECTURE.md](./ARCHITECTURE.md) for the full per-gap detail.
