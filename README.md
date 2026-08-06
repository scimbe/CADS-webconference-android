# CADS-webconference-android

Android client scaffold for [CADS-webconference-demo](https://github.com/scimbe/CADS-webconference-demo)
— and the **flagship proof** for [The Development System](https://github.com/scimbe/CADS-devsystem)
([CADS-Tunnel#382](https://github.com/scimbe/CADS-Tunnel/issues/382)), a self-optimizing,
agent-driven development pipeline. This repo is the actual target software a real
pipeline run builds against; the pipeline's own definitions, role contracts, and this
run's full iteration history live in the separate coordination repo,
[`CADS-devsystem`](https://github.com/scimbe/CADS-devsystem) (see
[`runs/webconference-android/`](https://github.com/scimbe/CADS-devsystem/tree/main/runs/webconference-android)
for the real, iteration-by-iteration record of what built this and why).

## Status

- ✅ Kotlin/Gradle scaffold (`namespace org.bunsenbrenner.webconference`,
  `minSdk 26` for `RTCPeerConnection`/WebRTC support, `compileSdk`/`targetSdk 35`),
  Gradle 9.6.1 wrapper checked in, AGP 9.3.1 with built-in Kotlin support (no
  separate Kotlin Gradle plugin needed as of AGP 9.0).
- ✅ Builds a real, signed debug APK (`./gradlew assembleDebug`), hermetically
  verified in `mingc/android-build-box`.
- ✅ Real Robolectric unit tests (`./gradlew testDebugUnitTest`, 14 total as of
  this writing across `MainActivityTest`/`MessageStoreTest`/`TextMessageBridgeTest`)
  exercise `MainActivity`'s real UI logic and Room persistence, not just that the
  project compiles.
- ✅ Continuous verification: [`.github/workflows/android-ci.yml`](.github/workflows/android-ci.yml)
  runs both on every push/PR against `main`; [Dependabot](.github/dependabot.yml)
  watches Gradle deps and Actions versions weekly, with vulnerability alerts and
  automated security updates enabled on the repo.
- ✅ One real review pass found and fixed two genuine issues: `allowBackup="true"`
  (a real, if currently low-stakes, security concern given this project's eventual
  purpose) and non-density-aware raw-pixel padding.
- ✅ Full toolchain modernization pass (Gradle, Kotlin, AGP, Material, compileSdk,
  CI Actions versions) driven by real Dependabot PRs, each investigated and
  verified hermetically rather than blindly merged -- see recent commit history.
- ✅ `native-bridge/` (UniFFI + cargo-ndk) is real, not a spike anymore: it
  exports `bridge_version`, `generate_noise_public_key_hex` (a genuine
  `ct_common::noise::generate_static_keypair()` call), and the #382 channel
  wire format for text messages (`TextMessage { msg_id, sender_pubkey,
  timestamp, body }` + `new_text_message`/`encode_text_message`/
  `decode_text_message`, JSON via `serde_json`, hermetically tested on both the
  Rust side (`native-bridge/src/message.rs`) and across the FFI boundary from
  Kotlin (`TextMessageBridgeTest.kt`)).
- ✅ `native-bridge/src/channel.rs`: a REAL `send_text`/`recv_text`, backed by
  a genuine `Noise_IK` handshake (`ct_common::a2a::a2a_initiate`/`a2a_respond`)
  and real encrypted message framing (`a2a_send`/`a2a_recv`) over a real TCP
  socket — `generate_channel_identity`, `dial_channel_direct`,
  `bind_channel_listener`/`ChannelListener::accept`, and
  `ChannelSession::send_text`/`recv_text`, all real UniFFI async exports
  (`async_runtime = "tokio"`), hermetically tested including a real two-instance
  integration test that completes an authenticated handshake and exchanges
  encrypted `TextMessage`s in both directions.
  **This is a DIRECT peer-to-peer session, not the full Agent-Fabric
  channel-join** — no broker/rendezvous, no NAT traversal, no `:443` relay
  fallback; both sides must already know how to reach each other (out of
  band). See [`docs/channel-join-options.md`](docs/channel-join-options.md)
  for the full survey of what the real broker-mediated channel-join stack
  (`ct-agent`'s `channel.rs`/`channel_run.rs`) looks like and why this
  increment doesn't build on it yet.
- ✅ **`MainActivity` is a real, working chat client, not a placeholder.**
  It calls `channel.rs`'s exports directly: generates a real identity on
  launch, `bindChannelListener`/`dialChannelDirect` to connect, a real send/receive
  message thread, and message history persisted locally via Room
  (`MessageStore`) across app restarts. Several DAU-lens passes have since
  hardened it against careless input (validating connect fields before any
  native call, real feedback instead of silent no-ops on an empty Send, a
  peer disconnect no longer permanently disabling reconnecting) — see recent
  commit history. **The run's own declared M1 milestone ("1:1 Text-Messaging
  end-to-end") has been confirmed for real**: two separately-booted, real KVM
  Android emulator instances, each running this actual compiled app with a
  distinct Noise_IK identity, connected directly and exchanged a real message
  — cross-checked screenshots from both devices, not just one side's claim
  (see [CADS-devsystem issue #13](https://github.com/scimbe/CADS-devsystem/issues/13)
  and `docs/emulator-walkthrough/` for the real evidence). This is genuine
  device-to-device messaging through the compiled app, not just a `cargo
  test` integration test between two local processes — though still direct-
  address only, per the channel-join caveat above.

## Building

```bash
./gradlew assembleDebug        # -> app/build/outputs/apk/debug/app-debug.apk
./gradlew testDebugUnitTest    # Robolectric unit tests, no emulator needed
```

Requires a full Android SDK/NDK toolchain (JDK 17, `compileSdk 35`). CI uses
GitHub-hosted `ubuntu-latest`, which ships one preinstalled; locally this project
has been verified inside `mingc/android-build-box` via Docker.
