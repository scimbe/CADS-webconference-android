//! Minimal UniFFI spike (#382, `devsystem.android_native_bridge`): proves the
//! Rust -> Kotlin bridge toolchain (UniFFI + cargo-ndk) works end to end in this
//! actual environment before the real Noise_IK/Agent-Fabric bridge is built.
//! `bridge_version` is deliberately trivial -- this crate does not yet touch
//! CADS-Tunnel's crypto/channel code at all.

uniffi::setup_scaffolding!();

/// Real, callable proof the native bridge toolchain works -- not a placeholder
/// string baked into Kotlin, an actual value computed in Rust and marshaled
/// across the FFI boundary UniFFI generates.
#[uniffi::export]
fn bridge_version() -> String {
    format!("native-bridge spike v{} (UniFFI, not yet the real Noise_IK/Agent-Fabric client)", env!("CARGO_PKG_VERSION"))
}
