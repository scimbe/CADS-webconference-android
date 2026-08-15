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

# Rust toolchain + Android targets + cargo-ndk.
#
# Real build error, live-caught (not guessed): `cargo install uniffi_bindgen` fails with
# "there is nothing to install in `uniffi_bindgen v0.32.0`, because it has no binaries" --
# that crate has no standalone CLI binary. The actual bindgen binary comes from `uniffi`
# itself when built with `features = ["cli"]` (already set in rust-core/Cargo.toml) and is
# invoked as `cargo run --bin uniffi-bindgen -- generate ...` *from inside* rust-core, not
# via a separate global install. Only cargo-ndk needs a global `cargo install` here.
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable \
    && /opt/cargo/bin/rustup target add \
       aarch64-linux-android armv7-linux-androideabi x86_64-linux-android i686-linux-android \
    && /opt/cargo/bin/cargo install cargo-ndk

# Android cmdline-tools + Gradle -- added specifically to actually verify
# rust-core/build.gradle.kts + android/settings.gradle.kts/app/build.gradle.kts (previously
# written but never run, no toolchain having been available). Pinned versions, not "latest",
# so this stays reproducible.
ENV GRADLE_VERSION=8.7
RUN curl -fsSL "https://services.gradle.org/distributions/gradle-${GRADLE_VERSION}-bin.zip" -o /tmp/gradle.zip \
    && unzip -q /tmp/gradle.zip -d /opt \
    && ln -s "/opt/gradle-${GRADLE_VERSION}/bin/gradle" /usr/local/bin/gradle \
    && rm /tmp/gradle.zip

RUN mkdir -p "$ANDROID_SDK_ROOT/cmdline-tools" \
    && curl -fsSL "https://dl.google.com/android/repository/commandlinetools-linux-11076708_latest.zip" -o /tmp/cmdline-tools.zip \
    && unzip -q /tmp/cmdline-tools.zip -d "$ANDROID_SDK_ROOT/cmdline-tools" \
    && mv "$ANDROID_SDK_ROOT/cmdline-tools/cmdline-tools" "$ANDROID_SDK_ROOT/cmdline-tools/latest" \
    && rm /tmp/cmdline-tools.zip \
    && yes | sdkmanager --licenses > /dev/null \
    && sdkmanager "platform-tools" "platforms;android-34" "build-tools;34.0.0" "ndk;26.3.11579264"
ENV ANDROID_NDK_HOME=/opt/android-sdk/ndk/26.3.11579264

WORKDIR /workspace
