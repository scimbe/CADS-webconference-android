# Real Android/native-bridge cross-compile toolchain, pinned to match
# CADS-webconference-android's own CI exactly (.github/workflows/android-ci.yml):
# NDK r27d (nttld/setup-ndk), cargo-ndk 4.1.2, aarch64-linux-android +
# x86_64-linux-android targets. Not the mingc/android-build-box image's bundled
# NDK (30.0.15729638-beta2 as of this build) -- deliberately pinned to the exact
# version CI verifies against, not "close enough".
FROM rust:1-slim-bookworm

RUN apt-get update && apt-get install -y --no-install-recommends \
    curl unzip ca-certificates \
    && rm -rf /var/lib/apt/lists/*

ENV ANDROID_NDK_HOME=/opt/android-ndk-r27d
RUN curl -fsSL https://dl.google.com/android/repository/android-ndk-r27d-linux.zip -o /tmp/ndk.zip \
    && unzip -q /tmp/ndk.zip -d /opt \
    && rm /tmp/ndk.zip

RUN rustup target add aarch64-linux-android x86_64-linux-android \
    && cargo install cargo-ndk --version 4.1.2 --locked

WORKDIR /work
