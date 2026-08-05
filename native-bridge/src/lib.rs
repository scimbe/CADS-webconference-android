//! Rust -> Kotlin bridge (#382, `devsystem.android_native_bridge`) for the
//! real Noise_IK/Agent-Fabric client. `bridge_version` proved the toolchain
//! (UniFFI + cargo-ndk) works end to end; `generate_noise_public_key_hex` is
//! the first real call into CADS-Tunnel's own crypto -- the exact
//! `ct_common::noise::generate_static_keypair()` every real client/origin in
//! this ecosystem uses (`Noise_IK_25519_ChaChaPoly_BLAKE2s`), not a
//! reimplementation. `channel.rs` is the first real handshake + data path:
//! a direct peer-to-peer `Noise_IK` session (`ct_common::a2a`) over a real
//! TCP socket -- see its module docs for exactly what that does and does not
//! prove yet toward the full broker-mediated Agent-Fabric channel-join.

mod channel;
mod message;

pub use channel::{
    bind_channel_listener, dial_channel_direct, generate_channel_identity, ChannelError,
    ChannelIdentity, ChannelListener, ChannelSession,
};
pub use message::{
    decode_text_message, encode_text_message, new_text_message, MessageDecodeError, TextMessage,
};

uniffi::setup_scaffolding!();

/// Real, callable proof the native bridge toolchain works -- not a placeholder
/// string baked into Kotlin, an actual value computed in Rust and marshaled
/// across the FFI boundary UniFFI generates.
#[uniffi::export]
fn bridge_version() -> String {
    format!("native-bridge v{} (UniFFI; now touches real ct-common Noise_IK crypto)", env!("CARGO_PKG_VERSION"))
}

/// Generates a real, fresh Noise_IK static keypair via
/// `ct_common::noise::generate_static_keypair()` -- the exact code path
/// every real CADS-Tunnel client/origin uses, not a stub or a
/// hand-rolled X25519 call. Returns only the public half (the Origin
/// Identity a peer would pin) as lowercase hex.
///
/// The private half is intentionally NOT returned across the FFI boundary in
/// this increment: `StaticKeypair` is `ZeroizeOnDrop` (CADS-Tunnel #250) and
/// is zeroized here, inside this function, the moment it goes out of scope --
/// this call proves the real crypto is reachable from Kotlin without yet
/// deciding how a persistent identity would be stored securely on-device
/// (Android Keystore), which is a separate, later increment.
#[uniffi::export]
fn generate_noise_public_key_hex() -> String {
    let keypair = ct_common::noise::generate_static_keypair();
    keypair.public.iter().map(|b| format!("{b:02x}")).collect()
}
