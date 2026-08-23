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
use std::time::Duration;

use ct_common::a2a::{a2a_initiate, a2a_respond, a2a_send};
use ct_common::noise::{generate_static_keypair, read_frame, StaticKeypair};
use snow::TransportState;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;

use crate::message::{decode_text_message, encode_text_message, MessageDecodeError, TextMessage};

/// Bound on the TCP connect and the Noise_IK handshake round-trip, on both the dial
/// (`dial_channel_direct`) and accept (`ChannelListener::accept`, after a connection is already
/// accepted) sides. Neither `TcpStream::connect` nor `a2a_initiate`/`a2a_respond` has any
/// built-in timeout -- this is a generic transport-agnostic primitive, and applying a bound is
/// the caller's job. Without one: a `dial_channel_direct` call against an unreachable/
/// blackholed address (this increment's own doc comment describes the address as
/// "manually-entered" -- exactly the kind of input a user can simply mistype) hangs the calling
/// Kotlin coroutine forever with no way to give up short of killing the app; worse, on the
/// accept side, `ChannelListener` is documented as single-shot ("waiting to accept exactly one
/// direct channel session") -- a peer that completes the TCP connect but then never sends its
/// handshake message (a stray port scanner, a stalled network path) leaves `accept()` blocked
/// forever on that one straggler, and the real intended peer can never be served at all. 20s
/// matches the dial-timeout convention already used elsewhere in this session's sibling repos
/// (e.g. CADS-p2p-vault's `--dial-timeout-secs` default).
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(20);

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
///
/// The guard counts CHARACTERS, not bytes, and rejects the whole string up front unless every
/// character is in the hex alphabet. That is load-bearing, not defensive style: the previous
/// version gated on `str::len()` (a BYTE count) and then indexed the same `&str` by byte range
/// (`&s[2 * i..2 * i + 2]`). A 64-BYTE key containing any multi-byte UTF-8 character therefore
/// passed the length check and then landed a slice endpoint mid-character, and range-indexing a
/// `str` at a non-char-boundary panics -- an abort unwinding across the FFI boundary out of real
/// user input (the peer-key text field), not a handled error. Validating the alphabet first also
/// makes every index below provably single-byte ASCII.
fn hex_decode_32(s: &str) -> Result<[u8; 32], ChannelError> {
    let digits: Vec<char> = s.chars().collect();
    if digits.len() != 64 {
        return Err(ChannelError::InvalidPeerKey {
            reason: format!("expected 64 lowercase-hex characters, got {} characters", digits.len()),
        });
    }
    let mut out = [0u8; 32];
    for (i, b) in out.iter_mut().enumerate() {
        let hi = hex_digit(digits[2 * i])?;
        let lo = hex_digit(digits[2 * i + 1])?;
        *b = (hi << 4) | lo;
    }
    Ok(out)
}

/// Accepts the same alphabet `u8::from_str_radix(_, 16)` did (so an uppercase-hex key a user
/// pasted from elsewhere keeps working), but as a total function over `char` -- no slicing.
fn hex_digit(c: char) -> Result<u8, ChannelError> {
    c.to_digit(16).map(|d| d as u8).ok_or_else(|| ChannelError::InvalidPeerKey {
        reason: format!("{c:?} is not a hexadecimal character"),
    })
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
    /// The peer's real, Noise_IK-authenticated static public key, when this session actually
    /// knows it -- `Some` on the dialer/initiator side (it's the caller-supplied,
    /// handshake-pinned `peer_public_key_hex` `dial_channel_direct` already verified: a wrong
    /// key fails the handshake outright, so reaching a live session means this key is real,
    /// not just claimed). `None` on the listener/responder side: `ct_common::a2a::a2a_respond`
    /// (pinned dependency, a separate repo) learns the initiator's static key internally during
    /// the handshake but does not return it, only the resulting `TransportState` -- so there is
    /// genuinely nothing authenticated to pin here today. Closing that side needs an upstream
    /// `ct_common::a2a` change, correctly scoped as its own separate increment, not guessed at
    /// or worked around by re-implementing the raw Noise handshake here.
    ///
    /// Real gap found live 2026-08-07 (#382 goal doc §8, DAU lens): [`TextMessage::sender_pubkey`]
    /// on a received message was, until this fix, taken verbatim from the wire -- entirely
    /// self-reported by whoever sent it, never checked against anything the receiving side
    /// actually authenticated. Harmless today only because nothing in the GUI currently trusts
    /// this field for anything (message direction alone drives rendering) -- but a forged value
    /// would have been silently accepted the moment that changes (e.g. real multi-peer display).
    /// [`Self::recv_text`] now overrides it with this real, known key whenever one is available.
    known_peer_public_key_hex: Option<String>,
}

impl ChannelSession {
    fn new(read: OwnedReadHalf, write: OwnedWriteHalf, transport: TransportState, known_peer_public_key_hex: Option<String>) -> Arc<Self> {
        Arc::new(Self { write: Mutex::new(write), read: Mutex::new(read), transport: Mutex::new(transport), known_peer_public_key_hex })
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

    /// Test-only infrastructure for requirement #22 criterion 2's remaining on-device half
    /// (`#382` goal doc, `development-system-goal.md`): `send_text` above always encodes a
    /// well-formed [`TextMessage`] -- there is no way to construct a malformed frame through the
    /// real production API, by design. This crate's own hermetic test
    /// (`recv_text_returns_a_typed_decode_error_for_a_real_malformed_frame_and_stays_usable_afterward`)
    /// already proves the native-session half of criterion 2 by reaching into this same session's
    /// private fields directly, but a Kotlin instrumented test has no such access -- it can only
    /// call what's exported over UniFFI. This is the same real, authenticated-hostile-peer
    /// mechanism that test already uses (encrypts+frames `bytes` verbatim via the exact same
    /// `a2a_send` `send_text` calls internally, skipping only `encode_text_message`), exposed as
    /// a real UniFFI function so an on-device test can drive the identical scenario through two
    /// real sessions on a real emulator, then inspect `logcat` for a Rust panic marker -- not a
    /// mock, not a simulated failure. Deliberately named and documented as test infrastructure,
    /// not a production capability: nothing in `MainActivity`'s own UI ever calls this.
    async fn send_raw_bytes_for_testing(&self, bytes: Vec<u8>) -> Result<(), ChannelError> {
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
        let mut message = decode_text_message(bytes)?;
        // See `known_peer_public_key_hex`'s own doc comment: override the wire's
        // self-reported value with the real, authenticated key whenever this
        // session actually knows one, rather than trusting an unverified claim.
        if let Some(real_key) = &self.known_peer_public_key_hex {
            message.sender_pubkey = real_key.clone();
        }
        Ok(message)
    }
}

/// Dials `peer_addr` (a `host:port` TCP address) and runs the **initiator** half of a direct
/// Noise_IK handshake pinned to `peer_public_key_hex`. Fails -- does not silently
/// "succeed" against the wrong peer -- if the remote's actual Noise static key does not match
/// `peer_public_key_hex`: `ct_common::a2a::a2a_initiate` encrypts its second handshake message
/// to that pinned key, so a wrong key fails the responder's read, which surfaces here as
/// `ChannelError::Handshake`/`Io` (the TCP connection drops when the responder's read errors).
/// The real body, parameterized over the timeout so tests can prove the timeout actually fires
/// without waiting the real [`HANDSHAKE_TIMEOUT`] -- `dial_channel_direct` below is a thin
/// wrapper that always passes the real constant.
async fn dial_channel_direct_with_timeout(
    identity: Arc<ChannelIdentity>,
    peer_public_key_hex: String,
    peer_addr: String,
    timeout: Duration,
) -> Result<Arc<ChannelSession>, ChannelError> {
    let peer_public_key = hex_decode_32(&peer_public_key_hex)?;
    let addr = parse_socket_addr(&peer_addr)?;
    let stream = tokio::time::timeout(timeout, TcpStream::connect(addr))
        .await
        .map_err(|_| ChannelError::Io { reason: format!("connect timed out after {timeout:?}") })??;
    let _ = stream.set_nodelay(true); // best-effort: lower handshake/message latency, not correctness-critical
    let (mut read, mut write) = stream.into_split();
    let transport = tokio::time::timeout(
        timeout,
        a2a_initiate(&mut write, &mut read, &identity.keypair.private, &peer_public_key),
    )
    .await
    .map_err(|_| ChannelError::Handshake { reason: format!("handshake timed out after {timeout:?}") })?
    .map_err(|e| ChannelError::Handshake { reason: e.to_string() })?;
    // Canonical lowercase-hex re-encoding of the real, decoded, handshake-pinned key -- not
    // the raw caller-supplied string verbatim, so a caller passing mixed-case hex (still valid
    // input to hex_decode_32) can never produce a sender_pubkey that fails a later byte-exact
    // comparison against generate_noise_public_key_hex()'s own always-lowercase output.
    Ok(ChannelSession::new(read, write, transport, Some(hex_encode_32(&peer_public_key))))
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
    dial_channel_direct_with_timeout(identity, peer_public_key_hex, peer_addr, HANDSHAKE_TIMEOUT).await
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

impl ChannelListener {
    /// The real body, parameterized over the timeout so tests can prove the timeout actually
    /// fires without waiting the real [`HANDSHAKE_TIMEOUT`] -- `accept` below is a thin wrapper
    /// that always passes the real constant.
    async fn accept_with_timeout(
        &self,
        identity: Arc<ChannelIdentity>,
        handshake_timeout: Duration,
    ) -> Result<Arc<ChannelSession>, ChannelError> {
        let (stream, _peer_addr) = self.listener.accept().await?;
        let _ = stream.set_nodelay(true);
        let (mut read, mut write) = stream.into_split();
        // The `accept()` wait above stays unbounded (that's a listening socket doing its job --
        // it should wait as long as the caller wants it to). The handshake AFTER a connection is
        // already accepted is different: `ChannelListener` is single-shot, so a peer that
        // connects and then never sends its handshake message (a stray scanner, a stalled
        // network path) would otherwise leave this call -- and the real intended peer -- stuck
        // forever. See `HANDSHAKE_TIMEOUT`'s own doc comment.
        let transport = tokio::time::timeout(
            handshake_timeout,
            a2a_respond(&mut write, &mut read, &identity.keypair.private),
        )
        .await
        .map_err(|_| ChannelError::Handshake { reason: format!("handshake timed out after {handshake_timeout:?}") })?
        .map_err(|e| ChannelError::Handshake { reason: e.to_string() })?;
        // `None`: see `known_peer_public_key_hex`'s own doc comment -- a2a_respond doesn't
        // hand back the initiator's key it authenticated internally, so there is nothing real
        // to pin here yet. recv_text falls back to the wire's own (unverified) value.
        Ok(ChannelSession::new(read, write, transport, None))
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl ChannelListener {
    /// Accepts exactly one incoming TCP connection and runs the **responder** half of the
    /// direct Noise_IK handshake against it, learning the peer's static key from the
    /// handshake itself (the responder does not need to know the initiator's key in advance --
    /// only the initiator pins the responder's).
    async fn accept(&self, identity: Arc<ChannelIdentity>) -> Result<Arc<ChannelSession>, ChannelError> {
        self.accept_with_timeout(identity, HANDSHAKE_TIMEOUT).await
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

    #[tokio::test]
    /// Requirement #22 criterion 2's own real claim, proven at the native-session level: a
    /// malformed frame received over an established, authenticated channel must surface as a
    /// real, typed `ChannelError::Decode` -- never a panic -- and criterion 3's claim that the
    /// channel stays usable afterward. `send_text` always encodes a well-formed `TextMessage`
    /// (there is no way to construct a malformed one through it), so this sends raw, deliberately
    /// invalid bytes through the exact same real `a2a_send` framing+encryption `send_text` uses
    /// internally -- the real hostile-peer scenario: an authenticated session sending bytes that
    /// don't decode as a real message, not a fake/mocked failure.
    async fn recv_text_returns_a_typed_decode_error_for_a_real_malformed_frame_and_stays_usable_afterward() {
        let responder_identity = generate_channel_identity();
        let initiator_identity = generate_channel_identity();

        let listener = bind_channel_listener("127.0.0.1:0".to_string()).await.expect("bind");
        let bound_addr = listener.local_addr().expect("local_addr");
        let responder_public_hex = responder_identity.public_key_hex();

        let accept_task = tokio::spawn({
            let responder_identity = responder_identity.clone();
            async move { listener.accept(responder_identity).await }
        });
        let initiator_session = dial_channel_direct(initiator_identity, responder_public_hex.clone(), bound_addr)
            .await
            .expect("initiator completes the handshake");
        let responder_session = accept_task.await.expect("accept task").expect("responder completes the handshake");

        // The real hostile/corrupted-peer case: bytes that are genuinely not a well-formed
        // TextMessage, sent through the real, authenticated, encrypted transport -- not a
        // mocked failure injected some other way.
        {
            let mut write = responder_session.write.lock().await;
            let mut transport = responder_session.transport.lock().await;
            a2a_send(&mut *write, &mut transport, b"not a real TextMessage at all")
                .await
                .expect("the real encrypted send of the hostile bytes itself must succeed");
        }

        let result = initiator_session.recv_text().await;
        match result {
            Err(ChannelError::Decode { .. }) => {}
            other => panic!("a malformed frame must surface as a real, typed ChannelError::Decode, never a panic or a different error: {other:?}"),
        }

        // Criterion 3's own claim, proven at this level too (already fixed on the Kotlin
        // caller's side -- this proves the native session itself was never the problem): a real,
        // well-formed message from the SAME peer immediately after a malformed one still arrives
        // normally, on the same session, no reconnect needed.
        let followup = crate::message::new_text_message(responder_public_hex.clone(), "still here after the bad frame".to_string());
        responder_session.send_text(followup.clone()).await.expect("send after the malformed frame");
        let received = initiator_session
            .recv_text()
            .await
            .expect("recv_text must still work on the same session after a prior malformed frame");
        assert_eq!(received.body, followup.body, "the real, well-formed follow-up message must arrive intact");
    }

    #[tokio::test]
    /// Requirement #22 criterion 2's on-device half is blocked on a real gap: no UniFFI API
    /// exists for Kotlin to send deliberately malformed bytes (`send_text` only ever encodes a
    /// well-formed message). This proves the new `send_raw_bytes_for_testing` -- the exact
    /// UniFFI-exported surface a Kotlin instrumented test will call -- reproduces the same real
    /// scenario the private-field-access test above already proved, through the actual public API
    /// surface, not internals. De-risks the follow-up instrumented test: if this passes, the
    /// Kotlin side calling the identical function should behave identically.
    async fn send_raw_bytes_for_testing_reproduces_the_real_malformed_frame_scenario_through_the_public_api() {
        let responder_identity = generate_channel_identity();
        let initiator_identity = generate_channel_identity();

        let listener = bind_channel_listener("127.0.0.1:0".to_string()).await.expect("bind");
        let bound_addr = listener.local_addr().expect("local_addr");
        let responder_public_hex = responder_identity.public_key_hex();

        let accept_task = tokio::spawn({
            let responder_identity = responder_identity.clone();
            async move { listener.accept(responder_identity).await }
        });
        let initiator_session = dial_channel_direct(initiator_identity, responder_public_hex.clone(), bound_addr)
            .await
            .expect("initiator completes the handshake");
        let responder_session = accept_task.await.expect("accept task").expect("responder completes the handshake");

        // The public API a Kotlin instrumented test will actually call -- no reaching into
        // private fields here.
        responder_session
            .send_raw_bytes_for_testing(b"not a real TextMessage at all".to_vec())
            .await
            .expect("the real encrypted send of the hostile bytes itself must succeed");

        let result = initiator_session.recv_text().await;
        match result {
            Err(ChannelError::Decode { .. }) => {}
            other => panic!("a malformed frame sent via the public test API must surface as a real, typed ChannelError::Decode, never a panic: {other:?}"),
        }

        // Criterion 3 again, through the public surface: the session survives and a real
        // follow-up message still arrives.
        let followup = crate::message::new_text_message(responder_public_hex.clone(), "still here after the bad frame via the public API".to_string());
        responder_session.send_text(followup.clone()).await.expect("send after the malformed frame");
        let received = initiator_session
            .recv_text()
            .await
            .expect("recv_text must still work on the same session after a prior malformed frame");
        assert_eq!(received.body, followup.body, "the real, well-formed follow-up message must arrive intact");
    }

    #[tokio::test]
    /// Real gap found live 2026-08-07 (#382 goal doc §8, DAU lens): `TextMessage::sender_pubkey`
    /// used to be taken verbatim from the wire, entirely self-reported -- nothing checked it
    /// against anything the receiving side actually authenticated during the real Noise_IK
    /// handshake. Proves the fix on the one side that can be closed within this crate alone (the
    /// dialer/initiator already has the peer's handshake-pinned key in scope; see
    /// `known_peer_public_key_hex`'s own doc comment for why the listener/responder side is a
    /// separate, correctly-scoped gap that needs an upstream `ct_common::a2a` change instead):
    /// the responder sends a real message with a deliberately FORGED `sender_pubkey`, and the
    /// initiator's `recv_text` must return the real, authenticated key, not the forged claim.
    async fn recv_text_on_the_initiator_side_overrides_a_forged_sender_pubkey_with_the_real_authenticated_key() {
        let responder_identity = generate_channel_identity();
        let initiator_identity = generate_channel_identity();
        let impostor_identity = generate_channel_identity(); // whose key the responder will falsely claim

        let listener = bind_channel_listener("127.0.0.1:0".to_string()).await.expect("bind");
        let bound_addr = listener.local_addr().expect("local_addr");
        let responder_public_hex = responder_identity.public_key_hex();
        let impostor_public_hex = impostor_identity.public_key_hex();

        let accept_task = tokio::spawn({
            let responder_identity = responder_identity.clone();
            async move { listener.accept(responder_identity).await }
        });
        let initiator_session = dial_channel_direct(initiator_identity, responder_public_hex.clone(), bound_addr)
            .await
            .expect("initiator completes the handshake");
        let responder_session = accept_task.await.expect("accept task").expect("responder completes the handshake");

        // The responder deliberately lies about who it is -- a real, well-formed TextMessage
        // whose sender_pubkey claims to be the impostor's key, not the responder's own real one.
        let forged = crate::message::new_text_message(impostor_public_hex.clone(), "trust me, I'm someone else".to_string());
        responder_session.send_text(forged.clone()).await.expect("send from responder");

        let received = initiator_session.recv_text().await.expect("recv on initiator");
        assert_ne!(received.sender_pubkey, impostor_public_hex, "the forged claim must never survive to the caller");
        assert_eq!(received.sender_pubkey, responder_public_hex, "must be overridden with the real, handshake-authenticated key");
        assert_eq!(received.body, forged.body, "only sender_pubkey is overridden -- the real message content is untouched");
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

    /// Real gap, found while auditing this module fresh (no prior review had covered it): the
    /// TCP connect and Noise_IK handshake had no timeout at all on either side. This proves the
    /// dial side: a real listener accepts the TCP connection but is deliberately never told to
    /// run its own handshake, so nothing ever answers -- exactly the "connected but silent" peer
    /// the timeout exists for. Uses `dial_channel_direct_with_timeout` directly with a short
    /// duration so the test doesn't have to wait the real `HANDSHAKE_TIMEOUT` (20s); the outer
    /// `tokio::time::timeout` is a guard so a regression (the fix reverted) fails this test
    /// quickly instead of hanging the whole suite.
    #[tokio::test]
    async fn dial_channel_direct_times_out_if_the_peer_accepts_but_never_completes_the_handshake() {
        let raw_listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let bound_addr = raw_listener.local_addr().expect("local_addr").to_string();
        let accept_task = tokio::spawn(async move {
            let (stream, _) = raw_listener.accept().await.expect("accept");
            tokio::time::sleep(Duration::from_secs(10)).await; // hold the connection open, silent
            drop(stream);
        });

        let identity = generate_channel_identity();
        let peer = generate_channel_identity();
        let start = std::time::Instant::now();
        let result = tokio::time::timeout(
            Duration::from_secs(3), // must never actually fire against the fix
            dial_channel_direct_with_timeout(identity, peer.public_key_hex(), bound_addr, Duration::from_millis(200)),
        )
        .await
        .expect("dial_channel_direct_with_timeout must return well within the outer guard, not hang");
        let elapsed = start.elapsed();

        match result {
            Err(ChannelError::Handshake { reason }) => assert!(reason.contains("timed out"), "reason: {reason}"),
            Err(other) => panic!("expected a Handshake timeout error, got a different ChannelError: {other}"),
            // `ChannelSession` has no `Debug` impl (see `dial_rejects_a_malformed_peer_key_without_panicking`'s own comment).
            Ok(_) => panic!("expected a Handshake timeout error, got a live session instead"),
        }
        assert!(elapsed < Duration::from_secs(2), "must fail quickly, not hang -- took {elapsed:?}");

        accept_task.abort();
    }

    /// The accept-side half of the same gap, and the more serious one: `ChannelListener` is
    /// single-shot, so without a timeout a peer that connects and then never sends its handshake
    /// message would leave `accept()` -- and the real intended peer -- stuck forever. Uses
    /// `accept_with_timeout` directly with a short duration for the same reason as the dial-side
    /// test above.
    #[tokio::test]
    async fn accept_times_out_if_a_peer_connects_but_never_sends_a_handshake_message() {
        let listener = bind_channel_listener("127.0.0.1:0".to_string()).await.expect("bind");
        let bound_addr = listener.local_addr().expect("local_addr");

        let silent_peer = tokio::spawn(async move {
            let stream = TcpStream::connect(bound_addr).await.expect("connect");
            tokio::time::sleep(Duration::from_secs(10)).await; // connect, then say nothing
            drop(stream);
        });

        let identity = generate_channel_identity();
        let start = std::time::Instant::now();
        let result = tokio::time::timeout(
            Duration::from_secs(3), // must never actually fire against the fix
            listener.accept_with_timeout(identity, Duration::from_millis(200)),
        )
        .await
        .expect("accept_with_timeout must return well within the outer guard, not hang");
        let elapsed = start.elapsed();

        match result {
            Err(ChannelError::Handshake { reason }) => assert!(reason.contains("timed out"), "reason: {reason}"),
            Err(other) => panic!("expected a Handshake timeout error, got a different ChannelError: {other}"),
            // `ChannelSession` has no `Debug` impl (see `dial_rejects_a_malformed_peer_key_without_panicking`'s own comment).
            Ok(_) => panic!("expected a Handshake timeout error, got a live session instead"),
        }
        assert!(elapsed < Duration::from_secs(2), "must fail quickly, not hang -- took {elapsed:?}");

        silent_peer.abort();
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

    /// Regression test for the real defect found at devsystem.review (run webconference-android,
    /// iteration 22), tracked as requirement #17. `hex_decode_32` used to gate on `str::len()`
    /// -- a BYTE count -- and then index the same `&str` by byte range, so a 64-BYTE key holding
    /// a multi-byte UTF-8 character passed the length check and then panicked on a
    /// non-char-boundary slice index ("byte index 2 is not a char boundary"). This is the exact
    /// repro from that review: U+20AC EURO SIGN (3 bytes, 1 character) + 61 ASCII characters is
    /// 64 BYTES but only 62 CHARACTERS. Before the fix this test aborts the process instead of
    /// failing an assertion; after it, the input is a plain typed error.
    #[test]
    fn hex_decode_32_rejects_a_64_byte_multi_byte_utf8_key_instead_of_panicking() {
        let key = format!("\u{20ac}{}", "a".repeat(61));
        assert_eq!(key.len(), 64, "the repro must be exactly 64 BYTES to reach the old byte-slice path");
        assert_eq!(key.chars().count(), 62, "...while being only 62 CHARACTERS");

        match hex_decode_32(&key) {
            Err(ChannelError::InvalidPeerKey { .. }) => {}
            Err(other) => panic!("expected InvalidPeerKey, got a different ChannelError: {other}"),
            Ok(_) => panic!("a 62-character key must never decode to 32 bytes"),
        }
    }

    /// The other half of the same bug class: a key that IS 64 characters but carries a
    /// non-ASCII character. Character-counting alone would let this through to the decode, so
    /// hex-alphabet membership has to be checked too.
    #[test]
    fn hex_decode_32_rejects_a_64_character_key_containing_a_non_hex_character() {
        for key in [
            format!("{}\u{20ac}", "a".repeat(63)),  // multi-byte, in-bounds by character count
            format!("{}z", "a".repeat(63)),          // plain ASCII, outside the hex alphabet
            format!("{} ", "a".repeat(63)),          // a stray space from a sloppy paste
        ] {
            assert_eq!(key.chars().count(), 64, "each case must clear the character-count gate");
            match hex_decode_32(&key) {
                Err(ChannelError::InvalidPeerKey { .. }) => {}
                Err(other) => panic!("expected InvalidPeerKey for {key:?}, got: {other}"),
                Ok(_) => panic!("a non-hex character must be rejected: {key:?}"),
            }
        }
    }

    #[test]
    fn hex_decode_32_rejects_keys_of_the_wrong_length_including_empty() {
        for key in ["", "abcd", &"a".repeat(63), &"a".repeat(65), &"a".repeat(128)] {
            match hex_decode_32(key) {
                Err(ChannelError::InvalidPeerKey { .. }) => {}
                Err(other) => panic!("expected InvalidPeerKey for a {}-char key, got: {other}", key.chars().count()),
                Ok(_) => panic!("a {}-character key must be rejected", key.chars().count()),
            }
        }
    }

    /// The added validation must not be over-restrictive: the real, well-formed keys this app
    /// actually produces (`ChannelIdentity::public_key_hex`, i.e. `hex_encode_32`) still decode,
    /// byte-for-byte, and an uppercase-hex key -- which the previous `u8::from_str_radix`
    /// implementation also accepted -- keeps working rather than silently becoming invalid.
    #[test]
    fn hex_decode_32_still_accepts_the_real_keys_this_app_produces() {
        let bytes: [u8; 32] = core::array::from_fn(|i| (i as u8).wrapping_mul(7).wrapping_add(3));
        let encoded = hex_encode_32(&bytes);
        assert_eq!(encoded.chars().count(), 64);
        assert_eq!(hex_decode_32(&encoded).expect("a real 64-char lowercase-hex key decodes"), bytes);
        assert_eq!(hex_decode_32(&encoded.to_uppercase()).expect("uppercase hex still decodes"), bytes);
    }
}
