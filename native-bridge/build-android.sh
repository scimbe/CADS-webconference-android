#!/usr/bin/env bash
# Cross-compiles native-bridge for Android (cargo-ndk) and regenerates its UniFFI
# Kotlin bindings, copying both into the app module. Run this after any change to
# native-bridge/src/lib.rs; the .so/.kt files it produces are committed. CI
# (.github/workflows/android-ci.yml, verify-native-bridge job) rebuilds both fresh
# on every push and fails if what's committed here doesn't match -- run this
# script and commit the result whenever native-bridge/src/lib.rs changes, or CI
# will catch the drift.
#
# Usage: ./build-android.sh
# Requires: a container/host with the Android NDK (ANDROID_NDK env var set) and
# either an existing Rust toolchain or network access to install one via rustup.
# Verified inside mingc/android-build-box.

set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"

if ! command -v rustc >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable --profile minimal
  # shellcheck disable=SC1090
  source "$HOME/.cargo/env"
fi

rustup target add aarch64-linux-android x86_64-linux-android

if ! command -v cargo-ndk >/dev/null 2>&1; then
  cargo install cargo-ndk --version 4.1.2 --locked
fi

APP_JNI_LIBS=../app/src/main/jniLibs
APP_KOTLIN=../app/src/main/java

rm -rf jniLibs "$APP_JNI_LIBS/arm64-v8a/libnative_bridge.so" "$APP_JNI_LIBS/x86_64/libnative_bridge.so"
cargo ndk -t arm64-v8a -t x86_64 -o jniLibs build --release
mkdir -p "$APP_JNI_LIBS/arm64-v8a" "$APP_JNI_LIBS/x86_64"
cp jniLibs/arm64-v8a/libnative_bridge.so "$APP_JNI_LIBS/arm64-v8a/"
cp jniLibs/x86_64/libnative_bridge.so "$APP_JNI_LIBS/x86_64/"

cargo build --lib --release --quiet
cargo run --quiet --features=uniffi/cli --bin uniffi-bindgen -- \
  generate --library target/release/libnative_bridge.so --language kotlin \
  --out-dir "$APP_KOTLIN" --no-format

echo "Done. Native libs in $APP_JNI_LIBS/{arm64-v8a,x86_64}/, bindings in $APP_KOTLIN/uniffi/native_bridge/"
