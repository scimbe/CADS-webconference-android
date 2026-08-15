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
- The Android app itself doesn't call into `rust-core` yet — `implementation(project(":rust-core"))`
  in `android/app/build.gradle.kts` is still a TODO; the `cargo-ndk` cross-compile step that
  produces the `.aar` isn't wired in.
- `KeyStoreIdentity.kt` (Android Keystore-backed key storage, gap 3) is written but **not
  compiled or tested** — no Android/Gradle/Kotlin toolchain was available in the environment
  it was written in (verified: only `javac` present, no SDK).
- No call UI. `MainActivity` is a placeholder screen.
- TURN (gap 2) has a tested config surface but no real TURN server/credentials behind it.
- Gap 4 (narrow bridge trust model) is server-side, out of this repo's scope.

See [ARCHITECTURE.md](./ARCHITECTURE.md) for the full per-gap detail.
