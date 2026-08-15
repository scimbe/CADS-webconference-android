# CADS-webconference-android — Architecture

Native Android port of [`scimbe/CADS-webconference-demo`](https://github.com/scimbe/CADS-webconference-demo),
which runs `ct-agent-wasm` (a WASM build of [`scimbe/ct-agent`](https://github.com/scimbe/ct-agent)) in-browser to
join a CADS-Tunnel Agent-Fabric channel: Noise_IK handshake, encrypted WebRTC signaling, real `RTCPeerConnection`.

This project cross-compiles the same Rust core (`ct-agent` / `ct-common`) for Android instead of WASM, and wraps it
with **UniFFI** rather than hand-written JNI, so the same core can later generate Swift bindings for an iOS port
without a second FFI layer.

## Layout

- `rust-core/` — Cargo workspace member. Pins the same `ct-agent`/`ct-common` commit the WASM build uses
  (see reference repo's `Agent.Dockerfile` `CT_AGENT_REF`). Exposes a UniFFI interface instead of `wasm-bindgen`.
- `android/` — Gradle/Kotlin app. Consumes the Rust core as a `.aar` (built via `cargo-ndk`), no business logic
  of its own beyond UI and platform integration (Keystore, WebRTC transport).
- `docker/` — Reproducible Rust + Android NDK build/test environment.

## The four known gaps this port must actually fix (not just carry over)

Documented in the reference repo's README / open issues. Each one gets a real fix here, not a port of the bug:

1. **Broken automatic WebRTC→direct-channel fallback** ([demo#129](https://github.com/scimbe/CADS-webconference-demo/issues/129)) —
   on a UDP-blocked network, the fallback fires but hands off a stale signaling WebSocket; call is silently
   abandoned ~2s later, ~14s of silence with no user feedback beforehand. Fix here: redesign the transport
   handoff as an explicit state machine (see `rust-core/` transport module, TODO) with a fresh signaling
   connection on fallback and visible UI feedback from t=0, not just after the automatic path fails.
2. **No TURN relay** (STUN only) — needs a real TURN server + credential provisioning. Tracked as a config/infra
   TODO in `rust-core/src/ice.rs` (stub).
3. **Identity keys in cleartext** (web: `localStorage`) — Android fix: **Android Keystore** for the actual key
   material (hardware-backed where available), **EncryptedSharedPreferences** for anything that must be
   file-backed. No cleartext key material on disk. Implemented in
   `android/app/src/main/kotlin/.../identity/KeyStoreIdentity.kt` + `KeyStoreIdentityTest.kt` (androidTest) --
   **written but not yet compiled/run**: no Android SDK/Gradle/Kotlin toolchain available in this dev
   environment (verified: only `javac` present). Needs a real instrumented-test run before this is trusted.
   Still open: wiring `KeyStoreIdentity` into the actual call-setup UI flow (currently a standalone class,
   nothing calls it yet), and the rust-core `.aar` build itself (`implementation(project(":rust-core"))` is
   still a TODO in `app/build.gradle.kts` -- `generateHolderIdentity()`/`generateNoiseIdentity()` exist and
   are tested in `rust-core/`, but aren't cross-compiled for Android yet).
4. **Narrow bridge trust model** — server-side (`bridge/server.js` in the reference repo), out of scope for the
   Android client itself but tracked here since the client's assumptions about bridge trust need to match
   whatever the server-side fix ends up being.

## Additional feature

**Channel-to-channel communication**, matching the webconference's Agent-Fabric channel-join mechanism — i.e.
this client should be able to join a CADS-Tunnel channel the same way the browser demo does, not a
simplified/parallel protocol.

## Status

Scaffold only. No working build yet. See commit history for progress.
