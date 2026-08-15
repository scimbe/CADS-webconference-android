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
- `android/app` depends on `project(":rust-core")`, and `rust-core/build.gradle.kts` wires
  `cargo-ndk` + UniFFI Kotlin bindgen (rust-android-gradle plugin). **Partially verified**: built a
  real Gradle+Android-SDK Docker image and ran `gradle tasks` against this project — plugin ids/
  versions resolve and all Gradle DSL is syntactically valid (no config errors), confirmed by
  reaching rust-android-gradle's own NDK-presence check before failing with a clean
  `NDK is not installed` (the NDK component was deliberately left uninstalled, large/slow download).
  Still unverified: an actual `:rust-core:assembleDebug` (real cargo-ndk cross-compile + UniFFI
  codegen + Kotlin compile), which needs the NDK added to the image first.
- Nothing in the app UI calls the generated `uniffi.ct_agent_android.*` bindings yet, or
  `KeyStoreIdentity.kt` (Android Keystore-backed key storage, gap 3, also unverified for the
  same toolchain reason).
- No call UI. `MainActivity` is a placeholder screen.
- TURN (gap 2) has a tested config surface but no real TURN server/credentials behind it.
- Gap 4 (narrow bridge trust model) is server-side, out of this repo's scope.

See [ARCHITECTURE.md](./ARCHITECTURE.md) for the full per-gap detail.
