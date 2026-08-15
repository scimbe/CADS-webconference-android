//! UniFFI-exposed interface for the Android (and later iOS) port of ct-agent/ct-common.
//!
//! Faithful port of `ct-agent`'s `wasm/src/lib.rs` (https://github.com/scimbe/ct-agent),
//! swapping `wasm-bindgen` for `uniffi` as the FFI boundary. Every function here is a thin
//! wrapper around `ct_common` -- the real protocol/crypto logic lives there, verified once,
//! same as the wasm build. Ported by reading the actual upstream source, not guessed.
//!
//! Ported so far: identity generation, channel id derivation, wire framing, and the full
//! Noise_IK handshake + transport state machine. NOT yet ported (see TODOs at the bottom):
//! holder_sign, build_channel_join_request, and the WebRTC signal encode/decode wire format
//! -- those need the same careful source-verified treatment before being added.

use std::sync::Mutex;

mod ice;
pub use ice::{build_ice_server_list, default_stun_only_ice_servers, IceServerConfig};

uniffi::setup_scaffolding!();

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum CtAgentError {
    #[error("{0}")]
    Generic(String),
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn from_hex(s: &str) -> Result<Vec<u8>, CtAgentError> {
    if s.len() % 2 != 0 {
        return Err(CtAgentError::Generic("hex string must have an even length".to_string()));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&s[i..i + 2], 16)
                .map_err(|_| CtAgentError::Generic("invalid hex character".to_string()))
        })
        .collect()
}

fn hex32(s: &str) -> Result<[u8; 32], CtAgentError> {
    let v = from_hex(s)?;
    <[u8; 32]>::try_from(v.as_slice())
        .map_err(|_| CtAgentError::Generic("expected 32 bytes (64 hex chars)".to_string()))
}

/// A freshly generated holder identity (ed25519) -- the channel member's own, stable
/// identity. Mirrors `ct-agent-wasm`'s `HolderIdentity`/`generate_holder_identity`.
/// SECURITY (ARCHITECTURE.md gap 3): the caller (Kotlin side) MUST persist `private_hex`
/// via Android Keystore / EncryptedSharedPreferences immediately -- never write it to
/// plain SharedPreferences, a plain file, or a log line.
#[derive(uniffi::Record)]
pub struct HolderIdentity {
    pub public_hex: String,
    pub private_hex: String,
}

#[uniffi::export]
pub fn generate_holder_identity() -> HolderIdentity {
    use ed25519_dalek::SigningKey;
    let sk = SigningKey::generate(&mut rand::rngs::OsRng);
    HolderIdentity {
        public_hex: to_hex(sk.verifying_key().as_bytes()),
        private_hex: to_hex(&sk.to_bytes()),
    }
}

/// A freshly generated Noise (X25519) static keypair -- the channel member's transport
/// key, distinct from its holder identity. Same Keystore requirement as `HolderIdentity`.
#[derive(uniffi::Record)]
pub struct NoiseIdentity {
    pub public_hex: String,
    pub private_hex: String,
}

#[uniffi::export]
pub fn generate_noise_identity() -> NoiseIdentity {
    let kp = ct_common::noise::generate_static_keypair();
    NoiseIdentity {
        public_hex: to_hex(&kp.public),
        private_hex: to_hex(&kp.private),
    }
}

/// Deterministic channel id for the link between two holder keys under a channel
/// operator -- bit-for-bit the same computation a native or browser peer performs, so
/// no coordination round-trip is needed to agree on it.
#[uniffi::export]
pub fn channel_id_for_link(
    operator_pubkey_hex: String,
    holder_a_hex: String,
    holder_b_hex: String,
) -> Result<String, CtAgentError> {
    let operator = hex32(&operator_pubkey_hex)?;
    let a = hex32(&holder_a_hex)?;
    let b = hex32(&holder_b_hex)?;
    let id = ct_common::channel::channel_id_for_link(&operator, &a, &b);
    Ok(to_hex(&id.0))
}

/// Frame a Noise wire message for a byte-stream transport (2-byte big-endian length
/// prefix + body) -- the exact framing native and browser peers use, so this client is
/// wire-indistinguishable from either.
#[uniffi::export]
pub fn frame_message(msg: Vec<u8>) -> Vec<u8> {
    ct_common::noise::frame(&msg)
}

const NOISE_MAX_MESSAGE: usize = 65535;

/// A Noise_IK handshake in progress. Wrapped in a `Mutex` (not exposed to callers) purely
/// because UniFFI objects are shared as `Arc<Self>` across the FFI boundary and need
/// interior mutability for `write_message`/`read_message`/`into_transport` to mutate
/// state through a shared reference -- the handshake itself is still only ever driven by
/// one caller at a time, same single-threaded state machine as the wasm version.
#[derive(uniffi::Object)]
pub struct NoiseHandshake {
    inner: Mutex<Option<snow::HandshakeState>>,
}

#[uniffi::export]
impl NoiseHandshake {
    /// Initiator side (mirrors `CT_CHANNEL_ROLE=initiate`): pins the peer's Noise public
    /// key up front, per Noise_IK's initiator property.
    #[uniffi::constructor]
    pub fn new_initiator(
        local_noise_private_hex: String,
        remote_noise_public_hex: String,
    ) -> Result<Self, CtAgentError> {
        let local = hex32(&local_noise_private_hex)?;
        let remote = hex32(&remote_noise_public_hex)?;
        let hs = ct_common::noise::client_handshake(&local, &remote)
            .map_err(|e| CtAgentError::Generic(e.to_string()))?;
        Ok(Self { inner: Mutex::new(Some(hs)) })
    }

    /// Responder side (mirrors `CT_CHANNEL_ROLE=accept`): learns the peer's identity FROM
    /// the first handshake message, needs only its own private key up front.
    #[uniffi::constructor]
    pub fn new_responder(local_noise_private_hex: String) -> Result<Self, CtAgentError> {
        let local = hex32(&local_noise_private_hex)?;
        let hs = ct_common::noise::origin_handshake(&local)
            .map_err(|e| CtAgentError::Generic(e.to_string()))?;
        Ok(Self { inner: Mutex::new(Some(hs)) })
    }

    pub fn write_message(&self, payload: Vec<u8>) -> Result<Vec<u8>, CtAgentError> {
        let mut guard = self.inner.lock().unwrap();
        let hs = guard
            .as_mut()
            .ok_or_else(|| CtAgentError::Generic("handshake already consumed by into_transport()".into()))?;
        let mut buf = [0u8; NOISE_MAX_MESSAGE];
        let n = hs
            .write_message(&payload, &mut buf)
            .map_err(|e| CtAgentError::Generic(e.to_string()))?;
        Ok(buf[..n].to_vec())
    }

    pub fn read_message(&self, msg: Vec<u8>) -> Result<Vec<u8>, CtAgentError> {
        let mut guard = self.inner.lock().unwrap();
        let hs = guard
            .as_mut()
            .ok_or_else(|| CtAgentError::Generic("handshake already consumed by into_transport()".into()))?;
        let mut buf = [0u8; NOISE_MAX_MESSAGE];
        let n = hs
            .read_message(&msg, &mut buf)
            .map_err(|e| CtAgentError::Generic(e.to_string()))?;
        Ok(buf[..n].to_vec())
    }

    pub fn is_finished(&self) -> Result<bool, CtAgentError> {
        let guard = self.inner.lock().unwrap();
        let hs = guard
            .as_ref()
            .ok_or_else(|| CtAgentError::Generic("handshake already consumed by into_transport()".into()))?;
        Ok(hs.is_handshake_finished())
    }

    /// Transition to the encrypted transport session once `is_finished()` is true --
    /// consumes this handshake's internal state (one-way, matching `snow`'s own model).
    pub fn into_transport(&self) -> Result<NoiseTransport, CtAgentError> {
        let mut guard = self.inner.lock().unwrap();
        let hs = guard
            .take()
            .ok_or_else(|| CtAgentError::Generic("handshake already consumed by into_transport()".into()))?;
        let t = hs
            .into_transport_mode()
            .map_err(|e| CtAgentError::Generic(e.to_string()))?;
        Ok(NoiseTransport { inner: Mutex::new(t) })
    }
}

/// An established, encrypted Noise_IK session -- carries the application-data traffic
/// (SDP offers/answers, ICE candidates, and eventually media signaling).
#[derive(uniffi::Object)]
pub struct NoiseTransport {
    inner: Mutex<snow::TransportState>,
}

#[uniffi::export]
impl NoiseTransport {
    pub fn encrypt(&self, plaintext: Vec<u8>) -> Result<Vec<u8>, CtAgentError> {
        let mut guard = self.inner.lock().unwrap();
        let mut buf = [0u8; NOISE_MAX_MESSAGE];
        let n = guard
            .write_message(&plaintext, &mut buf)
            .map_err(|e| CtAgentError::Generic(e.to_string()))?;
        Ok(buf[..n].to_vec())
    }

    pub fn decrypt(&self, ciphertext: Vec<u8>) -> Result<Vec<u8>, CtAgentError> {
        let mut guard = self.inner.lock().unwrap();
        let mut buf = [0u8; NOISE_MAX_MESSAGE];
        let n = guard
            .read_message(&ciphertext, &mut buf)
            .map_err(|e| CtAgentError::Generic(e.to_string()))?;
        Ok(buf[..n].to_vec())
    }
}

/// Sign a byte string (the edge's 32-byte single-use possession challenge, in practice --
/// see [`build_channel_join_request`]'s doc for the full join sequence) with a holder's
/// ed25519 private key. The signature is sent RAW on the wire (no framing) as the direct
/// response to that challenge.
#[uniffi::export]
pub fn holder_sign(holder_private_hex: String, message: Vec<u8>) -> Result<Vec<u8>, CtAgentError> {
    use ed25519_dalek::Signer;
    let sk = ed25519_dalek::SigningKey::from_bytes(&hex32(&holder_private_hex)?);
    Ok(sk.sign(&message).to_bytes().to_vec())
}

/// Build the exact bytes a channel member sends to join a channel-to-channel session,
/// from a pre-minted, hex-encoded `SignedChannelGrant` (this client cannot mint its own
/// grant -- that needs the channel operator's private key -- a backend hands each peer
/// its own grant hex out of band) and the endpoint this member advertises.
///
/// Full join sequence over the WebSocket, mirroring `channel_broker::read_channel_join_on_stream`:
/// 1. send `frame_message(build_channel_join_request(grant, endpoint))` as one message
/// 2. read the next 32 bytes -- `b"NO"` (2 bytes) means refused, otherwise it's a 32-byte
///    single-use possession challenge
/// 3. send `holder_sign(holder_private_hex, challenge)` (64 raw bytes, no framing) next
/// 4. from here the socket is a raw relay splice until a partner also joins, then an
///    `"OK <peer...>\n"` ack arrives on both sides and the Noise handshake begins
#[uniffi::export]
pub fn build_channel_join_request(grant_hex: String, endpoint: String) -> Result<Vec<u8>, CtAgentError> {
    let grant_bytes = from_hex(&grant_hex)?;
    let grant = ct_common::channel::SignedChannelGrant::decode(&grant_bytes)
        .map_err(|e| CtAgentError::Generic(e.to_string()))?;
    let req = ct_common::channel::ChannelJoinRequest { grant, endpoint };
    Ok(req.encode())
}

/// WebRTC signaling messages -- what rides over a [`NoiseTransport`] session (encrypt the
/// encoded bytes, send over the channel; decrypt the peer's bytes, decode back into one of
/// these). A thin, self-delimiting wire format carrying SDP/candidate text verbatim (this
/// crate never parses SDP -- Android's own WebRTC stack generates/consumes it). Unlike the
/// wasm build (which decodes into a loosely-typed JS object), UniFFI gives Kotlin a real
/// sealed class here -- an improvement on the wire-compatible port, not a departure from it
/// (the *encoding* is bit-for-bit identical, only the decoded Rust/Kotlin-side shape differs).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum SignalMessage {
    Offer { sdp: String },
    Answer { sdp: String },
    /// `sdp_mline_index` absent (`None`) is encoded on the wire as `u16::MAX` -- WebRTC's
    /// own `RTCIceCandidateInit.sdpMLineIndex` is optional and a real index never reaches
    /// anywhere close to that value.
    IceCandidate { candidate: String, sdp_mid: Option<String>, sdp_mline_index: Option<u16> },
    /// Explicit "hanging up" -- lets the peer tear down its `RTCPeerConnection` promptly
    /// instead of waiting on an ICE-failure timeout when the channel just closes.
    Bye,
}

const SIGNAL_TYPE_OFFER: u8 = 1;
const SIGNAL_TYPE_ANSWER: u8 = 2;
const SIGNAL_TYPE_ICE: u8 = 3;
const SIGNAL_TYPE_BYE: u8 = 4;
const SIGNAL_NO_MLINE_INDEX: u16 = u16::MAX;

fn push_u16_str(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u16).to_be_bytes());
    out.extend_from_slice(s.as_bytes());
}
fn push_u8_str(out: &mut Vec<u8>, s: &str) {
    out.push(s.len() as u8);
    out.extend_from_slice(s.as_bytes());
}
fn take_n<'a>(cur: &mut &'a [u8], n: usize) -> Result<&'a [u8], CtAgentError> {
    if cur.len() < n {
        return Err(CtAgentError::Generic("truncated signal message".to_string()));
    }
    let (head, tail) = cur.split_at(n);
    *cur = tail;
    Ok(head)
}
fn take_u8(cur: &mut &[u8]) -> Result<u8, CtAgentError> {
    Ok(take_n(cur, 1)?[0])
}
fn take_u16_str(cur: &mut &[u8]) -> Result<String, CtAgentError> {
    let len = u16::from_be_bytes(take_n(cur, 2)?.try_into().unwrap()) as usize;
    String::from_utf8(take_n(cur, len)?.to_vec())
        .map_err(|_| CtAgentError::Generic("signal message field is not valid UTF-8".to_string()))
}
fn take_u8_str(cur: &mut &[u8]) -> Result<String, CtAgentError> {
    let len = take_u8(cur)? as usize;
    String::from_utf8(take_n(cur, len)?.to_vec())
        .map_err(|_| CtAgentError::Generic("signal message field is not valid UTF-8".to_string()))
}

impl SignalMessage {
    fn encode_inner(&self) -> Vec<u8> {
        let mut out = Vec::new();
        match self {
            SignalMessage::Offer { sdp } => {
                out.push(SIGNAL_TYPE_OFFER);
                push_u16_str(&mut out, sdp);
            }
            SignalMessage::Answer { sdp } => {
                out.push(SIGNAL_TYPE_ANSWER);
                push_u16_str(&mut out, sdp);
            }
            SignalMessage::IceCandidate { candidate, sdp_mid, sdp_mline_index } => {
                out.push(SIGNAL_TYPE_ICE);
                push_u16_str(&mut out, candidate);
                push_u8_str(&mut out, sdp_mid.as_deref().unwrap_or(""));
                out.extend_from_slice(&sdp_mline_index.unwrap_or(SIGNAL_NO_MLINE_INDEX).to_be_bytes());
            }
            SignalMessage::Bye => out.push(SIGNAL_TYPE_BYE),
        }
        out
    }

    fn decode_inner(bytes: &[u8]) -> Result<Self, CtAgentError> {
        let mut cur = bytes;
        let kind = take_u8(&mut cur)?;
        match kind {
            SIGNAL_TYPE_OFFER => Ok(SignalMessage::Offer { sdp: take_u16_str(&mut cur)? }),
            SIGNAL_TYPE_ANSWER => Ok(SignalMessage::Answer { sdp: take_u16_str(&mut cur)? }),
            SIGNAL_TYPE_ICE => {
                let candidate = take_u16_str(&mut cur)?;
                let mid = take_u8_str(&mut cur)?;
                let mline = u16::from_be_bytes(take_n(&mut cur, 2)?.try_into().unwrap());
                Ok(SignalMessage::IceCandidate {
                    candidate,
                    sdp_mid: (!mid.is_empty()).then_some(mid),
                    sdp_mline_index: (mline != SIGNAL_NO_MLINE_INDEX).then_some(mline),
                })
            }
            SIGNAL_TYPE_BYE => Ok(SignalMessage::Bye),
            other => Err(CtAgentError::Generic(format!("unknown signal message type {other}"))),
        }
    }
}

#[uniffi::export]
pub fn encode_signal_offer(sdp: String) -> Vec<u8> {
    SignalMessage::Offer { sdp }.encode_inner()
}

#[uniffi::export]
pub fn encode_signal_answer(sdp: String) -> Vec<u8> {
    SignalMessage::Answer { sdp }.encode_inner()
}

/// `sdp_mid`/`sdp_mline_index` mirror `RTCIceCandidateInit`'s own optional fields --
/// `None` for "absent", matching a candidate gathered before the remote description is set.
#[uniffi::export]
pub fn encode_signal_ice_candidate(
    candidate: String,
    sdp_mid: Option<String>,
    sdp_mline_index: Option<u16>,
) -> Vec<u8> {
    SignalMessage::IceCandidate { candidate, sdp_mid, sdp_mline_index }.encode_inner()
}

#[uniffi::export]
pub fn encode_signal_bye() -> Vec<u8> {
    SignalMessage::Bye.encode_inner()
}

/// Decode a signal message received from the peer (after [`NoiseTransport::decrypt`]).
#[uniffi::export]
pub fn decode_signal_message(bytes: Vec<u8>) -> Result<SignalMessage, CtAgentError> {
    SignalMessage::decode_inner(&bytes)
}

// TODO, next iteration:
// - ICE/TURN config (gap 2, ARCHITECTURE.md) -- not part of ct-agent-wasm's surface at
//   all, this is new for the native client, needs its own design.
// - transport fallback state machine (gap 1, ARCHITECTURE.md) -- also new, the reference
//   repo's bug lives in call-transport-shared.js, not in this Rust core.
// - Android Keystore wiring on the Kotlin side (gap 3) -- this crate returns private key
//   hex from generate_holder_identity/generate_noise_identity; the android/ module needs
//   to immediately wrap that in EncryptedSharedPreferences, never store it as returned.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holder_and_noise_identities_round_trip_through_hex() {
        let holder = generate_holder_identity();
        assert_eq!(from_hex(&holder.public_hex).unwrap().len(), 32);
        assert_eq!(from_hex(&holder.private_hex).unwrap().len(), 32);

        let noise = generate_noise_identity();
        assert_eq!(from_hex(&noise.public_hex).unwrap().len(), 32);
        assert_eq!(from_hex(&noise.private_hex).unwrap().len(), 32);
    }

    /// Real signature verification, not just "it returns 64 bytes" -- proves holder_sign
    /// produces a valid ed25519 signature over the exact challenge bytes, verifiable with
    /// the holder's own public key (the edge does exactly this check on join).
    #[test]
    fn holder_sign_produces_a_verifiable_ed25519_signature() {
        use ed25519_dalek::{Verifier, VerifyingKey, Signature};

        let holder = generate_holder_identity();
        let challenge = b"32-byte-ish possession challenge".to_vec();

        let sig_bytes = holder_sign(holder.private_hex.clone(), challenge.clone()).unwrap();
        assert_eq!(sig_bytes.len(), 64, "ed25519 signatures are always 64 bytes");

        let vk = VerifyingKey::from_bytes(&hex32(&holder.public_hex).unwrap()).unwrap();
        let sig = Signature::from_slice(&sig_bytes).unwrap();
        vk.verify(&challenge, &sig).expect("signature must verify against the holder's own public key");

        // A signature over different bytes with the same key must NOT verify -- sanity
        // check that this isn't a no-op / always-true check.
        assert!(vk.verify(b"different message", &sig).is_err());
    }

    #[test]
    fn build_channel_join_request_rejects_garbage_grant_hex_instead_of_panicking() {
        // No way to mint a real SignedChannelGrant here (that needs the channel
        // operator's private key, held server-side, not part of this crate) -- so this
        // test only proves malformed input is a clean Err, not a panic, which matters
        // since this is the first thing called on a hostile/garbled join sequence.
        let result = build_channel_join_request("not-valid-hex!!".to_string(), "relay-only".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn signal_message_offer_answer_bye_round_trip() {
        for msg in [
            SignalMessage::Offer { sdp: "v=0\r\no=- 1 2 IN IP4 127.0.0.1\r\n".to_string() },
            SignalMessage::Answer { sdp: "v=0\r\no=- 3 4 IN IP4 127.0.0.1\r\n".to_string() },
            SignalMessage::Bye,
        ] {
            let encoded = msg.encode_inner();
            let decoded = SignalMessage::decode_inner(&encoded).unwrap();
            assert_eq!(decoded, msg);
        }
    }

    #[test]
    fn signal_message_ice_candidate_round_trips_with_and_without_optional_fields() {
        let full = SignalMessage::IceCandidate {
            candidate: "candidate:1 1 UDP 2130706431 192.0.2.1 54321 typ host".to_string(),
            sdp_mid: Some("audio".to_string()),
            sdp_mline_index: Some(0),
        };
        assert_eq!(SignalMessage::decode_inner(&full.encode_inner()).unwrap(), full);

        // Both optional fields absent (a candidate gathered before the remote description
        // sets mid/mline-index) -- proves None isn't confused with Some(0)/Some("") on the wire.
        let bare = SignalMessage::IceCandidate {
            candidate: "candidate:2 1 UDP 2130706431 192.0.2.2 54322 typ host".to_string(),
            sdp_mid: None,
            sdp_mline_index: None,
        };
        assert_eq!(SignalMessage::decode_inner(&bare.encode_inner()).unwrap(), bare);
    }

    #[test]
    fn encode_signal_helpers_match_direct_enum_construction() {
        assert_eq!(
            encode_signal_offer("sdp-a".to_string()),
            SignalMessage::Offer { sdp: "sdp-a".to_string() }.encode_inner()
        );
        assert_eq!(encode_signal_bye(), SignalMessage::Bye.encode_inner());
        let decoded = decode_signal_message(encode_signal_answer("sdp-b".to_string())).unwrap();
        assert_eq!(decoded, SignalMessage::Answer { sdp: "sdp-b".to_string() });
    }

    #[test]
    fn channel_id_for_link_is_order_independent() {
        let op = to_hex(&[1u8; 32]);
        let a = to_hex(&[2u8; 32]);
        let b = to_hex(&[3u8; 32]);
        let id_ab = channel_id_for_link(op.clone(), a.clone(), b.clone()).unwrap();
        let id_ba = channel_id_for_link(op, b, a).unwrap();
        assert_eq!(id_ab, id_ba, "channel id must not depend on holder argument order");
    }

    /// Real end-to-end Noise_IK handshake + encrypted transport round trip between an
    /// initiator and a responder -- the actual authenticated key exchange two Android
    /// peers (or an Android peer and a native/browser peer) perform to establish a
    /// channel-to-channel session. Not a mock: real `snow`/`ct_common` state machines on
    /// both sides, real ciphertext exchanged, real decrypt-and-compare at the end.
    #[test]
    fn noise_ik_handshake_and_transport_round_trip() {
        let initiator_noise = generate_noise_identity();
        let responder_noise = generate_noise_identity();

        let initiator = NoiseHandshake::new_initiator(
            initiator_noise.private_hex.clone(),
            responder_noise.public_hex.clone(),
        )
        .unwrap();
        let responder = NoiseHandshake::new_responder(responder_noise.private_hex.clone()).unwrap();

        // Noise_IK: one message from initiator, one from responder, then both finished.
        let msg1 = initiator.write_message(vec![]).unwrap();
        responder.read_message(msg1).unwrap();
        let msg2 = responder.write_message(vec![]).unwrap();
        initiator.read_message(msg2).unwrap();

        assert!(initiator.is_finished().unwrap());
        assert!(responder.is_finished().unwrap());

        let initiator_transport = initiator.into_transport().unwrap();
        let responder_transport = responder.into_transport().unwrap();

        let plaintext = b"channel-to-channel: hello from an Android peer".to_vec();
        let ciphertext = initiator_transport.encrypt(plaintext.clone()).unwrap();
        assert_ne!(ciphertext, plaintext, "must actually be encrypted, not passed through");
        let decrypted = responder_transport.decrypt(ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }
}
