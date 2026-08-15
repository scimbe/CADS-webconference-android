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

Not yet done:
- `android/app` now depends on `project(":rust-core")`, and `rust-core/build.gradle.kts` wires
  `cargo-ndk` + UniFFI Kotlin bindgen (rust-android-gradle plugin) — but this is **written, not
  verified**: no Gradle/Android/NDK toolchain was available to actually run a build. Needs a real
  `./gradlew :rust-core:assembleDebug` before it's trusted; the plugin API may have moved on.
- Nothing in the app UI calls the generated `uniffi.ct_agent_android.*` bindings yet, or
  `KeyStoreIdentity.kt` (Android Keystore-backed key storage, gap 3, also unverified for the
  same toolchain reason).
- No call UI. `MainActivity` is a placeholder screen.
- TURN (gap 2) has a tested config surface but no real TURN server/credentials behind it.
- Gap 4 (narrow bridge trust model) is server-side, out of this repo's scope.

See [ARCHITECTURE.md](./ARCHITECTURE.md) for the full per-gap detail.
