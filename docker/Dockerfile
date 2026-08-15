# Reproducible build/test environment: Rust (for rust-core, cross-compiled via cargo-ndk)
# + Android SDK/NDK (for android/). Kept as one image so CI and local dev match.

FROM eclipse-temurin:17-jdk-jammy AS base

ENV ANDROID_SDK_ROOT=/opt/android-sdk \
    RUSTUP_HOME=/opt/rustup \
    CARGO_HOME=/opt/cargo \
    PATH=/opt/cargo/bin:/opt/android-sdk/cmdline-tools/latest/bin:/opt/android-sdk/platform-tools:$PATH

RUN apt-get update && apt-get install -y --no-install-recommends \
      curl unzip git build-essential pkg-config \
    && rm -rf /var/lib/apt/lists/*

# Rust toolchain + Android targets + cargo-ndk
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable \
    && /opt/cargo/bin/rustup target add \
       aarch64-linux-android armv7-linux-androideabi x86_64-linux-android i686-linux-android \
    && /opt/cargo/bin/cargo install cargo-ndk uniffi_bindgen

# Android commandline-tools (NDK pinned via android/local.properties once that exists)
# TODO: pin exact SDK/NDK versions once android/ has a real build.gradle.kts

WORKDIR /workspace
