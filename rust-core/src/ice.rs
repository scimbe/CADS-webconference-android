//! ARCHITECTURE.md gap 2: the reference web demo only ever configures a public STUN
//! server (`RTCPeerConnection`'s `iceServers` used to be empty, then gained STUN --
//! see the demo's README, issue demo#18) and has "no TURN infrastructure/credentials to
//! offer". STUN alone can't traverse symmetric NAT or a locked-down corporate network --
//! that needs a real TURN relay. This module is the *configuration surface* for that: it
//! does not invent or hardcode a TURN deployment (this crate has no more TURN
//! infrastructure than the reference repo did), it gives the app a single, testable,
//! UniFFI-exposed place to assemble a real ICE server list once real TURN
//! credentials/URLs are provisioned (app config, a backend that mints short-lived TURN
//! credentials, etc.) -- shared with a future iOS port the same way the rest of
//! rust-core is, instead of duplicating this list-building logic per platform.
//!
//! `uniffi::setup_scaffolding!()` is called once, in lib.rs -- not repeated here.

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct IceServerConfig {
    /// One or more URLs for a single logical server, e.g. `["stun:stun.l.google.com:19302"]`
    /// or `["turn:turn.example.org:3478?transport=udp", "turn:turn.example.org:3478?transport=tcp"]`.
    pub urls: Vec<String>,
    /// TURN long-term or short-lived-credential username. `None` for a STUN-only entry.
    pub username: Option<String>,
    /// TURN credential (password, or a short-lived-credential HMAC per RFC 5766 §10).
    /// `None` for a STUN-only entry. Never logged -- callers should treat this the same
    /// way rust-core's identity private-key hex is treated (ARCHITECTURE.md gap 3).
    pub credential: Option<String>,
}

impl IceServerConfig {
    fn stun(url: &str) -> Self {
        Self { urls: vec![url.to_string()], username: None, credential: None }
    }
}

/// The STUN-only fallback, matching exactly what the reference web demo ships today
/// (demo#18) -- resolves ordinary NAT, does NOT traverse symmetric NAT or a locked-down
/// network. Callers should prefer [`build_ice_server_list`] with real TURN config once
/// available; this exists so "no TURN configured yet" still produces a working (if
/// limited) ICE server list instead of an empty one.
#[uniffi::export]
pub fn default_stun_only_ice_servers() -> Vec<IceServerConfig> {
    vec![IceServerConfig::stun("stun:stun.l.google.com:19302")]
}

/// Assemble a real ICE server list: STUN always included (cheap, no credentials needed,
/// helps ordinary-NAT cases resolve without waiting on/relaying through TURN), plus a
/// TURN entry when the caller has real, provisioned credentials.
///
/// `turn_urls` empty means "no TURN available" -- returns STUN-only rather than erroring,
/// since a STUN-only list is a legitimate (if limited) configuration, not a broken one.
#[uniffi::export]
pub fn build_ice_server_list(
    turn_urls: Vec<String>,
    turn_username: Option<String>,
    turn_credential: Option<String>,
) -> Vec<IceServerConfig> {
    let mut servers = default_stun_only_ice_servers();
    if !turn_urls.is_empty() {
        servers.push(IceServerConfig { urls: turn_urls, username: turn_username, credential: turn_credential });
    }
    servers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_list_is_stun_only_with_no_credentials() {
        let servers = default_stun_only_ice_servers();
        assert_eq!(servers.len(), 1);
        assert!(servers[0].urls[0].starts_with("stun:"));
        assert!(servers[0].username.is_none());
        assert!(servers[0].credential.is_none());
    }

    #[test]
    fn build_list_without_turn_urls_falls_back_to_stun_only() {
        let servers = build_ice_server_list(vec![], Some("user".to_string()), Some("secret".to_string()));
        assert_eq!(servers.len(), 1, "no TURN urls means no TURN entry, even if credentials were passed");
        assert!(servers[0].urls[0].starts_with("stun:"));
    }

    #[test]
    fn build_list_with_turn_urls_includes_stun_and_turn() {
        let turn_urls = vec!["turn:turn.example.org:3478?transport=udp".to_string()];
        let servers = build_ice_server_list(turn_urls.clone(), Some("user".to_string()), Some("secret".to_string()));
        assert_eq!(servers.len(), 2, "STUN entry plus the TURN entry");
        assert!(servers[0].urls[0].starts_with("stun:"));
        assert_eq!(servers[1].urls, turn_urls);
        assert_eq!(servers[1].username.as_deref(), Some("user"));
        assert_eq!(servers[1].credential.as_deref(), Some("secret"));
    }
}
