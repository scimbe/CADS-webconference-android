//! Channel wire format for text messages (#382, `devsystem.android_native_bridge` --
//! backlog item after `generate_noise_public_key_hex`: "Channel-Wire-Format fuer
//! Textnachrichten festlegen (msg_id, sender_pubkey, timestamp, body) und in der
//! native-bridge UniFFI-API als send_text/recv_text exponieren").
//!
//! This lands the wire format itself plus real, tested encode/decode across the
//! UniFFI boundary -- **not** `send_text`/`recv_text` against a live channel.
//! There is no channel-join and no Noise_IK handshake state machine wired into
//! this crate yet (`ct_common::noise::client_handshake`/`origin_handshake` are
//! unused here so far); actually putting bytes on the wire needs both, and both
//! are later, separate increments. What this module proves is: given a message,
//! the two peers agree on exactly how it is represented as bytes, and a
//! corrupted/truncated buffer is rejected with a typed error instead of a panic
//! -- the two properties `send_text`/`recv_text` will need underneath them.
//!
//! ## Encoding: JSON via `serde_json`
//!
//! This ecosystem's own wire types consistently use `serde` + `serde_json`, not a
//! compact binary format: `ct_common::channel` (`ChannelId`, `AgentCard`,
//! `CapacityOffer`, `UsageReceipt`, ...), `ct_common::cookbook`, `ct_common::crew`
//! and `ct_common::mcp`'s JSON-RPC framing all round-trip through
//! `serde_json::to_string`/`from_str` (or `to_vec`/`from_slice`). Matching that
//! precedent -- rather than introducing `bincode`/`postcard`, which nothing else
//! in this ecosystem uses -- keeps one on-the-wire convention for anyone reading
//! Rust across these repos, and JSON is trivially inspectable while this project
//! is still proving out its wire format. If a later increment needs the extra
//! compactness of a binary encoding (e.g. once media attachments make payload
//! size matter), that is a deliberate follow-up, not something to preempt here.
//!
//! ## `msg_id`: UUIDv4
//!
//! Chosen over a ULID because nothing here needs `msg_id` to be lexicographically
//! sortable by creation time -- `timestamp` is already a separate, explicit field
//! for that -- and UUIDv4 (via the `uuid` crate, `v4` feature: 122 bits of CSPRNG
//! randomness) is the simplest, most-widely-supported choice for "a globally
//! unique id with no coordination", with a first-class `java.util.UUID` type on
//! the Kotlin side if a later increment wants to parse it there instead of
//! treating it as an opaque string.
//!
//! ## `sender_pubkey`: lowercase hex
//!
//! Matches `generate_noise_public_key_hex`'s existing convention exactly (the
//! same lowercase-hex profile `ct_common::channel`'s own `card_hex` module uses
//! for its fixed-size byte fields) so a `TextMessage.sender_pubkey` and a
//! `generate_noise_public_key_hex()` return value are directly comparable
//! strings, with no re-encoding needed to check "did this message come from the
//! peer I pinned".

use uuid::Uuid;

/// A single text message on an Agent-Fabric channel (#382 wire format).
///
/// Plain text only for this increment -- media (images, voice notes, files) is
/// a real, separate wire-format extension for a later backlog item, not
/// something to half-retrofit into `body: String` now (e.g. by smuggling a
/// content-type/blob-reference into the same field). When that lands it should
/// most likely be a new variant/field, not a change to this struct's meaning.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, uniffi::Record)]
pub struct TextMessage {
    /// A fresh UUIDv4 (see module docs), identifying this message so a
    /// duplicate delivery (retried send, at-least-once channel relay) can be
    /// deduplicated by the receiver.
    pub msg_id: String,
    /// The sender's Noise_IK static public key, lowercase hex -- same format
    /// `generate_noise_public_key_hex()` returns.
    pub sender_pubkey: String,
    /// Unix time in milliseconds, set by the sender at compose time.
    pub timestamp: u64,
    /// Plain-text message body.
    pub body: String,
}

/// Errors decoding a [`TextMessage`] off the wire. A real, typed error crossing
/// the UniFFI boundary -- decoding attacker-controlled or truncated bytes must
/// never panic across FFI (an unwinding panic across an `extern "C"` boundary is
/// undefined behavior), so every failure path here is a `Result::Err`, not an
/// `unwrap`/`expect`/indexing panic.
#[derive(Debug, uniffi::Error)]
pub enum MessageDecodeError {
    /// `bytes` was not valid UTF-8, so it cannot be JSON at all.
    InvalidUtf8 { reason: String },
    /// `bytes` was valid UTF-8 but not a well-formed `TextMessage` -- truncated,
    /// missing/extra fields, or a wrong JSON type for a field.
    Malformed { reason: String },
}

impl std::fmt::Display for MessageDecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MessageDecodeError::InvalidUtf8 { reason } => {
                write!(f, "TextMessage bytes are not valid UTF-8: {reason}")
            }
            MessageDecodeError::Malformed { reason } => {
                write!(f, "TextMessage bytes are not a well-formed message: {reason}")
            }
        }
    }
}

impl std::error::Error for MessageDecodeError {}

/// Builds a fresh, well-formed [`TextMessage`]: generates `msg_id` (UUIDv4) and
/// stamps `timestamp` as the current Unix time in milliseconds, so Kotlin never
/// has to construct those two fields by hand (and can't accidentally reuse a
/// `msg_id` or get the clock source wrong).
///
/// `system_time_millis_since_epoch` panics only if the host clock is set before
/// the Unix epoch, which would indicate a broken host environment, not a
/// reachable-from-Kotlin input-validation failure -- this is not a decode path.
#[uniffi::export]
pub fn new_text_message(sender_pubkey: String, body: String) -> TextMessage {
    TextMessage {
        msg_id: Uuid::new_v4().to_string(),
        sender_pubkey,
        timestamp: system_time_millis_since_epoch(),
        body,
    }
}

fn system_time_millis_since_epoch() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is set before the Unix epoch")
        .as_millis() as u64
}

/// Encodes a [`TextMessage`] as its wire form (UTF-8 JSON bytes). Pure and
/// infallible -- every `TextMessage` is representable, so there is no `Result`
/// on this side, only on [`decode_text_message`].
#[uniffi::export]
pub fn encode_text_message(message: TextMessage) -> Vec<u8> {
    // `TextMessage` has no field type that can fail to serialize (no maps with
    // non-string keys, no NaN/Infinity floats, ...), so this can't actually
    // fail; `unwrap_or_default` documents "well-formed struct -> always
    // succeeds" without smuggling a panic-shaped `.unwrap()` into a function
    // whose whole job is to be panic-free at the FFI boundary.
    serde_json::to_vec(&message).unwrap_or_default()
}

/// Decodes a [`TextMessage`] from its wire form. Never panics: malformed input
/// (not UTF-8, not valid JSON, wrong shape, truncated) returns a real
/// [`MessageDecodeError`] instead.
#[uniffi::export]
pub fn decode_text_message(bytes: Vec<u8>) -> Result<TextMessage, MessageDecodeError> {
    // Checked separately (rather than relying on serde_json's own UTF-8
    // handling) so the two real failure modes -- "not text at all" vs. "text
    // but not a well-formed message" -- are distinguishable to a caller, which
    // matters for diagnosing e.g. a truncated multi-byte UTF-8 sequence at a
    // buffer boundary vs. a truncated JSON object.
    if let Err(e) = std::str::from_utf8(&bytes) {
        return Err(MessageDecodeError::InvalidUtf8 { reason: e.to_string() });
    }
    serde_json::from_slice(&bytes).map_err(|e| MessageDecodeError::Malformed { reason: e.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> TextMessage {
        TextMessage {
            msg_id: "b7f3c2a0-4e9d-4b8a-9c1e-6f2a1d5e8b3c".to_string(),
            sender_pubkey: "7a3f9c1e2b4d6a8f0e1c3b5d7a9f0e2c4b6a8d0f2e4c6a8b0d2f4e6a8c0b2d4e".to_string(),
            timestamp: 1_754_290_123_456,
            body: "hello from the native bridge".to_string(),
        }
    }

    /// Round-trip: encode -> decode preserves every field exactly.
    #[test]
    fn round_trip_preserves_every_field() {
        let original = sample();
        let bytes = encode_text_message(original.clone());
        let decoded = decode_text_message(bytes).expect("well-formed message decodes");
        assert_eq!(decoded, original);
    }

    /// Round-trip with a body containing characters that stress JSON escaping
    /// and multi-byte UTF-8, to catch an encoder that only happens to work on
    /// plain ASCII.
    #[test]
    fn round_trip_preserves_unicode_and_escapes_in_body() {
        let mut original = sample();
        original.body = "quote \" backslash \\ newline \n emoji \u{1F980} \u{00e9}".to_string();
        let bytes = encode_text_message(original.clone());
        let decoded = decode_text_message(bytes).expect("well-formed message decodes");
        assert_eq!(decoded, original);
    }

    /// Round-trip with an empty body and a zero timestamp -- boundary values,
    /// not just a "happy path" value for every field.
    #[test]
    fn round_trip_preserves_boundary_values() {
        let mut original = sample();
        original.body = String::new();
        original.timestamp = 0;
        let bytes = encode_text_message(original.clone());
        let decoded = decode_text_message(bytes).expect("well-formed message decodes");
        assert_eq!(decoded, original);
    }

    /// Decoding non-UTF-8 bytes returns the real typed error, not a panic.
    #[test]
    fn decode_rejects_invalid_utf8_without_panicking() {
        let garbage: Vec<u8> = vec![0xff, 0xfe, 0xfd, 0x00, 0x01];
        let err = decode_text_message(garbage).expect_err("invalid UTF-8 must not decode");
        assert!(matches!(err, MessageDecodeError::InvalidUtf8 { .. }));
    }

    /// Decoding well-formed UTF-8 that is not valid JSON at all returns the
    /// real typed error, not a panic.
    #[test]
    fn decode_rejects_non_json_text_without_panicking() {
        let not_json = b"this is not json".to_vec();
        let err = decode_text_message(not_json).expect_err("non-JSON text must not decode");
        assert!(matches!(err, MessageDecodeError::Malformed { .. }));
    }

    /// Decoding a truncated version of a real encoded message -- cut off
    /// mid-object, the shape an at-least-once/lossy channel transport could
    /// actually deliver -- returns the real typed error, not a panic.
    #[test]
    fn decode_rejects_truncated_input_without_panicking() {
        let bytes = encode_text_message(sample());
        let truncated = bytes[..bytes.len() / 2].to_vec();
        let err = decode_text_message(truncated).expect_err("truncated JSON must not decode");
        assert!(matches!(err, MessageDecodeError::Malformed { .. }));
    }

    /// Decoding well-formed JSON that is missing a required field returns the
    /// real typed error, not a panic.
    #[test]
    fn decode_rejects_missing_field_without_panicking() {
        let missing_body = br#"{"msg_id":"x","sender_pubkey":"ab","timestamp":1}"#.to_vec();
        let err = decode_text_message(missing_body).expect_err("missing field must not decode");
        assert!(matches!(err, MessageDecodeError::Malformed { .. }));
    }

    /// A fixed, hand-written test vector decodes to the exact expected struct.
    /// This is the one test in this module that does NOT derive its expected
    /// bytes from `encode_text_message` -- it is a literal byte string, so a
    /// future change that makes encode/decode internally consistent with each
    /// other but silently incompatible with this exact wire shape (e.g.
    /// renaming a JSON field, changing `timestamp`'s JSON type) is caught here,
    /// not just by the round-trip tests above.
    #[test]
    fn decode_matches_a_fixed_hand_written_test_vector() {
        let wire: &[u8] = br#"{"msg_id":"b7f3c2a0-4e9d-4b8a-9c1e-6f2a1d5e8b3c","sender_pubkey":"7a3f9c1e2b4d6a8f0e1c3b5d7a9f0e2c4b6a8d0f2e4c6a8b0d2f4e6a8c0b2d4e","timestamp":1754290123456,"body":"hello from the native bridge"}"#;
        let decoded = decode_text_message(wire.to_vec()).expect("fixed test vector decodes");
        assert_eq!(decoded, sample());
    }

    /// `new_text_message` fills in a real UUIDv4 `msg_id` (not e.g. an empty
    /// string or a fixed placeholder) and a plausible, non-zero millisecond
    /// Unix timestamp, and two calls never collide on `msg_id`.
    #[test]
    fn new_text_message_generates_a_real_uuid_and_timestamp() {
        let a = new_text_message("ab".to_string(), "hi".to_string());
        let b = new_text_message("ab".to_string(), "hi".to_string());
        assert_ne!(a.msg_id, b.msg_id, "two messages must not share a msg_id");
        assert!(Uuid::parse_str(&a.msg_id).is_ok(), "msg_id must be a real UUID: {}", a.msg_id);
        // Sanity bound: some time after this module was written, in
        // milliseconds since the Unix epoch (year ~2025 in millis).
        assert!(a.timestamp > 1_700_000_000_000, "timestamp must be real Unix millis, got {}", a.timestamp);
    }
}
