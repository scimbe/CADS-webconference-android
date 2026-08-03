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
  `minSdk 26` for `RTCPeerConnection`/WebRTC support), Gradle 8.7 wrapper checked in.
- ✅ Builds a real, signed debug APK (`./gradlew assembleDebug`), hermetically
  verified in `mingc/android-build-box`.
- ✅ A real Robolectric unit test (`./gradlew testDebugUnitTest`) proves
  `MainActivity` actually displays its status string, not just that the project
  compiles.
- ✅ Continuous verification: [`.github/workflows/android-ci.yml`](.github/workflows/android-ci.yml)
  runs both on every push/PR against `main`.
- ✅ One real review pass found and fixed two genuine issues: `allowBackup="true"`
  (a real, if currently low-stakes, security concern given this project's eventual
  purpose) and non-density-aware raw-pixel padding.
- ⏸ **`MainActivity` is a placeholder `TextView`, not a working client.** The real
  target — an Agent-Fabric channel-join + Noise_IK handshake + WebRTC client,
  matching `ct-agent-wasm`'s browser behavior — needs a Rust→Android bridge, since
  CADS-Tunnel's Noise_IK/Agent-Fabric code is pure Rust with no existing
  Android/JNI path. That's a real architecture decision (`cargo-ndk` vs `UniFFI`)
  currently **blocking further pipeline iterations on this repo** — see
  [CADS-Tunnel#382](https://github.com/scimbe/CADS-Tunnel/issues/382) for the open
  question and full context.

## Building

```bash
./gradlew assembleDebug        # -> app/build/outputs/apk/debug/app-debug.apk
./gradlew testDebugUnitTest    # Robolectric unit tests, no emulator needed
```

Requires a full Android SDK/NDK toolchain (JDK 17, `compileSdk 34`). CI uses
GitHub-hosted `ubuntu-latest`, which ships one preinstalled; locally this project
has been verified inside `mingc/android-build-box` via Docker.
