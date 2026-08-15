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
   abandoned ~2s later, ~14s of silence with no user feedback beforehand. Fixed (logic layer): `rust-core/src/transport_fallback.rs`
   is a tested, pure state machine (`TransportFallbackController`) — every `FallingBackToDirectChannel` transition
   carries `needs_fresh_signaling: true` (the actual bug was reusing a stale socket), and status updates are
   forced well before the reference bug's ~14s silence window. 6 tests, all passing. Still needed: wiring this
   controller to real Android WebRTC `PeerConnection.Observer` callbacks and a real timer (currently driven by
   a caller-supplied `elapsed_ms`, by design, for testability) — the decision logic is done, the platform glue isn't.
2. **No TURN relay** (STUN only) — needs a real TURN server + credential provisioning. `rust-core/src/ice.rs`
   provides the (tested) configuration surface — `build_ice_server_list()` takes real TURN URLs/credentials
   once provisioned and falls back to STUN-only otherwise. This crate has no more TURN infrastructure than the
   reference repo did — the actual server/credentials still need to come from somewhere (a backend minting
   short-lived TURN credentials, most likely) before this gap is really closed, not just made configurable.
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
