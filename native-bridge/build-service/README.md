# Android/native-bridge build service (CADS-devsystem#12)

Real answer to a real, repeatedly-hit problem: the production `devsystem-web` host (4 CPU /
7.6GB, no swap, already running a live fleet) has OOM-killed local `cargo-ndk` cross-compilation
of `native-bridge/` multiple times, even at `-j1`/`codegen-units=1`. This directory is a real,
tested build toolchain that runs the same cross-compile on separate infrastructure instead.

## What this is

`Dockerfile` pins the **exact** toolchain `.github/workflows/android-ci.yml` verifies against —
NDK **r27d** (not the newer NDK bundled in the community `mingc/android-build-box` image),
`cargo-ndk` **4.1.2**, `aarch64-linux-android` + `x86_64-linux-android` targets — so a build run
through it is directly comparable to what CI already checks, not an approximation.

`verify.sh` runs a real cross-compile inside that image and performs the exact same two checks
`verify-native-bridge`'s CI job does: a byte-for-byte Kotlin bindings diff, and a per-ABI
`nm -D --defined-only` exported-symbol diff — against whatever's currently committed in
`app/src/main/{jniLibs,java}`.

**Verified live** (2026-08-05, on a real 36-core/125GB host, genuinely separate from the
production `devsystem-web` host): both checks pass — Kotlin bindings byte-identical, both ABIs'
105 exported symbols match exactly what's currently committed.

## Channel-connection shape: auction role + `devsystem_iterate`, not a new peer-to-peer listener

The issue offered two paths: a direct-address respawn-loop channel (mirroring
`pipeline/src/bin/github_issue_channel_client.rs`/`github_issue_channel_handler.rs`, #48), or
filling `devsystem.android_native_bridge`'s existing auction role and reporting through
`devsystem_iterate`/`devsystem_iterate --remote`. This went with the second:

- `devsystem.android_native_bridge` is already a real, live role in the `webconference-android`
  run's spec — no new role to declare.
- `devsystem_iterate --remote` already exists, is tested, and (as of this work) has its
  redirect-following bug fixed (CADS-devsystem commit `9c9290e`) — no new client binary needed.
- A permanently-reachable peer-to-peer channel listener needs a long-lived server process; an
  ephemeral build-on-demand job (this is periodic, not low-latency-interactive) is a better fit
  for the request/report pattern `devsystem_iterate` already provides than for holding a socket
  open indefinitely from a session-bound environment.

## Known real gap: reporting the result

Submitting a real build-request/result round trip via `devsystem_iterate --remote` needs the
same M2M authentication path currently being scoped on CADS-devsystem#7 (the tunnel's
`require_login` gate has no bearer-token support today, confirmed directly against
`crates/control-plane/src/gate.rs`) — this build service's own live-report step is blocked on
the same fix, not a separate one.

## Usage

```bash
docker build -t devsystem-android-build-service:r27d native-bridge/build-service
./native-bridge/build-service/verify.sh
```
