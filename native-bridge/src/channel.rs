//! Direct peer-to-peer Agent-Fabric channel session (#382, devsystem.android_native_bridge).
//!
//! ## What this is -- and, just as importantly, what it is NOT
//!
//! The real `ct-agent channel init`/`channel join` CLI flow (`scimbe/ct-agent`,
//! `native/src/channel.rs` + `native/src/channel_run.rs`, ~8.5k lines together) drives a
//! **broker-mediated rendezvous**: an agent presents a `SignedChannelGrant` to the edge's
//! `ct_edge::channel_broker` over QUIC, proves possession of the grant's holder key, and the
//! broker pairs it with another member of the same channel, relaying each side's advertised
//! endpoint (and, in later phases, its own edge-observed reflexive address for NAT
//! traversal/hole-punching). That whole stack -- grants, the broker, rendezvous, NAT punching,
//! the `:443` TLS-TCP relay fallback -- lives across `ct-agent` and CADS-Tunnel's `crates/edge`
//! and is NOT reachable from here without either pulling in `ct_edge` (a server-side crate) or
//! reimplementing the broker's admission protocol client-side inside a mobile `.so`, which is a
//! larger, separate increment. See `docs/channel-join-options.md` for the full survey.
//!
//! What ct-agent's `channel.rs` also makes clear, though: once two members have each other's
//! endpoint and Noise static public key (by ANY means -- the broker rendezvous is just one way
//! to learn them), the actual session is `ct_common::a2a`'s `a2a_initiate`/`a2a_respond` +
//! `a2a_send`/`a2a_recv` -- a REAL `Noise_IK` handshake and encrypted-message framing, generic
//! over any `AsyncRead + AsyncWrite` duplex (ct-agent's own
//! `two_agents_carry_data_over_a_channel_session` test drives it over a real QUIC bi-stream;
//! this module drives the identical primitive over a real TCP socket instead). `ct_common` is
//! already a direct dependency of this crate at the exact pinned tag (`v0.4.13`) that has this
//! module, so wiring it in needs no new CADS-Tunnel-side code and no modification to
//! CADS-Tunnel or ct-agent -- purely additive here.
//!
//! This module implements exactly that slice: given a peer's Noise_IK static public key and a
//! TCP `host:port` the caller already knows how to reach (worked out by the caller, entirely
//! out of band -- for this increment that means two local processes, e.g. two Android
//! emulators/devices on the same LAN with a manually-entered address, NOT real rendezvous/NAT
//! traversal across the open internet), one side dials and runs the Noise_IK **initiator**,
//! the other listens and runs the **responder**. What's real: a genuine mutual Noise_IK
//! handshake (AEAD-authenticated -- dialing with the wrong peer public key fails the handshake
//! outright, it does not silently "succeed" against the wrong peer) and real encrypted
//! `TextMessage`s exchanged over the resulting transport session. What's still missing toward
//! "two Android devices exchange text over a channel": broker-authorized channel membership,
//! rendezvous (so neither side needs to already know the other's address), NAT
//! traversal/hole-punching, and the `:443` relay fallback -- all upstream in ct-agent/CADS-Tunnel,
//! not reimplemented here.
//!
//! ## Why `#[uniffi::export(async_runtime = "tokio")]`
//!
//! UniFFI 0.32 can drive a plain `async fn` export by polling it directly on whatever executor
//! the foreign side (Kotlin) supplies -- no Rust-side runtime required, *provided* the future
//! only awaits runtime-agnostic primitives. That does not hold here: `tokio::net::TcpStream`/
//! `TcpListener` register themselves with a live Tokio reactor the moment they're created, so
//! polling them from a bare foreign executor with no Tokio runtime underneath panics
//! ("there is no reactor running"). The `tokio` feature on the `uniffi` dependency plus
//! `async_runtime = "tokio"` on each export makes UniFFI spawn/poll the future on a real,
//! lazily-initialized Tokio runtime it manages internally, so every `tokio::net` call here
//! always has a reactor under it -- verified empirically by this module's own `cargo test`
//! integration test (`two_native_bridge_instances_complete_a_real_handshake_and_exchange_a_real_message`),
//! which exercises the exact same exported async functions end to end.

use std::net::SocketAddr;
use std::sync::Arc;

use ct_common::a2a::{a2a_initiate, a2a_respond, a2a_send};
use ct_common::noise::{generate_static_keypair, read_frame, StaticKeypair};
use snow::TransportState;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

use crate::message::{decode_text_message, encode_text_message, MessageDecodeError, TextMessage};

/// Errors from the direct peer-to-peer channel path. A real, typed error crossing the UniFFI
/// boundary -- same discipline as [`crate::message::MessageDecodeError`]: no panics across the
/// FFI boundary on attacker-controlled or simply-wrong input (a malformed hex string, an
/// unparseable address, a peer that hangs up mid-handshake).
#[derive(Debug, uniffi::Error)]
pub enum ChannelError {
    /// `peer_public_key_hex` was not exactly 64 lowercase-hex characters.
    InvalidPeerKey { reason: String },
    /// `peer_addr`/`bind_addr` did not parse as a `host:port` socket address.
    InvalidAddress { reason: String },
    /// A TCP-level failure: connect refused, bind failed, connection reset, ...
    Io { reason: String },
    /// The Noise_IK handshake itself failed -- most notably, the peer's actual static key did
    /// not match `peer_public_key_hex` (the AEAD tag check), so no session formed. This is the
    /// authentication property the whole handshake exists for, not an edge case to paper over.
    Handshake { reason: String },
    /// A received frame was not a well-formed [`TextMessage`] once decrypted.
    Decode { reason: String },
}

impl std::fmt::Display for ChannelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChannelError::InvalidPeerKey { reason } => write!(f, "invalid peer public key: {reason}"),
            ChannelError::InvalidAddress { reason } => write!(f, "invalid address: {reason}"),
            ChannelError::Io { reason } => write!(f, "channel I/O error: {reason}"),
            ChannelError::Handshake { reason } => write!(f, "Noise_IK handshake failed: {reason}"),
            ChannelError::Decode { reason } => write!(f, "malformed message received: {reason}"),
        }
    }
}

impl std::error::Error for ChannelError {}

impl From<std::io::Error> for ChannelError {
    fn from(e: std::io::Error) -> Self {
        // a2a_send/a2a_recv/a2a_initiate/a2a_respond all surface as io::Error (ct_common::a2a
        // wraps snow's handshake/AEAD failures into io::ErrorKind::InvalidData -- see its own
        // `noise_io` helper), so a single mapping covers both the transport and the crypto
        // failure without needing to pattern-match error kinds here.
        ChannelError::Io { reason: e.to_string() }
    }
}

impl From<MessageDecodeError> for ChannelError {
    fn from(e: MessageDecodeError) -> Self {
        ChannelError::Decode { reason: e.to_string() }
    }
}

fn hex_encode_32(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Decode 64 lowercase-hex chars into 32 bytes. Mirrors
/// `ct-agent`'s own `channel.rs::decode_hex_32` convention (same lowercase-hex profile
/// `generate_noise_public_key_hex` already returns), but returns a typed [`ChannelError`]
/// instead of `Option` since this is a real FFI input-validation boundary.
fn hex_decode_32(s: &str) -> Result<[u8; 32], ChannelError> {
    if s.len() != 64 {
        return Err(ChannelError::InvalidPeerKey {
            reason: format!("expected 64 lowercase-hex characters, got {} characters", s.len()),
        });
    }
    let mut out = [0u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        *b = u8::from_str_radix(&s[2 * i..2 * i + 2], 16)
            .map_err(|e| ChannelError::InvalidPeerKey { reason: e.to_string() })?;
    }
    Ok(out)
}

fn parse_socket_addr(s: &str) -> Result<SocketAddr, ChannelError> {
    s.parse().map_err(|e: std::net::AddrParseError| ChannelError::InvalidAddress { reason: e.to_string() })
}

/// This device's Noise_IK identity for a direct channel session.
///
/// The private key never crosses the FFI boundary as a value -- unlike
/// `generate_noise_public_key_hex` (which generates, reads the public half, and lets the
/// private half be zeroized on drop within a single call, #250), a real session needs the
/// private key to persist for the lifetime of the handshake and every message after it. This
/// object is how: Kotlin holds an opaque handle (a UniFFI `Object`, reference-counted via
/// `Arc`) and can only ever read `public_key_hex()` off it; the private key lives inside the
/// wrapped `StaticKeypair`, which is still `ZeroizeOnDrop` (#250) and is zeroized the moment
/// this object's last reference is dropped, exactly as if it had never left Rust at all.
#[derive(uniffi::Object)]
pub struct ChannelIdentity {
    keypair: StaticKeypair,
}

#[uniffi::export]
impl ChannelIdentity {
    /// The public half of this identity, as lowercase hex -- the value to hand to the OTHER
    /// side (out of band) as its `peer_public_key_hex`.
    fn public_key_hex(&self) -> String {
        hex_encode_32(&self.keypair.public)
    }
}

/// Generates a fresh Noise_IK identity for a direct channel session, via the same
/// `ct_common::noise::generate_static_keypair()` real crypto call
/// `generate_noise_public_key_hex` already uses -- not a second, drifting implementation.
#[uniffi::export]
pub fn generate_channel_identity() -> Arc<ChannelIdentity> {
    Arc::new(ChannelIdentity { keypair: generate_static_keypair() })
}

/// A live, handshaked direct peer-to-peer channel session: a real Noise_IK transport session
/// over a real TCP socket. `send_text`/`recv_text` are the real thing -- backed by
/// `ct_common::a2a::a2a_send`/`a2a_recv` framing plus the existing `TextMessage` wire codec
/// (`crate::message`) -- not stubs.
#[derive(uniffi::Object)]
pub struct ChannelSession {
    write: Mutex<OwnedWriteHalf>,
    read: Mutex<OwnedReadHalf>,
    transport: Mutex<TransportState>,
}

impl ChannelSession {
    fn new(read: OwnedReadHalf, write: OwnedWriteHalf, transport: TransportState) -> Arc<Self> {
        Arc::new(Self { write: Mutex::new(write), read: Mutex::new(read), transport: Mutex::new(transport) })
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl ChannelSession {
    /// Encrypts `message` (via `encode_text_message`'s existing JSON wire form) and sends it
    /// over the established Noise_IK transport session.
    ///
    /// Held locks: this takes the write half's and the transport session's locks for the
    /// duration of one send -- both fast, bounded operations (an in-memory encrypt plus a
    /// `write_all` of a small framed message), never an indefinite wait. See [`Self::recv_text`]'s
    /// own doc comment for why holding `transport` briefly here is safe now, where it wasn't
    /// on the receive side.
    async fn send_text(&self, message: TextMessage) -> Result<(), ChannelError> {
        let bytes = encode_text_message(message);
        let mut write = self.write.lock().await;
        let mut transport = self.transport.lock().await;
        a2a_send(&mut *write, &mut transport, &bytes).await?;
        Ok(())
    }

    /// Receives and decrypts one [`TextMessage`] from the established Noise_IK transport
    /// session. Blocks (asynchronously) until a message arrives or the connection is lost.
    ///
    /// Real deadlock, found and fixed (labor-setup.com, issue #13): this used to go through
    /// `ct_common::a2a::a2a_recv`, which holds `transport` for its ENTIRE body, including the
    /// indefinite wait for the next frame to arrive on the wire. `MainActivity`'s real usage
    /// calls `recv_text` in a loop the instant a session connects, so `transport` was
    /// effectively locked forever from that point on -- any concurrent `send_text` could never
    /// acquire it. Fixed by inlining `a2a_recv`'s own two real steps (verified against its
    /// actual body, not guessed) with the lock scoped to only the second one: wait for a raw
    /// frame with ONLY the `read` lock held (`transport` is not needed for that wait at all --
    /// framing and decryption are genuinely separate steps), then take `transport` just long
    /// enough for the synchronous, bounded `read_message` decrypt. A concurrent `send_text` can
    /// now really acquire `transport` while `recv_text` is blocked on the network, which is
    /// where it spends nearly all of its time.
    async fn recv_text(&self) -> Result<TextMessage, ChannelError> {
        let ciphertext = {
            let mut read = self.read.lock().await;
            read_frame(&mut *read).await?
        };
        let bytes = {
            let mut transport = self.transport.lock().await;
            let mut plaintext = vec![0u8; ciphertext.len()];
            let n = transport
                .read_message(&ciphertext, &mut plaintext)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("noise: {e}")))?;
            plaintext.truncate(n);
            plaintext
        };
        Ok(decode_text_message(bytes)?)
    }
}

/// Dials `peer_addr` (a `host:port` TCP address) and runs the **initiator** half of a direct
/// Noise_IK handshake pinned to `peer_public_key_hex`. Fails -- does not silently
/// "succeed" against the wrong peer -- if the remote's actual Noise static key does not match
/// `peer_public_key_hex`: `ct_common::a2a::a2a_initiate` encrypts its second handshake message
/// to that pinned key, so a wrong key fails the responder's read, which surfaces here as
/// `ChannelError::Handshake`/`Io` (the TCP connection drops when the responder's read errors).
#[uniffi::export(async_runtime = "tokio")]
pub async fn dial_channel_direct(
    identity: Arc<ChannelIdentity>,
    peer_public_key_hex: String,
    peer_addr: String,
) -> Result<Arc<ChannelSession>, ChannelError> {
    let peer_public_key = hex_decode_32(&peer_public_key_hex)?;
    let addr = parse_socket_addr(&peer_addr)?;
    let stream = TcpStream::connect(addr).await?;
    let _ = stream.set_nodelay(true); // best-effort: lower handshake/message latency, not correctness-critical
    let (mut read, mut write) = stream.into_split();
    let transport = a2a_initiate(&mut write, &mut read, &identity.keypair.private, &peer_public_key)
        .await
        .map_err(|e| ChannelError::Handshake { reason: e.to_string() })?;
    Ok(ChannelSession::new(read, write, transport))
}

/// A bound TCP listener waiting to accept exactly one direct channel session. Split from
/// `dial_channel_direct`'s single-call shape because the responder side of a first real
/// increment like this one typically needs to bind an ephemeral port (`"0.0.0.0:0"`) and learn
/// what the OS actually assigned via [`ChannelListener::local_addr`] BEFORE the peer can dial
/// it -- something one `accept_and_connect(...)`-style async call could not hand back
/// mid-flight.
#[derive(uniffi::Object)]
pub struct ChannelListener {
    listener: TcpListener,
}

#[uniffi::export]
impl ChannelListener {
    /// This listener's bound local address (`host:port`) -- what to hand to the dialing peer
    /// as `peer_addr`, out of band. Synchronous (no `.await` needed): `local_addr()` on an
    /// already-bound `tokio::net::TcpListener` is itself a plain, non-blocking syscall, so this
    /// is a separate, plain `#[uniffi::export]` block from `accept()`'s below rather than
    /// mixing sync and async methods under one `async_runtime = "tokio"` block.
    fn local_addr(&self) -> Result<String, ChannelError> {
        Ok(self.listener.local_addr()?.to_string())
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl ChannelListener {
    /// Accepts exactly one incoming TCP connection and runs the **responder** half of the
    /// direct Noise_IK handshake against it, learning the peer's static key from the
    /// handshake itself (the responder does not need to know the initiator's key in advance --
    /// only the initiator pins the responder's).
    async fn accept(&self, identity: Arc<ChannelIdentity>) -> Result<Arc<ChannelSession>, ChannelError> {
        let (stream, _peer_addr) = self.listener.accept().await?;
        let _ = stream.set_nodelay(true);
        let (mut read, mut write) = stream.into_split();
        let transport = a2a_respond(&mut write, &mut read, &identity.keypair.private)
            .await
            .map_err(|e| ChannelError::Handshake { reason: e.to_string() })?;
        Ok(ChannelSession::new(read, write, transport))
    }
}

/// Binds `bind_addr` (a `host:port` TCP address, e.g. `"0.0.0.0:0"` to let the OS assign a
/// free port) and returns a [`ChannelListener`] ready to `accept()` one direct channel session.
#[uniffi::export(async_runtime = "tokio")]
pub async fn bind_channel_listener(bind_addr: String) -> Result<Arc<ChannelListener>, ChannelError> {
    let addr = parse_socket_addr(&bind_addr)?;
    let listener = TcpListener::bind(addr).await?;
    Ok(Arc::new(ChannelListener { listener }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The payoff test: two independent `ChannelIdentity`s (as if two separate native-bridge
    /// instances / two separate Android processes), one binding+listening, one dialing, drive
    /// the exact exported async UniFFI functions end to end over a REAL TCP socket on
    /// loopback -- a real Noise_IK handshake completes and a real encrypted `TextMessage`
    /// round-trips through it in both directions. This is the local-process-to-local-process
    /// proof the task calls for; it is explicitly NOT proof of real device-to-device messaging
    /// over a real channel (no broker, no rendezvous, no NAT traversal -- see the module docs).
    #[tokio::test]
    async fn two_native_bridge_instances_complete_a_real_handshake_and_exchange_a_real_message() {
        let responder_identity = generate_channel_identity();
        let initiator_identity = generate_channel_identity();

        let listener = bind_channel_listener("127.0.0.1:0".to_string()).await.expect("bind");
        let bound_addr = listener.local_addr().expect("local_addr");

        let responder_public_hex = responder_identity.public_key_hex();
        let initiator_public_hex = initiator_identity.public_key_hex();

        let accept_task = tokio::spawn({
            let responder_identity = responder_identity.clone();
            async move { listener.accept(responder_identity).await }
        });

        let initiator_session = dial_channel_direct(initiator_identity.clone(), responder_public_hex.clone(), bound_addr)
            .await
            .expect("initiator completes the handshake");
        let responder_session = accept_task.await.expect("accept task").expect("responder completes the handshake");

        // Initiator -> responder, a real encrypted TextMessage.
        let outbound = crate::message::new_text_message(initiator_public_hex.clone(), "hello from device A".to_string());
        initiator_session.send_text(outbound.clone()).await.expect("send from initiator");
        let received = responder_session.recv_text().await.expect("recv on responder");
        assert_eq!(received, outbound, "the responder decrypts exactly what the initiator sent");

        // ...and the reverse direction, proving the session is genuinely bidirectional, not
        // just "initiator writes, responder reads".
        let reply = crate::message::new_text_message(responder_public_hex.clone(), "hello back from device B".to_string());
        responder_session.send_text(reply.clone()).await.expect("send from responder");
        let received_reply = initiator_session.recv_text().await.expect("recv on initiator");
        assert_eq!(received_reply, reply, "the initiator decrypts exactly what the responder sent");
    }

    /// The real deadlock (labor-setup.com, issue #13), reproduced and proven fixed: starts
    /// `recv_text` FIRST, exactly like `MainActivity`'s real `receiveLoop()` does the instant a
    /// session connects -- it blocks waiting for a message that hasn't been sent yet. While that
    /// call is genuinely in flight (not raced -- a real `sleep` gives it time to actually reach
    /// its indefinite wait), a concurrent `send_text` on the OTHER session must still complete.
    /// On the old code (a single `transport` lock held for `recv_text`'s entire body) this test
    /// hangs forever; `tokio::time::timeout` turns that into a real, clear test failure instead
    /// of an actually-frozen CI job.
    #[tokio::test]
    async fn a_concurrent_send_does_not_deadlock_behind_an_in_flight_recv() {
        let responder_identity = generate_channel_identity();
        let initiator_identity = generate_channel_identity();

        let listener = bind_channel_listener("127.0.0.1:0".to_string()).await.expect("bind");
        let bound_addr = listener.local_addr().expect("local_addr");
        let responder_public_hex = responder_identity.public_key_hex();
        let initiator_public_hex = initiator_identity.public_key_hex();

        let accept_task = tokio::spawn({
            let responder_identity = responder_identity.clone();
            async move { listener.accept(responder_identity).await }
        });
        let initiator_session = dial_channel_direct(initiator_identity.clone(), responder_public_hex, bound_addr)
            .await
            .expect("initiator completes the handshake");
        let responder_session = accept_task.await.expect("accept task").expect("responder completes the handshake");

        // Start receiving on the responder FIRST -- this is the real ordering that deadlocked:
        // nothing has been sent yet, so this genuinely blocks on the network wait.
        let recv_task = tokio::spawn(async move { responder_session.recv_text().await });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await; // let recv_text actually reach its wait

        let outbound = crate::message::new_text_message(initiator_public_hex, "should not deadlock".to_string());
        let send_result = tokio::time::timeout(std::time::Duration::from_secs(5), initiator_session.send_text(outbound.clone())).await;
        assert!(send_result.is_ok(), "send_text must complete within 5s even while a recv_text is genuinely in flight -- it deadlocked instead");
        send_result.unwrap().expect("the send itself must succeed");

        let received = tokio::time::timeout(std::time::Duration::from_secs(5), recv_task)
            .await
            .expect("recv_task must finish within 5s")
            .expect("recv task")
            .expect("recv_text must succeed");
        assert_eq!(received, outbound, "the responder must decrypt exactly what was sent while it was waiting");
    }

    /// Authentication property, not an edge case: dialing with a WRONG `peer_public_key_hex`
    /// (i.e. pinning a different identity than who's actually listening) must fail the
    /// handshake, not silently succeed against the real listener under a false identity.
    #[tokio::test]
    async fn dialing_with_the_wrong_peer_public_key_fails_the_handshake() {
        let responder_identity = generate_channel_identity();
        let initiator_identity = generate_channel_identity();
        let impostor_identity = generate_channel_identity(); // a third identity, NOT who's listening

        let listener = bind_channel_listener("127.0.0.1:0".to_string()).await.expect("bind");
        let bound_addr = listener.local_addr().expect("local_addr");

        let accept_task = tokio::spawn(async move { listener.accept(responder_identity).await });

        // The initiator pins the IMPOSTOR's public key, not the real responder's.
        let result = dial_channel_direct(initiator_identity, impostor_identity.public_key_hex(), bound_addr).await;
        assert!(result.is_err(), "a handshake pinned to the wrong peer key must fail, not succeed");

        // The responder side observes the same failure (its read of the initiator's message
        // fails once the initiator's handshake write is encrypted to the wrong key), not a
        // silently-admitted impostor session.
        let responder_result = accept_task.await.expect("accept task");
        assert!(responder_result.is_err(), "the responder side must also see the handshake fail");
    }

    /// A real FFI input-validation boundary: a malformed `peer_public_key_hex` (wrong length,
    /// non-hex characters) is a typed `ChannelError`, never a panic.
    #[tokio::test]
    async fn dial_rejects_a_malformed_peer_key_without_panicking() {
        let identity = generate_channel_identity();
        // `ChannelSession` has no `Debug` impl (it wraps live socket halves + a Noise
        // transport session, not something meaningful to print) so `Result::expect_err`
        // (which requires `T: Debug`) doesn't fit here -- match instead.
        match dial_channel_direct(identity, "not-hex".to_string(), "127.0.0.1:1".to_string()).await {
            Err(ChannelError::InvalidPeerKey { .. }) => {}
            Err(other) => panic!("expected InvalidPeerKey, got a different ChannelError: {other}"),
            Ok(_) => panic!("a malformed peer key must be rejected, not accepted"),
        }
    }

    /// Likewise for a malformed address.
    #[tokio::test]
    async fn dial_rejects_a_malformed_address_without_panicking() {
        let identity = generate_channel_identity();
        let peer = generate_channel_identity();
        match dial_channel_direct(identity, peer.public_key_hex(), "not-an-address".to_string()).await {
            Err(ChannelError::InvalidAddress { .. }) => {}
            Err(other) => panic!("expected InvalidAddress, got a different ChannelError: {other}"),
            Ok(_) => panic!("a malformed address must be rejected, not accepted"),
        }
    }
}
