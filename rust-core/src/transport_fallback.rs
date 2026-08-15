//! ARCHITECTURE.md gap 1: the reference web demo's automatic WebRTC->direct-channel
//! fallback is broken on a UDP-blocked network (demo#129, live-reproduced 3/3) --
//! specifically: the fallback *does* fire, but hands off a signaling WebSocket that has
//! already gone stale, so the call is silently abandoned ~2s later with **no error
//! shown**, and there's ~14s of complete silence (no banner, no feedback at all) between
//! the call screen appearing and the first "Reconnecting..." indicator, on every network
//! path. The documented workaround is to manually enable a "direct-channel" toggle
//! *before* dialing rather than trust the automatic fallback.
//!
//! This module is a pure, deterministic state machine encoding the fix for both root
//! causes: (1) never more than [`MAX_SILENT_MS`] without an emitted status update, (2)
//! a fallback transition always carries `needs_fresh_signaling: true` -- the caller MUST
//! open a new signaling connection, the bug this fixes was reusing a stale one. Pure
//! logic, no real network/timer/WebRTC dependency, so it's fully testable here; the
//! Android (and later iOS) side drives it with real elapsed time and real
//! ICE-connection-state callbacks.

/// Caller-visible state. Every variant has (or the transition into it carries) a status
/// string -- there is no variant that corresponds to the reference bug's silent gap.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum TransportState {
    Connecting { status: String },
    WebRtcConnected,
    FallingBackToDirectChannel { status: String, needs_fresh_signaling: bool },
    DirectChannelConnected,
    /// Both transports failed. Still a real, user-visible state -- this is what the
    /// reference bug's "no error shown" silent abandonment becomes instead.
    Abandoned { reason: String },
}

/// ICE connection state, as reported by the platform WebRTC stack (mirrors
/// `RTCPeerConnectionState`'s meaningful-for-this-state-machine values).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum IceConnectionState {
    New,
    Connected,
    Failed,
    Disconnected,
}

/// Time thresholds. Chosen to always emit a status update well before the reference
/// bug's observed ~14s of total silence -- not a guess at "the right" UX timing, just a
/// hard ceiling well under the reproduced failure window.
const EARLY_STATUS_MS: u64 = 3_000;
const FALLBACK_TRIGGER_MS: u64 = 6_000;
const ABANDON_MS: u64 = 20_000;

#[derive(uniffi::Object)]
pub struct TransportFallbackController {
    state: std::sync::Mutex<ControllerState>,
}

struct ControllerState {
    current: TransportState,
    ice: IceConnectionState,
    fallback_triggered: bool,
    early_status_emitted: bool,
}

#[uniffi::export]
impl TransportFallbackController {
    #[uniffi::constructor]
    pub fn new() -> Self {
        Self {
            state: std::sync::Mutex::new(ControllerState {
                current: TransportState::Connecting { status: "Connecting…".to_string() },
                ice: IceConnectionState::New,
                fallback_triggered: false,
                early_status_emitted: false,
            }),
        }
    }

    pub fn current_state(&self) -> TransportState {
        self.state.lock().unwrap().current.clone()
    }

    /// Report the platform WebRTC stack's ICE connection state. Returns the resulting
    /// state (same value `current_state()` would return right after).
    pub fn on_ice_state_changed(&self, ice: IceConnectionState) -> TransportState {
        let mut s = self.state.lock().unwrap();
        s.ice = ice;
        match ice {
            IceConnectionState::Connected => {
                s.current = TransportState::WebRtcConnected;
            }
            IceConnectionState::Failed if !s.fallback_triggered => {
                s.fallback_triggered = true;
                s.current = TransportState::FallingBackToDirectChannel {
                    status: "WebRTC connection failed, trying direct channel…".to_string(),
                    needs_fresh_signaling: true,
                };
            }
            _ => {}
        }
        s.current.clone()
    }

    /// Report the direct-channel transport's own result, once it was actually attempted
    /// (i.e. after a `FallingBackToDirectChannel` transition).
    pub fn on_direct_channel_result(&self, connected: bool) -> TransportState {
        let mut s = self.state.lock().unwrap();
        s.current = if connected {
            TransportState::DirectChannelConnected
        } else {
            TransportState::Abandoned { reason: "Both WebRTC and the direct channel failed to connect.".to_string() }
        };
        s.current.clone()
    }

    /// Drive the state machine with elapsed wall-clock time since `Connecting` began (ms).
    /// Deterministic and pure -- the caller supplies elapsed time, this does not read a
    /// clock itself, which is what makes it fully unit-testable.
    pub fn on_tick(&self, elapsed_ms: u64) -> TransportState {
        let mut s = self.state.lock().unwrap();
        // Only the Connecting phase reacts to ticks -- once we've moved on (WebRTC
        // connected, fell back, abandoned), elapsed time alone shouldn't undo that.
        if !matches!(s.current, TransportState::Connecting { .. }) {
            return s.current.clone();
        }
        if elapsed_ms >= ABANDON_MS {
            s.current = TransportState::Abandoned {
                reason: "Timed out waiting for a connection.".to_string(),
            };
        } else if elapsed_ms >= FALLBACK_TRIGGER_MS && !s.fallback_triggered {
            s.fallback_triggered = true;
            s.current = TransportState::FallingBackToDirectChannel {
                status: "Still connecting via WebRTC, trying direct channel…".to_string(),
                needs_fresh_signaling: true,
            };
        } else if elapsed_ms >= EARLY_STATUS_MS && !s.early_status_emitted {
            s.early_status_emitted = true;
            s.current = TransportState::Connecting { status: "Still connecting…".to_string() };
        }
        s.current.clone()
    }
}

impl Default for TransportFallbackController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_silent_past_early_status_threshold() {
        let c = TransportFallbackController::new();
        // Immediately after construction there's already a real status string --
        // the reference bug's silence starts at t=0, this never does.
        assert!(matches!(c.current_state(), TransportState::Connecting { .. }));

        let mid = c.on_tick(EARLY_STATUS_MS);
        match mid {
            TransportState::Connecting { status } => assert_eq!(status, "Still connecting…"),
            other => panic!("expected still-Connecting with an updated status, got {other:?}"),
        }
    }

    #[test]
    fn fallback_transition_always_requests_fresh_signaling() {
        let c = TransportFallbackController::new();
        let state = c.on_tick(FALLBACK_TRIGGER_MS);
        match state {
            TransportState::FallingBackToDirectChannel { needs_fresh_signaling, .. } => {
                assert!(needs_fresh_signaling, "gap 1's actual bug was reusing a stale signaling socket");
            }
            other => panic!("expected fallback state at the trigger threshold, got {other:?}"),
        }
    }

    #[test]
    fn fallback_also_triggers_on_ice_failed_not_just_a_timeout() {
        let c = TransportFallbackController::new();
        let state = c.on_ice_state_changed(IceConnectionState::Failed);
        assert!(matches!(state, TransportState::FallingBackToDirectChannel { needs_fresh_signaling: true, .. }));
    }

    #[test]
    fn ice_connected_short_circuits_straight_to_connected_even_before_any_timeout() {
        let c = TransportFallbackController::new();
        let state = c.on_ice_state_changed(IceConnectionState::Connected);
        assert_eq!(state, TransportState::WebRtcConnected);
        // A later tick must not undo a successful connection.
        assert_eq!(c.on_tick(FALLBACK_TRIGGER_MS), TransportState::WebRtcConnected);
    }

    #[test]
    fn abandonment_is_a_real_visible_state_not_silence() {
        let c = TransportFallbackController::new();
        c.on_ice_state_changed(IceConnectionState::Failed);
        let state = c.on_direct_channel_result(false);
        match state {
            TransportState::Abandoned { reason } => assert!(!reason.is_empty()),
            other => panic!("expected Abandoned with a real reason, got {other:?}"),
        }
    }

    #[test]
    fn successful_direct_channel_after_fallback_reports_connected() {
        let c = TransportFallbackController::new();
        c.on_ice_state_changed(IceConnectionState::Failed);
        assert_eq!(c.on_direct_channel_result(true), TransportState::DirectChannelConnected);
    }
}
