//! UniFFI-exposed interface for the Android (and later iOS) port of ct-agent/ct-common.
//! Mirrors the surface `ct-agent-wasm` exposes to CADS-webconference-demo's app.js, but
//! over UniFFI instead of wasm-bindgen. No logic lives here yet — this is the scaffold
//! that the real channel-join / Noise_IK / transport code will be ported into.

uniffi::setup_scaffolding!();

/// Placeholder: mirrors `holderSign`/`buildChannelJoinRequest` from the web demo.
/// TODO: port from ct-agent/ct-common once the crate is pinned in Cargo.toml.
#[uniffi::export]
pub fn channel_join_placeholder() -> String {
    "not yet implemented — see ARCHITECTURE.md".to_string()
}

// TODO modules, ported from the reference repo's call-*.js + ct-agent core:
// mod identity;   // Noise_IK keys — Android Keystore/EncryptedSharedPreferences backed, see ARCHITECTURE.md gap 3
// mod transport;  // WebRTC + direct-channel transport and the fallback handoff state machine, gap 1
// mod ice;        // STUN + TURN config, gap 2
// mod channel;    // Agent-Fabric channel-join / channel-to-channel communication
