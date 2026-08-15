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

**The Rust core actually cross-compiles for Android — verified, not assumed.** With the NDK added
to the Docker image, `cargo ndk -t arm64-v8a build` inside `rust-core/` succeeds and produces
`target/aarch64-linux-android/debug/libct_agent_android.so` — confirmed via `readelf -h` to be a
real `ELF64 AArch64` shared library, not just a file in the right directory. `gradle tasks` also
resolves cleanly against the full project (plugin ids/versions, all DSL) with the NDK present.

Not yet done:
- `:rust-core:assembleDebug` itself (the full Gradle-orchestrated build: cargo-ndk for all 4 ABIs
  + UniFFI Kotlin bindgen + packaging as an Android library) hasn't been run yet — only the raw
  `cargo ndk build` step (the highest-risk, most novel part) and a plain `gradle tasks` sync have
  been verified individually so far.
- Nothing in the app UI calls the generated `uniffi.ct_agent_android.*` bindings yet, or
  `KeyStoreIdentity.kt` (Android Keystore-backed key storage, gap 3, still unverified — needs an
  actual Gradle Kotlin-compile pass, not yet attempted).
- No call UI. `MainActivity` is a placeholder screen.
- TURN (gap 2) has a tested config surface but no real TURN server/credentials behind it.
- Gap 4 (narrow bridge trust model) is server-side, out of this repo's scope.

See [ARCHITECTURE.md](./ARCHITECTURE.md) for the full per-gap detail.
