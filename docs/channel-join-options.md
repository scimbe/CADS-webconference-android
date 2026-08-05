# Channel-join options for `native-bridge` (#382, `devsystem.android_native_bridge`)

This documents what a real search of the CADS-Tunnel ecosystem's source found about where
"channel join" actually lives, the options it opened up, and which one this increment took.
Written alongside the increment that acted on it (`native-bridge/src/channel.rs`), not as a
standalone research memo -- read it together with that module's own doc comments.

## Where the real channel-join logic lives

The `ct-agent` binary is **not** in the CADS-Tunnel monorepo at all -- it moved to its own
repo, [`scimbe/ct-agent`](https://github.com/scimbe/ct-agent), pinned by CADS-Tunnel's own
`crates/client/Cargo.toml` and `crates/agent-tools/Cargo.toml` at a specific git rev
(`86ab198ffc70d6dcb9ee1bb55efa2191dbd8d408` as of this writing). That repo has a full local
checkout at `/home/becke/workspace/ct-agent`, at exactly that pinned rev. Its structure:

- `native/src/lib.rs` -- a real library crate (`pub mod channel; pub mod channel_run; ...`),
  not something private to a binary.
- `native/src/channel.rs` (893 lines) -- the agent-side **broker admission client**: presents a
  `SignedChannelGrant` (`ct_common::channel::ChannelJoinRequest`) to the edge's
  `ct_edge::channel_broker` over a QUIC bi-stream (or a generic duplex -- see
  `present_channel_join_on_stream`, explicitly transport-agnostic, and
  `present_channel_relay_join_on_stream` for the `:443` TLS-TCP relay-leg variant), proves
  possession of the grant's holder key over a challenge/response, and learns the paired peer's
  advertised endpoint (+ Noise key, holder key, attestation, and -- in the AutoNAT phase --
  its OWN edge-observed reflexive address). Genuinely reusable: generic over
  `AsyncRead + AsyncWrite`, hermetically tested against a real in-process edge broker.
- `native/src/channel_run.rs` (7,584 lines) -- the actual `ct-agent channel run` CLI command's
  machinery: NAT traversal / hole-punching orchestration, the relay-to-direct cutover, reconnect
  state, superpeer election, and all of `ct-agent`'s own process/config/logging model wired
  around the channel session. This is where the CLI's real weight is -- not cleanly separable
  from the CLI's own process model without a substantial extraction effort.
- `ct_common::a2a` (`CADS-Tunnel/crates/common/src/a2a.rs`, part of the `ct-common` crate
  `native-bridge` **already** depends on at the exact pinned tag `v0.4.13` that has this module
  -- confirmed via `git show v0.4.13:crates/common/src/a2a.rs`) -- the actual `Noise_IK`
  handshake (`a2a_initiate`/`a2a_respond`) and encrypted-message framing
  (`a2a_send`/`a2a_recv`), generic over any duplex stream. This is the payoff primitive
  `ct-agent`'s own `channel.rs` test (`two_agents_carry_data_over_a_channel_session`) uses once
  ITS broker rendezvous has told each side the other's endpoint -- it does not depend on the
  broker at all.

## The real options this opened up

1. **Full broker-mediated channel-join**: pull in `ct-agent`'s `channel.rs` (a real library
   dependency -- `git = "https://github.com/scimbe/ct-agent"`, same as CADS-Tunnel's own
   `crates/client`/`crates/agent-tools` already do) plus enough of `channel_run.rs`'s orchestration
   to get a genuinely-authorized, rendezvous-paired, NAT-traversing channel session. This is the
   "real thing" -- grant-based membership, no manually-shared addresses, works across the open
   internet -- but it also pulls in `ct_edge` as a *client-side* dependency for the broker
   protocol's client half is fine (it's already like that in `channel.rs`'s own tests, which
   import `ct_edge::channel_broker`/`ct_edge::transport` only as **dev-dependencies** to spin up
   a test edge -- the actual client-side code in `channel.rs` itself needs no `ct_edge` at
   runtime). The real cost is `channel_run.rs`'s 7.5k lines of CLI-process-shaped orchestration:
   extracting just the reusable core (open a session once we know the peer's endpoint + key) from
   the parts that assume `ct-agent`'s own long-running-process/reconnect/superpeer model is a
   substantial, separate scoping effort, not something to force into this increment.
2. **Direct peer-to-peer session, no broker** (**the path this increment took**): skip the
   broker/rendezvous step entirely. Given a peer's Noise static public key and a reachable
   `host:port` (worked out by the caller, out of band), drive `ct_common::a2a` directly over a
   plain TCP socket. Zero new CADS-Tunnel-side code, zero dependency on `ct_edge` or `ct-agent`
   at all -- purely additive inside `native-bridge`. This is real: a genuine mutual `Noise_IK`
   handshake (AEAD-authenticated, so a wrong pinned key fails outright) and real encrypted
   `TextMessage`s over the resulting session -- see `native-bridge/src/channel.rs` and its
   integration test. What it does NOT give: broker-authorized channel membership, rendezvous (so
   today each side must already know how to reach the other), NAT traversal/hole-punching, or the
   `:443` relay fallback.

## Why (2), not (1), for this increment

Option (1) is the real, complete answer for "two Android devices exchange text over a channel"
across the open internet, and should be the next real increment once this one has proven the
handshake+data-path primitive works end to end from Kotlin. But it is genuinely large: beyond
the grant/possession-proof wiring, real device-to-device use needs the NAT
traversal/hole-punching and relay-fallback machinery that lives in `channel_run.rs`'s
CLI-process-shaped code, which is not yet factored into something a mobile `.so` can call
directly without either (a) depending on a large slice of `ct-agent`'s own binary-oriented
internals, or (b) a real extraction effort inside `ct-agent` itself (out of scope here -- and
out of scope for CADS-Tunnel too, per this task's constraints -- `ct-agent` is a separate repo
this task does not touch either).

Option (2), by contrast, is a real, bounded, honestly-scopable slice available TODAY with the
dependencies `native-bridge` already has: it proves the actual cryptographic/data-path
primitive (the same `ct_common::a2a` the broker-mediated path would ALSO end up using once
paired) end to end, hermetically tested, cross-compiled, and callable from Kotlin -- without
speculatively reimplementing any of `channel_run.rs`'s orchestration. That is what
`native-bridge/src/channel.rs` implements.

## What's still missing toward "two Android devices exchange text over a channel"

- Broker-authorized channel membership (`SignedChannelGrant`, `ct_edge::channel_broker`).
- Rendezvous, so neither side needs to already know the other's reachable address.
- NAT traversal / hole-punching for two devices behind ordinary home/mobile NATs.
- The `:443` TLS-TCP relay fallback for networks that block the channel's ports outright.
- Real device-to-device testing (this increment's integration test proves two LOCAL PROCESSES
  complete a real handshake and exchange a real message on one machine -- not two real Android
  devices over a real network).
