#!/usr/bin/env bash
# Runs a real cross-compile of native-bridge (both Android ABIs) inside the
# Dockerfile in this directory, then verifies the result against what's
# currently committed in app/src/main/{jniLibs,java} -- the exact same two
# checks .github/workflows/android-ci.yml's verify-native-bridge job runs,
# just from a different host. Exits non-zero on any mismatch.
#
# Usage (from the repo root):
#   docker build -t devsystem-android-build-service:r27d native-bridge/build-service
#   ./native-bridge/build-service/verify.sh
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."  # repo root
IMAGE=devsystem-android-build-service:r27d

docker run --rm -v "$(pwd)":/work -w /work/native-bridge "$IMAGE" bash -c '
  set -euo pipefail
  cargo ndk -t arm64-v8a -t x86_64 -o /tmp/fresh-jniLibs build --release
  cargo build --lib --release --quiet
  mkdir -p /tmp/fresh-kotlin
  cargo run --quiet --features=uniffi/cli --bin uniffi-bindgen -- \
    generate --library target/release/libnative_bridge.so --language kotlin \
    --out-dir /tmp/fresh-kotlin --no-format

  echo "--- Kotlin bindings ---"
  diff -u /tmp/fresh-kotlin/uniffi/native_bridge/native_bridge.kt \
    /work/app/src/main/java/uniffi/native_bridge/native_bridge.kt \
    && echo "OK: Kotlin bindings byte-identical"

  echo "--- exported symbols ---"
  for pair in "arm64-v8a:/tmp/fresh-jniLibs/arm64-v8a" "x86_64:/tmp/fresh-jniLibs/x86_64"; do
    abi="${pair%%:*}"; dir="${pair##*:}"
    fresh=$(nm -D --defined-only "$dir/libnative_bridge.so" | awk "{print \$3}" | sort)
    committed=$(nm -D --defined-only "/work/app/src/main/jniLibs/$abi/libnative_bridge.so" | awk "{print \$3}" | sort)
    if [ "$fresh" = "$committed" ]; then
      echo "OK: $abi exported symbols match ($(echo "$fresh" | wc -l) symbols)"
    else
      echo "MISMATCH: $abi"
      diff <(echo "$fresh") <(echo "$committed") || true
      exit 1
    fi
  done
'
